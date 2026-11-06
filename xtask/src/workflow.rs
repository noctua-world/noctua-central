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

/// The file a release is made from (`NOCTUA.md §22.6`).
pub(crate) const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";

/// Everything [`WORKFLOWS`] holds: the gate's workflow and the release's.
///
/// **A list, because every file there is one GitHub runs or one a step can
/// reach.** A third workflow is a second gate, or a second way into the
/// environments that hold the release App's key, and a script or a composite
/// action there is code a step runs. One more is a line here, which is a line
/// in a diff.
const THE_WORKFLOWS: [&str; 2] = [WORKFLOW, RELEASE_WORKFLOW];

/// The branches a push to which makes a release: each is a channel
/// (`NOCTUA.md §22.1`), and the release is made on the one pushed to.
const RELEASED_FROM: [&str; 2] = ["staging", "main"];

/// The events that run a workflow with this repository's authority on behalf
/// of a change made somewhere else.
///
/// `pull_request_target` runs on a pull request from a fork with this
/// repository's secrets and a token that may write, and `workflow_run` runs
/// after another workflow has, a fork's pull request's among them, with the
/// same. No workflow here is started either way.
const WITH_THIS_REPOSITORYS_AUTHORITY: [&str; 2] = ["pull_request_target", "workflow_run"];

/// What a job may ask its workflow's token for: reading the tree it checks out.
///
/// **And nothing else, in either workflow.** The gate writes nothing on GitHub,
/// and the release writes with the App's token, never this one; a token that may
/// write is one every build script and test a job runs could use.
const THE_ONE_PERMISSION: &str = "contents: read";

/// The actions that restore a cache, which the release never runs.
///
/// **Read by the start of the name**, so the half of the cache action that only
/// restores is one of them.
const RESTORES_A_CACHE: [&str; 2] = ["actions/cache", "Swatinem/rust-cache"];

/// What the release job runs, step by step and in this order, and every line
/// each step holds: an action by its name, then its inputs; a command as it is
/// written, then what it is handed.
///
/// **Every step, not the cargo commands among them.** The job holds the App's
/// token, which may push to `staging` and `main` past their rules and move a
/// `v*` tag, so what may run beside it is this list and nothing else: a script
/// a step runs, a composite action, or a second command on a line is a step
/// that is not here. The stack comes up before the release, whose gate reads
/// it, and the release's command names the channel as the branch pushed to.
///
/// **And every line of each, not the line that says what it runs.** An input
/// is part of what an action does: `repository:` beside the checkout's inputs
/// checks out another repository's code, which the release then builds and runs
/// with the App's token, and a `permission-` input of the token's own asks the
/// App for whatever it was granted. So each step holds exactly these lines,
/// indented as written here under the step's first key, and a key or an input
/// more, or one fewer, is a step that is not this one. A step's `name:` is its
/// label and is not held; the commit an action is pinned to is held by
/// [`every_action_is_a_commit`].
const THE_RELEASE_STEPS: [&[&str]; 6] = [
    &[
        "uses: actions/create-github-app-token",
        "id: app",
        "with:",
        "  app-id: ${{ vars.RELEASE_APP_ID }}",
        "  private-key: ${{ secrets.RELEASE_APP_KEY }}",
        "  permission-contents: write",
        "  permission-pull-requests: write",
    ],
    &[
        "uses: actions/checkout",
        "with:",
        "  fetch-depth: 0",
        "  persist-credentials: false",
    ],
    &["run: rustup toolchain install --no-self-update"],
    &["run: cargo install --locked cargo-deny"],
    &["run: cargo xtask dev"],
    &[
        "run: cargo xtask release --channel ${{ github.ref_name }} --publish",
        "env:",
        "  GH_TOKEN: ${{ steps.app.outputs.token }}",
    ],
];

/// The keys the release workflow holds at its top, each once.
///
/// **And no other.** `env:` there reaches every step, the token's among them,
/// and `defaults:` decides how every `run:` is run; neither is a key any rule
/// here reads.
const THE_RELEASE_WORKFLOWS_KEYS: [&str; 5] = ["name", "on", "permissions", "concurrency", "jobs"];

/// The keys the release job holds, each once.
///
/// **And no other.** `container:` runs every step, the one handed the token
/// among them, inside an image this tree does not build; `services:` starts
/// one beside them; `env:` and `defaults:` reach every step; `strategy:` runs
/// the job more than once; and `if:`, `needs:` or `outputs:` tie it to what
/// another job decides.
const THE_RELEASE_JOBS_KEYS: [&str; 4] = ["runs-on", "environment", "permissions", "steps"];

/// The environment the release job enters: the branch's own, which holds
/// the App's key and which GitHub lets a job enter from that branch alone.
const THE_RELEASES_ENVIRONMENT: &str = "release-${{ github.ref_name }}";

/// The one line that hands the App's token on: to the release step, as the
/// variable `gh` reads.
const THE_TOKEN_HANDED: &str = "GH_TOKEN: ${{ steps.app.outputs.token }}";

/// Where in the release workflow an expression of [`THE_EXPRESSIONS`] stands.
#[derive(Clone, Copy, Debug)]
enum Place {
    /// The lines under the workflow's `concurrency:`.
    ConcurrencyGroup,
    /// The keys of the release job itself.
    Job,
    /// The lines of the step that mints the App's token.
    MintingStep,
    /// The lines of the step that releases.
    ReleaseStep,
}

/// Every line of the release workflow that holds an expression, written out
/// whole, and where it stands.
///
/// **Six, and no other, because GitHub evaluates an expression before anything
/// runs.** The token is in whatever line names it, however it is named:
/// `steps['app'].outputs.token` is `steps.app.outputs.token` spelled with an
/// index, `toJSON(steps)` is every step's output at once, and `toJSON(github)`
/// holds the workflow's own token. A list of names to refuse is a list of
/// spellings; a list of the lines the release needs is closed. The branch names
/// the concurrency group, the environment and the channel; the App's id and its
/// key reach the step that mints the token; and the token reaches the release
/// step, as the variable `gh` reads.
const THE_EXPRESSIONS: [(&str, Place); 6] = [
    ("group: release-${{ github.ref }}", Place::ConcurrencyGroup),
    ("environment: release-${{ github.ref_name }}", Place::Job),
    ("app-id: ${{ vars.RELEASE_APP_ID }}", Place::MintingStep),
    (
        "private-key: ${{ secrets.RELEASE_APP_KEY }}",
        Place::MintingStep,
    ),
    (
        "run: cargo xtask release --channel ${{ github.ref_name }} --publish",
        Place::ReleaseStep,
    ),
    (THE_TOKEN_HANDED, Place::ReleaseStep),
];

/// The action that mints the release App's token, which the release job runs
/// and nothing else does.
///
/// **Not one of [`SETS_UP_AND_JUDGES_NOTHING`]**, which every file CI reaches
/// shares. It runs nothing of this tree's, so it is no cargo command, but what
/// it mints may push to `staging` and `main` past their rules: the release job
/// runs it as the first of [`THE_RELEASE_STEPS`], and anywhere else
/// [`nothing_else_holds_the_releases_authority`] refuses it.
const MINTS_THE_RELEASE_TOKEN: &str = "actions/create-github-app-token";

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
const SETS_UP_AND_JUDGES_NOTHING: [&str; 4] = [
    // Fetches the tree.
    "actions/checkout",
    // The cache, which changes what a build costs and not what it decides. The
    // release restores none (`RESTORES_A_CACHE`).
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
///
/// **And a double-quoted scalar of one line is read with its escapes
/// resolved**, because that is the text a shell is handed: read as written,
/// `run: "\x63argo clippy --workspace"` and `run: "C\x3dcargo; $C clippy
/// --workspace"` each ran a second lints gate in `ci.yml` with nothing said.
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
        return vec![if resolved == inner {
            line.to_owned()
        } else {
            format!("{key}: {resolved}")
        }];
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
                .filter(|line| !line.starts_with('#') && !is_only_a_label(line))
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
                    runs_cargo_as_an_action(line).len()
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
            // itself. **Unless a shell would run more than its first word**
            // ([`is_only_a_label`]), because the same line in a block scalar or
            // a script is a command.
            if is_only_a_label(trimmed) {
                continue;
            }
            // **A standard action is an invocation.** `actions-rs/cargo` <!-- allow-reference: names `actions-rs/cargo` — a GitHub Action, not a path in this tree --> runs
            // cargo from a `with: command:` block and needs no `run:` line at
            // all, so nothing here would ever have seen it.
            for action in runs_cargo_as_an_action(trimmed) {
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

/// Every workflow `.github/` holds, bounded, and how many actions were judged.
///
/// **Apart from [`only_runs_the_gate`], which reads what CI runs**: this reads
/// what CI may hold, the authority each workflow starts from, which code each
/// action is, and how the release is made. The `CI` step reports both.
///
/// **What it holds them to:** `.github/` holds [`THE_WORKFLOWS`] and nothing
/// else; each opens with `permissions: {}` and no job asks for more than
/// [`THE_ONE_PERMISSION`]; neither runs on an event of
/// [`WITH_THIS_REPOSITORYS_AUTHORITY`]; every action any file CI reaches uses
/// is pinned to a full commit, with its release in a comment; the release is
/// made as [`the_release_runs_as_written`] says; no other file CI reaches
/// holds what makes one ([`nothing_else_holds_the_releases_authority`]);
/// every key of a file GitHub runs is written out where a rule reads it
/// ([`every_key_is_written_out`]); and every file CI reaches ends its lines
/// where every reader here ends them ([`every_line_ends_at_a_line_feed`]).
///
/// The count is of the actions [`every_action_is_a_commit`] judged, for the
/// floor the step puts under it: every other rule here requires something that
/// a reader which read nothing cannot find, and that one only refuses.
pub(crate) fn every_workflow_is_bounded(root: &Path) -> (Vec<Finding>, usize) {
    let mut findings = Vec::new();
    every_line_ends_at_a_line_feed(root, &mut findings);
    no_letter_reads_as_a_space(root, &mut findings);
    github_holds_the_workflows(root, &mut findings);
    for file in THE_WORKFLOWS {
        let path = root.join(file);
        // **A missing workflow is said once**: the gate's by `only_runs_the_gate`,
        // which cannot judge anything without it, and the release's by
        // `github_holds_the_workflows` above.
        if !path.is_file() {
            continue;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                no_authority_but_reading(file, &text, &mut findings);
                nothing_made_elsewhere_starts_it(file, &text, &mut findings);
                if file == RELEASE_WORKFLOW {
                    the_release_runs_as_written(&text, &mut findings);
                }
            }
            Err(error) => findings.push(Finding {
                file: file.to_owned(),
                line: 1,
                what: format!("`{file}` could not be read: {error}"),
                fix: "a workflow this gate cannot read is one GitHub still runs, so every \
                      rule here would pass over it unjudged. Make it a readable UTF-8 file"
                    .to_owned(),
            }),
        }
    }
    nothing_else_holds_the_releases_authority(root, &mut findings);
    every_key_is_written_out(root, &mut findings);
    let judged = every_action_is_a_commit(root, &mut findings);
    (findings, judged)
}

/// The characters a YAML parser may end a line at and [`str::lines`] does
/// not, each with the name a finding gives it: a carriage return with no line
/// feed after it, where YAML 1.2 ends one, and NEL, LS and PS, where a YAML
/// 1.1 parser ends one as well.
const THE_OTHER_LINE_BREAKS: [(char, &str); 4] = [
    ('\r', "a carriage return"),
    ('\u{85}', "U+0085 (NEL)"),
    ('\u{2028}', "U+2028 (LS)"),
    ('\u{2029}', "U+2029 (PS)"),
];

/// Every file CI reaches ends each line at a line feed, and at none of
/// [`THE_OTHER_LINE_BREAKS`].
///
/// **Every reader here splits a file at a line feed**, and a carriage return
/// before one goes with it; YAML also ends a line at a carriage return of its
/// own, and a YAML 1.1 parser at NEL, LS and PS. What follows one of those on
/// a comment, or on a step's `name:`, which the release's steps leave out as a
/// label, is YAML no rule here reads. Measured: a step carried after a
/// carriage return in a comment of `release.yml`, and a `workflow_dispatch`
/// trigger after one under `on:`, passed the whole gate with `check passed`.
/// **Every file CI reaches, not the workflows alone**, because the cargo walk
/// splits the justfile and a script a step runs the same way.
fn every_line_ends_at_a_line_feed(root: &Path, findings: &mut Vec<Finding>) {
    // A walk that stopped is said by `every_action_is_a_commit`, which walks
    // the same files.
    let Ok((files, _)) = everything_ci_reaches(root) else {
        return;
    };
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = relative_to(root, &path);
        for (at, line) in text.lines().enumerate() {
            for (_, named) in THE_OTHER_LINE_BREAKS
                .iter()
                .filter(|&&(other, _)| line.contains(other))
            {
                findings.push(Finding {
                    file: relative.clone(),
                    line: at + 1,
                    what: format!("`{relative}` ends a line at {named}, which no reader here does"),
                    fix: "every rule on CI's files reads a line up to its line feed, and a YAML \
                          parser ends one at a carriage return too, and some at NEL, LS and PS, \
                          so what follows one on a comment or a step's name is a key or a step \
                          GitHub reads and no rule here does. End each line with a line feed \
                          alone"
                        .to_owned(),
                });
            }
        }
    }
}

/// Every file CI reaches holds no character the readers here take for a space
/// and YAML and a shell take for a letter ([`a_space_to_the_readers_alone`]).
///
/// **Every reader here trims and splits a line at whitespace as Rust reads
/// it**, and YAML indents and separates with a space and a tab alone, and a
/// shell splits words at those and a line feed. So a line indented with U+3000
/// or a no-break space is deeper to a rule here than to GitHub. Measured: a key
/// `\u{3000}B: "x` under a step's `env:`, below a block scalar, was read as the
/// scalar's text, and the quote it left open swallowed the macOS job's gate
/// step with nothing said; `yq` reads the key, and the step as its string.
fn no_letter_reads_as_a_space(root: &Path, findings: &mut Vec<Finding>) {
    // A walk that stopped is said by `every_action_is_a_commit`, which walks
    // the same files.
    let Ok((files, _)) = everything_ci_reaches(root) else {
        return;
    };
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = relative_to(root, &path);
        for (at, line) in text.lines().enumerate() {
            let mut said: Vec<char> = Vec::new();
            for letter in line
                .chars()
                .filter(|&letter| a_space_to_the_readers_alone(letter))
            {
                if said.contains(&letter) {
                    continue;
                }
                said.push(letter);
                findings.push(Finding {
                    file: relative.clone(),
                    line: at + 1,
                    what: format!(
                        "`{relative}` holds U+{:04X}, a space to every reader here and a letter \
                         to YAML and to a shell",
                        u32::from(letter)
                    ),
                    fix: "every rule on CI's files trims and splits a line at whitespace as Rust \
                          reads it, and YAML indents with a space alone, so a line indented with \
                          another space is deeper to a rule here than to GitHub, and a key or a \
                          step GitHub reads is passed over as a block scalar's text. Write a \
                          space, U+0020, or a tab"
                        .to_owned(),
                });
            }
        }
    }
}

/// Whether Rust reads `letter` as whitespace and YAML and a shell do not.
///
/// Any whitespace but a space, a tab, a line feed and
/// [`THE_OTHER_LINE_BREAKS`], which [`every_line_ends_at_a_line_feed`] names.
fn a_space_to_the_readers_alone(letter: char) -> bool {
    letter.is_whitespace()
        && !matches!(letter, ' ' | '\t' | '\n')
        && !THE_OTHER_LINE_BREAKS
            .iter()
            .any(|&(other, _)| other == letter)
}

/// `.github/` holds every one of [`THE_WORKFLOWS`], and nothing else.
fn github_holds_the_workflows(root: &Path, findings: &mut Vec<Finding>) {
    let mut held = Vec::new();
    if let Err(error) = walk(&root.join(WORKFLOWS), &mut |path| {
        held.push(relative_to(root, path));
    }) {
        findings.push(Finding {
            file: WORKFLOWS.to_owned(),
            line: 1,
            what: format!("`{WORKFLOWS}/` could not be walked: {error}"),
            fix: "what sits there is what GitHub runs, and a walk that stopped cannot say \
                  what that is"
                .to_owned(),
        });
        return;
    }
    for file in &held {
        if !THE_WORKFLOWS.contains(&file.as_str()) {
            findings.push(Finding {
                file: file.clone(),
                line: 1,
                what: format!(
                    "`{file}` sits under `{WORKFLOWS}/`, which holds `{WORKFLOW}` and \
                     `{RELEASE_WORKFLOW}` and nothing else"
                ),
                fix: "a workflow there is one GitHub runs on its own events, with its own \
                      token and its own way into the environments that hold the release \
                      App's key, and a script or an action there is code a step runs. A \
                      file belongs there with a rule that reads it: name it in \
                      `THE_WORKFLOWS` and say what holds it"
                    .to_owned(),
            });
        }
    }
    if !held.iter().any(|file| file == RELEASE_WORKFLOW) {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: 1,
            what: format!("`{RELEASE_WORKFLOW}` is missing"),
            fix: "a push to `staging` or `main` is what releases that channel (NOCTUA.md \
                  §22.6), and the workflow is what makes the release, and the one place the \
                  release App's token is minted"
                .to_owned(),
        });
    }
}

/// A workflow opens with `permissions: {}`, and a job in it asks for
/// [`THE_ONE_PERMISSION`] at most.
///
/// **A workflow with no `permissions:` at its top hands every job the token the
/// repository's settings give**, and so does every build script and test that
/// job runs. Opened with none, a job holds only what it asks for.
///
/// **Every `permissions` key, wherever YAML opens one** ([`the_keys_named`]):
/// quoted, behind an escape, after an anchor or a tag, or in a job written as
/// a flow mapping. Each line is read every way [`as_yaml_reads_them`] gives
/// it: on its own as written and with the escapes that spell letters
/// resolved, and joined to the lines a backslash at its end continues. Read only where a line opens on the
/// bare word, a job of `ci.yml` under `"permissions":` held a token that
/// writes, with nothing said; and read only joined, `name: gate \` hid
/// a `permissions: write-all` on the line after it.
fn no_authority_but_reading(file: &str, text: &str, findings: &mut Vec<Finding>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut opened = false;
    let mut said: Vec<Finding> = Vec::new();
    for (at, read) in as_yaml_reads_them(text) {
        for version in &read {
            let code = the_code_before_a_comment(version);
            for keyed in the_keys_named(&code, "permissions") {
                let value = keyed.value;
                if indent_of(lines[at]) == 0 && keyed.at == 0 {
                    opened = true;
                    if value != "{}" {
                        said.push(Finding {
                            file: file.to_owned(),
                            line: at + 1,
                            what: format!("`{file}` opens with `permissions: {value}`"),
                            fix: "every job without a `permissions:` of its own holds what the \
                                  top of the file grants, and here that is to be nothing: \
                                  write `permissions: {}` there, and let each job ask for \
                                  reading the tree"
                                .to_owned(),
                        });
                    }
                    continue;
                }
                for grant in the_grants(&lines, at, &value) {
                    if grant != THE_ONE_PERMISSION {
                        said.push(Finding {
                            file: file.to_owned(),
                            line: at + 1,
                            what: format!(
                                "a job in `{file}` asks for `{grant}`, and a job here asks for \
                                 `{THE_ONE_PERMISSION}` alone"
                            ),
                            fix: "the gate writes nothing on GitHub, and the release writes \
                                  with the App's token and never with this one; a token that \
                                  may write is one every build script and test the job runs \
                                  could use. Ask for `contents: read`, which the checkout \
                                  needs, and nothing more"
                                .to_owned(),
                        });
                    }
                }
            }
        }
    }
    // Both readings of a line are one line, and say it once.
    for finding in said {
        let already = findings
            .iter()
            .any(|held| held.line == finding.line && held.what == finding.what);
        if !already {
            findings.push(finding);
        }
    }
    if !opened {
        findings.push(Finding {
            file: file.to_owned(),
            line: 1,
            what: format!("`{file}` does not open with `permissions: {{}}`"),
            fix: "a workflow with nothing at its top hands each job the token the \
                  repository's settings give, which may write. Write `permissions: {}` \
                  above `jobs:`, and let each job ask for reading the tree"
                .to_owned(),
        });
    }
}

/// What one `permissions:` key grants: each `name: level` it holds, written
/// one way whatever spacing the file gives it.
///
/// **Its value on its own line, a flow mapping, or the lines under it**, which
/// are the three ways YAML writes one; a word such as `write-all` is a grant
/// of its own. Quotes around a name or a level are not part of what it grants.
fn the_grants(lines: &[&str], at: usize, value: &str) -> Vec<String> {
    let held: Vec<String> = if value.is_empty() {
        the_lines_under(lines, at)
            .into_iter()
            .map(|index| the_code_before_a_comment(lines[index]))
            .collect()
    } else if let Some(inside) = value
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    {
        inside.split(',').map(str::to_owned).collect()
    } else {
        vec![value.to_owned()]
    };
    let unquoted = |text: &str| text.trim().trim_matches(['"', '\'']).trim().to_owned();
    held.iter()
        .map(|grant| match grant.split_once(':') {
            Some((name, level)) => format!("{}: {}", unquoted(name), unquoted(level)),
            None => unquoted(grant),
        })
        .filter(|grant| !grant.is_empty())
        .collect()
}

/// No workflow runs on an event that lends a change made elsewhere this
/// repository's authority ([`WITH_THIS_REPOSITORYS_AUTHORITY`]).
///
/// **Read as a word, every way YAML may read the line** ([`as_yaml_reads_them`])
/// and without regard to case, so a block key, a flow sequence, a quoted name
/// and an escape are each the event. Read as the line is written alone,
/// `"pull_request\x5ftarget":` under `on:` started `ci.yml` on a fork's pull
/// request with nothing said.
fn nothing_made_elsewhere_starts_it(file: &str, text: &str, findings: &mut Vec<Finding>) {
    let mut said: Vec<(usize, &str)> = Vec::new();
    for (at, read) in as_yaml_reads_them(text) {
        let codes = read
            .iter()
            .map(|version| the_code_before_a_comment(version).to_ascii_lowercase());
        for code in codes {
            for event in WITH_THIS_REPOSITORYS_AUTHORITY {
                if code.contains(event) && !said.contains(&(at, event)) {
                    said.push((at, event));
                    findings.push(Finding {
                        file: file.to_owned(),
                        line: at + 1,
                        what: format!("`{file}` runs on `{event}`"),
                        fix: "it runs with this repository's secrets and a token that may write, \
                              on behalf of a change made in a fork, and a build script of that \
                              change holds what the run holds. A pull request is judged on \
                              `pull_request`, which a fork's runs with no secret and a token \
                              that can only read"
                            .to_owned(),
                    });
                }
            }
        }
    }
}

/// Every action a file CI reaches uses is pinned to a full commit, with its
/// release in a comment beside it; and how many were judged.
///
/// **A local action is a file of this commit**, walked and judged as one, so
/// it needs no pin of its own.
///
/// **Every `uses` key of every line** ([`the_actions_used`]), and one this
/// reader cannot read on its line is refused rather than passed over: a value
/// on the lines below its key, a block scalar, an anchor, a tag, an alias or a
/// collection, and a key YAML joins across a backslash and a line break
/// ([`the_lines_a_backslash_joins`]), which no single line holds. The
/// allow-list reads lines as the cargo walk joins them and would not see that
/// key either.
fn every_action_is_a_commit(root: &Path, findings: &mut Vec<Finding>) -> usize {
    let files = match everything_ci_reaches(root) {
        Ok((files, _)) => files,
        Err(error) => {
            findings.push(Finding {
                file: WORKFLOWS.to_owned(),
                line: 1,
                what: format!("what CI reaches could not be walked: {error}"),
                fix: "every action a step uses is judged from the files CI reaches, and \
                      a walk that stopped judges none of them"
                    .to_owned(),
            });
            return 0;
        }
    };
    let mut judged = 0_usize;
    for path in files {
        if !path
            .extension()
            .is_some_and(|kind| kind == "yml" || kind == "yaml")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let relative = relative_to(root, &path);
        let not_read = |at: usize| Finding {
            file: relative.clone(),
            line: at + 1,
            what: format!("`{relative}` uses an action this gate cannot read on its line"),
            fix: "the pins and the actions CI may use are read from the line a `uses` key \
                  stands on, plain or quoted, wherever YAML opens a key. A value on the lines \
                  below it, a block scalar, an anchor, a tag, an alias or a collection, or a \
                  key a backslash carries across a line break, is an action both would pass \
                  over. Write `uses: owner/name@<commit> # <release>` on one line"
                .to_owned(),
        };
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let code = the_command_without_its_comment(line);
            let release = line[code.len()..].trim().trim_start_matches('#').trim();
            for used in the_actions_used(line) {
                let Some(used) = used else {
                    judged += 1;
                    findings.push(not_read(at));
                    continue;
                };
                if used.starts_with('.') {
                    continue;
                }
                judged += 1;
                let reference = used.split_once('@').map_or("", |(_, reference)| reference);
                if !is_a_commit(reference.trim()) {
                    findings.push(Finding {
                        file: relative.clone(),
                        line: at + 1,
                        what: format!("`uses: {used}` is not pinned to a full commit"),
                        fix: "a tag or a branch is a name its owner can move, so the code a \
                              step runs could change under a workflow nobody edited — in the \
                              job that holds the release App's token as in any other — and a \
                              full 40-character commit cannot. Pin it by the commit, with its \
                              release in a comment after it"
                            .to_owned(),
                    });
                } else if release.is_empty() {
                    findings.push(Finding {
                        file: relative.clone(),
                        line: at + 1,
                        what: format!("`uses: {used}` is pinned with no release beside it"),
                        fix: "the commit says what runs and not what it is: the release in a \
                              comment after it, such as `# v1.2.3`, is what a reader checks \
                              against the action's own tags, and what an update moves"
                            .to_owned(),
                    });
                }
            }
        }
        // **And a key no single line holds**: `"us\` on one line and `es":` on
        // the next are one key to YAML and none to a reader of lines.
        for (at, joined) in the_lines_a_backslash_joins(&text) {
            for _ in the_actions_used(&joined) {
                findings.push(not_read(at));
            }
        }
    }
    judged
}

/// Whether a revision is a full commit SHA, as GitHub writes one.
fn is_a_commit(reference: &str) -> bool {
    reference.len() == 40
        && reference
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// No file CI reaches but the release workflow holds what makes a release:
/// the App's token, the environment that holds its key, a secret, a step's
/// output, or `--publish`.
///
/// **Every rule on the release reads `release.yml`**, and `ci.yml` runs on
/// every push, to `staging` and `main` among them. GitHub admits a job to an
/// environment by its branch and not by its workflow, so a job there that
/// entered `release-main`, minted the App's token from the key it holds,
/// restored a cache a pull request saved and ran `cargo xtask release
/// --publish` would make a release past every one of those rules. Each part of
/// that is refused wherever else GitHub reads it: the token's action, an
/// environment, a secret and a step's output in a file GitHub runs
/// ([`github_runs_it`]), the workflows and a composite action's file; and
/// `--publish` in every file CI reaches, the justfile and a script a step runs
/// among them. A justfile or a script holds no expression GitHub evaluates and
/// no key a runner reads, and `docker-compose.yml`'s own `environment:` is the
/// stack's.
///
/// **Read as words, not as keys or steps**, so a flow mapping, an index, a
/// quoted key or a condition is no way past ([`the_releases_authority_in`]),
/// and each line is read as YAML may read it ([`as_yaml_reads_them`]).
fn nothing_else_holds_the_releases_authority(root: &Path, findings: &mut Vec<Finding>) {
    // A walk that stopped is said by `every_action_is_a_commit`, which walks
    // the same files.
    let Ok((files, _)) = everything_ci_reaches(root) else {
        return;
    };
    for path in files {
        let relative = relative_to(root, &path);
        if relative == RELEASE_WORKFLOW {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let github = github_runs_it(&relative);
        for (at, read) in as_yaml_reads_them(&text) {
            for what in the_releases_authority_in(&read, github) {
                findings.push(Finding {
                    file: relative.clone(),
                    line: at + 1,
                    what: format!("`{relative}` {what}, which the release workflow alone does"),
                    fix: "the rules that hold the release read `release.yml` alone, and a \
                          workflow runs on a push to `staging` and `main` like any other: an \
                          environment admits a job by its branch, so a job elsewhere that \
                          entered `release-main`, minted the App's token and published would \
                          make a release past every one of them. The token's action, the \
                          environment, a secret, a step's output, which is how the token is \
                          handed on, and `--publish` are the parts of that, and each is the \
                          release workflow's alone. Make the release there"
                        .to_owned(),
                });
            }
        }
    }
}

/// Every file GitHub runs that CI reaches writes each key out, as a word
/// before its colon on one line ([`the_keys_not_written_out`],
/// [`the_lines_left_open`], [`the_comments_cut_too_soon`]).
///
/// **Every rule here reads a key by the word written before its colon**:
/// `permissions`, `environment`, `uses`, `if`, `shell` and the rest. YAML has
/// three ways to give a mapping a key written somewhere else: `?`, then the
/// key, with its value after a `:` on the line below; an alias, which stands
/// for a word anchored elsewhere; and `<<`, which merges a mapping anchored
/// elsewhere. A job of `ci.yml` holding `? environment` with `: release-main`
/// on the line below entered that environment past the rule that refuses one,
/// and the same three ways pass a grant or an action by every rule that reads
/// those. A flow collection left open past its line is a fourth, because its
/// key's colon may stand on a later line. A quoted scalar left open is a
/// fifth, the other way round: the lines it runs on are its text, and a reader
/// of lines reads them as keys. With `name: "check` at the top of `ci.yml`,
/// `permissions: {}` on the line after and the quote closed on the line after
/// that, the file opened with no `permissions` key at all, and the rule that
/// requires one read the one inside the string. A `#` the readers here take
/// for a comment where YAML reads on is a sixth, which hides what follows it;
/// so a file GitHub runs writes none of them.
fn every_key_is_written_out(root: &Path, findings: &mut Vec<Finding>) {
    // A walk that stopped is said by `every_action_is_a_commit`, which walks
    // the same files.
    let Ok((files, _)) = everything_ci_reaches(root) else {
        return;
    };
    for path in files {
        let relative = relative_to(root, &path);
        if !github_runs_it(&relative) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (at, line) in text.lines().enumerate() {
            for how in the_keys_not_written_out(&the_code_before_a_comment(line)) {
                findings.push(Finding {
                    file: relative.clone(),
                    line: at + 1,
                    what: format!("`{relative}` writes a key {how}"),
                    fix: "every rule on a workflow reads a key by the word written before \
                          its colon on the line it opens, and a key written after `?`, as an \
                          alias or merged in with `<<` is one none of them reads: the \
                          environment, the permissions, the actions and the conditions would \
                          all pass it unjudged. Write the key out where it stands, as \
                          `name: value`"
                        .to_owned(),
                });
            }
        }
        for (at, what) in the_lines_left_open(&text) {
            findings.push(Finding {
                file: relative.clone(),
                line: at + 1,
                what: format!("`{relative}` leaves {what} open past its line"),
                fix: "inside `{` or `[` YAML lets an entry, its colon among it, run on to a \
                      later line, and inside a quote it reads the lines that follow as text; \
                      every rule on a workflow reads a key with its colon on the line the key \
                      stands on, so `{ permissions` with `: write-all }` below it is a grant \
                      none of them reads, and a key inside an open quote is one they read and \
                      GitHub does not. Close each `{`, `[` and quote on the line that opens it, \
                      or write the mapping as a block"
                    .to_owned(),
            });
        }
        for at in the_comments_cut_too_soon(&text) {
            findings.push(Finding {
                file: relative.clone(),
                line: at + 1,
                what: format!(
                    "`{relative}` holds a `#` every reader here ends the line at, and YAML reads \
                     on past it"
                ),
                fix: "YAML opens a quoted scalar only where a node starts and reads a `#` inside \
                      one as text, and the readers here end a line at a `#` outside any quote, \
                      the way a shell does: in `{ ke\"y: \"a # b\", permissions: write-all }` \
                      YAML reads a grant none of them reads. Write no quote inside a key or a \
                      plain value of that line, or no `#` inside its quoted value"
                    .to_owned(),
            });
        }
    }
}

/// Whether GitHub reads `relative` as YAML it runs: a file under `.github/`,
/// or the file a composite action is written in, named action.yml or
/// action.yaml.
///
/// **Not every YAML file CI reaches**: `docker-compose.yml` is reached, and its
/// `environment:` and `secrets:` are keys of the stack's, which no runner
/// reads.
fn github_runs_it(relative: &str) -> bool {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    let yaml = Path::new(name)
        .extension()
        .is_some_and(|kind| kind.eq_ignore_ascii_case("yml") || kind.eq_ignore_ascii_case("yaml"));
    yaml && (relative.starts_with(".github/")
        || name.eq_ignore_ascii_case("action.yml")
        || name.eq_ignore_ascii_case("action.yaml"))
}

/// What of the release's authority one line holds, by what it does, read both
/// ways [`as_yaml_reads_them`] gives it.
///
/// **In a file GitHub runs** ([`github_runs_it`]): the token's action wherever
/// its name is written; a key `environment`, quoted or not, in a block or a
/// flow mapping, or after `?`; the word `secrets` anywhere in the code, which takes in
/// `secrets.KEY`, `secrets['KEY']`, `toJSON(secrets)`, a condition and
/// `secrets: inherit`; and `steps` in any expression, which takes in
/// `steps.app.outputs.token`, `steps['app']` and `toJSON(steps)` and leaves the
/// key `steps:` alone. An expression is read in a comment too, because inside a
/// block scalar a `#` line is text GitHub evaluates. **In any file**,
/// `--publish`, with the quotes and backslashes a shell spends taken out
/// first.
fn the_releases_authority_in(read: &[String; 2], github: bool) -> Vec<String> {
    let mut held = Vec::new();
    for version in read {
        let code = the_code_before_a_comment(version).to_ascii_lowercase();
        let lowered = version.to_ascii_lowercase();
        let expressions = the_expressions_in(&lowered);
        let in_an_expression = |word: &str| {
            expressions
                .iter()
                .any(|expression| !the_ends_of_the_word(expression, word).is_empty())
        };
        // A key is followed by its colon, or follows `?` with its colon on the
        // line after.
        let as_a_key = |end: &usize| {
            code[*end..]
                .trim_start_matches(['"', '\''])
                .trim_start()
                .starts_with(':')
                || follows_an_explicit_key(&code, *end - "environment".len())
        };
        if github && code.contains(&MINTS_THE_RELEASE_TOKEN.to_ascii_lowercase()) {
            held.push("mints the release App's token".to_owned());
        }
        if github
            && the_ends_of_the_word(&code, "environment")
                .iter()
                .any(as_a_key)
        {
            held.push("enters an environment".to_owned());
        }
        if github
            && (!the_ends_of_the_word(&code, "secrets").is_empty() || in_an_expression("secrets"))
        {
            held.push("reads a secret".to_owned());
        }
        if github && in_an_expression("steps") {
            held.push("reads a step's output".to_owned());
        }
        if code.replace(['"', '\'', '\\'], "").contains("--publish") {
            held.push("publishes a release".to_owned());
        }
    }
    held.sort_unstable();
    held.dedup();
    held
}

/// The text of every expression `text` opens: what follows each `${{`, up to
/// the `}}` that closes it or the end of the text.
fn the_expressions_in(text: &str) -> Vec<&str> {
    text.split("${{")
        .skip(1)
        .map(|after| after.split_once("}}").map_or(after, |(inside, _)| inside))
        .collect()
}

/// Where `word` ends, each time it stands in `text` as a word of its own.
///
/// **Not as a part of a longer name**: `secrets` is in `toJSON(secrets)` and
/// `secrets['KEY']`, and not in `my-secrets-dir`. A name is what a context, a
/// key or a step's id is written with: letters, digits, `_` and `-`.
fn the_ends_of_the_word(text: &str, word: &str) -> Vec<usize> {
    let is_a_letter_of_a_name =
        |letter: char| letter.is_ascii_alphanumeric() || matches!(letter, '_' | '-');
    text.match_indices(word)
        .filter(|&(at, _)| {
            !text[..at]
                .chars()
                .next_back()
                .is_some_and(is_a_letter_of_a_name)
                && !text[at + word.len()..]
                    .chars()
                    .next()
                    .is_some_and(is_a_letter_of_a_name)
        })
        .map(|(at, _)| at + word.len())
        .collect()
}

/// Whether a key of one line of YAML may open at `at`: at the start of the
/// line, after a sequence's `-` or an explicit key's `?` with a space after
/// it, or inside a flow collection after `{`, `[` or `,`; with an anchor or a
/// tag of its own before it or not ([`the_property_ending`]): `&a`, `!!str`,
/// or a verbatim tag such as `!<tag:yaml.org,2002:str>`.
///
/// **Where YAML opens one, and nowhere else**, so `echo uses: x` in a
/// command is a word and not a key, and `- { name: lint, uses: x }` holds two.
fn a_key_may_open_at(code: &str, at: usize) -> bool {
    let mut before = &code[..at];
    loop {
        let trimmed = before.trim_end();
        if trimmed.is_empty() || trimmed.ends_with(['{', '[', ',']) {
            return true;
        }
        let spaced = trimmed.len() < before.len();
        // `- &a uses: x`, `- !!str uses: x` and `- !<tag:yaml.org,2002:str>
        // uses: x` are each the key `uses`.
        if spaced && let Some(property) = the_property_ending(trimmed) {
            before = &trimmed[..trimmed.len() - property.len()];
            continue;
        }
        match trimmed.strip_suffix(['-', '?']) {
            Some(rest)
                if spaced
                    && (rest.is_empty()
                        || rest.ends_with(char::is_whitespace)
                        || rest.ends_with(['{', '[', ','])) =>
            {
                before = rest;
            }
            _ => return false,
        }
    }
}

/// The anchor or the tag `text` ends with, if it ends with one.
///
/// **A verbatim tag runs from its `!<` to its `>`**, and what it holds may
/// be a `,`, a `[` or a `]`, which split no tag. Read as the last word after a
/// space, a `{`, a `[` or a `,`, `!<tag:yaml.org,2002:str>`, the tag `!!str`
/// stands for, left `2002:str>`, which is no property, and a `permissions` or
/// a `uses` key after it was read by no rule. An anchor's name and a tag's
/// other spellings hold none of those four.
fn the_property_ending(text: &str) -> Option<&str> {
    let verbatim = text
        .strip_suffix('>')
        .and_then(|inside| inside.rfind("!<"))
        .filter(|&opens| {
            !text[opens + 2..text.len() - 1].contains(|letter: char| letter.is_whitespace())
                && text[..opens].chars().next_back().is_none_or(|letter| {
                    letter.is_whitespace() || matches!(letter, '{' | '[' | ',')
                })
        });
    if let Some(opens) = verbatim {
        return Some(&text[opens..]);
    }
    let word = text
        .rsplit(|letter: char| letter.is_whitespace() || matches!(letter, '{' | '[' | ','))
        .next()
        .unwrap_or(text);
    word.starts_with(['&', '!']).then_some(word)
}

/// One place a key stands on a line of YAML, as [`the_keys_named`] finds it.
struct Keyed {
    /// Where it opens on the line, its quote among it.
    at: usize,
    /// What is written after its colon on the line, to the end of its entry.
    value: String,
}

/// Each place `key` stands as a key on one line of YAML, plain or quoted,
/// read without regard to case, wherever [`a_key_may_open_at`] says one may.
///
/// **Not only where a line opens.** A flow mapping holds a key after another,
/// as in `{ name: lint, uses: … }`; a quoted key is a key; and a flow sequence
/// holds mappings of its own. Read only where a line opens, a third-party
/// action and a job's grant written any of those ways passed every rule that
/// reads its key, with nothing said.
fn the_keys_named(code: &str, key: &str) -> Vec<Keyed> {
    let lowered = code.to_ascii_lowercase();
    let mut found = Vec::new();
    for end in the_ends_of_the_word(&lowered, key) {
        let start = end - key.len();
        let quote = code[..start]
            .chars()
            .next_back()
            .filter(|letter| matches!(letter, '"' | '\''));
        let at = start - quote.map_or(0, char::len_utf8);
        if !a_key_may_open_at(code, at) {
            continue;
        }
        let mut rest = &code[end..];
        if let Some(quote) = quote {
            match rest.strip_prefix(quote) {
                Some(after) => rest = after,
                None => continue,
            }
        }
        let Some(after) = rest.trim_start().strip_prefix(':') else {
            continue;
        };
        // A plain key's colon has a space, the end of the line or the end of
        // a flow entry after it: `uses:x` is one word.
        let a_colon_that_ends_a_key = after.is_empty()
            || after.starts_with(char::is_whitespace)
            || after.starts_with([',', '}', ']']);
        if quote.is_none() && !a_colon_that_ends_a_key {
            continue;
        }
        let in_flow = what_a_line_leaves_open(&code[..at]).0 > 0;
        found.push(Keyed {
            at,
            value: the_entry(after, in_flow),
        });
    }
    found
}

/// What one line of YAML leaves open at its end: how many flow collections,
/// and the quote of a quoted scalar still open, if one is
/// ([`the_line_read_as_yaml`]).
fn what_a_line_leaves_open(line: &str) -> (usize, Option<char>) {
    let (depth, quote, _) = the_line_read_as_yaml(line);
    (depth, quote)
}

/// One line of YAML as YAML reads it: how many flow collections it leaves
/// open, the quote of a quoted scalar still open at its end, if one is, and
/// where its code ends, at the `#` that opens a comment or at the line's end.
///
/// **A quote opens a scalar only where a node may start**
/// ([`a_node_may_open_after`]); anywhere else it is a letter of a plain
/// scalar, as in `echo "a"` or `it's`. Inside a quoted scalar a bracket is
/// text, and so is a `#`: read without the quotes, `name: "}"` inside a flow
/// mapping closed it, and the key the mapping held on the next line was read
/// by no rule. A double quote is escaped by a backslash, and a single quote by
/// another. Outside one, `#` opens a comment where a space or the line's start
/// is before it.
fn the_line_read_as_yaml(line: &str) -> (usize, Option<char>, usize) {
    let mut depth = 0_usize;
    let mut quote: Option<char> = None;
    let mut letters = line.char_indices().peekable();
    while let Some((at, letter)) = letters.next() {
        match (quote, letter) {
            (Some('"'), '\\') => {
                letters.next();
            }
            (Some('\''), '\'') if letters.peek().is_some_and(|&(_, next)| next == '\'') => {
                letters.next();
            }
            (Some(open), _) if letter == open => quote = None,
            (None, '#')
                if line[..at]
                    .chars()
                    .next_back()
                    .is_none_or(char::is_whitespace) =>
            {
                return (depth, quote, at);
            }
            (None, '"' | '\'') if a_node_may_open_after(&line[..at], depth) => {
                quote = Some(letter);
            }
            (None, '{' | '[') => depth += 1,
            (None, '}' | ']') => depth = depth.saturating_sub(1),
            // A letter of a quoted scalar, or of a plain one.
            _ => {}
        }
    }
    (depth, quote, line.len())
}

/// Whether a node of one line of YAML may open after `before`, with `depth`
/// flow collections open.
///
/// Where a key may ([`a_key_may_open_at`]), or as a value: after a `:` with a
/// space after it, or with none in a flow collection; with an anchor or a tag
/// of its own before it or not.
fn a_node_may_open_after(before: &str, depth: usize) -> bool {
    let mut before = before;
    loop {
        let trimmed = before.trim_end();
        match the_property_ending(trimmed) {
            Some(property) if trimmed.len() < before.len() => {
                before = &trimmed[..trimmed.len() - property.len()];
            }
            _ => break,
        }
    }
    let trimmed = before.trim_end();
    a_key_may_open_at(before, before.len())
        || (trimmed.ends_with(':') && (depth > 0 || trimmed.len() < before.len()))
}

/// The value written at the start of `text`, to the end of its entry.
///
/// A flow collection runs to the bracket that closes it, and a quoted scalar
/// to its quote. Anything else runs, inside a flow collection, to the comma
/// or bracket that ends the entry, and outside one to the end of the line.
fn the_entry(text: &str, in_flow: bool) -> String {
    let text = text.trim();
    let mut depth = 0_usize;
    let end = match text.chars().next() {
        Some('{' | '[') => text.char_indices().find_map(|(at, letter)| {
            match letter {
                '{' | '[' => depth += 1,
                '}' | ']' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some(at + 1);
                    }
                }
                _ => {}
            }
            None
        }),
        Some(quote @ ('"' | '\'')) => text[1..].find(quote).map(|at| at + 2),
        _ if in_flow => text.find([',', '}', ']']),
        _ => None,
    };
    end.map_or(text, |end| &text[..end]).trim().to_owned()
}

/// Whether the word that opens at `at` in one line of YAML is a key written
/// after `?`, YAML's explicit form, whose value stands on the line after it.
fn follows_an_explicit_key(code: &str, at: usize) -> bool {
    let before = code[..at].trim_end_matches(['"', '\'']);
    let trimmed = before.trim_end();
    trimmed.len() < before.len()
        && trimmed
            .strip_suffix('?')
            .is_some_and(|rest| a_key_may_open_at(rest, rest.len()))
}

/// Each way one line of YAML writes a key that is not written out as a word
/// before its colon: after `?`, as an alias, or as a merge.
fn the_keys_not_written_out(code: &str) -> Vec<String> {
    let mut ways = Vec::new();
    let where_a_key_may_open = code
        .char_indices()
        .filter(|&(at, _)| a_key_may_open_at(code, at));
    for (at, letter) in where_a_key_may_open {
        let rest = &code[at + letter.len_utf8()..];
        match letter {
            '?' if rest.is_empty() || rest.starts_with(char::is_whitespace) => {
                ways.push("after `?`, YAML's explicit form".to_owned());
            }
            '*' => {
                let name = rest
                    .find(|letter: char| letter.is_whitespace() || ",[]{}".contains(letter))
                    .map_or(rest, |end| &rest[..end]);
                if !name.is_empty()
                    && (name.ends_with(':') || rest[name.len()..].trim_start().starts_with(':'))
                {
                    ways.push("as an alias of a word anchored elsewhere".to_owned());
                }
            }
            '<' if rest
                .strip_prefix('<')
                .is_some_and(|after| after.trim_start().starts_with(':')) =>
            {
                ways.push("as `<<`, which merges a mapping anchored elsewhere".to_owned());
            }
            _ => {}
        }
    }
    ways
}

/// The lines of `text` that leave a flow collection or a quoted scalar open
/// past their end, a block scalar's own lines left out: each line's index, and
/// what it leaves open ([`what_a_line_leaves_open`]).
///
/// **Inside a flow collection YAML lets an entry run over lines**, so a key's
/// colon may stand on a line after the key itself: `{ permissions` with
/// `: write-all }` on the line below is a grant no reader of lines sees.
/// **And a quoted scalar runs over lines too**, its line breaks folded, with
/// or without a backslash before them, so the lines it runs on are text a
/// reader of lines reads as keys. A block scalar's lines are its text, a
/// command's braces and quotes among them ([`the_lines_of_yaml`]).
fn the_lines_left_open(text: &str) -> Vec<(usize, String)> {
    let mut open = Vec::new();
    for (at, line) in the_lines_of_yaml(text) {
        let (depth, quote) = what_a_line_leaves_open(line);
        if depth > 0 {
            open.push((at, "a flow collection".to_owned()));
        }
        if quote.is_some() {
            open.push((at, "a quoted scalar".to_owned()));
        }
    }
    open
}

/// The lines of `text` whose code the readers here end at a `#` that YAML
/// reads on past ([`the_line_read_as_yaml`]).
///
/// **The readers here end a line's code at a `#` as a shell does**, with any
/// quote opening a quoted span, and YAML opens a quoted scalar only where a
/// node starts. So in `{ ke"y: "a # b", permissions: write-all }` the readers
/// take the `#` for a comment, after a span that opened at the key's `"`, and
/// YAML reads a key `ke"y`, the string `a # b` and a grant: measured, that
/// grant, an action, a second cargo command and an environment each passed
/// every rule. A line YAML ends sooner than the readers do is read for more
/// than it holds, which refuses more and never less.
fn the_comments_cut_too_soon(text: &str) -> Vec<usize> {
    the_lines_of_yaml(text)
        .into_iter()
        .filter(|&(_, line)| {
            let (_, _, code) = the_line_read_as_yaml(line);
            the_command_without_its_comment(line).len() < code
        })
        .map(|(at, _)| at)
        .collect()
}

/// The lines of `text` YAML reads as YAML, each at its index: every line but
/// a comment, a blank and a block scalar's own.
///
/// **A block scalar's lines are its text**, a command's braces, quotes and
/// `#` among them, and run to the first line indented no further than the key
/// whose value it is, its column measured past every `-` before it and the
/// whitespace after each ([`past_its_dashes`]). Measured past `- ` alone,
/// `-   run: |` put that key two columns left of where YAML reads it, so the
/// step's own keys below it were read as the scalar's text: a quoted scalar
/// left open on one of them swallowed the macOS job's gate step, and no rule
/// said a word. A column read too far right reads more lines as YAML, which
/// refuses more and never less.
fn the_lines_of_yaml(text: &str) -> Vec<(usize, &str)> {
    let mut held = Vec::new();
    // The column of the key that opened a block scalar, while one is open.
    let mut block: Option<usize> = None;
    for (at, line) in text.lines().enumerate() {
        let code = the_code_before_a_comment(line);
        if code.is_empty() {
            continue;
        }
        let indent = indent_of(line);
        if block.is_some_and(|opened| indent > opened) {
            continue;
        }
        block = None;
        held.push((at, line));
        let entry = past_its_dashes(&code);
        let header = entry.split_once(':').map_or(entry, |(_, rest)| rest);
        if the_block_style(header).is_some() {
            block = Some(indent + code.len() - entry.len());
        }
    }
    held
}

/// Each line of `text` as YAML may read it, at the index it starts on, each
/// as it is written and with the escapes of a double-quoted scalar that spell
/// letters resolved: every line on its own, and every line that ends in a
/// backslash once more, joined to the lines it continues
/// ([`the_lines_a_backslash_joins`]).
///
/// **On its own as well as joined, because what a backslash at the end of a
/// line does depends on where it stands.** In a double-quoted scalar it
/// escapes the line break, so `"secr\` and `ets"` on the line after are one
/// word to GitHub and two halves to a reader of lines; in a shell script it
/// continues the command. At the end of a comment or of a plain scalar it is a
/// letter, and the line after is a line of its own: read only joined,
/// `runs-on: macos-latest # \` hid an `environment: release-main` below it,
/// and `name: gate \` a `permissions: write-all`, from every rule. And
/// `\x73ecrets` is `secrets` once resolved ([`with_the_escapes_resolved`]). A
/// rule that refuses a word reads every reading, so no spelling is a way past
/// it; and a quoted scalar a file GitHub runs leaves open past its line is
/// refused ([`the_lines_left_open`]), so no line read on its own is one GitHub
/// reads as text. An escape is resolved where it spells a letter
/// ([`with_the_letters_its_escapes_spell`]).
fn as_yaml_reads_them(text: &str) -> Vec<(usize, [String; 2])> {
    let both = |line: &str| [line.to_owned(), with_the_letters_its_escapes_spell(line)];
    let mut read: Vec<(usize, [String; 2])> = text
        .lines()
        .enumerate()
        .map(|(at, line)| (at, both(line)))
        .collect();
    for (at, joined) in the_lines_a_backslash_joins(text) {
        read.push((at, both(&joined)));
    }
    read.sort_by_key(|&(at, _)| at);
    read
}

/// A line with each escape a double-quoted scalar may hold resolved, but for
/// one that spells a quote, a backslash, a `#` or a space, which is left as it
/// is written.
///
/// **What spells a key or a word is resolved, and what decides where a
/// line's code ends is not**, because every rule that reads this reading ends
/// it at a `#` again, by a shell's quoting. Resolved whole,
/// `{ ke"y: "a\x20\x23 b", "perm\x69ssions": write-all }` ended at a `#` its
/// escapes spelled, before the grant, and its line as written spelled no key
/// named `permissions`, where YAML reads `a # b` and then the grant.
fn with_the_letters_its_escapes_spell(line: &str) -> String {
    let letters: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < letters.len() {
        if letters[index] != '\\' {
            out.push(letters[index]);
            index += 1;
            continue;
        }
        let width = match letters.get(index + 1) {
            Some('x') => 4,
            Some('u') => 6,
            Some('U') => 10,
            Some(_) => 2,
            None => 1,
        };
        let written: String = letters[index..letters.len().min(index + width)]
            .iter()
            .collect();
        let resolved = with_the_escapes_resolved(&written);
        let ends_code = resolved.contains(|letter: char| {
            letter.is_whitespace() || matches!(letter, '"' | '\'' | '\\' | '#')
        });
        out.push_str(if ends_code { &written } else { &resolved });
        index += width;
    }
    out
}

/// Each line of `text` that ends in a backslash, at its index, joined to the
/// lines the backslashes continue, their indentation dropped: what a
/// double-quoted scalar does with an escaped line break, and a shell with a
/// line it continues.
///
/// **From every such line, not only the first of a run**, because the first
/// may be a comment, which continues nothing, and the next one the start of
/// what the backslashes join.
fn the_lines_a_backslash_joins(text: &str) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut joined_up = Vec::new();
    for (starts, line) in lines.iter().enumerate() {
        if !line.ends_with('\\') {
            continue;
        }
        let mut joined = (*line).to_owned();
        let mut at = starts;
        while joined.ends_with('\\') && at + 1 < lines.len() {
            joined.pop();
            at += 1;
            joined.push_str(lines[at].trim_start());
        }
        joined_up.push((starts, joined));
    }
    joined_up
}

/// The release workflow makes the release as [`THE_RELEASE_STEPS`] say, from
/// a channel's branch, and nowhere else.
///
/// It holds [`THE_RELEASE_WORKFLOWS_KEYS`] at its top and no other key; it
/// fires on a push to the branches of [`RELEASED_FROM`] and on nothing else; it
/// holds one job, which holds [`THE_RELEASE_JOBS_KEYS`] and no other, runs on
/// Linux and enters [`THE_RELEASES_ENVIRONMENT`]; it restores no cache; no step
/// of it runs on a condition; the job runs its steps, each holding its own
/// lines, and no others; and it evaluates [`THE_EXPRESSIONS`], each where that
/// list puts it, and no other, so the App's key and token reach the steps that
/// need them alone.
fn the_release_runs_as_written(text: &str, findings: &mut Vec<Finding>) {
    let lines: Vec<&str> = text.lines().collect();
    let tops: Vec<usize> = (0..lines.len())
        .filter(|&at| indent_of(lines[at]) == 0 && !the_code_before_a_comment(lines[at]).is_empty())
        .collect();
    holds_these_keys_alone(
        &lines,
        &tops,
        &THE_RELEASE_WORKFLOWS_KEYS,
        "the release workflow",
        findings,
    );
    the_release_fires_on_a_channel(&lines, findings);
    the_release_is_one_job(&lines, findings);
    no_cache_reaches_the_release(&lines, findings);
    no_step_runs_on_a_condition(RELEASE_WORKFLOW, text, findings);
    let steps = the_steps_of(&lines, "release");
    the_release_runs_its_steps(&steps, findings);
    every_expression_is_where_the_release_needs_it(&lines, text, &steps, findings);
}

/// The keys at `keys` are among `allowed`, each written once.
///
/// **Read as written, quotes and all**: `"container":` is a key a parser reads
/// as `container`, and here it is one more key that is not on the list.
fn holds_these_keys_alone(
    lines: &[&str],
    keys: &[usize],
    allowed: &[&str],
    whose: &str,
    findings: &mut Vec<Finding>,
) {
    let mut seen: Vec<String> = Vec::new();
    for &at in keys {
        let (key, _) = key_and_value(lines[at]);
        if !allowed.contains(&key.as_str()) || seen.contains(&key) {
            findings.push(Finding {
                file: RELEASE_WORKFLOW.to_owned(),
                line: at + 1,
                what: format!("{whose} holds `{key}:`, and it holds {allowed:?} alone, each once"),
                fix: "every key there is one a rule here reads, and the others change what \
                      runs beside the App's token: `env:` and `defaults:` reach every step, \
                      `container:` runs each inside an image this tree does not build, \
                      `services:` starts one beside them, and `strategy:` runs the job more \
                      than once. A key written twice is one YAML reads one way and a reader \
                      another. A key to add is a line in `THE_RELEASE_WORKFLOWS_KEYS` or \
                      `THE_RELEASE_JOBS_KEYS`, with the rule that reads it"
                    .to_owned(),
            });
        }
        seen.push(key);
    }
}

/// The release fires on a push to a channel's branch, and on nothing else.
///
/// **Read as the three keys it is written with**, `on:`, then `push:`, then
/// `branches:` with the channels, so a shape this cannot read is a finding and
/// not a pass: another event, a filter beside the branches, a tag, or a branch
/// list that is not [`RELEASED_FROM`] is each said by name.
fn the_release_fires_on_a_channel(lines: &[&str], findings: &mut Vec<Finding>) {
    /// What every finding about the trigger says to do: one rule, whichever
    /// part of the trigger broke it.
    const FIX: &str = "a release is made by a push to a channel's branch and by nothing \
                       else (NOCTUA.md §22.6): another event, another branch, a tag or a path \
                       filter releases from something that is not a channel, or does not \
                       release where one is. Write `on:`, then `push:` under it, then \
                       `branches: [staging, main]` under that";
    let Some(on) = top_level(lines, "on").filter(|&at| key_and_value(lines[at]).1.is_empty())
    else {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: 1,
            what: "the release workflow does not say, as a block of its own, what it fires on"
                .to_owned(),
            fix: FIX.to_owned(),
        });
        return;
    };
    let events = the_keys_under(lines, on);
    for &at in &events {
        let (event, value) = key_and_value(lines[at]);
        if event != "push" || !value.is_empty() {
            findings.push(Finding {
                file: RELEASE_WORKFLOW.to_owned(),
                line: at + 1,
                what: format!(
                    "the release fires on `{}`",
                    the_code_before_a_comment(lines[at])
                ),
                fix: FIX.to_owned(),
            });
        }
    }
    let Some(&push) = events
        .iter()
        .find(|&&at| key_and_value(lines[at]).0 == "push")
    else {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: on + 1,
            what: "the release does not fire on a push".to_owned(),
            fix: FIX.to_owned(),
        });
        return;
    };
    let filters = the_keys_under(lines, push);
    for &at in &filters {
        let (filter, _) = key_and_value(lines[at]);
        if filter != "branches" {
            findings.push(Finding {
                file: RELEASE_WORKFLOW.to_owned(),
                line: at + 1,
                what: format!("the release's push is filtered by `{filter}`"),
                fix: FIX.to_owned(),
            });
        }
    }
    let branches: Vec<String> = filters
        .iter()
        .find(|&&at| key_and_value(lines[at]).0 == "branches")
        .map(|&at| the_branches(lines, at))
        .unwrap_or_default();
    let mut named: Vec<&str> = branches.iter().map(String::as_str).collect();
    named.sort_unstable();
    let mut channels = RELEASED_FROM;
    channels.sort_unstable();
    if named != channels {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: push + 1,
            what: format!(
                "the release fires on a push to {named:?}, and a release is made from \
                 {RELEASED_FROM:?} alone"
            ),
            fix: FIX.to_owned(),
        });
    }
}

/// The branches a `branches:` key names: a flow list on its own line, or the
/// items under it, each past its `-` and the whitespace after it
/// ([`past_its_dashes`]). A `-` with none after it is a letter of the branch:
/// read with every leading `-` taken off, `--staging` and `--main` under the
/// key, which YAML reads as one plain scalar and GitHub as a branch of that
/// name, were the two channels.
fn the_branches(lines: &[&str], at: usize) -> Vec<String> {
    let (_, value) = key_and_value(lines[at]);
    let named: Vec<String> = match value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        Some(inside) => inside.split(',').map(str::to_owned).collect(),
        None if value.is_empty() => the_lines_under(lines, at)
            .into_iter()
            .map(|index| past_its_dashes(&the_code_before_a_comment(lines[index])).to_owned())
            .collect(),
        None => vec![value],
    };
    named
        .iter()
        .map(|branch| branch.trim().trim_matches(['"', '\'']).to_owned())
        .filter(|branch| !branch.is_empty())
        .collect()
}

/// The release workflow holds the `release` job and no other, on Linux, in
/// the environment of the branch it releases.
///
/// **And no key but [`THE_RELEASE_JOBS_KEYS`]**, each once.
fn the_release_is_one_job(lines: &[&str], findings: &mut Vec<Finding>) {
    let jobs: Vec<(usize, String)> = top_level(lines, "jobs")
        .map(|at| the_keys_under(lines, at))
        .unwrap_or_default()
        .into_iter()
        .map(|at| (at, key_and_value(lines[at]).0))
        .collect();
    for (at, job) in &jobs {
        if job != "release" {
            findings.push(Finding {
                file: RELEASE_WORKFLOW.to_owned(),
                line: at + 1,
                what: format!("the release workflow holds the job `{job}` beside `release`"),
                fix: "every job in it runs on the push that releases a channel, and the rules \
                      here read the `release` job alone; a second is a job nothing holds to \
                      the steps, the environment or the token. What it would do belongs in \
                      the gate"
                    .to_owned(),
            });
        }
    }
    let Some(&(job, _)) = jobs.iter().find(|(_, job)| job == "release") else {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: 1,
            what: "the release workflow has no `release` job".to_owned(),
            fix: "the job is what makes the release, and every rule about its steps, its \
                  environment and the App's token reads it by that name"
                .to_owned(),
        });
        return;
    };
    let keys = the_keys_under(lines, job);
    holds_these_keys_alone(
        lines,
        &keys,
        &THE_RELEASE_JOBS_KEYS,
        "the release job",
        findings,
    );
    let held: Vec<(String, String)> = keys
        .into_iter()
        .map(|at| key_and_value(lines[at]))
        .collect();
    let named = |key: &str| {
        held.iter()
            .find(|(held, _)| held == key)
            .map(|(_, value)| value.trim_matches(['"', '\'']).to_owned())
    };
    let entered = named("environment");
    if entered.as_deref() != Some(THE_RELEASES_ENVIRONMENT) {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: job + 1,
            what: format!(
                "the release job enters {}, and it enters `{THE_RELEASES_ENVIRONMENT}` \
                 alone",
                entered.map_or_else(|| "no environment".to_owned(), |name| format!("`{name}`"))
            ),
            fix: "the environment holds the App's id and its key, and GitHub lets a job \
                  enter `release-staging` from `staging` alone and `release-main` from `main` \
                  alone; a job that enters another mints the token from a branch that is not \
                  the channel it releases, and one that enters none cannot mint it"
                .to_owned(),
        });
    }
    let runner = named("runs-on").unwrap_or_default();
    if !runner.starts_with("ubuntu-") {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: job + 1,
            what: format!("the release job runs on `{runner}`, which is not a Linux runner"),
            fix: "the release runs the gate first, and only Linux's runner brings up the \
                  stack the gate's steps read (NOCTUA.md §22.2); elsewhere those steps are \
                  not applicable, and the release is judged by less than the gate every \
                  pull request passed. Run it on `ubuntu-latest`"
                .to_owned(),
        });
    }
}

/// The release restores no cache.
fn no_cache_reaches_the_release(lines: &[&str], findings: &mut Vec<Finding>) {
    for (at, line) in lines.iter().enumerate() {
        for used in the_actions_used(line).into_iter().flatten() {
            let name = used.split('@').next().unwrap_or(&used).trim();
            let lowered = name.to_ascii_lowercase();
            let restores = RESTORES_A_CACHE.iter().any(|cache| {
                let cache = cache.to_ascii_lowercase();
                lowered == cache || lowered.starts_with(&format!("{cache}/"))
            });
            if restores {
                findings.push(Finding {
                    file: RELEASE_WORKFLOW.to_owned(),
                    line: at + 1,
                    what: format!("the release restores a cache, through `{name}`"),
                    fix: "a cache holds what an earlier run built and saved — `dev`'s are \
                          restored on every branch, and the run that saved one decided what \
                          it holds — and the release is built from the branch it releases \
                          and nothing else. The release job builds from cold"
                        .to_owned(),
                });
            }
        }
    }
}

/// One step of a job: the line it opens on, what it runs, and every line of it.
struct Step {
    /// The index of the line its `- ` opens.
    opens: usize,
    /// What it runs, written as [`THE_RELEASE_STEPS`] writes a step: an
    /// action by its name, a command as it is written, or nothing.
    runs: String,
    /// The indices of every line it holds, its first among them.
    lines: Vec<usize>,
    /// What it holds, as [`THE_RELEASE_STEPS`] writes it, with the index each
    /// line was read from ([`as_the_steps_are_written`]).
    held: Vec<(usize, String)>,
}

/// The line the job `job` opens on.
fn the_job(lines: &[&str], job: &str) -> Option<usize> {
    top_level(lines, "jobs").and_then(|jobs| {
        the_keys_under(lines, jobs)
            .into_iter()
            .find(|&at| key_and_value(lines[at]).0 == job)
    })
}

/// The steps of the job `job`, in order.
fn the_steps_of(lines: &[&str], job: &str) -> Vec<Step> {
    let Some(job) = the_job(lines, job) else {
        return Vec::new();
    };
    let Some(steps) = the_keys_under(lines, job)
        .into_iter()
        .find(|&at| key_and_value(lines[at]).0 == "steps")
    else {
        return Vec::new();
    };
    the_keys_under(lines, steps)
        .into_iter()
        .map(|opens| {
            let runs = std::iter::once(opens)
                .chain(the_keys_under(lines, opens))
                .find_map(|at| {
                    let (key, value) = key_and_value(lines[at]);
                    match key.as_str() {
                        "uses" => Some(format!(
                            "uses: {}",
                            value.split('@').next().unwrap_or_default().trim()
                        )),
                        "run" => Some(format!("run: {value}")),
                        _ => None,
                    }
                })
                .unwrap_or_default();
            let held = as_the_steps_are_written(lines, opens);
            let lines = std::iter::once(opens)
                .chain(the_lines_under(lines, opens))
                .collect();
            Step {
                opens,
                runs,
                lines,
                held,
            }
        })
        .collect()
}

/// The lines of the step that opens at `opens`, written as
/// [`THE_RELEASE_STEPS`] writes them, each with the index it was read from.
///
/// **Each at its depth under the step's first key**, so an input moved out
/// of `with:` or a line that continues the value above it is a line that
/// differs; an action by its name, the commit beside it being the pins' to
/// judge; and the step's `name:` left out, which is a label. A line indented
/// less than the step's keys is marked as such, so it matches nothing.
fn as_the_steps_are_written(lines: &[&str], opens: usize) -> Vec<(usize, String)> {
    let keys_at = indent_of(lines[opens]) + 2;
    std::iter::once(opens)
        .chain(the_lines_under(lines, opens))
        .filter_map(|at| {
            let code = the_code_before_a_comment(lines[at]);
            let (depth, code) = if at == opens {
                (
                    Some(0),
                    code.strip_prefix("- ").unwrap_or(&code).trim().to_owned(),
                )
            } else {
                (indent_of(lines[at]).checked_sub(keys_at), code)
            };
            let (key, value) = key_and_value(&code);
            if depth == Some(0) && key == "name" {
                return None;
            }
            let code = if depth == Some(0) && key == "uses" {
                format!(
                    "uses: {}",
                    value.split('@').next().unwrap_or_default().trim()
                )
            } else {
                code
            };
            Some((
                at,
                depth.map_or_else(
                    || format!("(indented less than the step) {code}"),
                    |depth| format!("{}{code}", " ".repeat(depth)),
                ),
            ))
        })
        .collect()
}

/// The release job runs [`THE_RELEASE_STEPS`], in order, and nothing else;
/// and each step holds its own lines and no other.
fn the_release_runs_its_steps(steps: &[Step], findings: &mut Vec<Finding>) {
    let ran: Vec<&str> = steps.iter().map(|step| step.runs.as_str()).collect();
    let listed: Vec<&str> = THE_RELEASE_STEPS.iter().map(|step| step[0]).collect();
    let quoted = |step: Option<&&str>, otherwise: &str| {
        step.filter(|step| !step.is_empty())
            .map_or_else(|| otherwise.to_owned(), |step| format!("`{}`", step.trim()))
    };
    if let Some(at) = (0..ran.len().max(listed.len())).find(|&at| ran.get(at) != listed.get(at)) {
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: steps.get(at).map_or(1, |step| step.opens + 1),
            what: format!(
                "the release job's step {} is {}, where it is {}",
                at + 1,
                quoted(ran.get(at), "missing"),
                quoted(listed.get(at), "nothing at all")
            ),
            fix: "the job holds the App's token, so what runs beside it is \
                  `THE_RELEASE_STEPS` in `xtask/src/workflow.rs` and nothing else: the \
                  token, the checkout, the toolchain, the audit's install, the stack, then \
                  the release. A step to add is a line there, which is a line in a diff"
                .to_owned(),
        });
    }
    for (number, (step, written)) in steps.iter().zip(THE_RELEASE_STEPS).enumerate() {
        if step.runs != written[0] {
            continue;
        }
        let held: Vec<&str> = step.held.iter().map(|(_, line)| line.as_str()).collect();
        let Some(differs) =
            (0..held.len().max(written.len())).find(|&at| held.get(at) != written.get(at))
        else {
            continue;
        };
        let nothing_more = || "nothing more".to_owned();
        findings.push(Finding {
            file: RELEASE_WORKFLOW.to_owned(),
            line: step
                .held
                .get(differs)
                .map_or(step.opens + 1, |(at, _)| at + 1),
            what: format!(
                "the release job's step {} holds {}, where it holds {}",
                number + 1,
                as_said(&held, differs).unwrap_or_else(nothing_more),
                as_said(written, differs).unwrap_or_else(nothing_more)
            ),
            fix: "an input is part of what an action does, and a key part of how a step \
                  runs: `repository:` or `ref:` beside the checkout's inputs builds another \
                  tree with the App's token, a `permission-` input asks the App for more \
                  than a release writes, and `env:`, `shell:` or `working-directory:` on a \
                  step change what its command is. Each step holds the lines \
                  `THE_RELEASE_STEPS` in `xtask/src/workflow.rs` writes for it, and a line \
                  to add is a line there"
                .to_owned(),
        });
    }
}

/// The line at `at` of a step's lines, as a finding says it: in backticks, with
/// the line it sits under named beside it.
///
/// **So two lines that differ by their indentation alone read differently**:
/// an input moved out of `with:` is the same text at another depth, and a
/// sentence that dropped the depth would say a line differs from itself.
fn as_said(lines: &[&str], at: usize) -> Option<String> {
    let line = lines.get(at)?;
    let depth = indent_of(line);
    let above = lines[..at]
        .iter()
        .rev()
        .find(|above| indent_of(above) < depth);
    Some(match above {
        Some(above) => format!("`{}` under `{}`", line.trim(), above.trim()),
        None => format!("`{}`", line.trim()),
    })
}

/// Every expression the release workflow holds is one of [`THE_EXPRESSIONS`],
/// on a line of its own, where that list puts it.
///
/// **Every line YAML may read one on, comments among them**
/// ([`as_yaml_reads_them`]): inside a block scalar a `#` line is text, and
/// GitHub evaluates what it holds; an escape can spell `${{` in a
/// double-quoted scalar; and a line that ends in a backslash continues on the
/// next. A line holding an expression is compared whole, so a second
/// expression on it, or a comment beside it, is a line that is not one of
/// these.
///
/// **Written anywhere else, the token is in the environment of what runs
/// there**: a job's `env:` hands it to every step, the stack's among them,
/// which builds and runs this tree's code. The release verb hands the token on
/// to `gh` alone (`crate::release::ghs_own`), and that is only worth anything
/// if the workflow hands it to the verb alone.
fn every_expression_is_where_the_release_needs_it(
    lines: &[&str],
    text: &str,
    steps: &[Step],
    findings: &mut Vec<Finding>,
) {
    for (at, read) in as_yaml_reads_them(text) {
        if !read.iter().any(|version| version.contains("${{")) {
            continue;
        }
        let written = read[0].trim().trim_start_matches("- ").trim();
        let placed = THE_EXPRESSIONS.iter().any(|&(expression, place)| {
            written == expression && the_lines_of(lines, steps, place).contains(&at)
        });
        if !placed {
            findings.push(Finding {
                file: RELEASE_WORKFLOW.to_owned(),
                line: at + 1,
                what: format!(
                    "the release workflow evaluates `{written}`, which is not one of \
                     `THE_EXPRESSIONS` where it stands"
                ),
                fix: "GitHub evaluates an expression before anything runs, and \
                      `toJSON(steps)` or `steps['app'].outputs.token` hands the App's token \
                      to whatever reads the line as surely as `steps.app.outputs.token` does. \
                      The release evaluates six, each on a line of its own where \
                      `THE_EXPRESSIONS` in `xtask/src/workflow.rs` puts it: the branch in the \
                      concurrency group, the environment and the channel, the App's id and \
                      key in the step that mints the token, and the token in the release \
                      step's `GH_TOKEN`. Anywhere else the key or the token is in the \
                      environment of what runs there, this tree's build scripts and tests \
                      among them, and the App may push to `staging` and `main` past their \
                      rules"
                    .to_owned(),
            });
        }
    }
}

/// The indices of the lines `place` names in the release workflow.
fn the_lines_of(lines: &[&str], steps: &[Step], place: Place) -> Vec<usize> {
    let the_step = |step: &[&str]| {
        steps
            .iter()
            .find(|held| held.runs == step[0])
            .map(|held| held.lines.clone())
            .unwrap_or_default()
    };
    match place {
        Place::ConcurrencyGroup => top_level(lines, "concurrency")
            .map(|at| the_lines_under(lines, at))
            .unwrap_or_default(),
        Place::Job => the_job(lines, "release")
            .map(|at| the_keys_under(lines, at))
            .unwrap_or_default(),
        Place::MintingStep => the_step(THE_RELEASE_STEPS[0]),
        Place::ReleaseStep => the_step(THE_RELEASE_STEPS[THE_RELEASE_STEPS.len() - 1]),
    }
}

/// How far a line is indented.
fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A line's code past every `-` that opens a sequence entry on it, and past
/// the whitespace after each.
///
/// **However much whitespace follows the `-`**, because YAML lets an entry's
/// node stand any number of spaces after it, and the node's keys stand where
/// it does. Read past `- ` alone, `-   run: |` put a block scalar's key at
/// the wrong column, `-   run: C=cargo; $C clippy --workspace` kept `run:` on
/// the name of the assignment, and `-   name:` was a command to one reader and
/// a label to another. A `-` with no space or tab after it is a letter of a
/// plain scalar, as in `-main`.
fn past_its_dashes(code: &str) -> &str {
    let mut entry = code.trim_start();
    while let Some(rest) = entry
        .strip_prefix('-')
        .filter(|rest| rest.starts_with([' ', '\t']))
    {
        entry = rest.trim_start();
    }
    entry
}

/// The lines a key at `at` holds: every line below it indented further, up to
/// the first that is not, with blank lines and comments passed over.
fn the_lines_under(lines: &[&str], at: usize) -> Vec<usize> {
    let indent = indent_of(lines[at]);
    let mut under = Vec::new();
    for (index, line) in lines.iter().enumerate().skip(at + 1) {
        if the_code_before_a_comment(line).is_empty() {
            continue;
        }
        if indent_of(line) <= indent {
            break;
        }
        under.push(index);
    }
    under
}

/// The keys directly under the key at `at`: the lines it holds at the
/// shallowest indentation among them.
fn the_keys_under(lines: &[&str], at: usize) -> Vec<usize> {
    let under = the_lines_under(lines, at);
    let Some(shallowest) = under.iter().map(|&index| indent_of(lines[index])).min() else {
        return Vec::new();
    };
    under
        .into_iter()
        .filter(|&index| indent_of(lines[index]) == shallowest)
        .collect()
}

/// A line's key and the value on its own line, comment and list marker off.
fn key_and_value(line: &str) -> (String, String) {
    let code = the_code_before_a_comment(line);
    let code = code.trim_start_matches("- ");
    match code.split_once(':') {
        Some((key, value)) => (key.trim().to_owned(), value.trim().to_owned()),
        None => (code.trim().to_owned(), String::new()),
    }
}

/// The line the top-level key `key` opens on.
fn top_level(lines: &[&str], key: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| indent_of(line) == 0 && key_and_value(line).0 == key)
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

/// Every directory one file of CI's may run a command in, as it names it:
/// the value of each `working-directory` key, wherever YAML opens one or on the
/// line below a key that holds none on its own, and the word after each `cd`
/// or `pushd`, wherever on a line it stands.
///
/// **Not only where a line opens on one.** Read as a line's first key, or
/// after `cd ` or `- run: cd ` at its start, `- working-directory: scripts`,
/// the same key in a flow mapping, `-   run: cd scripts && …`, `run: cd
/// scripts && …` as a step's second key and `cd scripts; bash gate.sh` each
/// left the script that `bash gate.sh` runs unwalked, and the `cargo fmt
/// --all` it held passed with nothing said. A word read as a directory that no
/// command runs in only gives a script one more place to be found.
fn the_directories_a_file_names(text: &str) -> Vec<String> {
    let mut named = Vec::new();
    // A `working-directory` key whose value is on the line below it.
    let mut below = false;
    for line in text.lines() {
        // The checkout root before the line is cut into words, as
        // `what_this_word_names` reads it.
        let code = with_the_escapes_resolved(&the_command_without_its_comment(line))
            .replace("${{ github.workspace }}", ".")
            .replace("${{github.workspace}}", ".")
            .replace("${GITHUB_WORKSPACE}", ".")
            .replace("$GITHUB_WORKSPACE", ".");
        if below && !code.trim().is_empty() {
            named.push(past_its_dashes(&code).to_owned());
            below = false;
        }
        for keyed in the_keys_named(&code, "working-directory") {
            below |= keyed.value.is_empty();
            named.push(keyed.value);
        }
        let words: Vec<&str> = code
            .split(|letter: char| {
                letter.is_whitespace() || matches!(letter, ';' | '&' | '|' | '(' | ')' | '{' | '}')
            })
            .filter(|word| !word.is_empty())
            .collect();
        for (at, word) in words.iter().enumerate() {
            if matches!(word.trim_matches(['"', '\'']), "cd" | "pushd")
                && let Some(next) = words[at + 1..].iter().find(|next| !next.starts_with('-'))
            {
                named.push((*next).to_owned());
            }
        }
    }
    named
        .iter()
        .map(|word| word.trim_matches(['"', '\'']).to_owned())
        .filter(|word| !word.is_empty())
        .collect()
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
    // a script is resolved against them as well as against the root
    // ([`the_directories_a_file_names`]).
    let mut directories: Vec<std::path::PathBuf> = Vec::new();
    for path in walked {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for named in the_directories_a_file_names(&text) {
            if let Some(directory) = under(root, &named).filter(|path| path.is_dir()) {
                directories.push(directory);
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
            let trimmed = past_its_dashes(line.trim());
            // `uses: ./ci/actions/extra` — a directory holding an action file.
            // **Read by the reader the pins read with** ([`the_actions_used`]),
            // so every action the pins pass over as local is walked, however its
            // key is written. Read where a line opens on `- uses:`,
            // `-   uses: ./ci/actions/extra`, `- { uses: ./ci/actions/extra }`
            // and `- "uses": ./ci/actions/extra` each left the action's
            // `cargo clippy --workspace` unread, with nothing said.
            for local in the_actions_used(line)
                .into_iter()
                .flatten()
                .filter(|used| used.starts_with('.'))
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

    no_step_runs_on_a_condition(WORKFLOW, ci, findings);
}

/// No step or job of `file` runs on a condition: whatever `if:` says, the
/// step may not run, and every rule here reads what it runs.
///
/// **Of each workflow**, the gate's and the release's: a condition on the
/// release's steps would let one of them stand aside on a run that
/// releases, as surely as one on the gate's.
///
/// **Every `if` key, wherever YAML opens one** ([`the_keys_named`]), read
/// every way [`as_yaml_reads_them`] gives each line, with its condition on its
/// line or on the lines below it. Read where a line opened on `if:`, `"if":`
/// or `'if':` alone, `"i\x66": false` on the step that runs the gate, and
/// `if:` with `false` on the line below, each skipped the gate in CI with
/// nothing said.
fn no_step_runs_on_a_condition(file: &str, text: &str, findings: &mut Vec<Finding>) {
    // **Any condition, not only the literal false.**
    // `if: github.event_name == 'never-happens'` never holds either, and a gate
    // cannot evaluate an expression — so what it can say is that a job carrying
    // a condition is a job that might not run, which is the thing this file
    // exists to prevent.
    //
    // **And a step's, not only a job's.** A reader of a line's first word
    // misses `- if: false`, the *step*-level spelling, which is what a step
    // condition looks like in YAML. Measured, on the step that runs the gate:
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
    // because `commands_in` still reads the untouched `run:` line.
    let lines: Vec<&str> = text.lines().collect();
    let mut said: Vec<usize> = Vec::new();
    for (index, read) in as_yaml_reads_them(text) {
        let codes = read
            .iter()
            .map(|version| the_code_before_a_comment(version));
        for keyed in codes.flat_map(|code| the_keys_named(&code, "if")) {
            let written = if keyed.value.is_empty() {
                the_lines_under(&lines, index)
                    .into_iter()
                    .map(|at| the_code_before_a_comment(lines[at]))
                    .collect::<Vec<_>>()
                    .join(" ")
            } else {
                keyed.value
            };
            let condition = written.trim().trim_matches(['\'', '"']);
            // `if:` with nothing after it and nothing under it states no
            // condition.
            if condition.is_empty() || said.contains(&index) {
                continue;
            }
            said.push(index);
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: format!("this step or job runs only when `{condition}`"),
                fix: "a required job that runs conditionally is a required job that may not \
                      run, and every check here reads its name and its body rather than \
                      whether it was reached. `if: false` was the spelling closed first; \
                      a comparison against an event name is the same thing said in a way \
                      no gate can evaluate. Remove the condition, or remove the job so its \
                      absence is a finding"
                    .to_owned(),
            });
        }
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
        // A step named after what it runs is prose. `is_only_a_label` is what
        // the walk of cargo commands uses, and two halves disagreeing about
        // whether a `name:` is a command is a defect of its own.
        if is_only_a_label(code) {
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
    // **And the two of those that are keys, wherever YAML opens one**
    // ([`the_keys_named`]), in a file GitHub runs. The needles read a line with
    // its quoted text spent, so `"continue-on-error": true` and `"shell": bash
    // {0}` on the step that runs the gate were each a key GitHub reads and no
    // needle did, with nothing said; an escape and a flow mapping are the same.
    if !github_runs_it(file) {
        return;
    }
    for (index, read) in as_yaml_reads_them(text) {
        for (key, needle) in NEUTERS_A_STEP {
            let Some((_, what, fix)) = NEUTERED.iter().find(|(held, _, _)| *held == needle) else {
                continue;
            };
            let keyed = read.iter().any(|version| {
                !the_keys_named(&the_code_before_a_comment(version), key).is_empty()
            });
            let already = findings.iter().any(|finding| {
                finding.file == file && finding.line == index + 1 && finding.what == *what
            });
            if keyed && !already {
                findings.push(Finding {
                    file: file.to_owned(),
                    line: index + 1,
                    what: (*what).to_owned(),
                    fix: (*fix).to_owned(),
                });
            }
        }
    }
}

/// The keys of [`NEUTERED`], each with the needle there that names it: a step
/// that chooses its own shell, and one whose failure is not the job's.
const NEUTERS_A_STEP: [(&str, &str); 2] = [
    ("shell", "shell:"),
    ("continue-on-error", "continue-on-error"),
];

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
        // `JOBS_MUST_RUN`'s own doc names. **Every label, and not
        // `is_only_a_label`**, which the readers that refuse use: there a label
        // read as a command refuses more, and here it is a requirement met by
        // prose.
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

/// Every action a line invokes that may run cargo: each that is not a local
/// action, the release's token action or one of [`SETS_UP_AND_JUDGES_NOTHING`].
///
/// `actions-rs/cargo` <!-- allow-reference: names `actions-rs/cargo` — a GitHub Action, not a path in this tree --> is the standard GitHub Action for it, and it takes the
/// subcommand in a `with:` block rather than in a `run:` line — so a gate that
/// reads what a step *runs* never sees it.
fn runs_cargo_as_an_action(line: &str) -> Vec<String> {
    the_actions_used(line)
        .into_iter()
        .filter_map(|uses| {
            // **An action this cannot read is not one it can say is allowed.**
            // `every_action_is_a_commit` says what is wrong with it; here it
            // is counted like any action off the list.
            let Some(uses) = uses else {
                return Some("uses: <an action this walk cannot read>".to_owned());
            };
            let name = uses.split('@').next().unwrap_or(&uses).trim();
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
            // **The token's action runs no cargo either**, and where it may
            // stand is the release's rule, not this one: the first of
            // `THE_RELEASE_STEPS`, and nowhere in any other file
            // (`nothing_else_holds_the_releases_authority`).
            if MINTS_THE_RELEASE_TOKEN.eq_ignore_ascii_case(name) {
                return None;
            }
            Some(format!("uses: {name}"))
        })
        .collect()
}

/// Every action one line uses: what each of its `uses` keys names, or `None`
/// for one whose value this reader cannot read on the line.
///
/// **One reader of `uses:`**, for the allow-list, the pins and the release's
/// cache rule, so the three cannot disagree about which lines use an action.
///
/// **Every `uses` key on the line, wherever YAML opens one**
/// ([`the_keys_named`]), with a double-quoted scalar's escapes resolved.
/// Measured against a reader of the line's first key: `- { name: lint, uses:
/// giraffate/clippy-action@v1 }` and `- "uses": giraffate/clippy-action@v1` <!-- allow-reference: names `giraffate/clippy-action@v1` — a GitHub Action, not a path in this tree -->
/// each passed the allow-list and the pins with nothing said, and before
/// that, `- { uses: X }` with its braces unstripped did the same.
///
/// A value on the lines after the key, a block scalar, an anchor, a tag, an
/// alias and a collection are not read: each is `None`, which
/// [`every_action_is_a_commit`] refuses.
fn the_actions_used(line: &str) -> Vec<Option<String>> {
    let code = with_the_escapes_resolved(&the_command_without_its_comment(line));
    the_keys_named(&code, "uses")
        .into_iter()
        .map(|keyed| {
            let value = keyed.value;
            if value.is_empty() || value.starts_with(['|', '>', '&', '!', '*', '{', '[']) {
                return None;
            }
            match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value
                    .strip_prefix(quote)
                    .and_then(|inner| inner.strip_suffix(quote))
                    .map(|inner| inner.trim().to_owned()),
                _ => Some(value),
            }
        })
        .collect()
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
///
/// **At every `=`, and not after what a line opens with.** Read past `- `
/// and `run:` alone, `-   run: C=cargo`, `- run: "C=cargo; …"`, `- "run":
/// C=cargo; …`, `- { run: C=cargo; … }`, `JOBS=1 C=cargo; …` and `C=cargo #
/// the tool` above a `$C clippy --workspace` each kept a word in front of the
/// name or after the value, and a second lints gate passed with nothing said.
/// The comment is cut first, as a shell cuts it, and a name is then read at
/// each `=` the rest of the line holds.
fn assigns_cargo(line: &str) -> Option<String> {
    let code = the_command_without_its_comment(line);
    code.match_indices('=')
        .find_map(|(at, _)| the_variable_one_statement_assigns(&code[..at], &code[at + 1..]))
}

/// What lets a word open right after it: a space, a separator, a subshell or
/// a group, and a quote, which may open the value YAML hands a shell.
const A_WORD_OPENS_AFTER: [char; 9] = [';', '&', '|', '(', '{', '[', '"', '\'', '`'];

/// The variable one `=` assigns `cargo` to, if it is an assignment and assigns
/// that: `before` is the line up to the `=`, and `after` the rest of it.
fn the_variable_one_statement_assigns(before: &str, after: &str) -> Option<String> {
    // **The name is the word that ends at the `=`**, so `export CARGO=cargo`,
    // `alias c=cargo` and `JOBS=1 C=cargo` each name the variable, whatever
    // stands before it; and `CARGO=$(which cargo)` resolves the path at run
    // time. Each runs cargo, and unread it leaves the count unchanged. A word
    // that opens after anything else, as `--root=cargo` does, is no name.
    let before = before.trim_end();
    let name = &before[before
        .trim_end_matches(|letter: char| letter.is_alphanumeric() || letter == '_')
        .len()..];
    let opens_a_word = before[..before.len() - name.len()]
        .chars()
        .next_back()
        .is_none_or(|letter| letter.is_whitespace() || A_WORD_OPENS_AFTER.contains(&letter));
    if name.is_empty() || !opens_a_word {
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
    // **The value ends where the shell ends it**: a command substitution at
    // its `)`, a quoted value at its quote, and anything else at the first
    // space, separator or bracket. Read as one value, everything after the `=`
    // in `CARGO=cargo; $CARGO clippy …` — assignment and use on one line, which
    // is ordinary shell — is `cargo; $CARGO clippy …`, matches nothing, and
    // leaves `ok 11 cargo command(s)` with a run byte-identical to a clean one.
    let after = after.trim_start();
    let end = match after.chars().next() {
        Some('$') if after.starts_with("$(") => after.find(')').map(|at| at + 1),
        Some(quote @ ('"' | '\'' | '`')) => after[1..].find(quote).map(|at| at + 2),
        _ => after.find(|letter: char| {
            letter.is_whitespace() || matches!(letter, ';' | '&' | '|' | ')' | '}' | ']' | ',')
        }),
    };
    let value = after[..end.unwrap_or(after.len())].trim_matches(['"', '\'', '`']);
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
const ASSIGNMENTS: [(&str, Option<&str>); 23] = [
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
    // **Whatever a line opens with, and whatever follows the value**: a wide
    // `-`, a quoted value, a quoted key, a flow mapping, a second assignment
    // in front of it, a comment after it, and a substitution in backticks.
    ("-   run: C=cargo; $C clippy --workspace", Some("C")),
    ("- run: \"C=cargo; $C clippy --workspace\"", Some("C")),
    ("- run: 'C=cargo; $C clippy --workspace'", Some("C")),
    ("- \"run\": C=cargo; $C clippy --workspace", Some("C")),
    ("- { run: C=cargo; $C clippy --workspace }", Some("C")),
    ("run: JOBS=1 C=cargo; $C clippy --workspace", Some("C")),
    ("C=cargo # the tool", Some("C")),
    ("run: C=`which cargo`", Some("C")),
    // **The value ends where the shell ends it**, and another assignment
    // after it is no part of it.
    ("run: C=cargo X=1; $C clippy --workspace", Some("C")),
    ("run: C=\"cargo\" X=1; $C clippy --workspace", Some("C")),
    // A flag's value is no variable.
    ("run: cargo install --locked --root=cargo cargo-deny", None),
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
    // holds two interpolations, `{{channel}}` and `{{flags}}`, each a declared
    // parameter of `release`. A call form that ever does need to be written
    // here can be named in the header beside the parameters, where a reader
    // can see it.
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
    // `@release channel *flags:` makes this gate say "`@release` is the name a
    // developer types and `release` is what this body runs" — inverted about
    // the one value it quotes, because `just --list` prints `release` and
    // `just release staging` is what a developer types, and the remedy it
    // implies is to rename a recipe that is already named correctly. The sigil
    // is at the front, so the `-offline` reading beside this is untouched.
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
/// parameters after the name — `release channel *flags:`. `is_label` cannot
/// be asked this: it matches a YAML `name:` key, and a justfile has none.
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

/// Whether a line is a YAML `name:` rather than something that runs, after
/// its entry's `-` and whatever whitespace follows it ([`past_its_dashes`]).
fn is_label(trimmed: &str) -> bool {
    past_its_dashes(trimmed).starts_with("name:")
}

/// What lets a shell run more than a command's first word on its line.
///
/// A separator, a pipe, an expansion or a substitution, a subshell, a
/// redirection, and a line continued.
const WHAT_A_SHELL_RUNS_PAST_A_WORD: [char; 10] =
    [';', '&', '|', '`', '$', '(', ')', '<', '>', '\\'];

/// Whether a line is a label and nothing a shell would run: [`is_label`],
/// holding none of [`WHAT_A_SHELL_RUNS_PAST_A_WORD`].
///
/// **A line that opens on `name:` is not always YAML.** Inside a block scalar,
/// and in a script or the justfile, it is a command, and `name: ; cargo
/// clippy --workspace` there runs clippy once `name:` is not found. Skipped as
/// a label, that line put a second lints gate in a `run: |` block of `ci.yml`,
/// and in a script a step runs, with the count unmoved and nothing said. So
/// the readers that refuse skip a label only when a shell would run nothing
/// past its first word either, and a label holding a separator is read as the
/// command it may be, which refuses more and never less. [`commands_in`], which
/// looks for what a job must run, still passes over every label: there, a
/// label read as a command is a requirement met by prose.
fn is_only_a_label(trimmed: &str) -> bool {
    is_label(trimmed) && !trimmed.contains(WHAT_A_SHELL_RUNS_PAST_A_WORD)
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
                "\nrelease channel *flags:\n",
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

    /// The shipped workflow with `step` written after its first cache step.
    ///
    /// **Found by the action's name**, so the pin beside it can move without
    /// moving this.
    fn after_the_cache(workflow: &str, step: &str) -> String {
        let at = workflow
            .find("      - uses: Swatinem/rust-cache@")
            .expect("the shipped workflow restores a cache");
        let end = at + workflow[at..].find('\n').expect("the step ends its line") + 1;
        format!("{}{step}{}", &workflow[..end], &workflow[end..])
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
            (
                // **And wherever YAML opens the key.** Each of the next three
                // passed the allow-list while it read a line's first key alone.
                "somebody else's action, after another key of a flow mapping",
                "      - { name: lint, uses: giraffate/clippy-action@v1 }\n",
                None,
            ),
            (
                "somebody else's action, under a quoted key",
                "      - \"uses\": giraffate/clippy-action@v1\n",
                None,
            ),
            (
                "somebody else's action, in a flow sequence of steps",
                "    steps: [ { uses: actions/checkout@v7 }, { uses: giraffate/clippy-action@v1 } ]\n",
                None,
            ),
            (
                // An action this reader cannot read is not one it can say is
                // on the list.
                "somebody else's action, behind an anchor",
                "      - uses: &a giraffate/clippy-action@v1\n",
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
            let planted = after_the_cache(&workflow, step);
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
                "      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1\n        with:\n          components: rustfmt\n",
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
            let planted = after_the_cache(&workflow, step);
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
                "release channel",
                "    cargo xtask release --channel {{channel}}",
                false,
            ),
            (
                "release channel=\"staging\"",
                "    cargo xtask release --channel {{channel}}",
                false,
            ),
            (
                "release +channel",
                "    cargo xtask release --channel {{channel}}",
                false,
            ),
            (
                "release *channel",
                "    cargo xtask release --channel {{channel}}",
                false,
            ),
            (
                "release $channel",
                "    cargo xtask release --channel {{channel}}",
                false,
            ),
            (
                "release +channel",
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
        let assigned = "flags := \"\"\"\ncheck: not a recipe\n\"\"\"\nrelease channel:\n    \
                        cargo xtask release --channel {{channel}}\n";
        assert_eq!(
            super::the_recipe_above(assigned, assigned.lines().count() - 1, false, "justfile"),
            "release channel",
            "the content line of a multi-line assignment was read as the recipe header"
        );
        // **A `#` comment holding a triple opens nothing.** `just` ends a
        // comment at the newline, so the quote inside one is not a string; a
        // reader that carries it swallows every line below into one group.
        let commented = "# a note about \"\"\" quoting\nrelease channel:\n    \
                         cargo xtask release --channel {{channel}}\n";
        assert_eq!(
            super::the_recipe_above(commented, commented.lines().count() - 1, false, "justfile"),
            "release channel",
            "a comment holding a triple quote opened a string that never closed"
        );
        // And the two legitimate spellings the `#` guard must not refuse, both
        // of which `just --dump` echoes back verbatim.
        for (what, text, header) in [
            (
                "a hash inside a quoted default",
                "release channel=\"a#b\":\n    cargo xtask release --channel {{channel}}\n",
                "release channel=\"a#b\"",
            ),
            (
                "a trailing comment after the header",
                "release channel: # the gate this wraps\n    cargo xtask release --channel {{channel}}\n",
                "release channel",
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
        let body = "\n    cargo xtask release --channel {{channel}}\n";
        for (what, text, expected) in [
            ("the plain header", "release channel:", "release channel"),
            (
                "a dependency after the colon",
                "release channel: build",
                "release channel",
            ),
            ("the echo sigil", "@release channel:", "@release channel"),
            (
                "a default and a dependency together",
                "release channel=\"staging\": build",
                "release channel=\"staging\"",
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
        let file = format!("@release channel *flags:{body}");
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
        assert!(is_label("-   name: cargo xtask check"));
        assert!(is_label("name: check"));
        assert!(!is_label("- run: cargo xtask check"));
        assert!(!is_label("cargo xtask build"));
        // **The readers that refuse skip a label a shell would run nothing
        // past**, and read the rest as the commands they may be.
        assert!(super::is_only_a_label("- name: cargo xtask check"));
        assert!(!super::is_only_a_label("name: ; cargo clippy --workspace"));
        assert!(!super::is_only_a_label("name: $(cargo clippy --workspace)"));

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

    /// The plugins' target is asked for by the toolchain file, and every job
    /// that builds installs from that file.
    ///
    /// **`cargo xtask check` builds every plugin for that target before the
    /// tests, and rustup installs no target it is not asked for.** It reads
    /// `[toolchain].targets` in `rust-toolchain.toml`, and every job that
    /// builds — the three that run the gate, the determinism job, and the
    /// release job, whose verb runs the gate — runs `rustup toolchain install`
    /// with no toolchain named, which installs what the file asks for. A
    /// toolchain named on that line, or installed by an action, is installed
    /// as it was told and not as the file says. A machine that already has the
    /// target stays green with the file's line deleted, and a fresh clone or a
    /// runner then fails its first gate at the `plugins` step — which is why
    /// both are read here rather than left to that failure.
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

        // **Every job that builds**, because each builds on its own runner.
        let ci = std::fs::read_to_string(root().join(WORKFLOW)).expect("the workflow is readable");
        let release = std::fs::read_to_string(root().join(super::RELEASE_WORKFLOW))
            .expect("the release workflow is readable");
        for (file, text, job) in [
            (WORKFLOW, &ci, "linux"),
            (WORKFLOW, &ci, "macos"),
            (WORKFLOW, &ci, "windows"),
            (WORKFLOW, &ci, "deterministic"),
            (super::RELEASE_WORKFLOW, &release, "release"),
        ] {
            let body = job_body(text, job).unwrap_or_else(|| panic!("`{file}` has a `{job}` job"));
            let installs: Vec<String> = super::commands_in(&body)
                .into_iter()
                .filter(|command| command.starts_with("rustup"))
                .collect();
            assert_eq!(
                installs,
                ["rustup toolchain install --no-self-update"],
                "the `{job}` job in `{file}` installs its toolchain with {installs:?}. With no \
                 toolchain named, rustup installs what `rust-toolchain.toml` asks for, \
                 `{target}` among it; a toolchain named there is installed as it was told"
            );
        }
    }

    /// A scratch tree holding this repository's workflows as they ship, with a
    /// change made to the text of `file`, and what the bounds say of it.
    ///
    /// **The shipped files, not a fragment**, so each case is the one change it
    /// names made to what this repository runs, and
    /// [`the_shipped_workflows_are_bounded`] is what says the files without the
    /// change are clean.
    fn bounded_after(
        about: &str,
        file: &str,
        change: impl FnOnce(&str) -> String,
    ) -> Vec<crate::gates::Finding> {
        let held = crate::check::LeavesNothingBehind::new(&format!("bounded-{about}"));
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        for shipped in super::THE_WORKFLOWS {
            std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("a shipped workflow");
        }
        let text = std::fs::read_to_string(scratch.join(file)).expect("copied above");
        std::fs::write(scratch.join(file), change(&text)).expect("writable");
        super::every_workflow_is_bounded(&scratch).0
    }

    /// `text` with its first `from` replaced by `to`, refused when `from` is not
    /// there, so a case cannot pass by changing nothing.
    fn once(text: &str, from: &str, to: &str) -> String {
        assert!(
            text.contains(from),
            "`{from}` is not in the shipped workflow, so this case changes nothing"
        );
        text.replacen(from, to, 1)
    }

    /// Each case: what it is, the file, the text it replaces and with what,
    /// and what a finding then says.
    type Case<'a> = (&'a str, &'a str, &'a str, &'a str, &'a str);

    /// Runs every case through [`bounded_after`] and asserts each is said.
    #[track_caller]
    fn each_is_said(cases: &[Case<'_>]) {
        for (about, file, from, to, said) in cases {
            let findings = bounded_after(about, file, |text| once(text, from, to));
            assert!(
                findings.iter().any(|finding| finding.what.contains(said)),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// The workflows as they ship are bounded, and every action in them was
    /// judged.
    ///
    /// **Every `uses` key, counted a second way**, because the pins are the
    /// one rule here that only refuses: a reader that judged none of them
    /// would say nothing, and so would a reader that judged half. Counted
    /// wherever the word stands before a colon, quoted or not, and not only
    /// where a line opens on it: a count of lines that open on `uses:` agreed
    /// with a reader that missed a flow mapping's second key.
    #[test]
    fn the_shipped_workflows_are_bounded() {
        let (findings, judged) = super::every_workflow_is_bounded(root());
        assert!(findings.is_empty(), "{findings:#?}");
        let written: usize = super::THE_WORKFLOWS
            .iter()
            .map(|file| {
                std::fs::read_to_string(root().join(file))
                    .expect("a shipped workflow")
                    .lines()
                    .map(|line| {
                        let code =
                            super::the_command_without_its_comment(line).to_ascii_lowercase();
                        code.match_indices("uses")
                            .filter(|&(at, _)| {
                                code[at + "uses".len()..]
                                    .trim_start_matches(['"', '\''])
                                    .trim_start()
                                    .starts_with(':')
                            })
                            .count()
                    })
                    .sum::<usize>()
            })
            .sum();
        assert_eq!(
            judged, written,
            "the workflows hold {written} `uses` key(s) and {judged} action(s) were judged"
        );
        let unchanged = bounded_after("unchanged", super::RELEASE_WORKFLOW, str::to_owned);
        assert!(unchanged.is_empty(), "{unchanged:#?}");
    }

    /// `.github/` holds the two workflows, both of them, and nothing else, and
    /// a workflow that cannot be read is said to be so.
    #[test]
    fn github_holds_the_workflows_and_nothing_else() {
        let held = crate::check::LeavesNothingBehind::new("bounded-held");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::copy(root().join(WORKFLOW), scratch.join(WORKFLOW)).expect("readable");
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what == format!("`{}` is missing", super::RELEASE_WORKFLOW)),
            "a tree with no release workflow: {findings:#?}"
        );

        std::fs::copy(
            root().join(super::RELEASE_WORKFLOW),
            scratch.join(super::RELEASE_WORKFLOW),
        )
        .expect("readable");
        let strays = [".github/workflows/nightly.yml", ".github/dependabot.yml"];
        for stray in strays {
            std::fs::write(scratch.join(stray), "version: 2\n").expect("writable");
        }
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        for stray in strays {
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.file == stray && finding.what.contains("sits under")),
                "`{stray}` was not said: {findings:#?}"
            );
        }
        for stray in strays {
            std::fs::remove_file(scratch.join(stray)).expect("written above");
        }

        std::fs::write(scratch.join(super::RELEASE_WORKFLOW), [0xff, 0xfe, b'\n'])
            .expect("writable");
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        assert!(
            findings
                .iter()
                .any(|finding| finding.what.contains("could not be read")),
            "a workflow that is not UTF-8: {findings:#?}"
        );
    }

    /// Each workflow opens with no permission, and a job asks for reading the
    /// tree and nothing more — in a block, a flow mapping or a word.
    ///
    /// **And under a key a reader of the bare word misses**: quoted, behind an
    /// escape, and in a job written whole as a flow mapping. Each passed with
    /// no finding while only a line opening on `permissions:` was read, and a
    /// quoted grant of reading alone still passes.
    #[test]
    fn a_workflow_opens_with_no_permission_and_a_job_asks_to_read_alone() {
        let release = super::RELEASE_WORKFLOW;
        let a_job = "    permissions:\n      contents: read\n";
        each_is_said(&[
            (
                "a-quoted-grant",
                WORKFLOW,
                a_job,
                "    \"permissions\":\n      contents: write\n      id-token: write\n",
                "asks for `id-token: write`",
            ),
            (
                "an-escaped-grant",
                WORKFLOW,
                a_job,
                "    \"perm\\x69ssions\":\n      contents: write\n",
                "asks for `contents: write`",
            ),
            (
                "an-anchored-grant",
                WORKFLOW,
                a_job,
                "    &p permissions:\n      contents: write\n",
                "asks for `contents: write`",
            ),
            (
                "a-job-in-a-flow-mapping",
                WORKFLOW,
                "jobs:\n",
                "jobs:\n  extra: { runs-on: ubuntu-latest, permissions: write-all, steps: [ { run: cargo xtask check } ] }\n",
                "asks for `write-all`",
            ),
            (
                "a-flow-grant-in-a-flow-mapping",
                WORKFLOW,
                "jobs:\n",
                "jobs:\n  extra: { runs-on: ubuntu-latest, \"permissions\": { contents: read, actions: write } }\n",
                "asks for `actions: write`",
            ),
        ]);
        let quoted = bounded_after("a-quoted-read", WORKFLOW, |text| {
            once(
                text,
                a_job,
                "    \"permissions\":\n      \"contents\": 'read'\n",
            )
        });
        assert!(
            quoted.is_empty(),
            "a grant of reading alone, quoted, asks for nothing more: {quoted:#?}"
        );
        each_is_said(&[
            (
                "unopened",
                WORKFLOW,
                "permissions: {}\n",
                "",
                "does not open with `permissions: {}`",
            ),
            (
                "opened-wide",
                release,
                "permissions: {}\n",
                "permissions: write-all\n",
                "opens with `permissions: write-all`",
            ),
            (
                "a-job-that-writes",
                WORKFLOW,
                "      contents: read\n",
                "      contents: write\n",
                "asks for `contents: write`",
            ),
            (
                "a-job-that-merges",
                release,
                "      contents: read\n",
                "      contents: read\n      pull-requests: write\n",
                "asks for `pull-requests: write`",
            ),
            (
                "a-flow-grant",
                WORKFLOW,
                "    permissions:\n      contents: read\n",
                "    permissions: { contents: read, actions: write }\n",
                "asks for `actions: write`",
            ),
            (
                "a-word-grant",
                release,
                "    permissions:\n      contents: read\n",
                "    permissions: write-all\n",
                "asks for `write-all`",
            ),
        ]);
    }

    /// No workflow runs on an event that lends a fork's change this
    /// repository's authority.
    ///
    /// **However the event is spelled**: as a block key, in a flow sequence,
    /// quoted, behind a `\x` or a `\u` escape, across a backslash's line break,
    /// or in capitals. Each escaped spelling passed while the line was read as
    /// written alone.
    #[test]
    fn no_workflow_runs_for_a_fork_with_this_repositorys_authority() {
        let fires = "on:\n  push:\n  pull_request:\n";
        let target = "runs on `pull_request_target`";
        let run = "runs on `workflow_run`";
        each_is_said(&[
            (
                "pull-request-target",
                WORKFLOW,
                fires,
                "on:\n  push:\n  pull_request:\n  pull_request_target:\n",
                target,
            ),
            (
                "workflow-run",
                WORKFLOW,
                fires,
                "on:\n  push:\n  pull_request:\n  workflow_run:\n    workflows: [check]\n",
                run,
            ),
            (
                "an-escaped-block-key",
                WORKFLOW,
                fires,
                "on:\n  push:\n  pull_request:\n  \"pull_request\\x5ftarget\":\n",
                target,
            ),
            (
                "an-escaped-flow-sequence",
                WORKFLOW,
                fires,
                "on: [push, pull_request, \"pull_request\\x5ftarget\"]\n",
                target,
            ),
            (
                "a-unicode-escape",
                WORKFLOW,
                fires,
                "on:\n  push:\n  pull_request:\n  \"workflow\\u005frun\":\n    workflows: [check]\n",
                run,
            ),
            (
                "across-a-line-break",
                WORKFLOW,
                fires,
                "on: [push, pull_request, \"workflow_\\\n  run\"]\n",
                run,
            ),
            (
                "in-capitals",
                WORKFLOW,
                fires,
                "on:\n  push:\n  pull_request:\n  Pull_Request_Target:\n",
                target,
            ),
        ]);
    }

    /// Every action is pinned to a full commit with its release beside it.
    ///
    /// **In a workflow, and in a composite action** that a step reaches
    /// outside the `.github/` directory.
    #[test]
    fn every_action_is_pinned_to_a_commit_with_its_release() {
        let release = super::RELEASE_WORKFLOW;
        let checkout = "actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1";
        each_is_said(&[
            (
                "a-tag",
                WORKFLOW,
                checkout,
                "actions/checkout@v7",
                "`uses: actions/checkout@v7` is not pinned to a full commit",
            ),
            (
                "a-short-commit",
                release,
                checkout,
                "actions/checkout@3d3c42e",
                "is not pinned to a full commit",
            ),
            (
                "as-long-as-a-commit",
                release,
                checkout,
                "actions/checkout@gggggggggggggggggggggggggggggggggggggggg",
                "is not pinned to a full commit",
            ),
            (
                "a-branch",
                release,
                "actions/create-github-app-token@64ab729a6294000f5aa4a4bc0b73d1f32bc45510",
                "actions/create-github-app-token@main",
                "is not pinned to a full commit",
            ),
            (
                "no-revision",
                WORKFLOW,
                "Swatinem/rust-cache@6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
                "Swatinem/rust-cache",
                "is not pinned to a full commit",
            ),
            (
                "no-release",
                WORKFLOW,
                "6323deb102c322ba6fcbdcafc7e3dddab59af2b6 # v2.9.2",
                "6323deb102c322ba6fcbdcafc7e3dddab59af2b6",
                "is pinned with no release beside it",
            ),
        ]);

        let held = crate::check::LeavesNothingBehind::new("bounded-composite");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join("ci/actions/extra")).expect("writable");
        for shipped in super::THE_WORKFLOWS {
            std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("readable");
        }
        let ci = std::fs::read_to_string(scratch.join(WORKFLOW)).expect("copied");
        let ci = once(
            &ci,
            "    steps:\n",
            "    steps:\n      - uses: ./ci/actions/extra\n",
        );
        std::fs::write(scratch.join(WORKFLOW), ci).expect("writable");
        std::fs::write(
            scratch.join("ci/actions/extra/action.yml"),
            "runs:\n  using: composite\n  steps:\n    - uses: actions/checkout@v4\n",
        )
        .expect("writable");
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        assert!(
            findings
                .iter()
                .any(|finding| finding.file == "ci/actions/extra/action.yml"
                    && finding.what.contains("is not pinned")),
            "an action a composite action uses: {findings:#?}"
        );
    }

    /// An action is pinned wherever YAML opens its `uses` key, and one whose
    /// value is not on the key's line is refused.
    ///
    /// **More places than the start of a line**: after another key of a flow
    /// mapping, under a quoted or escaped key, and in a flow sequence of
    /// steps. Each passed the pins with nothing said while the reader read a
    /// line's first key alone.
    #[test]
    fn every_action_is_pinned_wherever_its_key_stands() {
        let gate = THE_GATE;
        let sha = "0123456789abcdef0123456789abcdef01234567";
        let after_the_gate: Vec<(&str, String, &str)> = vec![
            (
                "after-another-key",
                format!("{gate}      - {{ name: lint, uses: owner/a@v1 }}\n"),
                "`uses: owner/a@v1` is not pinned to a full commit",
            ),
            (
                "a-quoted-key",
                format!("{gate}      - \"uses\": owner/a@v1\n"),
                "`uses: owner/a@v1` is not pinned to a full commit",
            ),
            (
                "an-escaped-key",
                format!("{gate}      - \"us\\x65s\": owner/a@v1\n"),
                "`uses: owner/a@v1` is not pinned to a full commit",
            ),
            (
                "a-flow-sequence",
                format!(
                    "{gate}  extra:\n    steps: [ {{ uses: actions/checkout@{sha} }}, {{ uses: owner/a@v1 }} ] # v1\n"
                ),
                "`uses: owner/a@v1` is not pinned to a full commit",
            ),
            (
                "a-value-below-its-key",
                format!("{gate}      - uses:\n          owner/a@{sha} # v1\n"),
                "uses an action this gate cannot read on its line",
            ),
            (
                "an-anchored-value",
                format!("{gate}      - uses: &a owner/a@{sha} # v1\n"),
                "uses an action this gate cannot read on its line",
            ),
            (
                "a-key-across-a-line-break",
                format!("{gate}      - \"us\\\n        es\": owner/a@{sha} # v1\n"),
                "uses an action this gate cannot read on its line",
            ),
            (
                "after-a-verbatim-tag",
                format!("{gate}      - !<tag:yaml.org,2002:str> uses: owner/a@v1\n"),
                "`uses: owner/a@v1` is not pinned to a full commit",
            ),
        ];
        for (about, to, said) in &after_the_gate {
            let findings = bounded_after(about, WORKFLOW, |text| once(text, gate, to));
            assert!(
                findings.iter().any(|finding| finding.what.contains(said)),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// The release fires on a push to a channel's branch and on nothing else.
    #[test]
    fn the_release_fires_on_a_push_to_a_channel_and_on_nothing_else() {
        let release = super::RELEASE_WORKFLOW;
        let on = "on:\n  push:\n    branches: [staging, main]\n";
        each_is_said(&[
            (
                "a-pull-request",
                release,
                on,
                "on:\n  push:\n    branches: [staging, main]\n  pull_request:\n",
                "the release fires on `pull_request:`",
            ),
            (
                "a-dispatch",
                release,
                on,
                "on:\n  workflow_dispatch:\n  push:\n    branches: [staging, main]\n",
                "the release fires on `workflow_dispatch:`",
            ),
            (
                "a-tag",
                release,
                "    branches: [staging, main]\n",
                "    branches: [staging, main]\n    tags: ['v*']\n",
                "the release's push is filtered by `tags`",
            ),
            (
                "a-third-branch",
                release,
                "[staging, main]",
                "[staging, main, dev]",
                "fires on a push to [\"dev\", \"main\", \"staging\"]",
            ),
            (
                "every-branch",
                release,
                on,
                "on:\n  push:\n",
                "fires on a push to []",
            ),
            (
                "written-inline",
                release,
                on,
                "on: push\n",
                "does not say, as a block of its own, what it fires on",
            ),
            (
                "branches-that-open-on-a-dash",
                release,
                "    branches: [staging, main]\n",
                "    branches:\n      --staging\n      --main\n",
                "fires on a push to [\"--main\", \"--staging\"]",
            ),
        ]);
        // **And the block list, which is the same two branches.**
        let listed = bounded_after("listed", release, |text| {
            once(
                text,
                "    branches: [staging, main]\n",
                "    branches:\n      - main\n      - staging\n",
            )
        });
        assert!(listed.is_empty(), "{listed:#?}");
    }

    /// The release workflow holds one job, on Linux, in its branch's
    /// environment.
    #[test]
    fn the_release_is_one_job_on_linux_in_its_branchs_environment() {
        let release = super::RELEASE_WORKFLOW;
        let environment = "    environment: release-${{ github.ref_name }}\n";
        each_is_said(&[
            (
                "a-fixed-environment",
                release,
                environment,
                "    environment: release-main\n",
                "the release job enters `release-main`",
            ),
            (
                "no-environment",
                release,
                environment,
                "",
                "the release job enters no environment",
            ),
            (
                "on-macos",
                release,
                "    runs-on: ubuntu-latest\n",
                "    runs-on: macos-latest\n",
                "runs on `macos-latest`, which is not a Linux runner",
            ),
            (
                "a-second-job",
                release,
                "jobs:\n",
                "jobs:\n  notes:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo notes\n",
                "holds the job `notes` beside `release`",
            ),
            (
                "renamed",
                release,
                "  release:\n",
                "  publish:\n",
                "the release workflow has no `release` job",
            ),
        ]);
    }

    /// The release restores no cache, through either action or the half that
    /// only restores.
    ///
    /// **Each sentence built from the action's name**, because a name with a
    /// slash written between backticks is a citation of a path to the reference
    /// gate, on any line of a Rust file.
    #[test]
    fn the_release_restores_no_cache() {
        let checked_out = "          persist-credentials: false\n";
        for (about, action) in [
            ("rust-cache", "Swatinem/rust-cache"),
            ("restore-only", "actions/cache/restore"),
        ] {
            let findings = bounded_after(about, super::RELEASE_WORKFLOW, |text| {
                once(
                    text,
                    checked_out,
                    &format!(
                        "{checked_out}      - uses: {action}@0123456789abcdef0123456789abcdef01234567 # v1\n"
                    ),
                )
            });
            let said = format!("the release restores a cache, through `{action}`");
            assert!(
                findings.iter().any(|finding| finding.what == said),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// The release job runs its steps, in order, and nothing else.
    #[test]
    fn the_release_job_runs_its_steps_and_nothing_else() {
        let release = super::RELEASE_WORKFLOW;
        let the_release =
            "        run: cargo xtask release --channel ${{ github.ref_name }} --publish\n";
        each_is_said(&[
            (
                "a-second-command",
                release,
                "        run: cargo xtask dev\n",
                "        run: cargo xtask dev && cargo xtask check\n",
                "step 5 is `run: cargo xtask dev && cargo xtask check`, where it is \
                 `run: cargo xtask dev`",
            ),
            (
                "a-script",
                release,
                "      - name: cargo xtask release\n",
                "      - run: bash ci/release.sh\n      - name: cargo xtask release\n",
                "step 6 is `run: bash ci/release.sh`",
            ),
            (
                "no-stack",
                release,
                "      - name: cargo xtask dev\n        run: cargo xtask dev\n",
                "",
                "step 5 is `run: cargo xtask release --channel ${{ github.ref_name }} \
                 --publish`, where it is `run: cargo xtask dev`",
            ),
            (
                "a-dry-run",
                release,
                the_release,
                "        run: cargo xtask release --channel ${{ github.ref_name }} --dry-run\n",
                "step 6 is `run: cargo xtask release --channel ${{ github.ref_name }} \
                 --dry-run`",
            ),
            (
                "a-fixed-channel",
                release,
                the_release,
                "        run: cargo xtask release --channel main --publish\n",
                "step 6 is `run: cargo xtask release --channel main --publish`",
            ),
            (
                "one-step-more",
                release,
                "          GH_TOKEN: ${{ steps.app.outputs.token }}\n",
                "          GH_TOKEN: ${{ steps.app.outputs.token }}\n      - run: echo done\n",
                "step 7 is `run: echo done`, where it is nothing at all",
            ),
            (
                "a-condition",
                release,
                the_release,
                "        if: always()\n        run: cargo xtask release --channel ${{ github.ref_name }} --publish\n",
                "this step or job runs only when `always()`",
            ),
        ]);
    }

    /// Each step of the release job holds the lines `THE_RELEASE_STEPS` writes
    /// for it, and no other.
    ///
    /// **An input is part of what an action does**: another repository checked
    /// out, a token asked for more, a credential left in the checkout, and a key
    /// or a continued value on a command are each a step that is not the one
    /// written down.
    #[test]
    fn each_release_step_holds_its_own_lines_and_no_other() {
        let release = super::RELEASE_WORKFLOW;
        let checked_out = "          persist-credentials: false\n";
        let narrowed =
            "          permission-contents: write\n          permission-pull-requests: write\n";
        let the_stack = "        run: cargo xtask dev\n";
        each_is_said(&[
            (
                "another-repository",
                release,
                checked_out,
                "          persist-credentials: false\n          repository: someone-else/noctua-central\n",
                "the release job's step 2 holds `repository: someone-else/noctua-central` \
                 under `with:`, where it holds nothing more",
            ),
            (
                "another-ref",
                release,
                "          fetch-depth: 0\n",
                "          ref: main\n          fetch-depth: 0\n",
                "the release job's step 2 holds `ref: main` under `with:`, where it holds \
                 `fetch-depth: 0` under `with:`",
            ),
            (
                "a-credential-left",
                release,
                checked_out,
                "",
                "the release job's step 2 holds nothing more, where it holds \
                 `persist-credentials: false` under `with:`",
            ),
            (
                "a-token-asked-for-more",
                release,
                narrowed,
                "          permission-administration: write\n",
                "the release job's step 1 holds `permission-administration: write` under \
                 `with:`, where it holds `permission-contents: write` under `with:`",
            ),
            (
                "an-input-moved-out",
                release,
                "          fetch-depth: 0\n",
                "        fetch-depth: 0\n",
                "the release job's step 2 holds `fetch-depth: 0`, where it holds \
                 `fetch-depth: 0` under `with:`",
            ),
            (
                "a-directory",
                release,
                the_stack,
                "        run: cargo xtask dev\n        working-directory: plugins\n",
                "the release job's step 5 holds `working-directory: plugins`, where it holds \
                 nothing more",
            ),
            (
                "a-continued-command",
                release,
                the_stack,
                "        run: cargo xtask dev\n          && cargo xtask check\n",
                "the release job's step 5 holds `&& cargo xtask check` under \
                 `run: cargo xtask dev`, where it holds nothing more",
            ),
            (
                "indented-less",
                release,
                the_stack,
                "       run: cargo xtask dev\n",
                "the release job's step 5 holds `(indented less than the step) run: cargo xtask \
                 dev`, where it holds `run: cargo xtask dev`",
            ),
        ]);
        // **And the label is not held**: a step renamed is the same step.
        let renamed = bounded_after("renamed-step", release, |text| {
            once(
                text,
                "      - name: cargo xtask dev\n",
                "      - name: the stack\n",
            )
        });
        assert!(renamed.is_empty(), "{renamed:#?}");
    }

    /// The App's key reaches the step that mints the token, and the token the
    /// release step alone.
    ///
    /// **However the expression names it**: by its path, by an index, as the
    /// whole of a context, in a comment, behind an escape, or across a line
    /// break a backslash escapes. Each case is a line the release does not need,
    /// and each is said by the line it is.
    #[test]
    fn the_token_reaches_the_release_step_alone() {
        let release = super::RELEASE_WORKFLOW;
        let handed = "        env:\n          GH_TOKEN: ${{ steps.app.outputs.token }}\n";
        let the_stack = "        run: cargo xtask dev\n";
        let beside_the_stack = |line: &str| format!("{the_stack}        env:\n          {line}\n");
        let said = |line: &str| {
            format!(
                "the release workflow evaluates `{line}`, which is not one of `THE_EXPRESSIONS`"
            )
        };
        let by_index = "LEAK: ${{ steps['app'].outputs.token }}";
        let every_output = "ALL: ${{ toJSON(steps) }}";
        let the_workflows_by_index = "OWN: ${{ github['token'] }}";
        let the_whole_context = "OWN: ${{ toJSON(github) }}";
        let the_key = "KEY: ${{ secrets.RELEASE_APP_KEY }}";
        let cases: Vec<(&str, String, String, String)> = vec![
            (
                "at-the-job",
                "    permissions:\n".to_owned(),
                "    env:\n      GH_TOKEN: ${{ steps.app.outputs.token }}\n    permissions:\n"
                    .to_owned(),
                said(super::THE_TOKEN_HANDED),
            ),
            (
                "to-the-stack",
                the_stack.to_owned(),
                beside_the_stack(super::THE_TOKEN_HANDED),
                said(super::THE_TOKEN_HANDED),
            ),
            (
                "the-key-elsewhere",
                the_stack.to_owned(),
                beside_the_stack(the_key),
                said(the_key),
            ),
            (
                "the-workflows-own",
                handed.to_owned(),
                "        env:\n          GH_TOKEN: ${{ github.token }}\n".to_owned(),
                said("GH_TOKEN: ${{ github.token }}"),
            ),
            (
                "a-second-name",
                handed.to_owned(),
                format!("{handed}          GITHUB_TOKEN: ${{{{ steps.app.outputs.token }}}}\n"),
                said("GITHUB_TOKEN: ${{ steps.app.outputs.token }}"),
            ),
            (
                "by-index",
                the_stack.to_owned(),
                beside_the_stack(by_index),
                said(by_index),
            ),
            (
                "every-output",
                the_stack.to_owned(),
                beside_the_stack(every_output),
                said(every_output),
            ),
            (
                "the-workflows-own-by-index",
                the_stack.to_owned(),
                beside_the_stack(the_workflows_by_index),
                said(the_workflows_by_index),
            ),
            (
                "the-whole-context",
                the_stack.to_owned(),
                beside_the_stack(the_whole_context),
                said(the_whole_context),
            ),
            (
                "in-a-comment",
                the_stack.to_owned(),
                format!("{the_stack}        # ${{{{ toJSON(steps) }}}}\n"),
                said("# ${{ toJSON(steps) }}"),
            ),
            (
                "behind-an-escape",
                "      - name: cargo xtask dev\n".to_owned(),
                "      - name: \"\\x24{{ toJSON(steps) }}\"\n".to_owned(),
                said("name: \"\\x24{{ toJSON(steps) }}\""),
            ),
            (
                "across-a-line",
                "      - name: cargo xtask dev\n".to_owned(),
                "      - name: \"$\\\n          {{ toJSON(steps) }}\"\n".to_owned(),
                said("name: \"${{ toJSON(steps) }}\""),
            ),
        ];
        for (about, from, to, said) in &cases {
            let findings = bounded_after(about, release, |text| once(text, from, to));
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.starts_with(said.as_str())),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// No file CI reaches but the release workflow mints the App's token,
    /// enters an environment, reads a secret or a step's output, or publishes.
    ///
    /// **The release written into `ci.yml` whole**, as it would be to get past
    /// every rule on `release.yml`, then each part of it alone, in the
    /// spellings a reader of keys would miss.
    #[test]
    fn nothing_but_the_release_holds_its_authority() {
        let said =
            |what: &str| format!("`{WORKFLOW}` {what}, which the release workflow alone does");
        let release = std::fs::read_to_string(root().join(super::RELEASE_WORKFLOW))
            .expect("the release workflow is readable");
        let the_job = &release[release.find("  release:\n").expect("the release job")..];
        let whole = bounded_after("released-from-ci", WORKFLOW, |text| {
            format!("{text}{}", the_job.replace("  release:\n", "  publish:\n"))
        });
        for what in [
            "mints the release App's token",
            "enters an environment",
            "reads a secret",
            "reads a step's output",
            "publishes a release",
        ] {
            assert!(
                whole.iter().any(|finding| finding.what == said(what)),
                "the release written into the gate's workflow: nothing said `{what}`: {whole:#?}"
            );
        }
    }

    /// Runs each case through [`bounded_after`] on the gate's workflow, the
    /// line that runs the gate replaced by the case's text, and asserts the
    /// workflow is said to do what the case names.
    #[track_caller]
    fn the_gates_workflow_is_said_to(cases: &[(&str, String, &str)]) {
        for (about, to, what) in cases {
            let said = format!("`{WORKFLOW}` {what}, which the release workflow alone does");
            let findings = bounded_after(about, WORKFLOW, |text| once(text, THE_GATE, to));
            assert!(
                findings.iter().any(|finding| finding.what == said),
                "{about}: nothing said `{what}`: {findings:#?}"
            );
        }
    }

    /// The line of the gate's workflow that runs the gate, after which each
    /// case of the release's authority is written.
    const THE_GATE: &str = "        run: cargo xtask check\n";

    /// The token's action, an environment and a secret are refused outside the
    /// release workflow, in the spellings a reader of keys would miss.
    ///
    /// **A quoted key, a flow mapping, an explicit key after `?`, an index, a
    /// whole context, an escape, a line a backslash continues, and a comment
    /// inside a block scalar.**
    #[test]
    fn each_part_of_the_releases_authority_is_refused_however_it_is_spelled() {
        let gate = THE_GATE;
        let env = |line: &str| format!("{gate}        env:\n          {line}\n");
        the_gates_workflow_is_said_to(&[
            (
                "the-token",
                format!(
                    "{gate}      - uses: {}@64ab729a6294000f5aa4a4bc0b73d1f32bc45510 # v3.3.0\n",
                    super::MINTS_THE_RELEASE_TOKEN
                ),
                "mints the release App's token",
            ),
            (
                "an-environment",
                format!("{gate}    environment: release-main\n"),
                "enters an environment",
            ),
            (
                "a-quoted-environment",
                format!("{gate}    \"environment\": release-main\n"),
                "enters an environment",
            ),
            (
                "a-flow-environment",
                format!("{gate}  other: {{ runs-on: ubuntu-latest, environment: release-main }}\n"),
                "enters an environment",
            ),
            (
                "an-explicit-environment",
                format!("{gate}    ? environment\n    : release-main\n"),
                "enters an environment",
            ),
            (
                "an-explicit-quoted-environment-in-a-flow-mapping",
                format!("{gate}  other: {{ ? \"environment\" : release-main }}\n"),
                "enters an environment",
            ),
            (
                "a-secret",
                env("KEY: ${{ secrets.RELEASE_APP_KEY }}"),
                "reads a secret",
            ),
            (
                "a-secret-by-index",
                env("KEY: ${{ secrets['RELEASE_APP_KEY'] }}"),
                "reads a secret",
            ),
            (
                "every-secret",
                env("ALL: ${{ toJSON(secrets) }}"),
                "reads a secret",
            ),
            (
                "inherited",
                format!("{gate}    secrets: inherit\n"),
                "reads a secret",
            ),
            (
                "a-secret-behind-an-escape",
                env("KEY: \"${{ s\\x65crets.RELEASE_APP_KEY }}\""),
                "reads a secret",
            ),
            (
                "a-secret-across-a-line",
                env("KEY: \"${{ secr\\\n            ets.RELEASE_APP_KEY }}\""),
                "reads a secret",
            ),
            (
                "a-secret-in-a-comment",
                "        run: |\n          # ${{ toJSON(secrets) }}\n          cargo xtask check\n"
                    .to_owned(),
                "reads a secret",
            ),
        ]);
    }

    /// A grant after a verbatim tag is a grant, and a backslash at the end of a
    /// comment or of a plain scalar continues nothing, so a grant or the
    /// release's authority on the line after it is refused.
    ///
    /// **A verbatim tag runs to its `>`**: read as words split at a comma,
    /// `!<tag:yaml.org,2002:str>` hid the key after it. **And each line is
    /// read on its own as well as joined.** Read only joined to the line above,
    /// a grant, an environment, a secret, the token's action and a publish
    /// below `# \` or `name: gate \` were each part of a comment or of a name,
    /// and every one passed.
    #[test]
    fn what_follows_a_verbatim_tag_or_a_backslash_that_continues_nothing_is_read() {
        let macos = "    runs-on: macos-latest\n    permissions:\n      contents: read\n";
        each_is_said(&[
            (
                "a-grant-after-a-verbatim-tag",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest\n    !<tag:yaml.org,2002:str> permissions: write-all\n",
                "asks for `write-all`",
            ),
            (
                "a-grant-after-a-plain-scalar",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest\n    name: gate \\\n    permissions: write-all\n",
                "asks for `write-all`",
            ),
            (
                "a-grant-after-a-comment",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest # \\\n    permissions: write-all\n",
                "asks for `write-all`",
            ),
        ]);
        let gate = THE_GATE;
        the_gates_workflow_is_said_to(&[
            (
                "an-environment-after-a-comment",
                format!("{gate}    timeout-minutes: 60 # \\\n    environment: release-main\n"),
                "enters an environment",
            ),
            (
                "an-environment-after-a-plain-scalar",
                format!("{gate}    name: gate \\\n    environment: release-main\n"),
                "enters an environment",
            ),
            (
                "a-secret-after-a-plain-scalar",
                format!("{gate}    name: gate \\\n    secrets: inherit\n"),
                "reads a secret",
            ),
            (
                "the-token-after-a-name",
                format!(
                    "{gate}      - name: lint \\\n        uses: {}@64ab729a6294000f5aa4a4bc0b73d1f32bc45510 # v3.3.0\n",
                    super::MINTS_THE_RELEASE_TOKEN
                ),
                "mints the release App's token",
            ),
            (
                "a-publish-after-a-comment",
                format!(
                    "{gate}      # \\\n      - run: cargo xtask release --channel main --publish\n"
                ),
                "publishes a release",
            ),
            (
                "a-publish-joined-after-a-comment",
                "        run: |\n          # \\\n          cargo xtask release --channel main --pub\\\n          \
                 lish\n"
                    .to_owned(),
                "publishes a release",
            ),
        ]);
    }

    /// A step's output and a publish are refused outside the release workflow,
    /// however they are spelled: by an index, as a whole context, in a comment
    /// inside a block scalar, and in a word a shell joins.
    #[test]
    fn a_steps_output_and_a_publish_are_refused_however_they_are_spelled() {
        let gate = THE_GATE;
        let env = |line: &str| format!("{gate}        env:\n          {line}\n");
        the_gates_workflow_is_said_to(&[
            (
                "an-output",
                env("TOKEN: ${{ steps.app.outputs.token }}"),
                "reads a step's output",
            ),
            (
                "an-output-by-index",
                env("TOKEN: ${{ steps['app'].outputs.token }}"),
                "reads a step's output",
            ),
            (
                "every-output",
                env("ALL: ${{ toJSON(steps) }}"),
                "reads a step's output",
            ),
            (
                "an-output-in-a-comment",
                "        run: |\n          # ${{ toJSON(steps) }}\n          cargo xtask check\n"
                    .to_owned(),
                "reads a step's output",
            ),
            (
                "published",
                "        run: cargo xtask release --channel main --publish\n".to_owned(),
                "publishes a release",
            ),
            (
                "published-in-parts",
                "        run: just release main --pub''lish\n".to_owned(),
                "publishes a release",
            ),
        ]);
    }

    /// What a step reaches holds none of the release's authority: a composite
    /// action, which GitHub runs as YAML, and a script, which publishes as
    /// surely as a workflow does.
    #[test]
    fn what_a_step_reaches_holds_none_of_the_releases_authority() {
        let gate = THE_GATE;
        let held = crate::check::LeavesNothingBehind::new("bounded-authority-reached");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        std::fs::create_dir_all(scratch.join("ci/actions/mint")).expect("writable");
        for shipped in super::THE_WORKFLOWS {
            std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("readable");
        }
        let ci = std::fs::read_to_string(scratch.join(WORKFLOW)).expect("copied");
        let ci = once(
            &ci,
            gate,
            &format!("{gate}      - uses: ./ci/actions/mint\n      - run: bash ci/publish.sh\n"),
        );
        std::fs::write(scratch.join(WORKFLOW), ci).expect("writable");
        let action = "ci/actions/mint/action.yml";
        let script = "ci/publish.sh";
        std::fs::write(
            scratch.join(action),
            format!(
                "runs:\n  using: composite\n  steps:\n    - uses: {}@64ab729a6294000f5aa4a4bc0b73d1f32bc45510 # v3.3.0\n",
                super::MINTS_THE_RELEASE_TOKEN
            ),
        )
        .expect("writable");
        std::fs::write(
            scratch.join(script),
            "cargo xtask release --channel main --publish\n",
        )
        .expect("writable");
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        for (file, what) in [
            (action, "mints the release App's token"),
            (script, "publishes a release"),
        ] {
            assert!(
                findings.iter().any(|finding| finding.file == file
                    && finding
                        .what
                        .ends_with(&format!("{what}, which the release workflow alone does"))),
                "{file}: nothing said `{what}`: {findings:#?}"
            );
        }
    }

    /// The release workflow and its job hold the keys their lists name, each
    /// once, and no other.
    #[test]
    fn the_release_holds_its_keys_and_no_other() {
        let release = super::RELEASE_WORKFLOW;
        let at_the_top = |key: &str| {
            format!(
                "the release workflow holds `{key}:`, and it holds {:?} alone, each once",
                super::THE_RELEASE_WORKFLOWS_KEYS
            )
        };
        let in_the_job = |key: &str| {
            format!(
                "the release job holds `{key}:`, and it holds {:?} alone, each once",
                super::THE_RELEASE_JOBS_KEYS
            )
        };
        let runs_on = "    runs-on: ubuntu-latest\n";
        let jobs = "jobs:\n";
        let cases: Vec<(&str, String, String)> = vec![
            (
                "an-environment-for-every-step",
                format!("env:\n  RUSTC_WRAPPER: ./wrap.sh\n{jobs}"),
                at_the_top("env"),
            ),
            (
                "a-default-for-every-step",
                format!("defaults:\n  run:\n    working-directory: plugins\n{jobs}"),
                at_the_top("defaults"),
            ),
            (
                "a-second-trigger",
                format!("on:\n  workflow_dispatch:\n{jobs}"),
                at_the_top("on"),
            ),
            (
                "a-container",
                format!("{runs_on}    container: example/any-image:latest\n"),
                in_the_job("container"),
            ),
            (
                "a-quoted-container",
                format!("{runs_on}    \"container\": example/any-image:latest\n"),
                in_the_job("\"container\""),
            ),
            (
                "a-service",
                format!("{runs_on}    services:\n      any:\n        image: example/any-image\n"),
                in_the_job("services"),
            ),
            (
                "a-matrix",
                format!("{runs_on}    strategy:\n      matrix:\n        n: [1, 2]\n"),
                in_the_job("strategy"),
            ),
            (
                "an-environment-for-the-job",
                format!("{runs_on}    env:\n      CARGO_HOME: /tmp/cargo\n"),
                in_the_job("env"),
            ),
            (
                "a-second-runner",
                format!("{runs_on}{runs_on}"),
                in_the_job("runs-on"),
            ),
        ];
        for (about, to, said) in &cases {
            let from = if to.starts_with(runs_on) {
                runs_on
            } else {
                jobs
            };
            let findings = bounded_after(about, release, |text| once(text, from, to));
            assert!(
                findings.iter().any(|finding| &finding.what == said),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// A file GitHub runs writes no key after `?`, as an alias or as `<<`, and
    /// leaves no flow collection open past its line, in either workflow; and
    /// a shell's `*)`, `<<EOF`, `${x:?y}` and braces in a step's command are
    /// none of those.
    ///
    /// **Each passes every rule that reads a key by its word.** A job of
    /// `ci.yml` holding `? permissions` with `: write-all` on the line below
    /// holds a token that writes, and `*e :` with `&e environment` elsewhere
    /// enters an environment, with no key `permissions` or `environment` on
    /// either line.
    #[test]
    fn a_key_written_after_a_question_mark_as_an_alias_or_merged_in_is_refused() {
        let gate = THE_GATE;
        let runs_on = "    runs-on: ubuntu-latest\n";
        let cases: Vec<(&str, &str, &str, String, &str)> = vec![
            (
                "an-explicit-grant",
                WORKFLOW,
                gate,
                format!("{gate}    ? permissions\n    : write-all\n"),
                "writes a key after `?`",
            ),
            (
                "an-explicit-key-in-a-flow-mapping",
                WORKFLOW,
                gate,
                format!("{gate}  extra: {{ ? runs-on : ubuntu-latest }}\n"),
                "writes a key after `?`",
            ),
            (
                "an-explicit-key-in-a-sequence",
                WORKFLOW,
                gate,
                format!("{gate}      - ? uses\n        : owner/a@v1\n"),
                "writes a key after `?`",
            ),
            (
                "an-alias-for-a-key",
                WORKFLOW,
                gate,
                format!("{gate}    *e : release-main\n"),
                "writes a key as an alias",
            ),
            (
                "an-alias-for-a-key-in-a-flow-mapping",
                WORKFLOW,
                gate,
                format!("{gate}  extra: {{ runs-on: ubuntu-latest, *e : release-main }}\n"),
                "writes a key as an alias",
            ),
            (
                "a-merge",
                WORKFLOW,
                gate,
                format!("{gate}    <<: *job\n"),
                "writes a key as `<<`",
            ),
            (
                "an-explicit-key-in-the-release",
                super::RELEASE_WORKFLOW,
                runs_on,
                format!("{runs_on}    ? container\n    : example/any-image\n"),
                "writes a key after `?`",
            ),
            (
                "a-flow-mapping-over-lines",
                WORKFLOW,
                "jobs:\n",
                "jobs:\n  extra: { runs-on: ubuntu-latest, permissions\n    : write-all }\n"
                    .to_owned(),
                "leaves a flow collection open past its line",
            ),
        ];
        for (about, file, from, to, said) in &cases {
            let findings = bounded_after(about, file, |text| once(text, from, to));
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.starts_with(&format!("`{file}` {said}"))),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }

        // **A shell's own words are not keys**, and a rule that read them as
        // keys would refuse a step for its command.
        let shell = "        run: |\n          case \"$x\" in\n          *) cargo xtask check ;;\n          \
                     esac\n          cat <<EOF\n          EOF\n          echo \"${x:?unset}\" ? done\n          \
                     f() {\n            cargo xtask check\n          }\n";
        let findings = bounded_after("a-shell-in-a-step", WORKFLOW, |text| {
            once(text, gate, shell)
        });
        assert!(
            findings.is_empty(),
            "a shell's `*)`, `<<EOF`, `?` and braces were read as YAML: {findings:#?}"
        );
    }

    /// Every `uses` key on a line is read, wherever YAML opens one, and one
    /// whose value is not on its line is read as none.
    ///
    /// **Row by row, both directions**: the positions a key stands in, and the
    /// words that only look like one.
    #[test]
    fn every_uses_key_on_a_line_is_read() {
        let read = |line: &str| super::the_actions_used(line);
        let one = |action: &str| vec![Some(action.to_owned())];
        for (line, expected) in [
            ("- uses: a/b@c", one("a/b@c")),
            ("uses: a/b@c # v1", one("a/b@c")),
            ("- { name: lint, uses: a/b@c }", one("a/b@c")),
            ("- {name: lint,uses: a/b@c}", one("a/b@c")),
            ("- \"uses\": a/b@c", one("a/b@c")),
            ("- 'uses': a/b@c", one("a/b@c")),
            ("- \"us\\x65s\": a/b@c", one("a/b@c")),
            ("- USES: a/b@c", one("a/b@c")),
            ("- uses: \"a/b@c\"", one("a/b@c")),
            ("- - uses: a/b@c", one("a/b@c")),
            ("- &a uses: a/b@c", one("a/b@c")),
            ("- !!str \"uses\": a/b@c", one("a/b@c")),
            ("- { !!str uses: a/b@c }", one("a/b@c")),
            ("- !<tag:yaml.org,2002:str> uses: a/b@c", one("a/b@c")),
            ("- { !<tag:yaml.org,2002:str> uses: a/b@c }", one("a/b@c")),
            ("- &a !<tag:x,[y]> \"uses\": a/b@c", one("a/b@c")),
            ("- &a!<x> uses: a/b@c", one("a/b@c")),
            ("- ? uses : a/b@c", one("a/b@c")),
            (
                "steps: [ { uses: a/b@c }, { uses: d/e@f, with: { x: 1 } } ]",
                vec![Some("a/b@c".to_owned()), Some("d/e@f".to_owned())],
            ),
            ("- uses:", vec![None]),
            ("- uses: |", vec![None]),
            ("- uses: &a a/b@c", vec![None]),
            ("- uses: !!str a/b@c", vec![None]),
            ("- uses: *a", vec![None]),
            ("- uses: \"a/b@c", vec![None]),
            ("- { uses: }", vec![None]),
            ("run: echo uses: a/b@c", vec![]),
            ("- name: reuses: a/b@c", vec![]),
            ("- uses-it: a/b@c", vec![]),
            ("- { uses:a/b@c }", vec![]),
            ("- ?uses: a/b@c", vec![]),
            ("- a!<tag:x,y> uses: a/b@c", vec![]),
            ("- !<tag:x y> uses: a/b@c", vec![]),
            ("# uses: a/b@c", vec![]),
        ] {
            assert_eq!(read(line), expected, "`{line}`");
        }
    }

    /// A line of a file CI reaches ends at a line feed, and at no other
    /// character a YAML parser ends one at.
    ///
    /// **Each case carries a key or a step past every reader**, after a
    /// carriage return, NEL, LS or PS on a comment or a step's name: a step
    /// before the release's, a trigger, an input of the release step, an
    /// action off the list and a grant. Each passed the whole gate while every
    /// reader ended a line at a line feed and nothing refused the others. A
    /// carriage return before a line feed is part of it, and the justfile is
    /// read as the workflows are.
    #[test]
    fn a_line_ends_at_a_line_feed_and_nowhere_else() {
        let release = super::RELEASE_WORKFLOW;
        let return_ = "ends a line at a carriage return";
        let named = "      - name: cargo xtask check\n";
        let macos = "    runs-on: macos-latest\n";
        each_is_said(&[
            (
                "a-step-after-a-comment",
                release,
                "brings it up before its own.\n",
                "brings it up before its own.\r      - run: echo /tmp/evil >> \"$GITHUB_PATH\"\n",
                return_,
            ),
            (
                "a-step-after-a-name",
                release,
                "      - name: cargo xtask dev\n",
                "      - name: cargo xtask dev\r      - run: echo /tmp/evil >> \"$GITHUB_PATH\"\n",
                return_,
            ),
            (
                "a-trigger-after-a-comment",
                release,
                "on:\n",
                "on:\n  # also by hand\r  workflow_dispatch:\n",
                return_,
            ),
            (
                "an-input-after-a-name",
                release,
                "      - name: cargo xtask release\n",
                "      - name: cargo xtask release\u{85}        working-directory: /tmp/evil\n",
                "ends a line at U+0085 (NEL)",
            ),
            (
                "an-action-after-a-name",
                WORKFLOW,
                named,
                "      - name: lint\r        uses: owner/a@v1\n      - name: cargo xtask check\n",
                return_,
            ),
            (
                "an-action-after-a-line-separator",
                WORKFLOW,
                named,
                "      - name: lint\u{2028}        uses: owner/a@v1\n      - name: cargo xtask check\n",
                "ends a line at U+2028 (LS)",
            ),
            (
                "a-grant-after-a-carriage-return",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest\r    permissions: write-all\n",
                return_,
            ),
            (
                "a-grant-after-a-line-separator",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest\u{2028}    permissions: write-all\n",
                "ends a line at U+2028 (LS)",
            ),
            (
                "a-grant-after-a-paragraph-separator",
                WORKFLOW,
                macos,
                "    runs-on: macos-latest\u{2029}    permissions: write-all\n",
                "ends a line at U+2029 (PS)",
            ),
        ]);

        let crlf = bounded_after("crlf", WORKFLOW, |text| text.replace('\n', "\r\n"));
        assert!(
            !crlf
                .iter()
                .any(|finding| finding.what.contains("ends a line at")),
            "a carriage return before a line feed is part of it: {crlf:#?}"
        );

        let held = crate::check::LeavesNothingBehind::new("bounded-justfile-breaks");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        for shipped in super::THE_WORKFLOWS {
            std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("readable");
        }
        std::fs::write(
            scratch.join("justfile"),
            "check:\n    # the gate\r    cargo clippy --workspace\n    cargo xtask check\n",
        )
        .expect("writable");
        let (findings, _) = super::every_workflow_is_bounded(&scratch);
        assert!(
            findings
                .iter()
                .any(|finding| finding.file == "justfile" && finding.what.contains(return_)),
            "a carriage return in the justfile: {findings:#?}"
        );
    }

    /// A file GitHub runs closes each quoted scalar on the line that opens it,
    /// and a quote inside a flow collection holds its brackets.
    ///
    /// **A quoted scalar left open runs on over the lines below**, so a key a
    /// reader of lines finds there is text to GitHub. With `name: "check` at
    /// the top of `ci.yml`, `permissions: {}` below it and the quote closed on
    /// the line after, the workflow opened with nothing at its top and nothing
    /// was said. **And a bracket inside a quote is text**: counted, `name: "}"`
    /// closed the flow mapping it stands in, and the rest of the mapping, on
    /// the line below, was read by no rule.
    #[test]
    fn a_quoted_scalar_closes_on_its_line_and_holds_its_brackets() {
        let top = "permissions: {}\n";
        let quoted = "leaves a quoted scalar open past its line";
        each_is_said(&[
            (
                "a-double-quote-over-lines",
                WORKFLOW,
                top,
                "name: \"check\npermissions: {}\n\"\n",
                quoted,
            ),
            (
                "a-single-quote-over-lines",
                WORKFLOW,
                top,
                "name: 'check\npermissions: {}\n'\n",
                quoted,
            ),
            (
                "a-double-quote-continued-by-a-backslash",
                WORKFLOW,
                top,
                "name: \"check \\\npermissions: {}\"\n",
                quoted,
            ),
            (
                "a-quoted-bracket-in-a-flow-mapping",
                WORKFLOW,
                "jobs:\n",
                "jobs:\n  extra: { runs-on: ubuntu-latest, name: \"}\", permissions\n    : write-all }\n",
                "leaves a flow collection open past its line",
            ),
        ]);
    }

    /// A file GitHub runs holds no `#` the readers here end a line at where YAML
    /// reads on, and a line YAML ends sooner than they do is no finding.
    ///
    /// **A quote YAML takes for a letter of a key opens a span to a shell's
    /// reading**, so the `#` inside the quoted value after it ended the line
    /// for every rule, and a grant, an action, a second cargo command and an
    /// environment after it each passed with nothing said; `yq` reads each as
    /// a key.
    #[test]
    fn a_hash_the_readers_end_a_line_at_and_yaml_reads_past_is_refused() {
        let gate = THE_GATE;
        let said = "holds a `#` every reader here ends the line at, and YAML reads on past it";
        let cases = [
            (
                "a-grant",
                format!(
                    "{gate}  extra: {{ ke\"y: \"a # b\", runs-on: ubuntu-latest, permissions: write-all }}\n"
                ),
            ),
            (
                "an-action",
                format!("{gate}      - {{ na\"me: \"a # b\", uses: owner/a@v1 }}\n"),
            ),
            (
                "a-second-gate",
                format!("{gate}      - {{ na\"me: \"a # b\", run: cargo clippy --workspace }}\n"),
            ),
            (
                "an-environment",
                format!("{gate}  extra: {{ ke\"y: \"a # b\", environment: release-main }}\n"),
            ),
        ];
        for (about, to) in &cases {
            let findings = bounded_after(about, WORKFLOW, |text| once(text, gate, to));
            assert!(
                findings.iter().any(|finding| finding.what.contains(said)),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
        // **And a `#` an escape spells is no comment**, so the key an escape
        // spells after it is read.
        let escaped = bounded_after("an-escaped-hash", WORKFLOW, |text| {
            once(
                text,
                gate,
                &format!(
                    "{gate}  extra: {{ ke\"y: \"a\\x20\\x23 b\", \"perm\\x69ssions\": write-all }}\n"
                ),
            )
        });
        assert!(
            escaped
                .iter()
                .any(|finding| finding.what.contains("asks for `write-all`")),
            "a grant after an escaped `#`: {escaped:#?}"
        );
        let named = "      - name: cargo xtask check\n";
        for (about, to) in [
            (
                "a-comment-after-an-apostrophe",
                "      - name: the gate's own # note\n",
            ),
            (
                "a-hash-in-a-quoted-value",
                "      - name: \"a # b\" # note\n",
            ),
        ] {
            let findings = bounded_after(about, WORKFLOW, |text| once(text, named, to));
            assert!(findings.is_empty(), "{about}: {findings:#?}");
        }
    }

    /// What one line of YAML leaves open at its end, row by row.
    ///
    /// **Both directions**: a quote where a node starts opens a scalar, and
    /// one inside a plain scalar is a letter of it; a bracket inside a quote
    /// is text, and a `#` there opens no comment; a doubled single quote and
    /// a backslash's quote close nothing.
    #[test]
    fn what_a_line_leaves_open_is_read_as_yaml_reads_it() {
        let double = Some('"');
        let single = Some('\'');
        for (line, expected) in [
            ("name: \"check", (0, double)),
            ("name: 'check", (0, single)),
            ("name: \"check \\", (0, double)),
            ("- \"a", (0, double)),
            ("- ? \"a", (0, double)),
            ("x: &a \"b", (0, double)),
            ("x: !!str \"b", (0, double)),
            ("x: !<tag:yaml.org,2002:str> 'b", (0, single)),
            ("x: \"a # b", (0, double)),
            ("x: [ \"a, b", (1, double)),
            ("name: \"a \\\" b", (0, double)),
            ("{ key:\"x", (1, double)),
            ("x: { name: \"}\", permissions", (1, None)),
            ("x: { a: \"b # c\", d", (1, None)),
            ("x: {\"a\":\"}\"", (1, None)),
            ("x: { name: 'it''s {', run: x }", (0, None)),
            ("name: \"a \\\"b\"", (0, None)),
            ("name: \"a\\\\\"", (0, None)),
            ("name: 'it''s'", (0, None)),
            ("name: it's fine", (0, None)),
            ("run: echo \"a", (0, None)),
            ("key:\"x", (0, None)),
            ("x: a # \"b", (0, None)),
            ("x: a#\"b", (0, None)),
            ("x: { a: b } # {", (0, None)),
        ] {
            assert_eq!(super::what_a_line_leaves_open(line), expected, "`{line}`");
        }
    }

    /// A condition, a shell of a step's own and a failure that is not the
    /// job's are each refused on the step that runs the gate, wherever YAML
    /// opens their key.
    ///
    /// **Escaped, below its key, after an anchor, in a flow mapping, and
    /// quoted.** Each passed with nothing said while `if:` was read where a
    /// line opened on it and `shell:` and `continue-on-error` as words of a
    /// line with its quoted text spent; the gate step skipped or its failure
    /// ignored, in CI, with the walk's count unmoved.
    #[test]
    fn a_step_is_neither_skipped_nor_neutered_however_its_key_is_written() {
        let gate = "      - name: cargo xtask check\n        run: cargo xtask check\n";
        let run = |about: &str, to: &str| {
            let held = crate::check::LeavesNothingBehind::new(&format!("neutered-{about}"));
            let scratch = held.root.clone();
            std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
            for shipped in super::THE_WORKFLOWS {
                std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("readable");
            }
            the_shipped_alias(&scratch);
            let ci = std::fs::read_to_string(scratch.join(WORKFLOW)).expect("copied");
            std::fs::write(scratch.join(WORKFLOW), once(&ci, gate, to)).expect("writable");
            only_runs_the_gate(&scratch).expect("readable").0
        };
        let named = "        name: cargo xtask check\n        run: cargo xtask check\n";
        let cases = [
            (
                "an-escaped-condition",
                format!("      - \"i\\x66\": false\n{named}"),
                "runs only when `false`",
            ),
            (
                "a-condition-below-its-key",
                format!("      - if:\n          false\n{named}"),
                "runs only when `false",
            ),
            (
                "an-anchored-condition",
                format!("      - &c if: false\n{named}"),
                "runs only when `false`",
            ),
            (
                "a-condition-in-a-flow-mapping",
                format!("{gate}      - {{ if: false, name: lint, run: cargo xtask check }}\n"),
                "runs only when `false`",
            ),
            (
                "a-quoted-continue-on-error",
                format!("{gate}        \"continue-on-error\": true\n"),
                "is marked `continue-on-error`",
            ),
            (
                "a-quoted-shell",
                format!("{gate}        \"shell\": bash {{0}}\n"),
                "chooses its own shell",
            ),
            (
                "an-escaped-shell",
                format!("{gate}        \"sh\\x65ll\": bash {{0}}\n"),
                "chooses its own shell",
            ),
        ];
        for (about, to, said) in &cases {
            let findings = run(about, to);
            assert!(
                findings.iter().any(|finding| finding.what.contains(said)),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
    }

    /// A line's code past its sequence's `-`, however much whitespace follows
    /// it, and a `-` with none after it kept as a letter.
    #[test]
    fn an_entry_is_read_past_its_dash_and_the_whitespace_after_it() {
        for (line, entry) in [
            ("- run: |", "run: |"),
            ("-   run: |", "run: |"),
            ("-\trun: |", "run: |"),
            ("-  -   run: |", "run: |"),
            ("      - name: x", "name: x"),
            ("-main", "-main"),
            ("--main", "--main"),
            ("- --main", "--main"),
            ("-", "-"),
        ] {
            assert_eq!(super::past_its_dashes(line), entry, "`{line}`");
        }
    }

    /// The step that runs the gate in the `macos` job, which each case below
    /// replaces.
    const THE_MACOS_GATE: &str = "      - name: cargo xtask check\n        run: cargo xtask check\n\n  # **The gate on Windows";

    /// A block scalar after a wide `-` ends at its key's own column.
    ///
    /// **Measured past `- ` alone**, the key of `-   run: |` stood two
    /// columns left of where YAML reads it, and the step's keys below it were
    /// read as the scalar's text: a quoted scalar left open there swallowed the
    /// `macos` job's gate step, a flow mapping left open hid what followed it,
    /// and a `#` the readers end a line at was passed over, each with nothing
    /// said. A wide block scalar whose text holds a quote, a brace and a `#`
    /// is still text.
    #[test]
    fn a_block_scalar_after_a_wide_dash_ends_at_its_keys_column() {
        let tail = "\n  # **The gate on Windows";
        let opened = "      -   run: |\n            true\n";
        let swallowed = "            - name: cargo xtask check\n              run: cargo xtask check\n           \"\n";
        let cases = [
            (
                "a-quoted-scalar",
                format!("{opened}          name: \"x\n{swallowed}{tail}"),
                "leaves a quoted scalar open past its line",
            ),
            (
                "a-nested-entry",
                format!(
                    "      -  -   run: |\n               true\n             name: \"x\n{swallowed}{tail}"
                ),
                "leaves a quoted scalar open past its line",
            ),
            (
                "a-flow-mapping",
                format!(
                    "{opened}          env: {{ A\n            : b }}\n      - name: cargo xtask check\n        run: cargo xtask check\n{tail}"
                ),
                "leaves a flow collection open past its line",
            ),
            (
                "a-hash",
                format!(
                    "{opened}          env: {{ ke\"y: \"a # b\", permissions: write-all }}\n      - name: cargo xtask check\n        run: cargo xtask check\n{tail}"
                ),
                "holds a `#` every reader here ends the line at",
            ),
        ];
        for (about, to, said) in &cases {
            let findings = bounded_after(about, WORKFLOW, |text| once(text, THE_MACOS_GATE, to));
            assert!(
                findings.iter().any(|finding| finding.what.contains(said)),
                "{about}: nothing said `{said}`: {findings:#?}"
            );
        }
        let text = format!(
            "      -   name: cargo xtask check\n          run: |\n            echo \"it's {{ # here\"\n            cargo xtask check\n{tail}"
        );
        let findings = bounded_after("a-wide-block", WORKFLOW, |ci| {
            once(ci, THE_MACOS_GATE, &text)
        });
        assert!(findings.is_empty(), "{findings:#?}");
    }

    /// What the walk of cargo commands says of a scratch tree, and how many it
    /// saw.
    ///
    /// The tree holds this repository's workflows as they ship, the `cargo
    /// xtask` alias, the files `extra` names, and a change made to the text
    /// of `ci.yml`, which is the one thing each case is about.
    fn gate_after(
        about: &str,
        extra: &[(&str, &str)],
        change: impl FnOnce(&str) -> String,
    ) -> (Vec<crate::gates::Finding>, usize) {
        let held = crate::check::LeavesNothingBehind::new(&format!("gate-{about}"));
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join(".github/workflows")).expect("writable");
        for shipped in super::THE_WORKFLOWS {
            std::fs::copy(root().join(shipped), scratch.join(shipped)).expect("a shipped workflow");
        }
        the_shipped_alias(&scratch);
        for (file, text) in extra {
            let path = scratch.join(file);
            std::fs::create_dir_all(path.parent().expect("a file in a directory"))
                .expect("writable");
            std::fs::write(path, text).expect("writable");
        }
        let text = std::fs::read_to_string(scratch.join(WORKFLOW)).expect("copied above");
        std::fs::write(scratch.join(WORKFLOW), change(&text)).expect("writable");
        only_runs_the_gate(&scratch).expect("readable")
    }

    /// A second cargo command in `ci.yml` is found however the line around it
    /// is written, and a label is still a label.
    ///
    /// **A wide `-`, a quoted value, a quoted key, a flow mapping, a second
    /// assignment, a comment, an escape, and a label a shell runs past.** Each
    /// ran a second lints gate, in `ci.yml` or in a script a step runs, with
    /// the count unmoved and nothing said.
    #[test]
    fn a_second_gate_is_found_however_the_line_around_it_is_written() {
        let gate = THE_GATE;
        let script = [("scripts/label.sh", "name: ; cargo clippy --workspace\n")];
        let alias = "`C=cargo` runs here";
        let clippy = "`cargo clippy --workspace` runs here";
        let cases = [
            (
                "a-wide-dash",
                "      -   run: C=cargo; $C clippy --workspace\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-quoted-value",
                "      - run: \"C=cargo; $C clippy --workspace\"\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-quoted-key",
                "      - \"run\": C=cargo; $C clippy --workspace\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-flow-mapping",
                "      - { run: C=cargo; $C clippy --workspace }\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-second-assignment",
                "      - run: JOBS=1 C=cargo; $C clippy --workspace\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-comment",
                "      - run: |\n          C=cargo # the tool\n          $C clippy --workspace\n",
                WORKFLOW,
                alias,
            ),
            (
                "an-escaped-command",
                "      - run: \"\\x63argo clippy --workspace\"\n",
                WORKFLOW,
                clippy,
            ),
            (
                "an-escaped-assignment",
                "      - run: \"C\\x3dcargo; $C clippy --workspace\"\n",
                WORKFLOW,
                alias,
            ),
            (
                "a-label-in-a-block",
                "      - name: lint\n        run: |\n          name: ; cargo clippy --workspace\n",
                WORKFLOW,
                clippy,
            ),
            (
                "a-label-in-a-script",
                "      - run: bash scripts/label.sh\n",
                "scripts/label.sh",
                clippy,
            ),
        ];
        for (about, step, file, said) in cases {
            let (findings, _) = gate_after(about, &script, |text| {
                once(text, gate, &format!("{gate}{step}"))
            });
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.file == file && finding.what.contains(said)),
                "{about}: nothing said `{said}` of `{file}`: {findings:#?}"
            );
        }
        // **And a label after a wide `-` is a label**, counted as nothing.
        let (clean, seen) = gate_after("clean", &[], str::to_owned);
        assert!(clean.is_empty(), "{clean:#?}");
        let (wide, counted) = gate_after("a-wide-label", &[], |text| {
            once(
                text,
                THE_MACOS_GATE,
                "      -   name: cargo xtask check\n          run: cargo xtask check\n\n  # **The gate on Windows",
            )
        });
        assert!(wide.is_empty(), "{wide:#?}");
        assert_eq!(
            counted, seen,
            "a step's name after a wide `-` was read as a command"
        );
    }

    /// Every local action a step uses, and every directory a step runs in, is
    /// walked, however the key or the command that names it is written.
    ///
    /// **Read where a line opened on `- uses:`, `working-directory:`, `cd ` or
    /// `- run: cd `**, each spelling here left the action's or the script's
    /// second gate unread, with nothing said.
    #[test]
    fn every_action_and_every_directory_a_step_names_is_walked() {
        let gate = THE_GATE;
        let action = "ci/actions/extra/action.yml";
        let script = "scripts/gate.sh";
        let extra = [
            (
                action,
                "runs:\n  using: composite\n  steps:\n    - run: cargo clippy --workspace\n",
            ),
            (script, "cargo fmt --all\n"),
        ];
        let cases = [
            (
                "an-action-after-a-wide-dash",
                "      -   uses: ./ci/actions/extra\n",
                action,
            ),
            (
                "an-action-in-a-flow-mapping",
                "      - { uses: ./ci/actions/extra }\n",
                action,
            ),
            (
                "an-action-under-a-quoted-key",
                "      - \"uses\": ./ci/actions/extra\n",
                action,
            ),
            (
                "an-action-under-an-escaped-key",
                "      - \"\\x75ses\": ./ci/actions/extra\n",
                action,
            ),
            (
                "a-directory-as-the-first-key",
                "      - working-directory: scripts\n        run: bash gate.sh\n",
                script,
            ),
            (
                "a-directory-in-a-flow-mapping",
                "      - { working-directory: scripts, run: bash gate.sh }\n",
                script,
            ),
            (
                "a-directory-below-its-key",
                "      - run: bash gate.sh\n        working-directory:\n          scripts\n",
                script,
            ),
            (
                "a-cd-after-a-wide-dash",
                "      -   run: cd scripts && bash gate.sh\n",
                script,
            ),
            (
                "a-cd-as-the-second-key",
                "      - name: lint\n        run: cd scripts && bash gate.sh\n",
                script,
            ),
            (
                "a-cd-before-a-semicolon",
                "      - run: |\n          cd scripts; bash gate.sh\n",
                script,
            ),
            (
                "a-cd-into-the-checkout",
                "      - run: cd \"$GITHUB_WORKSPACE/scripts\" && bash gate.sh\n",
                script,
            ),
            (
                "a-pushd",
                "      - run: pushd scripts && bash gate.sh\n",
                script,
            ),
        ];
        for (about, step, reached) in cases {
            let (findings, _) = gate_after(about, &extra, |text| {
                once(text, gate, &format!("{gate}{step}"))
            });
            assert!(
                findings.iter().any(|finding| finding.file == reached),
                "{about}: `{reached}` was not walked: {findings:#?}"
            );
        }
    }

    /// A file CI reaches holds no space but a space and a tab.
    ///
    /// **A space to Rust alone is a letter to YAML**: a key indented with
    /// U+3000 below a block scalar was the scalar's text to every reader here,
    /// and the quote it left open swallowed the `macos` job's gate step.
    #[test]
    fn no_letter_is_read_as_a_space() {
        each_is_said(&[
            (
                "an-ideographic-space",
                WORKFLOW,
                THE_MACOS_GATE,
                "      - name: lint\n        run: true\n        env:\n          A: |\n            text\n          \u{3000}B: \"x\n            - name: cargo xtask check\n              run: cargo xtask check\n           \"\n\n  # **The gate on Windows",
                "holds U+3000, a space to every reader here",
            ),
            (
                "a-no-break-space",
                super::RELEASE_WORKFLOW,
                "name: release\n",
                "name: release\n#\u{a0}a note\n",
                "holds U+00A0, a space to every reader here",
            ),
        ]);
        let tab = bounded_after("a-tab", WORKFLOW, |text| {
            once(text, "name: check\n", "name: check\n#\ta note\n")
        });
        assert!(tab.is_empty(), "a tab is a space to YAML too: {tab:#?}");
    }

    /// The branches a release fires on are the channels the verb releases.
    ///
    /// **Two lists of one set**: the trigger's branches here, and the values
    /// `--channel` accepts in `crate::release`. A channel added to one is a
    /// branch that releases nothing, or a channel nothing releases.
    #[test]
    fn the_release_fires_on_every_channel_and_on_no_other_branch() {
        let mut channels: Vec<String> =
            <crate::release::Channel as clap::ValueEnum>::value_variants()
                .iter()
                .filter_map(clap::ValueEnum::to_possible_value)
                .map(|value| value.get_name().to_owned())
                .collect();
        channels.sort();
        let mut fired: Vec<String> = super::RELEASED_FROM.map(str::to_owned).to_vec();
        fired.sort();
        assert_eq!(fired, channels);
    }
}
