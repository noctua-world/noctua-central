//! Every value on a published page, against the registry it came from.
//!
//! **The same gap as `the_registry_reaches_the_code.rs`, one artifact over.**
//! `cargo xtask check` compares the files under `docs/` with what the emitter
//! produces *now*, so an emitter that carries a value wrongly agrees with itself
//! and the check is green. This file is what stands between the registries and
//! the Markdown pages.
//!
//! The failure this exists for, reproduced with simultaneous one-line mutations
//! and a single `cargo xtask build`:
//!
//! * every severity published as `info` — so `NC-JRNL-0001`, "the journal's hash
//!   chain is broken", read as informational and retryable;
//! * every error published as retryable;
//! * every email's parameter list published as `none`;
//! * every Argon2id cost published as `0`;
//! * every command published as a non-mutating `Read` needing no confirmation —
//!   the *exact* failure `the_registry_reaches_the_code.rs` exists for;
//! * every log field published as `U64`, and nothing optional.
//!
//! Without this file, `cargo xtask check` is green through all of them, measured:
//! every generated file matches, and the two generations are byte-identical.
//! `NOCTUA.md §15.3` makes these pages normative — they are generated from the
//! same source the code consumes *so that they cannot drift*, and they are the source the in-app
//! pages will render — so a page that lies is a document that lies.
//!
//! # Two rules this file follows
//!
//! **Every mapping is written out here**, never asked of the emitter: a test
//! that consults `severity_word` to check `severity_word` agrees with itself
//! through any swap. A mapping written out is what catches a `FieldType` swap
//! one file over.
//!
//! **The pages are read from disk**, not regenerated. Comparing a fresh emission
//! against a fresh emission is the `§19.3` failure.

use std::path::{Path, PathBuf};

use nc_catalog_compiler::compile;
use nc_catalog_compiler::ledger::Bootstrap;
use nc_catalog_compiler::model::{
    Catalog, Database, FieldType, Level, OnLockout, Owner, Raising, Reversibility, Severity,
    Strategy, Unit,
};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).to_path_buf()
}

fn catalog() -> Catalog {
    compile(&root(), Bootstrap::Refuse)
        .expect("the catalog this repository ships compiles")
        .catalog
}

/// The seven pages `cargo xtask docs` writes.
const PAGES: [&str; 7] = [
    "capabilities.md",
    "commands.md",
    "emails.md",
    "errors.md",
    "limits.md",
    "log-events.md",
    "notifications.md",
];

/// Every table heading on a page, in the order they appear.
///
/// A heading is the row above the `|---|---|` rule, which is what makes it a
/// heading rather than a row: Markdown gives a table no other marker, and the
/// emitter writes the rule on the line directly below. Reading them this way is
/// what lets a table that nobody wrote down be found, which a list checked with
/// `any` cannot do.
fn the_headings_of(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    lines
        .iter()
        .enumerate()
        .filter(|(at, _)| {
            lines
                .get(at + 1)
                .is_some_and(|below| below.starts_with("|---"))
        })
        .map(|(_, line)| (*line).to_owned())
        .collect()
}

/// One published page, as it is on disk.
fn page(name: &str) -> String {
    let path = root().join("docs").join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("docs/{name} is not readable: {error}"))
}

/// Every table row of a page, as its cells, with the header and rule dropped.
///
/// A Markdown row is `| a | b | c |`; splitting on `|` yields two empty ends,
/// which are trimmed away. Rows are keyed by their first cell everywhere below,
/// so a page that reordered its rows still compares.
fn rows(text: &str) -> Vec<Vec<String>> {
    text.lines()
        .map(str::trim)
        .filter(|line| line.starts_with('|') && line.ends_with('|'))
        .filter(|line| !line.trim_matches(['|', '-', ' '].as_slice()).is_empty())
        .map(|line| {
            // **The emitter's own escape, honoured.** `cell()` writes `\|` for a
            // pipe inside a value, and splitting on every `|` produced an extra
            // cell and shifted every column after it — the parser and the
            // emitter disagreeing about the format they share.
            let mut cells = Vec::new();
            let mut current = String::new();
            let mut escaped = false;
            for character in line.trim_matches('|').chars() {
                match character {
                    // **The escape is kept.** Unescaping it is the other half
                    // of the same mistake: an expected value built through
                    // `cell_text` keeps the escape — a pipe is `\|` there too —
                    // so a *correct* page would fail on any sentence holding a
                    // pipe, which is exactly what `cell()` exists for.
                    '\\' if !escaped => {
                        escaped = true;
                        current.push('\\');
                    }
                    '|' if !escaped => cells.push(core::mem::take(&mut current)),
                    other => {
                        escaped = false;
                        current.push(other);
                    }
                }
            }
            cells.push(current);
            cells
                .into_iter()
                .map(|cell| cell.trim().to_owned())
                .collect()
        })
        .collect()
}

/// The row whose first cell is `key`, in a page's tables.
///
/// **Exactly one, not the first.** A `find` compares the first row with that key
/// and never looks further, so an emitter publishing a code twice — once
/// correctly and once with anything at all in it — is green under it: the first
/// row satisfies every assertion and the second is read by nothing. A page that
/// lists the same error code twice with two different meanings is worse than one
/// that omits it, because a reader stops at the first.
fn row<'a>(rows: &'a [Vec<String>], key: &str) -> &'a Vec<String> {
    let mut matching = rows.iter().filter(|row| row[0] == key);
    let first = matching
        .next()
        .unwrap_or_else(|| panic!("no published row for `{key}`"));
    assert!(
        matching.next().is_none(),
        "`{key}` is published on more than one row, and a reader stops at the first"
    );
    first
}

/// The part of a page under one heading, from `## name` to the next `##`.
///
/// **Everything below is read inside a section**, which closes a whole class:
/// `has(label, value)` and `matching.any(…)` asked of the *page* let any two
/// entries' facts be exchanged with every assertion still passing. Measured,
/// on the registry that ships: `journal.verify` published as a
/// mutating `Delete` and `limits.prune` as a non-mutating `Read` — a read-only
/// command shown as destructive and a destructive one shown as needing no
/// confirmation — with `cargo xtask check` green. That second half is
/// word-for-word the failure this file's own header says it exists to close.
fn section<'a>(page: &'a str, heading: &str) -> &'a str {
    let opening = format!("## `{heading}`\n");
    let at = page
        .find(&opening)
        .unwrap_or_else(|| panic!("no section for `{heading}`"));
    let rest = &page[at + opening.len()..];
    rest.find("\n## ").map_or(rest, |end| &rest[..end])
}

/// How many cells this file has actually compared.
///
/// A floor, for the `§19.3` reason every gate in this repository carries one: a
/// test that walked an empty catalog would pass having compared nothing.
#[derive(Default)]
struct Counted(usize);

impl Counted {
    fn same(&mut self, published: &str, expected: &str, about: &str) {
        assert_eq!(published, expected, "{about}");
        self.0 += 1;
    }

    /// The floor: at least `least` cells, and never zero.
    ///
    /// **Never `x >= x`, which cannot fire.** A `least` computed from the same
    /// catalog with the same per-entry multiplier the loop has just used is one.
    /// Proved by the case floors exist for: with such floors, a catalog with
    /// `errors`, `areas`, `limits.tables` and `limits.stores` emptied compares
    /// **zero** cells and passes, `0 >= 0`. That is `§19.3`'s
    /// "gate wired to nothing", in the file whose header says a floor exists to
    /// prevent exactly it.
    ///
    /// A floor derived from the catalog is still worth having — it scales, and
    /// it catches a loop that skipped entries. What it cannot do on its own is
    /// notice an empty catalog, so that is asserted separately and absolutely.
    fn floor(&self, least: usize, about: &str) {
        assert!(
            self.0 > 0,
            "no cell of {about} was compared. An empty catalog compares nothing and \
             every derived floor is then zero, which is the shape a floor exists to \
             refuse"
        );
        assert!(
            self.0 >= least,
            "only {} cell(s) of {about} were compared, so this proves little",
            self.0
        );
    }
}

/// The word a severity is published as.
///
/// **Written out, not asked of `docs::severity_word`.** Flattening that function
/// so every severity published as `info` left the whole gate green; a test that
/// consulted it would have stayed green too.
const fn severity_word(severity: Severity) -> &'static str {
    match severity {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Error => "error",
        Severity::Critical => "critical",
    }
}

/// The sentence a raising state is published as, written out for the same reason.
///
/// **Written out, not asked of `Raising::written` — the emitter's own
/// function.** It is the one word-mapping here a test *could* ask the emitter
/// for, because `Strategy::written` and `Level::written` are `pub(crate)` while
/// `Raising::written` is `pub`; asked of it, swapping its two arms publishes the
/// inverted sentence for every code with `cargo xtask check` byte-identical to
/// a clean run, under the page's own pinned paragraph saying *"the column is the
/// count"*. `raising` has no other published surface — there is no
/// `ErrorCode::raising()` — so this is its only comparison.
const fn raising_word(raising: Raising) -> &'static str {
    match raising {
        Raising::Raised => "yes",
        Raising::Reserved => "not yet — the number is held",
    }
}

/// The word a level is published as, written out for the same reason.
const fn level_word(level: Level) -> &'static str {
    match level {
        Level::Trace => "trace",
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

/// How a boolean is published.
const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

/// The sentence the "what happens at lockout" column publishes.
///
/// **The sixth mapping of this shape, with its arms apart.** The emitter's
/// `what_a_lockout_does` is deliberately split into four, under a paragraph
/// saying the spelled `refuse` and an absent `on_lockout` are two decisions that
/// agree today and must be changeable apart. A mirror written inline as
/// `(Some(_), Some(OnLockout::Refuse) | None)` — one arm with two alternatives —
/// puts them back together and bounds the emitter's split by nothing: measured,
/// that arm split by hand and one half corrupted survives, where twelve sibling
/// controls in this file all die to the same treatment.
///
/// **And the half that survives is the *spelled* one.**
/// `catalog/limits.toml` spells `refuse` on no budget at all: four
/// budgets carry a `lockout_after`, three of them with no `on_lockout` — the
/// `(Some(_), None)` arm — and the fourth is `device_pin`'s
/// `on_lockout = "wipe-credential"`. So the arm no declared budget reaches is
/// `(Some(_), Some(OnLockout::Refuse))`, and the absent half is the one three
/// of the four rows drive.
///
/// So the arms are apart here too, and
/// [`every_variant_is_published_as_the_word_it_names`] asks each of them the
/// question the column asks — which is what reaches an arm the registry does
/// not.
///
/// `const fn`, which is what the type wall asks of a function handing back a
/// `&'static str`: its result is a compile-time value, so it is not a door into
/// the log's static-string currency.
#[expect(
    clippy::match_same_arms,
    reason = "the spelled `refuse` and an absent `on_lockout` are two decisions that \
              agree today, and the emitter this mirrors is split for that reason: \
              merged, the spelled variant cannot be corrupted without the absent one \
              going with it, and neither can be compared on its own"
)]
const fn lockout_word(lockout_after: Option<u32>, on_lockout: Option<OnLockout>) -> &'static str {
    match (lockout_after, on_lockout) {
        (None, _) => "there is no lockout on this surface",
        (Some(_), Some(OnLockout::WipeCredential)) => {
            "the stored credential is destroyed, so the next sign-in is a full \
             re-authentication"
        }
        // **And `None` is a consequence, not a blank.** Rendered as an em dash on
        // the rows that *have* a lockout, it shows a reader a lockout with no
        // stated effect — while `model.rs` reads an absent value as refusing
        // until the window passes, which is what the page says.
        (Some(_), Some(OnLockout::Refuse)) => {
            "further attempts are refused until the window passes"
        }
        (Some(_), None) => "further attempts are refused until the window passes",
    }
}

/// The word a database is published as, written out for the same reason.
const fn database_word(database: Database) -> &'static str {
    match database {
        Database::Server => "`server`",
        Database::LocalFile => "`local`",
    }
}

/// The word a strategy is published as, written out for the same reason.
const fn strategy_word(strategy: Strategy) -> &'static str {
    match strategy {
        Strategy::OldestFirst => "`oldest-first`",
        Strategy::PolicyPack => "`policy-pack`",
        Strategy::RefuseAtLimit => "`refuse-at-limit`",
    }
}

/// The words whose rows a limit counts are published as, written out for the
/// same reason.
///
/// **A refusing writer can count one owner's rows**, and the page published
/// such a number as the table's own bound: `store_entries` holds its
/// `Maximum rows` for every plugin installed in every realm.
const fn counted_over_word(max_rows: Option<u64>, per: Option<Owner>) -> &'static str {
    match (max_rows, per) {
        (None, _) => "—",
        (Some(_), None) => "the whole table",
        (Some(_), Some(Owner::Plugin)) => "each plugin's rows in each realm",
        (Some(_), Some(Owner::Unit)) => "each unit's rows",
    }
}

/// The answers this repository may publish to "can it be undone" for a command
/// that changes state, written out by meaning.
///
/// **A closed set, and not the absence of one word.**
/// `!published.contains("yes")` over the fixed sentences [`reversibility_word`]
/// returns — none of which holds `yes` — is true for every value it can range
/// over, and asked of this file's *mirror* rather than the page it judges a copy
/// of the claim instead of the claim.
///
/// Measured: the emitter's `Write` sentence changed to *"of course, the kernel
/// reverses it each time"* in `crates/nc-catalog-compiler/src/docs.rs` **and**
/// in both mirrors, then `cargo run -p xtask -- build` —
/// `docs/commands.md:36  | Can it be undone | of course, the kernel reverses it
/// each time |` for `limits.prune`, a `DELETE … COMMIT` with no tombstone and
/// no copy — gave `17 passed; 0 failed` here and `check passed` overall. The
/// mirrors catch a one-sided edit; they say nothing when an author changes the
/// emitter and the mirror together, which is what an author does when they mean
/// the change. The page-level backstop beside them is a needle, and the same
/// file already records a rewording walking past one.
///
/// So the set is written down, and a sentence that is not in it fails until
/// somebody puts it there on purpose. **The `write` answer is what the kernel's
/// undo does**: `kernel.undo` and `kernel.redo` take back a step's store
/// writes and the blobs it made, and refuse to pass a step that did anything
/// else — the sentence says both, and names the two commands that do it.
///
/// **And the answer of a `write` command undo stops at**, which every one the
/// kernel runs on the realm itself is: without it the page tells a reader
/// `limits.prune` can be undone while the kernel makes every step that ran it a
/// barrier, and a set with one `write` answer accepts that sentence for every
/// `write` command alike.
const REFUSALS: [&str; 3] = [
    "yes, for what the kernel keeps — its `store` writes and the blobs it made — through `kernel.undo` and `kernel.redo`; a step that left the machine or cannot be undone, with no compensation registered for it, is one undo stops at, and a timer it armed or an event it published stays as it is (§11.5)",
    "no — a step that ran it is one undo stops at (§11.5)",
    "**no**, and a human confirms it each time (§14.6)",
];

/// The two commands a person steps through their history with, which are the
/// only `write` commands whose step the page may say undo takes back.
const THE_HISTORY: [&str; 2] = ["kernel.undo", "kernel.redo"];

/// What one command's block on `docs/commands.md` answers to "can it be undone".
///
/// **Exactly one row, not the first** — [`row`]'s discipline. `REFUSALS` closes
/// the *value* and leaves the *row* open, so read as the first row, a second
/// answer riding behind a compliant one is read by nothing. Measured, the
/// emitter writing both rows into every command's block:
///
/// ```text
/// | Can it be undone | not automatically — it changes state and nothing here reverses it |
/// | Can it be undone | yes — the kernel restores the rows from its undo log |
/// ```
///
/// for `limits.prune`, a `DELETE … COMMIT` with no tombstone — `cargo xtask
/// check` gave `check passed`, byte for byte the baseline, and the drift gate
/// passed too because the emitter is what produced it. The
/// three guards that look like they would see it do not: the prose pin skips
/// every line opening with `|`, the dispatcher mirror asks membership, and the
/// page-level needle is a phrase any rewording walks past.
fn the_undo_answer(block: &str, name: &str) -> String {
    let label = "| Can it be undone |";
    let mut matching = block.lines().filter(|line| line.starts_with(label));
    let answer = matching
        .next()
        .unwrap_or_else(|| panic!("`{name}` has no \"can it be undone\" row"));
    assert!(
        matching.next().is_none(),
        "`{name}` is published with more than one answer to \"can it be undone\", and a \
         reader stops at the first"
    );
    answer[label.len()..]
        .trim_end_matches('|')
        .trim()
        .to_owned()
}

/// The word a reversibility is published as, for a command of the history
/// when `the_history`.
const fn reversibility_word(reversibility: Reversibility, the_history: bool) -> &'static str {
    // **The answer, not the registry's word.** The column asks "can it be
    // undone", and `read` / `write` is a permission, not an answer.
    //
    // **And the answer has to be true**, which a mirror cannot say. A
    // byte-for-byte copy of the emitter's own match, when that says *"yes,
    // the kernel can reverse it"* of `limits.prune` — a `DELETE … ; COMMIT`
    // with no tombstone and no copy, which nothing reverses — asserts the false
    // sentence and stays green. `no_mutating_command_claims_more_undo_than_the_kernel_has`
    // is the assertion that is not a mirror.
    match reversibility {
        Reversibility::Read => "nothing to undo — it changes nothing",
        Reversibility::Write if the_history => {
            "yes, for what the kernel keeps — its `store` writes and the blobs it made — \
             through `kernel.undo` and `kernel.redo`; a step that left the machine or cannot \
             be undone, with no compensation registered for it, is one undo stops at, and a \
             timer it armed or an event it published stays as it is (§11.5)"
        }
        Reversibility::Write => "no — a step that ran it is one undo stops at (§11.5)",
        Reversibility::Irreversible => "**no**, and a human confirms it each time (§14.6)",
    }
}

/// Every line of prose the six generated pages publish, page by page.
///
/// Pinned rather than derived, because prose is what the emitter authors: there
/// is nothing in the registry to compare it against, and that is exactly why
/// it has to be written down here to be read at all.
const PROSE: [(&str, &str); 146] = [
    (
        "capabilities.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "capabilities.md",
        "<!-- Source: catalog/capabilities.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("capabilities.md", "# Capabilities"),
    (
        "capabilities.md",
        "Every capability a plugin can be granted, and the functions of the plugin world each one grants. **Deny by default** (`NOCTUA.md §11.3`): a plugin's manifest lists the capabilities it asks for, and every function of a capability it was not granted answers `denied` — a value the plugin reads and goes on from, never a trap — so a plugin learns its limits by being told no.",
    ),
    (
        "capabilities.md",
        "Every function the world imports is on this page exactly once, under a capability or in an interface given to every plugin: `crates/nc-kernel/tests/capabilities.rs` reads the world with its own parser and holds this list to it in both directions. A resource's method is written as the parser names it, `[method]<resource>.<name>`, and is reachable only through the function that hands out the resource, which answers to the same capability.",
    ),
    ("capabilities.md", "## Granted on request"),
    ("capabilities.md", "## Given to every plugin"),
    (
        "commands.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "commands.md",
        "<!-- Source: catalog/commands.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("commands.md", "# Commands"),
    (
        "commands.md",
        "Every command the catalog declares — the kernel's own — with what a dispatcher decides on: the Cedar action it is authorized as, how its resource is extracted, whether it changes state, and whether it can be undone.",
    ),
    (
        "commands.md",
        "**The kernel registers every one of them and decides each at its decision point**, as it decides a plugin's, and runs each one it allows itself. A plugin's commands are not on this page: a plugin declares them at `activate`, and the kernel holds each to the judge that holds these.",
    ),
    (
        "commands.md",
        "**`Action` is what the policy pack decides on, and the pack has rules of its own.** `policy/core/quorum.cedar` forbids `Delete` unless the context carries `quorum_met`, and the schema makes that attribute required — so *every* command whose action is `Delete` needs a quorum, whatever else it declares. A reader designing a scheduled caller for one has to read the pack as well as this page.",
    ),
    ("commands.md", "## `journal.verify`"),
    (
        "commands.md",
        "**en** — Verify activity history — Check that every recorded action is intact and in order.",
    ),
    (
        "commands.md",
        "**pt-BR** — Verificar histórico de atividades — Conferir que toda ação registrada está intacta e em ordem.",
    ),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"from\":{\"type\":\"integer\",\"minimum\":0}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.redo`"),
    (
        "commands.md",
        "**en** — Redo — Make again the last step you took back.",
    ),
    (
        "commands.md",
        "**pt-BR** — Refazer — Fazer de novo o último passo que você desfez.",
    ),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.undo`"),
    (
        "commands.md",
        "**en** — Undo — Take back your last step: everything it changed in Noctua, and the files it made.",
    ),
    (
        "commands.md",
        "**pt-BR** — Desfazer — Desfazer o seu último passo: tudo o que ele mudou no Noctua e os arquivos que ele criou.",
    ),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `limits.prune`"),
    (
        "commands.md",
        "**en** — Clear old records — Remove the oldest records from any table that has passed its limit.",
    ),
    (
        "commands.md",
        "**pt-BR** — Limpar registros antigos — Remover os registros mais antigos de qualquer tabela que passou do limite.",
    ),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"table\":{\"type\":\"string\"},\"dry_run\":{\"type\":\"boolean\"}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    (
        "emails.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "emails.md",
        "<!-- Source: catalog/emails.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("emails.md", "# Emails"),
    (
        "emails.md",
        "Every transactional email. Same contract as notifications: one that is not declared does not exist, and both of its message keys resolve in every locale.",
    ),
    (
        "emails.md",
        "**Nothing sends any of these yet either.** There is no mail transport in this tree, and `catalog/emails.toml` marks the single entry below as the one that exists to prove the mechanism — the entry a later stage replaces.",
    ),
    ("emails.md", "## `catalog.mechanism`"),
    ("emails.md", "Parameters: none"),
    (
        "errors.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "errors.md",
        "<!-- Source: catalog/errors.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("errors.md", "# Error codes"),
    (
        "errors.md",
        "Every error this platform declares. A code is **append-only**: it identifies one error for as long as the logs live, so a code here is never reused and never repointed (`NOCTUA.md §15.1`).",
    ),
    (
        "errors.md",
        "**Declared is not raised.** The *Raised* column says which of these the platform produces today, measured by looking for the code's own variant in `crates/` outside the generated crate, its own tests and its comments. This page said \"every error the platform can return\" over all of them, including codes nothing constructs at all — and then stated how many, a number written into this sentence rather than counted, which was wrong the next time a code was reserved. The column is the count.",
    ),
    ("errors.md", "## Areas"),
    (
        "errors.md",
        "An area with no codes is a reservation: the prefix is spoken for and the subsystem is a later stage.",
    ),
    (
        "limits.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "limits.md",
        "<!-- Source: catalog/limits.toml. Run `cargo xtask docs`. -->",
    ),
    ("limits.md", "# Limits"),
    (
        "limits.md",
        "Every bound this catalog declares. A table that can grow without bound and has no entry here **fails the build** — that gate is what makes this a policy rather than a habit (`§15.8`).",
    ),
    (
        "limits.md",
        "**Declared is not the same as enforced, and each part of this page has its own answer.** The kernel holds every plugin to every bound under **Kernel**. A `local` table is held by `crates/nc-local`, which refuses a unit at a `refuse-at-limit` bound as it commits, and by the pruner, which `Kernel::pump` and the `limits.prune` command run over the local database — and nothing in this tree calls either on a timer yet. Nothing here prunes a `server` table, and there is no rate limiter and no key derivation, as their own sections say. This paragraph said *nothing on this page has a runner* while every `refuse-at-limit` bound under **Tables** was refused at already. One thing is enforced and is not a number — `access_records`' ordering column, which a trigger assigns (`migrations/0006_access_records_orders_by_insertion.sql`), because a bound that orders by a value a caller writes is a bound a caller can move.",
    ),
    (
        "limits.md",
        "**A table is in one of two databases**, and its `Database` column says which: `server` is PostgreSQL, built by `migrations/`, and `local` is the SQLite file on the user's own machine, built by `local-migrations/`. A table whose strategy is `refuse-at-limit` is one whose oldest rows are not the pruner's to take — user state, what a plugin keeps or a blob a unit pinned, or a unit's images and notes, which an undo needs whole and which leave with their unit — so nothing prunes it by its own count: its `Maximum rows` is where its writer refuses a new row, and the refusal is the bound. `Counted over` says whose rows that number counts: a table counted over each plugin's rows in each realm holds that many for every plugin installed in every realm the machine serves, and one counted over each unit's rows that many for every unit the store holds, so its own size is the number times its owners, not the number.",
    ),
    (
        "limits.md",
        "Each table below says where its own bound comes from, in its own last column or in the prose above it.",
    ),
    ("limits.md", "## Tables"),
    ("limits.md", "## Stores outside PostgreSQL"),
    ("limits.md", "## Key derivation"),
    (
        "limits.md",
        "Argon2id, per `§10.5` and `§10.6` — the sections `catalog/limits.toml` itself cites over these entries. (This line said `§15.5`, which is rate limits: the page named a section about a different subject, eleven lines from a second, independently written mention of the same algorithm.) **Nothing derives a key yet**: there is no `argon2` dependency in this tree, and these are the parameters the stage that adds one will read.",
    ),
    ("limits.md", "## Rate limits"),
    (
        "limits.md",
        "Every authentication surface, per identifier and per client address. **Attempts before the delay starts** is how many are answered at once; after that the delay curve applies, escalating, its last value repeating (`§15.5`). A surface answers an unlimited number of attempts when its **Lockout** column reads `never` — which 6 of these do — and a curve ending at zero would say the same, though none here does. Lockout is a later number than the budget on purpose: a lockout below it would mean the curve never runs.",
    ),
    (
        "limits.md",
        "**None of this is enforced yet.** There is no rate limiter and no Argon2 dependency in this tree; these rows are the declaration the surfaces of a later stage read.",
    ),
    ("limits.md", "## Kernel"),
    (
        "limits.md",
        "The bounds the local kernel holds every plugin to (`§11.3`), read from `[kernel]` and from nowhere else: a plugin's manifest may lower some of them for itself and raise none. How many keys one plugin may keep and how many writes one unit may record are not here — they are `store_entries` and `store_writes` under **Tables**, where their writer refuses at them, so each is one number held in one place.",
    ),
    (
        "limits.md",
        "**Every one is enforced by `crates/nc-kernel`.** It reads them as the ceiling a plugin's manifest may lower, and holds a plugin's instance to its memory, instances, tables, stack and fuel, each call into it to its epoch deadline — the plugin's own running, which a ticker advances every `epoch_tick_ms` — and to the deadline of a whole call tree, every host call it makes but a dispatch to `host_call_deadline_ms`, its kills but those at a call tree's deadline to `restart_budget` inside `restart_window_ms`, its declarations to `commands_per_plugin`, a call tree's dispatches to its deadline, depth and count, each question the decision point asks the directory to `host_call_deadline_ms`, a confirmation or an approval to `confirmation_ttl_ms`, a dispatch's arguments and answer to `payload_bytes`, the store's keys, values and listings to theirs, what one unit stages to `store_writes`' bound in keys and to `unit_bytes` in bytes, a blob to `blob_bytes`, each read of one to `blob_read_chunk_bytes`, the blob readers and writers a plugin holds to `blob_handles`, the units it holds begun inside one command to `unit_handles`, what the blobs' sweep keeps of a blob no unit holds to `blob_orphan_grace_ms`, how far back an undo and a redo look in a person's history to `undo_history`, a plugin's network request to `net_header_count` headers, `net_request_head_bytes` of URL and headers and `net_request_body_bytes` of body and its answer to `net_header_count` and `net_response_body_bytes`, the transport a daemon sends it with to `net_timeout_ms`, the timers a plugin holds to `timers_per_plugin`, each to `timer_min_interval_ms` and `timer_max_delay_ms`, the events waiting for delivery to `event_queue` and `event_queue_bytes`, the surfaces a plugin holds to `surfaces_per_plugin` and each tree it renders to `ui_tree_nodes`, `ui_tree_depth` and `ui_text_bytes`, and a plugin's own log to `log_ring_lines` lines of `log_line_bytes` each.",
    ),
    (
        "log-events.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "log-events.md",
        "<!-- Source: catalog/log-events.toml. Run `cargo xtask docs`. -->",
    ),
    ("log-events.md", "# Log events"),
    (
        "log-events.md",
        "Every event the central log can receive. **No field here can be written as free text at a logging call**: the kinds below are the only ones the registry has, `type = \"id\"` is refused outright, and a `String` cannot be made into a `LogValue` (`§15.4`). What that closes is a sentence a caller composed; what it does not close is a caller who encodes something into a value of a kind the registry does allow — `Snowflake` is written as its Crockford base32 text, and twelve of its characters are the caller’s. The source gate refuses the conversion at a logging call site, which is where it is visible; `crates/nc-log/src/value.rs` states the residual, and `NOCTUA.md §5.1` owns it.",
    ),
    (
        "log-events.md",
        "**`nc-kernel` is the one crate outside `nc-log` that logs.** It emits every `kernel.*` event below, `net.refused`, `journal.verified` and `journal.broken`; every other event is declared for a later stage, and `cargo xtask check` counts the crates that may log beside their logging call sites. The types and the guarantee are real today; the traffic is the kernel's alone.",
    ),
    ("log-events.md", "## `db.migrated`"),
    ("log-events.md", "Migrations were applied"),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `ids.clock_backwards`"),
    (
        "log-events.md",
        "Minting refused because the clock moved backwards",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `ids.flake_collision`"),
    (
        "log-events.md",
        "A machine-flake candidate was taken and the next was tried",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `ids.sequence_exhausted`"),
    (
        "log-events.md",
        "This millisecond ran out of sequence numbers and minting waited",
    ),
    ("log-events.md", "Level: `debug`"),
    ("log-events.md", "## `journal.appended`"),
    ("log-events.md", "An entry was appended to the chain"),
    ("log-events.md", "Level: `debug`"),
    ("log-events.md", "## `journal.broken`"),
    (
        "log-events.md",
        "Verification of the journal's chain failed, and at which entry",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `journal.verified`"),
    (
        "log-events.md",
        "The chain verified from the entry `from` names to its last, and how many entries that was",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `kernel.capability_denied`"),
    (
        "log-events.md",
        "A plugin called a host function of a capability it was not granted, and was answered denied",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.dispatch_refused`"),
    (
        "log-events.md",
        "A dispatch was refused before its command ran, or its unit was not kept, and why",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.host_call_refused`"),
    (
        "log-events.md",
        "A plugin's host call was refused for where it ran or whose name it used, rather than for a capability: a change where nothing may change, an act for a person where no call tree runs, or an event under a topic not its own — logged once per function and code for each instance of the plugin",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.plugin_killed`"),
    (
        "log-events.md",
        "A plugin was stopped while it ran, and its store was dropped",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.plugin_quarantined`"),
    (
        "log-events.md",
        "A plugin that was killed is not run again until it is released: it crashed too often inside its restart window, where a kill at its call tree's deadline is not counted, or did not come back as it was",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `kernel.plugin_restarted`"),
    (
        "log-events.md",
        "A plugin that was killed or released was instantiated again and activated as it was",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `kernel.recovered`"),
    (
        "log-events.md",
        "The kernel, opening, found a unit of work a stopped process left open in the journal, reverted what the store kept of it, and closed it as aborted",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.revert_failed`"),
    (
        "log-events.md",
        "The store refused the reversal of a unit the journal never closed — the journal did not take its close, or its process stopped before it — so the unit stays committed in the store and open in the journal until the next prune of the kernel that left it, or a kernel opening over the two, reverts it",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `kernel.step_reversed`"),
    (
        "log-events.md",
        "A step of a person's history was undone or redone, and the unit that did it was kept",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `kernel.unit_aborted`"),
    (
        "log-events.md",
        "A unit of work was aborted, and nothing it staged was kept",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `log.dropped`"),
    (
        "log-events.md",
        "Events were dropped because the queue was full",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `log.store_unreachable`"),
    ("log-events.md", "The log store did not accept a batch"),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `net.refused`"),
    (
        "log-events.md",
        "A plugin's net.fetch was refused before anything was sent, or by the resolver, or for an answer too large — logged once per reason for each instance of the plugin",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `prune.pruned`"),
    (
        "log-events.md",
        "A table was over its limit and rows were removed",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `prune.within_limit`"),
    (
        "log-events.md",
        "A table was inspected and was below its limit",
    ),
    ("log-events.md", "Level: `debug`"),
    (
        "notifications.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "notifications.md",
        "<!-- Source: catalog/notifications.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("notifications.md", "# Notifications"),
    (
        "notifications.md",
        "Every notification a user can receive. **A notification that is not declared does not exist** — sending one takes a generated typed handle, so one that was removed is a compile error (`§15.2`).",
    ),
    (
        "notifications.md",
        "**Nothing sends any of these yet**, and the single entry below exists to prove the mechanism: `catalog/notifications.toml` marks it as the entry a later stage replaces. Read this page as the shape a notification takes, not as a list of what a user will receive. `docs/emails.md` carries the same disclosure over the same kind of entry; the limits, commands and log-event pages say it of their own registries. This page did not, and published a self-test fixture as a feature.",
    ),
    ("notifications.md", "## `catalog.mechanism`"),
    ("notifications.md", "Severity: info. Parameters: `{name}`"),
];

/// The comment block `catalog/errors.reserved.toml` opens with.
///
/// The checksum below it covers the reservations, not this, and the file is
/// deliberately not drift-checked — so this list is what reads these lines.
const LEDGER_HEADER: [&str; 20] = [
    "# The error codes this repository has used, and what each was reserved to",
    "# mean. Append-only: the compiler refuses to repoint or remove a line, because",
    "# every log entry ever written with a code still refers to it (NOCTUA.md",
    "# §15.1).",
    "#",
    "# `cargo xtask build` appends to this file. It is NOT generated output and is",
    "# not compared against a regeneration — an earlier version was, and a hand edit",
    "# ratified itself. The checksum below is over the reservations and over how many",
    "# there are, and it is what catches an edit made outside the compiler.",
    "#",
    "# A ledger only ever grows, which is why the count is inside the checksum: a",
    "# file holding fewer lines than it says it holds lost some, and every code they",
    "# reserved would otherwise be re-derived with whatever the registry means by it",
    "# today.",
    "#",
    "# Each line records the message key **and** the developer description, because",
    "# a code's meaning is both. Recording only the key let a code be rewritten from",
    "# one error to an unrelated one — key intact, description and both locale",
    "# sentences replaced — with the gate green.",
    "",
];

/// Every line of prose the six generated pages publish.
///
/// **`each_page_names_the_registry_it_came_from` reads line two and nothing
/// else.** Everything from line three down to the first table is free text the
/// emitter assembles, and this compares it. Measured, each with
/// `cargo xtask build && cargo xtask check` green:
///
/// ```text
/// docs/errors.md   "A code is **recyclable**: once the logs that mention it
///                   have aged out, the number is free again and may be
///                   repointed at a different error (`NOCTUA.md §15.1`)"
/// docs/errors.md   "An area with no codes has been retired: its prefix is free
///                   again and the next subsystem may take it."
/// docs/errors.md   the heading became `# Log events` while `Source:` still
///                   named `catalog/errors.toml`
/// ```
///
/// — the page whose whole subject is the append-only rule, citing `§15.1` for
/// its opposite.
///
/// **And `catalog/errors.reserved.toml`'s header, which is outside its own
/// checksum.** That checksum is taken over the reservations deliberately, and
/// the ledger is deliberately not drift-checked, so its header is reachable
/// from neither: measured without
/// `the_ledger_header_says_the_file_is_append_only`, rewritten to *"Advisory
/// only: a line here may be edited or deleted by hand once the logs that
/// mention the code have aged out"*, the file that exists to make "never reuse
/// a code" enforceable ships instructions to reuse one, with the whole gate
/// green.
///
/// Pinned rather than derived, because prose is what the emitter authors: there
/// is nothing in the registry to compare it against, and the two-place edit is
/// the point.
#[test]
fn every_line_of_prose_a_page_publishes_is_written_down() {
    let mut held: Vec<(String, String)> = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(root().join("docs"))
        .expect("docs/ is generated")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            std::path::Path::new(name)
                .extension()
                .is_some_and(|kind| kind == "md")
        })
        // A file written by hand is not a page the emitter authors.
        .filter(|name| {
            !nc_catalog_compiler::HAND_WRITTEN_FILES.contains(&format!("docs/{name}").as_str())
        })
        .collect();
    names.sort();
    for name in &names {
        for line in page(name).lines() {
            if !line.trim().is_empty() && !line.starts_with('|') {
                held.push((name.clone(), line.to_owned()));
            }
        }
    }
    let expected: Vec<(String, String)> = PROSE
        .iter()
        .map(|(page, line)| ((*page).to_owned(), (*line).to_owned()))
        .collect();
    assert_eq!(
        held, expected,
        "a generated page publishes prose that is not written down here. Every sentence \
         on these pages is one the emitter authors rather than one it copies from a \
         registry, so nothing else can tell whether it is true — `docs/errors.md` \
         carried `A code is **recyclable**` under a heading about the append-only rule \
         with the whole gate green"
    );
}

/// The header of `catalog/errors.reserved.toml`, which is outside its checksum.
///
/// That checksum is taken over the reservations deliberately, and the ledger is
/// deliberately not drift-checked, so its header is reachable from neither:
/// without this test, rewritten to *"Advisory only: a line here may be edited
/// or deleted by hand once the logs that mention the code have aged out"*, the
/// file that exists to make "never reuse a code" enforceable ships instructions
/// to reuse one, with the whole gate green.
#[test]
fn the_ledger_header_says_the_file_is_append_only() {
    let ledger = std::fs::read_to_string(root().join("catalog/errors.reserved.toml"))
        .expect("the ledger is generated");
    let header: Vec<&str> = ledger
        .lines()
        .take_while(|line| line.starts_with('#') || line.trim().is_empty())
        .collect();
    assert_eq!(
        header, LEDGER_HEADER,
        "`catalog/errors.reserved.toml`'s header changed. The checksum below it is taken \
         over the reservations only, and this file is deliberately not drift-checked, so \
         nothing else reads these lines — and they are what tell a reader the file is \
         append-only"
    );
}

/// Every column heading a generated page publishes, written down.
///
/// **A page can put every value under the wrong heading.** The row comparisons
/// in this file key on the first cell and then read `row[1]`, `row[2]` by
/// *position*, and the locale columns are the only ones resolved by name — so,
/// without this list, exchanging two labels in the emitter leaves every cell
/// where it was and every assertion green:
///
/// ```text
/// | Code | Retryable | Severity | Raised | … |
/// | `NC-JRNL-0001` | critical | no | yes | … |
///   ->  ok  882 test(s)   /   check passed
/// ```
///
/// A reader of `docs/errors.md` — which `NOCTUA.md §15.3` makes normative, and
/// which the in-app pages will render — then reads that code as *not
/// retryable → no* and its severity as `no`. The limits, rate-limit and
/// notification tables are read the same way, so this list holds every table.
///
/// A written list, because a heading is prose the emitter authors rather than a
/// value it copies: there is nothing in the registry to compare it against.
#[test]
fn every_page_publishes_its_values_under_the_headings_written_down_here() {
    // Every heading, with how many tables carry it, compared as a whole. Checked
    // with `any` over a list that misses a table, the rate-limit one — ten rows
    // by eight columns, the largest on these pages — is named nowhere. Measured,
    // with such a list, four of its labels exchanged in
    // the emitter and the pages regenerated: `17 passed`, `check passed`, and
    // every verdict line byte-identical to the green run, while the page told a
    // reader that `device_pin`'s lockout threshold was its consequence and its
    // scope was its budget. `any` cannot see a table that was never written
    // down, so the count is held here too and the comparison is an equality: a
    // table that arrives unpinned fails, and so does one that leaves.
    const HEADINGS: [(&str, usize, &str); 13] = [
        (
            "capabilities.md",
            1,
            "| Capability | Interface | Functions | en | pt-BR |",
        ),
        (
            "capabilities.md",
            1,
            "| Interface | Functions | Why no manifest is asked |",
        ),
        ("limits.md", 1, "| Limit | Value | What it bounds |"),
        (
            "errors.md",
            1,
            "| Code | Severity | Retryable | Raised | Parameters | Developer description | en | pt-BR |",
        ),
        ("errors.md", 1, "| Area | Codes | What it covers |"),
        (
            "limits.md",
            1,
            "| Table | Database | Strategy | Ordered by | Maximum rows | Counted over | Prune batch (rows per run) | Why it has no number |",
        ),
        (
            "limits.md",
            1,
            "| Store | Retention (days) | Maximum disk | Enforced by |",
        ),
        (
            "limits.md",
            1,
            "| Purpose | Memory | Iterations | Parallelism (lanes) | Output | Salt |",
        ),
        (
            "limits.md",
            1,
            "| Surface | Budget | Window (s) | Attempts before the delay starts | Delay curve (ms) | Lockout | On lockout | Cumulative |",
        ),
        ("log-events.md", 22, "| Field | Kind | Optional |"),
        ("notifications.md", 1, "| | Key | en | pt-BR |"),
        ("emails.md", 1, "| | Key | en | pt-BR |"),
        ("commands.md", 4, "| | |"),
    ];
    let mut written: Vec<(String, usize, String)> = HEADINGS
        .iter()
        .map(|(named, times, heading)| ((*named).to_owned(), *times, (*heading).to_owned()))
        .collect();
    written.sort();

    let mut published: Vec<(String, usize, String)> = Vec::new();
    for named in PAGES {
        let text = page(named);
        let mut seen: Vec<(String, usize)> = Vec::new();
        for heading in the_headings_of(&text) {
            match seen.iter_mut().find(|(held, _)| *held == heading) {
                Some((_, times)) => *times += 1,
                None => seen.push((heading, 1)),
            }
        }
        published.extend(
            seen.into_iter()
                .map(|(heading, times)| ((*named).to_owned(), times, heading)),
        );
    }
    published.sort();

    assert_eq!(
        published, written,
        "the headings the six pages publish are not the ones written down here. A row's cells \
         are compared by position, so a heading that moved leaves every value under the wrong \
         name with nothing else able to tell, and a table nobody wrote down has every one of \
         its labels held by nothing"
    );
    assert!(
        published.len() >= PAGES.len(),
        "{} heading(s) were read across {} page(s), so a page publishes no table at all and \
         this comparison is between two empty halves",
        published.len(),
        PAGES.len()
    );

    // And the rate-limit table's rows are in the order the emitter claims, which
    // is an ordering a reader leans on: the comment above the sort says why — "a
    // reader scanning for `activation_code` looked in the wrong place" — and the
    // row comparison finds each row by `(surface, per which)` and never looks at
    // where it sits.
    let limits = page("limits.md");
    let surfaces: Vec<&str> = limits
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|rest| rest.split_once('`'))
        .map(|(surface, _)| surface)
        .filter(|surface| {
            nc_catalog::RATE_LIMITS
                .iter()
                .any(|limit| limit.surface == *surface)
        })
        .collect();
    let mut expected = surfaces.clone();
    expected.sort_unstable();
    assert_eq!(
        surfaces, expected,
        "`docs/limits.md` publishes its rate-limit rows out of surface order"
    );
    assert!(surfaces.len() >= 4, "{} row(s) were read", surfaces.len());
}

/// No page tells a reader that more of a command which changes state can be
/// taken back than the kernel takes back.
///
/// **The mirror above cannot ask this**, and the emitter is where the sentence
/// is written, so a claim and its "test" moved together. What is reversed is
/// narrow: `kernel.undo` puts back the store's writes of a person's newest step
/// and takes the pin off the blobs it made, and refuses to pass a step that
/// sent a request, pruned a table or could not be undone; `nc-prune` issues
/// `DELETE` and commits, and the journal's table refuses `DELETE` outright. A
/// page saying more is a safety claim a reader can act on.
#[test]
fn no_mutating_command_claims_more_undo_than_the_kernel_has() {
    let page = std::fs::read_to_string(root().join("docs/commands.md")).expect("the page is there");
    let mut mutating = 0_usize;
    for entry in nc_catalog::COMMANDS {
        if !entry.mutating {
            continue;
        }
        mutating += 1;
        // **What the page on disk answers**, not what this file's own mirror
        // would answer. The mirror is `reversibility_word`, and asking it is
        // what made this assertion vacuous.
        let published = the_undo_answer(section(&page, entry.name), entry.name);
        assert!(
            REFUSALS.contains(&published.as_str()),
            "`{}` changes state and `docs/commands.md` answers `{published}` to \"can \
             it be undone\". The kernel takes back a step's store writes and its blobs \
             and nothing else, so an answer outside the decided ones is a safety claim a \
             reader can act on. If it has genuinely changed, the new sentence goes into \
             `REFUSALS` as a decision",
            entry.name
        );
        // And the generated crate's enum reaches the compiler's, which is the
        // conversion the page is written through. A page and a crate that
        // disagree about one command is the drift the section reader closed.
        let mirrored = reversibility_word(
            match entry.reversibility {
                nc_catalog::Reversibility::Read => Reversibility::Read,
                nc_catalog::Reversibility::Write => Reversibility::Write,
                nc_catalog::Reversibility::Irreversible => Reversibility::Irreversible,
            },
            THE_HISTORY.contains(&entry.name),
        );
        assert_eq!(
            published, mirrored,
            "`{}` is published on the page as `{published}` and reaches this file's \
             conversion as `{mirrored}`",
            entry.name
        );
    }
    // The floor: a registry with no mutating command would satisfy the loop by
    // never entering it, and `limits.prune` is one.
    assert!(mutating > 0, "no command in the registry changes state");
    // **And the command undo stops at, by name, on the page.** The kernel makes
    // a step that pruned a table a barrier (`leaves_the_machine` in
    // `crates/nc-kernel/src/history.rs`), and a row saying "yes" there is a
    // sentence in the set, answered for the wrong command.
    let pruned = the_undo_answer(section(&page, "limits.prune"), "limits.prune");
    assert_eq!(
        pruned, REFUSALS[1],
        "`limits.prune` is a step undo stops at, and the page answers `{pruned}`"
    );
    for name in THE_HISTORY {
        let answered = the_undo_answer(section(&page, name), name);
        assert_eq!(
            answered, REFUSALS[0],
            "`{name}` steps through the history, and the page answers `{answered}`"
        );
    }
    assert!(
        !page.contains("the kernel can reverse it"),
        "the page still tells a reader something here can be undone"
    );

    // **And the generated crate, which is what a kernel author reads.** A guard
    // scoped to the page misses `crates/nc-catalog/src/commands.rs` documenting
    // `Reversibility::Write` as "changes state the kernel can reverse" — the
    // sentence `docs.rs`'s own comment calls "a safety claim nothing supports",
    // in the file the kernel compiles against. A needle misses it too when the
    // wording has no trailing `it`.
    //
    // **And the compiler's own enum, one file further out.** `Reversibility` is
    // *declared* in `crates/nc-catalog-compiler/src/model.rs` — a `pub` type in
    // a `pub` module, so its doc comment is what rustdoc publishes as this
    // platform's definition of a `write` command, against `docs/commands.md`,
    // against the generated crate, and against `AGENTS.md`. Measured, with a
    // guard naming only the generated crate and the emitter: the sentence in
    // `crates/nc-catalog/src/commands.rs` fails at once, and the same sentence
    // in `model.rs` leaves the tree at `check passed`. Every file that publishes
    // the definition is read.
    for source in [
        "crates/nc-catalog/src/commands.rs",
        "crates/nc-catalog-compiler/src/model.rs",
        "crates/nc-catalog-compiler/src/docs.rs",
    ] {
        let text = std::fs::read_to_string(root().join(source))
            .unwrap_or_else(|error| panic!("`{source}` is in this repository: {error}"));
        // **What rustdoc publishes, which is the doc comments.** Scanning the
        // whole file made `docs.rs` a finding for the comment that *quotes* the
        // removed sentence while explaining why it went — a needle matching the
        // argument for its own rule, which this tree has walked into before. A
        // `///` or `//!` line is what a reader sees; an ordinary `//` is a note
        // to whoever edits the file.
        let published: String = text
            .lines()
            .map(str::trim_start)
            .filter(|line| line.starts_with("///") || line.starts_with("//!"))
            .collect::<Vec<_>>()
            .join("\n");
        for claim in [
            "the kernel can reverse",
            "can be undone by",
            "is reversible",
        ] {
            assert!(
                !published.contains(claim),
                "`{source}` documents `{claim}`, and that is what a kernel author reads — \
                 undo stops at a destructive command rather than reversing it"
            );
        }
    }
}

/// The word a field kind is published as.
///
/// The mapping a swap hides. With both sides consulting the emitter's own
/// function, `ErrorCode` and `Id` exchanged in the emitter leave every
/// descriptor-level test one file over green.
const fn kind_word(kind: FieldType) -> &'static str {
    match kind {
        FieldType::U64 => "count",
        FieldType::I64 => "signed number",
        FieldType::U32 => "whole number, at most 32 bits wide",
        FieldType::Bool => "flag",
        FieldType::Duration => "span of time",
        FieldType::ErrorCode => "code from the error registry",
        FieldType::Id => "opaque identifier, written to the log exactly as it is",
        FieldType::Snowflake => "Snowflake, written as its Crockford base32 text",
        FieldType::Literal => "fixed text chosen when the code was written, never at run time",
        FieldType::Enum => "variant of an enum the platform declares — a state, never a name",
    }
}

#[test]
fn every_error_is_published_with_its_own_severity_and_retryability() {
    let catalog = catalog();
    let published = rows(&page("errors.md"));
    let mut counted = Counted::default();
    for entry in &catalog.errors {
        let row = row(&published, &format!("`{}`", entry.code));
        counted.same(&row[1], severity_word(entry.severity), &entry.code);
        counted.same(&row[2], yes_no(entry.retryable), &entry.code);
        // **Whether the platform raises it at all**: without it, a header
        // reading "every error the platform can return" sits over codes nothing
        // constructs.
        counted.same(&row[3], raising_word(entry.raising), &entry.code);
        // **The parameters**, which the registry declares: `§10.9` makes a
        // response a key *plus parameters*, and a caller reading this page to
        // build one cannot otherwise learn what the sentence asks for.
        counted.same(&row[4], &parameters(&entry.params), &entry.code);
        // The description, escaped for a table cell but otherwise unchanged.
        assert!(
            row[5].contains(&cell_text(&entry.description)),
            "`{}` is published as `{}` and declares `{}`",
            entry.code,
            row[5],
            entry.description
        );
        counted.0 += 1;
    }
    // **A registry with only empty parameter lists would make the column
    // above vacuous**, so at least one error has to declare some.
    assert!(
        catalog.errors.iter().any(|entry| !entry.params.is_empty()),
        "no error declares a parameter, so the column proves nothing"
    );
    // **In the order a reader looks a code up.** The registry is append-only, so
    // its declaration order is reservation order — and a code added to an
    // existing area lands at the bottom: unsorted, `NC-JRNL-0006`, `-0007` and
    // `-0008` publish below `NC-IDS-0006`, with `NC-IDS-0007` below them. The
    // generated enum sorts, so the two surfaces would disagree about one list.
    let codes: Vec<&str> = published
        .iter()
        .filter_map(|row| row[0].strip_prefix('`')?.strip_suffix('`'))
        .filter(|code| code.starts_with("NC-"))
        .collect();
    let mut expected = codes.clone();
    expected.sort_by_key(|code| {
        let mut parts = code.splitn(3, '-');
        let area = parts.nth(1).unwrap_or_default().to_owned();
        let number: u32 = parts.next().unwrap_or_default().parse().unwrap_or_default();
        (area, number)
    });
    assert_eq!(codes, expected, "the error table is not in order");
    assert_eq!(
        codes.len(),
        catalog.errors.len(),
        "the page publishes {} of {} codes",
        codes.len(),
        catalog.errors.len()
    );

    // **And at least one is not raised**, or the column above is a column of one
    // value and says nothing.
    assert!(
        catalog
            .errors
            .iter()
            .any(|entry| entry.raising != Raising::Raised),
        "every error is declared as raised, so that column proves nothing"
    );
    // Four per error at least, and the areas table underneath.
    counted.floor(catalog.errors.len() * 5, "errors.md");
    for (area, covers) in &catalog.areas {
        let row = row(&published, &format!("`{area}`"));
        // **How many codes the area holds**, which is what separates a live
        // subsystem from a reserved prefix: ten of seventeen have none, and the
        // page named them all the same way.
        let declared = catalog
            .errors
            .iter()
            .filter(|entry| entry.code.split('-').nth(1) == Some(area.as_str()))
            .count();
        assert_eq!(
            row[1],
            if declared == 0 {
                "none yet".to_owned()
            } else {
                declared.to_string()
            },
            "`{area}` publishes {} codes and holds {declared}",
            row[1]
        );
        assert!(
            row[2].contains(covers),
            "`{area}` is published as `{}`",
            row[1]
        );
    }
}

#[test]
fn every_limit_is_published_with_the_numbers_that_bound_it() {
    let catalog = catalog();
    let published = rows(&page("limits.md"));
    let mut counted = Counted::default();

    for (name, table) in &catalog.limits.tables {
        let row = row(&published, &format!("`{name}`"));
        // **Which database, which is which directory of migrations creates it.**
        counted.same(&row[1], database_word(table.database), name);
        counted.same(&row[2], strategy_word(table.strategy), name);
        counted.same(&row[3], &format!("`{}`", table.order_by), name);
        counted.same(
            &row[4],
            &table
                .max_rows
                // **Not "the policy pack".** That names a live mechanism for a
                // table nothing bounds; the page says `none` and carries the
                // registry's own `pack_reason` in a column of its own.
                .map_or_else(|| "none".to_owned(), |rows| rows.to_string()),
            name,
        );
        counted.same(&row[5], counted_over_word(table.max_rows, table.per), name);
        counted.same(
            &row[6],
            &table
                .prune_batch
                .map_or_else(|| "—".to_owned(), |batch| batch.to_string()),
            name,
        );
        // **And the last column, which matters most.** `pack_reason` is the
        // whole published justification
        // for a table `§15.8` refuses to bound — for `journal_entries` it is the
        // sentence saying audit retention is a per-realm policy-pack property
        // and that the table grows without limit, "a gap with a date on it
        // rather than a bound". `load.rs` forces the registry to carry it;
        // without this the page can drop it for an em dash with
        // `cargo xtask check` green, measured, so the row reads `| none | — |`
        // and the reason is nowhere.
        counted.same(
            &row[7],
            &table
                .pack_reason
                .as_ref()
                // Trimmed: the registry holds the reason as a TOML multi-line
                // string and keeps its closing newline, and the emitter writes
                // it into one cell.
                .map_or_else(|| "—".to_owned(), |reason| reason.trim().to_owned()),
            name,
        );
    }

    for (name, store) in &catalog.limits.stores {
        let row = row(&published, &format!("`{name}`"));
        counted.same(&row[1], &store.retention_days.to_string(), name);
        // **In the unit a reader thinks in, and the exact number too.** The
        // column published `8589934592` beside one headed `Retention (days)`:
        // two numbers on one row, in different units, one of them saying which.
        counted.same(
            &row[2],
            &format!(
                "{} GiB ({} bytes)",
                store.max_disk_bytes / (1 << 30),
                store.max_disk_bytes
            ),
            name,
        );
        // **Who enforces it**, which the code side compares too — without it the
        // page can publish "PostgreSQL's own pruner, which runs on every commit"
        // for the log store's thirty-day retention with the whole gate green.
        // `§15.3` makes these pages normative, and naming the wrong
        // enforcer is the one thing a reader of a limits page acts on.
        counted.same(&row[3], &cell_text(&store.enforced_by), name);
    }

    // **The key-derivation costs**, which can otherwise be published as zero
    // with the whole gate green. A published Argon2id cost of 0 is a document telling an
    // operator their passwords are protected by nothing.
    for (purpose, argon) in [
        ("password", catalog.limits.argon2id.password),
        ("device PIN", catalog.limits.argon2id.device_pin),
    ] {
        let row = row(&published, purpose);
        // **In the unit a reader thinks in.** A bare number in a column whose
        // header names no unit misleads: `Output 32` and `Salt 16` are *bytes*,
        // and a reader assuming bits is out by eight. The page owns a
        // humanizer, and this holds every figure here to it.
        counted.same(
            &row[1],
            &format!(
                "{} MiB ({} bytes)",
                u64::from(argon.memory_kib) / 1024,
                u64::from(argon.memory_kib) * 1024
            ),
            purpose,
        );
        counted.same(&row[2], &argon.iterations.to_string(), purpose);
        counted.same(&row[3], &argon.parallelism.to_string(), purpose);
        counted.same(&row[4], &format!("{} bytes", argon.output_length), purpose);
        counted.same(&row[5], &format!("{} bytes", argon.salt_length), purpose);
        assert!(
            argon.memory_kib > 0 && argon.iterations > 0,
            "a key-derivation cost of zero reached the page for {purpose}"
        );
    }

    counted.floor(
        catalog.limits.tables.len() * 4 + catalog.limits.stores.len() * 2 + 10,
        "limits.md",
    );
}

#[test]
fn every_capability_is_published_with_its_functions_and_its_sentence() {
    // **What a manifest asks for, and what asking for it reaches.** The page is
    // the one a person granting a plugin reads, so a capability published with
    // another's functions tells them they are granting one thing and grant
    // another.
    let catalog = catalog();
    let text = page("capabilities.md");
    let table = rows(&text);
    let header = table
        .iter()
        .find(|row| row.first().is_some_and(|cell| cell == "Capability"))
        .expect("the capabilities table has a header");
    let mut counted = Counted::default();
    for entry in &catalog.capabilities {
        let row = row(&table, &format!("`{}`", entry.name));
        counted.same(&row[1], &format!("`{}`", entry.interface), &entry.name);
        // Written out: each function backticked, joined by commas, and the
        // capability that grants a declaration rather than a call said so.
        let functions = if entry.functions.is_empty() {
            "none — it grants a declaration, not a call".to_owned()
        } else {
            entry
                .functions
                .iter()
                .map(|function| format!("`{function}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        counted.same(&row[2], &functions, &entry.name);
        for locale in &catalog.locales {
            let tag = &locale.meta.bcp47;
            let column = header
                .iter()
                .position(|cell| cell == tag)
                .unwrap_or_else(|| panic!("no `{tag}` column: {header:?}"));
            let declared = locale
                .message
                .get(&entry.description)
                .unwrap_or_else(|| panic!("`{}` has no `{tag}` sentence", entry.description));
            counted.same(&row[column], &cell_text(declared), &entry.name);
        }
    }
    // **And what every plugin is given, function by function**, written the
    // way a capability's are, so a reader sees which calls no manifest is
    // asked about rather than an interface whose reason may predate them.
    for entry in &catalog.ungated {
        let row = row(&table, &format!("`{}`", entry.interface));
        let functions = if entry.functions.is_empty() {
            "none".to_owned()
        } else {
            entry
                .functions
                .iter()
                .map(|function| format!("`{function}`"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        counted.same(&row[1], &functions, &entry.interface);
        counted.same(&row[2], &cell_text(entry.reason.trim()), &entry.interface);
    }
    counted.floor(
        catalog.capabilities.len() * 4 + catalog.ungated.len() * 2,
        "capabilities.md",
    );
}

/// The words a kernel limit's value is published with, for each unit.
///
/// **Written out, not asked of the emitter**, for the reason this file's header
/// gives. A byte count is the largest binary unit that divides it with the exact
/// figure beside it, like every size on the limits page.
fn kernel_value(value: u64, unit: Unit) -> String {
    let bytes = |value: u64| {
        for (word, size) in [("GiB", 1_u64 << 30), ("MiB", 1 << 20), ("KiB", 1 << 10)] {
            if value >= size && value.is_multiple_of(size) {
                return format!("{} {word} ({value} bytes)", value / size);
            }
        }
        format!("{value} bytes")
    };
    match unit {
        Unit::Bytes => bytes(value),
        Unit::Milliseconds => format!("{value} milliseconds"),
        Unit::Fuel => format!("{value} units of fuel"),
        Unit::Ticks => format!("{value} ticks of the epoch"),
        Unit::Count => value.to_string(),
    }
}

#[test]
fn every_kernel_limit_is_published_in_the_unit_it_is_counted_in() {
    // **A number with no unit is half a bound.** `30000` beside the call tree's
    // deadline reads as seconds as easily as milliseconds, and the epoch's
    // deadline is a count of ticks that is not a time at all until it is
    // multiplied by the tick.
    let catalog = catalog();
    let table = rows(&page("limits.md"));
    let mut counted = Counted::default();
    for limit in catalog.limits.kernel.each() {
        let row = row(&table, &format!("`{}`", limit.name));
        counted.same(&row[1], &kernel_value(limit.value, limit.unit), limit.name);
        counted.same(&row[2], &cell_text(limit.bounds), limit.name);
    }
    // The units the shipped limits use, so a value published in the wrong one
    // is read in each of them.
    assert_eq!(
        kernel_value(catalog.limits.kernel.memory_bytes, Unit::Bytes),
        "64 MiB (67108864 bytes)"
    );
    counted.floor(catalog.limits.kernel.each().len() * 2, "limits.md's kernel");
}

#[test]
fn every_command_is_published_with_what_a_dispatcher_decides_on() {
    let catalog = catalog();
    let text = page("commands.md");
    assert!(!catalog.commands.is_empty(), "nothing was compared");
    let mut counted = Counted::default();
    for entry in &catalog.commands {
        assert!(
            text.contains(&format!("## `{}`\n", entry.name)),
            "`{}` has no section on the page",
            entry.name
        );
        // **Inside this command's own section.** Keyed page-globally, any two
        // commands' facts could be exchanged with every assertion still passing.
        let mine = rows(section(&text, &entry.name));
        // **Exactly one row per label, not at least one.** `the_undo_answer`
        // argues for this above; read with `any`, a *second* row under the same
        // label published beside the right one leaves this green: measured, one
        // literal added to the emitter, `cargo xtask build`, then
        // `check passed` — and `limits.prune`, a `DELETE … COMMIT` command,
        // published as both `Delete` and `Read`, eight lines under the page's
        // own paragraph saying every `Delete` command needs a quorum. A reader
        // stops at the first row, and the first row is whichever the emitter
        // wrote first.
        let has = |label: &str, value: &str| {
            let under: Vec<&Vec<String>> = mine.iter().filter(|row| row[0] == label).collect();
            assert!(
                under.len() <= 1,
                "`{}` is published with {} rows labelled {label}, and a reader stops at \
                 the first",
                entry.name,
                under.len()
            );
            under.first().is_some_and(|row| row[1] == value)
        };
        for (label, value) in [
            ("Action", format!("`{}`", entry.action)),
            (
                "Resource selector",
                format!("`{}`", entry.resource_selector),
            ),
            ("Mutating", yes_no(entry.mutating).to_owned()),
            (
                "Can it be undone",
                reversibility_word(
                    entry.reversibility,
                    THE_HISTORY.contains(&entry.name.as_str()),
                )
                .to_owned(),
            ),
        ] {
            assert!(
                has(label, &value),
                "no command row publishes {label} as `{value}` for `{}`",
                entry.name
            );
            counted.0 += 1;
        }
        // **The keybinding row exists only when there is a keybinding**, which
        // is why it is not in the list above: otherwise `Keybinding | — |` sits
        // on every row of the shipped page, because nothing declares one.
        match entry.keybinding.as_deref() {
            Some(key) => assert!(
                has("Keybinding", &format!("`{key}`")),
                "`{}` declares `{key}` and the page does not publish it",
                entry.name
            ),
            None => assert!(
                !mine.iter().any(|row| row[0] == "Keybinding"),
                "`{}` declares no keybinding and the page publishes a row for one",
                entry.name
            ),
        }
        counted.0 += 1;
        // And the arguments schema, which is what a caller is validated against.
        assert!(
            section(&text, &entry.name).contains(entry.args_schema.trim()),
            "`{}`'s arguments schema is not on its page",
            entry.name
        );
        counted.0 += 1;
    }
    // **Every command published as a non-mutating `Read`** is the mutation that
    // goes out green without this. `mutating` is what undo, the journal and the allowlist
    // read; `Irreversible` is what forces a human confirmation.
    assert!(
        catalog.commands.iter().any(|entry| entry.mutating),
        "no command mutates, so `Mutating` proves nothing"
    );
    assert!(
        catalog
            .commands
            .iter()
            .any(|entry| entry.reversibility != Reversibility::Read),
        "every command is a Read, so `Reversibility` proves nothing"
    );
    counted.floor(catalog.commands.len() * 6, "commands.md");
}

#[test]
fn every_log_event_is_published_with_the_kinds_the_registry_gave_it() {
    let catalog = catalog();
    let text = page("log-events.md");
    assert!(!catalog.events.is_empty(), "nothing was compared");
    let mut counted = Counted::default();
    for entry in &catalog.events {
        assert!(
            text.contains(&format!("## `{}`\n", entry.name)),
            "`{}` has no section on the page",
            entry.name
        );
        // **Inside this event's own section.** Page-global, these let most
        // events be published carrying the *next* event's level and the next
        // event's field table with everything green.
        let mine = section(&text, &entry.name);
        // **Written out, not `{:?}`.** Compared against the same `Debug`
        // spelling the emitter uses, a variant rename changes the published page
        // and this test with it — and the page publishes `OldestFirst` where the
        // registry says `oldest-first`, with the test agreeing because both
        // sides ask the same wrong question.
        assert!(
            mine.contains(&format!("Level: `{}`\n", level_word(entry.level))),
            "`{}` is `{}` and its own section says otherwise",
            entry.name,
            level_word(entry.level)
        );
        let published = rows(mine);
        for field in &entry.fields {
            let key = format!("`{}`", field.name);
            let matching: Vec<&Vec<String>> = published
                .iter()
                .filter(|row| row[0] == key && row.len() == 3)
                .collect();
            assert!(!matching.is_empty(), "`{key}` is on no published row");
            // A field name may repeat across events, so every row carrying it
            // has to agree — which is also true of the registry.
            assert!(
                matching
                    .iter()
                    .any(|row| row[1] == kind_word(field.kind) && row[2] == yes_no(field.optional)),
                "`{}`.`{}` is {} / optional {} and no published row says so: {matching:?}",
                entry.name,
                field.name,
                kind_word(field.kind),
                field.optional
            );
            counted.0 += 2;
        }
    }
    // **The wall itself.** Every field published as `U64` with nothing optional
    // left the page claiming a type system the registry does not have.
    let kinds: std::collections::BTreeSet<&str> = catalog
        .events
        .iter()
        .flat_map(|entry| entry.fields.iter())
        .map(|field| kind_word(field.kind))
        .collect();
    assert!(
        kinds.len() > 1,
        "every field is one kind, so the kind column proves nothing"
    );
    // Derived, not a constant: a constant floor is slack the moment the page
    // grows, while every other floor scales.
    let fields: usize = catalog.events.iter().map(|entry| entry.fields.len()).sum();
    counted.floor(fields * 2, "log-events.md");
}

#[test]
fn every_email_publishes_the_parameters_it_requires() {
    let catalog = catalog();
    let text = page("emails.md");
    assert!(!catalog.emails.is_empty(), "nothing was compared");
    let mut compared = 0_usize;
    for entry in &catalog.emails {
        assert!(
            text.contains(&format!("## `{}`\n", entry.id)),
            "`{}` has no section on the page",
            entry.id
        );
        // **Published as `none` for every email**, with the gate green. A
        // template rendered without a parameter it requires is an email that
        // goes out with a hole in it.
        let expected = if entry.params.is_empty() {
            "Parameters: none".to_owned()
        } else {
            format!(
                "Parameters: {}",
                entry
                    .params
                    .iter()
                    .map(|param| format!("`{{{param}}}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        // Page-global, so two emails' parameter lists could be exchanged — one
        // template rendered without a parameter it requires is an email that
        // goes out with a hole in it.
        assert!(
            section(&text, &entry.id).contains(&expected),
            "`{}` requires {:?} and its own section says otherwise",
            entry.id,
            entry.params
        );
        compared += 1;
    }
    // The shipped registry's one email declares no parameters, so the populated
    // branch cannot be falsified from here. It is driven over a scratch catalog
    // in `nc-catalog-compiler`'s `the_pages_carry_values_the_shipped_registry_
    // happens_not_to_use`, together with the two other branches this registry
    // does not reach — a `|` in a message, and an event with no fields.
    assert_eq!(compared, catalog.emails.len());
}

#[test]
fn every_notification_publishes_its_severity_and_its_parameters() {
    // Published as constants — `info` and `none` — both leave the whole gate
    // green without this, measured. A notification's severity is what
    // decides how loudly a person is interrupted.
    let catalog = catalog();
    let text = page("notifications.md");
    assert!(!catalog.notifications.is_empty(), "nothing was compared");
    for entry in &catalog.notifications {
        let mine = section(&text, &entry.id);
        assert!(
            mine.contains(&format!("Severity: {}.", severity_word(entry.severity))),
            "`{}` is {:?} and its page says otherwise: {mine}",
            entry.id,
            entry.severity
        );
        assert!(
            mine.contains(&format!("Parameters: {}", parameters(&entry.params))),
            "`{}` requires {:?} and its page says otherwise",
            entry.id,
            entry.params
        );
    }
}

/// A value as a table cell publishes it.
///
/// **The escaper itself, not a copy of it.** A hand mirror of `docs.rs::cell`
/// that applies neither `printable` nor the `<` arm — and that arm exists
/// because a locale sentence reading *"Ask &lt;your administrator&gt; to
/// look."* put a raw tag into a published table — makes **this test** fail
/// with the page correct on the first registry value holding an angle bracket:
/// measured with `enforced_by` given a `<docker-compose.yml>`, the page says
/// `&amp;lt;` and the mirror says `<`, and the refusal names `docs/limits.md` —
/// which is the outcome `docs.rs`'s own comment argues against, a page blamed
/// for a registry line.
///
/// **What the trade costs, with both sides calling one function.** This
/// comparison cannot notice a change to the escaping. That is acceptable
/// only because the escaper has gates that do *not* mirror it: dropping the `<`
/// arm is caught by `no_doc_line_or_cell_carries_an_invisible_character`, and
/// dropping `printable` by `docs::tests::nothing_a_translator_can_write_ends_a_cell_early`.
/// Both catches are measured.
fn cell_text(text: &str) -> String {
    nc_catalog_compiler::cell_for_tests(text)
}

// **Where a comparison escapes a cell, it calls this rather than a hand mirror.**
// A mirror like `.replace('|', "\\|").replace(['\n', '\r'], " ")` is not what
// this function does: `printable` runs first and `<` becomes `&lt;`, because a
// page is Markdown with HTML in it. So an ordinary sentence holding an angle
// bracket — the exact shape the `<` arm exists for — makes **the test** fail
// with the page correct, and names a generated file rather than the registry
// line the sentence came from.
//
// The cost is the one written above for the first site: both sides being one
// function means these comparisons cannot see an escaping change. That is
// acceptable only because the escaper is held without mirroring elsewhere, and
// it is — measured here, not assumed: with `<` not escaped, and again with the
// bar not escaped, `cargo test -p nc-catalog-compiler` fails both times.

/// How a parameter list is published.
fn parameters(params: &[String]) -> String {
    if params.is_empty() {
        "none".to_owned()
    } else {
        params
            .iter()
            .map(|param| format!("`{{{param}}}`"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[test]
fn every_log_event_publishes_the_line_that_describes_it() {
    // The description is emitted through `cell()` and compared by nothing.
    let catalog = catalog();
    let text = page("log-events.md");
    for entry in &catalog.events {
        assert!(
            section(&text, &entry.name).contains(&cell_text(&entry.description)),
            "`{}` is described as `{}` and its own section says otherwise",
            entry.name,
            entry.description
        );
    }
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "four pages carry locale columns and each publishes them in a shape \
              of its own; splitting this would put the same comparison in four \
              places, which is how three of them went uncompared"
)]
fn every_sentence_is_published_under_the_locale_it_came_from() {
    // **The columns are what the page is for.** Without this, swapping which
    // locale each column draws from publishes every user-visible sentence under
    // the wrong language heading — Portuguese under `en`, English under
    // `pt-BR` — with `check passed`, measured. The generated Rust stays
    // correct through that, so the *documents* are the only thing that lies, and
    // `docs.rs`'s own header says a reader comparing the two languages is doing
    // the thing this surface exists for.
    let catalog = catalog();
    let mut compared = 0_usize;
    let locales: Vec<&str> = catalog
        .locales
        .iter()
        .map(|locale| locale.meta.bcp47.as_str())
        .collect();
    assert!(locales.len() >= 2, "one locale compares nothing");

    // The column a locale's sentences appear in, read from the header row.
    let column_of = |header: &[String], tag: &str| {
        header
            .iter()
            .position(|cell| cell == tag)
            .unwrap_or_else(|| panic!("no `{tag}` column: {header:?}"))
    };

    let errors = page("errors.md");
    let table = rows(&errors);
    let header = table
        .iter()
        .find(|row| row.iter().any(|cell| cell == "Code"))
        .expect("the errors table has a header");
    for entry in &catalog.errors {
        let row = row(&table, &format!("`{}`", entry.code));
        for tag in &locales {
            let published = &row[column_of(header, tag)];
            let declared = catalog
                .locales
                .iter()
                .find(|locale| locale.meta.bcp47 == *tag)
                .and_then(|locale| locale.message.get(&entry.message))
                .unwrap_or_else(|| panic!("`{}` has no `{tag}` message", entry.message));
            assert_eq!(
                published,
                &cell_text(declared),
                "`{}`'s `{tag}` column does not hold its `{tag}` sentence",
                entry.code
            );
            compared += 1;
        }
    }
    let _ = compared;

    // **And the other three pages that carry locale columns**, not only
    // `errors.md`: `messages()` drives `notifications.md` and `emails.md`, and
    // `commands()` writes its own `**{tag}** —` lines. Measured with `errors.md`
    // alone checked: reversing the locale each column draws from **in
    // `messages()` only** publishes Portuguese under `en` and English under
    // `pt-BR` on both pages, with everything green.
    for (name, entries) in [
        (
            "notifications.md",
            catalog
                .notifications
                .iter()
                .map(|entry| {
                    (
                        entry.id.clone(),
                        vec![entry.title.clone(), entry.body.clone()],
                    )
                })
                .collect::<Vec<_>>(),
        ),
        (
            "emails.md",
            catalog
                .emails
                .iter()
                .map(|entry| {
                    (
                        entry.id.clone(),
                        vec![entry.subject.clone(), entry.body.clone()],
                    )
                })
                .collect::<Vec<_>>(),
        ),
    ] {
        let text = page(name);
        for (id, keys) in entries {
            let mine = rows(section(&text, &id));
            let header = mine
                .iter()
                .find(|row| row.iter().any(|cell| cell == "Key"))
                .unwrap_or_else(|| panic!("`{id}` has no message table on {name}"));
            for key in keys {
                // The key is the *second* cell on these pages: the first names
                // which message it is (`Title`, `Body`).
                let quoted = format!("`{key}`");
                let row = mine
                    .iter()
                    .find(|row| row.get(1) == Some(&quoted))
                    .unwrap_or_else(|| panic!("no row for `{key}` on {name}"));
                for tag in &locales {
                    let published = &row[column_of(header, tag)];
                    let declared = catalog
                        .locales
                        .iter()
                        .find(|locale| locale.meta.bcp47 == *tag)
                        .and_then(|locale| locale.message.get(&key))
                        .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"));
                    assert_eq!(
                        published,
                        &cell_text(declared),
                        "`{key}`'s `{tag}` column on {name} does not hold its `{tag}` sentence"
                    );
                    compared += 1;
                }
            }
        }
    }

    // And `commands.md`, whose locale lines are prose rather than a table.
    let commands = page("commands.md");
    for entry in &catalog.commands {
        let mine = section(&commands, &entry.name);
        for tag in &locales {
            let sentence = |key: &str| {
                catalog
                    .locales
                    .iter()
                    .find(|locale| locale.meta.bcp47 == *tag)
                    .and_then(|locale| locale.message.get(key))
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"))
                    .clone()
            };
            let line = format!(
                "**{tag}** — {} — {}",
                cell_text(&sentence(&entry.title)),
                cell_text(&sentence(&entry.description))
            );
            assert!(
                mine.contains(&line),
                "`{}`'s `{tag}` line is not what the registry declares",
                entry.name
            );
            compared += 1;
        }
    }

    // **And the two locales differ**, so a page publishing one language twice
    // would not pass the comparison above by both sides agreeing.
    let differ = catalog.errors.iter().any(|entry| {
        let mut sentences = locales.iter().map(|tag| {
            catalog
                .locales
                .iter()
                .find(|locale| locale.meta.bcp47 == *tag)
                .and_then(|locale| locale.message.get(&entry.message))
        });
        let first = sentences.next().flatten();
        sentences.any(|other| other != first)
    });
    assert!(
        differ,
        "no message differs between locales, so this proves nothing"
    );

    // The floor, at the end so every comparison above counts toward it: one
    // per error per locale, plus the notifications, the emails and the
    // commands. Placed here because clippy is right that a count nothing
    // reads is a count nothing measures.
    assert!(
        compared >= catalog.errors.len() * locales.len(),
        "only {compared} sentence(s) were compared"
    );
}

#[test]
fn every_rate_limit_is_published_with_the_numbers_that_bound_it() {
    // Without this, zeroing every one of them publishes `login` and
    // `activation_code` as answering an unlimited number of attempts — which the
    // page's own sentence three lines above the table says is what a curve
    // ending at zero means — with `check passed` and formatting and lints clean,
    // measured. These rows are read here.
    let catalog = catalog();
    let published = rows(&page("limits.md"));
    let mut counted = Counted::default();
    assert!(
        !catalog.limits.rate_limits.is_empty(),
        "nothing was compared"
    );
    for limit in &catalog.limits.rate_limits {
        for (which, budget) in [
            ("identifier", &limit.per_identifier),
            ("address", &limit.per_address),
        ] {
            // **Exactly one, which is what every other table on this page
            // asks**, not a `.find(…)`: a reader stops at the first —
            // `row()`'s whole doc says so. The page→registry walk asks only
            // membership plus `judged >= declared`, so a *second* `login` row
            // raises `judged` and passes it: measured, one `out.push_str` in
            // `docs.rs` emitting `| `login` | per identifier | 900 | 100000 | 0
            // | never | there is no lockout on this surface | no |` in sorted
            // position gave `16 passed`, `ok 15 generated file(s) match the
            // catalog` and `check passed` — publishing 100,000 attempts and a
            // back-off curve ending at zero, which the sentence three lines
            // above that table calls an unlimited number of attempts.
            let mut matching = published.iter().filter(|row| {
                row[0] == format!("`{}`", limit.surface) && row[1] == format!("per {which}")
            });
            let row = matching
                .next()
                .unwrap_or_else(|| panic!("no `{}` / per {which} row", limit.surface));
            assert!(
                matching.next().is_none(),
                "`{}` / per {which} is published on more than one row, and a reader stops \
                 at the first",
                limit.surface
            );
            counted.same(&row[2], &budget.window_seconds.to_string(), &limit.surface);
            counted.same(&row[3], &budget.max_attempts.to_string(), &limit.surface);
            counted.same(
                &row[4],
                &budget
                    .delay_ms
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                &limit.surface,
            );
            counted.same(
                &row[5],
                &budget
                    .lockout_after
                    // **With the denominator, which the two numbers do not share.**
                    // `after 10` and `after 8` are ten failures inside the window and
                    // eight ever, and without the denominator the column says
                    // neither.
                    .map_or_else(
                        || "never".to_owned(),
                        |after| {
                            if budget.cumulative {
                                format!("after {after} failed attempts, counted for ever")
                            } else {
                                format!(
                                    "after {after} failed attempts in {} s",
                                    budget.window_seconds
                                )
                            }
                        },
                    ),
                &limit.surface,
            );
            // **What happens at lockout**, which reaches the generated code and
            // must reach the page: it is the one consequence in this table that
            // is not a delay — `device_pin` destroys the stored credential
            // (`§10.6`) — and a reader of the published limits has to see it.
            // **And whether there is a lockout at all**: without it, rows
            // reading `Lockout: never` can publish a consequence for one
            // anyway — the enumeration DoS `catalog/limits.toml` forbids over
            // exactly those rows.
            counted.same(
                &row[6],
                lockout_word(budget.lockout_after, budget.on_lockout),
                &limit.surface,
            );
            counted.same(&row[7], yes_no(budget.cumulative), &limit.surface);
            // The page's own claim, asserted rather than published: a curve that
            // ends at zero answers an unlimited number of attempts.
            assert!(
                budget.delay_ms.last().is_some_and(|last| *last > 0),
                "`{}`'s per-{which} delay curve ends at zero",
                limit.surface
            );
        }
    }
    // **And both answers reachable**, or the column is a constant: one surface
    // wipes the credential and the rest do not.
    let wiping = catalog
        .limits
        .rate_limits
        .iter()
        .flat_map(|limit| [&limit.per_identifier, &limit.per_address])
        .filter(|budget| budget.on_lockout == Some(OnLockout::WipeCredential))
        .count();
    assert!(
        wiping > 0 && wiping < catalog.limits.rate_limits.len() * 2,
        "every budget answers the same thing at lockout, so the column proves nothing"
    );
    counted.floor(
        catalog.limits.rate_limits.len() * 12,
        "limits.md rate limits",
    );
}

#[test]
fn a_line_break_in_a_message_cannot_break_the_table_it_is_in() {
    // `cell()` collapses `\n`, `\r`, U+2028 and U+2029 to a space, and the
    // errors page's row comparison would notice one only incidentally, by the
    // row going missing. Without this test, removing the collapse while keeping
    // the pipe escape leaves the suite green, measured.
    //
    // Asserted as a property of every page rather than of one: a table row is a
    // line, so a value holding a line break ends the row early and every column
    // after it moves up.
    for name in [
        "errors.md",
        "notifications.md",
        "emails.md",
        "limits.md",
        "commands.md",
        "log-events.md",
    ] {
        for line in page(name).lines() {
            if !line.trim_start().starts_with('|') {
                continue;
            }
            let cells = line.trim_matches('|').split('|').count();
            assert!(
                cells >= 2,
                "docs/{name} has a table row with one cell, which is what a line break \
                 inside a value produces: {line:?}"
            );
        }
    }
    // And the collapse itself, over every message the catalog holds, since the
    // shipped registry has no line break in any of them.
    let catalog = catalog();
    for locale in &catalog.locales {
        for (key, message) in &locale.message {
            assert!(
                !message.contains(['\n', '\r', '\u{2028}', '\u{2029}']),
                "`{key}` in {} holds a line break, and every page renders it in a table \
                 row — so this is the case `cell()` exists for and the pages must be \
                 re-read after it lands",
                locale.meta.bcp47
            );
        }
    }
}

#[test]
fn every_section_on_every_page_is_an_entry_the_registry_declares() {
    // **The other direction.** The registry-to-page tests here walk the
    // registry and ask "is it on the page"; this one asks "is everything on the
    // page in the registry". Measured without it: an emitter adding one command section and
    // one error row that no registry declares published
    //
    //     ## `audit.disable`  | Action | `Read` | … | Keybinding | `C-c C-x` |
    //     **en** — Disable the audit log — Turns the command journal off.
    //     | `NC-JRNL-9999` | info | yes | Nothing is wrong | …
    //
    // with `check passed`. `docs.rs`'s own header claims the opposite — "a page
    // for a registry entry that no longer exists is reported" — and `§15.3`
    // makes these normative and the source the in-app pages will render.
    let catalog = catalog();
    let declared = |page: &str| -> Vec<String> {
        match page {
            "commands.md" => catalog.commands.iter().map(|e| e.name.clone()).collect(),
            "log-events.md" => catalog.events.iter().map(|e| e.name.clone()).collect(),
            "notifications.md" => catalog.notifications.iter().map(|e| e.id.clone()).collect(),
            "emails.md" => catalog.emails.iter().map(|e| e.id.clone()).collect(),
            other => panic!("no registry for {other}"),
        }
    };
    let mut checked = 0_usize;
    for name in [
        "commands.md",
        "log-events.md",
        "notifications.md",
        "emails.md",
    ] {
        let known = declared(name);
        for line in page(name).lines() {
            let Some(heading) = line.strip_prefix("## `") else {
                continue;
            };
            let entry = heading.trim_end_matches('`');
            assert!(
                known.iter().any(|declared| declared == entry),
                "docs/{name} publishes `{entry}`, which no registry declares"
            );
            checked += 1;
        }
    }

    // And the errors page, whose entries are rows rather than sections.
    let codes: Vec<&str> = catalog.errors.iter().map(|e| e.code.as_str()).collect();
    for row in rows(&page("errors.md")) {
        let Some(code) = row[0].strip_prefix('`').and_then(|c| c.strip_suffix('`')) else {
            continue;
        };
        if !code.starts_with("NC-") {
            continue;
        }
        assert!(
            codes.contains(&code),
            "docs/errors.md publishes `{code}`, which catalog/errors.toml does not declare"
        );
        checked += 1;
    }

    checked += every_row_of_the_limits_page_is_declared(&catalog);
    checked += every_row_of_the_capabilities_page_is_declared(&catalog);

    // **And `docs/log-events.md`'s field rows, in this direction too.** The
    // registry→page walk asserts a row exists for each declared field; the
    // prose pin drops every line starting `|`, and the heading pin reads header
    // rows only — so a table *body* row is outside every other mechanism.
    // Measured: one literal `| \`operator_note\` | free text | no |`
    // added after the field loop published eleven rows, one per event, on the
    // page whose own emitted header reads *"No field here can carry free
    // text"*, with `cargo xtask check` byte-identical to a clean run.
    checked += every_field_row_of_the_log_events_page_is_declared(&catalog);

    // And the errors page's Areas rows, which fall through the `NC-` filter.
    let areas: Vec<&str> = catalog.areas.keys().map(String::as_str).collect();
    // And its own floor, for the same reason.
    let mut judged_areas = 0_usize;
    let mut inside = false;
    for line in page("errors.md").lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            inside = heading.trim() == "Areas";
            continue;
        }
        if !inside {
            continue;
        }
        let Some(area) = line
            .strip_prefix("| `")
            .and_then(|rest| rest.split('`').next())
        else {
            continue;
        };
        assert!(
            areas.contains(&area),
            "docs/errors.md publishes the area `{area}`, which catalog/errors.toml does \
             not declare"
        );
        judged_areas += 1;
        checked += 1;
    }
    assert_eq!(
        judged_areas,
        areas.len(),
        "{judged_areas} area row(s) of `docs/errors.md` were judged and the registry \
         declares {}",
        areas.len()
    );

    assert!(
        checked >= catalog.commands.len() + catalog.events.len() + catalog.errors.len(),
        "only {checked} published entry(ies) were checked against a registry"
    );
}

/// Every field row `docs/log-events.md` publishes is one the registry declares.
///
/// Split out because the caller crossed a hundred lines with it inline. Returns
/// how many rows it judged, so the caller's floor still counts them.
fn every_field_row_of_the_log_events_page_is_declared(catalog: &Catalog) -> usize {
    // **Its own floor**, because the caller's is paid twice over by the other
    // sub-walks: sections and error rows contribute 44, limits 15, fields 23 and
    // areas 17, against a floor of 42. Any one of them can go to zero. Measured
    // with one character — `strip_prefix("| \`")` widened to `"|  \`"` — plus
    // the defect this walk exists for: eleven `| \`operator_note\` | free text |
    // no |` rows published on the page whose own header reads "No field here can
    // carry free text", and `check passed`.
    let mut fields = 0_usize;
    let events = page("log-events.md");
    let mut event = String::new();
    for line in events.lines() {
        if let Some(heading) = line.strip_prefix("## `") {
            heading.trim_end_matches('`').clone_into(&mut event);
            continue;
        }
        let Some(field) = line
            .strip_prefix("| `")
            .and_then(|rest| rest.split('`').next())
        else {
            continue;
        };
        let declared = catalog
            .events
            .iter()
            .find(|entry| entry.name == event)
            .is_some_and(|entry| entry.fields.iter().any(|one| one.name == field));
        assert!(
            declared,
            "docs/log-events.md publishes the field `{field}` under `{event}`, which \
             catalog/log-events.toml does not declare — and every field on that page is a \
             `LogValue`, so a row nothing declares is a kind the type wall does not have"
        );
        fields += 1;
    }
    let published: usize = catalog.events.iter().map(|entry| entry.fields.len()).sum();
    assert_eq!(
        fields, published,
        "{fields} field row(s) of `docs/log-events.md` were judged and the registry \
         declares {published}. A sub-walk that stops selecting rows is absorbed by the \
         caller's total, which the other three pay twice over"
    );
    fields
}

/// Every row `docs/capabilities.md` publishes is a capability or an interface
/// the registry declares.
///
/// The page→registry direction for the one page whose entries are rows of two
/// tables. Returns how many rows it judged, with a floor of its own for the
/// reason the limits walk below carries one.
fn every_row_of_the_capabilities_page_is_declared(catalog: &Catalog) -> usize {
    let mut judged = 0_usize;
    let mut section = String::new();
    for line in page("capabilities.md").lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            heading.trim().clone_into(&mut section);
            continue;
        }
        let Some(name) = line
            .strip_prefix("| `")
            .and_then(|rest| rest.split('`').next())
        else {
            continue;
        };
        let declared = match section.as_str() {
            "Granted on request" => catalog.capabilities.iter().any(|entry| entry.name == name),
            "Given to every plugin" => catalog.ungated.iter().any(|entry| entry.interface == name),
            other => panic!("docs/capabilities.md has a section `{other}` this walk does not know"),
        };
        assert!(
            declared,
            "docs/capabilities.md publishes `{name}` under `{section}`, which \
             catalog/capabilities.toml does not declare there"
        );
        judged += 1;
    }
    assert_eq!(
        judged,
        catalog.capabilities.len() + catalog.ungated.len(),
        "{judged} row(s) of `docs/capabilities.md` were judged against what the registry \
         declares"
    );
    judged
}

/// Every row `docs/limits.md` publishes is an entry the registry declares.
///
/// Split out because `every_section_on_every_page_is_an_entry_the_registry_declares`
/// crossed a hundred lines with it inline. Returns how many rows it judged, so
/// the caller's floor still counts them.
fn every_row_of_the_limits_page_is_declared(catalog: &Catalog) -> usize {
    // **Nothing walked `limits.md` in this direction at all.** Its
    // three tables are keyed by a name in the first cell and the only walk over
    // them goes registry → page; `Counted::floor` is derived from catalog sizes,
    // so an extra row raises no count. Measured, one literal row appended after
    // the tables loop in `docs.rs`:
    //
    // ```text
    // | `journal_entries_archive` | `oldest-first` | `recorded` | 1000000 | 5000 | — |
    //   ok    15 generated file(s) match the catalog
    //   ok    2 table(s) created in the migrations were checked …
    // ```
    //
    // A normative page publishing a row-bounded table that `catalog/limits.toml`
    // does not declare and `migrations/` does not create.
    //
    // The rate-limit table is keyed by a surface that appears twice — once per
    // budget — so its names are collected rather than matched one to one.
    let tables: Vec<&str> = catalog.limits.tables.keys().map(String::as_str).collect();
    let stores: Vec<&str> = catalog.limits.stores.keys().map(String::as_str).collect();
    let surfaces: Vec<&str> = catalog
        .limits
        .rate_limits
        .iter()
        .map(|limit| limit.surface.as_str())
        .collect();
    // `Hashing` is two named fields rather than a map, and the page writes the
    // purpose in its first cell — so the two names are written out here, which
    // is what a comparison needs anyway.
    //
    // **The page's spelling, and the rows are selected without backticks.** The
    // Key-derivation table writes `| password |` and `| device PIN |` as plain
    // literals, so a walk requiring a backtick reaches neither row and never
    // enters the arm below — and a list holding `device_pin`, the *registry
    // field* spelling, fails the moment the arm runs. The registry→page
    // direction above holds `"device PIN"` too.
    let derivations: [&str; 2] = ["password", "device PIN"];
    // The kernel's limits, by the key `[kernel]` writes each under.
    let kernel: Vec<&str> = catalog
        .limits
        .kernel
        .each()
        .into_iter()
        .map(|limit| limit.name)
        .collect();
    let mut judged = 0_usize;
    let mut section = String::new();
    for line in page("limits.md").lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            heading.trim().clone_into(&mut section);
            continue;
        }
        // Backticks stripped rather than required, so a table whose first cell
        // is a plain literal is selected too, and the separator row is not.
        let Some(name) = line
            .strip_prefix("| ")
            .and_then(|rest| rest.split(" |").next())
            .map(|cell| cell.trim().trim_matches('`'))
            // The header row and the separator under it are not entries. A
            // header's first cell is the column's name — `Table`, `Store`,
            // `Purpose`, `Surface` — and the row after it is dashes.
            .filter(|cell| {
                !cell.is_empty()
                    && !cell.starts_with("---")
                    && !matches!(*cell, "Table" | "Store" | "Purpose" | "Surface" | "Limit")
            })
        else {
            continue;
        };
        let known: &[&str] = match section.as_str() {
            "Tables" => &tables,
            "Stores outside PostgreSQL" => &stores,
            "Rate limits" => &surfaces,
            "Key derivation" => derivations.as_slice(),
            "Kernel" => kernel.as_slice(),
            other => panic!("docs/limits.md has a section `{other}` this walk does not know"),
        };
        assert!(
            known.contains(&name),
            "docs/limits.md publishes `{name}` under `{section}`, which \
             catalog/limits.toml does not declare"
        );
        judged += 1;
    }

    // **A floor inside the helper.** The caller's is
    // `checked >= commands + events + errors` = 42, against contributions of
    // fifteen sections, twenty-nine error rows, thirteen limits rows and
    // seventeen area rows — so any one sub-walk could go dark and the total
    // still paid it. Measured: a Key-derivation arm dead for exactly that
    // reason goes unreported without this floor.
    let declared = tables.len() + stores.len() + surfaces.len() + derivations.len() + kernel.len();
    assert!(
        judged >= declared,
        "{judged} row(s) of `docs/limits.md` were judged and the registry declares \
         {declared} across its five tables. A sub-walk that stops selecting rows is \
         absorbed by the caller's total"
    );
    judged
}

#[test]
fn each_page_names_the_registry_it_came_from() {
    // Every generated file's `Source:` header set to the bare directory leaves
    // the whole gate green without a check of it, and that line is what tells a
    // reader which registry to edit. The pages carry the same line, and this is
    // their check.
    for (name, source) in [
        ("errors.md", "catalog/errors.toml and catalog/i18n/"),
        (
            "notifications.md",
            "catalog/notifications.toml and catalog/i18n/",
        ),
        ("emails.md", "catalog/emails.toml and catalog/i18n/"),
        ("limits.md", "catalog/limits.toml"),
        ("commands.md", "catalog/commands.toml and catalog/i18n/"),
        ("log-events.md", "catalog/log-events.toml"),
        (
            "capabilities.md",
            "catalog/capabilities.toml and catalog/i18n/",
        ),
    ] {
        let text = page(name);
        // **The whole line, in its place**, like the Rust-side twin. A bare
        // `contains` over the page passes a header truncated to
        // `<!-- Source: catalog/. -->` with the real path buried in body prose —
        // and that line is the only thing telling a reader which registry to
        // edit.
        assert_eq!(
            text.lines().nth(1),
            Some(format!("<!-- Source: {source}. Run `cargo xtask docs`. -->").as_str()),
            "docs/{name} does not name {source} on its `Source:` line"
        );
        for named in source.split(" and ") {
            assert!(
                root().join(named.trim_end_matches('/')).exists(),
                "docs/{name} names {named}, which does not exist"
            );
        }
    }
}

/// Every sentence [`lockout_word`] publishes, over every variant and over the
/// absence of one.
///
/// Its own function because [`every_variant_is_published_as_the_word_it_names`]
/// is at clippy's line limit with it inline, which is the reason this file's
/// other helpers are functions too.
#[track_caller]
fn every_lockout_is_published_as_the_sentence_it_names() {
    // **The sixth table, whose spelled `refuse` arm no declared budget
    // reaches.** `catalog/limits.toml` spells `refuse` nowhere: of the four
    // budgets with a `lockout_after`, three declare no `on_lockout` and the
    // fourth is `wipe-credential`. So the loop in
    // `every_rate_limit_is_published_with_its_budget` compares
    // `(Some(_), Some(OnLockout::Refuse))` against nothing — measured, that half
    // of [`lockout_word`] corrupted on its own while the twelve sibling controls
    // in this file all die to the same treatment. Asked here over every variant
    // of the enum and over the absence of one, and over both answers to "is
    // there a lockout", which is the other input the column reads.
    for on_lockout in [OnLockout::Refuse, OnLockout::WipeCredential] {
        let meant = match on_lockout {
            OnLockout::Refuse => "further attempts are refused until the window passes",
            OnLockout::WipeCredential => {
                "the stored credential is destroyed, so the next sign-in is a full \
                 re-authentication"
            }
        };
        assert_eq!(
            lockout_word(Some(8), Some(on_lockout)),
            meant,
            "`{on_lockout:?}` is published as `{}`, and that sentence is the whole of \
             what a reader of `docs/limits.md` learns happens when a surface locks",
            lockout_word(Some(8), Some(on_lockout))
        );
        assert_eq!(
            lockout_word(None, Some(on_lockout)),
            "there is no lockout on this surface",
            "a budget with no `lockout_after` is published as `{}` because it spells \
             `{on_lockout:?}`, and six rows reading `Lockout: never` published a \
             consequence for a lockout they do not have",
            lockout_word(None, Some(on_lockout))
        );
    }
    // The absent `on_lockout`, which `model.rs` reads as refusing until the
    // window passes. Written apart from the spelled `refuse` for the reason
    // `what_a_lockout_does` gives: two decisions that agree today, and neither
    // can be changed without the other unless something asks them separately.
    assert_eq!(
        lockout_word(Some(8), None),
        "further attempts are refused until the window passes",
        "a lockout with no stated consequence is published as `{}`, and `model.rs` \
         reads an absent value as refusing until the window passes",
        lockout_word(Some(8), None)
    );
    assert_eq!(
        lockout_word(None, None),
        "there is no lockout on this surface",
        "a budget with neither a lockout nor a consequence is published as `{}`",
        lockout_word(None, None)
    );
}

/// Every word this file publishes for a variant, over every variant.
///
/// **The mirrors are only asserted where the registry reaches them.** Each
/// `*_word` here is read by a loop over the declared entries, so an arm no
/// entry reaches is compared against nothing — and some of them are in that
/// position today. `catalog/log-events.toml` declares no `trace` event, so
/// `level_word`'s first arm is dead; `catalog/commands.toml` declares `read`
/// and `write` and no `irreversible`, so `reversibility_word`'s third arm is
/// dead. Measured, `Irreversible` given
/// `"**yes**, the kernel reverses it each time"` here and in the emitter it
/// mirrors: `check passed`, every verdict line identical, with
/// `no_mutating_command_claims_more_undo_than_the_kernel_has` reading this very function.
///
/// **And [`kind_word`], which is the fifth table of this shape, with arms no
/// declared field reaches.** A kind no field in `catalog/log-events.toml` is
/// declared with — `I64` and `Id` are two — is an arm the loop in
/// `every_field_is_published_with_its_kind_and_whether_it_is_optional` compares
/// against nothing. Measured with four kinds undeclared and a distinct wrong
/// spelling on each of them — identical bodies trip clippy's `match_same_arms`,
/// so they have to differ — `17 passed` and `cargo xtask check` byte-identical
/// to a clean run; the same corruption on one *live* arm is a failure. Its
/// compiler-side twin, `every_field_kind_is_spelled_as_the_thing_it_names` in
/// `nc-catalog-compiler`'s `model.rs`, is written out for this reason.
/// `severity_word`, `strategy_word` and `raising_word` were each measured too,
/// and every arm of those three is reached.
///
/// **And [`lockout_word`], the sixth table, whose
/// `(Some(_), Some(OnLockout::Refuse))` arm is reached by no declared budget.**
/// `catalog/limits.toml` spells `refuse` on no budget at all — three of its four
/// `lockout_after` rows declare no `on_lockout` and the fourth is
/// `wipe-credential` — so the loop over the declared limits compares the spelled
/// half against nothing, and a mirror merging it into the absent one as a
/// single arm with two alternatives undoes exactly what the emitter it mirrors
/// is split apart to prevent.
///
/// Written out as exhaustive `match`es, the shape
/// `every_field_kind_generates_the_variant_it_names` uses: a variant added to
/// either enum is a compile error in this file rather than an arm that quietly
/// stops being checked.
#[test]
fn every_variant_is_published_as_the_word_it_names() {
    for level in [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
    ] {
        let meant = match level {
            Level::Trace => "trace",
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        };
        assert_eq!(
            level_word(level),
            meant,
            "`{level:?}` is published as `{}`, and the level a page prints is the \
             registry's own spelling",
            level_word(level)
        );
    }

    for reversibility in [
        Reversibility::Read,
        Reversibility::Write,
        Reversibility::Irreversible,
    ] {
        for the_history in [false, true] {
            let meant = match reversibility {
                Reversibility::Read => "nothing to undo — it changes nothing",
                Reversibility::Write if the_history => {
                    "yes, for what the kernel keeps — its `store` writes and the blobs it made \
                     — through `kernel.undo` and `kernel.redo`; a step that left the machine or \
                     cannot be undone, with no compensation registered for it, is one undo stops \
                     at, and a timer it armed or an event it published stays as it is (§11.5)"
                }
                Reversibility::Write => "no — a step that ran it is one undo stops at (§11.5)",
                Reversibility::Irreversible => "**no**, and a human confirms it each time (§14.6)",
            };
            assert_eq!(
                reversibility_word(reversibility, the_history),
                meant,
                "`{reversibility:?}` is published as `{}`",
                reversibility_word(reversibility, the_history)
            );
        }
    }

    for kind in [
        FieldType::U64,
        FieldType::I64,
        FieldType::U32,
        FieldType::Bool,
        FieldType::Duration,
        FieldType::ErrorCode,
        FieldType::Id,
        FieldType::Snowflake,
        FieldType::Literal,
        FieldType::Enum,
    ] {
        let meant = match kind {
            FieldType::U64 => "count",
            FieldType::I64 => "signed number",
            FieldType::U32 => "whole number, at most 32 bits wide",
            FieldType::Bool => "flag",
            FieldType::Duration => "span of time",
            FieldType::ErrorCode => "code from the error registry",
            FieldType::Id => "opaque identifier, written to the log exactly as it is",
            FieldType::Snowflake => "Snowflake, written as its Crockford base32 text",
            FieldType::Literal => "fixed text chosen when the code was written, never at run time",
            FieldType::Enum => "variant of an enum the platform declares — a state, never a name",
        };
        assert_eq!(
            kind_word(kind),
            meant,
            "`{kind:?}` is published as `{}`, and the word a page prints for a field's \
             kind is what a reader of `docs/log-events.md` learns the field holds",
            kind_word(kind)
        );
    }

    every_lockout_is_published_as_the_sentence_it_names();

    // **And the claim itself, on the arm no command reaches.**
    // `no_mutating_command_claims_more_undo_than_the_kernel_has` applies this test to every
    // *declared* command, which is what leaves the third variant out. A command
    // a human must confirm each time is not one this repository can undo,
    // whether or not the registry declares one today.
    let published = reversibility_word(Reversibility::Irreversible, false);
    assert!(
        REFUSALS.contains(&published),
        "an irreversible command would be published as `{published}`, which is not one \
         of the answers this repository is allowed to give — undo stops at such a \
         command rather than reversing it"
    );
}

/// The events `docs/log-events.md` says the kernel emits are the events the
/// kernel records, both ways.
///
/// **A pin cannot see an omission.** A sentence naming every `kernel.*` event,
/// `journal.verified` and `journal.broken` while
/// `crates/nc-kernel/src/dispatch.rs` records `net.refused` beside them is held
/// as written by the prose pin above, omission and all. Measured, with that
/// sentence on the page: this test red, with `net.refused` on the kernel's side
/// alone. So the names are read from both sides: every `nc_log::events::`
/// type a kernel source names, put back into the dotted name the catalog
/// gives it, against `kernel.*` and every event the sentence names.
#[test]
fn the_events_the_page_says_the_kernel_emits_are_the_ones_it_records() {
    fn sources(directory: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory)
            .expect("the kernel's sources are readable")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                sources(&path, into);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                into.push(path);
            }
        }
    }
    // The type the generated crate gives an event, written out here: each part
    // of the dotted name, and each word of it, capitalized.
    let type_of = |name: &str| -> String {
        name.split(['.', '_'])
            .map(|word| {
                let mut letters = word.chars();
                letters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(letters).collect()
                })
            })
            .collect()
    };
    let catalog = catalog();
    let mut files = Vec::new();
    sources(&root().join("crates/nc-kernel/src"), &mut files);
    assert!(files.len() > 10, "the kernel's sources were not found");
    let text: String = files
        .iter()
        .map(|file| std::fs::read_to_string(file).expect("a kernel source is readable"))
        .collect();
    let recorded: std::collections::BTreeSet<&str> = catalog
        .events
        .iter()
        .map(|entry| entry.name.as_str())
        .filter(|name| {
            let named = format!("nc_log::events::{}", type_of(name));
            text.match_indices(&named).any(|(at, _)| {
                !text[at + named.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
            })
        })
        .collect();
    assert!(
        recorded.contains("kernel.plugin_killed"),
        "no kernel event was found recorded, so this compares nothing: {recorded:?}"
    );
    let published = page("log-events.md");
    let sentence = published
        .lines()
        .find(|line| {
            line.starts_with("**`nc-kernel` is the one crate outside `nc-log` that logs.**")
        })
        .expect("the page says which crate logs");
    let emits = sentence
        .split_once("It emits ")
        .and_then(|(_, rest)| rest.split_once(';'))
        .map(|(named, _)| named)
        .expect("the sentence says what the kernel emits");
    let named: Vec<&str> = emits.split('`').skip(1).step_by(2).collect();
    assert!(named.contains(&"kernel.*"), "{sentence}");
    let said: std::collections::BTreeSet<&str> = catalog
        .events
        .iter()
        .map(|entry| entry.name.as_str())
        .filter(|name| name.starts_with("kernel.") || named.contains(name))
        .collect();
    for name in &named {
        assert!(
            *name == "kernel.*" || catalog.events.iter().any(|entry| entry.name == *name),
            "the page says the kernel emits `{name}`, which the catalog does not declare"
        );
    }
    assert_eq!(
        said, recorded,
        "`docs/log-events.md` says the kernel emits the first set and its sources \
         record the second: an event on one side only is a sentence a reader trusts \
         about a log line that is, or is not, there"
    );
}

/// What the limits page says holds its tables is there to hold them.
///
/// **The page names what runs its tables**: `crates/nc-local` refuses a commit
/// at every `refuse-at-limit` bound, and the kernel runs the pruner from
/// `Kernel::pump` and from `limits.prune`. Its opening names each, and this
/// holds each name to something that exists: the command in
/// the catalog, the method on the kernel, and the refusals in the store — so
/// the sentence goes red the day one of them goes, rather than outliving it.
#[test]
fn what_the_limits_page_says_holds_its_tables_exists() {
    let published = page("limits.md");
    let opening = published
        .lines()
        .find(|line| line.starts_with("**Declared is not the same as enforced"))
        .expect("the limits page says what is enforced");
    for named in [
        "`crates/nc-local`",
        "`Kernel::pump`",
        "`limits.prune`",
        "**Kernel**",
    ] {
        assert!(
            opening.contains(named),
            "the opening no longer names {named}: {opening}"
        );
    }
    assert!(
        catalog()
            .commands
            .iter()
            .any(|command| command.name == "limits.prune"),
        "the page names `limits.prune` and the catalog declares no such command"
    );
    let kernel = std::fs::read_to_string(root().join("crates/nc-kernel/src/lib.rs"))
        .expect("the kernel's root is readable");
    assert!(
        kernel.contains("pub async fn pump(&self) -> Pumped"),
        "the page names `Kernel::pump` and the kernel has no such method"
    );
    let store = std::fs::read_to_string(root().join("crates/nc-local/src/store.rs"))
        .expect("the local store is readable");
    for refusal in [
        "Quota {",
        "TooManyBlobs,",
        "more images or notes than one unit may record",
    ] {
        assert!(
            store.contains(refusal),
            "the page says `crates/nc-local` refuses at its bounds, and its store no longer \
             holds `{refusal}`"
        );
    }
    assert!(
        published.contains("## Kernel\n"),
        "the opening points at a **Kernel** section the page does not have"
    );
}
