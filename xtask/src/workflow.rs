//! CI runs the gate, and only the gate.
//!
//! `NOCTUA.md §19.4`: **the command surface does not grow.** The way that stops
//! being true is not a decision anybody makes — it is a CI file that learns, one
//! green pull request at a time, to run `cargo fmt --check` and then
//! `cargo clippy` and then `cargo test`, because someone wanted a faster failure
//! on a bad day. After that the gate a developer runs and the gate CI runs are
//! two different gates, and the one that decides is whichever happens to be
//! stricter that week.
//!
//! So the rule is mechanical: **every `cargo` invocation in the workflow is
//! either `cargo xtask <verb>` or `cargo install`.** The second is a
//! prerequisite — `cargo-deny` is a binary the gate shells out to — and it
//! produces no verdict of its own.
//!
//! This is not a YAML parser. It reads what a step *runs*, which is what the
//! rule is about, and a workflow written in a shape it cannot read is reported
//! rather than passed.
//!
//! # Every file, not one file
//!
//! A gate that reads `.github/workflows/ci.yml` and nothing else lets four
//! ordinary shapes put a second cargo command into CI with `check passed` and
//! the count unchanged:
//!
//! * a second workflow file — `nightly.yml` running `cargo test`, `cargo fmt` <!-- allow-reference: names `nightly.yml` — a probe file, written and removed -->
//!   and `cargo clippy`, which is a whole second gate;
//! * a **composite action** under `.github/actions/`, invoked from `ci.yml` by <!-- allow-reference: names `.github/actions/` — a probe directory, written and removed -->
//!   `uses: ./.github/actions/extra`, whose `run:` such a gate never opens;
//! * shell indirection *inside `ci.yml` itself* — `C=cargo` and then
//!   `$C clippy`, which never spells `cargo `;
//! * a **tab** instead of a space: `cargo\ttest`, which `find("cargo ")` misses.
//!
//! So the walk is over all of `.github/`, the match is on the word `cargo`
//! however it is spelled, and an assignment whose value is `cargo` is itself a
//! finding — because the rule cannot survive a gate that only reads one
//! spelling in one file.

use std::path::Path;

use crate::gates::{Finding, walk};
use crate::relative_to;

/// Where CI's configuration lives. All of it.
pub(crate) const WORKFLOWS: &str = ".github";

/// Every name `just` will load a justfile from.
///
/// It searches all four, and a gate that reads one makes the whole surface a
/// developer types invisible with a `git mv` on a case-sensitive filesystem.
/// **And without regard to case**, which [`the_files_answering_to`] says
/// more about: `just` loads `JuStFiLe` as readily as `justfile`.
const JUSTFILE: [&str; 4] = ["justfile", ".justfile", "JUSTFILE", "Justfile"];

/// Every name GNU make looks for, in the order it looks for them.
///
/// **`GNUmakefile` first, which is the one make prefers.** With the list at
/// `["Makefile"]`, renaming the file to the name make reads *before*
/// `Makefile` takes a whole second gate out of the walk with the count
/// unchanged — and `makefile` in lower case is the third name make searches.
const MAKEFILE: [&str; 3] = ["GNUmakefile", "makefile", "Makefile"];

/// The file the repository's own gate is written in.
pub(crate) const WORKFLOW: &str = ".github/workflows/ci.yml";

/// What each job must actually run, beside its name.
///
/// **A name is not a job.** `JOBS` alone checks that a line `deterministic:`
/// exists and nothing about the body — so the step can be replaced with
/// `run: echo the determinism check used to be here` and the gate calls it
/// `ok`. That job is the only cross-process determinism check in the
/// repository, criterion 4's sole enforcement, and the only thing that can see
/// an ambient rustfmt configuration or a locale. And a system job's own
/// command can become `cargo xtask docs` — every cargo invocation still being
/// `cargo xtask <verb>` — while nothing runs the gate on that system at all.
///
/// A name checked without its body is the defect a bare count of invocations
/// has, moved one level in: from the job's name to the job's body.
///
/// The text is matched against what a step **runs**, not against the job's
/// prose: `- name: cargo xtask check` is a label, and a job renamed after the
/// command it no longer runs would otherwise satisfy this.
///
/// **`diff -r` alone is satisfiable by a tautology.** Measured: replacing the
/// job's whole body with `mkdir -p /tmp/first/docs` and
/// `diff -r /tmp/first/docs /tmp/first/docs` leaves the job name present, the
/// needle present, the invocation count down from four to two — which a zero
/// floor does not see — and `check passed`. The one cross-process determinism
/// check there is becomes a directory diffed against itself. So the job has to
/// build twice and diff each generated tree against the one it saved, and each
/// requirement is its own needle rather than one that a substring can satisfy.
///
/// **And how many times, because `any()` cannot count to two.** An entry
/// satisfied by one occurrence enforces the requirement in the paragraph above,
/// "the job has to build twice", as "the job builds". Measured: the body
/// reduced to one `cargo xtask build`, then `cp -r`, then `diff -r` — a copy
/// diffed against its own source, in one process, with no second generation
/// anywhere — prints
///
/// ```text
/// ok    3 cargo command(s) in the workflow, …
/// check passed
/// ```
///
/// which is the same "diff against itself" defect the paragraph above names,
/// reached by deleting one line.
const JOBS_MUST_RUN: &[(&str, &str, usize)] = &[
    // Without the `run:` prefix: what is compared is the *command*, after
    // the key that introduces it and after any `&&` before it. One gate on each
    // of the three systems (`NOCTUA.md §22.2`).
    ("linux", "cargo xtask check", 1),
    ("macos", "cargo xtask check", 1),
    ("windows", "cargo xtask check", 1),
    // Two builds, which is what makes the second generation a second *process*.
    ("deterministic", "cargo xtask build", 2),
    // **And the second build is in a second directory**, which is what makes it
    // a second *environment* and not only a second process. Two builds in this
    // checkout share the working directory, the filesystem and the order
    // `read_dir` returns — and `catalog/i18n/` is read with `read_dir`, with
    // locale order deciding `Locale::ALL`, every column of `MESSAGES` and four
    // `docs/` pages. A second run in the same directory agrees with the first
    // about all of that whatever the emitter does with it, so a cross-process
    // check run in one directory is blind to the one input that actually
    // differs between machines.
    //
    // The copy is laid down in reverse order on a path of a different length,
    // so the two runs differ the way two developers' machines differ.
    ("deterministic", "cpio -pdm", 1),
    (
        "deterministic",
        "cd /tmp/a-second-checkout-of-noctua-central",
        1,
    ),
    (
        "deterministic",
        "diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-catalog/src",
        1,
    ),
    (
        "deterministic",
        // **The left operand only, here.** The right one is held by
        // `the_determinism_job_diffs_everything_the_emitter_writes`, which pairs
        // the operands of every `diff` line; writing it out again in this list
        // is one rule in two places, and the copy would be a literal that has to
        // track the workflow's line breaks.
        "diff -r /tmp/a-second-checkout-of-noctua-central/docs",
        1,
    ),
    // **And the two the job's own comment calls "a third of what
    // `cargo xtask build` emits"**: the generated log-event structs, and the
    // append-only ledger. Both are in `ci.yml`, and without these rows either
    // pair can be deleted from the job with no finding — and the ledger is the
    // one artifact whose whole purpose is that it never changes except by
    // appending.
    (
        "deterministic",
        "diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-log/src",
        1,
    ),
    (
        "deterministic",
        "diff /tmp/a-second-checkout-of-noctua-central/catalog/errors.reserved.toml",
        1,
    ),
];

/// The order the determinism job's commands have to happen in.
///
/// **Counting them is not enough either.** Two builds, a copy and a diff in any
/// order include *copy, diff, build, build* — which compares a tree to itself
/// before either build has run. What makes the second generation a second
/// process is the sequence: build, save what it wrote, build again, compare.
///
/// Prefixes of commands, matched against the job's body in order; each has to
/// appear after the one before it.
const IN_THIS_ORDER: [&str; 5] = [
    "cargo xtask build",
    "cpio -pdm",
    "cd /tmp/a-second-checkout-of-noctua-central",
    "cargo xtask build",
    "diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-catalog/src",
];

/// The actions CI may use that set something up and judge nothing.
///
/// An allow-list and not a needle, because an action that runs anything of
/// ours is a second gate whatever it is called.
///
/// **At module scope so a census can pin it.** Inside the reader no test can
/// see it, and a single negative fixture naming one action leaves any other
/// name free to be added with the suite green. Measured with
/// `actions-rs/clippy-check`, which is a real <!-- allow-reference: names `actions-rs/clippy-check` — a GitHub Action, not a path in this tree -->
/// and widely used clippy action: allowed here it runs a second verdict in CI,
/// which is what `NOCTUA.md §19.4` is about.
const SETS_UP_AND_JUDGES_NOTHING: [&str; 5] = [
    // Fetches the tree.
    "actions/checkout",
    // Installs the channel the workflow names, which `rust-toolchain.toml`'s own
    // header records is not the same as reading that file: this action resolves
    // `stable` itself and CI runs no rustup.
    "dtolnay/rust-toolchain",
    // The cache, which changes what a build costs and not what it decides.
    "Swatinem/rust-cache",
    "actions/cache",
    // Uploads what a failing run leaves behind.
    "actions/upload-artifact",
];

/// The directory CI copies the tree into for its second build.
///
/// Written once because `ci.yml` writes it six times — a `cpio`, a `cd` and four
/// `diff` lines — and two lists here have to agree with all of them.
#[cfg(test)]
const SECOND_CHECKOUT: &str = "a-second-checkout-of-noctua-central";

/// How many times the walk follows what CI reaches before it stops.
///
/// Bounded, because a script that names itself would otherwise not terminate —
/// and a chain deeper than this is a CI setup nobody can read anyway.
const HOPS: usize = 16;

/// The jobs CI must have, and what each is for.
///
/// **A count of invocations is not a claim that CI does anything.** Deleting the
/// whole `deterministic` job takes two from the number, and a floor that
/// compares it against zero does not see that — so without this list the
/// sole cross-process determinism check in the repository, and the only place
/// the ledger's determinism is checked at all, can be removed with `check passed`
/// printed over it.
const JOBS: [(&str, &str); 4] = [
    (
        "linux",
        "runs `cargo xtask check`, which is the whole gate (NOCTUA.md §19.4), on the \
         one system whose runner brings the stack up",
    ),
    (
        "macos",
        "runs `cargo xtask check` on macOS (NOCTUA.md §22.2), where the steps that need \
         the stack say they are not applicable and everything else is judged",
    ),
    (
        "windows",
        "runs `cargo xtask check` on Windows (NOCTUA.md §22.2), where the steps that need \
         the stack say they are not applicable and everything else is judged",
    ),
    (
        "deterministic",
        "builds twice **in two processes** and diffs every tree the build writes. \
         Not from cold: both builds share one job and one `rust-cache`, and \
         saying otherwise claimed a guarantee nobody had arranged. What the two \
         processes catch that `check` cannot is anything constant *within* a \
         process — an ambient rustfmt configuration, a locale, a path",
    ),
];

/// The jobs that run the gate, and the runner family each must run on.
///
/// **A job named for a system runs on it.** A `macos` job whose `runs-on:`
/// names an Ubuntu runner satisfies every other rule here — it runs the gate,
/// and nothing needs the stack — and proves nothing about macOS, so a defect
/// only that system has would pass CI with all three jobs green. The family
/// is read, not the version: `macos-latest` and `macos-15` are both macOS.
const RUNS_ON: [(&str, &str); 3] = [
    ("linux", "ubuntu-"),
    ("macos", "macos-"),
    ("windows", "windows-"),
];

/// The jobs that run the gate where no stack runs (`NOCTUA.md §22.2`).
///
/// Each runs `cargo xtask check` and nothing that brings the stack up: their
/// runners have no container stack, and the gate says which of its steps are
/// not applicable there.
const WITHOUT_THE_STACK: [&str; 2] = ["macos", "windows"];

/// The one command CI is allowed to reach the gate through.
const XTASK: &str = "xtask";

/// The other allowed shape: installing a binary the gate calls.
const INSTALL: &str = "install";

/// Whether a line opens a YAML key rather than continuing the value above it.
///
/// `run: cargo xtask check` and `RUSTFLAGS:` are keys; `--offline` and
/// `-D warnings` are what a key's value looks like when it wraps. A key is a
/// word of the characters YAML allows unquoted in one, followed by a colon and
/// then end-of-line or a space — which is what keeps `https://example.com` and
/// `cargo:rerun-if-changed=x` from reading as one.
fn opens_a_key(line: &str) -> bool {
    let Some((key, rest)) = line.split_once(':') else {
        return false;
    };
    !key.is_empty()
        && key.chars().all(|character| {
            character.is_alphanumeric() || matches!(character, '_' | '-' | '.' | '"' | '\'')
        })
        && (rest.is_empty() || rest.starts_with(' '))
}

/// One YAML double-quoted scalar's text, with its escapes resolved.
///
/// YAML 1.2 §5.7 gives the escape table, and the only reason this reader needs
/// it is that `\n` in a double-quoted scalar **is a line break** — so a step
/// written `run: "cargo xtask check\ncargo-clippy --workspace -- -D warnings"`
/// runs two commands, and a per-line reader sees one word it recognises as
/// neither. A second lints gate can stand there with the transcript unmoved.
///
/// **The obvious repair — split on `\n` — is wrong in both directions**, both
/// measured against `yq 3.4.3`: `"printf \"a\\ncargo-clippy …\""`
/// is ONE command, because `\\` is a literal backslash and the `n` after it is
/// an `n`; and the same attack spelled `\x0a` is two commands that splitting on
/// `\n` would miss. Resolving the escapes is what gets both right, so that is
/// what this does; an escape it does not know is kept verbatim rather than
/// guessed at, which cannot invent a break.
fn with_the_escapes_resolved(inner: &str) -> String {
    let letters: Vec<char> = inner.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < letters.len() {
        if letters[index] != '\\' {
            out.push(letters[index]);
            index += 1;
            continue;
        }
        let Some(next) = letters.get(index + 1).copied() else {
            out.push('\\');
            break;
        };
        index += 2;
        match next {
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            '0' => out.push('\0'),
            'N' => out.push('\u{85}'),
            'L' => out.push('\u{2028}'),
            'P' => out.push('\u{2029}'),
            '\\' | '"' | '/' | ' ' => out.push(next),
            'x' | 'u' | 'U' => {
                let width = match next {
                    'x' => 2,
                    'u' => 4,
                    _ => 8,
                };
                let digits: String = letters[index..].iter().take(width).collect();
                match u32::from_str_radix(&digits, 16)
                    .ok()
                    .and_then(char::from_u32)
                {
                    Some(letter) if digits.chars().count() == width => {
                        out.push(letter);
                        index += width;
                    }
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            }
            other => {
                out.push('\\');
                out.push(other);
            }
        }
    }
    out
}

/// The lines one physical line of a workflow actually holds.
///
/// One, unless it is a `key: "…"` whose double-quoted scalar carries a line
/// break of its own. The key is kept on the first, so every rule downstream
/// reads the step the way it reads any other.
fn the_lines_behind_the_escapes(line: &str) -> Vec<String> {
    let Some((key, value)) = line.split_once(':') else {
        return vec![line.to_owned()];
    };
    // **A scalar may wear an anchor, a tag, or both, and may carry a comment.**
    // Asking that the trimmed value both start and end with `"` leaves a step
    // written `run: "a\ncargo-clippy …"  # the gate` failing the suffix test and
    // never split — six characters of comment appended to a measured attack,
    // and the gate then has nothing at all to say. Allowing a trailing comment
    // alone is, measured against `yq 3.4.3`, too weak three more ways:
    // `!!str "a\nb"` and `&g "a\nb"` are both two-line scalars whose value does
    // not start with `"`, and `"a\nb"#note` — no space before the hash — is
    // still a comment.
    let mut rest = value.trim_start();
    loop {
        let property = rest
            .strip_prefix('&')
            .or_else(|| rest.strip_prefix('!'))
            .and_then(|tail| tail.split_once(' '));
        match property {
            Some((_, tail)) => rest = tail.trim_start(),
            None => break,
        }
    }
    let Some(opened) = rest.strip_prefix('"') else {
        return vec![line.to_owned()];
    };
    // The closing quote is the first unescaped one; what may follow it is
    // whitespace, a comment, or nothing. `yq` reads anything else as a parse
    // error, so a reader that refuses it refuses nothing a workflow can hold.
    let letters: Vec<char> = opened.chars().collect();
    let mut index = 0;
    while index < letters.len() {
        if letters[index] == '\\' {
            index += 2;
            continue;
        }
        if letters[index] == '"' {
            break;
        }
        index += 1;
    }
    if index >= letters.len() {
        return vec![line.to_owned()];
    }
    let inner: String = letters[..index].iter().collect();
    let after: String = letters[index + 1..].iter().collect();
    if !after.trim_start().is_empty() && !after.trim_start().starts_with('#') {
        return vec![line.to_owned()];
    }
    let resolved = with_the_escapes_resolved(&inner);
    let mut held: Vec<String> = resolved
        .split(['\n', '\r', '\u{85}', '\u{2028}', '\u{2029}'])
        .filter(|piece| !piece.trim().is_empty())
        .map(str::to_owned)
        .collect();
    if held.len() <= 1 {
        return vec![line.to_owned()];
    }
    held[0] = format!("{key}: {}", held[0]);
    held
}

/// The block-scalar style a header opens, if it opens one.
///
/// **A header is a grammar, not three string literals.** YAML 1.2
/// §8.1.1 gives `c-b-block-header(m,t)` as the style character, then an optional
/// indentation indicator and an optional chomping indicator *in either order*,
/// then `s-b-comment` — so `|`, `|-`, `|2`, `|2-`, `>-`, `>2+` and every one of
/// those followed by ` # anything` are all block headers. Matching the whole
/// rest of the line against `"|" | "|-" | "|+"` and the same three for `>`
/// misses the rest of them.
///
/// Measured with the real tool: `yq 3.4.3` reads `- run: | # the gate, and the
/// two lints it also runs` with two indented commands below it as one two-line
/// script. To a reader of the three literals it is not a block at all — the
/// body lines are swallowed as a plain-scalar continuation onto the header, and
/// `the_command_without_its_comment` then cuts everything from the ` #`, so
/// every rule downstream reads `run: |` and nothing else. A whole second lints
/// gate can stand under such a header with the transcript unmoved.
fn the_block_style(rest: &str) -> Option<char> {
    let mut letters = rest.trim_start().chars();
    let Some(style @ ('|' | '>')) = letters.next() else {
        return None;
    };
    // The indicators, in either order, at most one of each.
    let (mut digits, mut chomping) = (0_u8, 0_u8);
    let mut rest = letters.as_str();
    loop {
        let mut letters = rest.chars();
        match letters.next() {
            Some(letter) if letter.is_ascii_digit() => digits += 1,
            Some('+' | '-') => chomping += 1,
            _ => break,
        }
        rest = letters.as_str();
    }
    if digits > 1 || chomping > 1 {
        return None;
    }
    // What may follow is nothing, or a comment.
    let tail = rest.trim_start();
    if tail.is_empty() || tail.starts_with('#') {
        Some(style)
    } else {
        None
    }
}

/// Every line of a workflow, with continuations and folded scalars joined.
///
/// **A per-line scan reads `cargo xtask check \` and `--offline` as two
/// commands, and neither of them holds both halves.** Measured, twice:
///
/// ```text
/// run: |                          run: >-
///   cargo xtask check \             cargo xtask check
///     --offline                      --offline
/// ```
///
/// Read line by line, both give `ok N cargo command(s)` and no `--offline`
/// finding, while `the_ways_measured_after_the_first_five` carries `--offline`
/// as a measured attack — of the one-line spelling only. A needle inside a test
/// fixture does not defend the folded form: it is not a rule, and it reports
/// "the workflow has moved" when it fires.
///
/// The line number reported is the line the logical command **began** on, which
/// is where a reader looks. `\` inside a heredoc is left alone by the caller's
/// own heredoc tracking; what is joined here is the text, and a `\` at the end
/// of a line is a continuation in every shell this tree runs.
fn joined_continuations(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    let mut folded: Option<(usize, usize, String)> = None;
    let mut continuing: Option<(usize, String)> = None;
    // The entry a plain scalar's continuation lines are appended to, and the
    // indent that decides which lines those are.
    let mut last: Option<(usize, usize)> = None;
    // The indent of the key that opened a `|` block, while one is open.
    let mut literal: Option<usize> = None;
    for (index, raw) in text.lines().enumerate() {
        let indent = raw.len() - raw.trim_start().len();
        let line = raw.trim();
        // A folded block runs to the first line indented no further than the
        // key that opened it. An empty line inside one is a paragraph break and
        // does not end it.
        if let Some((at, opened_at, ref mut body)) = folded {
            if line.is_empty() || indent > opened_at {
                if !line.is_empty() {
                    body.push(' ');
                    body.push_str(line);
                }
                continue;
            }
            out.push((at, body.clone()));
            folded = None;
        }
        if let Some((at, ref mut body)) = continuing {
            body.push(' ');
            body.push_str(line.trim_end_matches('\\').trim());
            if line.ends_with('\\') {
                continue;
            }
            out.push((at, body.clone()));
            continuing = None;
            continue;
        }
        if let Some(rest) = line.strip_prefix("run:")
            && the_block_style(rest) == Some('>')
        {
            folded = Some((index, indent, "run:".to_owned()));
            continue;
        }
        if line.ends_with('\\') {
            continuing = Some((index, line.trim_end_matches('\\').trim().to_owned()));
            continue;
        }
        // **And the spelling that needs no marker at all.** A YAML plain scalar
        // continues onto every following line indented further than the key that
        // opened it, so
        //
        // ```text
        // run: cargo xtask check
        //   --offline
        // ```
        //
        // is one command to every YAML parser there is — `yq` reads
        // `cargo xtask check --offline` — and to a per-line reader it is a gate
        // and a stray word. Measured: that step went into `ci.yml` with the count
        // unmoved at twelve, no `--offline` finding, and `check passed`, while
        // both spellings the doc above names were caught. The same rule is what
        // makes `RUSTFLAGS:` with `-D warnings` on the next line an export the
        // env reader could not see.
        //
        // A literal block is the exception and has to be tracked for it: under
        // `run: |` every line is its own command, and joining an indented shell
        // body onto the line above would report a command that is not there.
        if let Some(opened_at) = literal {
            if line.is_empty() || indent > opened_at {
                out.push((index, line.to_owned()));
                last = None;
                continue;
            }
            literal = None;
        }
        if let Some((at, opened_at)) = last
            && indent > opened_at
            && !line.starts_with("- ")
            && !opens_a_key(line)
        {
            let (_, body) = &mut out[at];
            body.push(' ');
            body.push_str(line);
            continue;
        }
        if let Some(rest) = line.split_once(':').map(|(_, rest)| rest.trim())
            && the_block_style(rest) == Some('|')
        {
            literal = Some(indent);
        }
        // **A double-quoted scalar may hold its own line breaks.** They are
        // resolved here rather than in the rules downstream, so the walk and
        // the second count read the same YAML and their identity stays an
        // identity. One line unless the escapes say otherwise.
        let held = the_lines_behind_the_escapes(line);
        last = Some((out.len(), indent));
        for piece in held {
            out.push((index, piece));
        }
    }
    if let Some((at, _, body)) = folded {
        out.push((at, body));
    }
    if let Some((at, body)) = continuing {
        out.push((at, body));
    }
    out
}

/// Every cargo invocation in the same files, counted by a different route.
///
/// **The two-counter identity every other walk in this crate carries.**
/// With `seen` the first statement of the command loop and the only number
/// this walk reports, floored merely at non-zero, three lines under it —
/// `if index > 140 { continue; }` — leave a real second gate in CI
/// (`cargo clippy --workspace --all-targets -- -D warnings`) unjudged with
/// the count going *up*, 11 to 12, and `check passed` over it. That is
/// `NOCTUA.md §19.4`, which is this file's whole subject.
///
/// `judged` is incremented on every path out of the verdict, including the two
/// branches above the command loop — `runs_cargo_as_an_action` and
/// `assigns_cargo` — which reach a verdict and fall through rather than
/// `continue` out of the line, so a line that is both an assignment and a
/// command is counted the same way by both counters. Measured with a
/// `continue` in each: `C=cargo` plus `$C xtask check` in `ci.yml` reports
/// `11 cargo command(s) became this gate's to judge and 10 reached a verdict`
/// over a run with no filter in it at all, and the suite cannot see the
/// spurious finding because its two tests assert only on `found[0]`.
///
/// **A second count of the same ground, taken outside the line loop.** `seen`
/// and `judged` both live in the *inner* command loop, so a filter in the
/// enclosing line loop pays them equally and the identity is silent — which is
/// the class its own doc names. Measured: three lines, `if index > 140 {
/// continue; }`, plus a whole second gate job appended to `ci.yml`, gave
/// `ok 9 cargo command(s)` — the count moving *down* — and `check passed`.
///
/// This shares no loop with the walk: it reads the same files and counts the
/// same invocations by a different route, mirroring only the two skips the walk
/// makes before counting — a comment is prose, a label is a name.
///
/// **What it is, and what it is not.** It shares `cargo_commands`,
/// `runs_cargo_as_an_action` and `assigns_cargo` with the walk, so what it
/// catches is a **filter in the loop around them** — which is exactly what it
/// says. A spelling those three predicates cannot see is invisible to both
/// counts equally, and no comparison of two numbers can find it: that is the
/// predicates' own tests' job, and `$(which cargo)` is the measured case —
/// with leading punctuation trimmed and trailing not, both counters agree at a
/// number that is short by one.
fn a_second_count_of(files: &[std::path::PathBuf]) -> usize {
    files
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| {
            text.lines()
                .map(str::trim)
                .filter(|line| !line.starts_with('#') && !is_label(line))
                .flat_map(the_lines_behind_the_escapes)
                .map(|line| {
                    let line = line.as_str();
                    // **All three shapes the walk counts, not one of them.**
                    // `seen` also counts an action that runs cargo and an
                    // assignment that names it, so counting `cargo_commands`
                    // alone here makes `seen` exceed this count on a workflow
                    // holding either, and the identity fires with a message
                    // about *a filter inside the line loop*, which is not what
                    // happened. A gate that reports the wrong cause teaches
                    // a reader to scroll past it, which is the failure
                    // `counts.rs`'s own header calls worse than no gate.
                    usize::from(runs_cargo_as_an_action(line).is_some())
                        + usize::from(assigns_cargo(line).is_some())
                        + cargo_commands(line).len()
                })
                .sum::<usize>()
        })
        .sum()
}

/// What a finding about this gate's own arithmetic begins with.
///
/// **A finding about the instrument is not an answer about the tree.** Measured:
/// real attacks on `NOCTUA.md §19.4` are answered by `the_walk_is_not_short` —
/// a sentence saying this tool's two counters differ — with the suite green and
/// the attack itself unreported, wherever an attack helper accepts a bare
/// `!findings.is_empty()`. Refusing that one sentence by its words is too weak,
/// because a filter placed *between* the two counters raises the **other**
/// internal sentence, `everything_reached_a_verdict`'s, which such a refusal
/// does not name.
///
/// So no helper names a sentence. Both internal findings carry this prefix
/// from the place they are raised, and `is_about_this_gate` is the one
/// predicate all three attack helpers ask.
const ABOUT_THIS_GATE: &str = "this gate's own accounting — ";

/// Whether this finding is about the gate rather than about the tree.
///
/// Behind `#[cfg(test)]` because only the three attack helpers ask it: what it
/// judges is whether a *test* accepted the wrong answer, not what the gate says
/// to a developer.
#[cfg(test)]
fn is_about_this_gate(finding: &Finding) -> bool {
    finding.what.starts_with(ABOUT_THIS_GATE)
}

/// The walk saw every cargo invocation that second count found.
fn the_walk_is_not_short(seen: usize, expected: usize, findings: &mut Vec<Finding>) {
    if seen == expected {
        return;
    }
    let said = format!(
        "{ABOUT_THIS_GATE}the CI walk judged {seen} cargo command(s) and a second count of \
         the same files found {expected}: something inside the line loop is filtering, and \
         a second gate in the part it skipped reads exactly like no second gate"
    );
    findings.push(Finding {
        file: WORKFLOW.to_owned(),
        line: 1,
        what: said,
        fix: "the two counters inside the command loop cannot see a filter in the loop \
              around them — both are paid equally. This count shares no loop with them"
            .to_owned(),
    });
}

/// The two counters this walk keeps, compared.
///
/// **`corpus.rs`'s `ASKED` pins that this call exists, not that it fires.** So
/// `if true { return; }` here leaves the call site intact, the pin satisfied
/// and every other test green — a wrapper between a walk and its identity is a
/// place for exactly that. `the_identity_reports_a_walk_that_lost_a_line` is
/// what holds it.
fn the_identity(seen: usize, judged: usize, findings: &mut Vec<Finding>) {
    let before = findings.len();
    crate::gates::everything_reached_a_verdict("cargo command", WORKFLOW, seen, judged, findings);
    // Marked where it is raised rather than recognised by its words: the
    // sentence belongs to `gates.rs` and is shared by several gates.
    for finding in &mut findings[before..] {
        finding.what.insert_str(0, ABOUT_THIS_GATE);
    }
}

/// The finding for a chain the transitive walk could not reach the end of.
///
/// **The bound's own paragraph promises this.** It says a chain deeper than
/// [`HOPS`] "is worth a finding of its own rather than a hang", and a loop that
/// runs out silently makes a CI setup deeper than the bound one this gate reads
/// part of and then reports on as though it had read all of it.
fn a_chain_deeper_than_the_walk(ran_out: bool) -> Option<Finding> {
    ran_out.then(|| Finding {
        file: WORKFLOW.to_owned(),
        line: 1,
        what: format!("what CI reaches is more than {HOPS} hops deep"),
        fix: "the walk that follows what a step runs stopped at its bound, so what \
              this gate read is part of the chain and it reported on that as though \
              it were all of it. Flatten the chain, or raise `HOPS` and say why in \
              the same edit"
            .to_owned(),
    })
}

/// Every file CI runs, and whether the walk that found them stopped short.
///
/// **Apart from [`only_runs_the_gate`], because the two together are more than
/// one body may hold**, and separate on its own merits: *what CI reaches* and
/// *what is wrong with what it runs* fail for different reasons, and the first
/// is the half a reader has to trust before any finding below it means
/// anything.
fn everything_ci_reaches(root: &Path) -> Result<(Vec<std::path::PathBuf>, bool), String> {
    let mut files = Vec::new();

    // **Every file under `.github/`, whatever it is called.** A walk filtered to
    // YAML alone, under a module header that says the directory is walked, lets
    // a shell script beside the workflow, invoked from `ci.yml` by a `run: bash`
    // line, run clippy, formatting and tests with the count unchanged.
    // A script CI executes is part of what CI runs.
    walk(&root.join(WORKFLOWS), &mut |path| {
        files.push(path.to_path_buf());
    })?;
    // **And every file those reach.** A script a step runs and a local
    // composite action a step `uses:` are part of what CI runs and may live
    // anywhere; walking `.github/` alone lets two shapes through — a shell
    // script kept beside the sources, and a composite action in a directory of
    // its own.
    //
    // **Transitively, because one hop is not a boundary.** Scanning only the
    // files the `.github/` walk produces and never what *those* reach,
    // `ci.yml` running `sh scripts/ci-a.sh`, which runs `sh scripts/ci-b.sh`,
    // which runs `cargo fmt`, `cargo clippy` and `cargo test --workspace`, leaves
    // the count unchanged — a whole second gate in CI, behind one extra
    // line. Following what is reached until nothing new appears costs one loop
    // and closes every depth rather than the first.
    //
    // Bounded, because a script that names itself would otherwise not terminate
    // — and a chain this deep is a CI setup nobody can read anyway, which is
    // worth a finding of its own rather than a hang.
    let mut hops = HOPS;
    let ran_out = follow_what_they_reach(root, &mut files, &mut hops);
    // **And the `justfile`, unconditionally.** Reached only when a workflow
    // line happens to contain the word `just` — and `ci.yml`'s does not — the
    // surface a developer actually types is gated by nobody.
    // Measured: the `check:` recipe replaced with `cargo fmt --all --check`,
    // `cargo clippy --workspace` and `cargo test --workspace` gives
    // `ok 4 cargo command(s)` and `check passed`, against a file whose own
    // header says *"Every one is a `cargo xtask` verb underneath — `just` is
    // never the only path"*.
    //
    // `§19.4` is that the command surface does not grow, and a second gate
    // typed by hand is exactly how it grows. The file is committed and it makes
    // the claim, so it is read whether or not CI mentions it.
    // **Every name `just` looks for, not the lower-case one.** `just` searches
    // `justfile`, `.justfile`, `JUSTFILE` and `Justfile`, and joining one
    // spelling means that on a case-sensitive filesystem renaming the file takes
    // the whole surface out of the gate: `10 cargo command(s)` becomes `4`, with
    // no floor on that number and a second gate in the recipes.
    // **And the makefile, on the same argument.** Opened only when some
    // already-reached file happens to contain the bare word `make`, a committed
    // `Makefile` holding `cargo fmt --check`, `cargo clippy` and
    // `cargo test --workspace` sits in the tree with the count unchanged
    // and `check passed`. A committed file that makes the claim is read whether
    // or not CI mentions it, which is the reasoning written above for the
    // justfile and applies to this one unchanged.
    let recipes = the_files_answering_to(root, &JUSTFILE)
        .into_iter()
        .chain(the_files_answering_to(root, &MAKEFILE));
    joined(root, &mut files, recipes);
    let ran_out = follow_what_they_reach(root, &mut files, &mut hops) || ran_out;
    files.sort();
    Ok((files, ran_out))
}

/// Reads every CI file and reports every cargo command that is not the gate.
///
/// Returns the findings and how many cargo invocations were inspected, so a
/// workflow that was renamed or emptied is visible rather than green — `§19.3`'s
/// first rule, applied to the file that exists to enforce `§19.4`.
///
/// # Errors
///
/// Returns why the workflow could not be read. Its absence is an error and not
/// an empty result: the gate would otherwise pass loudest exactly when the thing
/// it guards had been deleted.
pub(crate) fn only_runs_the_gate(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    if !root.join(WORKFLOW).is_file() {
        return Err(format!(
            "{WORKFLOW} is missing. It is what keeps CI and a developer running the \
             same gate (NOCTUA.md §19.4), and a missing one is how they start to differ"
        ));
    }

    let (files, ran_out) = everything_ci_reaches(root)?;

    // **And what the justfile reaches**, which a walk finished before the
    // justfile joins cannot see — hence `everything_ci_reaches` follows what
    // the files reach a second time. Without that, a recipe shelling out to a
    // shell script — or `import`ing the file its recipes moved to — puts a
    // whole second gate one line beyond the boundary, with the count
    // unchanged. Measured, `check:` gaining one line that runs a script that
    // runs `cargo fmt`, `cargo clippy` and `cargo test --workspace`:
    // `ok 10 cargo command(s)` and `check passed`.
    let mut findings: Vec<Finding> = a_chain_deeper_than_the_walk(ran_out).into_iter().collect();
    let mut seen = 0_usize;
    let mut judged = 0_usize;
    let expected = a_second_count_of(&files);
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = relative_to(root, &path);
        // The workflow's own files, as against the `justfile` and the scripts
        // it reaches: a recipe a developer types may be offline; CI may not.
        let place = the_place_of(root, &path);
        let (file_is_ci, _) = the_role_of(&place);
        // Every file this walk found, not the workflow alone: a `justfile`
        // recipe and a script a step runs discard a verdict the same way.
        no_verdict_is_discarded(&relative, &text, &mut findings);
        // **The recipe a command sits under.** Unless a recipe's *name* is
        // related to the verb its body runs, any recipe can call any verb with
        // the one-line description above it — the whole of what `just --list`
        // prints — going on saying what it used to do. Measured by swapping the
        // bodies of `check` and `check-offline`: the gate a developer types
        // becomes the smaller gate, with README, AGENTS and the recipe's own
        // sentence all still calling it THE gate, and nothing else in the tree
        // says a word.
        for (index, line) in joined_continuations(&text) {
            let (header, recipe) = the_recipe_and_its_name(&text, index, file_is_ci, &place);
            // Already trimmed and already joined; `trimmed` is kept as the name
            // the rest of this loop reads it by.
            let trimmed = line.as_str();
            // Comments are prose, and this repository's CI prose explains the
            // rule — a gate that read its own explanation would report it.
            if trimmed.starts_with('#') {
                continue;
            }
            // A step's `name:` is a label, not a command, and CI steps are
            // commonly named after what they run — `- name: cargo xtask check`.
            // Reading one as an invocation reports the workflow for describing
            // itself.
            if is_label(trimmed) {
                continue;
            }
            // **A standard action is an invocation.** `actions-rs/cargo` <!-- allow-reference: names `actions-rs/cargo` — a GitHub Action, not a path in this tree --> runs
            // cargo from a `with: command:` block and needs no `run:` line at
            // all, so nothing here would ever have seen it.
            if let Some(action) = runs_cargo_as_an_action(trimmed) {
                seen += 1;
                findings.push(finding(
                    &relative,
                    index + 1,
                    &action,
                    "an action whose job is to run cargo is a cargo command with a \
                     different spelling. Everything CI checks is a step of \
                     `cargo xtask check`",
                ));
                judged += 1;
                // **And a line may be more than one shape**, so these three
                // arms fall through and both counters see the same line the
                // same way. With a `continue` in each, the walk counts such a
                // line once while `a_second_count_of` — which adds all three —
                // counts it twice; `the_identity` then fires with a sentence
                // about a filter in the line loop, which is false about its own
                // cause, and the real second command on that line is never
                // reported: `CARGO=cargo; cargo clippy --workspace` is an
                // assignment *and* a cargo command, and the clippy half goes
                // unread.
            }
            // **An alias is an invocation.** `C=cargo` then `$C clippy` runs
            // clippy and never spells `cargo `, so the assignment is where it
            // has to be caught.
            if let Some(alias) = assigns_cargo(trimmed) {
                seen += 1;
                findings.push(finding(
                    &relative,
                    index + 1,
                    &format!("{alias}=cargo"),
                    "a name for `cargo` is a cargo command wearing a different \
                     spelling, and the gate that keeps CI and a developer running \
                     one gate cannot read shell variables. Write the command out",
                ));
                judged += 1;
            }
            for command in cargo_commands(trimmed) {
                // See this function's doc: `seen` alone is payable by a filter.
                seen += 1;
                let verb = command.split_whitespace().nth(1).unwrap_or("");
                // `--offline` is a smaller gate wearing the gate's name; the
                // verb check is a prefix match, so it satisfied `JOBS_MUST_RUN`
                // while skipping the database, the integration tests and the
                // dependency audit. Asked of CI's files and not of the
                // `justfile`, whose `check-offline` recipe is what a developer
                // types on a train.
                // **And outside CI it is allowed in one place only.** The
                // exclusion is for `check-offline`, a recipe a developer types
                // on a train; applied to every non-CI file, it lets a shell
                // script a step runs carry the flag with no recipe name at all
                // and nothing speaks. The name that permits it is the one that
                // says so.
                if an_offline_gate_is_wearing_the_gates_name(&recipe, &command, file_is_ci) {
                    findings.push(finding(
                        &relative,
                        index + 1,
                        &first_words(&command),
                        "`--offline` skips the steps that need the stack — the database, \
                         the integration tests and the dependency audit — so whatever \
                         runs this reports a gate it did not run. It exists for a \
                         developer with no network, in a recipe whose name ends \
                         `-offline` and says so, and `check.rs` says CI never passes it",
                    ));
                } else if !file_is_ci && verb == XTASK && !recipe.is_empty() {
                    if let Some(named) = a_recipe_runs_what_it_is_called(&recipe, &header, &command)
                    {
                        findings.push(finding(
                            &relative,
                            index + 1,
                            &first_words(&command),
                            &named,
                        ));
                    }
                } else if verb != XTASK && verb != INSTALL {
                    findings.push(finding(
                        &relative,
                        index + 1,
                        &first_words(&command),
                        "everything CI checks is a step of `cargo xtask check`, so a \
                     second cargo command here makes CI and a developer run two \
                     different gates — and the one that decides becomes whichever \
                     is stricter that week (NOCTUA.md §19.4). Put the check inside \
                         the verb instead",
                    ));
                }
                // One exit, so the identity above cannot be paid by a branch.
                judged += 1;
            }
        }
    }

    // **And the jobs are still there.** A workflow that runs only the gate and
    // has lost the job that checks determinism across processes is a workflow
    // that passes this and proves less.
    let ci = std::fs::read_to_string(root.join(WORKFLOW)).unwrap_or_default();
    jobs_are_what_they_are_named(&ci, &mut findings);
    ci_supplies_what_the_gate_needs(root, &ci, &mut findings);
    the_alias_still_runs_the_gate(root, &mut findings);

    the_identity(seen, judged, &mut findings);
    the_walk_is_not_short(seen, expected, &mut findings);
    Ok((findings, seen))
}

/// The alias that decides what `cargo xtask` means.
///
/// **Every gate in this file reads what CI *types*, and what that resolves to
/// is read here.** `.cargo/config.toml` defines the alias, so one line there
/// decides whether `cargo xtask check` runs the gate at all. Measured:
///
/// ```text
/// -xtask = "run --quiet --locked --package xtask --"
/// +xtask = "test --quiet --locked --package xtask --bins"
///
/// $ cargo xtask check
/// EXIT=0   test result: ok. 21 passed
/// ```
///
/// Exit zero, the gate never ran, `10 cargo command(s)` unchanged and no
/// finding anywhere — while this file walks all of `.github/` sixteen hops
/// deep and reads all four spellings of the justfile *unconditionally* because
/// that is "the surface a developer types". The one file that is CI's actual
/// entry point, and whose own comment calls itself "the entire reason the
/// command surface needs no bootstrap step", is read by nothing else.
///
/// What it has to be is a `run` of this package: `run --package xtask` is the
/// only form that executes the gate. `--quiet` and `--locked` are the shipped
/// flags and are checked as prose by nothing — what is checked is the verb and
/// the package, because those are what decide whether the verdict is the gate's.
const ALIAS: &str = ".cargo/config.toml";

/// The legacy spelling of the same file, which cargo prefers when both exist.
///
/// Cargo reads `.cargo/config` as well and warns that it is *using* it over <!-- allow-reference: names `.cargo/config` — the legacy spelling this repository deliberately does not ship -->
/// `.cargo/config.toml` — so the alias this gate reads is not the one that
/// decides, and a second file beside the one it opens is a whole gate switched
/// off. Both are read here, and either may carry the alias.
const LEGACY_ALIAS: &str = ".cargo/config";

/// The alias is a `run` of the `xtask` package, or `cargo xtask` is not the gate.
fn the_alias_still_runs_the_gate(root: &Path, findings: &mut Vec<Finding>) {
    // **Cargo prefers `.cargo/config` when both exist**, and says so: allow-reference: names `.cargo/config` — the legacy spelling this repository deliberately does not ship
    // `warning: both …/.cargo/config and …/.cargo/config.toml exist. Using
    // …/.cargo/config`. Reading only the one this repository ships leaves the
    // file that actually decides unread, so both are judged and either may
    // carry the alias.
    if root.join(LEGACY_ALIAS).is_file() {
        one_alias_file(root, LEGACY_ALIAS, findings);
        no_flags_reach_the_compiler(root, LEGACY_ALIAS, findings);
    }
    one_alias_file(root, ALIAS, findings);
    no_flags_reach_the_compiler(root, ALIAS, findings);
    // **And every ancestor, because cargo walks to the filesystem root.** Against
    // a reader of the repository's own cargo configuration alone, one directory up is enough:
    //
    // ```text
    // $ printf '[build]\nrustflags = ["--cap-lints=allow"]\n' > ../.cargo/config.toml
    //   ok    rustfmt's configuration walk stops in this repository
    //   ok    lints
    // check passed
    // ```
    //
    // The line above it is the point — this repository asks and answers exactly
    // this question for **rustfmt**, and **cargo** is owed the same question. In
    // the real layout that parent is the workspace's own git repository, so the
    // file is committable.
    // **Canonical, because `parent()` is lexical.** The root is written
    // `xtask/..`, whose lexical parent is `xtask` — so a lexical walk climbs back
    // *into* the repository and reports its own configuration as one above it.
    let here = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut at = here.parent();
    while let Some(above) = at {
        for name in [ALIAS, LEGACY_ALIAS] {
            if above.join(name).is_file() {
                findings.push(Finding {
                    file: above.join(name).to_string_lossy().into_owned(),
                    line: 1,
                    what: "a cargo configuration above this repository decides what \
                           `cargo xtask check` compiles with"
                        .to_owned(),
                    fix: "cargo merges every `.cargo/config.toml` from the working \
                          directory up to the filesystem root, so a `[build] rustflags` \
                          there reaches this build — `--cap-lints=allow` in one turns \
                          off every workspace lint and `unsafe_code = \"forbid\"` with \
                          `check passed`. Move it, or accept that this run's verdict is \
                          about a different compilation than CI's"
                        .to_owned(),
                });
            }
        }
        at = above.parent();
    }

    // **And the one cargo really reads, which is not an ancestor.** Cargo merges
    // `$CARGO_HOME/config.toml` into the chain regardless of where it sits, and
    // `CARGO_HOME` defaults to a dot-cargo directory in the home — so the walk
    // above only reaches it by coincidence, when the home happens to be under a
    // parent of this repository. Where `CARGO_HOME` is an XDG path under the
    // user's data directory it is not, so the loop checks a home that holds no
    // cargo configuration and never opens the file that decides the build.
    // Measured, with six real clippy findings in criterion 3's own suite and a
    // `[build]` `rustflags` of `--cap-lints=allow` written there:
    //
    // ```text
    //   ok    formatting
    //   ok    lints
    //   ok    818 test(s)
    // check passed
    // ```
    //
    // — every count byte-identical, no finding about a cargo configuration, and
    // `--cap-lints=allow` demotes `unsafe_code = "forbid"` along with the rest.
    the_home_cargo_reads(cargo_home(), &here, findings);
}

/// The configuration in cargo's own home, judged for what reaches the compiler.
///
/// **The home is a parameter rather than a read.** `unsafe_code = "forbid"`
/// covers the test module too, so a test cannot `set_var` to point this
/// somewhere it owns — and the alternative, a test that re-implements the
/// judgement over its own path, is the copy-of-the-thing-under-test shape this
/// repository does not write. The caller reads the environment; this
/// takes the answer.
fn the_home_cargo_reads(
    home: Option<std::path::PathBuf>,
    here: &Path,
    findings: &mut Vec<Finding>,
) {
    if let Some(home) = home
        && !home.starts_with(here)
        && !here.ancestors().any(|above| above == home)
    {
        for name in ["config.toml", "config"] {
            // **Judged by what it holds, not by existing.** A file here is
            // ordinary — a registry mirror, a linker for a target, `[net]` — and
            // a gate that failed every machine carrying one is a gate somebody
            // switches off, which is the same argument this file makes two
            // screens down about `CARGO_HOME` itself. What is not ordinary is a
            // table that reaches rustc, and that is the question the repository's
            // own configuration is already asked.
            let path = home.join(name);
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let named = path.to_string_lossy().into_owned();
            let Ok(table) = text.parse::<toml::Table>() else {
                findings.push(Finding {
                    file: named,
                    line: 1,
                    what: "cargo's own home holds a file cargo reads and this gate \
                           cannot parse"
                        .to_owned(),
                    fix: "cargo merges it into every build wherever that directory sits, \
                          so a file it can read and this cannot is a set of flags nobody \
                          has looked at"
                        .to_owned(),
                });
                continue;
            };
            for reaching in table
                .keys()
                .filter(|key| REACHES_RUSTC.contains(&key.as_str()))
            {
                findings.push(Finding {
                    file: named.clone(),
                    line: 1,
                    what: format!(
                        "cargo's own home declares `{reaching}`, which decides what \
                         `cargo xtask check` compiles with"
                    ),
                    fix: "cargo merges `$CARGO_HOME/config.toml` into every build \
                          wherever that directory sits, so it is not caught by the walk \
                          up from this repository — a `[build] rustflags` there turns off \
                          every workspace lint and `unsafe_code = \"forbid\"` with \
                          `check passed`. Move it, or accept that this run's verdict is \
                          about a different compilation than CI's"
                        .to_owned(),
                });
            }
        }
    }
}

/// The tables of a cargo configuration that reach the compiler.
///
/// The repository's own file is held to `alias` and nothing else, because it is
/// this repository's to write. A file in cargo's home is not — it is the
/// developer's, and most of what belongs in it is nobody's business here. These
/// are the tables that change what `check` compiles: `build` carries
/// `rustflags` and `rustdocflags`, `target` carries per-target `rustflags` and
/// the linker, and `unstable` turns on flags that change the lint pipeline.
const REACHES_RUSTC: [&str; 3] = ["build", "target", "unstable"];

/// Where cargo keeps its own configuration, as cargo resolves it.
///
/// `CARGO_HOME` if it is set, and a dot-cargo directory in the home otherwise.
/// Canonicalised, because
/// the caller compares it against a canonical repository path — and a symlinked
/// home would otherwise never match.
fn cargo_home() -> Option<std::path::PathBuf> {
    let named = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cargo"))
        })?;
    Some(named.canonicalize().unwrap_or(named))
}

/// A YAML key with the quotes YAML lets it carry removed.
///
/// `"RUSTFLAGS"`, `'RUSTFLAGS'` and `RUSTFLAGS` are one key to every YAML
/// parser, and this file's other reader already knew that for `if:`.
fn unquoted(key: &str) -> &str {
    key.trim_matches(['"', '\''])
}

/// Nothing in the workflow's own `env:` blocks reaches rustc.
///
/// The process-environment check beside this one cannot see them: a runner
/// exports them before the step starts, and a developer's shell never does. So
/// the file is read for the same family of names, and a match is a finding
/// against the workflow — where the fix is.
///
/// Parsed as lines rather than as YAML: `serde_yaml` is unmaintained and
/// `AGENTS.md` rule 3 says a dependency that could reasonably be written here is
/// asked about first. What is needed is "a key at any indentation whose name is
/// in the family", which is one `split_once`.
fn the_workflow_exports_nothing_that_reaches_rustc(root: &Path, findings: &mut Vec<Finding>) {
    // Every workflow file, walked — a second one added later exports its own
    // environment and would be outside a list naming only the first.
    let mut files = Vec::new();
    let _ = crate::gates::walk(&root.join(WORKFLOWS), &mut |path| {
        if path
            .extension()
            .is_some_and(|kind| kind == "yml" || kind == "yaml")
        {
            files.push(path.to_path_buf());
        }
    });
    files.sort();
    for path in files {
        let named = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // **Joined the way every other reader here joins.** A YAML key whose
        // value begins on the next line is one export to any parser —
        // `RUSTFLAGS:` with `-D warnings` under it reads as `RUSTFLAGS: -D
        // warnings`, and `yq` says so — and a reader of raw lines matches the key
        // with an empty value and never makes the finding. The
        // process-environment half still refuses at run time, so CI goes red on
        // the runner after spending the whole build: for the right reason at the
        // wrong moment, which is the cost this file half exists to avoid.
        for (index, line) in joined_continuations(&text) {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            let Some((key, value)) = trimmed.split_once(':') else {
                continue;
            };
            // **A quoted key is a key, and a flow mapping holds several.** Read
            // raw with `key.trim()`, `"RUSTFLAGS": -D warnings` — valid YAML,
            // and what a formatter that quotes every key writes — is not a
            // match, and neither is `env: {CARGO_TERM_COLOR: always, RUSTFLAGS:
            // -D warnings}`, whose first `:` yields the key `env`. The reader
            // eight hundred lines down strips quotes for `if:` as well;
            // measured, both spellings leave `ok` where the bare one is a
            // finding. The process-environment half still refuses at run time,
            // so CI would go red — for the right reason at the wrong moment,
            // after the runner has spent the build.
            let pieces: Vec<&str> = if let Some(flow) = trimmed
                .split_once('{')
                .and_then(|(_, rest)| rest.rsplit_once('}'))
                .map(|(inside, _)| inside)
            {
                flow.split(',').collect()
            } else {
                vec![trimmed]
            };
            let Some((key, value)) = pieces
                .iter()
                .filter_map(|piece| piece.split_once(':'))
                .find(|(key, value)| {
                    reaches_rustc(unquoted(key.trim())) && !value.trim().is_empty()
                })
            else {
                let _ = (key, value);
                continue;
            };
            if !reaches_rustc(unquoted(key.trim())) || value.trim().is_empty() {
                continue;
            }
            findings.push(Finding {
                file: named.clone(),
                line: index + 1,
                what: format!(
                    "this workflow exports `{}`, and the gate refuses to run with it set",
                    key.trim()
                ),
                fix: "cargo passes it to rustc alongside whatever `.cargo/config.toml` \
                      says, so a run with it set is not the run this repository's own \
                      gate makes — and the gate's only step here *is* that gate, so the \
                      workflow can never go green. `check` already runs \
                      `cargo clippy -- -D warnings` itself, which is where that rule \
                      belongs: warnings are errors at the gate and nowhere else"
                    .to_owned(),
            });
        }
    }
}

/// Whether an environment variable's name is one cargo hands to rustc.
///
/// Named so the process half and the workflow half ask one question.
fn reaches_rustc(name: &str) -> bool {
    matches!(
        name,
        "RUSTFLAGS" | "RUSTDOCFLAGS" | "RUSTC" | "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER"
    ) || (name.starts_with("CARGO_")
        && (name.ends_with("RUSTFLAGS")
            || name.ends_with("RUSTDOCFLAGS")
            || name.ends_with("RUSTC")
            || name.ends_with("RUSTC_WRAPPER")
            || name.ends_with("RUSTC_WORKSPACE_WRAPPER")))
        // **And the profile, which carries no flag by that name and turns
        // `debug_assert!` and the overflow checks off all the same.**
        // `CARGO_PROFILE_TEST_DEBUG_ASSERTIONS=false` is an environment
        // variable cargo turns into `-C debug-assertions=off`: measured,
        // `--print cfg` answers `debug_assertions` on a clean tree and
        // nothing with it set, and twelve tests that fail on a falsified
        // `debug_assert!` pass under it. It ends with none of the five names
        // above, so a family that reads only the end of the name walks past it.
        // `overflow-checks` is the half that costs most here: the stack's bound
        // test records that it "panics today only because the test profile
        // inherits `dev`'s overflow checks".
        || name.starts_with("CARGO_PROFILE_")
}

/// The cargo configuration carries an alias and nothing that reaches rustc.
///
/// **The `[alias]` table is not the only one that decides.** This gate
/// exists because "one line there decides whether `cargo xtask check` runs the
/// gate at all" — and the same file has a second table that decides whether the
/// gate, once run, can fail. Measured, with eight real clippy violations planted
/// in acceptance criterion 3's own suite and three lines appended here:
///
/// ```toml
/// [build]
/// rustflags = ["--cap-lints=allow"]
/// ```
///
/// ```text
/// ok    72909 source line(s) compile no canary        <- every count up
/// ok    lints
/// ok    789 test(s)
/// check passed
/// ```
///
/// No predicate weakened, no walk narrowed, every canary found. And it is not
/// only clippy: `--cap-lints=allow` demotes `[workspace.lints.rust] unsafe_code =
/// "forbid"`, so a crate with `#![forbid(unsafe_code)]` and a real dereference of
/// a raw pointer builds clean. Acceptance criterion 1 is "no failures and no
/// warnings", and this makes both true by making neither reachable.
///
/// The rule is a whitelist rather than a list of forbidden keys, because the set
/// of ways cargo can pass a flag to rustc grows with cargo: `build.rustflags`,
/// `build.rustdocflags`, `target.<triple>.rustflags`, and whatever is added next.
/// What this repository needs from this file is one alias.
fn no_flags_reach_the_compiler(root: &Path, file: &str, findings: &mut Vec<Finding>) {
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
        return;
    };
    // **Asked of a parser, not of the text.** Finding a table by
    // `strip_prefix('[')` misses TOML's dotted-key form, which is the same table:
    // `build.rustflags = […]` *is* `[build] rustflags = […]` — it is the spelling
    // `cargo --config` itself uses — and a file with no `[` line declares no
    // table such a reader can see. Measured, one line above the shipped `[alias]` with
    // four real clippy violations planted in criterion 3's own suite:
    //
    // ```text
    // build.rustflags = ["--cap-lints=allow"]
    //
    // ok    lints
    // ok    807 test(s)
    // check passed
    // ```
    //
    // while the identical setting written with a header was caught. This
    // repository makes exactly this argument about a manifest one file over —
    // "the question is asked of the parser, not of the text" — and it holds
    // here unchanged.
    let parsed: Result<toml::Table, _> = text.parse();
    match parsed {
        Ok(table) => {
            for named in table.keys() {
                if named != "alias" {
                    findings.push(Finding {
                        file: file.to_owned(),
                        line: 1,
                        what: format!(
                            "this file declares `{named}`, and the only table it may hold \
                             is `alias`"
                        ),
                        fix: "cargo reads this file before anything else and it can hand \
                              rustc flags of its own — `--cap-lints=allow` under `build` \
                              turns off every workspace lint *and* \
                              `unsafe_code = \"forbid\"`, with `ok lints` and \
                              `check passed` over real violations. Lints belong in \
                              `[workspace.lints]`, where a diff shows them"
                            .to_owned(),
                    });
                }
            }
        }
        Err(problem) => findings.push(Finding {
            file: file.to_owned(),
            line: 1,
            what: format!("this file is not TOML cargo can read: {problem}"),
            fix: "cargo reads it before anything else, so a file it cannot parse is a \
                  build whose flags nobody has read — including this gate"
                .to_owned(),
        }),
    }
    let stripped = crate::gates::without_comments(&text, "#");
    for (index, line) in stripped.lines().enumerate() {
        let line = line.trim();
        if let Some(table) = line
            .strip_prefix('[')
            .and_then(|rest| rest.split(']').next())
        {
            // The line-oriented pass is kept for its *line number*, which the
            // parsed table set does not carry, and reports nothing the parser has
            // not already reported.
            if table != "alias" {
                findings.push(Finding {
                    file: file.to_owned(),
                    line: index + 1,
                    what: format!(
                        "this file declares `[{table}]`, and the only table it may hold \
                         is `[alias]`"
                    ),
                    fix: "cargo reads this file before anything else and it can hand \
                          rustc flags of its own — `--cap-lints=allow` in `[build]` \
                          rustflags turns off every workspace lint *and* \
                          `unsafe_code = \"forbid\"`, with `ok lints` and `check passed` \
                          over eight real violations. Lints belong in \
                          `[workspace.lints]`, where a diff shows them"
                        .to_owned(),
                });
            }
        }
    }
    // **And the environment, which is the same flag by another route.** Cargo
    // joins `RUSTFLAGS` with what this file says, so a shell that exports it
    // reaches the compiler without touching the repository at all.
    // **The family, not three names.** Cargo maps *every* config key to an
    // environment variable — `[build] rustflags` is `CARGO_BUILD_RUSTFLAGS`, and
    // `[target.<triple>] rustflags` is `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` — so
    // refusing the table in the file and three fixed variable names leaves the
    // same table reachable by its other spelling. Measured:
    //
    // ```text
    // $ CARGO_BUILD_RUSTFLAGS="--cap-lints=allow" cargo xtask check
    //   ok    lints
    // check passed
    // ```
    //
    // over the same eight real violations the file half refuses.
    // **And the environment CI exports, which is not this process's.** Reading
    // `std::env::vars()` alone is silent on every developer machine and fires
    // only on a runner, where it is fatal: a `ci.yml` setting `RUSTFLAGS: -D
    // warnings` at workflow scope reaches every job's gate step, and the
    // gate refuses to run with `RUSTFLAGS` set — those two rules cannot both
    // hold, so the workflow can never go green, and nothing finds it unless
    // something reads the file that sets it.
    //
    // Reported against the workflow rather than against `.cargo/config.toml`,
    // because that is the file to edit.
    the_workflow_exports_nothing_that_reaches_rustc(root, findings);
    let named: Vec<String> = std::env::vars()
        .map(|(name, _)| name)
        .filter(|name| {
            // **And the compiler itself, not only its flags.** `RUSTC_WRAPPER`
            // replaces every rustc invocation, clippy's included, so a wrapper
            // that appends `--cap-lints=allow` does what the `[build]` table is
            // refused for — and the file half refuses `rustc-wrapper` there, so
            // leaving it out here makes the two halves disagree. Measured, with a real
            // `needless_range_loop` planted:
            //
            // ```text
            // RUSTC_WRAPPER=<sh -c 'exec "$@" --cap-lints=allow'> cargo xtask check
            //   ok    lints
            // check passed
            // ```
            // **`CARGO_HOME` is deliberately not here.** It does not belong
            // to this family: it carries no flag, it chooses the *file* that
            // might, and setting it to an XDG data directory is what an
            // ordinary machine does — so listing it would fail every run on
            // such a machine with a message about a hazard that is not
            // present. A gate that refuses
            // an ordinary arrangement is a gate somebody switches off. The file
            // it names is read directly instead, one screen up.
            // **The rule, not a second copy of it.** `reaches_rustc`'s own doc
            // says it is "named so the process half and the workflow half ask
            // one question", and a transcription of it here rather than a call
            // is the shape this file's own catalogue calls one rule written
            // twice with the second copy narrower: a clause added to the family
            // would have two places to be added in.
            reaches_rustc(name)
        })
        .collect();
    for named in named {
        let named = named.as_str();
        if let Ok(set) = std::env::var(named)
            && !set.trim().is_empty()
        {
            findings.push(Finding {
                file: file.to_owned(),
                line: 1,
                what: format!("`{named}` is set in this environment, to `{set}`"),
                fix: "cargo passes it to rustc alongside anything this file says, so a \
                      run with it set is not the run CI makes. Unset it, or accept that \
                      this run's verdict is about a different compilation"
                    .to_owned(),
            });
        }
    }
}

/// Every word `cargo xtask` may resolve to.
///
/// A whitelist rather than a list of refused flags, for the reason `AGENTS.md`
/// invariant 10 gives about `[build]`: the ways cargo can change what a command
/// does grow with cargo, and a deny-list is a list of the ones somebody thought
/// of. `--` is the separator that hands the rest to `xtask` itself.
///
/// **`-p` is here because `runs_the_gate` already accepts it.** A whitelist that
/// refused a spelling the gate beside it takes is the "too strong" half of a
/// repair, and this one is measured: without `-p`, `xtask = "run -p xtask
/// --locked --"` — a fixture this file's own tests hand in as legitimate — is
/// refused.
const ALIAS_MAY_HOLD: [&str; 7] = [
    "run",
    "--quiet",
    "--locked",
    "--package",
    "-p",
    "xtask",
    "--",
];

/// The words `cargo xtask` resolves to, read with the TOML parser.
///
/// Cargo accepts an alias as a string or as a list of strings, and it accepts
/// the key dotted (`alias.xtask`) as readily as under a `[alias]` header. All
/// three are the same alias, and a whitespace split over the raw text answers
/// only the first — reporting a legitimate list form as missing the `--locked`
/// that is its own fourth element.
fn the_alias_in(text: &str) -> Result<Vec<String>, String> {
    // **A `Table`, not a `Value`.** `"…".parse::<toml::Value>()` parses a TOML
    // *value expression*, so a whole document is `unexpected content, expected
    // nothing` at line 1 column 1 — which reads as a broken config file and is
    // the parser refusing the wrong question.
    // **Written as explicit `Err`s**, not `map_err`/`ok_or_else`:
    // `the_verdicts_the_source_can_push` reads the literals behind `Err(` and
    // `.push(`, and a sentence handed to a closure is a sentence the census
    // cannot see — a closure here takes one of these out of
    // `WHAT_THE_GATE_CAN_SAY` while it still prints.
    let document: toml::Table = match text.parse() {
        Ok(document) => document,
        Err(error) => {
            return Err(format!("this cargo configuration does not parse: {error}"));
        }
    };
    let Some(alias) = document.get("alias").and_then(|table| table.get("xtask")) else {
        return Err("this file declares no `xtask` alias".to_owned());
    };
    match alias {
        toml::Value::String(one) => Ok(one.split_whitespace().map(ToOwned::to_owned).collect()),
        toml::Value::Array(many) => many
            .iter()
            .map(|word| match word.as_str() {
                Some(text) => Ok(text.to_owned()),
                None => Err(
                    "the `xtask` alias is a list holding something that is not a string".to_owned(),
                ),
            })
            .collect(),
        other => Err(format!(
            "the `xtask` alias is a {}, and cargo takes a string or a list of strings",
            other.type_str()
        )),
    }
}

/// The `xtask` alias in one cargo configuration file.
fn one_alias_file(root: &Path, file: &str, findings: &mut Vec<Finding>) {
    let Ok(text) = std::fs::read_to_string(root.join(file)) else {
        findings.push(Finding {
            file: file.to_owned(),
            line: 1,
            what: "the `cargo xtask` alias is not there".to_owned(),
            fix: "every gate in this file checks what CI types, and this is what those \
                  words resolve to. Without it `cargo xtask check` is not a command at \
                  all, and `§19.4`'s one gate has no entry point"
                .to_owned(),
        });
        return;
    };
    // **Asked of the TOML parser, not split out of the text.** Splitting the
    // line after `xtask =` on whitespace sets aside the argument this file makes
    // 200 lines above about `[build]` versus `build.rustflags`, for the one
    // string CI resolves through. Two costs, both measured:
    //
    // * cargo accepts an alias as a **list** — `xtask = ["run", "--quiet",
    //   "--locked", "--package", "xtask", "--"]` — and cargo resolves it
    //   correctly; the whitespace split reports `` `cargo xtask` runs `cargo
    //   ["run", …, "--locked", …]`, without `--locked` ``, which asserts the
    //   inverse of the file's contents. Too strong, and green only while
    //   nothing legitimate tries it.
    // * a dotted key (`alias.xtask = "…"`) is the same alias in a table a
    //   line-walk never enters.
    //
    // The parser answers both, and the decoy a hand-written `[alias]`-table
    // walk has to guard against as well: with the `and_then` outside the
    // `find_map`, a `[env] xtask = …` above the real alias decides —
    // `cargo xtask check` runs 21 tests, exits 0, and the gate never runs.
    let words = match the_alias_in(&text) {
        Ok(words) => words,
        Err(problem) => {
            findings.push(Finding {
                file: file.to_owned(),
                line: 1,
                what: problem,
                fix: "`cargo xtask <verb>` is the whole command surface (NOCTUA.md §19.4), \
                      and this file is what makes those words mean anything"
                    .to_owned(),
            });
            return;
        }
    };
    let alias = words.join(" ");
    let alias = alias.as_str();
    let words: Vec<&str> = words.iter().map(String::as_str).collect();
    // **A whitelist, because the ways cargo can hand rustc a flag grow with
    // cargo.** `AGENTS.md` invariant 10 says exactly that, and it holds for the
    // alias's own argument list as much as for this file's *tables* — a gate
    // that looks for three words leaves every other word unread.
    // Two ordinary cargo spellings, each needing no second table:
    //
    // ```text
    // --config target.<triple>.runner=["true"]   -> `cargo xtask check` prints
    //                                               nothing and exits 0
    // --manifest-path <another tree>/Cargo.toml  -> another workspace's xtask
    //                                               prints `check passed`
    // ```
    //
    // In both, a gate that looks for three words reports
    // `ok 11 cargo command(s)`. Naming what is
    // allowed is the only shape that closes a set nobody can enumerate.
    for word in &words {
        if !ALIAS_MAY_HOLD.contains(word) {
            findings.push(Finding {
                file: file.to_owned(),
                line: 1,
                what: format!("`cargo xtask` is an alias holding `{word}`"),
                fix: format!(
                    "this alias is what `cargo xtask check` resolves to, so anything \
                     inside it decides what runs — `--config` can give cargo a runner \
                     that never starts the binary, and `--manifest-path` can point the \
                     whole command at another tree, both of them printing a clean \
                     transcript. What it may hold is written down as {ALIAS_MAY_HOLD:?} \
                     and nothing else; add a word there in the same edit that adds it \
                     here, and say why"
                ),
            });
        }
    }
    let runs_the_gate = words.first() == Some(&"run")
        && words
            .windows(2)
            .any(|pair| matches!(pair[0], "--package" | "-p") && pair[1] == "xtask");
    if !runs_the_gate {
        findings.push(Finding {
            file: file.to_owned(),
            line: 1,
            what: format!("`cargo xtask` is an alias for `cargo {alias}`"),
            fix: "that is what `cargo xtask check` resolves to, so it decides whether \
                  the gate runs at all — and every other check here reads the words CI \
                  types rather than what they mean. It has to be a `run` of the `xtask` \
                  package"
                .to_owned(),
        });
    }
    // **`--locked`, read and not stepped over.** The header of
    // `.cargo/config.toml` calls the flag load-bearing and writes the
    // measurement out — `rm Cargo.lock && cargo xtask check` passes without it.
    //
    // Measured: with the flag dropped and `Cargo.lock` moved aside,
    // `cargo xtask --help` exits 0 and the lockfile is regenerated in silence;
    // with it, cargo refuses. A gate that reads this file for the verb and the
    // package and not for the flag that decides whether the lockfile is real is
    // a gate reading past the thing it came for.
    if !words.contains(&"--locked") {
        findings.push(Finding {
            file: file.to_owned(),
            line: 1,
            what: format!("`cargo xtask` runs `cargo {alias}`, without `--locked`"),
            fix: "the lockfile is what makes a run reproducible, and without this flag \
                  cargo regenerates a missing or stale one and carries on — so the gate \
                  passes against dependencies nobody wrote down. `.cargo/config.toml`'s \
                  own header carries the measurement"
                .to_owned(),
        });
    }
}

/// What a path names, however it is spelled.
///
/// **A spelling is not a file.** On a filesystem that ignores case, as macOS's
/// and Windows' do, `JUSTFILE` and `justfile` are one file; and a step into a
/// directory and back out of it is a second spelling of where it began. A walk
/// that asks whether it holds a path by its spelling reads the one justfile
/// three times there and counts each of its six commands three times: on one
/// tree, `ok 19 cargo command(s)` on Linux and `31` on macOS. The path the
/// system resolves a spelling to is the file; a path it cannot resolve is kept
/// as spelled, which is then all there is to compare.
fn the_file_itself(path: &Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Where a spelling puts a file: its steps back taken out, and every name
/// it went through kept as spelled.
///
/// **A step back is not a place.** `cd ci && sh ../.github/scripts/x.sh`
/// reaches a script under `.github/` through a step into `ci` and back out,
/// and read from the spelling that is a script outside it: a second reading of
/// the file the `.github/` walk already holds, so its commands are counted
/// twice and each finding in it is reported twice. And `.github/../justfile`
/// is the root justfile: read as one of CI's files, its recipe that runs
/// offline is refused.
///
/// **A link is a place.** A link names its file from where the link sits,
/// and so does a link to a directory: a justfile the `.github/` walk finds
/// through a directory link is read there as one of CI's files, as it is
/// found. So the steps back are taken out as written, and the system is asked
/// only whether what is left names the same file. Where it does not, the
/// spelling stepped back out of a directory link, which the system takes from
/// where the link points: the place is then the directory the system resolves,
/// with the last name as spelled.
fn the_place_of(root: &Path, path: &Path) -> String {
    let mut place = std::path::PathBuf::new();
    for part in path.strip_prefix(root).unwrap_or(path).components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
                if matches!(
                    place.components().next_back(),
                    Some(std::path::Component::Normal(_))
                ) =>
            {
                place.pop();
            }
            other => place.push(other),
        }
    }
    let written = root.join(&place);
    if the_file_itself(&written) == the_file_itself(path) {
        return relative_to(root, &written);
    }
    path.parent()
        .and_then(|directory| std::fs::canonicalize(directory).ok())
        .zip(path.file_name())
        .map_or_else(
            || relative_to(root, path),
            |(directory, name)| relative_to(&the_file_itself(root), &directory.join(name)),
        )
}

/// What the judgment reads a file as, from the place that reached it: one
/// of CI's own files, and a justfile.
///
/// **One answer, for the walk that joins a file and the loop that judges
/// it**, so the two cannot disagree about which spellings are one reading.
/// Both halves are read from where [`the_place_of`] says the spelling puts
/// the file: a file under `.github/` is CI's, where no command may be offline,
/// and a file named as one of [`JUSTFILE`] has recipes, whose names are held
/// to what their bodies run.
fn the_role_of(relative: &str) -> (bool, bool) {
    let basename = relative.rsplit('/').next().unwrap_or(relative);
    (
        relative.starts_with(WORKFLOWS),
        answers_to(basename, &JUSTFILE),
    )
}

/// Adds to `files` each path in `reached` that is a reading it does not hold
/// yet, and says whether any was.
///
/// **A reading is a file in the role its place gives it.** By
/// [`the_file_itself`], a file reached under a second spelling is not read
/// twice, and its commands not counted twice. By [`the_role_of`] as well,
/// because the judgment reads two roles from where a spelling puts a file and
/// the `.github/` walk runs first: compared by the file alone, a root
/// `justfile` that is also a file under `.github/` through a link, in either
/// direction, is judged as a workflow and nothing else, and no recipe of it
/// is held to its name. Two spellings are one reading when they name one file
/// in one role, so two names of one justfile on a filesystem that ignores
/// case are read once, and by [`the_place_of`] so is a step into a directory
/// and back out, into `.github/` or out of it as well. The spelling kept is
/// the first that reached the reading.
fn joined(
    root: &Path,
    files: &mut Vec<std::path::PathBuf>,
    reached: impl IntoIterator<Item = std::path::PathBuf>,
) -> bool {
    let reading = |path: &Path| {
        (
            the_file_itself(path),
            the_role_of(&the_place_of(root, path)),
        )
    };
    let mut known: Vec<_> = files.iter().map(|path| reading(path)).collect();
    let mut grew = false;
    for path in reached {
        let it = reading(&path);
        if !known.contains(&it) {
            known.push(it);
            files.push(path);
            grew = true;
        }
    }
    grew
}

/// Whether `name` is one of `names`, with case set aside.
///
/// What [`the_files_answering_to`] reads a directory by, and what tells
/// [`the_role_of`] that a file is a justfile.
fn answers_to(name: &str, names: &[&str]) -> bool {
    names.iter().any(|known| known.eq_ignore_ascii_case(name))
}

/// The files in `directory` that answer to one of `names`, by the names they
/// have.
///
/// **Listed, not joined.** `root.join("JUSTFILE").is_file()` asks the
/// filesystem, and one that ignores case says yes to every spelling of a file
/// that is there under one of them, so a walk that joins each name reads one
/// justfile three times on macOS and Windows. A name read from the directory
/// is the name the file has, so it is read once, and named in a finding as it
/// is named on disk.
///
/// **Compared without case, on every system.** `just` compares so on its
/// own, so a justfile renamed `JuStFiLe` is still the surface a developer
/// types on Linux, measured against `just 1.55.1`; and a name that differs
/// from one of make's only in case is that name on the filesystems macOS and
/// Windows check out onto. A file one of the two would load on any system is
/// read on every system, so all three read the same files.
fn the_files_answering_to(directory: &Path, names: &[&str]) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut found: Vec<std::path::PathBuf> = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| answers_to(name, names))
        })
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    found.sort();
    found
}

/// `named` under `base`, when every segment before its last is a directory.
///
/// **As POSIX resolves a path, on every system.** Windows normalizes a path
/// before it asks the filesystem anything, striking a name and the `..` after
/// it out whether or not the name exists, so `cd "$(dirname "$0")/.."` in
/// `xtask/leaves-nothing.sh` names the root there — a name that is no
/// directory, then a step back from it — and a walk that joins a word whole
/// resolves every word a second time against it. On one tree such a walk
/// prints `ok 41 cargo command(s)` on Windows and `31` on macOS; by the count,
/// the ten are the two scripts and the justfile the words reach, read once
/// more through that second root, which is inferred and not measured apart.
/// POSIX's `cd` refuses such a path, as bash says with
/// `cd: missing/../ci: No such file or directory`, and Linux and macOS refuse
/// it on their own; asking for each segment before stepping past it makes the
/// three systems reach the same directories and the same files. What the last
/// segment must be is the caller's question.
fn under(base: &Path, named: &str) -> Option<std::path::PathBuf> {
    let mut reached = base.to_path_buf();
    for segment in Path::new(named).components() {
        if !reached.is_dir() {
            return None;
        }
        reached.push(segment);
    }
    Some(reached)
}

/// Adds every file `files` reaches, until nothing new appears.
///
/// Called twice: once for what the workflows reach, and again after the
/// justfiles join them. **The second call is the point** — a loop run only
/// before the justfile is added reads the justfile and nothing it reaches. A
/// recipe shelling out to a script, or `import`ing the file its recipes moved
/// to, then puts a whole second gate one line beyond the boundary with the
/// cargo count unchanged.
///
/// **One budget between the two calls, not one each.** With a fresh [`HOPS`]
/// per call the walk really reaches thirty-two and the finding beside it fires
/// whenever *either* call runs out — measured with a chain of twenty scripts,
/// where the whole chain is read, every cargo invocation in it is judged, and
/// the run still says `what CI reaches is more than 16 hops deep`. A gate that
/// is wrong in the direction of a false finding teaches a reader to ignore it,
/// which is what `counts.rs`'s header exists to say. The budget is spent across
/// both calls, so the bound the document states is the bound the walk has,
/// and `ran_out` means it stopped short.
fn follow_what_they_reach(
    root: &Path,
    files: &mut Vec<std::path::PathBuf>,
    left: &mut usize,
) -> bool {
    // **A hop is a step that found something.** Spending the budget on every
    // pass of this loop, including the one that finds nothing and ends it,
    // returns the first call — which always ends that way — with fifteen of
    // sixteen left, and a chain of fifteen reached through the justfile then
    // reports `more than 16 hops deep` with the whole chain read. Measured
    // through `everything_ci_reaches` rather than through this function:
    // `depth=15 chain_scripts_read=15 ran_out=true` — the false finding the
    // shared budget exists to prevent, one off.
    while *left > 0 {
        if !joined(root, files, reached_from(root, files)) {
            return false;
        }
        *left -= 1;
    }
    // **Running out means the budget is spent *and there is more to find*.**
    // Spending the last hop is not the same as stopping short: a chain of
    // exactly [`HOPS`] is read whole and leaves nothing behind, and reporting it
    // as too deep is the same false finding the shared budget and the hop rule
    // prevent, one step further along. So the question is asked rather than
    // inferred from the counter — by [`joined`] itself, on a copy, so "more to
    // find" means a reading it would add.
    joined(root, &mut files.clone(), reached_from(root, files))
}

/// Every file in the tree one word of a CI file could name.
///
/// Apart from [`reached_from`] because understanding the word, rather than
/// joining it to a path, takes that function past a hundred lines: a workflow
/// writes the checkout root as a variable, and a `for` loop names its scripts
/// with a glob.
fn what_this_word_names(
    root: &Path,
    word: &str,
    directories: &[std::path::PathBuf],
) -> Vec<std::path::PathBuf> {
    // **The checkout root, however CI spells it.** `$GITHUB_WORKSPACE` is *the*
    // idiomatic way to name it in a workflow and `${{ github.workspace }}` is
    // the expression form — a step running `bash "$GITHUB_WORKSPACE/ci/lint.sh"`
    // reaches a whole second gate that a walk reading the word literally does
    // not, with the count unchanged. Both mean the directory being walked.
    let named = word
        .replace("${{ github.workspace }}", ".")
        .replace("$GITHUB_WORKSPACE", ".")
        .replace("${GITHUB_WORKSPACE}", ".");
    // Shell punctuation is not part of a path: a `for` loop writes `ci/*.sh;`,
    // and the trailing semicolon makes the glob below match nothing.
    let named = named
        .trim_start_matches("./")
        .trim_end_matches([';', '&', '|', ')', ',']);

    let mut found = Vec::new();
    // **And relative to any `working-directory:` the file names.** A step can
    // say `working-directory: scripts` and then `run: bash gate.sh`, and
    // resolving `gate.sh` against the repository root finds nothing — so a whole
    // second gate runs `cargo fmt`, `cargo clippy` and `cargo test --workspace`
    // with the count unchanged. `cd scripts && bash gate.sh` is the same
    // hole.
    //
    // Every such directory in the file, not the one in scope: this reads line by
    // line and a step's keys can come in any order, so scoping would mean
    // parsing YAML. Trying more directories can only find more of what CI runs,
    // never less — and a file found this way is one CI can reach.
    for base in std::iter::once(root.to_path_buf()).chain(directories.iter().cloned()) {
        // **A glob names files without naming one.** `for s in ci/*.sh; do bash
        // "$s"; done` runs every script in a directory and no word in it is a
        // file. Expanded against the tree, because what CI runs is what the glob
        // matches when it runs.
        if named.contains('*') {
            found.extend(matching(&base, named));
            continue;
        }
        if let Some(candidate) = under(&base, named).filter(|path| path.is_file()) {
            found.push(candidate);
        }
    }
    // **A file a step *names* is not a file a step *runs*.** Following every
    // word that resolves to a file in the tree and reading it as shell lets one
    // ordinary step — `run: wc -l crates/nc-ids/src/lib.rs`, or a determinism job
    // diffing a named source — pull a Rust file into the CI walk and report
    // eight findings against correct code: `if text.is_empty() || …` read as a
    // step whose commands are conditional, and a `//` comment read as a
    // swallowed failure, because `the_code_before_a_comment` knows only `#`.
    // Measured, all eight against `crates/nc-ids/src/lib.rs`, and a reader sent
    // to fix a file with nothing wrong in it.
    //
    // So this follows what it can read as commands. A file in another language
    // is not followed at all rather than misread: a second gate written in
    // Python is not findable here — its `cargo` sits inside quotes and commas
    // this reader does not see — and misparsing it buys false findings and no
    // catches. If one appears, this list is where it is added, with the reader
    // that can read it.
    found.retain(|path| reads_as_commands(path));
    found
}

/// Whether a file is one this walk can read as a list of commands.
///
/// A shell script, a `just` or `make` file, or a workflow — and a file with no
/// extension at all, which is what `justfile` and `Makefile` are, and what a
/// script committed without one is.
fn reads_as_commands(path: &Path) -> bool {
    const AS_COMMANDS: [&str; 8] = ["sh", "bash", "zsh", "just", "yml", "yaml", "mk", "make"];
    path.extension()
        .and_then(|kind| kind.to_str())
        .is_none_or(|kind| AS_COMMANDS.contains(&kind))
}

/// Every file in the tree a shell glob would match.
///
/// Only `*`, and only within one path segment — which is what a workflow's
/// `ci/*.sh` is. A `**` would need the walk to descend, and no shape this has
/// met uses one; if one appears, it is a finding waiting to be written rather
/// than a silent miss, because the segments around it still have to match.
fn matching(base: &Path, pattern: &str) -> Vec<std::path::PathBuf> {
    let Some((directory, name)) = pattern.rsplit_once('/') else {
        return Vec::new();
    };
    if directory.contains('*') {
        return Vec::new();
    }
    let (before, after) = name.split_once('*').unwrap_or((name, ""));
    let mut found = Vec::new();
    let Some(Ok(entries)) = under(base, directory).map(std::fs::read_dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let entry_name = entry.file_name();
        let Some(entry_name) = entry_name.to_str() else {
            continue;
        };
        if entry_name.starts_with(before)
            && entry_name.ends_with(after)
            && entry_name.len() >= before.len() + after.len()
            && entry.path().is_file()
        {
            found.push(entry.path());
        }
    }
    found.sort();
    found
}

/// Every file the workflows reach outside `.github/`.
///
/// A script a step runs and a local composite action a step `uses:` are both
/// part of what CI runs, and both may live anywhere in the tree — so walking
/// `.github/` alone lets a shell script beside the sources and a composite
/// action in a directory of its own through.
/// CI's boundary is what its steps reach.
fn reached_from(root: &Path, walked: &[std::path::PathBuf]) -> Vec<std::path::PathBuf> {
    // Every directory a `working-directory:` or a `cd` names, gathered first so
    // a script is resolved against them as well as against the root.
    let mut directories: Vec<std::path::PathBuf> = Vec::new();
    for path in walked {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines() {
            let code = the_code_before_a_comment(line);
            let code = code.as_str();
            let named = code
                .strip_prefix("working-directory:")
                .map(str::trim)
                .or_else(|| {
                    code.strip_prefix("cd ")
                        .or_else(|| code.strip_prefix("- run: cd "))
                        .and_then(|rest| rest.split("&&").next())
                        .map(str::trim)
                });
            if let Some(named) = named {
                let named = named.trim_matches(['"', '\'']);
                if !named.is_empty()
                    && let Some(directory) = under(root, named).filter(|path| path.is_dir())
                {
                    directories.push(directory);
                }
            }
        }
    }
    directories.sort();
    directories.dedup();
    let mut reached = Vec::new();
    for path in walked {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines() {
            let trimmed = line.trim().trim_start_matches("- ");
            // `uses: ./ci/actions/extra` — a directory holding an action file.
            if let Some(local) = trimmed.strip_prefix("uses:").map(str::trim)
                && local.starts_with("./")
            {
                for name in ["action.yml", "action.yaml"] {
                    let named = format!("{}/{name}", local.trim_start_matches("./"));
                    reached.extend(under(root, &named).filter(|path| path.is_file()));
                }
            }
            // **A task runner reaches its own file by name, not by path.**
            // `just lint` names no file, and the `justfile` is outside
            // `.github/` — so without this CI can run a whole second gate
            // (`cargo fmt --check`, `cargo clippy`, `cargo test`) through one
            // committed recipe with the count unchanged and
            // `check passed`. That is a fifth shape past `§19.4`, and it uses a
            // file this repository ships whose own header says every
            // recipe is a `cargo xtask` verb underneath.
            for (command, names) in [("just", JUSTFILE.as_slice()), ("make", MAKEFILE.as_slice())] {
                if trimmed
                    .split_whitespace()
                    .any(|word| word.trim_matches(['"', '\'']) == command)
                {
                    reached.extend(the_files_answering_to(root, names));
                }
            }
            // **A `just` module names a file the word does not.** `just` loads
            // a module from one of four names beside the justfile:
            //
            //     extra.just  extra/mod.just  extra/justfile  extra/.justfile
            //     allow-reference: names `just` looks for, none of them a file here
            //
            // and the walk looks for words that *are* files —
            // `extra` is not one, so without this the module is never opened.
            // Measured: the three recipes that go red inline (`cargo fmt --all`,
            // `cargo clippy --workspace`, `cargo test --workspace`) moved into
            // `extra.just` behind one `mod extra` line leave the count unchanged
            // and `check passed`, with `just --show extra::lint`
            // resolving the recipe.
            //
            // The same shape as the four the header lists, in the file the
            // `JUSTFILE` constant exists to find.
            if let Some(module) = trimmed.strip_prefix("mod ") {
                // `mod name`, `mod name 'path'` and `mod? name` all name the
                // module first; a quoted path after it is where to look.
                let mut words = module.split_whitespace();
                if let Some(name) = words.next().map(|word| word.trim_matches(['?', '"', '\''])) {
                    let given = words
                        .next()
                        .map(|word| word.trim_matches(['"', '\'']).to_owned());
                    let candidates = given.into_iter().chain([
                        format!("{name}.just"),
                        format!("{name}/mod.just"),
                        format!("{name}/justfile"),
                        format!("{name}/.justfile"),
                    ]);
                    for candidate in candidates {
                        reached.extend(under(root, &candidate).filter(|path| path.is_file()));
                    }
                }
            }
            // Any word that names a file in this tree, which is what a
            // `run: bash scripts/…` reaches.
            // **And the right-hand side of an assignment.** `assigns_cargo`
            // catches `C=cargo` then `$C clippy`; the same shape one word along
            // — a variable assigned a script's path, then run through it — names
            // no file to this scan, because the assignment is one word. Splitting on `=` finds
            // the path, and a path this scan finds is a file CI can reach.
            // **The checkout root before the line is cut into words**, because
            // `${{ github.workspace }}/ci/lint.sh` is four words to
            // `split_whitespace` and one path to CI.
            let trimmed = &trimmed
                .replace("${{ github.workspace }}", ".")
                .replace("${{github.workspace}}", ".");
            for word in trimmed.split_whitespace().flat_map(|word| word.split('=')) {
                let named = word.trim_matches(['"', '\'']);
                if named.starts_with('-') {
                    continue;
                }
                reached.extend(what_this_word_names(root, named, &directories));
            }
        }
    }
    reached
}

/// CI declares a service for every stack `cargo xtask check` requires.
///
/// **The gate that keeps CI on one command has to see whether CI can run
/// it.** This file asserts what the workflow's cargo commands are and what its
/// jobs are called, and this asserts that the runner is given the things
/// those commands need. Measured in exactly the state a gate job produces
/// — a bare checkout, a fresh unmigrated PostgreSQL service, no log store:
///
/// ```text
/// error the log store is not accepting connections at 127.0.0.1:9428:
///       Connection refused (os error 111)
///       `cargo xtask dev` brings the log store up
/// error check failed: the log store did not report its bounds
/// EXIT=1
/// ```
///
/// Every other step in that state is `ok` or an announced skip; the store is
/// the single blocker. And the fix the gate prints is then a command such a job
/// cannot run: its PostgreSQL is a *service container*, not the compose stack,
/// so `cargo xtask dev` has nothing to bring up.
///
/// **The rule is that CI brings that stack up, and not that it declares one of
/// its own.** A `services:` block naming every image `docker-compose.yml`
/// names, at the same digest, is refused: adding one gets two findings,
/// `CI names … itself instead of bringing the stack up`, measured.
///
/// **The rule is stated in the half a reader is shown**, which is this block
/// and not a `//` comment rustdoc does not publish. Why a service container
/// cannot stand in for the stack — it takes no command, so the log store loses
/// its retention policy, nothing migrates its database, and the health check
/// authenticates by `trust` — is in the body, where the measurement that
/// establishes it belongs.
fn ci_supplies_what_the_gate_needs(root: &Path, ci: &str, findings: &mut Vec<Finding>) {
    let Ok(compose) = std::fs::read_to_string(root.join("docker-compose.yml")) else {
        return;
    };
    // **One reader of `image:`, not a fourth copy of `strip_prefix("image:")`.**
    // One space before the colon is a key docker resolves and such a copy
    // cannot read — measured, with the stack's own gate reading it and a copy
    // here reporting `the stack declares 1 image(s), and this rule is written
    // against a stack of two`. `crate::gates::the_image_on` is the one spelling.
    let images = |text: &str| -> Vec<String> {
        text.lines()
            .filter_map(crate::gates::the_image_on)
            .map(|image| image.trim().trim_matches(['"', '\'']).to_owned())
            .collect()
    };
    let wanted = images(&compose);
    // The floor, on the side that can silently empty: a compose file this
    // reader could not parse would demand nothing of CI and pass.
    if wanted.len() < 2 {
        findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: 1,
            what: format!(
                "the stack declares {} image(s), and this rule is written against a stack \
                 of two",
                wanted.len()
            ),
            fix: "the comparison below is between what `docker-compose.yml` runs and what \
                  CI brings up, so a compose file that reads as empty asks nothing of CI \
                  and passes. Read the stack again"
                .to_owned(),
        });
        return;
    }
    // **A service container is not the stack.** A rule comparing `image:`
    // strings — every image the compose file names under CI's `services:`, at
    // the same digest, with "declare it under `services:`" as its fix — is green
    // about a workflow that cannot pass: a service container takes an image, an
    // environment, ports and `docker create` options, and *not* a command,
    // while `docker-compose.yml` hands the log store its whole retention policy
    // as one:
    //
    // ```text
    // docker run … victoria-logs   ->  GET /flags: 200, Content-Length: 0
    // docker compose up logs       ->  -retentionPeriod="30d" …
    //
    // error the log store answered `/flags` with nothing
    // error check failed: the log store reported no flags
    // ```
    //
    // Nothing migrates its database either, and a health command copied from
    // compose authenticates by `trust`. Three findings, one cause: a second
    // stack built by hand beside the one the compose file defines.
    //
    // So the rule is the simple one. CI brings *that* stack up, and the
    // reader below asks only whether it does.
    // **Every image CI names, not the ones spelled exactly as compose spells
    // them.** Intersecting with `wanted` misses the ordinary case:
    // `docker-compose.yml` pins `postgres:18-alpine@sha256:…` while a
    // hand-written `services:` block names the *tag*, and the digest is the one
    // spelling nobody writing a service container would use. Measured: under an
    // intersection, a `services:` block with `image: postgres:18-alpine` leaves
    // every other test in this module green; the same block with the digest is
    // named. The rule the README and `ci.yml`'s own comment state is that
    // **no image is named in the workflow**, and `wanted.len() < 2` above
    // already covers a compose file that reads as empty.
    let named_by_ci = images(ci);
    for image in &named_by_ci {
        findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: where_it_says(ci, image),
            what: format!("CI names `{image}` itself instead of bringing the stack up"),
            fix: "an image named here is a second definition of a service `docker-compose.\
                  yml` already defines, and CI cannot reproduce what that file gives it — a \
                  service container has no command, so the log store loses the retention \
                  flags the gate reads back, and nothing applies the migrations. Delete it \
                  and let `cargo xtask dev` bring up the stack"
                .to_owned(),
        });
    }
    no_stack_where_none_runs(ci, findings);
    // And the step itself, in the job whose subject is the whole gate: without
    // it the gate has no database and no store, and every step that reads one
    // fails.
    let Some(job) = job_body(ci, "linux") else {
        findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: 1,
            what: "there is no `linux` job to bring the stack up in".to_owned(),
            fix: "`JOBS` says what this workflow has to contain; the rule here is about \
                  what that job does before it runs the gate"
                .to_owned(),
        });
        return;
    };
    // **Commands, not the body's text.** The block above this job in `ci.yml`
    // explains at length why `cargo xtask dev` is there, and a reader that
    // searched the body would be satisfied by that explanation — which is the
    // shape `commands_in`'s own doc records: a job's prose satisfying the
    // requirement it explains.
    let ran = commands_in(&job);
    let brings_it_up = ran.iter().position(|line| line.contains("xtask dev"));
    let runs_the_gate = ran.iter().position(|line| line.contains("xtask check"));
    match (brings_it_up, runs_the_gate) {
        (Some(up), Some(gate)) if up < gate => {}
        (None, _) => findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: where_it_says(ci, "cargo xtask check"),
            what: "the `linux` job runs the gate without bringing the stack up".to_owned(),
            fix: "`cargo xtask check` asks a running database what it holds and a running \
                  log store what it was started with, and neither is a step `--offline` \
                  skips. Add a `cargo xtask dev` step before it"
                .to_owned(),
        }),
        (Some(_), None) => findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: where_it_says(ci, "cargo xtask dev"),
            what: "the `linux` job brings the stack up and never runs the gate".to_owned(),
            fix: "the stack is what the gate needs, not a thing to start for its own sake"
                .to_owned(),
        }),
        (Some(_), Some(_)) => findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: where_it_says(ci, "cargo xtask dev"),
            what: "the `linux` job brings the stack up after running the gate".to_owned(),
            fix: "the gate reads the database and the store while it runs, so a stack \
                  started afterwards is a stack it never saw"
                .to_owned(),
        }),
    }
}

/// The macOS and Windows jobs bring no stack up.
///
/// **And no stack where none runs.** Those jobs run the gate without one, by
/// design: the steps that need it say they are not applicable there, and a
/// `cargo xtask dev` step in one of those jobs is a step that cannot succeed on
/// that runner — or, on one that happened to have docker, a stack the gate
/// there never reads. Its own function so `ci_supplies_what_the_gate_needs`
/// stays inside the line limit.
fn no_stack_where_none_runs(ci: &str, findings: &mut Vec<Finding>) {
    for system in WITHOUT_THE_STACK {
        let Some(job) = job_body(ci, system) else {
            continue;
        };
        if commands_in(&job)
            .iter()
            .any(|line| line.contains("xtask dev"))
        {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: where_it_says(ci, &format!("{system}:")),
                what: format!("the `{system}` job brings the stack up, and only Linux runs one"),
                fix: "the stack runs on Linux, where the `linux` job brings it up and the gate \
                      reads it (NOCTUA.md §22.2); on macOS and Windows the gate says which of \
                      its steps are not applicable and runs the rest. Remove the \
                      `cargo xtask dev` step from this job"
                    .to_owned(),
            });
        }
    }
}

/// The workflow fires, and no job is switched off in place.
///
/// Apart from `jobs_are_what_they_are_named` because the two together pass a
/// hundred lines, and because it is a different question: everything else
/// there reads what a job *contains*, and this reads whether it is ever asked
/// to.
fn the_workflow_actually_runs(ci: &str, findings: &mut Vec<Finding>) {
    // **A workflow can be alive and decide nothing, seven ways.** Each of these
    // was measured printing `ok 4 cargo command(s) …` and `check passed`:
    //
    // ```text
    // run: cargo xtask check || true          the verdict is discarded
    // continue-on-error: true                 the job fails and the run is green
    // on: push: branches: [does-not-exist]    it fires on nothing
    // paths-ignore: ['**']                    likewise
    // if: github.event_name == 'never'        a condition that is never true
    // exit 0                                  at the head of the determinism body
    // if false; then … fi                     around the determinism body
    // ```
    //
    // Closing a spelling — literal `if: false`, or a heredoc — is not closing a
    // class, and this is the class: **anything that makes a step's verdict not
    // count.**

    // **It fires on a push and on a pull request, unconditionally.** Asking
    // whether a line reads `push:` says nothing about `branches:` or
    // `paths-ignore:` under it, and a filter matching nothing is a workflow that
    // never runs.
    //
    // **Any key at all, not four names.** A list of `branches`,
    // `branches-ignore`, `paths` and `paths-ignore` misses the others GitHub
    // offers. Measured, with the rest of the file untouched:
    //
    // ```text
    // on: push: tags: ['v*']              -> check passed
    //     pull_request: types: [labeled]  -> check passed
    // ```
    //
    // A repository whose tags are cut rarely then runs the gate rarely, and a
    // `types:` list narrows a pull request to one event nobody triggers. An
    // unconditional `push:` has nothing under it, so what this asks is that:
    // the body is empty. That answers for every filter GitHub has and every one
    // it adds later, which a list of names cannot.
    for event in ["push", "pull_request"] {
        let Some(body) = event_body(ci, event) else {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: 1,
                what: format!("the workflow does not run on `{event}`"),
                fix: "a workflow that fires on nothing is a gate that never runs, and \
                      every check in this file reads what the jobs contain rather than \
                      whether they are asked to. CI and a developer run one gate \
                      (NOCTUA.md §19.4), which needs CI to run"
                    .to_owned(),
            });
            continue;
        };
        if let Some(filter) = body
            .lines()
            .map(the_code_before_a_comment)
            .find(|line| !line.is_empty())
        {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: 1,
                what: format!("`{event}` is filtered by `{filter}`"),
                fix: "a filter decides which pushes run the gate, and this gate's whole \
                      point is that every one of them does — a branch list or a path \
                      filter is how CI comes to be green on the change that broke it. \
                      Run on every push and every pull request"
                    .to_owned(),
            });
        }
    }

    // **Any condition, not only the literal false.**
    // `if: github.event_name == 'never-happens'` never holds either, and a gate
    // cannot evaluate an expression — so what it can say is that a job carrying
    // a condition is a job that might not run, which is the thing this file
    // exists to prevent.
    for (index, line) in ci.lines().enumerate() {
        // **The list marker first, the way every other reader in this file does
        // it.** `is_label`, `commands_in`, `reached_from` and
        // `runs_cargo_as_an_action` all strip `- ` before matching, and without
        // it `- if: false` — the *step*-level spelling, which is what a step
        // condition looks like in YAML — matches nothing at all. Measured,
        // on the step that runs the gate:
        //
        // ```text
        //   - if: false
        //     name: cargo xtask check
        //     run: cargo xtask check
        //
        // cargo xtask check
        //   ok  10 cargo command(s) in CI's files and the justfile, each one the gate…
        // check passed
        // ```
        //
        // The whole gate skipped in CI, every run green, and the count unmoved
        // because `commands_in` still reads the untouched `run:` line. The
        // quoted key is stripped for the same reason: `"if": false` is valid
        // YAML and names the same key.
        let code = the_code_before_a_comment(line);
        let code = code.as_str();
        let code = code.trim_start_matches("- ").trim();
        let Some(condition) = code
            .strip_prefix("if:")
            .or_else(|| code.strip_prefix("\"if\":"))
            .or_else(|| code.strip_prefix("'if':"))
        else {
            continue;
        };
        let condition = condition.trim().trim_matches(['\'', '"']);
        if condition.is_empty() {
            continue;
        }
        findings.push(Finding {
            file: WORKFLOW.to_owned(),
            line: index + 1,
            what: format!("this step or job runs only when `{condition}`"),
            fix: "a required job that runs conditionally is a required job that may not \
                  run, and every check here reads its name and its body rather than \
                  whether it was reached. `if: false` was the spelling closed first; \
                  a comparison against an event name is the same thing said in a way no \
                  gate can evaluate. Remove the condition, or remove the job so its \
                  absence is a finding"
                .to_owned(),
        });
    }
}

/// Shapes that leave a step alive and its verdict uncounted.
///
/// Each was measured leaving `check passed` with the cargo count unchanged.
/// The list is of *shapes*, not of spellings: closing `if: false` or a heredoc
/// on its own does not close the class — which is **anything that makes a
/// step's verdict not count**.
const NEUTERED: [(&str, &str, &str); 8] = [
    (
        "||",
        "a command's failure is swallowed by what follows `||`",
        "the gate's verdict is the point of running it, and `||` throws it away \
         while every check in this file still sees the command. This read `|| true`, \
         and `|| :` is the same line with the shell's other no-op — so what it asks \
         now is that a command CI runs has no alternative at all",
    ),
    (
        "shell:",
        "a step chooses its own shell, which decides whether a failure counts",
        "GitHub runs `run:` under `bash -e` so that any failing command fails the step, \
         and `shell: bash {0}` — the documented way to change it — drops the `-e`, after \
         which only the *last* command's status is read. Every entry in this list assumes \
         the default. If a step needs another shell, it needs a reason next to it and a \
         rule here",
    ),
    (
        "set +o",
        "a shell block turns off one of its own safety options with `set +o`",
        "`set +o errexit` is `set +e` written the long way and `set +o pipefail` is the \
         same hole one flag along — after either, a command can fail and the step still \
         exits zero. This list is of shapes rather than spellings, and both spellings \
         are the same shape",
    ),
    (
        "set +e",
        "a shell block turns off its own failure handling with `set +e`",
        "every command after it can fail and the step still exits zero, which is the \
         same as not running them — and each one is still read as a command here",
    ),
    (
        "continue-on-error",
        "a job or step is marked `continue-on-error`",
        "the job then fails and the run is still green, which is the same as not running \
         it — and nothing here would notice, because the body is unchanged",
    ),
    (
        "exit 0",
        "a step ends early with `exit 0`",
        "every command after it is text, and this file reads commands. If the step is \
         not wanted, delete it, so its absence is a finding",
    ),
    (
        "if false",
        "a shell block is switched off by `if false`",
        "the commands inside are never run and are still read as commands here — the \
         same defect as wrapping them in a heredoc, one syntax along",
    ),
    (
        "if true; then :",
        "a shell block is replaced by a no-op",
        "the commands are gone and the step still exits zero",
    ),
];

/// No command in `file` has its verdict thrown away.
///
/// **Not only in `ci.yml`.** The walk above it reads the `justfile` and every
/// script a step reaches, and asking this of the workflow alone lets the
/// surface a developer actually types swallow the gate's answer with the cargo
/// count unchanged. Measured, with the workflow untouched:
///
/// ```text
/// justfile   check:  cargo xtask check || true
///            -> ok 10 cargo command(s) in the workflow, and check passed
/// ```
///
/// against a file whose own header says *"`just` is never the only path"*. A
/// verdict is discarded the same way wherever it is written, so this asks it of
/// every file that walk found.
fn no_verdict_is_discarded(file: &str, text: &str, findings: &mut Vec<Finding>) {
    for (index, line) in text.lines().enumerate() {
        let code = the_code_before_a_comment(line);
        let code = code.as_str();
        // A step named after what it runs is prose. `is_label` is what the rest
        // of this file uses, and two halves disagreeing about whether a
        // `name:` is a command is a defect of its own.
        if is_label(code) {
            continue;
        }
        // **A command sent to the background is a command nobody waits for.**
        // `run: cargo xtask check &` exits 0 at once under `bash -e` and the
        // child is killed with the step — the verdict never existed. `&&` is
        // not this, which is why it is read as a trailing token rather than as
        // a substring.
        if the_line_backgrounds_a_command(code) {
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: "a command is sent to the background with `&`".to_owned(),
                fix: "the step then exits before it has finished and the runner kills it, \
                      so its verdict never exists — while every check here still reads it \
                      as a command that ran"
                    .to_owned(),
            });
        }
        // **And the class those needles are eight spellings of.** `if false` and
        // `if true; then :` are needles, and needles close nothing: against
        // those two alone, `while false; do … done`,
        // `for _ in ${NOTHING:-}; do … done` and `if ! true; then … fi` each
        // wrap the same commands and each leave `cargo xtask check`
        // byte-identical to a clean run, with the
        // determinism job — the only cross-process determinism check this
        // repository has — running nothing at all. A shell has five words that
        // can decide whether a command runs, and the grammar closes that list
        // where a list of spellings cannot.
        // **The words without their trailing space, and one comparison.** With
        // every entry ending in exactly one space,
        // `format!("{first} ") == *word || first == word.trim_end()` is the same
        // predicate written twice, either half deletable with nothing anywhere
        // deciding — and since `workflow.rs` is in `xtask/mutants.py`, a
        // `||`→`&&` mutation there is an equivalent mutant, for which the runner
        // records a kill of a clause nothing decides.
        for word in ["if", "while", "until", "for", "case"] {
            if code.split_whitespace().next() != Some(word) {
                continue;
            }
            // **And only when it wraps a command this gate requires.** A rule
            // reporting any control flow at the head of a line refuses `ci.yml`
            // itself, which opens one to delete a scratch directory before the
            // build — a guard around a cleanup, with `cargo xtask build` on
            // the next line, outside it. Refusing that is the rule being wrong
            // in the direction that costs a reader time and teaches everyone
            // to stop reading it. What the rule is about is a *verdict* whose
            // running is decided at runtime, so it asks what the construct
            // holds, up to the word that closes it.
            if !wraps_a_command(text, index, file == WORKFLOW) {
                continue;
            }
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: format!(
                    "a step's commands are inside `{word}`, so whether they run at all is \
                     decided at runtime"
                ),
                fix: "a check CI runs has to run. Every reader in this file sees the \
                      command and counts its verdict, and a condition or a loop around it \
                      means the verdict may be of nothing — measured, the determinism job \
                      wrapped in `while false; do … done` left every count in this gate \
                      unchanged. Write the commands at the top level of the step"
                    .to_owned(),
            });
        }
        let bare = without_quoted_spans(code);
        for (needle, what, fix) in NEUTERED {
            if !bare.contains(needle) {
                continue;
            }
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: (*what).to_owned(),
                fix: (*fix).to_owned(),
            });
        }
    }
}

/// Whether this line carries a command the gate requires CI to run.
///
/// **Every command this file requires, not the ones that begin `cargo`.** To a
/// walk asking `cargo_commands` alone, a construct around the determinism
/// job's four `diff` lines — the whole verdict of the only cross-process
/// determinism check this repository has, and the four `diff` rows of
/// `JOBS_MUST_RUN` — holds no command.
/// `until true; do … done` around them leaves both builds running, the copy
/// made, every required line present for the readers that count text, and
/// nothing at all compared.
///
/// So the question is asked of the list that says what CI must run rather than
/// of a prefix: a row added to `JOBS_MUST_RUN` is guarded by this the moment it
/// is written, which is what a second list would not have given.
///
/// **And only in the file those rows are about.** `JOBS_MUST_RUN` names what
/// `ci.yml`'s two jobs run; the walk this gate does also reaches the justfile
/// and `xtask/the-checkout-shape.sh`, and that script runs `cpio -pdm` — one of
/// the rows — to make its scratch *copy*. A copy judges nothing, so asking the
/// rows of every file reports an ordinary guard around it as a verdict decided
/// at run time — measured, and the very false positive `wraps_a_command` exists
/// to stop, one file over. A cargo command is a verdict wherever it
/// is; a required *line* is a verdict only where it is required.
fn a_command_this_gate_requires(code: &str, the_workflow: bool) -> bool {
    !cargo_commands(code).is_empty()
        || (the_workflow
            && JOBS_MUST_RUN
                .iter()
                .any(|(_, command, _)| code.contains(command)))
}

/// Whether the control-flow construct opening at `at` holds a command the gate
/// requires.
///
/// The construct runs to the word that closes it — `fi`, `done` or `esac` — and
/// what matters is whether a cargo invocation is inside. A guard around a
/// cleanup is ordinary shell; a guard around `cargo xtask check` is the gate
/// running or not depending on the day.
///
/// Nesting is counted, so an inner `if` does not end an outer one. An unclosed
/// construct reaches the end of the text, which errs toward reporting: a step
/// whose control flow never closes is not something to pass over.
fn wraps_a_command(text: &str, at: usize, the_workflow: bool) -> bool {
    let mut depth = 0_usize;
    for line in text.lines().skip(at) {
        let code = the_code_before_a_comment(line);
        let code = code.as_str();
        // **Every opener and every closer on the line, not the first word.**
        // Reading only the first word means a construct opened *and* closed on
        // one line — `if [ "$a" = "$b" ]; then a=; else a=${a#*/}; fi`, which is
        // ordinary shell — counts its opener and never its closer, so the depth
        // never comes back down and every line after it reads as inside the
        // construct. Measured: one such line in `xtask/the-checkout-shape.sh`
        // makes a first-word reader report that the script's
        // `cargo xtask check`, forty lines and three closed constructs later,
        // is wrapped in a `while`. The same blindness reads `*) case $x in` as
        // no opener and `esac;;` as no closer, which is the other direction and
        // the one that lets a wrapped verdict through.
        //
        // Quoted spans are spent first, so a word inside a message is not a
        // construct — `echo "done"` closes nothing.
        let bare = without_quoted_spans(code);
        let mut opened = 0_usize;
        let mut shut = 0_usize;
        let words: Vec<&str> = bare.split_whitespace().collect();
        for (index, spelled) in words.iter().enumerate() {
            // A closer carries the punctuation of the construct it sits in:
            // `esac;;` inside a case arm, `fi;` before another command.
            let word = spelled.trim_end_matches([';', '&', '|', ')']);
            // **Where a statement may begin, and nowhere else.** Spending quoted
            // spans and then counting every bare `done` makes `echo first build
            // done` close the construct around it, so the walk answers
            // "nothing wrapped" before it reaches the cargo line, and nothing
            // reports a gate whose running is decided at run time.
            // Measured; the unquoted spelling is the other half of the
            // blindness the quoted one shows.
            let begins_a_statement = index.checked_sub(1).is_none_or(|earlier| {
                let before = words[earlier];
                before.ends_with(';')
                    || before.ends_with(')')
                    || matches!(before, "then" | "else" | "do" | "in" | "{" | "&&" | "||")
            });
            if !begins_a_statement {
                continue;
            }
            if matches!(word, "if" | "while" | "until" | "for" | "case") {
                opened += 1;
            }
            if matches!(word, "fi" | "done" | "esac") {
                shut += 1;
            }
        }
        depth += opened;
        if a_command_this_gate_requires(code, the_workflow) {
            return true;
        }
        depth = depth.saturating_sub(shut);
        if depth == 0 {
            return false;
        }
    }
    true
}

/// A line up to the `#` that opens a comment, with quoted text left alone.
///
/// **`split('#')` cuts at the first `#` anywhere.** A shell line may hold one
/// inside quotes — `echo "the #1 reason"` — and cutting there hands the rest of
/// the line back as nothing, so every rule below stops reading it: a needle
/// after such a `#` is invisible. Counting quote characters and testing the
/// parity does not fix it either, because an apostrophe in ordinary prose
/// (`echo "it's fine"`) makes the count odd. The state is what decides, so it is
/// tracked: a `#` opens a comment only outside a span, and a quote inside a span
/// of the other kind is text.
///
/// **And this is one rule, written once**, because a `#` opens a comment only
/// at the start of a word. `TAG=${GITHUB_REF#refs/tags/} && cargo clippy
/// --workspace` is ordinary CI shell, and a second copy of this reader that
/// truncates it at the `#` lets a second lints gate run with
/// `12 cargo command(s)` unmoved and `check passed`, while five rules that read
/// a line through this function see nothing after such a `#`: the
/// discarded-verdict rule, the `||` wrapper, the step filter, the `if:` reader
/// and the `working-directory` scan. `sed s#a#b#` is the same shape. Two
/// readers with a repair made to one is the defect this repository has
/// produced most: one rule written twice, with the second copy narrower.
///
/// The trimming is this caller's, because a line's indentation is not code.
fn the_code_before_a_comment(line: &str) -> String {
    the_command_without_its_comment(line).trim().to_owned()
}

/// The same line with the contents of quoted spans spent.
///
/// The needles below name shell that turns a verdict off, and a line may write
/// any of them *as data*: `echo "set +e"` says nothing about how the step runs.
/// The quotes are kept so the line still reads as a line; only what is between
/// them is spent.
fn without_quoted_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut inside: Option<char> = None;
    // **A backslash quotes the character after it.** Read without that,
    // `echo \"` opens a span that never closes, so everything after it on the
    // line becomes spaces — and every rule that reads this then searches a line
    // of blanks. Measured: `cargo xtask check || true` written after an escaped
    // quote in a workflow step leaves every other xtask test green, and through
    // the gate itself the step count goes *up*, the step prints `ok`, and the
    // `||` finding disappears. The eight needles
    // that say a verdict was thrown away, and the backgrounding `&` rule, are
    // all downstream of this function.
    //
    // What it must not do is swallow the character it quotes: a `\` inside a
    // span blanks its pair the way the span blanks everything else, and outside
    // one it is written through with what follows it, so a path like `a\b` is
    // still a path.
    //
    // **And inside single quotes a backslash quotes nothing.** `'a\\'` is a
    // backslash followed by the closing quote, so the span ends there — and
    // reading the quote as escaped leaves the span open and spends the rest of
    // the line as blanks. Measured: `docs: echo 'a\\' && cargo xtask docs ||
    // true` in the justfile then leaves the transcript byte-identical with the
    // cargo count unmoved. The other two readers name this one as their model,
    // so it carries the correction they carry.
    let mut escaped = false;
    for character in line.chars() {
        if escaped {
            escaped = false;
            out.push(if inside.is_some() { ' ' } else { character });
            continue;
        }
        if character == '\\' && inside != Some('\'') {
            escaped = true;
            out.push(if inside.is_some() { ' ' } else { character });
            continue;
        }
        match (inside, character) {
            (None, '\'' | '"') => {
                inside = Some(character);
                out.push(character);
            }
            (Some(open), _) if open == character => {
                inside = None;
                out.push(character);
            }
            (Some(_), _) => out.push(' '),
            (None, _) => out.push(character),
        }
    }
    out
}

/// Whether any command on this line is sent to the background.
///
/// **Not only the last one.** Splitting the line on `;` and `|` and asking
/// whether a segment *ends* with `&` reads `cargo xtask check --offline & echo
/// started` as one segment ending in `started`, and the gate says nothing: the
/// shell runs the gate in the background, the recipe exits 0 at once, and the
/// verdict never exists. The corpus names this shape — "`&` — the gate runs in
/// the background and nobody waits" — and the trailing spelling is only one of
/// its forms.
///
/// A quoted `&` is text, `&&` is two commands in sequence, and `2>&1` and `&>`
/// are redirections. What is left is a `&` that ends a command.
fn the_line_backgrounds_a_command(code: &str) -> bool {
    let spent = without_quoted_spans(code);
    let characters: Vec<char> = spent.chars().collect();
    characters.iter().enumerate().any(|(at, character)| {
        *character == '&'
            && characters.get(at + 1) != Some(&'&')
            && at
                .checked_sub(1)
                .is_none_or(|before| characters[before] != '&')
            && at
                .checked_sub(1)
                .is_none_or(|before| characters[before] != '>')
            // **A redirection puts its `<` or `>` *before* the `&`.** Excusing
            // every `&` followed by a digit, for `<&3` — which the clause below
            // covers — also excuses `&2>/dev/null`, which is not a redirection
            // at all: the `&` backgrounds the command and the `2>` belongs to
            // what follows. Measured under that excuse, `run: cargo xtask check
            // &2>/dev/null` leaves `ok 12 cargo command(s)` unmoved and
            // `check passed`, while `bash -ec` returns before the child has
            // run.
            && at
                .checked_sub(1)
                .is_none_or(|before| characters[before] != '<')
            && characters.get(at + 1).is_none_or(|after| *after != '>')
    })
}

/// The lines under one `on:` event, or `None` when the workflow has no such event.
fn event_body(ci: &str, event: &str) -> Option<String> {
    let opening = format!("{event}:");
    // **And the flow spelling.** `on: [push, pull_request]` is the same list as
    // the block form, and reading only the block form reports a workflow that
    // runs on both as running on neither — a false refusal about the one line
    // that decides whether CI runs at all.
    if let Some(flow) = ci.lines().find_map(|line| {
        let line = line.trim();
        line.strip_prefix("on:")
            .map(str::trim)
            .filter(|rest| rest.starts_with('['))
    }) {
        let named = flow
            .trim_matches(['[', ']'].as_slice())
            .split(',')
            .any(|held| held.trim() == event);
        return named.then(String::new);
    }
    let mut lines = ci.lines().skip_while(|line| {
        let line = line.trim().trim_start_matches("- ");
        line != opening && line != event
    });
    let first = lines.next()?;
    let indent = first.len() - first.trim_start().len();
    Some(
        lines
            .take_while(|line| {
                line.trim().is_empty() || line.len() - line.trim_start().len() > indent
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Every job CI must have, running what it is for.
fn jobs_are_what_they_are_named(ci: &str, findings: &mut Vec<Finding>) {
    for &(job, must_run, times) in JOBS_MUST_RUN {
        let Some(body) = job_body(ci, job) else {
            continue;
        };
        // **What the job runs, not what it mentions.** Keeping every line
        // that starts `run:` *or holds no colon* keeps every argument of an
        // `echo` and every `#` comment. Measured: the whole body replaced by
        // `echo cargo xtask build`, `echo diff -r …` and the rest satisfies
        // every requirement with `check passed`, the first `echo` saying
        // verbatim "the determinism check used to be here" — the attack
        // `JOBS_MUST_RUN`'s own doc names. Worse: with the body reduced to
        // `run: true`, `cargo xtask build` is satisfied by the job's own
        // committed *prose* — "a third of what `cargo xtask build` emits".
        //
        // A command is what a shell would execute: a line that is not a comment,
        // with the needle at the start of it or of one of its `&&`/`|`/`;`
        // segments. `echo diff -r a b` runs `echo`.
        let runs = commands_in(&body);
        let ran = runs
            .iter()
            .filter(|command| command.starts_with(must_run))
            .count();
        if ran < times {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: 1,
                what: format!(
                    "the `{job}` job does not run `{must_run}` {times} time(s); it runs \
                     it {ran}"
                ),
                fix: format!(
                    "a job's name is not a job. `{job}` exists to run `{must_run}`, and a \
                     body that no longer does is a job that passes this gate while \
                     proving nothing — the same defect the name check replaced, one \
                     level in"
                ),
            });
        }
    }
    // **And in that order**, because two builds, a copy and a diff in any order
    // include *copy, diff, build, build*: a tree compared with itself before
    // either build has run.
    if let Some(body) = job_body(ci, "deterministic") {
        let runs = commands_in(&body);
        let mut from = 0;
        every_comparison_follows_the_second_build(&runs, findings);
        for step in IN_THIS_ORDER {
            if let Some(at) = runs
                .iter()
                .skip(from)
                .position(|command| command.starts_with(step))
            {
                from += at + 1;
            } else {
                findings.push(Finding {
                    file: WORKFLOW.to_owned(),
                    line: 1,
                    what: format!(
                        "the `deterministic` job does not reach `{step}` in the order \
                             {IN_THIS_ORDER:?}"
                    ),
                    fix: "the job proves that two *processes* emit the same bytes, and \
                              only this order does that: build, save what the build wrote, \
                              build again, compare. Any other arrangement of the same \
                              commands compares something with itself"
                        .to_owned(),
                });
                break;
            }
        }
    }
    for (job, family) in RUNS_ON {
        let Some(body) = job_body(ci, job) else {
            continue;
        };
        let runner = body
            .lines()
            .map(the_code_before_a_comment)
            .find_map(|line| {
                line.trim()
                    .strip_prefix("runs-on:")
                    .map(|runner| runner.trim().trim_matches(['"', '\'']).to_owned())
            })
            .unwrap_or_default();
        if !runner.starts_with(family) {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: where_it_says(ci, &format!("{job}:")),
                what: format!(
                    "the `{job}` job runs on `{runner}`, which is not a `{family}` runner"
                ),
                fix: format!(
                    "a job named for a system is the proof that the gate passes there \
                     (NOCTUA.md §22.2), and one on another runner proves it of the wrong \
                     system with every rule here green. Run `{job}` on a `{family}…` runner"
                ),
            });
        }
    }
    the_workflow_actually_runs(ci, findings);

    // **Once.** A second copy of this loop at the end of
    // `the_workflow_actually_runs`, which this calls one line up, makes
    // deleting the `deterministic` job produce *two* findings saying so, and
    // `check` print the same sentence twice — and `.any(…)` is true of one
    // finding and of two, so the test that proves the deletion is caught asks
    // for exactly one.
    //
    // It belongs here: the other function reads whether a job is ever asked to
    // run, and this reads whether it is there at all.
    for (job, why) in JOBS {
        if !ci
            .lines()
            .any(|line| line.trim_start() == format!("{job}:"))
        {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: 1,
                what: format!("the `{job}` job is gone"),
                fix: format!(
                    "that job {why}. Counting cargo invocations cannot see a \
                              job that was deleted — the number simply gets smaller"
                ),
            });
        }
    }
}

/// Every comparison in the determinism job follows its second build.
///
/// **`IN_THIS_ORDER` ends at the first `diff`, so it orders none of the job's
/// other three.**
/// `IN_THIS_ORDER` ends at the first `diff`, so the job's other three —
/// the log's generated source, the documentation tree and the
/// append-only error ledger — are counted by `JOBS_MUST_RUN` and
/// ordered by nothing else. Measured: moving those three
/// above the second `cargo xtask build` leaves them comparing the fresh
/// copy against the tree it was copied from, equal by construction, and
/// the whole gate said `check passed`. Three of the four artefacts the
/// emitter writes lost the only cross-process determinism check this
/// repository has, and one of them is a ledger with no git behind it.
/// //
/// The rule is about every comparison rather than a longer list: a
/// `diff` before the second build compares something with itself
/// whatever its operands are, so there is no fifth to add later.
fn every_comparison_follows_the_second_build(runs: &[String], findings: &mut Vec<Finding>) {
    let built = runs
        .iter()
        .enumerate()
        .filter(|(_, command)| command.starts_with("cargo xtask build"))
        .map(|(at, _)| at)
        .nth(1);
    for (at, command) in runs.iter().enumerate() {
        if !command.starts_with("diff") {
            continue;
        }
        if built.is_none_or(|second| at < second) {
            findings.push(Finding {
                file: WORKFLOW.to_owned(),
                line: 1,
                what: format!(
                    "the `deterministic` job runs `{}` before its second \
                     `cargo xtask build`",
                    command
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                fix: "a comparison before the second build reads the copy the first \
                      build's output was copied into, so it compares that output \
                      with itself and passes however the emitter behaves. Every \
                      `diff` in the job belongs after the build that runs in the \
                      copy"
                    .to_owned(),
            });
        }
    }
}

/// The one-based line a needle first appears on, or line one.
fn where_it_says(text: &str, needle: &str) -> usize {
    text.lines()
        .position(|line| line.contains(needle))
        .map_or(1, |at| at + 1)
}

/// One job's body: everything indented under its name, up to the next job.
fn job_body(workflow: &str, job: &str) -> Option<String> {
    let opening = format!("{job}:");
    let mut lines = workflow
        .lines()
        .skip_while(|line| line.trim_start() != opening);
    let first = lines.next()?;
    let indent = first.len() - first.trim_start().len();
    Some(
        lines
            .take_while(|line| {
                line.trim().is_empty() || line.len() - line.trim_start().len() > indent
            })
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

/// Every command a job's body would execute, in order.
///
/// A `run:` value, or a line of a `run: |` block, split at the operators a shell
/// treats as ending one command — so `echo diff -r a b` yields `echo diff -r a
/// b` and not `diff -r a b`. Comments are dropped: a `#` line is prose, and
/// reading prose as a command lets a job's own explanation satisfy the
/// requirement it explains.
fn commands_in(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    // **A heredoc is data, not execution.** Read as execution, every
    // requirement in `JOBS_MUST_RUN` and the whole of `IN_THIS_ORDER` are
    // satisfied by the determinism job's body wrapped in
    // `cat <<'PLAN' >/dev/null … PLAN`, followed by
    // `echo the determinism check used to be here` — the shell runs `cat` and
    // `echo`, the reader sees six commands in the right order, and
    // `check passed`. That echoed sentence is the one `JOBS_MUST_RUN`'s own doc
    // names.
    //
    // The tag is whatever follows `<<` (with `-` and quotes stripped), and the
    // block ends at a line that is exactly that tag. Anything between the two is
    // text being written somewhere, not commands being run.
    let mut heredoc: Option<String> = None;
    for line in body.lines() {
        if let Some(tag) = &heredoc {
            if line.trim() == tag.as_str() {
                heredoc = None;
            }
            continue;
        }
        let line = line.trim_start().trim_start_matches("- ").trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // **`<<<` is a here-string and `1<<20` is a shift.** Both hold the two
        // characters a heredoc opens with, and reading either as one takes a tag
        // that never appears again — after which every command below is inside a
        // block that never closes and no requirement is read at all. The
        // narrowest rule that keeps `<<'PLAN' >/dev/null` working — the
        // spelling the paragraph above measures — is to refuse only the two
        // shapes: a third `<`, and anything inside an arithmetic expansion.
        let opens_a_block = line.split_once("<<").is_some_and(|(before, after)| {
            !after.starts_with('<') && !before.ends_with('$') && !before.contains("$((")
        });
        if opens_a_block && let Some((_, after)) = line.split_once("<<") {
            // **The word first, then its quotes.** Stripping quotes from the
            // whole tail and *then* taking the first word reads
            // `<<'PLAN' >/dev/null` as the tag `PLAN'`: the leading quote goes
            // because it is at the end of the tail, the trailing one does not
            // because a redirect follows it. The block then never closes, every
            // command after it goes unread, and the requirements fail for a
            // reason that has nothing to do with what the file says. That
            // spelling is not hypothetical — it is the one this function's own
            // comment measures.
            let tag: String = after
                .trim_start_matches('-')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .trim_matches(['\'', '"'])
                .to_owned();
            if !tag.is_empty() {
                heredoc = Some(tag);
            }
        }
        // A step's other keys — `name:`, `uses:`, `with:` — are not commands.
        // `run: |` opens a block whose following lines are.
        // **A `name:` is a label, whatever is inside it.** Skipping a key line
        // only when it holds no `&&` makes a step named
        // `nothing && cargo xtask build && cp -r … && diff -r …` six commands
        // that satisfy every requirement — with the job's `run:` reading
        // `echo the determinism check used to be here`, the sentence
        // `JOBS_MUST_RUN`'s own doc names. `is_label` is what the
        // other half of this file uses, and the two halves disagreeing about
        // whether a `name:` is a command is the whole defect.
        if is_label(line) {
            continue;
        }
        let text = match line.strip_prefix("run:") {
            Some(rest) => rest.trim(),
            None if line.contains(':') => continue,
            None => line,
        };
        if text == "|" || text.is_empty() {
            continue;
        }
        // A single bar is the separator, so `&&` and `||` fall out of the same
        // pass: `a || b` becomes two empty segments and `b`, and an empty one is
        // dropped below.
        let flattened = text.replace(['&', ';'], "|");
        for segment in flattened.split('|') {
            let command = segment.trim();
            if !command.is_empty() && !command.starts_with('#') {
                found.push(command.to_owned());
            }
        }
    }
    found
}

/// One finding about a command CI should not run.
/// One finding about a command that runs where only the gate should.
///
/// **Not "CI runs", which is wrong for most of them.** This walk deliberately
/// reads the `justfile` in all four spellings and every shell script a step
/// reaches — what this module calls "the surface a developer types" — and CI
/// runs `cargo xtask check` directly, through none of them. A finding about a
/// justfile line that opens by telling its reader CI runs a command CI never
/// runs, with the file and line printed beside it saying `justfile`, is a
/// sentence contradicting its own locator. This file states the principle that
/// rules it out: *a gate that reports the wrong cause teaches a reader to
/// scroll past it.*
///
/// The command stays in the `what` rather than moving to the `fix`, because
/// `a_second_gate_command_is_a_finding_and_the_two_allowed_shapes_are_not`
/// reads it there.
fn finding(file: &str, line: usize, command: &str, why: &str) -> Finding {
    Finding {
        file: file.to_owned(),
        line,
        what: format!("`{command}` runs here, and it is not the gate"),
        fix: why.to_owned(),
    }
}

/// One line with the YAML comment that may follow the command taken off.
///
/// A `#` inside quotes is data — `run: echo "a # b"` — so the quote state is
/// tracked. Everything from an unquoted `#` on is a note to a reader, and a
/// reader's note is not something CI runs.
///
/// **And a `#` only opens a comment where the languages say it does**: after
/// whitespace, or at the start. It is a comment character in YAML and in every
/// shell only at the beginning of a word, and reading it anywhere truncates an
/// ordinary line of CI shell — `TAG=${GITHUB_REF#refs/tags/} && cargo clippy
/// --workspace …` loses everything from the `#` on, so a second lints gate runs
/// with the count unmoved and `check passed`. `sed s#a#b#` is the same
/// shape. Both counters call this function, so `the_walk_is_not_short`'s
/// identity is paid equally and cannot see it: this is the mirror of the defect
/// above, a scanner reading real code as a comment and skipping it.
fn the_command_without_its_comment(line: &str) -> String {
    // **A backslash escapes the quote that would have closed.**
    // `without_quoted_spans`, three hundred lines up in this file, carries
    // exactly this rule. Measured here without it,
    // `echo "a\"b # c" && cargo clippy --workspace` left the scan
    // inside a string at the `#`, the rest of the line was kept as a command,
    // and a second judging binary went into CI invisible to the gate.
    // `cargo_commands` calls this before anything else it does.
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (at, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match (quote, character) {
            // Inside a quote or outside it: a shell escapes with `\\` wherever
            // it stands, so this arm is not limited to the quoted half.
            (Some('\''), '\\') => {}
            (_, '\\') => escaped = true,
            (Some(open), _) if character == open => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, '#') if at == 0 || line[..at].ends_with([' ', '\t']) => {
                return line[..at].to_owned();
            }
            _ => {}
        }
    }
    line.to_owned()
}

/// The words that put a command after them rather than being one.
///
/// **And a runner that takes arguments of its own before the command.**
/// A set that decides *adjacency* — the word after a wrapper opens a command
/// and the one after that does not — misses `rustup run <toolchain> <binary>`,
/// the ordinary way CI pins a toolchain per command, which puts the binary
/// **three** words on: measured, `run: rustup run nightly miri test
/// --workspace` in `ci.yml` leaves `12 cargo command(s)` unmoved and `check
/// passed` — a second interpreter running this tree's whole suite, invisible
/// to a rule that needs command position for the four names in
/// `judges_this_tree` below.
/// `timeout 30 …`, `sudo -u ci …`, `stdbuf -oL …` and `nohup …` are the
/// same shape.
///
/// So a wrapper opens command position for the **rest of the command**,
/// not for one word — which is what a wrapper means — and that stickiness
/// is used for the judging binaries only. `cargo-deny` stays an argument to
/// `cargo install`, which is the one invocation this gate allows, because
/// nothing there is a wrapper.
const PUTS_A_COMMAND_AFTER_IT: [&str; 14] = [
    "bash", "sh", "zsh", "dash", "xargs", "env", "time", "nice", "rustup", "timeout", "sudo",
    "stdbuf", "nohup",
    // **And `find`, whose `-exec` is the other idiom for the sweep `xargs`
    // does.** `find crates -name '*.rs' -exec rustfmt --check {} +` is a
    // whole second formatting gate, and `-exec` is neither `-` nor `-c`, so
    // without `find` here no word after it opens a command: measured, the
    // count stays unmoved with that step in `ci.yml`. `find` is already used in this very
    // workflow.
    "find",
];

/// What ends one command and begins the next.
///
/// **`&` is one.** The spacer below isolates it, and an enumeration that
/// leaves it out lets `true & cargo-clippy --workspace …` — a whole second
/// lints gate — leave `ok 12 cargo command(s)` unmoved and `check passed`,
/// while the same line with `;` is caught. `AGENTS.md` claims this gate holds
/// every cargo invocation CI reaches "however it is spelled"; `&` is a
/// separator in every shell there is.
///
/// One rule, named, because separate enumerations of the same characters fall
/// short of them separately.
fn separates_commands(word: &str) -> bool {
    matches!(word, "&&" | "||" | "|" | ";" | "&")
}

/// One shell line with a space either side of every command separator.
///
/// A separator glued to the word before it — `add rustfmt; rustfmt --check` —
/// is invisible to a scan that splits on whitespace, and the scan below reads
/// command position and the install excuse from exactly those separators. A
/// separator inside a quoted span is data (`echo "a;b"`), so the quote state
/// decides, as it does everywhere else in this reader.
fn with_the_separators_spaced(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut quote: Option<char> = None;
    let characters: Vec<char> = line.chars().collect();
    let mut at = 0_usize;
    while at < characters.len() {
        let character = characters[at];
        match (quote, character) {
            // **An escaped quote does not close the quote.** This reader's own
            // doc says the quote state decides "as it does everywhere else in
            // this reader", and the other two readers carry a backslash rule.
            // Out of phase, an *unspaced* `;` is never spaced and the word
            // glued to it is never `cargo`: measured without this arm,
            // `echo "a\"b";cargo clippy --workspace` put a second judging
            // binary into CI with the transcript byte-identical, and one space
            // before the `;` — which no shell requires, and which this function
            // exists to make unnecessary — is the control.
            // **Inside a quote or outside it**, because a shell escapes with
            // `\\` wherever it stands: with the arm limited to the quoted half,
            // `echo 'a\\';cargo-clippy --workspace` — the backslash outside the
            // quote — put a second judging binary into CI with the count
            // unmoved. `without_quoted_spans`, which both of these cite as the
            // model, reads it this way.
            // A backslash inside *single* quotes is an ordinary character —
            // `'a\\'` is a backslash and the quote closes at the second `'` —
            // so it escapes nothing there and everything elsewhere.
            (Some('\''), '\\') => out.push(character),
            (_, '\\') => {
                out.push(character);
                if let Some(next) = characters.get(at + 1) {
                    out.push(*next);
                    at += 1;
                }
            }
            (Some(open), _) if character == open => {
                quote = None;
                out.push(character);
            }
            (None, '"' | '\'') => {
                quote = Some(character);
                out.push(character);
            }
            (None, '&' | '|' | ';') => {
                let doubled = characters.get(at + 1) == Some(&character) && character != ';';
                out.push(' ');
                out.push(character);
                if doubled {
                    out.push(character);
                    at += 1;
                }
                out.push(' ');
            }
            _ => out.push(character),
        }
        at += 1;
    }
    out
}

/// Every `cargo …` invocation on one line, however it is spaced.
///
/// **Whitespace, not a space.** `find("cargo ")` misses `cargo\ttest`, which is
/// the same command to every shell there is.
fn cargo_commands(line: &str) -> Vec<String> {
    // **A trailing YAML comment is not part of the command.** `run: cargo xtask
    // check # rustup is not used here; rustfmt comes from the step above` is an
    // ordinary annotation, and with a wrapper word anywhere on the line the
    // sticky rule below puts `rustfmt` into command position inside it —
    // measured, the shipped gate's own step is then reported as running
    // `rustfmt comes from`. A `#` outside quotes ends the line for this reader.
    let line = the_command_without_its_comment(line);
    // **A separator is a separator whether or not it is spaced.** The scan
    // below splits on whitespace, so `rustup component add rustfmt; rustfmt
    // --check …` is one word `rustfmt;` and one word `rustfmt`: the separator
    // is never seen, `installing` never clears, and the second half — a whole
    // second formatting gate — is excused by the `add` in the first. Measured,
    // that step in `ci.yml` left `12 cargo command(s)` unmoved and `check
    // passed`; with one space before the `;` the same line is a finding. No
    // shell requires the space, and `echo linting; cargo-clippy …` hides a
    // judging binary the same way.
    let line = with_the_separators_spaced(&line);
    let line = line.as_str();
    // **Whether this line is a command at all**, which is what decides how hard
    // the rule below has to look. A line whose first word is a YAML key other
    // than `run:` is data — `name: check formatting with rustfmt` is prose, and
    // `components: rustfmt` is a `with:` block that installs a component and
    // runs nothing. Everything else is shell: a `run:` line, a line inside a
    // `run: |` block, a justfile recipe body, a script.
    let a_command_line = line
        .trim_start_matches("- ")
        .split_whitespace()
        .next()
        .is_none_or(|word| word == "run:" || !word.ends_with(':'));
    let mut found = Vec::new();
    let mut opens_a_command = true;
    // Sticky from the first wrapper to the end of this command, and reset by
    // the separators that end one.
    let mut after_a_wrapper = false;
    // Whether this segment installs rather than runs. `rustup component add
    // rustfmt` and `rustup toolchain install nightly --component miri` name a
    // judging binary and run nothing, and a separator ends the excuse: in
    // `rustup component add rustfmt && rustfmt --check` the second half is a
    // gate.
    let mut installing = false;
    for (index, word) in line.split_whitespace().enumerate() {
        if separates_commands(word) {
            after_a_wrapper = false;
            installing = false;
        } else if matches!(word, "install" | "add") {
            installing = true;
        } else if PUTS_A_COMMAND_AFTER_IT.contains(&word) {
            after_a_wrapper = true;
        }
        let opens_a_command = std::mem::replace(
            &mut opens_a_command,
            // A separator ends one command and begins the next; a YAML key or
            // list marker introduces one.
            // **And past an interpreter.** `bash -c "cargo-clippy …"` puts the
            // command one word further on, which is not command position by the
            // rule above — so without this a whole second lints-and-formatting
            // gate goes into both `ci.yml` and the `justfile` with the count
            // unchanged. A shell's `-c` argument *is* a command, and the quote
            // in front of it is not part of the name.
            // **And past a runner that takes a command as its argument.**
            // Without it, `git ls-files "*.rs" | xargs rustfmt --check
            // --edition 2024` passes, while the same `rustfmt` written first on
            // the line is caught: `xargs` puts the command one word on, exactly
            // as `bash -c` does, and `env` does the same. A second formatting
            // gate with the count unchanged is the defect `§19.4` names.
            // **`run:` opens a command; another YAML key does not.** With any
            // word ending in `:` opening one, `components: rustfmt` — a `with:`
            // block of the toolchain action, which *installs* a component and
            // runs nothing — puts `rustfmt` in command position and reports the
            // shipped workflow as running a second gate, unless the block names
            // two components and the comma keeps `rustfmt,` from matching. A
            // justfile recipe header ends with `:`
            // too, and its body is on the next line, so nothing is lost.
            separates_commands(word)
                || matches!(word, "-" | "-c" | "run:")
                || PUTS_A_COMMAND_AFTER_IT.contains(&word),
        );
        // A word that *is* `cargo`, or a path ending in it. `cargo-deny` is not
        // an invocation; `~/.cargo/bin/cargo` is, and calling it by path is a fourth way past a bare-word match. <!-- allow-reference: names `~/.cargo/bin/cargo` — a path on a developer's machine, never in this tree -->
        // **Both ends, and that is a finding.** With leading punctuation
        // trimmed and trailing not, `$(which cargo)` splits into `$(which` and
        // `cargo)` — the first stripped to `which`, the second left with a
        // parenthesis stuck to it, and neither reads as `cargo`. A second lints
        // gate written that way contributes **zero** to both the walk and the
        // second count, so `the_walk_is_not_short`'s identity holds while the
        // walk is short: both counters call this function, so anything it
        // cannot see is invisible to the comparison meant to catch that.
        // **And `just`'s own line prefixes.** `@` says do not echo, `-` says
        // ignore a failure, `+` says pass the line to the shell — this
        // repository's `default` recipe already writes `@just --list`.
        // Untrimmed, a word wearing one of them is not a cargo command, so it
        // is not `seen`, is in neither count, and every rule downstream of this
        // function — the offline rule, the recipe-name rule, the second-gate
        // rule — never looks at it. Measured: `@cargo xtask check --offline` in
        // the shipped justfile's `docs` recipe, with the whole suite green.
        let bare = word.trim_matches(['"', '\'', '(', ')', '$', '`', '@', '-', '+']);
        // **An assignment is not an invocation.** A word holding `=` sets a
        // variable, and its value may perfectly well be a directory whose last
        // segment is the word `cargo` — a cargo installation directory is the
        // ordinary case. Read as a command it reports a line that runs nothing
        // at all, which sends a reader to correct working configuration. What
        // an assignment *does* mean is handled by `assigns_cargo`, which asks
        // whether the value is the program rather than a place.
        if bare
            .split('/')
            .next()
            .is_some_and(|first| first.contains('='))
        {
            continue;
        }
        let name = bare.rsplit('/').next().unwrap_or(bare);
        // **A cargo subcommand's own binary is a cargo command.** Asking for a
        // word that *is* `cargo` misses what rustup ships beside it,
        // `cargo-clippy` and `cargo-fmt`: running either directly runs clippy
        // or rustfmt over the workspace, and CI gains a second lints gate with
        // the count unchanged. That is "a cargo command wearing a different spelling", which
        // is the class `assigns_cargo` and `runs_cargo_as_an_action` exist for.
        //
        // The verb is what follows the hyphen, so `cargo-clippy` reads as
        // `cargo clippy` and lands on the same rule as the spaced form.
        //
        // **Only where a command can start**, because `cargo-deny` is also an
        // *argument*: `cargo install --locked cargo-deny` is the one install
        // this gate allows, and reading its last word as an invocation would
        // report the line the gate exists to permit.
        // **And the binaries underneath, which are on the runner already.**
        // `rustfmt --check` and `clippy-driver` are what `cargo fmt` and
        // `cargo clippy` invoke, and neither is a cargo command by name — so a
        // step running `rustfmt --check --edition 2024 crates/…` is a second
        // formatting gate that a cargo-only reader leaves with the count
        // unchanged. `§19.4` is that CI
        // and a developer run one gate; a tool that judges this tree is a
        // second one whatever binary it is.
        // **Anywhere after a wrapper, not only adjacent to one.** These four
        // are never an argument in a legitimate line of this workflow or of the
        // justfile — a step that names one is running it — and `rustup run
        // nightly miri` is what shows the adjacent rule too narrow.
        // **Anywhere on a line that is shell, and the wrapper list stops being
        // the rule.** These four are never an argument in a legitimate command:
        // a line that names one is running it, unless it is installing it. A
        // list of words that open command position, doing that job, is an
        // enumeration one word short every time — `exec rustfmt --check`, the
        // way a wrapper script ends, walks through it with the count unmoved,
        // and `flock`, `command`, `eval`, `su` and `setsid` are the same hole.
        // This file's own doc calls an enumeration standing in for a rule a
        // shape to refuse; this is the rule.
        //
        // **The cost, stated:** a `run:` line that merely mentions one of the
        // four — `run: echo "rustfmt comes from the step above"` — is reported.
        // That is a step naming a judging binary, which is worth a reader's
        // eye, and it is the direction that fails loudly rather than silently.
        // Prose *outside* a command line is not reached at all: `name:` and
        // `components:` are keys, and `a_command_line` is false for them.
        let judges_this_tree = !installing
            && (a_command_line || opens_a_command || after_a_wrapper)
            && matches!(name, "rustfmt" | "clippy-driver" | "rustdoc" | "miri");
        // **And a `cargo-*` binary is judged the same way the four above are.**
        // Asking for *command position* is the enumeration the paragraph above
        // sets aside: measured, `exec cargo-clippy --workspace --all-targets --
        // -D warnings` in `ci.yml` then leaves the transcript byte-identical to
        // a clean run, with the count unmoved, while the same line without
        // `exec` is reported at once. A `cargo-` binary on a command
        // line is a second judging binary wherever it stands, and the cost — a
        // line that merely mentions one being reported — is the same cost, taken
        // for the same reason.
        let is_cargo = name == "cargo"
            || (!installing
                && (a_command_line || opens_a_command || after_a_wrapper)
                && name.starts_with("cargo-"));
        if !is_cargo && !judges_this_tree {
            continue;
        }
        found.push(
            line.split_whitespace()
                .skip(index)
                .collect::<Vec<_>>()
                .join(" "),
        );
    }
    found
}

/// The action a line invokes, when that action's job is to run cargo.
///
/// `actions-rs/cargo` <!-- allow-reference: names `actions-rs/cargo` — a GitHub Action, not a path in this tree --> is the standard GitHub Action for it, and it takes the
/// subcommand in a `with:` block rather than in a `run:` line — so a gate that
/// reads what a step *runs* never sees it.
fn runs_cargo_as_an_action(line: &str) -> Option<String> {
    // **Flow style opens a mapping too.** Unless the braces are stripped,
    // `- { uses: X }` never reaches this reader, so any third-party action is
    // invisible to the allow-list — measured with `giraffate/clippy-action@v1`, a clippy action <!-- allow-reference: names `giraffate/clippy-action@v1` — a GitHub Action, not a path in this tree -->
    // like the one that list's own doc names: one pair of braces walks past
    // it, the count does not move, and nothing is reported. `gates.rs`'s
    // `is_a_build_key` carries a mutation operator for the same class.
    let line = line.trim_start_matches("- ").trim();
    let line = line
        .strip_prefix('{')
        .map_or(line, |inside| inside.trim_end_matches('}').trim());
    // A flow mapping may hold more than one key — `{ uses: X, with: … }` — and
    // the action is the first, up to the comma.
    let uses = line
        .strip_prefix("uses:")?
        .split(',')
        .next()
        .unwrap_or_default()
        .trim();
    let name = uses.split('@').next().unwrap_or(uses).trim();
    // A local composite action is walked as a file of its own.
    if name.starts_with('.') {
        return None;
    }
    if SETS_UP_AND_JUDGES_NOTHING
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(name))
    {
        return None;
    }
    Some(format!("uses: {name}"))
}

/// The variable a line assigns `cargo` to, if it assigns one.
///
/// **Every assignment on the line, not the first `=` on it.** Reading
/// `statement.split_once('=')` and stopping lets any assignment written in
/// front of the alias hide it — no unusual name needed. Measured, a CI step reading
/// `run: JOBS=1; C=cargo; $C clippy --workspace --all-targets -- -D warnings`:
/// `ok 12 cargo command(s)` and `check passed`, which is the clean tree's own
/// line, over a second lints gate the walk never saw. The same step without the
/// leading `JOBS=1` is refused by name.
fn assigns_cargo(line: &str) -> Option<String> {
    let statement = line
        .trim_start_matches("- ")
        .trim_start_matches("run:")
        .trim();
    // The separators the value rule already cuts on are where one shell
    // statement ends and the next begins, so they are where the next assignment
    // can start.
    statement
        .split(&[';', '&', '|'][..])
        .find_map(the_variable_one_statement_assigns)
}

/// The variable one shell statement assigns `cargo` to, if it assigns one.
fn the_variable_one_statement_assigns(statement: &str) -> Option<String> {
    let (name, value) = statement.split_once('=')?;
    // **Three spellings, not one.** `alias c=cargo` is a shell builtin whose
    // name holds a space once the line is split, and `CARGO=$(which cargo)`
    // resolves the path at run time — both run cargo, and unread they leave
    // the count unchanged.
    let name = name
        .trim()
        .trim_start_matches("export ")
        .trim_start_matches("alias ")
        .trim();
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    // **A name that holds a directory is not a name that holds cargo.** A
    // variable set to a cargo *installation directory* resolves, by the path
    // rule below, to a value whose last segment is the word `cargo` — and it is
    // a location, which cannot be run.
    // Reported, it sends a reader to a line that is doing nothing wrong, and
    // the tempting fix (refuse any value that is not `…/bin/cargo`) would undo
    // the live rule: `CARGO=cargo`, `alias c=cargo` and `CARGO=$(which cargo)`
    // all resolve to the bare word. So the exclusion is by *name*: these hold
    // paths by convention and none of them can stand in command position.
    let named_like_a_location = matches!(name, "PATH" | "CARGO_HOME" | "RUSTUP_HOME")
        || name.ends_with("_HOME")
        || name.ends_with("_DIR")
        || name.ends_with("_PATH");
    // **The value ends where the statement does**, which is why the caller
    // splits on separators first. Read as one value, everything after the `=`
    // in `CARGO=cargo; $CARGO clippy …` — assignment and use on one line, which
    // is ordinary shell — is `cargo; $CARGO clippy …`, matches nothing, and
    // leaves `ok 11 cargo command(s)` with a run byte-identical to a clean one.
    // The same assignment on its own line is caught, so the escape is the
    // semicolon and nothing else.
    let value = value.trim().trim_matches(['"', '\'']);
    // **And the value has to look like one.** An exclusion by *name* alone
    // lets a renamed variable hide a whole gate: measured, a CI step reading
    // `TOOLS_DIR=cargo; $TOOLS_DIR clippy …` leaves
    // `ok 12 cargo command(s)` and `check passed` — the clean tree's own line —
    // while `TOOLS=cargo` on the same line is refused by name. A location is a
    // path, and a path holds a separator: a cargo home written as an absolute
    // directory or through the home variable, and a search path with a cargo
    // bin directory on it, are all still excused — the rows in `ASSIGNMENTS`
    // spell them out. None of `CARGO=cargo`, `alias c=cargo` or
    // `CARGO=$(which cargo)` is affected.
    if named_like_a_location && value.contains('/') {
        return None;
    }
    // `cargo`, a path ending in it, or a command substitution that finds it.
    let resolved = value
        .trim_start_matches("$(")
        .trim_end_matches(')')
        .split_whitespace()
        .next_back()
        .unwrap_or(value);
    let is_cargo = [value, resolved]
        .iter()
        .any(|text| *text == "cargo" || text.rsplit('/').next() == Some("cargo"));
    if is_cargo {
        Some(name.to_owned())
    } else {
        None
    }
}

/// What each way of writing an alias resolves to.
///
/// **Both halves of `assigns_cargo`, named as rows.** Excluding a variable by
/// *name* alone lets `TOOLS_DIR=cargo` hide a whole second lints gate with
/// `ok 12 cargo command(s)` and `check passed` — the clean tree's own line —
/// while `TOOLS=cargo` on the same line is refused. And reading only the first
/// `=` on the line with `split_once('=')` lets any assignment written in front
/// of the alias hide it just as well.
///
/// Both directions are rows: what must be found, and what must stay excused.
#[cfg(test)]
const ASSIGNMENTS: [(&str, Option<&str>); 12] = [
    ("run: CARGO=cargo; $CARGO clippy --workspace", Some("CARGO")),
    ("run: export CARGO=cargo", Some("CARGO")),
    ("run: alias c=cargo", Some("c")),
    ("run: CARGO=$(which cargo)", Some("CARGO")),
    ("run: CARGO=/usr/local/bin/cargo", Some("CARGO")),
    // Named like a location and holding the bare word, which is an alias.
    (
        "run: TOOLS_DIR=cargo; $TOOLS_DIR clippy --workspace",
        Some("TOOLS_DIR"),
    ),
    // A second assignment in front of it.
    ("run: JOBS=1; C=cargo; $C clippy --workspace", Some("C")),
    ("run: RUSTFLAGS=-D warnings; CARGO=cargo", Some("CARGO")),
    // Named like a location *and* holding one: still excused.
    // **A location whose last segment *is* the word**, which is the only shape
    // the exclusion exists for: without it this reads as an alias, and reported
    // it sends a reader to a line doing nothing wrong.
    ("run: CARGO_HOME=/usr/local/cargo", None),
    ("run: CARGO_HOME=$HOME/.cargo", None),
    ("run: PATH=/usr/local/cargo/bin:$PATH", None),
    // Not an assignment at all.
    ("run: cargo xtask check", None),
];

/// Every spelling of an alias is found, and every location is still excused.
#[test]
fn an_alias_is_found_however_the_line_is_written() {
    let mut wrong = Vec::new();
    for (line, expected) in ASSIGNMENTS {
        let answered = assigns_cargo(line);
        if answered.as_deref() != expected {
            wrong.push(format!(
                "  `{line}` — the reader answered {answered:?}, and it assigns \
                 {expected:?}. A variable holding `cargo` is a second gate the walk \
                 counts, and one holding a path is a location that cannot stand in \
                 command position"
            ));
        }
    }
    // One sentence, said where the verdict census reads it: the reason belongs
    // in the row rather than in an assertion message no list holds.
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// What is wrong when a recipe's name is not what its body runs, or nothing.
///
/// **A recipe is named for what it runs**: the verb, and `-offline` after it
/// when the command carries `--offline`. Unless the two are related, any recipe
/// can call any verb with the one-line description above it — the whole of
/// what `just --list` prints — going on saying what it used to do. Measured by
/// swapping the bodies of `check` and `check-offline`: the gate a developer
/// types becomes the smaller gate, with README, AGENTS and the recipe's own
/// sentence all still calling it THE gate.
fn a_recipe_runs_what_it_is_called(recipe: &str, header: &str, command: &str) -> Option<String> {
    // **An interpolation the recipe does not declare.** This rule reads the
    // body as text, so `cargo xtask check {{flags}}` with `flags := "--offline"`
    // above it is the swapped-body attack of the doc above, restored in two
    // lines — the gate a developer types becomes the smaller gate and the name
    // still matches. A recipe's *parameters* are declared in its header and
    // are ordinary; anything else is a value this gate cannot see.
    // **A parameter keeps its name when it carries a default.** `just` writes
    // `release version="…":`, so the word is the name *and* the default — a
    // literal version here would be read by `release.rs` as this file carrying
    // the workspace version, which is a release that would rewrite a comment —
    // read whole, the legitimate recipe is refused and told "A recipe's own
    // parameters are declared in its header; this one is not", which is the
    // inverse of the truth.
    //
    // **And no call form is excused.** Letting `{{justfile_directory()}}`
    // through as "a call is not a variable somebody set", written as
    // *ends with a parenthesis*, excuses every one of `just`'s functions,
    // including the three whose whole job is to fetch a value from outside the
    // file: `env_var`, `env_var_or_default` and `shell`. With that excuse,
    // `cargo xtask check {{env_var("FLAGS")}}` in this repository's own `check`
    // recipe is the swapped-body attack restored and the gate says nothing at
    // all. Refusing every call costs nothing, measured: the shipped `justfile`
    // holds one interpolation, `{{version}}`, which is a declared parameter of
    // `release`. A call form that ever does need to be written here can be
    // named in the header beside the parameters, where a reader can see it.
    // **And a parameter keeps its sigil as well as its default.** `just` writes
    // a variadic as `+args` or `*args` and an exported one as `$name`; with the
    // sigil left on the declared name, the body's `{{args}}` is never found
    // among the declared and a correct recipe is refused with "A recipe's own
    // parameters are declared in its header; this one is not", which is the
    // inverse of the truth.
    let declared: Vec<&str> = header
        .split_whitespace()
        .skip(1)
        .map(|word| {
            word.trim_start_matches(['+', '*', '$'])
                .split('=')
                .next()
                .unwrap_or_default()
        })
        .collect();
    for (at, _) in command.match_indices("{{") {
        let inside = command[at + 2..]
            .split("}}")
            .next()
            .unwrap_or_default()
            .trim();
        if !declared.contains(&inside) {
            return Some(format!(
                "`{recipe}` runs a cargo command holding `{{{{{inside}}}}}`, which this \
                 gate reads as text and cannot resolve — a variable in the body is how \
                 the gate a developer types becomes a different gate with the name and \
                 the description above it unchanged. A recipe's own parameters are \
                 declared in its header; this one is not"
            ));
        }
    }
    let ran = command.split_whitespace().nth(2).unwrap_or_default();
    let named = if command.contains("--offline") {
        format!("{ran}-offline")
    } else {
        ran.to_owned()
    };
    if recipe == named {
        return None;
    }
    Some(format!(
        "`{recipe}` is the name a developer types and `{named}` is what this body \
         runs. The description above it is the whole of what `just --list` prints, \
         so a recipe that changes what it runs goes on saying what it used to do — \
         and `check` carrying `--offline` is the smaller gate wearing the gate's name"
    ))
}

/// Whether `--offline` here is the smaller gate wearing the gate's name.
///
/// CI may never carry it. Outside CI it is carried by one recipe, the one whose
/// name ends `-offline` and says so — an exclusion for that recipe applied to
/// every non-CI file lets a shell script a step runs carry it with no recipe
/// name at all, and nothing speaks.
fn an_offline_gate_is_wearing_the_gates_name(
    recipe: &str,
    command: &str,
    file_is_ci: bool,
) -> bool {
    command.contains("--offline") && (file_is_ci || !recipe.ends_with("-offline"))
}

/// The recipe header a line sits under, and the name at the front of it.
fn the_recipe_and_its_name(
    text: &str,
    index: usize,
    file_is_ci: bool,
    named: &str,
) -> (String, String) {
    let header = the_recipe_above(text, index, file_is_ci, named);
    // **`just`'s echo sigil is not part of the name.** Read with it,
    // `@release version:` makes this gate say "`@release` is the name a
    // developer types and `release` is what this body runs" — inverted about
    // the one value it quotes, because `just --list` prints `release` and
    // `just release 9.9.9` is what a developer types, and the remedy it implies
    // is to rename a recipe that is already named correctly. The sigil is at
    // the front, so the `-offline` reading beside this is untouched.
    let name = header
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_start_matches('@')
        .to_owned();
    (header, name)
}

/// The recipe header a line of a `justfile` sits under, or the empty string.
///
/// The whole header, because its words after the name are the parameters the
/// body may interpolate and everything else in a `{{…}}` is a value this gate
/// reads as text and cannot resolve.
///
/// A recipe header is a line at **column zero** ending in `:`, with its
/// parameters after the name — `release version:`. `is_label` cannot be asked
/// this: it matches a YAML `name:` key, and a justfile has none.
fn the_recipe_above(text: &str, index: usize, file_is_ci: bool, named: &str) -> String {
    // **And only in a file that has recipes.** Without this, `ends_with(':')`
    // silently does duty as "this is a justfile", and the header cannot be
    // widened: the walk runs over every non-CI file CI reaches, and
    // `xtask/the-checkout-shape.sh` holds `keep=${1:+the caller named it}` at
    // column zero — a shell line with a colon that a wider reader takes for a
    // recipe header, after which this repository's own committed script is
    // refused with "`keep=${1` is the name a developer types". Measured
    // against the obvious repair. Naming the file is what lets the header rule
    // be about headers.
    //
    // **And makefiles are outside it deliberately, not as collateral.** The walk
    // reads `MAKEFILE` too and this narrowing names only `JUSTFILE`, so a
    // `check:` target running `cargo xtask check-offline` in a `Makefile` is
    // silent while the identical justfile recipe is refused. That looks like an
    // exclusion list turning a gate off — but the repair is measured
    // **too strong**:
    // `VAR ::= x`, `.PHONY: check` and `%.o: %.c` all read as recipes named
    // `VAR`, `.PHONY` and `%.o`, each then compared against a cargo verb. The
    // rule's content is justfile-specific besides — it is about what
    // `just --list` prints above a recipe, while a make target is named for the
    // artifact it builds, so an ordinary `ci:` running the gate would be refused
    // for having the wrong name. A makefile gate is a different rule and wants
    // writing as one; what this narrowing must not do is *look* like it covers
    // them, which is why it says so here.
    let (_, has_recipes) = the_role_of(named);
    if file_is_ci || !has_recipes {
        return String::new();
    }
    // A comment at column zero can end in a colon — this file's own
    // `The nine left out are:` does — and is not a recipe.
    //
    // **And a recipe may name a dependency after its colon.**
    // `release version: build` is ordinary `just`, and a filter wanting
    // the colon at the *end* of the line does not see it as a header at
    // all — so the body below it is attributed to the previous recipe
    // and its author is told "A recipe's own parameters are declared in
    // its header; this one is not" over a header that is not theirs.
    //
    // **The state crosses the line boundary.** A triple-quoted default spans
    // lines, so a line may be *inside* a string rather than at the top level —
    // and a line inside one is not a header however it reads. A one-line
    // reader takes the content line `a:b` of a multi-line default for the
    // recipe header.
    // A header written across lines is one header. The physical lines are
    // grouped into logical ones — joined while a triple-quoted string is still
    // open — and the question is then asked of the logical line, which is what
    // `just` itself parses.
    let mut logical: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut reached_by: Option<(char, bool)> = None;
    for line in text.lines().take(index + 1) {
        if !current.is_empty() {
            current.push('\n');
        }
        current.push_str(line);
        reached_by = the_headers_colon_within(line, reached_by).1;
        if reached_by.is_none() {
            logical.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        logical.push(current);
    }
    logical
        .iter()
        .rfind(|line| {
            !line.starts_with(char::is_whitespace)
                && !line.starts_with('#')
                && the_headers_colon(line).is_some()
        })
        .map(|line| {
            the_headers_colon(line)
                .map_or(line.as_str(), |at| &line[..at])
                .trim_end()
                .to_owned()
        })
        .unwrap_or_default()
}

/// Where a recipe header's own colon is, if the line has one.
///
/// **Not `split_once(':')`, because a parameter's default may hold one.** `just`
/// writes `serve addr="127.0.0.1:8080" mode="dev":`, and a reader that cuts at
/// the first colon returns the header `serve addr="127.0.0.1` — so `mode`, a
/// parameter the header genuinely declares, is gone and a body writing
/// `{{mode}}` is refused with "A recipe's own parameters are declared in its
/// header; this one is not".
///
/// `:=` is an assignment rather than a header, and a colon inside a string is
/// data. **The escape is tracked**, because a reader that takes `\"` for a
/// closing quote finds the next colon inside what it thinks is a string and
/// returns no header at all — the recipe then vanishes and the whole rule goes
/// silent for it, which is the same hole one escape along. Measured against the
/// repair without it.
fn the_headers_colon(line: &str) -> Option<usize> {
    the_headers_colon_within(line, None).0
}

/// The same question asked of a line that a triple-quoted string may reach into.
///
/// **A recipe header is a line, and a `just` string is not.** A reader that
/// knows `just`'s three triple forms but keeps a one-line shape reads a default
/// written across lines — which `just 1.55.1` parses as one recipe with two
/// parameters, measured — as three unrelated lines. The header line opens a
/// triple that never closes and returns `None`; the *content* line `a:b` stands
/// at column zero, is not a comment and holds a colon, so `the_recipe_above`
/// takes the string's own text for the recipe header.
///
/// So the state crosses the line boundary. What is returned beside the colon is
/// the triple still open at the end of the line, and a line reached by one is not
/// a header at all. Only triples cross: `just`'s single-delimiter strings end at
/// the newline, and a file where one does not is a file `just` itself refuses.
fn the_headers_colon_within(
    line: &str,
    reached_by: Option<(char, bool)>,
) -> (Option<usize>, Option<(char, bool)>) {
    /// Whether the delimiter at `at` is written three times over.
    ///
    /// `just` has six string forms, not three: `"""…"""`, `\'\'\'…\'\'\'` and
    /// ```` ```…``` ```` are the indented spellings of the other three, and all
    /// three are legal as a parameter default. Measured against `just 1.55.1`,
    /// which parses each of the three as one recipe with two parameters.
    fn thrice(line: &str, at: usize, kind: char) -> bool {
        let mut spelling = [0_u8; 4];
        let one: &str = kind.encode_utf8(&mut spelling);
        line[at..].starts_with(one.repeat(3).as_str())
    }

    // The delimiter, and whether it was written three times over.
    let mut quote: Option<(char, bool)> = reached_by;
    let mut escaped = false;
    // The header's own colon, once one is seen: `Some(None)` for `:=`, which is
    // an assignment rather than a header, and `Some(Some(at))` for a header's.
    let mut colon: Option<Option<usize>> = None;
    let mut letters = line.char_indices();
    while let Some((at, letter)) = letters.next() {
        if escaped {
            escaped = false;
            continue;
        }
        match letter {
            // **A backslash escapes inside `"…"` and not inside `'…'`.**
            // `just`'s single-quoted strings are raw, which this file already
            // holds for three of its own readers — `a_backslash_inside_single_
            // quotes_does_not_escape_the_quote` pins it for one and
            // `an_escaped_quote_does_not_close_a_quote` for two more.
            // Escaping in both, `build sep='\':` — the canonical reason raw
            // strings exist — never closes its quote, the header's own colon
            // reads as data, and the line stops being a header at all.
            '\\' if matches!(quote, Some(('"', _))) => escaped = true,
            // **A backtick is a quote to `just` too.** It accepts a command
            // substitution as a parameter default, and an untracked backtick
            // lets the colon inside one read as the header's own — truncating
            // the header and refusing a correct recipe with the parameters
            // sentence, false about it.
            // **And a triple is one delimiter, not three.** Read as three, a
            // `"""` opens a string, closes it and opens it again, so every
            // quote inside the literal toggles and a colon in it reads as the
            // header's own. `serve x="""a"b:c""" y="d":` — which `just`
            // parses as one recipe with two parameters — comes back as the
            // header `serve x="""a"b`, losing `y`, and the body's `{{y}}` is
            // then refused with the parameters sentence. The `'''` and
            // ```` ``` ```` forms are the same hole twice more.
            '"' | '\'' | '`' => match quote {
                Some((open, tripled)) if open == letter => {
                    if !tripled {
                        quote = None;
                    } else if thrice(line, at, letter) {
                        letters.next();
                        letters.next();
                        quote = None;
                    }
                }
                Some(_) => {}
                None => {
                    let tripled = thrice(line, at, letter);
                    if tripled {
                        letters.next();
                        letters.next();
                    }
                    quote = Some((letter, tripled));
                }
            },
            // **The colon is recorded, not returned.** A `return` here leaves
            // nothing after the header's own colon scanned for quotes — and an
            // assignment opening a triple, `flags := """`, then reports "no
            // string open". The logical line ends there, and the string's own
            // content line `check: not a recipe` stands at column zero, is not
            // a comment and holds a colon, so it is taken for the recipe header
            // and the real recipe below is lost: the defect this function's doc
            // names, one spelling along.
            ':' if quote.is_none() && reached_by.is_none() => {
                if colon.is_none() {
                    colon = if line[at + 1..].starts_with('=') {
                        Some(None)
                    } else {
                        Some(Some(at))
                    };
                }
            }
            // **A `#` outside a string ends the line.** `just` takes the rest
            // as a comment, so a `"""` written in one opens nothing — and a
            // reader that carries it swallows every line below into one group.
            // Guarded on `quote.is_none()`, because `release version="a#b":` is
            // a recipe `just --dump` echoes back verbatim, and a bare `break`
            // loses it whole. Both measured.
            '#' if quote.is_none() => break,
            _ => {}
        }
    }
    // Only a triple reaches the next line. A single delimiter left open is a
    // line `just` itself will not parse, and carrying it would silence every
    // recipe below it.
    let still_open = quote.filter(|(_, tripled)| *tripled);
    (colon.flatten().filter(|_| still_open.is_none()), still_open)
}

/// Whether a line is a YAML `name:` rather than something that runs.
fn is_label(trimmed: &str) -> bool {
    trimmed
        .strip_prefix("- ")
        .unwrap_or(trimmed)
        .starts_with("name:")
}

/// The first three words of a command, for a message that fits on a line.
fn first_words(command: &str) -> String {
    command
        .split_whitespace()
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {

    /// The bound is the bound the production caller has, measured through it.
    ///
    /// **The test beside this one drives the walk directly, and that is not
    /// where the budget is split.** [`super::everything_ci_reaches`] calls the
    /// walk twice, and the shallow half of that test is `walk(4)` against a fresh
    /// sixteen — so it does not ask what the *second* call sees. Measured through
    /// this entry point with a hop spent on every pass: a chain of fifteen
    /// reached from the justfile was read whole, `chain_scripts_read=15`, and the
    /// run still said `what CI reaches is more than 16 hops deep`, because the
    /// first call had spent one of the sixteen on the pass that finds nothing and
    /// ends it.
    ///
    /// So a hop is a step that *found* something, and this drives the real entry
    /// point at the bound and one past it. A rule about a bound whose test does
    /// not go through the caller that splits it is a rule tested somewhere else.
    #[test]
    fn the_bound_is_what_the_production_caller_reaches() {
        let held = crate::check::LeavesNothingBehind::new("workflow-bound");
        let build = |depth: usize| -> std::path::PathBuf {
            let root = held.root.join(format!("d{depth}"));
            std::fs::create_dir_all(root.join(".github/workflows")).expect("writable");
            std::fs::write(
                root.join(".github/workflows/ci.yml"),
                "on: push\njobs:\n  check:\n    steps:\n      - run: cargo xtask check\n",
            )
            .expect("writable");
            std::fs::create_dir_all(root.join("ci")).expect("writable");
            std::fs::write(root.join("justfile"), "check:\n    sh ci/s01.sh\n").expect("writable");
            for step in 1..=depth {
                let body = if step == depth {
                    "echo the end of the chain\n".to_owned()
                } else {
                    format!("sh ci/s{:02}.sh\n", step + 1)
                };
                std::fs::write(root.join(format!("ci/s{step:02}.sh")), body).expect("writable");
            }
            root
        };
        let reach = |depth: usize| {
            let root = build(depth);
            let (files, ran_out) = super::everything_ci_reaches(&root).expect("the walk runs");
            let read = files
                .iter()
                .filter(|path| crate::relative_to(&root, path).starts_with("ci/s"))
                .count();
            (read, ran_out)
        };

        // At the bound: every script read, and nothing said.
        let (read, ran_out) = reach(super::HOPS);
        assert_eq!(
            read,
            super::HOPS,
            "a chain of {} was not read whole by a walk bounded at {}",
            super::HOPS,
            super::HOPS
        );
        assert!(
            !ran_out,
            "a chain of {} is the bound, not past it, and the walk reported stopping \
             short — which is the false finding this bound was reworked to remove",
            super::HOPS
        );
        assert_eq!(super::a_chain_deeper_than_the_walk(ran_out), None);

        // Past it: the walk stops short and says so.
        let (read, ran_out) = reach(super::HOPS * 2);
        assert!(
            ran_out,
            "a chain of {} is past a bound of {} and the walk did not say it stopped \
             short",
            super::HOPS * 2,
            super::HOPS
        );
        assert!(
            read < super::HOPS * 2,
            "the walk read the whole chain and still reported running out"
        );
        drop(held);
    }

    /// The walk reports its bound when it is spent, and not when it is not.
    ///
    /// **Both sides of the bound, driven.** With neither the bound nor the
    /// finding beside it driven, the walk can take a fresh budget on each of its
    /// two calls and report running out when it has not. Measured with a chain
    /// of twenty scripts: every one was read, every cargo invocation in it was
    /// judged, and the run still said `what CI reaches is more than 16 hops
    /// deep`.
    ///
    /// A finding that fires when nothing is wrong is the failure `counts.rs`'s
    /// header is about: it teaches a reader to ignore the gate. So both
    /// directions are here, and the depth is written against [`super::HOPS`]
    /// rather than as a number, so raising the bound moves the test with it.
    #[test]
    fn the_walk_says_it_ran_out_exactly_when_it_did() {
        // **Guarded, because a panic in this test leaves a chain of scripts
        // behind.** Its neighbours in this file build a raw `temp_dir()` path
        // and remove it on the way out, which is the path a *failing* test does
        // not take — the backlog `BUILDS_A_SCRATCH` counts. A new one does not
        // join a backlog when the guard is one line.
        let held = crate::check::LeavesNothingBehind::new("workflow-hops");
        let scratch = held.root.clone();
        let chain = |depth: usize| {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join("ci")).expect("writable");
            for step in 1..=depth {
                let body = if step == depth {
                    "echo the end of the chain\n".to_owned()
                } else {
                    format!("sh ci/s{:02}.sh\n", step + 1)
                };
                std::fs::write(scratch.join(format!("ci/s{step:02}.sh")), body).expect("writable");
            }
        };
        let walk = |depth: usize| {
            chain(depth);
            let mut files = vec![scratch.join("ci/s01.sh")];
            let mut hops = super::HOPS;
            let ran_out = super::follow_what_they_reach(&scratch, &mut files, &mut hops);
            (ran_out, files.len())
        };

        // Well inside the bound: the whole chain is read and nothing is said.
        let (ran_out, read) = walk(4);
        assert!(
            !ran_out,
            "a chain of 4 is inside a bound of {}, and the walk reported running out",
            super::HOPS
        );
        assert_eq!(read, 4, "the walk did not read the whole chain it fits");
        assert_eq!(super::a_chain_deeper_than_the_walk(ran_out), None);

        // Past it: the walk stops short and says so, which is what the bound's
        // own paragraph promises.
        let (ran_out, read) = walk(super::HOPS * 2);
        assert!(
            ran_out,
            "a chain of {} is past a bound of {} and the walk did not say it stopped \
             short — so what it read is part of the chain and it reports on that as \
             though it were all of it",
            super::HOPS * 2,
            super::HOPS
        );
        assert!(
            read < super::HOPS * 2,
            "the walk read the whole chain and still reported running out"
        );
        let said = super::a_chain_deeper_than_the_walk(ran_out).expect("a finding");
        assert!(
            said.what.contains(&format!("{} hops", super::HOPS)),
            "the finding does not say what the bound is: {}",
            said.what
        );

        // **And the budget is spent across calls, not restored between them.**
        // [`super::everything_ci_reaches`] calls the walk twice — once for what
        // the workflows reach and again once the justfiles have joined them —
        // and a fresh [`super::HOPS`] per call lets the walk really reach twice
        // the bound while the finding fires whenever *either* call runs out.
        // Two calls here, one budget: what the second sees is what the first
        // left.
        chain(super::HOPS * 2);
        let mut files = vec![scratch.join("ci/s01.sh")];
        let mut hops = super::HOPS;
        let first = super::follow_what_they_reach(&scratch, &mut files, &mut hops);
        let second = super::follow_what_they_reach(&scratch, &mut files, &mut hops);
        assert!(
            first && second,
            "a chain of {} did not exhaust a budget of {} over two calls",
            super::HOPS * 2,
            super::HOPS
        );
        assert_eq!(
            hops, 0,
            "the second call started from a fresh budget instead of continuing the \
             first's, so the walk reaches twice the bound this tree writes down"
        );

        // **And a reading new only by its role is more to find.** The walk
        // holds the justfile as CI's, through a link beside the workflows, and
        // a chain one past the bound ends by running it as a justfile: a
        // reading the walk would add. Asked whether it holds the file alone,
        // the walk says it read everything and that reading goes unjudged.
        chain(super::HOPS + 1);
        std::fs::write(
            scratch.join(format!("ci/s{:02}.sh", super::HOPS + 1)),
            "sh justfile\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("justfile"), "check:\n    cargo xtask build\n")
            .expect("writable");
        std::fs::create_dir_all(scratch.join(".github")).expect("writable");
        nc_platform::testing::link_file(&scratch.join("justfile"), &scratch.join(".github/j.yml"))
            .expect("a link is writable");
        let mut files = vec![scratch.join("ci/s01.sh"), scratch.join(".github/j.yml")];
        let mut hops = super::HOPS;
        assert!(
            super::follow_what_they_reach(&scratch, &mut files, &mut hops),
            "the walk spent its bound with the justfile still to be read as one, and \
             said it had read everything: {files:#?}"
        );
        drop(held);
    }

    /// A backslash inside single quotes is a backslash.
    ///
    /// **The arm for it, decided here.** Two of this reader's three
    /// single-quote rules are driven by the test below; without this one,
    /// deleting the third — the one that stops a `\` escaping inside `'…'`,
    /// which is what a shell does — leaves every test green. Without it the
    /// quote never closes, the `#` after it reads as quoted rather than as a
    /// comment, and the words a developer commented out are scanned as
    /// commands.
    #[test]
    fn a_backslash_inside_single_quotes_does_not_escape_the_quote() {
        assert_eq!(
            super::the_command_without_its_comment("echo 'a\\' # cargo clippy"),
            "echo 'a\\' ",
            "the `\\` inside single quotes escaped the quote that closes them, so the \
             comment after it was read as part of the command"
        );
        // And outside quotes it still escapes, which is the other half.
        assert_eq!(
            super::the_command_without_its_comment("echo a\\# b # real"),
            "echo a\\# b ",
            "an escaped `#` outside quotes was read as opening a comment"
        );
    }

    /// An escaped quote does not change the quote state, in all three readers.
    ///
    /// **Both clauses answer a measurement, and each has a case.** The
    /// backslash arm of `the_command_without_its_comment` answers
    /// `echo "a\"b # c" && cargo clippy --workspace` going into CI unseen; the
    /// same arm of `with_the_separators_spaced` answers a `cargo clippy` hidden
    /// behind an unspaced `;`. A clause written for a measurement and left
    /// undriven is a clause the next sweep deletes, and this suite is what stops
    /// that.
    #[test]
    fn an_escaped_quote_does_not_close_a_quote() {
        // The comment reader: the `#` is inside the string, so the whole line
        // survives and the `cargo clippy` behind it is read.
        let line = r#"echo "a\"b # c" && cargo clippy --workspace"#;
        assert_eq!(super::the_command_without_its_comment(line), line);
        // And a `#` that really does open a comment still cuts.
        assert_eq!(
            super::the_command_without_its_comment("cargo xtask check # and a note"),
            "cargo xtask check ",
        );
        // **And the third reader, which the other two name as their model**,
        // carries the correction they carry: `'a\'` is a backslash and a
        // closing quote, so the span ends and what follows is code. Measured
        // with a whole gate hidden behind it.
        assert_eq!(
            super::without_quoted_spans(r"echo 'a\' && cargo xtask docs"),
            r"echo '  ' && cargo xtask docs",
        );
        // **Outside a quote too.** A shell escapes with `\` wherever it
        // stands, so neither arm is limited to the quoted half: `echo
        // 'a\';cargo-clippy` closes nothing, so the `;` stays outside the quote
        // and is spaced, and the binary behind it is read.
        assert_eq!(
            super::with_the_separators_spaced(r"echo 'a\';cargo-clippy"),
            r"echo 'a\' ; cargo-clippy",
        );
        assert_eq!(
            super::the_command_without_its_comment(r"echo a\# b && cargo xtask check"),
            r"echo a\# b && cargo xtask check",
        );
        // The separator spacer: the `;` is outside the string, so it is spaced
        // and the word behind it becomes a command of its own.
        assert_eq!(
            super::with_the_separators_spaced(r#"echo "a\"b";cargo clippy"#),
            r#"echo "a\"b" ; cargo clippy"#,
        );
        // And a separator *inside* a string is still left alone.
        assert_eq!(
            super::with_the_separators_spaced(r#"echo "a;b""#),
            r#"echo "a;b""#,
        );
    }

    /// Every clause of the three workflow readers anyone could delete.
    ///
    /// **Three branches no other test decides** — the glob that spans
    /// directories, the empty `if:`, and the line that closes a heredoc — and
    /// removing all three leaves every other test green, so without this test
    /// `cargo xtask check` prints the same lines and the same test count. The
    /// last is the one that costs something: with it gone a heredoc never ends,
    /// so every command after the first one in the file stops being read, and
    /// `commands_in`'s own comment measures a heredoc wrapped around the
    /// determinism job defeating this gate.
    #[test]
    fn the_workflow_readers_decide_every_clause() {
        // `matching`: a `*` in the directory half is a pattern this cannot
        // answer, and answering it wrongly is worse than not answering.
        let scratch = std::env::temp_dir().join(format!("nc-glob-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("one/two")).expect("writable");
        std::fs::write(scratch.join("one/two/a.yml"), "").expect("writable");
        assert_eq!(
            super::matching(&scratch, "one/two/*.yml").len(),
            1,
            "a `*` in the name half is the form this reads"
        );
        // **A directory literally named `*`, because nothing else tells the
        // guard apart from its absence.** `one/*/a.yml` returns nothing either
        // way — `read_dir` simply fails on a path that does not exist — so a
        // case built from a plausible pattern leaves the guard as text. What
        // the guard actually decides is whether a `*` in the directory half is
        // a wildcard this refuses or a name it looks up, and the two answers
        // differ only where that name is real.
        //
        // **Where a name can hold a `*`.** Windows refuses one in any name, so
        // there no directory called `*` exists for the two readings to differ
        // over, in this scratch or in any checkout the reader walks; the clause
        // is held on the systems where the name is real.
        match std::fs::create_dir_all(scratch.join("*")) {
            Err(refused) if refused.kind() == std::io::ErrorKind::InvalidFilename => {}
            made => {
                made.expect("writable");
                std::fs::write(scratch.join("*/a.yml"), "").expect("writable");
                let left_over = super::matching(&scratch, "*/a.yml");
                assert!(
                    left_over.is_empty(),
                    "a `*` in the directory half is a pattern this does not expand, so it \
                     answers with nothing — and a reader that took it for a name would \
                     report a file under a directory no workflow ever wrote: {left_over:?}"
                );
            }
        }
        let left_over = super::matching(&scratch, "one/*/a.yml");
        assert!(
            left_over.is_empty(),
            "a `*` spanning directories is not a pattern this expands either: {left_over:?}"
        );
        let left_over = super::matching(&scratch, "no-slash-at-all");
        assert!(
            left_over.is_empty(),
            "a pattern with no directory is not one either: {left_over:?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);

        // `the_workflow_actually_runs`: a condition is reported, and `if:` with
        // nothing after it is not a condition.
        let conditions = |ci: &str| {
            let mut findings = Vec::new();
            super::the_workflow_actually_runs(ci, &mut findings);
            findings
                .iter()
                .filter(|finding| finding.what.contains("runs only when"))
                .count()
        };
        assert_eq!(
            conditions("jobs:\n  build:\n    if: github.event_name == 'never'\n"),
            1,
            "a condition on a job is what this reports"
        );
        // **The step-level spellings, which are the ones a step actually
        // carries.** A reader matching `if:` at the start of a trimmed line
        // misses a step's condition, written `- if: …`, so the one place a
        // condition does the most damage — the step that runs the gate — is
        // outside it.
        for spelling in [
            "- if: false",
            "\"if\": false",
            "'if': false",
            "- \"if\": false",
        ] {
            assert_eq!(
                conditions(&format!("jobs:\n  build:\n    steps:\n      {spelling}\n")),
                1,
                "`{spelling}` is a condition and was read as none"
            );
        }
        assert_eq!(
            conditions("jobs:\n  build:\n    if:\n"),
            0,
            "`if:` with nothing after it states no condition, and reporting it would \
             print a finding whose own sentence names nothing"
        );

        // `commands_in`: the heredoc ends at its tag, and what follows is read.
        let body = "cat <<'PLAN' >/dev/null\ncargo xtask nonsense\nPLAN\ncargo xtask build\n";
        let commands = super::commands_in(body);
        assert!(
            commands.contains(&"cargo xtask build".to_owned()),
            "the heredoc did not end at its tag, so everything after it went unread: \
             {commands:?}"
        );
        assert!(
            !commands.contains(&"cargo xtask nonsense".to_owned()),
            "text written into a heredoc is data, not a command: {commands:?}"
        );
        // The three spellings of the tag, because the quotes and the `-` are
        // stripped in an order that reading cannot settle: `PLAN'` is the tag
        // the wrong order produces for the first of them.
        for opener in ["cat <<'PLAN' >/dev/null", "cat <<PLAN", "cat <<-\"PLAN\""] {
            let commands = super::commands_in(&format!("{opener}\nin the block\nPLAN\nafter\n"));
            assert_eq!(
                commands.last().map(String::as_str),
                Some("after"),
                "`{opener}` did not close at `PLAN`: {commands:?}"
            );
        }
    }

    use super::{
        JOBS_MUST_RUN, SECOND_CHECKOUT, WORKFLOW, first_words, is_label, job_body,
        only_runs_the_gate,
    };
    use std::path::Path;

    /// Gives a scratch tree the `cargo xtask` alias, which every fixture needs.
    ///
    /// `only_runs_the_gate` reads what `cargo xtask` resolves to, so a tree
    /// without the alias is a finding of its own — correct, and not what any of
    /// these fixtures is about.
    fn the_shipped_alias(scratch: &Path) {
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::write(
            scratch.join(super::ALIAS),
            "[alias]\nxtask = \"run --quiet --locked --package xtask --\"\n",
        )
        .expect("writable");
    }

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    /// What CI has to supply before the gate, decided case by case.
    ///
    /// **Every finding this rule can make, reached.** Each other scratch fixture
    /// in this module hits `ci_supplies_what_the_gate_needs`'s early return —
    /// there is no `docker-compose.yml` beside them — so without this the whole
    /// function is exercised only through
    /// `this_repositorys_workflow_runs_only_the_gate`, which asserts
    /// `findings.is_empty()` on the real tree and therefore cannot notice a rule
    /// being *removed*. Measured: the body replaced with an immediate return
    /// left `cargo test --locked --workspace` green in every suite, and
    /// `ci.yml` reordered to run `cargo xtask dev` *after* `cargo xtask check`
    /// then gave `check passed`.
    ///
    /// This rule answers the two defects `ci.yml` describes at length — a
    /// `services:` block instead of the stack, and nothing migrating the
    /// database — so it is the rule most worth not losing.
    #[test]
    fn ci_that_does_not_supply_the_stack_is_a_finding() {
        let scratch = std::env::temp_dir().join(format!("nc-ci-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::write(
            scratch.join("docker-compose.yml"),
            "services:\n  postgres:\n    image: postgres:18-alpine@sha256:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90\n  logs:\n    image: victoriametrics/victoria-logs:v1.0.0\n",
        )
        .expect("writable");

        let job = |steps: &str| {
            format!("jobs:\n  linux:\n    runs-on: ubuntu-latest\n    steps:\n{steps}")
        };
        let up = "      - run: cargo xtask dev\n";
        let gate = "      - run: cargo xtask check\n";
        // A system job with no stack beside a `linux` job that is right, and one
        // with a stack brought up on a runner that has none.
        let beside = |system: &str, steps: &str| {
            format!(
                "{}  {system}:\n    runs-on: {system}-latest\n    steps:\n{steps}",
                job(&format!("{up}{gate}"))
            )
        };

        for (what, ci, expected) in [
            (
                "the stack, then the gate",
                job(&format!("{up}{gate}")),
                None,
            ),
            (
                "the gate on macOS with no stack",
                beside("macos", gate),
                None,
            ),
            (
                "the gate on Windows with no stack",
                beside("windows", gate),
                None,
            ),
            (
                "a stack brought up on macOS",
                beside("macos", &format!("{up}{gate}")),
                Some("the `macos` job brings the stack up, and only Linux runs one"),
            ),
            (
                "a stack brought up on Windows",
                beside("windows", &format!("{up}{gate}")),
                Some("the `windows` job brings the stack up, and only Linux runs one"),
            ),
            (
                "the gate with no stack",
                job(gate),
                Some("the `linux` job runs the gate without bringing the stack up"),
            ),
            (
                "a stack and no gate",
                job(up),
                Some("the `linux` job brings the stack up and never runs the gate"),
            ),
            (
                "the stack after the gate",
                job(&format!("{gate}{up}")),
                Some("the `linux` job brings the stack up after running the gate"),
            ),
            (
                "no linux job at all",
                "jobs:\n  other:\n    steps:\n      - run: cargo xtask check\n".to_owned(),
                Some("there is no `linux` job to bring the stack up in"),
            ),
            (
                "a service block naming the tag, not the digest",
                format!(
                    "jobs:\n  linux:\n    runs-on: ubuntu-latest\n    services:\n      \
                     db:\n        image: postgres:18-alpine\n    steps:\n{up}{gate}"
                ),
                Some("CI names `postgres:18-alpine` itself instead of bringing the stack up"),
            ),
            (
                "a service block instead of the stack",
                format!(
                    "jobs:\n  linux:\n    runs-on: ubuntu-latest\n    services:\n      \
                     db:\n        image: postgres:18-alpine\n    steps:\n{up}{gate}"
                ),
                Some("CI names `postgres:18-alpine` itself instead of bringing the stack up"),
            ),
        ] {
            let mut findings = Vec::new();
            super::ci_supplies_what_the_gate_needs(&scratch, &ci, &mut findings);
            match expected {
                None => assert!(
                    findings.is_empty(),
                    "`{what}` is what CI is supposed to do and the rule refused it: \
                     {findings:#?}"
                ),
                Some(said) => {
                    assert!(
                        findings.iter().any(|finding| finding.what == said),
                        "`{what}` produced no finding saying `{said}`: {findings:#?}"
                    );
                }
            }
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn the_identity_reports_a_walk_that_lost_a_line() {
        let mut said = Vec::new();
        super::the_identity(11, 9, &mut said);
        assert!(
            said.iter()
                .any(|finding| finding.what.contains("reached a verdict")),
            "eleven became this gate's to judge and nine reached a verdict, and the \
             identity said nothing: {said:#?}"
        );
        let mut agreed = Vec::new();
        super::the_identity(11, 11, &mut agreed);
        assert!(
            agreed.is_empty(),
            "the counters agreed and it spoke: {agreed:#?}"
        );
    }

    #[test]
    fn the_second_count_notices_a_filter_in_the_line_loop() {
        // **`a_second_count_of` and `the_walk_is_not_short`, decided here.**
        // They exist because "the two counters inside the command loop cannot
        // see a filter in the loop around them", and with no test here,
        // `if seen == expected { return; }` replaced by `if true { return; }`
        // leaves every test green.
        let files = [std::path::PathBuf::from("probe.yml")];
        let mut said = Vec::new();
        super::the_walk_is_not_short(9, 12, &mut said);
        assert!(
            said.iter()
                .any(|finding| finding.what.contains("second count of the same files")),
            "the walk judged 9 where a second count found 12 and nothing said so: {said:#?}"
        );

        let mut agreed = Vec::new();
        super::the_walk_is_not_short(12, 12, &mut agreed);
        assert!(
            agreed.is_empty(),
            "the two counts agreed and the floor spoke anyway: {agreed:#?}"
        );

        // **And the direction that says the *second count* broke.** With only
        // `seen < expected` driven, the comparison can be widened to `>=` with
        // the whole suite green — and the comment in `a_second_count_of` names
        // a workflow that makes `seen` exceed that count. The twin comparisons
        // in `references.rs` and `sections.rs` carry exactly this fixture.
        let mut over = Vec::new();
        super::the_walk_is_not_short(13, 12, &mut over);
        assert!(
            !over.is_empty(),
            "a walk that judged more than a second count of the same files was read as \
             agreement, which is the state that says the second count is broken"
        );
        // And the second count reads what it is given: a file that does not
        // exist contributes nothing rather than panicking.
        assert_eq!(super::a_second_count_of(&files), 0);
    }

    #[test]
    fn this_repositorys_workflow_runs_only_the_gate() {
        let (findings, seen) = only_runs_the_gate(root()).expect("the workflow is readable");
        assert!(findings.is_empty(), "{findings:#?}");
        // The floor: a workflow that stopped invoking cargo at all would produce
        // no findings and prove nothing.
        assert!(seen >= 2, "only {seen} cargo command(s) were inspected");
    }

    /// The walk reads each file once, however many spellings reach it.
    ///
    /// **A walk that compares spellings counts one tree three ways**: on one
    /// tree such a walk prints `ok 19 cargo command(s)` on Linux, `31` on macOS
    /// and `41` on Windows. A filesystem that ignores case answers for every
    /// name `just` loads at once, so such a walk reads the one justfile three
    /// times; and Windows makes a directory of `cd "$(dirname "$0")/.."`, the
    /// root under a second spelling, through which it reads every file a word
    /// reaches once more, which by the count is the ten past macOS, inferred.
    /// Linux's filesystem holds neither shape, so the scratch below plants each
    /// in a form every system reads alike: a justfile only a lookup without
    /// case finds, a `cd` into a directory and back out of it, and a `cd`
    /// through a name that is no directory, which POSIX's `cd` refuses and
    /// which a path handed to Windows whole resolves.
    ///
    /// **And a step back across `.github/` is no second place**, in either
    /// direction: a script under it reached from a step into another directory
    /// and back out, and the justfile reached from a step into `.github/` and
    /// back out, are each read once, in the role of where they are.
    #[test]
    fn the_walk_reads_each_file_once_however_it_is_reached() {
        let (files, _) = super::everything_ci_reaches(root()).expect("the walk runs");
        let mut distinct: Vec<(std::path::PathBuf, (bool, bool))> = files
            .iter()
            .map(|path| {
                (
                    std::fs::canonicalize(path).expect("a file the walk found resolves"),
                    super::the_role_of(&super::the_place_of(root(), path)),
                )
            })
            .collect();
        distinct.sort();
        distinct.dedup();
        assert_eq!(
            distinct.len(),
            files.len(),
            "the walk holds one file in one role under two spellings, and counts its \
             commands twice: {files:#?}"
        );
        let (_, seen) = only_runs_the_gate(root()).expect("the workflow is readable");
        let readings: Vec<std::path::PathBuf> =
            distinct.into_iter().map(|(file, _)| file).collect();
        assert_eq!(
            seen,
            super::a_second_count_of(&readings),
            "the walk's count is not the count over the distinct readings it reaches"
        );

        let held = crate::check::LeavesNothingBehind::new("workflow-spellings");
        let scratch = &held.root;
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join(".github/scripts")).expect("writable");
        std::fs::create_dir_all(scratch.join("ci")).expect("writable");
        std::fs::create_dir_all(scratch.join("tools")).expect("writable");
        std::fs::write(
            scratch.join(super::WORKFLOW),
            "on: push\njobs:\n  check:\n    steps:\n      - run: cd ci/.. && sh ci/gate.sh\n      \
             - run: cd missing/../ci && sh elsewhere.sh\n      \
             - run: cd tools && sh ../.github/scripts/docs.sh\n      \
             - run: cd .github && sh ../JuStFiLe\n      - run: cargo xtask check\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("ci/gate.sh"), "cargo fmt --all --check\n").expect("writable");
        std::fs::write(
            scratch.join(".github/scripts/docs.sh"),
            "cargo doc --workspace\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("ci/elsewhere.sh"), "cargo test --workspace\n")
            .expect("writable");
        std::fs::write(
            scratch.join("JuStFiLe"),
            "lint:\n    cargo clippy --workspace\n",
        )
        .expect("writable");
        let (findings, seen) = only_runs_the_gate(scratch).expect("the scratch tree reads");
        // By the file's last name, so a second spelling of its path is
        // counted with the first rather than beside it.
        let about = |name: &str| {
            findings
                .iter()
                .filter(|said| said.file.rsplit('/').next() == Some(name))
                .count()
        };
        assert_eq!(
            about("gate.sh"),
            1,
            "a script reached through the root and through a `cd` into `ci` and back was \
             not reported once: {findings:#?}"
        );
        assert_eq!(
            about("elsewhere.sh"),
            0,
            "the walk stepped back out of a name that is no directory, which POSIX's `cd` \
             refuses: {findings:#?}"
        );
        assert_eq!(
            about("JuStFiLe"),
            1,
            "a justfile `just` loads was not reported once, under the name it has, nor \
             once more as a workflow's file through a step into `.github/` and back: \
             {findings:#?}"
        );
        assert_eq!(
            about("docs.sh"),
            1,
            "a script under `.github/` reached through a step into another directory and \
             back was not reported once: {findings:#?}"
        );
        assert_eq!(
            seen, 4,
            "four cargo commands, each counted once: {findings:#?}"
        );
        drop(held);
    }

    /// A file reached as one of CI's own, as a script and as a justfile is
    /// judged in each of the three roles.
    ///
    /// **Through links, as a committed tree can hold them.** The `.github/`
    /// walk meets the root `justfile` first through a link beside the
    /// workflows, as a workflow's file, where a recipe has no name to hold; a
    /// step's word reaches it through a second link, as a script with no
    /// recipes; and the join of what `just` loads reaches it last, as
    /// `justfile`, where `check:` running `cargo xtask build` is a recipe that
    /// does not run what it is called. Compared by the file alone, the
    /// justfile is read as a workflow and nothing else; compared by either half
    /// of the role alone, one of the three readings is dropped and the count
    /// says so.
    ///
    /// **Made as every suite here makes a file link**, through
    /// `nc_platform::testing::link_file`, which the refusal twins of `nc-local`
    /// and of the blob directory make on every system's job.
    #[test]
    fn a_file_reached_in_three_roles_is_judged_in_each() {
        let held = crate::check::LeavesNothingBehind::new("workflow-roles");
        let scratch = &held.root;
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join("ci")).expect("writable");
        std::fs::write(
            scratch.join(super::WORKFLOW),
            "on: push\njobs:\n  check:\n    steps:\n      - run: sh ci/lint\n      \
             - run: cargo xtask check\n",
        )
        .expect("writable");
        let justfile = scratch.join("justfile");
        std::fs::write(&justfile, "check:\n    cargo xtask build\n").expect("writable");
        for link in [".github/justfile.yml", "ci/lint"] {
            nc_platform::testing::link_file(&justfile, &scratch.join(link))
                .expect("a link is writable");
        }
        let (findings, seen) = only_runs_the_gate(scratch).expect("the scratch tree reads");
        let about = |name: &str| {
            findings
                .iter()
                .filter(|said| said.file == name)
                .collect::<Vec<_>>()
        };
        let recipe = about("justfile");
        assert!(
            recipe.len() == 1 && recipe[0].fix.contains("`build` is what this body runs"),
            "the justfile was not judged as one, under the name `just` loads it by: \
             {findings:#?}"
        );
        assert!(
            about(".github/justfile.yml").is_empty() && about("ci/lint").is_empty(),
            "the same file read as a workflow's or as a script was judged as a justfile: \
             {findings:#?}"
        );
        assert_eq!(
            seen, 4,
            "the gate, and the recipe once in each of three roles: {findings:#?}"
        );
        drop(held);
    }

    /// A spelling's place has no step back in it, and keeps the links it
    /// went through.
    ///
    /// **Both halves of [`super::the_place_of`], driven.** A step into a
    /// directory and back out names where it began, across `.github/` in
    /// either direction. A link to a directory is a place of its own, so a
    /// justfile the `.github/` walk finds through one is read as CI's, as the
    /// walk finds it. And a step back out of that link names what the system
    /// resolves, which taking the step out as written does not, on a system
    /// that follows the link first.
    ///
    /// **The last is asked as the property it is**: a place with no step back
    /// in it, naming the file the spelling names. Windows takes such a step
    /// where the spelling puts it, and Linux and macOS where the link points,
    /// so the two places differ and each is right on its own system.
    #[test]
    fn a_place_has_no_step_back_and_keeps_its_links() {
        let held = crate::check::LeavesNothingBehind::new("workflow-places");
        let scratch = &held.root;
        for directory in [".github/scripts", "ci", "sub"] {
            std::fs::create_dir_all(scratch.join(directory)).expect("writable");
        }
        for file in [
            ".github/scripts/docs.sh",
            ".github/x",
            "justfile",
            "sub/justfile",
            "x",
        ] {
            std::fs::write(scratch.join(file), file).expect("writable");
        }
        nc_platform::testing::link_directory(&scratch.join("sub"), &scratch.join(".github/d"))
            .expect("a link is writable");
        let place = |spelled: &str| super::the_place_of(scratch, &scratch.join(spelled));
        assert_eq!(
            place("ci/../.github/scripts/docs.sh"),
            ".github/scripts/docs.sh",
            "a step into a directory and back out is a second place for a file under \
             `.github/`"
        );
        assert_eq!(
            place(".github/../justfile"),
            "justfile",
            "a step into `.github/` and back out makes the justfile one of CI's files"
        );
        assert_eq!(
            super::the_role_of(&place(".github/d/justfile")),
            (true, true),
            "a justfile found through a link under `.github/` lost the place it was \
             found in"
        );
        let out_of_the_link = place(".github/d/../x");
        assert!(
            out_of_the_link.split('/').all(|name| name != "..")
                && super::the_file_itself(&scratch.join(&out_of_the_link))
                    == super::the_file_itself(&scratch.join(".github/d/../x")),
            "a step back out of a link is placed at `{out_of_the_link}`, which is not \
             where the system takes it"
        );
        drop(held);
    }

    /// The seven attacks measured against `yq 3.4.3`.
    ///
    /// Their own function because the table below is at clippy's line bound,
    /// and because they are one family: each is a spelling YAML says is one
    /// thing and a line reader reads as another — a second lints gate behind a
    /// double-quoted scalar's own `\n`, the same break written `\x0a`, and a
    /// block header wearing a comment.
    /// <!-- allow-logging: `the_attacks_yaml_spells_seven_ways` hands back workflow fragments written as literals in this file, and nothing a caller chose -->
    fn the_attacks_yaml_spells_seven_ways()
    -> [(&'static str, &'static str, &'static str, &'static str); 7] {
        [
            // **Three, each measured against `yq 3.4.3`.** A second
            // lints gate can hide behind a double-quoted scalar's own `\n`,
            // behind the same break spelled `\x0a`, and behind a block header
            // wearing a comment — all three of which YAML says are one thing
            // and a line reader reads as another.
            (
                "a second lints gate behind a double-quoted scalar's own newline",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: \"true\\ncargo-clippy --workspace --all-targets -- -D warnings\"\n      \
                 - run: cargo xtask check",
            ),
            (
                "the same break spelled as a hex escape",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: \"true\\x0acargo-clippy --workspace --all-targets -- -D warnings\"\n      \
                 - run: cargo xtask check",
            ),
            (
                "a second lints gate under a block header wearing a comment",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: | # the gate, and the lints beside it\n          cargo xtask check\n          \
                 cargo-clippy --workspace --all-targets -- -D warnings",
            ),
            // **Four more, each a two-line scalar to `yq 3.4.3`.** A reader that
            // asks the value both start and end with `"` leaves the gate with
            // nothing at all to say behind a trailing comment, a tag, an
            // anchor, or a hash with no space before it.
            (
                "the same newline, behind a comment after the scalar",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: \"true\\ncargo-clippy --workspace --all-targets -- -D warnings\"  # the gate\n      \
                 - run: cargo xtask check",
            ),
            (
                "the same newline, behind a tag",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: !!str \"true\\ncargo-clippy --workspace --all-targets -- -D warnings\"\n      \
                 - run: cargo xtask check",
            ),
            (
                "the same newline, behind an anchor",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: &gate \"true\\ncargo-clippy --workspace --all-targets -- -D warnings\"\n      \
                 - run: cargo xtask check",
            ),
            (
                "the same newline, with no space before the comment",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: \"true\\ncargo-clippy --workspace --all-targets -- -D warnings\"#the gate\n      \
                 - run: cargo xtask check",
            ),
        ]
    }

    /// Every way of throwing the verdict away that this gate has met.
    ///
    /// Its own function because with the two escaped-quote rows the list passes
    /// clippy's hundred-line bound, which is the same split
    /// `the_scripts_both_stores_answer` records in `nc-journal`.
    /// <!-- allow-logging: `the_attacks_this_gate_survives` hands back the names, paths and workflow fragments written as literals in this file, and nothing a caller chose -->
    fn the_attacks_this_gate_survives()
    -> [(&'static str, &'static str, &'static str, &'static str); 14] {
        [
            // A variadic parameter is declared with a sigil, and the sigil is
            // not part of the name the body writes. The gate must still refuse
            // an *undeclared* one, which is what this row asks: the recipe
            // declares `+flags` and the body writes `{{other}}`.
            (
                "a name no sigil declares",
                "justfile",
                "\nrelease version:\n",
                "\nrelease +flags:\n    cargo xtask check {{other}}\n",
            ),
            // **A `just` function that reads the environment.** An excuse for
            // every interpolation ending in a parenthesis, so that
            // `{{justfile_directory()}}` passes, lets `env_var` through too,
            // and with it the gate a developer types is whatever the
            // environment says. The second row is the control that says the
            // rule fires on the ordinary spelling as well, so neither is
            // green for the other's reason.
            (
                "`{{env_var(\"FLAGS\")}}` in the gate's own recipe",
                "justfile",
                "\n    cargo xtask check\n",
                "\n    cargo xtask check {{env_var(\"FLAGS\")}}\n",
            ),
            (
                "`{{flags}}` in the gate's own recipe",
                "justfile",
                "\n    cargo xtask check\n",
                "\n    cargo xtask check {{flags}}\n",
            ),
            // **Behind an escaped quote, which spends the rest of the line.**
            // To a `without_quoted_spans` that knows `'` and `"` and nothing of
            // `\`, a step ending `echo \"` opens a span that never closes and
            // every needle below then searches a line of blanks. Measured: five
            // spellings of *throw the verdict away* survived behind one, the
            // whole xtask suite stayed green, and through the gate the step
            // count went **up** while the `||` finding disappeared.
            (
                "`|| true` behind an escaped quote",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: echo \\\" && cargo xtask check || true",
            ),
            (
                "`&` behind an escaped quote",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: echo \\\" && cargo xtask check &",
            ),
            (
                "`|| :`, the shell's other no-op",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: cargo xtask check || :",
            ),
            (
                "`set +e` at the head of a block",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: |\n          set +e\n          cargo xtask check",
            ),
            (
                "a tag filter, which `FILTERS` did not list",
                super::WORKFLOW,
                "\n  push:\n",
                "\n  push:\n    tags: ['v*']\n",
            ),
            (
                "a `types:` list on the pull request",
                super::WORKFLOW,
                "\n  pull_request:\n",
                "\n  pull_request:\n    types: [labeled]\n",
            ),
            (
                "`|| true` in the surface a developer types",
                "justfile",
                "\n    cargo xtask check\n",
                "\n    cargo xtask check || true\n",
            ),
            (
                "`set +o errexit`, which is `set +e` written the long way",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: |\n          set +o errexit\n          cargo xtask check\n          true",
            ),
            (
                "a second lints gate through the clippy driver rustup ships",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: cargo-clippy --workspace --all-targets -- -D warnings\n      \
                 - run: cargo xtask check",
            ),
            (
                "the same driver one word past command position, behind `bash -c`",
                super::WORKFLOW,
                "run: cargo xtask check",
                "run: bash -c \"cargo-clippy --workspace -- -D warnings\"\n      \
                 - run: cargo xtask check",
            ),
            (
                "and in the justfile, which is the surface a developer types",
                "justfile",
                "\n    cargo xtask check\n",
                "\n    cargo xtask check\n    sh -c \"cargo-fmt --all -- --check\"\n",
            ),
        ]
    }

    #[test]
    /// Every shape that makes the gate's answer not count, on the real files.
    ///
    /// **The first five pass a one-file gate at once.** Each mutation below,
    /// applied to this repository's own `ci.yml` or `justfile`, leaves the whole
    /// of `cargo xtask check` ending `check passed`, with the cargo count
    /// unchanged, when `NEUTERED` is a list of spellings read in one file —
    /// every one of these is a spelling not on such a list or a file it does
    /// not read.
    ///
    /// Written against copies of the shipped files rather than against a
    /// hand-made fragment, because four of those five get through by being
    /// *somewhere else* than the fragment a test would have built.
    fn no_way_of_discarding_the_verdict_survives_this_gate() {
        let scratch =
            std::env::temp_dir().join(format!("nc-workflow-neutered-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);
        let ci = std::fs::read_to_string(root().join(super::WORKFLOW)).expect("the workflow");
        let justfile = std::fs::read_to_string(root().join("justfile")).expect("the justfile");

        // `(what is changed, the file, the text before, the text after)`.
        let attacks: Vec<(&str, &str, &str, &str)> = the_attacks_this_gate_survives()
            .into_iter()
            .chain(the_attacks_yaml_spells_seven_ways())
            .collect();

        for (what, file, before, after) in attacks {
            let (ci_text, just_text) = if file == "justfile" {
                assert!(justfile.contains(before), "{what}: the justfile has moved");
                (ci.clone(), justfile.replacen(before, after, 1))
            } else {
                assert!(ci.contains(before), "{what}: the workflow has moved");
                (ci.replacen(before, after, 1), justfile.clone())
            };
            std::fs::write(scratch.join(super::WORKFLOW), &ci_text).expect("writable");
            std::fs::write(scratch.join("justfile"), &just_text).expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                !findings.is_empty(),
                "{what} left the gate with nothing to say"
            );
            assert!(
                findings.iter().any(|finding| finding.file == file),
                "{what} was reported, but not against {file}: {findings:#?}"
            );
            // **And reported as what it is, not as the gate disagreeing with
            // itself.** `the_walk_is_not_short` fires whenever the line loop and
            // the second count differ, and its message is about this tool's
            // internals. Measured: a reader that misreads YAML answers three
            // real attacks that way — a second lints gate behind a double-quoted
            // scalar's own `\n`, the same break spelled `\x0a`, and a block
            // header wearing a comment — so without this assertion the suite is
            // green, a finding is produced, and what it says is that the
            // counters disagree. A
            // developer reads that as noise about the tool and scrolls past the
            // gate that is hidden.
            assert!(
                !findings.iter().any(super::is_about_this_gate),
                "{what} was answered with this gate's own accounting rather than with \
                 what the attack does — so the attack itself went unreported: \
                 {findings:#?}"
            );
        }

        the_ways_measured_after_the_first_five(&scratch, &ci, &justfile);
        the_two_ways_an_ampersand_hides_a_gate(&scratch, &ci, &justfile);

        // And the two files as they ship, through the same scratch path, so a
        // gate that reported everything would fail here rather than pass above.
        std::fs::write(scratch.join(super::WORKFLOW), &ci).expect("writable");
        std::fs::write(scratch.join("justfile"), &justfile).expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The two spellings of `&` that hide a gate.
    ///
    /// **A separate function because the one below would pass a hundred
    /// lines**, and because it drives its cases rather than returning them: a
    /// helper that hands back a `&'static str` it was not given is a door into
    /// the log's type wall, which this repository refuses on purpose.
    ///
    /// `&` ends a command in every shell, and with it missing from this file's
    /// separator lists and from the backgrounding rule, a second lints gate, and
    /// a gate run in the background that nobody waits for, each leave `ok 12
    /// cargo command(s)` unmoved and `check passed`.
    fn the_two_ways_an_ampersand_hides_a_gate(scratch: &Path, ci: &str, justfile: &str) {
        let second_gate = "cargo fmt --all --check\ncargo clippy --workspace\n";
        for (what, names, contents) in [
            (
                "`&` before something else, which backgrounds the gate just the same",
                "sent to the background",
                ci.replacen(
                    "run: cargo xtask check",
                    "run: cargo xtask check & echo started",
                    1,
                ),
            ),
            (
                "a second gate after `&`, which ends a command in every shell",
                "and it is not the gate",
                ci.replacen(
                    "run: cargo xtask check",
                    "run: cargo xtask check\n      - name: lints\n        run: true & cargo-clippy --workspace --all-targets",
                    1,
                ),
            ),
        ] {
            std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
            std::fs::write(scratch.join("scripts/gate.sh"), second_gate).expect("writable");
            std::fs::write(scratch.join("justfile"), justfile).expect("writable");
            std::fs::write(scratch.join(super::WORKFLOW), &contents).expect("writable");
            let (findings, _) = only_runs_the_gate(scratch).expect("readable");
            // **The finding this case is about, not any finding.** Asserting
            // only that the gate says *something* lets the second case pass with
            // `&` taken out of `separates_commands`: what it catches is the
            // backgrounding rule, which fires on the same line for a different
            // reason. The sibling loop ten lines up asserts the file too, and an
            // assertion written narrower the second time is this file's own
            // signature defect.
            assert!(
                findings.iter().any(|finding| finding.what.contains(names)),
                "{what} left the gate with nothing to say about it: {findings:?}"
            );
            // **And the third helper asks it too.** `ABOUT_THIS_GATE`'s own
            // paragraph says `is_about_this_gate` "is the one predicate all
            // three attack helpers ask", and this is the third. The clause
            // above names the command, so this loop cannot be answered by the
            // counters alone today; it is asserted rather than argued, because
            // that is what the paragraph claims.
            assert!(
                !findings.iter().any(super::is_about_this_gate),
                "{what} was answered with this gate's own accounting rather than with \
                 what the attack does: {findings:?}"
            );
        }
        let _ = std::fs::remove_dir_all(scratch.join("scripts"));
    }

    /// The ways past a gate that closes only the first five.
    ///
    /// Split out because the test would pass a hundred lines. Every one of these
    /// leaves the cargo count unchanged, or takes the justfile out of the walk
    /// entirely and lowers it.
    fn the_ways_measured_after_the_first_five(scratch: &Path, ci: &str, justfile: &str) {
        // **More, each measured passing a gate that closes the first five.**
        // Every one leaves the cargo count unchanged, or takes the justfile out
        // of the walk entirely and lowers it.
        // `(what, the file it goes in, its contents)`.
        let more = the_ways_this_gate_has_been_beaten_since(scratch, ci, justfile);
        let second_gate = "cargo fmt --all --check\ncargo clippy --workspace\n";
        for (what, file, contents) in more {
            let _ = std::fs::remove_file(scratch.join("Justfile"));
            // The script exists for every case: what is under test is whether
            // the walk *opens* it, not whether a name resolves.
            std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
            std::fs::write(scratch.join("scripts/gate.sh"), second_gate).expect("writable");
            std::fs::write(scratch.join(super::WORKFLOW), ci).expect("writable");
            std::fs::write(scratch.join("justfile"), justfile).expect("writable");
            std::fs::write(scratch.join(file), &contents).expect("writable");
            let (findings, _) = only_runs_the_gate(scratch).expect("readable");
            assert!(
                !findings.is_empty(),
                "{what} left the gate with nothing to say"
            );
            // **Every attack this helper carries is held by this one line.** At the
            // bare non-empty check this loop accepts an answer about this gate's own
            // arithmetic: measured with that check alone, `Justfile` filtered
            // out of the walk above the counters leaves the whole suite green,
            // that attack answered only by the counters disagreeing. A third
            // copy of a rule left narrower than the second is this file's
            // signature defect.
            assert!(
                !findings.iter().any(super::is_about_this_gate),
                "{what} was answered with this gate's own accounting rather than with \
                 what the attack does — so the attack itself went unreported: \
                 {findings:#?}"
            );
            // **And not "against the planted file", which these attacks are
            // not about.** Five of them are transitive: the workflow is what
            // carries the plant and the script it reaches is where the second
            // gate is reported, which is the right answer, and an assertion
            // copied from the sibling loop refuses it — measured.
        }
        let _ = std::fs::remove_file(scratch.join("Justfile"));
        let _ = std::fs::remove_dir_all(scratch.join("scripts"));
    }

    /// Nine ways past a gate that closes only the first five.
    ///
    /// Its own function because with the two-shape row — a line that is an
    /// assignment *and* a cargo command, which a walk that `continue`s counts
    /// once and the second count counts twice — the list passes clippy's
    /// hundred-line bound.
    /// <!-- allow-logging: `the_ways_this_gate_has_been_beaten_since` hands back workflow fragments written as literals in this file, and nothing a caller chose -->
    fn the_ways_this_gate_has_been_beaten_since(
        scratch: &Path,
        ci: &str,
        justfile: &str,
    ) -> [(&'static str, &'static str, String); 9] {
        let scripted = |recipe: &str, script: &str| {
            std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
            std::fs::write(scratch.join("scripts/gate.sh"), script).expect("writable");
            justfile.replacen("\n    cargo xtask check\n", recipe, 1)
        };
        let second_gate = "cargo fmt --all --check\ncargo clippy --workspace\n";
        [
            (
                "`&` — the gate runs in the background and nobody waits",
                super::WORKFLOW,
                ci.replacen("run: cargo xtask check", "run: cargo xtask check &", 1),
            ),
            (
                "`shell: bash {0}`, which drops the `-e` every other rule assumes",
                super::WORKFLOW,
                ci.replacen(
                    "run: cargo xtask check",
                    "shell: bash {0}\n        run: |\n          cargo xtask check\n          true",
                    1,
                ),
            ),
            (
                "`--offline`, which is the gate minus every step that needs the stack",
                super::WORKFLOW,
                ci.replacen(
                    "run: cargo xtask check",
                    "run: cargo xtask check --offline",
                    1,
                ),
            ),
            (
                // **The spelling that needs no marker.** A YAML plain scalar
                // continues onto every line indented further than its key, so
                // this is `cargo xtask check --offline` to every parser there
                // is — `yq` reads it that way — while a per-line reader sees a
                // gate and a stray word. Measured without `joined_continuations`
                // reading it: `12 cargo command(s)` unmoved, no finding, and
                // `check passed`. The two spellings above needed a `\\` or a
                // `>`; this one needs nothing.
                "`--offline` on a plain scalar's second line",
                super::WORKFLOW,
                ci.replacen(
                    "run: cargo xtask check",
                    "run: cargo xtask check\n          --offline",
                    1,
                ),
            ),
            // **A line that is two shapes at once.** `CARGO=cargo; cargo
            // clippy …` is an assignment *and* a cargo command. With a
            // `continue` in each of the walk's arms, it counts one where the
            // second count counts two — and the clippy half, a real second
            // gate, is never reported.
            (
                "a second gate on the same line as the assignment that names cargo",
                super::WORKFLOW,
                ci.replacen(
                    "run: cargo xtask check",
                    "run: |\n          cargo xtask check\n          \
                     CARGO=cargo; cargo clippy --workspace --all-targets -- -D warnings",
                    1,
                ),
            ),
            (
                "a second gate reached through a shell variable",
                super::WORKFLOW,
                ci.replacen(
                    "run: cargo xtask check",
                    "run: |\n          S=scripts/gate.sh\n          cargo xtask check\n          bash $S",
                    1,
                ),
            ),
            (
                "a justfile recipe shelling out to a script the walk never opened",
                "justfile",
                scripted("\n    cargo xtask check\n    bash scripts/gate.sh\n", second_gate),
            ),
            (
                "a justfile that `import`s the file its recipes moved to",
                "justfile",
                scripted(
                    "\n    cargo xtask check\n",
                    second_gate,
                )
                .replacen("# Convenience", "import 'scripts/gate.sh'\n\n# Convenience", 1),
            ),
            (
                "the justfile renamed to another name `just` loads",
                "Justfile",
                justfile.replacen(
                    "\n    cargo xtask check\n",
                    "\n    cargo fmt --all --check\n    cargo clippy --workspace\n",
                    1,
                ),
            ),
        ]
    }

    /// A cargo configuration above the repository is found and reported.
    ///
    /// **The ancestor walk, driven both ways.** Held by
    /// `assert!(findings.is_empty())` alone, deleting the whole block leaves
    /// every test in this crate green — and with it deleted, a
    /// `.cargo/config.toml` one directory up carrying
    /// `[build] rustflags = ["--cap-lints=allow"]` gives `ok lints` over
    /// a real `needless_range_loop`, with every walk's count up and
    /// `check passed`. In the real layout that parent is the workspace's own git
    /// repository, so the file is committable.
    #[test]
    fn a_configuration_above_the_repository_is_reported() {
        let above =
            std::env::temp_dir().join(format!("nc-above-configuration-{}", std::process::id()));
        let scratch = above.join("repository");
        let _ = std::fs::remove_dir_all(&above);
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::create_dir_all(above.join(".cargo")).expect("writable");
        std::fs::write(
            scratch.join(super::ALIAS),
            "[alias]\nxtask = \"run --quiet --locked --package xtask --\"\n",
        )
        .expect("writable");
        // Clean, so the only thing to report is the one above.
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(&scratch, &mut findings);
        assert!(findings.is_empty(), "{findings:#?}");

        std::fs::write(
            above.join(super::ALIAS),
            "[build]\nrustflags = [\"--cap-lints=allow\"]\n",
        )
        .expect("writable");
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(&scratch, &mut findings);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("above this repository")),
            "cargo merges every `.cargo/config.toml` from here to the filesystem root, \
             and the one directory up was not reported: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&above);
    }

    /// A workflow that exports a flag reaching rustc is reported.
    ///
    /// **The two halves of one rule read two different environments.** The
    /// process half reads `std::env::vars()`, which a developer's shell never
    /// holds and a runner sets before the step starts — so alone it is silent
    /// everywhere it could be acted on and fatal in the one place it fires.
    /// A `ci.yml` exporting `RUSTFLAGS: -D warnings` at workflow scope reaches
    /// every job's gate step, and the gate refuses to run with it set:
    /// such a workflow can never go green.
    #[test]
    fn a_workflow_that_exports_a_compiler_flag_is_reported() {
        let scratch = std::env::temp_dir().join(format!("nc-wf-env-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        // **Driven through the public entry, not the function.** Calling the
        // reader directly leaves the *wiring* unheld: deleting the one line that
        // calls it from `no_flags_reach_the_compiler` leaves such a test green, and
        // a gate nothing calls is a gate that is not there. The scratch needs
        // the alias file `the_alias_still_runs_the_gate` reads first.
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::write(
            scratch.join(super::ALIAS),
            "[alias]\nxtask = \"run --quiet --locked --package xtask --\"\n",
        )
        .expect("writable");
        let write = |text: &str| {
            std::fs::write(scratch.join(".github/workflows/ci.yml"), text).expect("writable");
            let mut findings = Vec::new();
            super::the_alias_still_runs_the_gate(&scratch, &mut findings);
            findings
                .into_iter()
                .filter(|finding| finding.what.contains("this workflow exports"))
                .collect::<Vec<_>>()
        };

        // The shape that ships: a workflow exporting nothing that reaches rustc.
        let findings = write("env:\n  CARGO_TERM_COLOR: always\n");
        assert!(findings.is_empty(), "{findings:#?}");

        // Every name in the family, at workflow scope and at job scope.
        for name in [
            "RUSTFLAGS",
            "RUSTDOCFLAGS",
            "RUSTC",
            "RUSTC_WRAPPER",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
        ] {
            let findings = write(&format!("env:\n  {name}: something\n"));
            assert_eq!(
                findings.len(),
                1,
                "`{name}` reaches rustc and the workflow exporting it was not reported"
            );
            assert!(findings[0].what.contains(name), "{findings:#?}");

            // Indented under a job, which is where a runner most often sets one.
            let findings = write(&format!("jobs:\n  check:\n    env:\n      {name}: x\n"));
            assert_eq!(findings.len(), 1, "`{name}` at job scope: {findings:#?}");
        }

        // A comment naming one is prose, not an export.
        let findings = write("env:\n  # RUSTFLAGS: -D warnings\n  CARGO_TERM_COLOR: always\n");
        assert!(findings.is_empty(), "a commented-out export: {findings:#?}");

        // And a key with no value sets nothing.
        let findings = write("env:\n  RUSTFLAGS:\n");
        assert!(findings.is_empty(), "an empty value: {findings:#?}");

        // **The spellings YAML allows for one key.** Read verbatim up to the
        // first `:`, a quoted key is not a match and a flow mapping yields the
        // key `env` — both valid YAML, both accepted, both exporting the flag
        // to the job whose only step is the gate. The reader for `if:` eight
        // hundred lines up strips quotes too, and one file should not hold the
        // rule in one reader and not the other.
        for spelling in [
            "env:\n  \"RUSTFLAGS\": -D warnings\n",
            "env:\n  'RUSTFLAGS': -D warnings\n",
            "env: {CARGO_TERM_COLOR: always, RUSTFLAGS: -D warnings}\n",
            "env: {\"RUSTFLAGS\": -D warnings}\n",
            "jobs:\n  check:\n    env: {RUSTFLAGS: -D warnings}\n",
        ] {
            let findings = write(spelling);
            assert_eq!(
                findings.len(),
                1,
                "`{spelling}` exports a flag that reaches rustc and was not reported: \
                 {findings:#?}"
            );
            assert!(findings[0].what.contains("RUSTFLAGS"), "{findings:#?}");
        }

        // The floor in the other direction: a flow mapping of ordinary keys is
        // not a finding, and neither is a name that merely contains one.
        for harmless in [
            "env: {CARGO_TERM_COLOR: always, CARGO_INCREMENTAL: 0}\n",
            "env:\n  MY_RUSTFLAGS_NOTE: read the docs\n",
        ] {
            let findings = write(harmless);
            assert!(findings.is_empty(), "`{harmless}`: {findings:#?}");
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The file cargo reads from its own home is judged, and only for what reaches rustc.
    ///
    /// **The ancestor walk only reaches it by coincidence.** Cargo merges
    /// `$CARGO_HOME/config.toml` into every build wherever that directory sits,
    /// and the walk beside this one climbs from the repository — so it finds the
    /// file only when the home happens to be under a parent of the checkout.
    /// Where the home is an XDG data directory, the walk checks a path that
    /// does not exist. Measured, with six real clippy findings in
    /// criterion 3's own suite and a `[build]` `rustflags` of `--cap-lints=allow`
    /// written there:
    ///
    /// ```text
    ///   ok    formatting
    ///   ok    lints
    ///   ok    818 test(s)
    /// check passed
    /// ```
    ///
    /// Both directions, because the gate has to be one a developer can live
    /// with: a home holding `[net]` is ordinary and must pass, and a gate that
    /// failed every machine carrying one is a gate somebody switches off.
    #[test]
    fn cargos_own_home_is_read_and_judged_by_what_it_holds() {
        let home = std::env::temp_dir().join(format!("nc-home-{}", std::process::id()));
        let scratch = std::env::temp_dir().join(format!("nc-home-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&home).expect("writable");
        // The floor under both halves: a home *inside* the repository, or one of
        // its ancestors, is already the ancestor walk's to report, and this
        // function returns early for both. If the scratch ever moved under the
        // home, every assertion below would pass by never running.
        assert!(
            !home.starts_with(&scratch) && !scratch.ancestors().any(|above| above == home),
            "the scratch home is where the ancestor walk already looks, so this test \
             would be about that walk"
        );
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::write(
            scratch.join(super::ALIAS),
            "[alias]\nxtask = \"run --quiet --locked --package xtask --\"\n",
        )
        .expect("writable");

        let judge = |home: &std::path::Path| {
            let mut findings = Vec::new();
            super::the_home_cargo_reads(Some(home.to_path_buf()), &scratch, &mut findings);
            findings
        };

        // An ordinary home: a file, and nothing in it that reaches the compiler.
        std::fs::write(
            home.join("config.toml"),
            "[net]\ngit-fetch-with-cli = true\n",
        )
        .expect("writable");
        let findings = judge(&home);
        assert!(
            findings.is_empty(),
            "an ordinary cargo home was reported, which is a gate a developer turns \
             off: {findings:#?}"
        );

        // And the one that decides what `check` compiles.
        std::fs::write(
            home.join("config.toml"),
            "[net]\ngit-fetch-with-cli = true\n\n[build]\nrustflags = \
             [\"--cap-lints=allow\"]\n",
        )
        .expect("writable");
        let findings = judge(&home);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("cargo's own home declares `build`")),
            "cargo merges its home's configuration into every build and this one turns \
             off every workspace lint and `unsafe_code`: {findings:#?}"
        );

        // **And every table that reaches the compiler, not only the one
        // measured.** With `[build]` the only one driven, the clauses for
        // `[target]` and `[unstable]` are decided by nothing and either can
        // be deleted with `check passed`.
        // `[target.<triple>] rustflags` is where `--cap-lints=allow` is written
        // by anyone who wants it to survive a `--target`, and `[unstable]`
        // turns on flags that change the lint pipeline outright.
        for table in super::REACHES_RUSTC {
            std::fs::write(
                home.join("config.toml"),
                format!("[net]\ngit-fetch-with-cli = true\n\n[{table}]\n"),
            )
            .expect("writable");
            let findings = judge(&home);
            assert!(
                findings.iter().any(|finding| finding
                    .what
                    .contains(&format!("cargo's own home declares `{table}`"))),
                "`[{table}]` in a cargo home reaches this build and was not reported: \
                 {findings:#?}"
            );
        }
        // And the direction that must stay legal: a home that declares none of
        // them is ordinary, and a gate that refused it is one somebody switches
        // off.
        std::fs::write(
            home.join("config.toml"),
            "[net]\ngit-fetch-with-cli = true\n",
        )
        .expect("writable");
        let left_over = judge(&home);
        assert!(
            left_over.is_empty(),
            "a cargo home that reaches the compiler in no way at all was reported: {left_over:?}"
        );

        let _ = std::fs::remove_dir_all(&home);
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The three lists this file holds that nothing else pins.
    ///
    /// **A list nobody writes down twice is a list that can lose an entry.**
    /// Measured: `REACHES_RUSTC` with `"target"` dropped — per-target
    /// `rustflags`, which is where `--cap-lints=allow` is written in a cargo
    /// home — and `IN_THIS_ORDER` with `"cpio -pdm"` dropped, which frees the
    /// copy to be taken *after* the second build so the only cross-process
    /// determinism check in the repository diffs a tree against itself, both
    /// leave every other test green, so without this test `check` passes.
    /// `SETS_UP_AND_JUDGES_NOTHING`, with one driver naming one action, lets
    /// any other name be allowed.
    ///
    /// Written out here, and compared. The fixtures below drive what each entry
    /// *does*; this is the half that notices one going missing.
    #[test]
    fn the_three_lists_this_file_decides_by_are_the_ones_written_down() {
        assert_eq!(
            super::REACHES_RUSTC,
            ["build", "target", "unstable"],
            "the cargo-configuration tables that reach the compiler moved"
        );
        assert_eq!(
            super::SETS_UP_AND_JUDGES_NOTHING,
            [
                "actions/checkout",
                "dtolnay/rust-toolchain",
                "Swatinem/rust-cache",
                "actions/cache",
                "actions/upload-artifact",
            ],
            "the actions CI may use moved: one that judges something is a second gate"
        );
        assert_eq!(
            super::IN_THIS_ORDER,
            [
                "cargo xtask build",
                "cpio -pdm",
                "cd /tmp/a-second-checkout-of-noctua-central",
                "cargo xtask build",
                "diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-catalog/src",
            ],
            "the determinism job's order moved — and *copy, diff, build, build* satisfies \
             every other rule here while comparing a tree with itself"
        );
    }

    /// What `cargo xtask` resolves to is read, not only what CI types.
    ///
    /// **One line in `.cargo/config.toml` decides whether the gate runs**, and
    /// every other check in this file reads the words CI types rather than what
    /// they mean. Measured:
    ///
    /// ```text
    /// xtask = "test --quiet --locked --package xtask --bins"
    /// $ cargo xtask check   ->   EXIT=0, 21 tests ran, the gate did not
    /// ```
    ///
    /// Exit zero, `10 cargo command(s)` unchanged, no finding anywhere.
    #[test]
    fn the_alias_is_read_as_well_as_the_words_ci_types() {
        let scratch = std::env::temp_dir().join(format!("nc-alias-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        let write = |alias: &str| {
            std::fs::write(
                scratch.join(super::ALIAS),
                format!("# a comment\n[alias]\n{alias}\n"),
            )
            .expect("writable");
        };

        // The shipped alias, and the two other spellings of the same thing.
        for good in [
            "xtask = \"run --quiet --locked --package xtask --\"",
            "xtask = \"run -p xtask --locked --\"",
            "xtask = \"run --locked --package xtask\"",
            // The flag last rather than first, which is the spelling a reader
            // is most likely to write — and not a byte-identical copy of the
            // first, which would be a fixture that cannot decide anything its
            // twin does not.
            "xtask = \"run --package xtask --locked --\"",
        ] {
            write(good);
            let mut findings = Vec::new();
            super::the_alias_still_runs_the_gate(&scratch, &mut findings);
            assert!(findings.is_empty(), "`{good}` was refused: {findings:#?}");
        }

        // And every way to make `cargo xtask check` mean something else.
        //
        // **The first entry is refused by the `--locked` clause alone**, and
        // differs from the accepted alias above it by that word and nothing
        // else. Without it no fixture turns on that clause: deleting the
        // clause is caught only by the verdict census — which pins the
        // sentence's existence, not the rule — and weakening it to
        // `alias.contains("locked")` is caught by nothing at all, so
        // `--unlocked-probe` satisfies it.
        for bad in [
            "xtask = \"run --quiet --package xtask --\"",
            // And one that *contains* the word without carrying the flag:
            // weakening the clause to `alias.contains("locked")` accepts this,
            // and the fixture above cannot tell the two rules apart.
            "xtask = \"run --quiet --package xtask --unlocked-probe --\"",
            "xtask = \"test --quiet --locked --package xtask --bins\"",
            "xtask = \"build --package xtask\"",
            "xtask = \"run --quiet --locked --package something-else --\"",
            "xtask = \"run --quiet\"",
            "notxtask = \"run --package xtask\"",
        ] {
            write(bad);
            let mut findings = Vec::new();
            super::the_alias_still_runs_the_gate(&scratch, &mut findings);
            assert!(
                !findings.is_empty(),
                "`{bad}` left the gate with nothing to say"
            );
        }
        // **And the row written to decide the `--locked` clause has to decide
        // it.** The comment above says `--unlocked-probe` is there because
        // weakening the clause to `alias.contains("locked")` accepts it — but
        // the loop asserts only that *something* is said, and what is said for
        // that row is the whitelist's sentence, raised because the word is not
        // in `ALIAS_MAY_HOLD` whatever the `--locked` clause does. By that loop
        // alone the row cannot decide the weakening its own comment names:
        // measured, with the clause neutered and the test green.
        for (alias, must_say) in [
            (
                "xtask = \"run --quiet --package xtask --\"",
                "without `--locked`",
            ),
            (
                "xtask = \"run --quiet --package xtask --unlocked-probe --\"",
                "without `--locked`",
            ),
        ] {
            write(alias);
            let mut findings = Vec::new();
            super::the_alias_still_runs_the_gate(&scratch, &mut findings);
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains(must_say)),
                "`{alias}` was reported, but not by the clause this row exists to \
                 decide — so weakening that clause would be caught by nothing: \
                 {findings:#?}"
            );
        }

        // A missing file is the same defect: the words CI types then mean nothing.
        let _ = std::fs::remove_file(scratch.join(super::ALIAS));
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(&scratch, &mut findings);
        assert_ne!(findings, [] as [crate::gates::Finding; 0]);

        // **A decoy in another table, with the real alias below it.** With the
        // `and_then` outside the `find_map`, the first line beginning `xtask`
        // decides and nothing asks which table it is in: `cargo xtask check`
        // runs 21 tests, exits 0, and the gate never runs.
        write(
            "[env]\nxtask = \"run --quiet --locked --package xtask --\"\n\
             [alias]\nxtask = \"test --quiet --locked --package xtask --bins\"",
        );
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(&scratch, &mut findings);
        assert!(
            !findings.is_empty(),
            "a decoy above `[alias]` hid what `cargo xtask` resolves to"
        );

        // **And the file cargo prefers when both exist.** It reads
        // `.cargo/config` over `.cargo/config.toml` and warns that it is doing allow-reference: names `.cargo/config` — the legacy spelling, written into a scratch tree by this test
        // so, and a gate that opens only the second misses it.
        write("[alias]\nxtask = \"run --quiet --locked --package xtask --\"");
        std::fs::write(
            scratch.join(super::LEGACY_ALIAS),
            "[alias]\nxtask = \"test --package xtask\"\n",
        )
        .expect("writable");
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(&scratch, &mut findings);
        assert!(
            !findings.is_empty(),
            "the file cargo actually reads was not judged"
        );
        let _ = std::fs::remove_file(scratch.join(super::LEGACY_ALIAS));

        // And the alias this repository actually ships.
        let mut findings = Vec::new();
        super::the_alias_still_runs_the_gate(root(), &mut findings);
        assert!(findings.is_empty(), "{findings:#?}");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// What a recipe's own name and its `just --list` description are.
    ///
    /// `just` prints the **last** comment line above a recipe and nothing else,
    /// so this reads the same thing rather than the whole block. A recipe with
    /// no comment above it has no description, which `just` shows as a blank.
    fn the_listed_descriptions(text: &str) -> Vec<(String, Option<String>)> {
        let mut found = Vec::new();
        let mut last_comment: Option<String> = None;
        for line in text.lines() {
            if let Some(comment) = line.strip_prefix("# ") {
                last_comment = Some(comment.trim().to_owned());
                continue;
            }
            if line.starts_with('#') {
                last_comment = Some(String::new());
                continue;
            }
            // **A `[doc('…')]` attribute *is* the description.** `just --list`
            // prints it instead of the comment, so skipping every `[`-line makes
            // the one attribute that carries the thing under test invisible:
            // measured against just 1.55.1, `[doc('`cargo install` for
            // cargo-deny, and two builds it diffs')]` above `check:` puts that
            // exact string — the one this test's own doc names as the defect it
            // guards — into the listing, with the test green.
            if let Some(said) = line
                .strip_prefix("[doc('")
                .and_then(|rest| rest.strip_suffix("')]"))
                .or_else(|| {
                    line.strip_prefix("[doc(\"")
                        .and_then(|rest| rest.strip_suffix("\")]"))
                })
            {
                last_comment = Some(said.trim().to_owned());
                continue;
            }
            // **Another `just` attribute sits between the comment and the recipe.**
            // `[group('stack')]`, `[confirm]`, `[private]`, `[linux]` — none is
            // a comment, a blank line or an indented body line, so without this
            // all of them fall through to "this recipe has no description"
            // while `just --list` goes on printing one. Measured against just
            // 1.55.1: one `[group('stack')]` above `dev` then fails this test
            // with a message stating the inverse of what the tool prints — a
            // red gate blaming a description that is on screen in front of
            // whoever groups the recipes.
            if line.starts_with('[') {
                continue;
            }
            if line.trim().is_empty() || line.starts_with([' ', '\t']) {
                last_comment = None;
                continue;
            }
            // A recipe header is `name:` or `name arg:` at column zero. An
            // assignment (`x := y`) and a setting (`set shell := …`) are not.
            if let Some(head) = line.split_once(':')
                && !head.1.starts_with('=')
            {
                let name = head.0.split_whitespace().next().unwrap_or("").to_owned();
                if !name.is_empty() {
                    found.push((name, last_comment.take()));
                }
            }
            last_comment = None;
        }
        found
    }

    /// Every recipe `just --list` prints describes itself in a whole sentence.
    ///
    /// **A paragraph above a recipe reaches a reader as its last clause.**
    /// `just --list` prints one line, so a paragraph above `check` — THE gate —
    /// lists it as *"`cargo install` for cargo-deny, and two builds it diffs"*,
    /// which is what CI runs **beside** it; above `check-offline`, as *"the
    /// ones a reader is most likely to believe were checked"*; above
    /// `checkout-shape`, as *"decays into prose"* — measured, and the one a new
    /// reader types first described as something it does not do.
    ///
    /// This tree already treats a diagnostic that begins mid-clause as a defect
    /// — `FIXTURES` in `check.rs` carries the paragraph — and a recipe listing
    /// is the same thing for the same reason.
    ///
    /// **What it judges is the shape, not the prose**: a description begins
    /// where a sentence begins and ends where one ends. It cannot tell a true
    /// description from a false one, and saying so here is the point — the
    /// numbers `check-offline` states are held by the skip census in
    /// `check.rs`, not by this.
    #[test]
    fn the_listing_reads_as_sentences() {
        let path = super::JUSTFILE
            .iter()
            .map(|name| root().join(name))
            .find(|candidate| candidate.is_file())
            .expect("this repository has a justfile");
        let text = std::fs::read_to_string(path).expect("readable");
        let described = the_listed_descriptions(&text);
        // **An equality, because a floor leaves slack.** A floor one below the
        // number of recipes passes a reader that silently drops one —
        // measured, a `.skip(13)` that loses `default` left such a test green.
        // Every other census in this tree is an equality for the same reason;
        // a recipe added or removed moves this number in the same edit.
        assert_eq!(
            described.len(),
            9,
            "the justfile reader found {} recipe(s) and this expects nine: either a \
             recipe moved, or it is reading the wrong lines — {described:#?}",
            described.len()
        );
        for (name, description) in &described {
            // `default` is `just` with no arguments and has no description by
            // design; every other recipe is something a reader chooses from a
            // list, and a list entry that says nothing is a recipe nobody runs.
            let Some(description) = description else {
                assert_eq!(
                    name, "default",
                    "`just --list` shows `{name}` with no description"
                );
                continue;
            };
            let opens = description.chars().next().expect("a non-empty comment");
            assert!(
                opens.is_uppercase() || opens == '`',
                "`just --list` shows `{name}` as \"{description}\", which begins \
                 mid-clause. The last comment line above a recipe is the whole of \
                 what `just` prints, so the explanation goes above it"
            );
            assert!(
                description.ends_with('.'),
                "`just --list` shows `{name}` as \"{description}\", which is cut \
                 off. The last comment line above a recipe is the whole of what \
                 `just` prints"
            );
        }
    }

    /// More ways past a narrower gate, each measured green against it.
    ///
    /// ```text
    /// run: bash "$GITHUB_WORKSPACE/ci/lint.sh"      -> ok 10 cargo command(s)
    /// run: for s in ci/*.sh; do bash "$s"; done     -> ok 10 cargo command(s)
    /// run: rustfmt --check --edition 2024 …         -> ok 10 cargo command(s)
    /// uses: giraffate/clippy-action@v1              -> ok 10 cargo command(s)
    /// ```
    ///
    /// The first two are a word that names no file: the checkout root written
    /// as the variable GitHub gives you, and a glob. The third is the binary
    /// underneath `cargo fmt`, already on the runner. The fourth is somebody
    /// else's action, which a rule matching only a last path segment of
    /// `cargo` lets through.
    ///
    /// **And every name in the list, not only the one measured.**
    /// `judges_this_tree` holds four binaries, and with a fixture for `rustfmt`
    /// alone, dropping `| "miri"` — or `rustdoc`, or `clippy-driver` — leaves
    /// the whole suite green. That is an enumeration standing in for a rule,
    /// a shape this file refuses. Each of the four has a step below that is
    /// reported, so removing its name from the list fails here.
    #[test]
    fn a_second_gate_behind_a_variable_a_glob_or_another_binary_is_found() {
        const RECIPES: &str = "cargo fmt --all --check\ncargo test --workspace\n";
        let scratch = std::env::temp_dir().join(format!("nc-ci-reach-{}", std::process::id()));
        for (label, step, script) in [
            (
                "the checkout root as a variable",
                "      - name: second\n        run: bash \"$GITHUB_WORKSPACE/ci/lint.sh\"\n",
                Some(RECIPES),
            ),
            (
                "the expression form of the same",
                "      - name: second\n        run: bash \"${{ github.workspace }}/ci/lint.sh\"\n",
                Some(RECIPES),
            ),
            (
                "a glob over a directory of scripts",
                "      - name: second\n        run: for s in ci/*.sh; do bash \"$s\"; done\n",
                Some(RECIPES),
            ),
            (
                "the binary underneath cargo fmt",
                "      - name: second\n        run: rustfmt --check --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                "the binary underneath cargo clippy",
                "      - name: second\n        run: clippy-driver --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                "the binary underneath cargo doc",
                "      - name: second\n        run: rustdoc --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                "an interpreter that runs this tree's tests",
                "      - name: second\n        run: miri test --workspace\n",
                None,
            ),
            (
                // **Three words past the gate.** `rustup run <toolchain>` is
                // how CI pins a toolchain per command, and the binary then sits
                // where no adjacency rule reaches. Measured against an adjacency
                // rule: `12 cargo command(s)` unmoved and `check passed`.
                "a judging binary behind a toolchain runner",
                "      - name: second\n        run: rustup run nightly miri test --workspace\n",
                None,
            ),
            (
                // **A separator with no space in front of it.** The scan reads
                // command position and the install excuse from separators, and
                // it splits on whitespace: `add rustfmt;` is one word, so
                // unspaced the separator is never seen, `installing` never
                // clears, and the second half of the line is excused by the
                // `add` in the first. Measured, this step in `ci.yml` left `12 cargo
                // command(s)` unmoved and `check passed`; the same line with
                // one space before the `;` was a finding. No shell requires the
                // space.
                "a judging binary after a glued separator",
                "      - name: second\n        run: rustup component add rustfmt; rustfmt --check --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                // And the same hole in front of the `cargo-*` driver family,
                // which needs no install to excuse it — only command position.
                "a driver after a glued separator",
                "      - name: second\n        run: echo linting; cargo-clippy --workspace --all-targets -- -D warnings\n",
                None,
            ),
            (
                "a judging binary behind a timeout",
                "      - name: second\n        run: timeout 600 clippy-driver --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                // **The word a wrapper script ends with**, and one an
                // enumeration falls short of. `exec` is neither `-` nor `-c`, so
                // to an enumeration no word after it opens a command and a whole
                // second formatting gate is invisible with the count unmoved.
                // `flock`, `command`, `eval`, `su` and `setsid` are the same
                // hole, which is why what closes it is a rule rather than a
                // sixth word.
                "a judging binary behind `exec`",
                "      - name: second\n        run: exec rustfmt --check --edition 2024 src/lib.rs\n",
                None,
            ),
            (
                "a judging binary behind a lock nobody listed",
                "      - name: second\n        run: flock /tmp/ci.lock rustfmt --check src/lib.rs\n",
                None,
            ),
            (
                // **And a `cargo-*` binary behind the same word.** Judged only
                // in command position while the four named above are judged
                // wherever they stand, it hides behind `exec` exactly as
                // `rustfmt` does from an enumeration: measured, transcript
                // byte-identical and the count unmoved.
                "a cargo binary behind `exec`",
                "      - name: second\n        run: exec cargo-clippy --workspace --all-targets\n",
                None,
            ),
            (
                // **A `#` that is not a comment.** `${GITHUB_REF#refs/tags/}` is
                // ordinary CI shell, and reading the `#` as a note truncates the
                // line before the gate that follows it — the mirror of the
                // defect the comment rule exists for.
                "a second gate after a parameter expansion",
                "      - name: second\n        run: TAG=${GITHUB_REF#refs/tags/} && cargo clippy --workspace --all-targets\n",
                None,
            ),
            (
                "somebody else's action",
                "      - uses: giraffate/clippy-action@v1\n",
                None,
            ),
        ] {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
            std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
            std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS)).expect("readable");
            std::fs::write(scratch.join("justfile"), "check:\n    cargo xtask check\n")
                .expect("writable");
            if let Some(recipes) = script {
                std::fs::create_dir_all(scratch.join("ci")).expect("writable");
                std::fs::write(scratch.join("ci/lint.sh"), recipes).expect("writable");
            }
            let workflow = std::fs::read_to_string(root().join(super::WORKFLOW)).expect("readable");
            let planted = workflow.replacen(
                "      - uses: Swatinem/rust-cache@v2\n",
                &format!("      - uses: Swatinem/rust-cache@v2\n{step}"),
                1,
            );
            assert_ne!(planted, workflow, "{label}: the step was not planted");
            std::fs::write(scratch.join(super::WORKFLOW), planted).expect("writable");

            let (findings, _) = super::only_runs_the_gate(&scratch).expect("the tree reads");
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("not the gate")),
                "{label}: a second gate was invisible to the CI gate: {findings:#?}"
            );
        }

        // **And the other direction, which is where this rule can be wrong.**
        // A step that installs a judging binary runs nothing, and a step whose
        // *description* names one is prose. Both are reportable by mistake:
        // the stickiness that catches `rustup run nightly miri` puts `rustfmt`
        // into command position inside `rustup component add rustfmt` unless
        // installing is excused, and a `with:` block's `components: rustfmt`
        // passes a rule where any key opens a command only when that block
        // names two components and the comma keeps it from matching.
        for (label, step) in [
            (
                "a component installed, which runs nothing",
                "      - name: second\n        run: rustup component add rustfmt\n",
            ),
            (
                "a toolchain installed with a component",
                "      - name: second\n        run: rustup toolchain install nightly --component miri\n",
            ),
            (
                "a `with:` block naming one component",
                "      - uses: dtolnay/rust-toolchain@stable\n        with:\n          components: rustfmt\n",
            ),
            (
                "a step described by what it is for",
                "      - name: check formatting with rustfmt\n        run: cargo xtask check\n",
            ),
        ] {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
            std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
            std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS)).expect("readable");
            for name in super::JUSTFILE {
                let from = root().join(name);
                if from.is_file() {
                    std::fs::copy(from, scratch.join(name)).expect("writable");
                }
            }
            let workflow = std::fs::read_to_string(root().join(super::WORKFLOW)).expect("readable");
            let planted = workflow.replacen(
                "      - uses: Swatinem/rust-cache@v2\n",
                &format!("      - uses: Swatinem/rust-cache@v2\n{step}"),
                1,
            );
            assert_ne!(planted, workflow, "{label}: the step was not planted");
            std::fs::write(scratch.join(super::WORKFLOW), planted).expect("writable");
            let (findings, _) = super::only_runs_the_gate(&scratch).expect("the tree reads");
            assert!(
                findings.is_empty(),
                "{label}: a step that runs no second gate was reported: {findings:#?}"
            );
        }

        // And the workflow this repository actually ships is not reported, so
        // the rule is about the shapes above and not about every step.
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS)).expect("readable");
        std::fs::copy(root().join(super::WORKFLOW), scratch.join(super::WORKFLOW))
            .expect("readable");
        for name in super::JUSTFILE {
            let from = root().join(name);
            if from.is_file() {
                std::fs::copy(from, scratch.join(name)).expect("writable");
            }
        }
        let (clean, _) = super::only_runs_the_gate(&scratch).expect("the tree reads");
        // Empty, not merely free of one phrase: a closure filtering for one
        // phrase was measured never seeing an element, so such an assertion
        // holds whatever the reader does.
        assert!(
            clean.is_empty(),
            "the shipped workflow was reported: {clean:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A second gate one line outside CI's reach, two ways.
    ///
    /// **Both measured green against a narrower walk.** `just` loads
    /// `mod extra` from `extra.just`, and a walk that looks for words that name
    /// files misses it — `extra` is not one. A committed `Makefile` opened only
    /// when some reached file happens to contain the bare word `make` breaks the
    /// argument the justfile beside it is read on unconditionally: that a
    /// committed file makes the claim whether or not CI mentions it. And
    /// `GNUmakefile` is the name make reads *first*.
    ///
    /// Each shape carries the same three recipes that go red when they are
    /// written inline, so what is being tested is where the gate looks and not
    /// what it judges.
    #[test]
    fn a_second_gate_beside_the_justfile_is_still_found() {
        const RECIPES: &str = "lint:\n    cargo fmt --all --check\n    \
                               cargo clippy --workspace --all-targets -- -D warnings\n    \
                               cargo test --workspace\n";
        let scratch = std::env::temp_dir().join(format!("nc-second-gate-{}", std::process::id()));
        for (label, files) in [
            (
                "a just module",
                vec![
                    ("justfile", "check:\n    cargo xtask check\n\nmod extra\n"),
                    ("extra.just", RECIPES),
                ],
            ),
            (
                "a just module with a path",
                vec![
                    (
                        "justfile",
                        "check:\n    cargo xtask check\n\nmod extra 'elsewhere.just'\n",
                    ),
                    ("elsewhere.just", RECIPES),
                ],
            ),
            (
                "a committed Makefile nothing mentions",
                vec![
                    ("justfile", "check:\n    cargo xtask check\n"),
                    ("Makefile", "lint:\n\tcargo test --workspace\n"),
                ],
            ),
            (
                "the name make reads first",
                vec![
                    ("justfile", "check:\n    cargo xtask check\n"),
                    ("GNUmakefile", "lint:\n\tcargo test --workspace\n"),
                ],
            ),
        ] {
            let _ = std::fs::remove_dir_all(&scratch);
            std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
            std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
            std::fs::copy(root().join(super::WORKFLOW), scratch.join(super::WORKFLOW))
                .expect("the real workflow is readable");
            std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS))
                .expect("the real alias is readable");
            for (name, text) in files {
                std::fs::write(scratch.join(name), text).expect("writable");
            }

            let (findings, _) = super::only_runs_the_gate(&scratch).expect("the tree reads");
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("not the gate")),
                "{label}: a second gate was invisible to the CI gate: {findings:#?}"
            );
        }

        // And the shape that is not a second gate, so the rule is not "any file
        // beside the justfile is a finding".
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::copy(root().join(super::WORKFLOW), scratch.join(super::WORKFLOW))
            .expect("readable");
        std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS)).expect("readable");
        std::fs::write(
            scratch.join("justfile"),
            "check:\n    cargo xtask check\n\nmod extra\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("extra.just"), "docs:\n    cargo xtask docs\n")
            .expect("writable");
        let (clean, _) = super::only_runs_the_gate(&scratch).expect("the tree reads");
        // Empty, not merely free of one phrase: a negative control over an
        // empty list is true whatever the reader did, and this closure was
        // measured never seeing an element.
        assert!(
            clean.is_empty(),
            "a module whose recipes are xtask verbs was reported: {clean:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The alias gate is reached from the step that composes the CI gates.
    ///
    /// **The call site, not the helper.** Tests that drive
    /// `the_alias_still_runs_the_gate` directly stay green when the one line
    /// that calls it from `only_runs_the_gate` is deleted — and what the deleted
    /// line holds is the rule that `cargo xtask check` means the same thing for
    /// CI and for a developer.
    #[test]
    fn the_composed_step_reaches_the_alias_gate() {
        let scratch = std::env::temp_dir().join(format!("nc-alias-site-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join(".cargo")).expect("writable");
        std::fs::copy(root().join(super::WORKFLOW), scratch.join(super::WORKFLOW))
            .expect("the real workflow is readable");
        for name in super::JUSTFILE {
            let from = root().join(name);
            if from.is_file() {
                std::fs::copy(from, scratch.join(name)).expect("writable");
            }
        }

        // The alias cargo reads, pointed at something that is not the gate —
        // the shape that runs 21 tests, exits 0, and never gates anything.
        std::fs::write(
            scratch.join(super::ALIAS),
            "[alias]\nxtask = \"test --quiet --package xtask --bins\"\n",
        )
        .expect("writable");
        let (findings, _) = super::only_runs_the_gate(&scratch).expect("the scratch tree reads");
        assert!(
            findings
                .iter()
                .any(|finding| finding.file.contains(".cargo/config.toml")),
            "an alias that no longer runs the gate was not reported by the step \
             that composes the CI gates: {findings:#?}"
        );

        // And the alias this repository ships, in the same tree, so the
        // assertion above is about the alias and not about the scratch tree.
        std::fs::copy(root().join(super::ALIAS), scratch.join(super::ALIAS)).expect("writable");
        let (clean, _) = super::only_runs_the_gate(&scratch).expect("the scratch tree reads");
        // Empty, for the reason above.
        assert!(
            clean.is_empty(),
            "the shipped alias was reported: {clean:#?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// One `on:` event's body is lines, not one line with `\\n` written in it.
    ///
    /// `join("\\n")` — the two-character literal — makes `body.lines()` yield a
    /// single line, so only the *first* key under an event is read. With the
    /// identical filter written second it passes:
    ///
    /// ```text
    /// on: push: branches: [does-not-exist] / tags: ['v*']   -> FAILED
    /// on: push: tags: ['v*'] / branches: [does-not-exist]   -> ok
    /// ```
    #[test]
    fn an_events_body_is_read_as_lines() {
        let ci = "on:\n  push:\n    branches: [main]\n    tags: ['v*']\n  pull_request:\n";
        let body = super::event_body(ci, "push").expect("push is there");
        assert_eq!(
            body.lines().count(),
            2,
            "the body came back as {body:?}, which is one line"
        );
        // And an unfiltered event has an empty body, which is what the check
        // above reads as "runs on every push".
        let plain = super::event_body("on:\n  push:\n  pull_request:\n", "push");
        assert_eq!(plain.as_deref().map(str::trim), Some(""));
    }

    /// A job named for a system runs on that system's runner, and the family is
    /// what is read, not the version.
    ///
    /// **Every system, each moved in turn.** The table is written here and not
    /// read from [`super::RUNS_ON`], so a row deleted there still has its job
    /// moved here, and the rule's silence fails this test: with only `macos`
    /// moved, a `windows` job on an Ubuntu runner would pass every rule.
    #[test]
    fn a_job_named_for_a_system_runs_on_it() {
        let systems = [
            ("linux", "ubuntu-", "ubuntu-24.04"),
            ("macos", "macos-", "macos-15"),
            ("windows", "windows-", "windows-2025"),
        ];
        // Every job on its own runner but `moved`, which runs on `runner`, or
        // names none at all when `runner` is `None`.
        let ci = |moved: &str, runner: Option<&str>| -> String {
            systems
                .iter()
                .map(|&(job, family, _)| {
                    let runs_on = if job == moved {
                        runner.map(|runner| format!("    runs-on: {runner}\n"))
                    } else {
                        Some(format!("    runs-on: {family}latest\n"))
                    };
                    format!(
                        "  {job}:\n{}    steps:\n      - run: cargo xtask check\n",
                        runs_on.unwrap_or_default()
                    )
                })
                .fold("jobs:\n".to_owned(), |all, job| all + &job)
        };
        let wrong = |workflow: &str| -> Vec<String> {
            let mut findings = Vec::new();
            super::jobs_are_what_they_are_named(workflow, &mut findings);
            findings
                .into_iter()
                .filter(|finding| finding.what.contains(" job runs on `"))
                .map(|finding| finding.what)
                .collect()
        };
        for (job, family, versioned) in systems {
            let latest = format!("{family}latest");
            for runner in [
                latest.clone(),
                versioned.to_owned(),
                format!("\"{latest}\""),
                format!("'{latest}'"),
                format!("{latest} # the newest"),
            ] {
                let refused = wrong(&ci(job, Some(&runner)));
                assert!(
                    refused.is_empty(),
                    "a `{job}` job on `{runner}` was refused: {refused:#?}"
                );
            }
            let elsewhere = systems
                .iter()
                .filter(|&&(other, _, _)| other != job)
                .map(|&(_, other, _)| format!("{other}latest"));
            for runner in elsewhere.chain([String::new()]) {
                assert_eq!(
                    wrong(&ci(job, Some(&runner))),
                    [format!(
                        "the `{job}` job runs on `{runner}`, which is not a `{family}` runner"
                    )],
                    "a `{job}` job on `{runner}` proves nothing about its system and was \
                     accepted"
                );
            }
            assert_eq!(
                wrong(&ci(job, None)),
                [format!(
                    "the `{job}` job runs on ``, which is not a `{family}` runner"
                )],
                "a `{job}` job that names no runner was accepted"
            );
        }
    }

    #[test]
    fn a_deleted_job_is_a_finding_and_not_a_smaller_number() {
        // **Deleting the whole `deterministic` job is a finding.** The count
        // goes from four cargo invocations to two, and a floor at zero does not
        // see that — so without this the sole cross-process determinism check
        // in the repository, and the only place the ledger's determinism is
        // checked at all, can be removed without a red light.
        let scratch = std::env::temp_dir().join(format!("nc-workflow-jobs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);
        let every = super::JOBS.map(|(job, _)| job);
        for missing in every {
            let kept: Vec<String> = every
                .iter()
                .filter(|job| **job != missing)
                .map(|job| format!("  {job}:\n    steps:\n      - run: cargo xtask check\n"))
                .collect();
            let kept = kept.concat();
            std::fs::write(scratch.join(super::WORKFLOW), format!("jobs:\n{kept}"))
                .expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            // **Exactly one, not at least one.** A rule written twice says the
            // same thing twice about one deleted job — and `.any(…)` is as true
            // of two findings as of one.
            let said = findings
                .iter()
                .filter(|finding| finding.what == format!("the `{missing}` job is gone"))
                .count();
            assert_eq!(
                said, 1,
                "a deleted `{missing}` job was reported {said} time(s): {findings:#?}"
            );
        }
        // **Removed at the end, not only at the start.** Calling
        // `remove_dir_all` before writing and never after leaves one behind on
        // every suite run — measured, `/tmp`'s `nc-*` count rising by two per
        // run.
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Whether a `diff`'s two operands are the same path, one in each checkout.
    ///
    /// **Not only which side each operand is on.** Asking only that, exchanging
    /// two diffs' right-hand operands leaves the only cross-process determinism
    /// check in the repository comparing two unrelated trees, with every reader
    /// of it silent: two whole-file `contains` calls and a per-line rule that
    /// reads the two prefixes. Neither generated crate is then compared against
    /// its own second generation, which is the whole of criterion 4's
    /// cross-process half.
    ///
    /// **And the ordinary ways to quote the same path are accepted.** Compared
    /// as written, the operands make the rule an exact-prefix test on one
    /// spelling — `"$GITHUB_WORKSPACE"/docs`. Requoting it as
    /// `"$GITHUB_WORKSPACE/docs"`, which is what most people write and which
    /// the shell reads identically, then fails with a message naming three
    /// reasons none of which is true; so do `${GITHUB_WORKSPACE}/docs` and a
    /// trailing slash on either operand. Measured. A gate that is green only
    /// because nothing legitimate has been written yet is the other way a
    /// repair is wrong, and this file says so about two other rules.
    ///
    /// So both operands are read as paths first: quoting off, the two spellings
    /// of the variable written one way, and no trailing slash.
    fn one_path_in_two_checkouts(left: &str, right: &str, here: &str) -> bool {
        let left = as_one_path(left);
        let here = as_one_path(here);
        let Some(there) = left.strip_prefix(&format!("/tmp/{SECOND_CHECKOUT}/")) else {
            return false;
        };
        as_one_path(right)
            .strip_prefix(&here)
            .map(|rest| rest.trim_start_matches('/'))
            .is_some_and(|mine| mine == there)
    }

    /// One `diff` operand as the path it names.
    ///
    /// The shell's quoting is not part of a path, `${GITHUB_WORKSPACE}` and
    /// `$GITHUB_WORKSPACE` are one variable, and a directory is the same
    /// directory with a slash after it.
    fn as_one_path(operand: &str) -> String {
        operand
            .replace(['"', '\''], "")
            .replace("${GITHUB_WORKSPACE}", "$GITHUB_WORKSPACE")
            .trim_end_matches('/')
            .to_owned()
    }

    /// The pairing rule, both ways.
    #[test]
    fn a_diff_compares_one_path_in_two_checkouts() {
        let here = "$GITHUB_WORKSPACE";
        let second = format!("/tmp/{SECOND_CHECKOUT}");
        assert!(one_path_in_two_checkouts(
            &format!("{second}/crates/nc-log/src"),
            &format!("{here}/crates/nc-log/src"),
            here
        ));
        // **And every other spelling of the same path**, because refusing one
        // is how a rule written to close a hole starts reporting a defect that
        // is not there. The caller passes the quoted form, which is how
        // `ci.yml` writes it.
        let quoted = "\"$GITHUB_WORKSPACE\"";
        // **And the left operand too.** `as_one_path` is applied to both sides,
        // and with only the right one driven the call on the left decides
        // nothing on this tree and can be deleted with the corpus green.
        // `ci.yml` quotes the right operand; someone quoting the left the same
        // way is the case.
        assert!(
            one_path_in_two_checkouts(&format!("\"{second}/docs\""), &format!("{here}/docs"), here),
            "a quoted left operand names the same path as an unquoted one"
        );
        for (operand, root) in [
            ("\"$GITHUB_WORKSPACE\"/docs", quoted),
            ("\"$GITHUB_WORKSPACE/docs\"", quoted),
            ("${GITHUB_WORKSPACE}/docs", here),
            ("$GITHUB_WORKSPACE/docs/", here),
            ("\"$GITHUB_WORKSPACE\"/docs", here),
        ] {
            assert!(
                one_path_in_two_checkouts(&format!("{second}/docs"), operand, root),
                "`{operand}` is the same path as `{second}/docs` in this checkout, and \
                 the rule read it as a different one"
            );
        }
        assert!(
            !one_path_in_two_checkouts(
                &format!("{second}/crates/nc-catalog/src"),
                &format!("{here}/crates/nc-log/src"),
                here
            ),
            "two different paths were read as one path in two checkouts"
        );
        assert!(
            !one_path_in_two_checkouts(
                &format!("{second}/crates/nc-log/src"),
                "crates/nc-log/src",
                here
            ),
            "a relative right-hand operand was accepted, and it means whatever the \
             working directory happens to be"
        );
        assert!(
            !one_path_in_two_checkouts(
                &format!("{second}/crates/nc-log/src"),
                &format!("{second}/crates/nc-log/src"),
                here
            ),
            "the second checkout was accepted as its own comparison"
        );
    }

    /// The determinism job diffs everything the emitter says it generates.
    ///
    /// **Two hand lists, related by nothing.** `ci.yml`'s `deterministic` job
    /// names four paths to `diff`, and `nc_catalog_compiler` declares what it
    /// generates in `GENERATED_DIRECTORIES` and `GENERATED_FILES`. They match
    /// today. A third generated directory would leave the **only cross-process
    /// determinism check in this repository** not covering it — and criterion 4
    /// rests on that job, because the in-process step generates twice in one
    /// process and cannot see a difference that depends on the filesystem.
    ///
    /// Nothing but this test would notice such a directory missing from the
    /// job either.
    #[test]
    fn the_determinism_job_diffs_everything_the_emitter_writes() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let workflow = std::fs::read_to_string(root.join(WORKFLOW))
            .expect("the workflow is a file in this tree");
        let body =
            job_body(&workflow, "deterministic").expect("the workflow has a `deterministic` job");
        // **Both operands, not the left one.** A needle on
        // `diff -r /tmp/SECOND/x ` and nothing else leaves every right-hand
        // side free: with them all *relative*, the entire comparison rests on
        // one `cd -` no test reads — delete that line and each diff compares
        // the second checkout against itself, with every test in this module
        // green and `check passed` over them. Rewriting one right operand to a
        // path inside the second checkout does the same for eight of the
        // fifteen generated files.
        //
        // `$GITHUB_WORKSPACE` is the runner's own name for the checkout, so
        // neither side depends on where the shell happens to be standing.
        //
        // **Read as paths, not as shell.** Every check below is a `contains`
        // over the body for one spelling of the anchor, so read as shell,
        // requoting a diff as `"$GITHUB_WORKSPACE/docs"` — which the shell reads
        // identically, and which is what most people write — reports `docs` as
        // compared against a relative path. The body and the anchor go through the same reader
        // the pairing rule uses.
        let body = as_one_path(&body);
        let here = as_one_path("\"$GITHUB_WORKSPACE\"");
        let here = here.as_str();
        let mut uncovered = Vec::new();
        for generated in nc_catalog_compiler::GENERATED_DIRECTORIES {
            if !body.contains(&format!("diff -r /tmp/{SECOND_CHECKOUT}/{generated} ")) {
                let missed = format!("  the directory `{generated}`");
                uncovered.push(missed);
            }
            if !body.contains(&format!("{here}/{generated}")) {
                let missed =
                    format!("  the directory `{generated}` is diffed against a relative path");
                uncovered.push(missed);
            }
        }
        for generated in nc_catalog_compiler::GENERATED_FILES {
            // The file, or the directory holding it — CI diffs
            // `crates/nc-log/src` whole, which covers `events.rs` inside it.
            let directory = generated.rsplit_once('/').map_or(generated, |(at, _)| at);
            if !body.contains(&format!("/tmp/{SECOND_CHECKOUT}/{generated}"))
                && !body.contains(&format!("/tmp/{SECOND_CHECKOUT}/{directory} "))
            {
                let missed = format!("  the file `{generated}`");
                uncovered.push(missed);
            }
        }
        // And the ledger, which is generated and is neither.
        if !body.contains(&format!(
            "/tmp/{SECOND_CHECKOUT}/catalog/errors.reserved.toml"
        )) || !body.contains(&format!("{here}/catalog/errors.reserved.toml"))
        {
            let missed = "  `catalog/errors.reserved.toml`".to_owned();
            uncovered.push(missed);
        }
        // A `cd` inside the step is then a convenience rather than the
        // comparison, and no diff may be written without an anchored right-hand
        // side: the count of `diff ` invocations and of anchored operands agree.
        // **Paired, because two whole-body tallies are not a pairing.**
        // Counting `diff ` lines and `$GITHUB_WORKSPACE` occurrences anywhere
        // in the body, and requiring the second to be at least the first, lets
        // a *comment* pay the budget. Measured: three of the four diffs
        // rewritten to compare the second checkout with itself, with the
        // anchors spent in one comment line, left every test in this module
        // green and `check passed`, with the generated catalog crate, the
        // generated log-event structs and the reservation ledger each compared
        // against themselves.
        //
        // A `diff` is its operands. The left one is the second checkout and the
        // right one is this checkout, and neither is a working directory.
        let mut wrong: Vec<String> = Vec::new();
        let mut diffs = 0_usize;
        let mut line = String::new();
        for text in body.lines() {
            // A `\` continuation makes one invocation out of two lines.
            let trimmed = text.trim();
            if line.is_empty() && !trimmed.starts_with("diff ") {
                continue;
            }
            line.push(' ');
            line.push_str(trimmed.trim_end_matches('\\').trim_end());
            if trimmed.ends_with('\\') {
                continue;
            }
            let operands: Vec<&str> = line
                .split_whitespace()
                .skip(1)
                .filter(|word| !word.starts_with('-'))
                .collect();
            diffs += 1;
            // **And the two operands are the same path, in two checkouts.**
            // Asking only that one side is under the second checkout and the
            // other under this one, exchanging two diffs' right-hand operands
            // leaves the only cross-process determinism check in the
            // repository comparing two unrelated trees, with every reader of it
            // silent: two whole-file `contains` calls and a per-line rule that
            // reads the two prefixes. Neither generated crate is then compared
            // against its own second generation.
            //
            // A diff of `X` in one checkout against `Y` in the other proves
            // nothing about either, which is the whole of criterion 4's
            // cross-process half.
            match operands.as_slice() {
                [left, right] if one_path_in_two_checkouts(left, right, here) => {}
                // Built before the push, so the verdict census — which reads
                // every `.push(` whose argument is a literal — does not take
                // this bookkeeping for a sentence the gate can say.
                _ => {
                    let found = format!("  {}", line.trim());
                    wrong.push(found);
                }
            }
            line.clear();
        }
        assert!(
            wrong.is_empty(),
            "{} of the determinism job's {diffs} diff(s) do not compare \
             /tmp/{SECOND_CHECKOUT}/… against {here}/…:\n{}\nA diff whose operands are \
             not those two things is comparing something else — a relative path means \
             whatever the working directory happens to be, a second path under /tmp is \
             the checkout compared with itself, and two different paths are two trees \
             neither of which is compared with its own second generation",
            wrong.len(),
            wrong.join("\n")
        );
        assert!(
            diffs >= 4,
            "{diffs} diff invocation(s) were read out of the determinism job, and it \
             runs more — a loop that reads none agrees with everything"
        );
        assert!(
            uncovered.is_empty(),
            "the determinism job does not compare {} of the things the emitter writes:\n\
             {}\nIt is the only cross-process determinism check there is, and criterion 4 \
             rests on it — the in-process step generates twice in one process and cannot \
             see a difference that depends on the filesystem",
            uncovered.len(),
            uncovered.join("\n")
        );
        assert!(
            nc_catalog_compiler::GENERATED_DIRECTORIES.len()
                + nc_catalog_compiler::GENERATED_FILES.len()
                >= 3,
            "the emitter declares fewer generated places than this repository has, so \
             this is comparing against the wrong list"
        );
    }

    #[test]
    fn the_determinism_job_builds_twice_in_the_order_that_proves_it() {
        let scratch =
            std::env::temp_dir().join(format!("nc-workflow-order-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);

        // **One build is not two, and any order is not the order.** Both were
        // measured passing a requirement written as `any()` over a floor of
        // zero: the determinism job reduced to a single `cargo xtask build`,
        // then `cp -r`, then `diff -r` — a copy diffed against its own source,
        // one process, no second generation — printed `ok 3 cargo command(s)`
        // and `check passed`.
        let deterministic = |steps: &str| {
            format!(
                "jobs:\n  check:\n    steps:\n      - run: cargo xtask check\n\
                 \x20 deterministic:\n    steps:\n{steps}"
            )
        };
        let one_build = "      - run: cargo xtask build\n\
                         \x20     - run: cp -r crates/nc-catalog/src /tmp/first/nc-catalog\n\
                         \x20     - run: cp -r docs /tmp/first/docs\n\
                         \x20     - run: diff -r /tmp/first/nc-catalog crates/nc-catalog/src\n\
                         \x20     - run: diff -r /tmp/first/docs docs\n";
        std::fs::write(scratch.join(super::WORKFLOW), deterministic(one_build)).expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            findings.iter().any(|finding| finding
                .what
                .contains("does not run `cargo xtask build` 2 time(s)")),
            "one build satisfied a job that has to build twice: {findings:#?}"
        );

        // The same commands, in an order that compares a tree with itself before
        // either build has run.
        let wrong_order = "      - run: cp -r crates/nc-catalog/src /tmp/first/nc-catalog\n\
                           \x20     - run: cp -r docs /tmp/first/docs\n\
                           \x20     - run: diff -r /tmp/first/nc-catalog crates/nc-catalog/src\n\
                           \x20     - run: diff -r /tmp/first/docs docs\n\
                           \x20     - run: cargo xtask build\n\
                           \x20     - run: cargo xtask build\n";
        std::fs::write(scratch.join(super::WORKFLOW), deterministic(wrong_order))
            .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("in the order")),
            "a diff before either build satisfied the job: {findings:#?}"
        );

        // **And a job holding every command, out of order.** The fixture above
        // uses `cp -r`, and the order this rule reads is written in terms of
        // `cpio -pdm` and a `cd` — so it fails for a *missing* command and
        // leaves the ordering half to this case: `from += at + 1` replaced with
        // `from = 0` degenerates the rule to membership, and without this case
        // `cargo test -p xtask` does not move. This one has all five, with the `diff` before the
        // second build, which is the arrangement the rule exists to refuse.
        let all_five_out_of_order = "      - run: cargo xtask build\n\
             \x20     - run: find . -name '*.rs' | cpio -pdm --quiet \
             /tmp/a-second-checkout-of-noctua-central\n\
             \x20     - run: cd /tmp/a-second-checkout-of-noctua-central\n\
             \x20     - run: diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-catalog/src \
             crates/nc-catalog/src\n\
             \x20     - run: cargo xtask build\n";
        std::fs::write(
            scratch.join(super::WORKFLOW),
            deterministic(all_five_out_of_order),
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("in the order")),
            "every command this job needs was present, with the comparison before the \
             second build, and the order was not judged: {findings:#?}"
        );
        // The same five in the written order pass, or the assertion above is
        // about a rule nothing can satisfy.
        let all_five_in_order = "      - run: cargo xtask build\n\
             \x20     - run: find . -name '*.rs' | cpio -pdm --quiet \
             /tmp/a-second-checkout-of-noctua-central\n\
             \x20     - run: cd /tmp/a-second-checkout-of-noctua-central\n\
             \x20     - run: cargo xtask build\n\
             \x20     - run: diff -r /tmp/a-second-checkout-of-noctua-central/crates/nc-catalog/src \
             crates/nc-catalog/src\n";
        std::fs::write(
            scratch.join(super::WORKFLOW),
            deterministic(all_five_in_order),
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            !findings
                .iter()
                .any(|finding| finding.what.contains("in the order")),
            "the five commands in the written order were refused: {findings:#?}"
        );

        // And the job this repository actually ships passes, or the two
        // assertions above are about a rule nothing can satisfy.
        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!(
                "jobs:\n  check:\n    steps:\n      - run: cargo xtask check\n{}",
                the_real_determinism_job()
            ),
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            !findings
                .iter()
                .any(|finding| finding.what.contains("deterministic")),
            "the order the job actually uses is a finding: {findings:#?}"
        );
        // **Removed at the end, not only at the start.** Calling
        // `remove_dir_all` before writing and never after leaves one behind on
        // every suite run — measured, `/tmp`'s `nc-*` count rising by two per
        // run.
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_second_gate_behind_a_working_directory_is_still_a_finding() {
        // **`working-directory:` can move a whole gate out of sight.** A step
        // saying `working-directory: scripts` then `run: bash gate.sh`, read
        // against the repository root, resolves `gate.sh` to nothing — so
        // `cargo fmt`, `cargo clippy` and `cargo test --workspace` run in CI with
        // `ok 4 cargo command(s)` and `check passed`. `cd scripts && bash gate.sh`
        // is the same hole.
        let scratch = std::env::temp_dir().join(format!("nc-workflow-wd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);
        std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
        std::fs::write(
            scratch.join("scripts/gate.sh"),
            "cargo clippy --all-targets\ncargo test --workspace\n",
        )
        .expect("writable");

        for step in [
            "      - name: extra\n        working-directory: scripts\n        run: bash gate.sh\n",
            "      - run: cd scripts && bash gate.sh\n",
        ] {
            std::fs::write(
                scratch.join(super::WORKFLOW),
                format!("{FIRES}jobs:\n  check:\n    steps:\n{step}"),
            )
            .expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            for hidden in ["cargo clippy", "cargo test"] {
                assert!(
                    findings.iter().any(|finding| finding.what.contains(hidden)),
                    "`{hidden}` behind a working directory was invisible: {findings:#?}"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn every_tree_the_build_writes_is_diffed() {
        // **Every pair in `ci.yml`, required.** Unrequired, the generated
        // log-event structs and the append-only ledger can be dropped from the
        // determinism job with no finding — and the ledger is the one artifact
        // whose whole purpose is that it never changes except by appending.
        // Deleting a requirement from `JOBS_MUST_RUN` stays green everywhere
        // else, because every fixture takes the job body from the real
        // `ci.yml`: this is what says which pairs have to be required.
        for tree in ["crates/nc-catalog/src", "crates/nc-log/src", "docs"] {
            assert!(
                JOBS_MUST_RUN.iter().any(|(job, command, _)| {
                    *job == "deterministic"
                        && command.starts_with("diff -r ")
                        && command.contains(tree)
                }),
                "`{tree}` is written by `cargo xtask build` and the determinism job is not \
                 required to diff it"
            );
        }
        // **And the diff is between two directories**, which is what the second
        // build being a second *environment* rests on. Two builds in this
        // checkout share the working directory, the filesystem and the order
        // `read_dir` returns — the one input that actually differs between
        // machines.
        assert!(
            JOBS_MUST_RUN.iter().any(|(job, command, _)| {
                *job == "deterministic" && command.starts_with("cd /tmp/")
            }),
            "nothing requires the second build to happen anywhere else"
        );
        assert!(
            JOBS_MUST_RUN.iter().any(|(job, command, _)| {
                *job == "deterministic" && command.contains("errors.reserved.toml")
            }),
            "the append-only ledger is not required to be diffed, and it is the one \
             artifact whose whole purpose is that it never changes except by appending"
        );
    }

    #[test]
    fn a_command_inside_a_heredoc_is_not_a_command() {
        // **The determinism job, inert, satisfying every requirement.** Read as
        // execution, its whole body wrapped in `cat <<'PLAN' >/dev/null … PLAN`
        // and followed by an `echo` leaves every `JOBS_MUST_RUN` count and the
        // whole of `IN_THIS_ORDER` satisfied — the shell runs `cat` and `echo` —
        // and `check passed`. The echoed sentence is the one `JOBS_MUST_RUN`'s
        // doc names.
        let inert = [
            "  deterministic:",
            "    steps:",
            "      - run: |",
            "          cat <<'PLAN' >/dev/null",
            "          cargo xtask build",
            "          cp -r crates/nc-catalog/src /tmp/first/nc-catalog",
            "          cp -r crates/nc-log/src /tmp/first/nc-log",
            "          cp -r docs /tmp/first/docs",
            "          cp catalog/errors.reserved.toml /tmp/first/errors.reserved.toml",
            "          cargo xtask build",
            "          diff -r /tmp/first/nc-catalog crates/nc-catalog/src",
            "          diff -r /tmp/first/nc-log crates/nc-log/src",
            "          diff -r /tmp/first/docs docs",
            "          diff /tmp/first/errors.reserved.toml catalog/errors.reserved.toml",
            "          PLAN",
            "          echo the determinism check used to be here",
        ]
        .join("\n")
            + "\n";
        let body = super::job_body(&inert, "deterministic").expect("the job is there");
        let commands = super::commands_in(&body);
        assert!(
            commands
                .iter()
                .all(|command| !command.starts_with("cargo xtask build")),
            "a heredoc's contents were read as commands: {commands:#?}"
        );
        assert!(
            commands.iter().any(|command| command.starts_with("cat ")),
            "the line that really runs was not read: {commands:#?}"
        );

        // And the same lines outside a heredoc are commands, or the assertion
        // above passes by reading nothing.
        let real = inert
            .replace("          cat <<'PLAN' >/dev/null\n", "")
            .replace("          PLAN\n", "");
        let body = super::job_body(&real, "deterministic").expect("the job is there");
        let commands = super::commands_in(&body);
        assert_eq!(
            commands
                .iter()
                .filter(|command| command.starts_with("cargo xtask build"))
                .count(),
            2,
            "{commands:#?}"
        );
    }

    #[test]
    fn a_workflow_that_never_fires_is_a_finding() {
        // **`on:` and `if:` are read.** Measured with neither read: `on:`
        // reduced to `workflow_dispatch` and `if: false` on the determinism job left
        // `ok 4 cargo command(s)` and `check passed` — CI never runs on a push,
        // the only cross-process determinism check never runs, and every gate
        // here was satisfied because each reads what the jobs *contain*.
        let scratch = std::env::temp_dir().join(format!("nc-workflow-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);

        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!(
                "on:\n  workflow_dispatch:\njobs:\n  check:\n    steps:\n      \
                 - run: cargo xtask check\n{}",
                the_real_determinism_job()
            ),
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        for event in ["push", "pull_request"] {
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains(&format!("does not run on `{event}`"))),
                "a workflow that never fires on {event} passed: {findings:#?}"
            );
        }

        // And a job switched off in place, which is a deletion with the name
        // left behind — and every other check here reads the name.
        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!("{}\n    if: false\n", gate()),
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("runs only when `false`")),
            "a job switched off in place passed: {findings:#?}"
        );

        // **And every other way to leave a step alive with its verdict
        // uncounted.** Seven were measured printing `ok 4 cargo command(s)` and
        // `check passed`; closing `if: false` and the heredoc on their own
        // closes two spellings and not the class.
        for (shape, expected) in [
            (
                "      - run: cargo xtask check || true\n",
                "swallowed by what follows `||`",
            ),
            (
                "      - run: cargo xtask check || :\n",
                "swallowed by what follows `||`",
            ),
            (
                "      - run: |\n          set +e\n          cargo xtask check\n",
                "`set +e`",
            ),
            ("    continue-on-error: true\n", "continue-on-error"),
            ("      - run: exit 0\n", "`exit 0`"),
            (
                "      - run: if false; then cargo xtask check; fi\n",
                "`if false`",
            ),
            (
                "      - run: if github.event_name == 'x'\n",
                "runs only when",
            ),
        ] {
            let ci = if expected == "runs only when" {
                format!("{}\n    if: github.event_name == 'never-happens'\n", gate())
            } else {
                format!("{}{shape}", gate())
            };
            std::fs::write(scratch.join(super::WORKFLOW), &ci).expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains(expected)),
                "a neutered workflow passed: {expected}\n{findings:#?}"
            );
        }

        // **And a trigger that fires on nothing**, which reading only the event
        // name cannot see: a branch list or a path filter decides *which*
        // pushes run the gate, and this gate's point is that every one does.
        for filter in [
            "on:\n  push:\n    branches: [does-not-exist]\n  pull_request:\n",
            "on:\n  push:\n    paths-ignore: ['**']\n  pull_request:\n",
        ] {
            let body = gate();
            let body = body.split_once("jobs:").expect("a workflow").1;
            std::fs::write(
                scratch.join(super::WORKFLOW),
                format!("{filter}jobs:{body}"),
            )
            .expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("is filtered by")),
                "a trigger that fires on nothing passed:\n{filter}{findings:#?}"
            );
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_second_gate_two_script_hops_from_the_workflow_is_still_a_finding() {
        let scratch = std::env::temp_dir().join(format!("nc-workflow-hops-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);

        // **What the reached files reach, too.** A `reached_from` that scans
        // the files the `.github/` walk produces and never what *those* reach
        // lets a workflow running one script that runs another, which runs
        // `cargo clippy` and `cargo test --workspace`, leave the count
        // unchanged, with a whole second gate in CI.
        //
        // The two script names below are written into the scratch tree and are
        // not paths in this one, which is what the markers on those lines say.
        let first = "scripts/ci-a.sh"; // allow-reference: a fixture this test writes into a scratch tree
        let second = "scripts/ci-b.sh"; // allow-reference: a fixture this test writes into a scratch tree
        std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!("jobs:\n  check:\n    steps:\n      - run: sh {first}\n"),
        )
        .expect("writable");
        std::fs::write(scratch.join(first), format!("sh {second}\n")).expect("writable");
        std::fs::write(
            scratch.join(second),
            "cargo clippy --all-targets\ncargo test --workspace\n",
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        for hidden in ["cargo clippy", "cargo test"] {
            assert!(
                findings.iter().any(|finding| finding.what.contains(hidden)),
                "`{hidden}` two hops from the workflow was invisible: {findings:#?}"
            );
        }

        // **And a job that keeps its name and loses its body.** That is the same
        // defect one level in: `deterministic:` still there, its
        // `build`-twice-and-diff replaced by an `echo`, and the gate calling it
        // `ok` — while it is the only cross-process determinism check there is.
        std::fs::write(
            scratch.join(super::WORKFLOW),
            "jobs:\n  check:\n    steps:\n      - run: cargo xtask check\n\
             \x20 deterministic:\n    steps:\n      - run: echo it used to be here\n",
        )
        .expect("writable");
        let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("`deterministic` job does not run")),
            "an emptied job body was not a finding: {findings:#?}"
        );

        // **And a system's job running something that is not the gate.** Every
        // cargo invocation is still `cargo xtask <verb>`, so the rule above is
        // satisfied while nothing runs the gate at all. The step's own *label*
        // must not satisfy it either.
        for system in ["linux", "macos", "windows"] {
            std::fs::write(
                scratch.join(super::WORKFLOW),
                format!(
                    "jobs:\n  {system}:\n    steps:\n      - name: cargo xtask check\n\
                     \x20       run: cargo xtask docs\n\
                     \x20 deterministic:\n    steps:\n      - run: diff -r a b\n"
                ),
            )
            .expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                findings.iter().any(|finding| finding
                    .what
                    .contains(&format!("`{system}` job does not run"))),
                "a `{system}` job that does not run the gate was not a finding: {findings:#?}"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The two lines every fixture needs before `jobs:`.
    ///
    /// A workflow that fires on nothing is a finding of its own, so a
    /// fixture about the *cargo* rule has to be a workflow that runs — the same
    /// reason each of them carries both jobs.
    const FIRES: &str = "on:\n  push:\n  pull_request:\n";

    /// The macOS and Windows gates, each on its runner, which a fixture carries
    /// beside the job it is about so that it is not refused for lacking them.
    const THE_OTHER_SYSTEMS: &str = "  macos:\n    runs-on: macos-latest\n    steps:\n      \
                                     - run: cargo xtask check\n  windows:\n    runs-on: \
                                     windows-latest\n    steps:\n      - run: cargo xtask \
                                     check\n";

    /// The two jobs a fixture must carry, since a workflow without them is a
    /// different finding and these tests are about the cargo rule.
    fn gate() -> String {
        format!(
            "{FIRES}jobs:\n  check:\n    steps:\n      - run: cargo xtask check\n{}",
            the_real_determinism_job()
        )
    }

    /// A scratch tree shaped like a repository with CI in it.
    ///
    /// Named per test rather than per process: these run in parallel, and one
    /// deleting the directory another is writing to is a flake.
    fn ways_scratch(about: &str) -> (crate::check::LeavesNothingBehind, std::path::PathBuf) {
        let held = crate::check::LeavesNothingBehind::new(&format!("ways-{about}"));
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);
        std::fs::create_dir_all(scratch.join(".github/actions/extra")).expect("writable");
        (held, scratch)
    }

    /// Writes one fixture and asserts the gate found something in it.
    fn caught(
        scratch: &Path,
        ci: &str,
        other: Option<(&str, &str)>,
        about: &str,
    ) -> Vec<crate::gates::Finding> {
        std::fs::write(scratch.join(super::WORKFLOW), ci).expect("writable");
        let _ = std::fs::remove_file(scratch.join(".github/actions/extra/action.yml"));
        let _ = std::fs::remove_file(scratch.join(".github/workflows/nightly.yml"));
        if let Some((where_, body)) = other {
            std::fs::write(scratch.join(where_), body).expect("writable");
        }
        let (findings, _) = only_runs_the_gate(scratch).expect("readable");
        assert!(!findings.is_empty(), "{about} was not caught");
        findings
    }

    #[test]
    fn a_job_that_echoes_its_commands_runs_none_of_them() {
        // **What the job runs, not what it mentions.** A scan keeping every
        // line that starts `run:` *or holds no colon* keeps every argument of
        // an `echo` and every `#` comment — so a body of `echo cargo xtask
        // build`, `echo diff -r …` satisfies every requirement with `check
        // passed`, its first line reading verbatim "the determinism check used
        // to be here". With the body reduced to `run: true`, `cargo xtask
        // build` is satisfied by the job's own committed prose.
        let (_held, scratch) = ways_scratch("echoed");
        for body in [
            "      - run: |\n             \x20         echo the determinism check used to be here\n             \x20         echo cargo xtask build\n             \x20         echo cp -r crates/nc-catalog/src /tmp/first/nc-catalog\n             \x20         echo cp -r docs /tmp/first/docs\n             \x20         echo diff -r /tmp/first/nc-catalog crates/nc-catalog/src\n             \x20         echo diff -r /tmp/first/docs docs\n",
            "      # cargo xtask build, cp -r crates/nc-catalog/src, cp -r docs,\n             \x20     # diff -r /tmp/first/nc-catalog crates/nc-catalog/src,\n             \x20     # diff -r /tmp/first/docs docs\n             \x20     - run: true\n",
        ] {
            std::fs::write(
                scratch.join(super::WORKFLOW),
                format!(
                    "jobs:\n  check:\n    steps:\n      - run: cargo xtask check\n                       deterministic:\n    steps:\n{body}"
                ),
            )
            .expect("writable");
            let (findings, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                findings.len() >= 5,
                "a job that only mentions its commands satisfied them: {findings:#?}"
            );
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn every_way_a_second_gate_was_added_without_this_noticing() {
        // Ordinary shapes, each measured putting a second cargo command into CI
        // with `check passed` and this gate's count unchanged.
        let (_held, scratch) = ways_scratch("shapes");

        // A second workflow file, which is a whole second gate.
        let found = caught(
            &scratch,
            &gate(),
            Some((".github/workflows/nightly.yml", "      - run: cargo test\n")),
            "a second workflow file",
        );
        assert!(found[0].file.ends_with("nightly.yml"), "{found:#?}");

        // A composite action, whose `run:` a workflow-only reader never opens.
        let found = caught(
            &scratch,
            &gate(),
            Some((
                ".github/actions/extra/action.yml",
                "    - run: cargo clippy\n",
            )),
            "a composite action",
        );
        assert!(found[0].file.contains("actions/extra"), "{found:#?}");

        // Shell indirection, which never spells `cargo ` at the call.
        let found = caught(
            &scratch,
            &format!(
                "{}      - run: |\n          C=cargo\n          $C clippy\n",
                gate()
            ),
            None,
            "a shell alias",
        );
        assert!(found[0].what.contains("C=cargo"), "{found:#?}");
        // And a path spelling of the same thing.
        let found = caught(
            &scratch,
            &format!("{}      - run: CARGO=/usr/bin/cargo\n", gate()),
            None,
            "an aliased path",
        );
        assert!(found[0].what.contains("CARGO=cargo"), "{found:#?}");

        // A tab, which is the same command to every shell there is.
        let found = caught(
            &scratch,
            &format!("{}      - run: cargo\ttest\n", gate()),
            None,
            "a tab",
        );
        assert!(found[0].what.contains("cargo test"), "{found:#?}");

        // **The standard action**, which needs no `run:` line at all.
        let found = caught(
            &scratch,
            &format!(
                "{}      - uses: actions-rs/cargo@v1\n        with:\n          command: clippy\n",
                gate()
            ),
            None,
            "actions-rs/cargo",
        );
        assert!(found[0].what.contains("actions-rs/cargo"), "{found:#?}");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn every_file_ci_reaches_is_read_wherever_it_lives() {
        let (_held, scratch) = ways_scratch("reached");

        // **A script beside the workflow**, which a YAML-only walk filters out
        // while the module header says the directory is walked.
        std::fs::create_dir_all(scratch.join(".github/scripts")).expect("writable");
        std::fs::write(
            scratch.join(".github/scripts").join("extra.sh"),
            "cargo clippy\n",
        )
        .expect("writable");
        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!("{}      - run: bash .github/scripts/extra.sh\n", gate()),
        )
        .expect("writable");
        let (found, _) = only_runs_the_gate(&scratch).expect("readable");
        assert!(
            found
                .iter()
                .any(|finding| finding.file.ends_with("extra.sh")),
            "a script under .github/ was not read: {found:#?}"
        );
        std::fs::remove_dir_all(scratch.join(".github/scripts")).expect("writable");

        // **Three shapes outside `.github/`**, which is not CI's boundary: CI's
        // boundary is what its steps reach.
        std::fs::create_dir_all(scratch.join("scripts")).expect("writable");
        std::fs::create_dir_all(scratch.join("ci/actions/extra")).expect("writable");
        std::fs::write(scratch.join("scripts/ci-extra.sh"), "cargo clippy\n").expect("writable");
        std::fs::write(
            scratch.join("ci/actions/extra/action.yml"),
            "runs:\n  using: composite\n  steps:\n    - run: cargo test\n",
        )
        .expect("writable");
        for (step, expected) in [
            (
                "      - run: bash scripts/ci-extra.sh\n",
                "scripts/ci-extra.sh",
            ),
            ("      - uses: ./ci/actions/extra\n", "ci/actions/extra"),
        ] {
            std::fs::write(scratch.join(super::WORKFLOW), format!("{}{step}", gate()))
                .expect("writable");
            let (found, _) = only_runs_the_gate(&scratch).expect("readable");
            assert!(
                found.iter().any(|finding| finding.file.contains(expected)),
                "`{expected}` is what CI runs and was not read: {found:#?}"
            );
        }
        std::fs::remove_dir_all(scratch.join("scripts")).expect("writable");
        std::fs::remove_dir_all(scratch.join("ci")).expect("writable");

        // And cargo called by its path, which is a fourth.
        let found = caught(
            &scratch,
            &format!(
                "{}      - run: ~/.cargo/bin/cargo fmt --all --check\n",
                gate()
            ),
            None,
            "cargo by path",
        );
        assert!(found[0].what.contains("cargo fmt"), "{found:#?}");

        // And the two remaining spellings of an alias.
        for (line, names) in [
            ("          CARGO=$(which cargo)", "CARGO=cargo"),
            ("          alias c=cargo", "c=cargo"),
        ] {
            let found = caught(
                &scratch,
                &format!("{}      - run: |\n{line}\n", gate()),
                None,
                names,
            );
            assert!(found[0].what.contains(names), "{found:#?}");
        }

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// The `deterministic:` job as this repository actually writes it.
    ///
    /// **Taken from `ci.yml` rather than written out here.** Three fixtures need
    /// a determinism job that satisfies [`super::JOBS_MUST_RUN`], and copying
    /// its commands into each one makes adding a requirement a four-place edit —
    /// which is how a pair comes to be in `ci.yml` and absent from the list.
    ///
    /// Not circular: these fixtures are about *cargo-command detection*, and the
    /// job body is a constant they need to be valid. Whether the shipped job is
    /// itself right is what `this_repositorys_workflow_runs_only_the_gate` asks,
    /// against the real file.
    fn the_real_determinism_job() -> String {
        let ci = std::fs::read_to_string(root().join(super::WORKFLOW)).expect("ci.yml is there");
        let mut lines = ci
            .lines()
            .skip_while(|line| line.trim_start() != "deterministic:");
        let first = lines.next().expect("the job is there");
        let indent = first.len() - first.trim_start().len();
        let body: Vec<&str> = lines
            .take_while(|line| {
                line.trim().is_empty() || line.len() - line.trim_start().len() > indent
            })
            .collect();
        format!("{first}\n{}\n", body.join("\n"))
    }

    /// A wrapper carries command position where nothing else reaches.
    ///
    /// **The wrapper fixtures above are answered by a different clause.** Every
    /// wrapper case in `the_attacks_this_gate_survives` and beside it is a
    /// `run:` line, and `a_command_line` is true for every one of those — so
    /// `after_a_wrapper` decides none of them: deleting it from both
    /// disjunctions in `cargo_commands` leaves every other test in this module
    /// green. A clause that no fixture reaches is a clause that can be deleted,
    /// and the list it reads can lose a word, with nothing anywhere saying so.
    ///
    /// What the stickiness is actually for is a line that is **not** a command
    /// line — a `with:` block's `args:`, handed to an action that runs it —
    /// where the binary sits two words past the wrapper, so neither
    /// `a_command_line` nor command position reaches it.
    ///
    /// Every word in the list, not a handful picked by hand: a word
    /// added to it is driven the moment it is written, and a word removed fails
    /// here.
    #[test]
    fn a_wrapper_reaches_a_judging_binary_where_no_other_clause_does() {
        // The control, first: the same line with no wrapper in it is reached by
        // nothing, which is what makes each case below about the wrapper alone.
        let left_over = super::cargo_commands("          args: rustfmt --check src/lib.rs");
        assert!(
            left_over.is_empty(),
            "a judging binary on a line that is not a command, in no command \
             position, was reported by something other than the wrapper rule: {left_over:?}"
        );
        // **Against a list written here, not against the one being measured.**
        // Driving `PUTS_A_COMMAND_AFTER_IT` itself is a fixture answered by the
        // thing it measures: deleting `nohup` from the const deletes its own
        // driver and such a test stays green — measured. So the
        // words are written down once more, and the two lists are compared. A
        // word added to the rule without a line here fails; a word removed
        // fails too.
        assert_eq!(
            super::PUTS_A_COMMAND_AFTER_IT,
            [
                "bash", "sh", "zsh", "dash", "xargs", "env", "time", "nice", "rustup", "timeout",
                "sudo", "stdbuf", "nohup", "find",
            ],
            "the words that put a command after them are not the ones driven below"
        );
        for wrapper in super::PUTS_A_COMMAND_AFTER_IT {
            let line = format!("          args: {wrapper} -x rustfmt --check src/lib.rs");
            let found = super::cargo_commands(&line);
            assert_eq!(
                found.len(),
                1,
                "`{wrapper}` did not carry command position to the binary two words \
                 after it: {found:?}"
            );
            assert!(
                found[0].starts_with("rustfmt"),
                "`{wrapper}` reported something other than the binary it runs: {found:?}"
            );
        }
    }

    /// A construct opened and closed on one line wraps only that line.
    ///
    /// **Reading the first word of a line is not reading the line.** A depth
    /// that goes up on `if` and comes down only on a line *beginning* with `fi`
    /// stays one higher for the rest of the file after
    /// `if [ "$a" = "$b" ]; then a=; else a=${a#*/}; fi`, which is ordinary
    /// shell and closes what it opens. Measured: one such line in
    /// `xtask/the-checkout-shape.sh` makes such a rule report that the script's
    /// `cargo xtask check`, forty lines and three closed constructs later, runs
    /// inside a `while` — the rule refusing correct code, which is the way to be
    /// wrong that teaches everyone to stop reading it.
    ///
    /// And the same blindness the other way: to a first-word reader an opener
    /// that is not the first word (`*) case $x in`) is no opener, and a closer
    /// carrying its arm's punctuation (`esac;;`) is no closer — so a verdict
    /// nested two constructs deep can be walked out of and the rule says
    /// nothing.
    #[test]
    fn a_construct_opened_and_closed_on_one_line_wraps_only_that_line() {
        let closed = "if [ \"$a\" = \"$b\" ]; then a=; else a=${a#*/}; fi\ncargo xtask check\n";
        assert!(
            !super::wraps_a_command(closed, 0, true),
            "a construct that closes on its own line was read as holding everything after it"
        );
        // Both halves of the other direction: an opener that does not begin the
        // line, and a closer wearing the punctuation of the arm it sits in.
        let nested = "while [ -n \"$r\" ]; do\n                        case $p in\n                          *) case $s in\n                               /) s=/$p;;\n                             esac;;\n                        esac\ndone\ncargo xtask check\n";
        assert!(
            !super::wraps_a_command(nested, 0, true),
            "a nested construct was not counted, so the walk left the loop early or late"
        );
        // And what the rule is for, which still has to fire.
        let wrapped = "while false; do\n  cargo xtask check\ndone\n";
        assert!(
            super::wraps_a_command(wrapped, 0, true),
            "a verdict inside `while false` is a verdict of nothing and was read as a \
             verdict"
        );
        // A word inside a message is not a construct.
        let spoken = "if [ -n \"$a\" ]; then echo \"done for now\"; fi\ncargo xtask check\n";
        assert!(
            !super::wraps_a_command(spoken, 0, true),
            "a quoted `done` was counted as closing a construct"
        );
        // **And a verdict that is not a cargo invocation is still a verdict.**
        // Asked of `cargo_commands` alone, the determinism job's four `diff`
        // lines — the whole of what that job compares — are no command at all:
        // `until true; do … done` around them leaves both builds running, the
        // copy made, every required line present for the readers that count
        // text, and nothing compared.
        let diffed = concat!(
            "until true; do\n",
            "  diff -r /tmp/a-second-checkout-of-noctua-central/docs \"$GITHUB_WORKSPACE\"/docs\n",
            "done\n"
        );
        assert!(
            super::wraps_a_command(diffed, 0, true),
            "the determinism job's verdict is a `diff`, and a construct around it was read \
             as holding no command"
        );
        // **And a required line is a verdict only where it is required.**
        // `cpio -pdm` is a row of `JOBS_MUST_RUN` because `ci.yml`'s
        // determinism job must make a second copy. The same command appears in
        // `xtask/the-checkout-shape.sh`, which this gate also walks, where it
        // makes that script's scratch copy and judges nothing — so asked of
        // every file, an ordinary guard around it is reported as a verdict
        // decided at run time, which is the false positive `wraps_a_command`
        // exists to stop. Measured.
        let copied = concat!(
            "if [ -d \"$scratch/checkout\" ]; then\n",
            "  ( cd \"$here\" && find . | cpio -pdm --quiet \"$scratch/checkout\" )\n",
            "fi\n"
        );
        assert!(
            super::wraps_a_command(copied, 0, true),
            "a guard around a required line of the workflow is a guard around a verdict"
        );
        assert!(
            !super::wraps_a_command(copied, 0, false),
            "a guard around a copy, in a file where that line is required of nobody, was \
             reported as a verdict decided at run time"
        );

        // **And a cargo invocation is a verdict in a file no row is about.**
        // The two halves of [`super::a_command_this_gate_requires`] can answer
        // each other's fixtures: every case above reaches the `JOBS_MUST_RUN`
        // half, so without this case deleting the cargo half leaves the suite
        // green. What only it decides is the justfile and the checkout script,
        // which no row of that list is written about and where a guard around
        // `cargo xtask check` is the gate running or not depending on the day.
        let recipe = "until true; do\n  cargo xtask check\ndone\n";
        assert!(
            super::wraps_a_command(recipe, 0, false),
            "a guard around `cargo xtask check` was read as holding no command in a file \
             the required-line list says nothing about"
        );

        // **And the quoted spans are spent here too.** This walk strips them
        // before it counts openers and closers, and the fixture above proving
        // it — `echo \"done for now\"` — is answered by `begins_a_statement`
        // instead, because a word at the end of a message begins no statement.
        // A message that *contains* a statement boundary is what only the
        // spending decides: without it the loop closes on the quoted `done`,
        // the verdict below reads as standing at the top level, and a
        // `cargo xtask check` that never runs is reported as one that does.
        let quoted = concat!(
            "until true; do\n",
            "  echo \"step 1 ; done ; step 2\"\n",
            "  cargo xtask check\n",
            "done\n"
        );
        assert!(
            super::wraps_a_command(quoted, 0, true),
            "a `done` inside a message closed the construct around a verdict, so the \
             verdict was read as running unconditionally"
        );
        // **And an unquoted one is not either.** Spending quoted spans and then
        // counting every bare `done` leaves `echo first build done` closing the
        // construct around the verdict, so the walk answers "nothing wrapped"
        // before it reaches the cargo line, and nothing reports a gate whose
        // running is decided at run time. The fixture above is the quoted
        // spelling; this is the unquoted one.
        let bare = "if [ -n \"$a\" ]; then\n  echo first build done\n  cargo xtask check\nfi\n";
        assert!(
            super::wraps_a_command(bare, 0, true),
            "an unquoted `done` in a message closed the construct around a verdict"
        );
    }

    #[test]
    fn the_shapes_that_must_stay_legal_still_are() {
        // So that none of the shapes the two tests above catch closes the gate
        // onto the workflow this repository actually ships.
        let (_held, scratch) = ways_scratch("legal");
        std::fs::write(
            scratch.join(super::WORKFLOW),
            format!(
                "{FIRES}jobs:\n  linux:\n    runs-on: ubuntu-latest\n    steps:\n\
                 \x20     - name: cargo clippy is not run here\n\
                 \x20     - run: cargo install --locked cargo-deny\n\
                 \x20     - run: cargo xtask check\n\
                 \x20     - run: echo ~/.cargo/bin\n\
                 \x20     # a comment mentioning cargo clippy\n{THE_OTHER_SYSTEMS}{}",
                the_real_determinism_job()
            ),
        )
        .expect("writable");
        let (findings, seen) = only_runs_the_gate(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(
            seen, 6,
            "the allowed invocations were not all counted: the gate, the install, the \
             gates on the other two systems, and the deterministic job's two builds"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Every form `just` gives a parameter is a form this gate reads.
    ///
    /// **The refusal is loud, which is why a gap in it costs so much.** When
    /// `a_recipe_runs_what_it_is_called` cannot find an interpolation among the
    /// declared names it says "A recipe's own parameters are declared in its
    /// header; this one is not" — so a spelling it does not know turns a
    /// correct `justfile` into a sentence stating the inverse of the truth —
    /// for the `name="default"` form, and for `+args`, `*args` and `$name`.
    ///
    /// The last row is the floor: a sigil trimmed off the *declared* name must
    /// not start excusing a body that names something else.
    #[test]
    fn every_way_just_declares_a_parameter_is_one_this_gate_reads() {
        // `the_recipe_above` hands this rule the header with everything from
        // its `:` onward already taken off, so these are written the way the
        // caller passes them and not the way the file spells them. The
        // spellings *it* has to survive — a dependency after the colon, an `@`
        // in front of the name — are pinned in
        // [`every_shape_of_recipe_header_is_read_as_one`] beside this.
        for (header, command, refused) in [
            (
                "release version",
                "    cargo xtask release {{version}}",
                false,
            ),
            (
                "release version=\"0.1.0\"",
                "    cargo xtask release {{version}}",
                false,
            ),
            (
                "release +version",
                "    cargo xtask release {{version}}",
                false,
            ),
            (
                "release *version",
                "    cargo xtask release {{version}}",
                false,
            ),
            (
                "release $version",
                "    cargo xtask release {{version}}",
                false,
            ),
            (
                "release +version",
                "    cargo xtask release {{elsewhere}}",
                true,
            ),
        ] {
            let said = super::a_recipe_runs_what_it_is_called("release", header, command);
            assert_eq!(
                said.is_some(),
                refused,
                "`{header}` with `{command}` — the gate said {said:?}"
            );
        }
    }

    /// A default written across lines is one string, and its content is not a header.
    ///
    /// **A line is not a string.** Measured against `just 1.55.1`: the file
    /// below is ONE recipe named `serve` with two parameters — `just --summary`
    /// prints `serve`, `just --dump` echoes it back unchanged. A one-line
    /// reader sees three lines: the header opens a triple that never closes,
    /// the closing line begins mid-string, and the *content* line `a:b` stands
    /// at column zero, is not a comment and holds a colon — so it is taken for
    /// the recipe header and the body's parameters are then judged against the
    /// string's own text.
    #[test]
    fn a_default_written_across_lines_is_not_three_recipes() {
        let text = "serve x=\"\"\"\na:b\n\"\"\" y=\"d\":\n    @echo \"{{x}} {{y}}\"\n";
        let body = text.lines().count() - 1;
        assert_eq!(
            super::the_recipe_above(text, body, false, "justfile"),
            "serve x=\"\"\"\na:b\n\"\"\" y=\"d\"",
            "the header of a recipe whose default spans lines is not read as one recipe"
        );
        // **An assignment's own triple crosses lines too.** A reader that
        // returns at the first top-level colon reports no string open for
        // `flags := \"\"\"`, ends the logical line there, and takes the string's
        // content line `check: not a recipe` for the recipe header. Measured
        // against `just 1.55.1`: one assignment plus one recipe — `--summary`
        // prints `release`, `--dump` echoes the file back, `just release 9`
        // runs it.
        let assigned = "flags := \"\"\"\ncheck: not a recipe\n\"\"\"\nrelease version:\n    \
                        cargo xtask release {{version}}\n";
        assert_eq!(
            super::the_recipe_above(assigned, assigned.lines().count() - 1, false, "justfile"),
            "release version",
            "the content line of a multi-line assignment was read as the recipe header"
        );
        // **A `#` comment holding a triple opens nothing.** `just` ends a
        // comment at the newline, so the quote inside one is not a string; a
        // reader that carries it swallows every line below into one group.
        let commented = "# a note about \"\"\" quoting\nrelease version:\n    \
                         cargo xtask release {{version}}\n";
        assert_eq!(
            super::the_recipe_above(commented, commented.lines().count() - 1, false, "justfile"),
            "release version",
            "a comment holding a triple quote opened a string that never closed"
        );
        // And the two legitimate spellings the `#` guard must not refuse, both
        // of which `just --dump` echoes back verbatim.
        for (what, text, header) in [
            (
                "a hash inside a quoted default",
                "release version=\"a#b\":\n    cargo xtask release {{version}}\n",
                "release version=\"a#b\"",
            ),
            (
                "a trailing comment after the header",
                "release version: # the gate this wraps\n    cargo xtask release {{version}}\n",
                "release version",
            ),
        ] {
            assert_eq!(
                super::the_recipe_above(text, text.lines().count() - 1, false, "justfile"),
                header,
                "{what}: a legitimate recipe was refused"
            );
        }
    }

    /// Every way `just` writes a recipe header is one this walk finds.
    ///
    /// **One sentence in this rule turns false whenever a header is misread.**
    /// "A recipe's own parameters are declared in its header; this one is not"
    /// is the inverse of the truth for `name="default"`, for `+args`, `*args`
    /// and `$name`, and for the two here — a dependency written after the
    /// colon, which stops the line being read as a header at all, and an `@`
    /// kept on the name. `every_way_just_declares_a_parameter_is_one_this_gate_reads`
    /// is handed the header *as a parameter*, so by construction it cannot see
    /// a header the reader fails to find; this one drives the reader.
    ///
    /// The last two rows are the floor. `flags := "--offline"` is an
    /// assignment, not a recipe, and a line in a file that is not a justfile is
    /// not a header however many colons it holds — `xtask/the-checkout-shape.sh`
    /// carries `keep=${1:+the caller named it}` at column zero, and a reader
    /// that takes it for one refuses this repository's own script.
    #[test]
    fn every_shape_of_recipe_header_is_read_as_one() {
        let body = "\n    cargo xtask release {{version}}\n";
        for (what, text, expected) in [
            ("the plain header", "release version:", "release version"),
            (
                "a dependency after the colon",
                "release version: build",
                "release version",
            ),
            ("the echo sigil", "@release version:", "@release version"),
            (
                "a default and a dependency together",
                "release version=\"0.1.0\": build",
                "release version=\"0.1.0\"",
            ),
            (
                "an assignment is not a recipe",
                "flags := \"--offline\"",
                "",
            ),
            // **A default may hold a colon.** `just` takes an address as a
            // default like any other string, and a reader that cuts at the
            // first colon loses every parameter after it — and with the
            // parameters the other way round the *name* rule fires instead,
            // naming a verb the recipe never claims.
            (
                "a colon inside a quoted default",
                "serve addr=\"127.0.0.1:8080\" mode=\"dev\":",
                "serve addr=\"127.0.0.1:8080\" mode=\"dev\"",
            ),
            (
                "the same, with the parameters the other way round",
                "serve mode=\"dev\" addr=\"127.0.0.1:8080\":",
                "serve mode=\"dev\" addr=\"127.0.0.1:8080\"",
            ),
            // And an escaped quote inside one, which a reader that takes `\"`
            // for a terminator loses the whole recipe to — the same hole one
            // escape along.
            (
                "an escaped quote inside a default",
                "greet msg=\"it\\\"s\":",
                "greet msg=\"it\\\"s\"",
            ),
            (
                "a single-quoted default holding a colon",
                "serve addr='127.0.0.1:8080':",
                "serve addr='127.0.0.1:8080'",
            ),
            // **`just`'s single quotes are raw**, so a backslash in one is a
            // backslash — which is the whole reason a recipe would use them.
            // A reader that escapes inside them never closes the quote and the
            // line stops being a header.
            (
                "a raw default ending in a backslash",
                "build sep='\\':",
                "build sep='\\'",
            ),
            // A command substitution as a default, which `just` accepts like
            // any other string and whose colon is data.
            // **`just` has six string forms, not three.** Each of the three
            // below is parsed by `just 1.55.1` as one recipe with two
            // parameters — measured — and a reader of three forms reads each
            // as three separate delimiters, so a quote inside the literal
            // toggles the state and the colon inside it is taken for the
            // header's own. The header comes back truncated, the second
            // parameter is gone, and the body's use of it is refused with a
            // sentence that is the inverse of the truth.
            (
                "a triple-quoted default holding a quote and a colon",
                "serve x=\"\"\"a\"b:c\"\"\" y=\"d\":",
                "serve x=\"\"\"a\"b:c\"\"\" y=\"d\"",
            ),
            (
                "a triple raw default holding a quote and a colon",
                "serve x='''a'b:c''' y='d':",
                "serve x='''a'b:c''' y='d'",
            ),
            (
                "a triple backtick default holding a backtick and a colon",
                "serve x=```a`b:c``` y=\"d\":",
                "serve x=```a`b:c``` y=\"d\"",
            ),
            (
                "a backtick default holding a colon",
                "run t=`date +%H:%M`:",
                "run t=`date +%H:%M`",
            ),
        ] {
            let file = format!("{text}{body}");
            let at = file.lines().count() - 2;
            let (header, _) = super::the_recipe_and_its_name(&file, at, false, "justfile");
            assert_eq!(header, expected, "{what}: {text:?}");
        }

        // And the name, which the sigil is not part of.
        let file = format!("@release version:{body}");
        let (_, name) = super::the_recipe_and_its_name(&file, 1, false, "justfile");
        assert_eq!(
            name, "release",
            "`just --list` prints the name without the sigil"
        );

        // A colon-bearing line in a file that is not a justfile is not a header.
        let script = "keep=${1:+the caller named it}\ncargo xtask check\n";
        let (header, name) =
            super::the_recipe_and_its_name(script, 1, false, "xtask/the-checkout-shape.sh");
        assert_eq!(
            (header.as_str(), name.as_str()),
            ("", ""),
            "a shell parameter expansion was read as a recipe header"
        );
    }

    #[test]
    fn a_second_gate_command_is_a_finding_and_the_two_allowed_shapes_are_not() {
        let scratch = std::env::temp_dir().join(format!("nc-workflow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        the_shipped_alias(&scratch);
        // Every fixture carries the jobs, because a workflow without them is a
        // different finding and this test is about the cargo rule.
        let write = |body: &str| {
            std::fs::write(
                scratch.join(super::WORKFLOW),
                format!(
                    "{FIRES}jobs:\n  linux:\n    runs-on: ubuntu-latest\n    steps:\n{body}\
                     {THE_OTHER_SYSTEMS}{}",
                    the_real_determinism_job()
                ),
            )
            .expect("writable");
        };

        // The shape the rule exists to refuse: the gate, plus a step that does
        // one of the things the gate already does.
        write("      - run: cargo xtask check\n      - run: cargo clippy -- -D warnings\n");
        let (findings, seen) = only_runs_the_gate(&scratch).expect("readable");
        assert_eq!(
            seen, 6,
            "the gate, the offender, the other two systems' gates, and the two builds"
        );
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings[0].what.contains("cargo clippy"),
            "{}",
            findings[0].what
        );

        // And the two shapes that are allowed, so the gate is not simply refusing
        // every cargo line it sees.
        write(
            "      - run: cargo xtask check\n      - run: cargo install --locked cargo-deny\n\
             \x20     - run: cargo xtask build\n",
        );
        let (findings, seen) = only_runs_the_gate(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(seen, 7, "and the other systems' gates and the two builds");

        // A comment describing the rule is prose, not a command — this file's own
        // header names `cargo fmt` and `cargo test` while explaining them.
        write("      # never: cargo test, cargo fmt\n      - run: cargo xtask check\n");
        let (findings, seen) = only_runs_the_gate(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(seen, 5, "the comment was read as a command");

        // And a step named after what it runs, which is how CI files are
        // ordinarily written. Reading the label as a second invocation would
        // report every well-named workflow there is.
        write(
            "      - run: cargo xtask check\n      - name: cargo deny\n\
             \x20       run: cargo install --locked cargo-deny\n",
        );
        let (findings, seen) = only_runs_the_gate(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        assert_eq!(seen, 6, "the step's name was read as a command");

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn a_missing_workflow_is_an_error_and_not_an_empty_pass() {
        // The §19.3 shape: a gate whose subject has been deleted must not report
        // the loudest success it has.
        let nowhere = std::env::temp_dir().join(format!("nc-workflow-gone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&nowhere);
        std::fs::create_dir_all(&nowhere).expect("writable");
        let error = only_runs_the_gate(&nowhere).expect_err("must fail");
        assert!(error.contains(".github/workflows/ci.yml"), "{error}");
        let _ = std::fs::remove_dir_all(&nowhere);
    }

    #[test]
    fn a_label_is_told_apart_from_a_command() {
        assert!(is_label("- name: cargo xtask check"));
        assert!(is_label("name: check"));
        assert!(!is_label("- run: cargo xtask check"));
        assert!(!is_label("cargo xtask build"));

        // **A `name:` is a label whatever is inside it**, and `commands_in` has
        // to agree with the half of this file that reads cargo invocations.
        // Skipping a key line only when it holds no `&&` reads a step named
        //
        // ```text
        // - name: nothing && cargo xtask build && cp -r … && diff -r …
        // ```
        //
        // as six commands. What is asserted here is the behaviour, not
        // the line that produces it: `commands_in`'s own `contains(':')`
        // fallback below the `is_label` call reaches this shape as well, so
        // deleting either one alone leaves this passing. Two readers agreeing is
        // the point — the defect is them disagreeing — and the assertion is on
        // the answer they have to agree about.
        let named = "      - name: nothing && cargo xtask build && diff -r a b\n";
        assert!(
            super::commands_in(named).is_empty(),
            "a step's name was read as the commands inside it: {:?}",
            super::commands_in(named)
        );
        // And the same line as a `run:` is six commands, so the assertion above
        // is about the key and not about the text after it.
        let run = "      - run: nothing && cargo xtask build && diff -r a b\n";
        assert_eq!(
            super::commands_in(run).len(),
            3,
            "{:?}",
            super::commands_in(run)
        );
    }

    #[test]
    fn a_command_is_named_briefly_enough_to_read() {
        assert_eq!(
            first_words("cargo clippy --workspace -- -D warnings"),
            "cargo clippy --workspace"
        );
        assert_eq!(first_words("cargo test"), "cargo test");
    }

    /// A key is a word, a colon, and then a space or the end of the line.
    ///
    /// **Both halves, decided here.** The rule's own paragraph says what they
    /// are for — keeping `https://example.com` and `cargo:rerun-if-changed=x`
    /// from reading as keys — and without this test, turning each off leaves
    /// every other test in this crate green.
    ///
    /// It matters because this predicate is what tells a key from the
    /// *continuation of the value above it*: a line wrongly read as a key ends
    /// the command being accumulated, and the half of that command below it is
    /// never judged. `--offline` on its own line is exactly such a half.
    #[test]
    fn a_key_is_a_word_then_a_colon_then_a_space_or_the_end_of_the_line() {
        for opens in ["run: cargo xtask check", "steps:", "uses: actions/checkout"] {
            assert!(super::opens_a_key(opens), "{opens:?} opens a key");
        }

        // **A colon with no space after it is a plain scalar, not a mapping.**
        // Read as a key, the value above it stops being accumulated there.
        for value in ["image:postgres", "cargo:rerun-if-changed=build.rs"] {
            assert!(
                !super::opens_a_key(value),
                "{value:?} is a value: YAML wants a space after the colon"
            );
        }

        // **And a line that begins with the colon has no key at all.**
        assert!(
            !super::opens_a_key(": nothing stands before this"),
            "a line opening with a colon names nothing"
        );

        // The shapes the paragraph names, kept here so the reason cannot drift
        // from the rule: both are ordinary text inside a wrapped value.
        assert!(!super::opens_a_key("see https://example.com/x for why"));
        assert!(!super::opens_a_key("--offline"));
    }

    /// The plugins' target is asked for by both places a toolchain comes from.
    ///
    /// **`cargo xtask check` builds every plugin for that target before the
    /// tests, and neither source of a toolchain installs a target it is not asked
    /// for.** rustup reads `[toolchain].targets` in `rust-toolchain.toml`; each
    /// job that runs the gate (`linux`, `macos`, `windows`) installs through an
    /// action that does not read that file and takes its own `targets:`. A
    /// machine that already has the target stays green with either line
    /// deleted, and a fresh clone or the runner then fails its first gate at
    /// the `plugins` step — which is why the two are read here rather than left
    /// to that failure.
    #[test]
    fn the_plugins_target_is_asked_for_by_the_toolchain_file_and_by_ci() {
        let target = crate::plugins::TARGET;
        let toolchain = std::fs::read_to_string(root().join("rust-toolchain.toml"))
            .expect("rust-toolchain.toml is readable");
        let named: Vec<String> = toolchain
            .parse::<toml::Table>()
            .expect("rust-toolchain.toml is TOML rustup can read")
            .get("toolchain")
            .and_then(toml::Value::as_table)
            .and_then(|table| table.get("targets"))
            .and_then(toml::Value::as_array)
            .map(|targets| {
                targets
                    .iter()
                    .filter_map(|name| name.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            named.iter().any(|name| name == target),
            "`rust-toolchain.toml`'s `[toolchain]` asks for the targets {named:?} and not \
             `{target}`, which `cargo xtask check` builds every plugin for; with \
             `profile = \"minimal\"` a fresh clone that does not ask for it does not have it"
        );

        let ci = std::fs::read_to_string(root().join(WORKFLOW)).expect("the workflow is readable");
        // **Every job that runs the gate**, because each builds the plugins on
        // its own runner.
        for system in ["linux", "macos", "windows"] {
            let job = job_body(&ci, system)
                .unwrap_or_else(|| panic!("the workflow has a `{system}` job"));
            let installed: Vec<String> = job
                .lines()
                .filter_map(|line| line.trim().strip_prefix("targets:"))
                .flat_map(|value| value.split(','))
                .map(|name| name.trim().trim_matches(['"', '\'', '[', ']']).to_owned())
                .collect();
            assert!(
                installed.iter().any(|name| name == target),
                "the `{system}` job's toolchain step installs the targets {installed:?} and \
                 not `{target}`. The action it uses does not read `rust-toolchain.toml`, so \
                 the target is asked for there as well, as `targets:` beside `components:`"
            );
        }
    }
}
