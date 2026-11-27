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

use nc_catalog_compiler::ledger::Bootstrap;
use nc_catalog_compiler::model::{
    Catalog, Database, FieldType, Level, OnLockout, Owner, Raising, Reversibility, Severity,
    Strategy, Unit,
};
use nc_catalog_compiler::surfaces::{ChannelKeys, Cli};
use nc_catalog_compiler::{Surfaces, compile};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).to_path_buf()
}

/// The three channels, in the order their Leaders are settled; a
/// development channel's Leader is the same for every checkout.
fn channels() -> [nc_channel::Channel; 3] {
    [
        nc_channel::Channel::Production,
        nc_channel::Channel::Staging,
        nc_channel::Channel::Development(nc_channel::Checkout::of(&root())),
    ]
}

/// What `cargo xtask` hands the compiler, built as `xtask/src/build.rs` builds it.
///
/// The CLI's tables come from `nc_cli` and each channel's keys
/// from `nc_channel`.
fn surfaces() -> Surfaces {
    Surfaces {
        cli: Cli {
            name: "noctua".to_owned(),
            usage: nc_cli::help::usage("noctua"),
            verbs: nc_cli::args::VERBS
                .iter()
                .map(|verb| {
                    (
                        (*verb).to_owned(),
                        nc_cli::help::what_it_does(verb).to_owned(),
                    )
                })
                .collect(),
            exit_codes: nc_cli::exit::Exit::ALL
                .iter()
                .map(|exit| (exit.code(), exit.meaning().to_owned()))
                .collect(),
            described: nc_cli::help::described,
        },
        channels: channels()
            .iter()
            .map(|channel| ChannelKeys {
                channel: channel.word(),
                leader: channel.leader(),
                chord: channel.chord_modifier(),
            })
            .collect(),
    }
}

fn catalog() -> Catalog {
    compile(&root(), Bootstrap::Refuse, &surfaces())
        .expect("the catalog this repository ships compiles")
        .catalog
}

/// The pages `cargo xtask docs` writes.
const PAGES: [&str; 12] = [
    "capabilities.md",
    "categories.md",
    "cli.md",
    "commands.md",
    "emails.md",
    "errors.md",
    "external-identifiers.md",
    "keybindings.md",
    "limits.md",
    "log-events.md",
    "mcp-tools.md",
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
///
/// **And the answer of a `write` command undo takes back whole**, one whose
/// one effect is the value it writes in its own unit: without it the page
/// tells a reader that undo stops at MCP's switch, while the person's undo
/// puts it back as it was — confirmed by the person when that switches MCP on.
///
/// **And the answer of a `write` command that is no step of its own**, the
/// activation of a surface's node: without it the page tells a reader that
/// undo stops at a click, while the command the click runs is a step of its
/// own that undo takes back.
const REFUSALS: [&str; 5] = [
    "yes, for what the kernel keeps — its `store` writes and the blobs it made — through `kernel.undo` and `kernel.redo`; a step that left the machine or cannot be undone, with no compensation registered for it, is one undo stops at, and a timer it armed or an event it published stays as it is (§11.5)",
    "no — a step that ran it is one undo stops at (§11.5)",
    "**no**, and a human confirms it each time (§14.6)",
    "yes — its one effect is the value it writes, which `kernel.undo` puts back as it was and `kernel.redo` writes again — each waiting for a person's confirmation when it would switch MCP on (§11.5)",
    "as what it runs — no step of its own: the command it dispatches is a step of its own when it changes state, which `kernel.undo` takes back as that command's (§11.5)",
];

/// The two commands a person steps through their history with, whose step
/// the page says undo takes back for what the kernel keeps.
const THE_HISTORY: [&str; 2] = ["kernel.undo", "kernel.redo"];

/// The `write` commands of the kernel's own whose step the page says undo
/// takes back whole, since their one effect is the value they write.
const TAKEN_BACK_WHOLE: [&str; 1] = ["settings.mcp.set"];

/// The `write` commands of the kernel's own the page says are no step of
/// their own, the command they dispatch being its own step.
const AS_WHAT_IT_RUNS: [&str; 1] = ["surfaces.activate"];

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
/// when `the_history` and for one undo takes back whole when `taken_back_whole`.
const fn reversibility_word(
    reversibility: Reversibility,
    the_history: bool,
    taken_back_whole: bool,
    runs_another: bool,
) -> &'static str {
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
        Reversibility::Read => "nothing to undo — it makes no step, so undo passes over it (§11.5)",
        Reversibility::Write if the_history => {
            "yes, for what the kernel keeps — its `store` writes and the blobs it made — \
             through `kernel.undo` and `kernel.redo`; a step that left the machine or cannot \
             be undone, with no compensation registered for it, is one undo stops at, and a \
             timer it armed or an event it published stays as it is (§11.5)"
        }
        Reversibility::Write if taken_back_whole => {
            "yes — its one effect is the value it writes, which `kernel.undo` puts back as it \
             was and `kernel.redo` writes again — each waiting for a person's confirmation \
             when it would switch MCP on (§11.5)"
        }
        Reversibility::Write if runs_another => {
            "as what it runs — no step of its own: the command it dispatches is a step of its \
             own when it changes state, which `kernel.undo` takes back as that command's \
             (§11.5)"
        }
        Reversibility::Write => "no — a step that ran it is one undo stops at (§11.5)",
        Reversibility::Irreversible => "**no**, and a human confirms it each time (§14.6)",
    }
}

/// Every line of prose the six generated pages publish, page by page.
///
/// Pinned rather than derived, because prose is what the emitter authors: there
/// is nothing in the registry to compare it against, and that is exactly why
/// it has to be written down here to be read at all. A `static`, because it is
/// past what clippy inlines as a constant.
static PROSE: [(&str, &str); 3007] = [
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
        "categories.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "categories.md",
        "<!-- Source: catalog/categories.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("categories.md", "# Categories"),
    (
        "categories.md",
        "Every category an entry may be listed under, by set (`NOCTUA.md §20.4`). A set is a standard's — its categories carry the names the standard gives them, and the rank it gives them where it ranks them — or the platform's own. A category is named for its set, a dot and a slug, and what the `categories` of a command and of an external identifier name are categories on this page: the build refuses any other. `noctua help categories` lists them with no daemon, in the reader's language.",
    ),
    ("categories.md", "## `apple`"),
    ("categories.md", "## `freedesktop`"),
    ("categories.md", "## `noctua`"),
    (
        "cli.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "cli.md",
        "<!-- Source: apps/cli/src/args.rs, apps/cli/src/exit.rs, apps/cli/src/help.rs, apps/cli/src/render.rs, catalog/commands.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("cli.md", "# CLI"),
    (
        "cli.md",
        "What the command line reads and answers (`NOCTUA.md §20.2`): its grammar, its verbs, its exit codes, and what its help says of every command the catalog declares. The CLI is named for its channel (`§17.1`), and this page writes production's name, `noctua`.",
    ),
    (
        "cli.md",
        "**A verb is one word and every command's name is dotted**, so the two never meet: a verb is what the CLI does itself rather than a command it is asked to dispatch — help, its own channel's daemon started, stopped and asked how it is, the development checkouts, and two that carry dispatches: `mcp`, which serves an MCP client and sends each call it makes to be decided as its session's agent, and `leader`, which announces the graphical environment a compositor's binding runs it in and dispatches the Leader's tap as the person — while a command is dispatched by the daemon, decided and journaled. **Each command below is what help prints of it**, in each language the environment can name, laid out by the one function help lays it out with: the CLI's own words in English, the command's texts from its entry. A plugin's commands are not on this page: the daemon's registry holds them while the plugin is loaded, and help lists them from there.",
    ),
    ("cli.md", "## Usage"),
    ("cli.md", "```text"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua [--json] <command> [arguments]"),
    ("cli.md", "  noctua [--json] checkouts [<checkout>]"),
    ("cli.md", "  noctua mcp <agent>"),
    (
        "cli.md",
        "  noctua [--json] help [<command> | <verb> | exit-codes | categories | external-identifiers | keybindings]",
    ),
    ("cli.md", "Options, before the command:"),
    (
        "cli.md",
        "  --json   write the answer as one JSON document on standard output",
    ),
    ("cli.md", "  --help   ask for help"),
    (
        "cli.md",
        "Arguments follow a command's name: `--<name> <value>`, `--<name>=<value>`, `--<boolean>` or `--<boolean>=false`, a list's flag once for each item, or `--` and one JSON object.",
    ),
    ("cli.md", "```"),
    ("cli.md", "## Verbs"),
    ("cli.md", "## Exit codes"),
    ("cli.md", "## `agents.create`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.create — Make an agent"),
    (
        "cli.md",
        "Make an agent that acts for you, with a budget of its own.",
    ),
    (
        "cli.md",
        "Makes a new agent of yours: an identity of its own that an MCP client acts as, for you alone. You give it a label and a budget, which each of its sessions spends one at a time, and it can do nothing until you grant it a feature. Only you can make one, asking directly, never from inside another action. An agent is never deleted, so undo stops at the step that made it.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agents, make an agent, new agent, mcp, assistant, delegate",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.create [arguments]"),
    ("cli.md", "  noctua agents.create -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --label <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --budget <whole number, from 1 to 100000>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.create — Criar um agente"),
    (
        "cli.md",
        "Criar um agente que age por você, com um orçamento próprio.",
    ),
    (
        "cli.md",
        "Cria um novo agente seu: uma identidade própria, com a qual um cliente MCP age só por você. Você lhe dá um nome e um orçamento, que cada sessão dele gasta uma unidade por vez, e ele não pode fazer nada até que você lhe dê acesso a um recurso. Só você pode criar um, pedindo diretamente, nunca de dentro de outra ação. Um agente nunca é apagado, então desfazer para no passo que o criou.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agentes, criar agente, novo agente, mcp, assistente, delegar",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.create [arguments]"),
    ("cli.md", "  noctua agents.create -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --label <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --budget <whole number, from 1 to 100000>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `agents.grant`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.grant — Grant an agent a feature"),
    (
        "cli.md",
        "Let one of your agents use a feature, for a reason and a time you give.",
    ),
    (
        "cli.md",
        "Lets one of your agents, named by the identifier Noctua gave it, use one feature for as many minutes as you say, within the longest a grant may run, and keeps why. A new grant on the same feature ends the one before it, and the agent still does only what you yourself may do. Only you can grant it, asking directly, never from inside another action. To take it back, revoke it: undo stops at a grant.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agents, grant, give access, allow, mcp, delegate",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.grant [arguments]"),
    ("cli.md", "  noctua agents.grant -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --agent <text, from 12 to 13 characters>  required",
    ),
    (
        "cli.md",
        "  --feature <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --minutes <whole number, from 1 to 43200>  required",
    ),
    (
        "cli.md",
        "  --reason <text, from 1 to 256 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "agents.grant — Dar a um agente acesso a um recurso",
    ),
    (
        "cli.md",
        "Deixar um agente seu usar um recurso, por um motivo e um prazo que você dá.",
    ),
    (
        "cli.md",
        "Deixa um agente seu, indicado pelo identificador que o Noctua lhe deu, usar um recurso por quantos minutos você disser, até o prazo mais longo que uma permissão pode ter, e guarda o motivo. Uma nova permissão no mesmo recurso encerra a anterior, e o agente continua fazendo só o que você mesmo pode fazer. Só você pode dar a permissão, pedindo diretamente, nunca de dentro de outra ação. Para retirá-la, revogue: desfazer para numa permissão dada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agentes, dar acesso, permissão, permitir, mcp, delegar",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.grant [arguments]"),
    ("cli.md", "  noctua agents.grant -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --agent <text, from 12 to 13 characters>  required",
    ),
    (
        "cli.md",
        "  --feature <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --minutes <whole number, from 1 to 43200>  required",
    ),
    (
        "cli.md",
        "  --reason <text, from 1 to 256 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `agents.list`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.list — List your agents"),
    (
        "cli.md",
        "Show the agents you made, each with what you granted it and until when.",
    ),
    (
        "cli.md",
        "Answers every agent you made, in the order you made them, with its label, its budget and each grant you gave it and have not ended, even one whose time is up: its feature, why you gave it, and from when until when. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agents, my agents, list agents, grants, mcp",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.list [arguments]"),
    ("cli.md", "  noctua agents.list -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.list — Listar seus agentes"),
    (
        "cli.md",
        "Mostrar os agentes que você criou, cada um com o que lhe deu e até quando.",
    ),
    (
        "cli.md",
        "Responde cada agente que você criou, na ordem em que os criou, com o nome, o orçamento e cada permissão que você lhe deu e não encerrou, mesmo a que já venceu: o recurso, o motivo e de quando até quando. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agentes, meus agentes, listar agentes, permissões, mcp",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.list [arguments]"),
    ("cli.md", "  noctua agents.list -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `agents.revoke`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.revoke — Revoke an agent's access"),
    (
        "cli.md",
        "End what you granted one of your agents: one feature, or all of them.",
    ),
    (
        "cli.md",
        "Ends the grant one of your agents holds on the feature you name, or every grant it holds when you name none, from its very next action. The agent itself is kept, with its budget, for you to grant again. Only you can revoke, asking directly, never from inside another action, and undo stops at a revocation.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agents, revoke, take access away, end a grant, mcp",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.revoke [arguments]"),
    ("cli.md", "  noctua agents.revoke -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --agent <text, from 12 to 13 characters>  required",
    ),
    ("cli.md", "  --feature <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "agents.revoke — Revogar o acesso de um agente"),
    (
        "cli.md",
        "Encerrar o que você deu a um agente seu: um recurso, ou todos.",
    ),
    (
        "cli.md",
        "Encerra a permissão que um agente seu tem no recurso que você indicar, ou todas as que ele tem se você não indicar nenhum, a partir da próxima ação dele. O agente continua guardado, com o orçamento, para você lhe dar acesso de novo. Só você pode revogar, pedindo diretamente, nunca de dentro de outra ação, e desfazer para numa revogação.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: agentes, revogar, tirar acesso, encerrar permissão, mcp",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua agents.revoke [arguments]"),
    ("cli.md", "  noctua agents.revoke -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --agent <text, from 12 to 13 characters>  required",
    ),
    ("cli.md", "  --feature <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "## `confirmations.list`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "confirmations.list — List what waits for a confirmation",
    ),
    (
        "cli.md",
        "Show every call waiting for a person to confirm it in the Noctua window.",
    ),
    (
        "cli.md",
        "Lists every call Noctua holds until a person confirms it in the Noctua window — one that cannot be undone, or one that switches MCP on: its number, the command, whether it can be undone, the person whose confirmation it waits for and the agent that asked for it when an agent did, and when the wait ends. Only the Noctua window can confirm or decline one; this command only shows them, and changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: confirmation, confirm, waiting, irreversible, mcp, pending, approve",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua confirmations.list [arguments]"),
    (
        "cli.md",
        "  noctua confirmations.list -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "confirmations.list — Listar o que aguarda confirmação",
    ),
    (
        "cli.md",
        "Mostrar cada chamada que aguarda uma pessoa confirmá-la na janela do Noctua.",
    ),
    (
        "cli.md",
        "Lista cada chamada que o Noctua segura até que uma pessoa a confirme na janela do Noctua — uma que não pode ser desfeita, ou uma que liga o MCP: seu número, o comando, se pode ser desfeita, a pessoa cuja confirmação ela aguarda e o agente que a pediu, quando foi um agente, e quando a espera termina. Só a janela do Noctua pode confirmar ou recusar uma delas; este comando apenas as mostra e não altera nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: confirmação, confirmar, aguardando, irreversível, mcp, pendente, aprovar",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua confirmations.list [arguments]"),
    (
        "cli.md",
        "  noctua confirmations.list -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `confirmations.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "confirmations.read — Read what waits for a confirmation",
    ),
    (
        "cli.md",
        "Show one call waiting for a confirmation, with the arguments it would run with.",
    ),
    (
        "cli.md",
        "Shows the call Noctua holds under the number given until a person confirms it in the Noctua window: the command, the arguments it would run with, the person whose confirmation it waits for, the agent that asked for it when an agent did, and when the wait ends. A number nothing waits under any more is refused. It changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: confirmation, confirm, waiting, irreversible, arguments, details",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua confirmations.read [arguments]"),
    (
        "cli.md",
        "  noctua confirmations.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --serial <whole number, at least 1>  required"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "confirmations.read — Ler o que aguarda confirmação",
    ),
    (
        "cli.md",
        "Mostrar uma chamada que aguarda confirmação, com os argumentos com que rodaria.",
    ),
    (
        "cli.md",
        "Mostra a chamada que o Noctua segura sob o número dado até que uma pessoa a confirme na janela do Noctua: o comando, os argumentos com que rodaria, a pessoa cuja confirmação ela aguarda, o agente que a pediu, quando foi um agente, e quando a espera termina. Um número sob o qual nada mais aguarda é recusado. Não altera nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: confirmação, confirmar, aguardando, irreversível, argumentos, detalhes",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua confirmations.read [arguments]"),
    (
        "cli.md",
        "  noctua confirmations.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --serial <whole number, at least 1>  required"),
    ("cli.md", "```"),
    ("cli.md", "## `drawer.back`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.back — Go back in the drawer"),
    (
        "cli.md",
        "Leave a sub-page of the drawer's page, or close that page if it has none.",
    ),
    (
        "cli.md",
        "Does what Esc does in the drawer: it goes back one step inside the drawer's page in focus or, when that page is not inside a sub-page, closes it in the drawer; a seat whose keyboard was on a drawer left showing nothing goes back to the prompt line.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: drawer, back, escape, close, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.back [arguments]"),
    ("cli.md", "  noctua drawer.back -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.back — Voltar na gaveta"),
    (
        "cli.md",
        "Sair de uma subpágina da página da gaveta, ou fechar a página se não houver.",
    ),
    (
        "cli.md",
        "Faz o que o Esc faz na gaveta: volta um passo dentro da página em foco na gaveta ou, quando essa página não está numa subpágina, fecha-a na gaveta; um assento cujo teclado estava numa gaveta que fica sem mostrar nada volta para a linha do prompt.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: gaveta, voltar, esc, fechar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.back [arguments]"),
    ("cli.md", "  noctua drawer.back -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `drawer.collapse`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.collapse — Collapse the drawer"),
    (
        "cli.md",
        "Hide the whole drawer, keeping every page open in it.",
    ),
    (
        "cli.md",
        "Hides the drawer below the footer, keeping the pages open in it; the end of the modeline then shows how many there are. Expanding it brings everything back.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: drawer, collapse, hide, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.collapse [arguments]"),
    ("cli.md", "  noctua drawer.collapse -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.collapse — Recolher a gaveta"),
    (
        "cli.md",
        "Esconder a gaveta inteira, mantendo abertas todas as páginas dela.",
    ),
    (
        "cli.md",
        "Esconde a gaveta abaixo do rodapé, mantendo abertas as páginas que há nela; o fim da linha de modo passa a mostrar quantas são. Expandi-la traz tudo de volta.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: gaveta, recolher, esconder, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.collapse [arguments]"),
    ("cli.md", "  noctua drawer.collapse -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `drawer.expand`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.expand — Expand the drawer"),
    (
        "cli.md",
        "Show the drawer again, with everything that was open in it.",
    ),
    (
        "cli.md",
        "Shows the drawer below the footer again, as it was when it was collapsed.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: drawer, expand, show, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.expand [arguments]"),
    ("cli.md", "  noctua drawer.expand -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.expand — Expandir a gaveta"),
    (
        "cli.md",
        "Mostrar a gaveta de novo, com tudo o que estava aberto nela.",
    ),
    (
        "cli.md",
        "Mostra de novo a gaveta abaixo do rodapé, como estava quando foi recolhida.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: gaveta, expandir, mostrar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.expand [arguments]"),
    ("cli.md", "  noctua drawer.expand -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `drawer.resize`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.resize — Resize the drawer"),
    (
        "cli.md",
        "Set how much of the height the drawer takes, below half.",
    ),
    (
        "cli.md",
        "Sets how much of the height the drawer shares with the workspace area it takes, in percent. It always stays shorter than the workspace area.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: drawer, resize, height, size, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.resize [arguments]"),
    ("cli.md", "  noctua drawer.resize -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --share <whole number, from 1 to 49>  required"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.resize — Redimensionar a gaveta"),
    (
        "cli.md",
        "Definir quanto da altura a gaveta ocupa, sempre menos da metade.",
    ),
    (
        "cli.md",
        "Define quanto da altura que a gaveta divide com a área de trabalho ela ocupa, em porcentagem. Ela sempre fica mais baixa que a área de trabalho.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: gaveta, redimensionar, altura, tamanho, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.resize [arguments]"),
    ("cli.md", "  noctua drawer.resize -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --share <whole number, from 1 to 49>  required"),
    ("cli.md", "```"),
    ("cli.md", "## `drawer.toward`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.toward — Choose where pages open"),
    (
        "cli.md",
        "Open the leader menu's pages in the workspace area or in the drawer.",
    ),
    (
        "cli.md",
        "Chooses where a page opened from the leader menu goes: the workspace area, where they go at first, or the drawer. The choice is kept until you change it.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: drawer, direction, workspace, leader menu, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.toward [arguments]"),
    ("cli.md", "  noctua drawer.toward -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --region <one of: workspace, drawer>  required"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "drawer.toward — Escolher onde as páginas abrem"),
    (
        "cli.md",
        "Abrir as páginas do menu do Leader na área de trabalho ou na gaveta.",
    ),
    (
        "cli.md",
        "Escolhe para onde vai uma página aberta pelo menu do Leader: a área de trabalho, para onde elas vão no início, ou a gaveta. A escolha fica guardada até você mudá-la.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: gaveta, direção, área de trabalho, menu do leader, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua drawer.toward [arguments]"),
    ("cli.md", "  noctua drawer.toward -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --region <one of: workspace, drawer>  required"),
    ("cli.md", "```"),
    ("cli.md", "## `footer.hide`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "footer.hide — Hide the footer"),
    (
        "cli.md",
        "Let the footer hide again where the setting hides it.",
    ),
    (
        "cli.md",
        "Lets the footer hide again where it is set to hide. Where it is set to always show, it stays.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: footer, hide, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua footer.hide [arguments]"),
    ("cli.md", "  noctua footer.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "footer.hide — Esconder o rodapé"),
    (
        "cli.md",
        "Deixar o rodapé se esconder de novo onde a configuração o esconde.",
    ),
    (
        "cli.md",
        "Deixa o rodapé se esconder de novo onde ele está configurado para isso. Onde está configurado para aparecer sempre, ele fica.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: rodapé, esconder, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua footer.hide [arguments]"),
    ("cli.md", "  noctua footer.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `footer.show`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "footer.show — Show the footer"),
    (
        "cli.md",
        "Show the footer's three lines now, even where the setting hides it.",
    ),
    (
        "cli.md",
        "Shows the footer's three lines now. Where the footer is set to hide, it stays until nothing holds it there any more.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: footer, show, modeline, prompt, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua footer.show [arguments]"),
    ("cli.md", "  noctua footer.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "footer.show — Mostrar o rodapé"),
    (
        "cli.md",
        "Mostrar agora as três linhas do rodapé, mesmo onde a configuração o esconde.",
    ),
    (
        "cli.md",
        "Mostra agora as três linhas do rodapé. Onde o rodapé está configurado para se esconder, ele fica até que nada mais o segure ali.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: rodapé, mostrar, linha de modo, prompt, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua footer.show [arguments]"),
    ("cli.md", "  noctua footer.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `info.clear`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.clear — Clear the info line"),
    ("cli.md", "Take the message off the info line."),
    (
        "cli.md",
        "Takes the message off the info line, which goes back to saying what each seat is on.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: info line, clear, message, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.clear [arguments]"),
    ("cli.md", "  noctua info.clear -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.clear — Limpar a linha de informações"),
    ("cli.md", "Tirar a mensagem da linha de informações."),
    (
        "cli.md",
        "Tira a mensagem da linha de informações, que volta a dizer onde cada assento está.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: linha de informações, limpar, mensagem, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.clear [arguments]"),
    ("cli.md", "  noctua info.clear -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `info.collapse`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.collapse — Collapse the info line"),
    ("cli.md", "Keep the info line to one line again."),
    (
        "cli.md",
        "Keeps the info line to one line again, cutting what does not fit.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: info line, collapse, less, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.collapse [arguments]"),
    ("cli.md", "  noctua info.collapse -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.collapse — Recolher a linha de informações"),
    (
        "cli.md",
        "Manter a linha de informações numa linha só de novo.",
    ),
    (
        "cli.md",
        "Mantém a linha de informações numa linha só de novo, cortando o que não couber.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: linha de informações, recolher, menos, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.collapse [arguments]"),
    ("cli.md", "  noctua info.collapse -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `info.expand`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.expand — Expand the info line"),
    (
        "cli.md",
        "Let the info line grow to show all it holds, each hover on a line of its own.",
    ),
    (
        "cli.md",
        "Lets the info line wrap and grow to show everything it holds, with what the pointer is over and what the keyboard is on each on a line of its own.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: info line, expand, more, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.expand [arguments]"),
    ("cli.md", "  noctua info.expand -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.expand — Expandir a linha de informações"),
    (
        "cli.md",
        "Deixar a linha de informações crescer para mostrar tudo o que tem.",
    ),
    (
        "cli.md",
        "Deixa a linha de informações quebrar e crescer para mostrar tudo o que tem, com o que está sob o ponteiro e onde está o teclado, cada um numa linha própria.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: linha de informações, expandir, mais, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.expand [arguments]"),
    ("cli.md", "  noctua info.expand -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `info.say`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.say — Say something on the info line"),
    (
        "cli.md",
        "Show a short message on the info line until it is cleared.",
    ),
    (
        "cli.md",
        "Shows a message on the info line, the middle line of the footer, in place of the one before it.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: info line, message, notify, say, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.say [arguments]"),
    ("cli.md", "  noctua info.say -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --text <text, from 1 to 4096 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "info.say — Dizer algo na linha de informações"),
    (
        "cli.md",
        "Mostrar uma mensagem curta na linha de informações até que seja limpa.",
    ),
    (
        "cli.md",
        "Mostra uma mensagem na linha de informações, a linha do meio do rodapé, no lugar da anterior.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: linha de informações, mensagem, avisar, dizer, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua info.say [arguments]"),
    ("cli.md", "  noctua info.say -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --text <text, from 1 to 4096 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `journal.verify`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "journal.verify — Verify activity history"),
    (
        "cli.md",
        "Check that every recorded action is intact and in order.",
    ),
    (
        "cli.md",
        "Reads the activity history from its start, or from the entry you name, and checks that each recorded action is still linked to the one before it. It changes nothing, and answers how many entries it checked, or the first one that does not hold and why.",
    ),
    ("cli.md", "Categories: noctua.history"),
    (
        "cli.md",
        "Tags: verify, check, history, activity, integrity, audit",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua journal.verify [arguments]"),
    ("cli.md", "  noctua journal.verify -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --from <whole number, at least 0>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "journal.verify — Verificar histórico de atividades",
    ),
    (
        "cli.md",
        "Conferir que toda ação registrada está intacta e em ordem.",
    ),
    (
        "cli.md",
        "Lê o histórico de atividades desde o início, ou desde a entrada que você indicar, e confere se cada ação registrada continua ligada à anterior. Não muda nada, e responde quantas entradas conferiu, ou a primeira que não confere e por quê.",
    ),
    ("cli.md", "Categories: noctua.history"),
    (
        "cli.md",
        "Tags: verificar, conferir, histórico, atividade, integridade, auditoria",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua journal.verify [arguments]"),
    ("cli.md", "  noctua journal.verify -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --from <whole number, at least 0>"),
    ("cli.md", "```"),
    ("cli.md", "## `kernel.find`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.find — Find a command"),
    (
        "cli.md",
        "Find commands by any word of their names or tags, in English or Portuguese.",
    ),
    (
        "cli.md",
        "Looks through every command Noctua can run, its own and every plugin's, for those whose names and tags, in English and in Portuguese, hold every word you type, each as letters in order, with accents and capitals ignored. It answers the best matches first, at most as many as you ask for, with where each word was found, and how many were found in all. It changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: search, filter, look up, list of commands"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.find [arguments]"),
    ("cli.md", "  noctua kernel.find -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --query <text, at most 64 characters>  required",
    ),
    ("cli.md", "  --limit <whole number, from 1 to 32>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.find — Encontrar um comando"),
    (
        "cli.md",
        "Encontrar comandos pelas palavras do nome ou das etiquetas, nas duas línguas.",
    ),
    (
        "cli.md",
        "Procura, entre todos os comandos que o Noctua pode executar, os dele e os de cada plugin, aqueles cujos nomes e etiquetas, em inglês e em português, contêm cada palavra digitada, com as letras na ordem, sem distinguir acentos nem maiúsculas. Responde primeiro os melhores resultados, no máximo quantos você pedir, com onde cada palavra foi encontrada e quantos foram encontrados ao todo. Não muda nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: procurar, pesquisar, buscar, filtrar, comandos disponíveis",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.find [arguments]"),
    ("cli.md", "  noctua kernel.find -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --query <text, at most 64 characters>  required",
    ),
    ("cli.md", "  --limit <whole number, from 1 to 32>"),
    ("cli.md", "```"),
    ("cli.md", "## `kernel.history`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.history — Show the next undo"),
    (
        "cli.md",
        "Name the step your next undo takes back, and whether an agent took it.",
    ),
    (
        "cli.md",
        "Shows the step your next undo would take back: its name, when it was taken, whether undo stops there, and the agent that took it when one of your agents did — an agent's steps are yours to undo, as your own are. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.history"),
    ("cli.md", "Tags: history, undo, next step, agent, steps"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.history [arguments]"),
    ("cli.md", "  noctua kernel.history -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.history — Mostrar o próximo desfazer"),
    (
        "cli.md",
        "Dizer qual passo o seu próximo desfazer desfaz, e se um agente o deu.",
    ),
    (
        "cli.md",
        "Mostra o passo que o seu próximo desfazer desfaria: seu nome, quando foi dado, se o desfazer para ali e o agente que o deu, quando foi um dos seus agentes — os passos de um agente são seus para desfazer, como os seus. Não altera nada.",
    ),
    ("cli.md", "Categories: noctua.history"),
    (
        "cli.md",
        "Tags: histórico, desfazer, próximo passo, agente, passos",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.history [arguments]"),
    ("cli.md", "  noctua kernel.history -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `kernel.redo`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.redo — Redo"),
    ("cli.md", "Make again the last step you took back."),
    (
        "cli.md",
        "Puts back the newest step you took back, as long as you have done nothing new since, after checking again that you may do each thing it did. A step whose undo needed corrections of its own cannot be redone.",
    ),
    ("cli.md", "Categories: noctua.history"),
    ("cli.md", "Tags: redo, do again, repeat, restore"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.redo [arguments]"),
    ("cli.md", "  noctua kernel.redo -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.redo — Refazer"),
    ("cli.md", "Fazer de novo o último passo que você desfez."),
    (
        "cli.md",
        "Refaz o passo mais recente que você desfez, desde que você não tenha feito nada novo depois, conferindo de novo que você pode fazer cada coisa que ele fez. Um passo cujo desfazer precisou de correções próprias não pode ser refeito.",
    ),
    ("cli.md", "Categories: noctua.history"),
    ("cli.md", "Tags: refazer, fazer de novo, repetir, restaurar"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.redo [arguments]"),
    ("cli.md", "  noctua kernel.redo -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `kernel.undo`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.undo — Undo"),
    (
        "cli.md",
        "Take back your last step: what it changed in Noctua, and the files it made.",
    ),
    (
        "cli.md",
        "Finds your newest step that can still be taken back and puts back every value it changed and the files it made, after checking again that you may do each thing it did. It stops at a step whose effects left this device, and at one whose values changed since.",
    ),
    ("cli.md", "Categories: noctua.history"),
    ("cli.md", "Tags: undo, take back, revert, go back"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.undo [arguments]"),
    ("cli.md", "  noctua kernel.undo -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "kernel.undo — Desfazer"),
    (
        "cli.md",
        "Desfazer o seu último passo: o que ele mudou no Noctua e os arquivos que criou.",
    ),
    (
        "cli.md",
        "Encontra o seu passo mais recente que ainda pode ser desfeito e devolve cada valor que ele mudou e os arquivos que criou, depois de conferir de novo que você pode fazer cada coisa que ele fez. Para em um passo cujos efeitos saíram deste aparelho, e em um cujos valores mudaram desde então.",
    ),
    ("cli.md", "Categories: noctua.history"),
    ("cli.md", "Tags: desfazer, voltar atrás, reverter, anular"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua kernel.undo [arguments]"),
    ("cli.md", "  noctua kernel.undo -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `leader.tap`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "leader.tap — Tap the Leader"),
    (
        "cli.md",
        "Do what one tap of the Leader key does in the interface.",
    ),
    (
        "cli.md",
        "Does what a tap of the Leader does: it shows the interface and its footer and puts the keyboard on the prompt line; tapped again there, it opens the leader menu in the drawer; and tapped while the menu is open in a window shown, it sends the next page opened to the other region — the drawer instead of the workspace area, or the other way round.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: leader, leader key, shortcut, menu, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua leader.tap [arguments]"),
    ("cli.md", "  noctua leader.tap -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "leader.tap — Tocar a tecla Leader"),
    (
        "cli.md",
        "Fazer o que um toque da tecla Leader faz na interface.",
    ),
    (
        "cli.md",
        "Faz o que um toque da tecla Leader faz: mostra a interface e o rodapé e leva o teclado para a linha do prompt; tocada de novo ali, abre o menu do Leader na gaveta; e tocada com o menu aberto numa janela à vista, manda a próxima página aberta para a outra região — a gaveta em vez da área de trabalho, ou o contrário.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: leader, tecla leader, atalho, menu, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua leader.tap [arguments]"),
    ("cli.md", "  noctua leader.tap -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "```"),
    ("cli.md", "## `limits.prune`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "limits.prune — Clear old records"),
    (
        "cli.md",
        "Remove the oldest records from any table that has passed its limit.",
    ),
    (
        "cli.md",
        "Deletes the oldest records from each table of this device's database that has passed its declared limit, or only from the table you name. A dry run counts what it would remove and removes nothing. What it removes cannot be brought back, so undo stops at the step that ran it.",
    ),
    ("cli.md", "Categories: noctua.maintenance"),
    (
        "cli.md",
        "Tags: prune, clean up, old records, limit, free space",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua limits.prune [arguments]"),
    ("cli.md", "  noctua limits.prune -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --table <text>"),
    (
        "cli.md",
        "  --dry_run  (a boolean: true when given alone, `=false` says false)",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "limits.prune — Limpar registros antigos"),
    (
        "cli.md",
        "Remover os registros mais antigos de qualquer tabela que passou do limite.",
    ),
    (
        "cli.md",
        "Apaga os registros mais antigos de cada tabela do banco de dados deste aparelho que passou do limite declarado, ou só da tabela que você indicar. Um ensaio conta o que removeria e não remove nada. O que ele remove não pode ser recuperado, então desfazer para no passo que o executou.",
    ),
    ("cli.md", "Categories: noctua.maintenance"),
    (
        "cli.md",
        "Tags: limpar, apagar, registros antigos, limite, liberar espaço",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua limits.prune [arguments]"),
    ("cli.md", "  noctua limits.prune -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --table <text>"),
    (
        "cli.md",
        "  --dry_run  (a boolean: true when given alone, `=false` says false)",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `naming.propose`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "naming.propose — Propose a file name"),
    (
        "cli.md",
        "Build a file name from a title, signature, keywords and extension.",
    ),
    (
        "cli.md",
        "Writes the name Noctua gives a saved file: its signature, title, keywords, identifier and extension, each cleaned of what a file name cannot hold. A file being renamed keeps the identifier you give, so it stays the same file; without one, a new identifier is made. It saves nothing and changes nothing.",
    ),
    ("cli.md", "Categories: noctua.files"),
    (
        "cli.md",
        "Tags: file name, name a file, rename, save as, propose",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua naming.propose [arguments]"),
    ("cli.md", "  noctua naming.propose -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --title <text, at most 1024 characters>"),
    (
        "cli.md",
        "  --signature <text, at most 255 characters>  (repeated, once for each item, at most 128 items)",
    ),
    (
        "cli.md",
        "  --keywords <text, at most 255 characters>  (repeated, once for each item, at most 128 items)",
    ),
    ("cli.md", "  --extension <text, at most 255 characters>"),
    ("cli.md", "  --id <text, from 1 to 13 characters>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "naming.propose — Propor um nome de arquivo"),
    (
        "cli.md",
        "Montar o nome de um arquivo com título, assinatura, palavras-chave e extensão.",
    ),
    (
        "cli.md",
        "Escreve o nome que o Noctua dá a um arquivo salvo: a assinatura, o título, as palavras-chave, o identificador e a extensão, cada um limpo do que um nome de arquivo não comporta. Um arquivo renomeado mantém o identificador que você informar e continua sendo o mesmo arquivo; sem ele, um identificador novo é criado. Não salva nada e não muda nada.",
    ),
    ("cli.md", "Categories: noctua.files"),
    (
        "cli.md",
        "Tags: nome de arquivo, nomear arquivo, renomear, salvar como, propor",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua naming.propose [arguments]"),
    ("cli.md", "  noctua naming.propose -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --title <text, at most 1024 characters>"),
    (
        "cli.md",
        "  --signature <text, at most 255 characters>  (repeated, once for each item, at most 128 items)",
    ),
    (
        "cli.md",
        "  --keywords <text, at most 255 characters>  (repeated, once for each item, at most 128 items)",
    ),
    ("cli.md", "  --extension <text, at most 255 characters>"),
    ("cli.md", "  --id <text, from 1 to 13 characters>"),
    ("cli.md", "```"),
    ("cli.md", "## `naming.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "naming.read — Read a file name"),
    (
        "cli.md",
        "Read a file name back into its identifier, signature, title and keywords.",
    ),
    (
        "cli.md",
        "Reads a name written in Noctua's file-naming scheme and answers what it holds: the identifier, the signature, the title, the keywords and the extension. It also says whether the name is already written exactly as Noctua writes it, and gives the spelling Noctua would use. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.files"),
    (
        "cli.md",
        "Tags: file name, read a name, identifier, parse, details",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua naming.read [arguments]"),
    ("cli.md", "  noctua naming.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --name <text, from 1 to 255 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "naming.read — Ler um nome de arquivo"),
    (
        "cli.md",
        "Ler um nome de arquivo: identificador, assinatura, título e palavras-chave.",
    ),
    (
        "cli.md",
        "Lê um nome escrito no esquema de nomes de arquivo do Noctua e responde o que ele contém: o identificador, a assinatura, o título, as palavras-chave e a extensão. Também diz se o nome já está escrito exatamente como o Noctua escreve, e dá a grafia que o Noctua usaria. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.files"),
    (
        "cli.md",
        "Tags: nome de arquivo, ler um nome, identificador, analisar, detalhes",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua naming.read [arguments]"),
    ("cli.md", "  noctua naming.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --name <text, from 1 to 255 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `page.actions`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.actions — Open the window's actions"),
    (
        "cli.md",
        "Open the actions on the window and the page in focus, in the drawer.",
    ),
    (
        "cli.md",
        "Opens, in the drawer, the list of what can be done to the window — hide it, close it, move it, resize it — as Leader w does. Choosing one runs it, asking for what it needs on the prompt line.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: window, actions, container, open, leader w, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.actions [arguments]"),
    ("cli.md", "  noctua page.actions -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.actions — Abrir as ações da janela"),
    (
        "cli.md",
        "Abrir na gaveta as ações sobre a janela e a página em foco.",
    ),
    (
        "cli.md",
        "Abre, na gaveta, a lista do que se pode fazer com a janela — escondê-la, fechá-la, movê-la, redimensioná-la — como Leader w faz. Escolher uma a executa, pedindo na linha de comando o que ela precisar.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: janela, ações, contêiner, abrir, leader w, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.actions [arguments]"),
    ("cli.md", "  noctua page.actions -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `page.back`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.back — Go back in a page"),
    (
        "cli.md",
        "Return from a sub-page to the page it was opened from.",
    ),
    (
        "cli.md",
        "Goes back one step inside a page — the one in focus, unless you name another. A page that is not inside a sub-page stays as it is.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: back, sub-page, navigate, return, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.back [arguments]"),
    ("cli.md", "  noctua page.back -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.back — Voltar dentro de uma página"),
    (
        "cli.md",
        "Sair de uma subpágina e voltar para a página de onde ela foi aberta.",
    ),
    (
        "cli.md",
        "Volta um passo dentro de uma página — a que está em foco, a menos que você indique outra. Uma página que não está numa subpágina fica como está.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: voltar, subpágina, navegar, retornar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.back [arguments]"),
    ("cli.md", "  noctua page.back -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.close`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.close — Close a page"),
    (
        "cli.md",
        "Close a page of the interface, in one place or everywhere it is shown.",
    ),
    (
        "cli.md",
        "Closes the page in focus in a window's workspace area or in the drawer, or the page you name — from the place you say, or from everywhere it is shown. A page nothing shows any more is closed for good; anything it started keeps running.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: page, close, interface, tab"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.close [arguments]"),
    ("cli.md", "  noctua page.close -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.close — Fechar uma página"),
    (
        "cli.md",
        "Fechar uma página da interface, num lugar ou em todos os lugares onde aparece.",
    ),
    (
        "cli.md",
        "Fecha a página em foco na área de trabalho de uma janela ou na gaveta, ou a página que você indicar — do lugar que você disser, ou de todos os lugares onde aparece. Uma página que nada mais mostra é fechada de vez; o que ela começou continua rodando.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: página, fechar, interface, aba"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.close [arguments]"),
    ("cli.md", "  noctua page.close -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.dev`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.dev — Open the dev pages"),
    (
        "cli.md",
        "Open the pages for developing and running Noctua.",
    ),
    (
        "cli.md",
        "Opens the list of dev pages, as Leader d does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Each dev page is authorized as any command is.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: dev, developer, logs, registries, open, leader d, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.dev [arguments]"),
    ("cli.md", "  noctua page.dev -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.dev — Abrir as páginas de dev"),
    (
        "cli.md",
        "Abrir as páginas para desenvolver e operar o Noctua.",
    ),
    (
        "cli.md",
        "Abre a lista de páginas de dev, como Leader d faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Cada página de dev é autorizada como qualquer comando.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: dev, desenvolvimento, logs, registros, abrir, leader d, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.dev [arguments]"),
    ("cli.md", "  noctua page.dev -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `page.focus`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.focus — Bring a page to the front"),
    (
        "cli.md",
        "Show a page that is open, in the workspace area or in the drawer.",
    ),
    (
        "cli.md",
        "Brings a page that is already open to the front of the place that shows it: a window's workspace area, or the drawer, which is expanded if it was collapsed.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: page, focus, switch, show, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.focus [arguments]"),
    ("cli.md", "  noctua page.focus -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>  required"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.focus — Trazer uma página para a frente"),
    (
        "cli.md",
        "Mostrar uma página aberta, na área de trabalho ou na gaveta.",
    ),
    (
        "cli.md",
        "Traz para a frente uma página que já está aberta, no lugar que a mostra: a área de trabalho de uma janela, ou a gaveta, que é expandida se estava recolhida.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: página, foco, alternar, mostrar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.focus [arguments]"),
    ("cli.md", "  noctua page.focus -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>  required"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.help`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.help — Open help"),
    (
        "cli.md",
        "Open the help pages, where the leader menu's direction says.",
    ),
    (
        "cli.md",
        "Opens the list of help pages, as Leader h does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Help already open is shown again there.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: help, manual, documentation, open, leader h, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.help [arguments]"),
    ("cli.md", "  noctua page.help -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.help — Abrir a ajuda"),
    (
        "cli.md",
        "Abrir as páginas de ajuda, onde a direção do menu do Leader indicar.",
    ),
    (
        "cli.md",
        "Abre a lista de páginas de ajuda, como Leader h faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Se a ajuda já está aberta, aparece de novo ali.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: ajuda, manual, documentação, abrir, leader h, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.help [arguments]"),
    ("cli.md", "  noctua page.help -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `page.into`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.into — Go into a sub-page"),
    (
        "cli.md",
        "Open a sub-page inside a page, such as one topic of help.",
    ),
    (
        "cli.md",
        "Goes one step deeper inside a page — the one in focus, unless you name another — to the topic given, so going back returns to where it was. A page goes only so many steps deep.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: sub-page, navigate, open, topic, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.into [arguments]"),
    ("cli.md", "  noctua page.into -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --topic <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.into — Entrar numa subpágina"),
    (
        "cli.md",
        "Abrir uma subpágina dentro de uma página, como um tópico da ajuda.",
    ),
    (
        "cli.md",
        "Avança um passo para dentro de uma página — a que está em foco, a menos que você indique outra — até o tópico indicado, para que voltar retorne a onde estava. Uma página só desce um número limitado de passos.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: subpágina, navegar, abrir, tópico, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.into [arguments]"),
    ("cli.md", "  noctua page.into -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --topic <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `page.next`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.next — Show the next page"),
    (
        "cli.md",
        "Bring the next page of a window's stack to the front.",
    ),
    (
        "cli.md",
        "Brings to the front the page after the one in focus in a window's workspace area. At the last page, nothing changes.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: page, next, switch, forward, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.next [arguments]"),
    ("cli.md", "  noctua page.next -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.next — Mostrar a próxima página"),
    (
        "cli.md",
        "Trazer para a frente a próxima página da pilha de uma janela.",
    ),
    (
        "cli.md",
        "Traz para a frente a página que vem depois da que está em foco na área de trabalho de uma janela. Na última página, nada muda.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: página, próxima, alternar, avançar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.next [arguments]"),
    ("cli.md", "  noctua page.next -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.open`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.open — Open a page"),
    (
        "cli.md",
        "Open a page of the interface: settings, help, dev, a surface or a list.",
    ),
    (
        "cli.md",
        "Opens a page in the workspace area of a window or in the drawer — where the drawer's direction says, unless you choose. A page already open is shown again where it is asked for, as one more view of the same page. Opening a page closes the leader menu.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: page, open, settings, help, dev, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.open [arguments]"),
    ("cli.md", "  noctua page.open -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --kind <one of: home, settings, help, dev, surface, leader, commands, shortcuts, actions>  required",
    ),
    ("cli.md", "  --topic <text, from 1 to 64 characters>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.open — Abrir uma página"),
    (
        "cli.md",
        "Abrir uma página: configurações, ajuda, dev, uma superfície ou uma lista.",
    ),
    (
        "cli.md",
        "Abre uma página na área de trabalho de uma janela ou na gaveta — onde a direção da gaveta indicar, a menos que você escolha. Uma página que já está aberta aparece de novo onde for pedida, como mais uma vista da mesma página. Abrir uma página fecha o menu do Leader.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: página, abrir, configurações, ajuda, dev, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.open [arguments]"),
    ("cli.md", "  noctua page.open -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --kind <one of: home, settings, help, dev, surface, leader, commands, shortcuts, actions>  required",
    ),
    ("cli.md", "  --topic <text, from 1 to 64 characters>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.previous`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.previous — Show the previous page"),
    (
        "cli.md",
        "Bring the previous page of a window's stack to the front.",
    ),
    (
        "cli.md",
        "Brings to the front the page before the one in focus in a window's workspace area. At the first page, nothing changes.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: page, previous, switch, back, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.previous [arguments]"),
    ("cli.md", "  noctua page.previous -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.previous — Mostrar a página anterior"),
    (
        "cli.md",
        "Trazer para a frente a página anterior da pilha de uma janela.",
    ),
    (
        "cli.md",
        "Traz para a frente a página que vem antes da que está em foco na área de trabalho de uma janela. Na primeira página, nada muda.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: página, anterior, alternar, voltar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.previous [arguments]"),
    ("cli.md", "  noctua page.previous -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `page.scroll`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.scroll — Scroll a page"),
    (
        "cli.md",
        "Set how far a page is scrolled, so it reopens where it was.",
    ),
    (
        "cli.md",
        "Sets how far down a page is scrolled, in CSS pixels — the page in focus, unless you name another — so every window shows it there and reopening it brings you back to the same place.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: scroll, page, position, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.scroll [arguments]"),
    ("cli.md", "  noctua page.scroll -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --to <whole number, from 0 to 4294967295>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.scroll — Rolar uma página"),
    (
        "cli.md",
        "Definir até onde uma página está rolada, para que reabra onde estava.",
    ),
    (
        "cli.md",
        "Define até onde uma página está rolada para baixo, em pixels CSS — a página em foco, a menos que você indique outra — para que todas as janelas a mostrem ali e reabri-la leve você de volta ao mesmo ponto.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: rolar, página, posição, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.scroll [arguments]"),
    ("cli.md", "  noctua page.scroll -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --page <whole number, at least 1>"),
    ("cli.md", "  --in <one of: workspace, drawer>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --to <whole number, from 0 to 4294967295>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `page.settings`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.settings — Open settings"),
    (
        "cli.md",
        "Open the settings, where the leader menu's direction says.",
    ),
    (
        "cli.md",
        "Opens the list of settings, as Leader s does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Settings already open is shown again there.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: settings, preferences, open, leader s, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.settings [arguments]"),
    ("cli.md", "  noctua page.settings -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.settings — Abrir as configurações"),
    (
        "cli.md",
        "Abrir as configurações, onde a direção do menu do Leader indicar.",
    ),
    (
        "cli.md",
        "Abre a lista de configurações, como Leader s faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Se as configurações já estão abertas, aparecem de novo ali.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: configurações, preferências, abrir, leader s, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.settings [arguments]"),
    ("cli.md", "  noctua page.settings -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `page.shortcuts`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.shortcuts — Open the page's shortcuts"),
    (
        "cli.md",
        "Open the shortcuts of the page in focus, in the drawer.",
    ),
    (
        "cli.md",
        "Opens, in the drawer, the list of the shortcuts the page in focus keeps for itself, as Leader / does. A page keeps its own shortcuts there rather than in the global bindings.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: shortcuts, page, keys, open, leader slash, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.shortcuts [arguments]"),
    ("cli.md", "  noctua page.shortcuts -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "page.shortcuts — Abrir os atalhos da página"),
    ("cli.md", "Abrir na gaveta os atalhos da página em foco."),
    (
        "cli.md",
        "Abre, na gaveta, a lista dos atalhos que a página em foco guarda para si, como Leader / faz. Uma página guarda ali os seus próprios atalhos, e não nos atalhos globais.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: atalhos, página, teclas, abrir, leader barra, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua page.shortcuts [arguments]"),
    ("cli.md", "  noctua page.shortcuts -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `prompt.ask`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.ask — Ask for a command's parameters"),
    (
        "cli.md",
        "Keep the prompt line on a command, asking for its parameters.",
    ),
    (
        "cli.md",
        "Keeps the prompt line on the command you chose, offering its parameters — on the one you name, when you name one — so what you type next is a parameter's value.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: prompt, parameters, command, arguments, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.ask [arguments]"),
    ("cli.md", "  noctua prompt.ask -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --command <text, from 1 to 96 characters>  required",
    ),
    ("cli.md", "  --parameter <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.ask — Pedir os parâmetros de um comando"),
    (
        "cli.md",
        "Manter a linha do prompt num comando, pedindo os parâmetros dele.",
    ),
    (
        "cli.md",
        "Mantém a linha do prompt no comando que você escolheu, oferecendo os parâmetros dele — no que você indicar, quando indicar um — para que o que você digitar em seguida seja o valor de um parâmetro.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: prompt, parâmetros, comando, argumentos, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.ask [arguments]"),
    ("cli.md", "  noctua prompt.ask -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --command <text, from 1 to 96 characters>  required",
    ),
    ("cli.md", "  --parameter <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "## `prompt.clear`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.clear — Clear the prompt line"),
    (
        "cli.md",
        "Empty the prompt line and stop asking for anything.",
    ),
    (
        "cli.md",
        "Empties the prompt line and stops asking for a command or its parameters, closing the list of commands in the drawer.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: prompt, clear, empty, cancel, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.clear [arguments]"),
    ("cli.md", "  noctua prompt.clear -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.clear — Limpar a linha do prompt"),
    (
        "cli.md",
        "Esvaziar a linha do prompt e parar de pedir qualquer coisa.",
    ),
    (
        "cli.md",
        "Esvazia a linha do prompt e para de pedir um comando ou os parâmetros dele, fechando a lista de comandos na gaveta.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: prompt, limpar, esvaziar, cancelar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.clear [arguments]"),
    ("cli.md", "  noctua prompt.clear -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `prompt.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.set — Type into the prompt line"),
    (
        "cli.md",
        "Set what the prompt line holds, as if it were typed there.",
    ),
    (
        "cli.md",
        "Sets the text of the prompt line, the bottom line of the footer. Text that begins with a semicolon or a colon opens the list of commands in the drawer, and the rest of it filters the list.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: prompt, type, text, command list, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.set [arguments]"),
    ("cli.md", "  noctua prompt.set -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --text <text, at most 4096 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "prompt.set — Digitar na linha do prompt"),
    (
        "cli.md",
        "Definir o que a linha do prompt contém, como se fosse digitado ali.",
    ),
    (
        "cli.md",
        "Define o texto da linha do prompt, a última linha do rodapé. Um texto que começa com ponto e vírgula ou dois-pontos abre a lista de comandos na gaveta, e o resto dele filtra a lista.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: prompt, digitar, texto, lista de comandos, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua prompt.set [arguments]"),
    ("cli.md", "  noctua prompt.set -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --text <text, at most 4096 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `settings.chord.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chord.read — Read the added key"),
    (
        "cli.md",
        "Show the key held with the Leader for more shortcuts.",
    ),
    (
        "cli.md",
        "Answers the key that, held with the Leader, reaches a second set of shortcuts in this installation of Noctua. It is Shift until you choose another. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: chord, added key, modifier, shift, shortcut, keyboard, settings",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chord.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.chord.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chord.read — Ver a tecla acrescentada"),
    (
        "cli.md",
        "Mostrar a tecla pressionada com o Leader para mais atalhos.",
    ),
    (
        "cli.md",
        "Responde qual tecla, pressionada junto com o Leader, alcança um segundo conjunto de atalhos nesta instalação do Noctua. É Shift até você escolher outra. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: tecla acrescentada, modificador, shift, atalho, teclado, configurações",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chord.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.chord.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `settings.chord.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chord.set — Change the added key"),
    (
        "cli.md",
        "Choose the key held with the Leader for more shortcuts.",
    ),
    (
        "cli.md",
        "Changes the key that, held with the Leader, reaches a second set of shortcuts, in this installation of Noctua and in no other: one of Control, Alt, Shift and Super. It is refused when the Leader already holds it, or when the Leader held with it would be another installation's Leader. A refused choice changes nothing. Only you can change it, asking directly, never from inside another action. Undo stops at this step: to go back, choose the key you had.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: chord, added key, modifier, shift, shortcut, keyboard, settings, change",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chord.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.chord.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --chord <text, from 1 to 16 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chord.set — Mudar a tecla acrescentada"),
    (
        "cli.md",
        "Escolher a tecla pressionada com o Leader para mais atalhos.",
    ),
    (
        "cli.md",
        "Muda a tecla que, pressionada junto com o Leader, alcança um segundo conjunto de atalhos, nesta instalação do Noctua e em nenhuma outra: uma entre Control, Alt, Shift e Super. É recusada quando o Leader já a inclui, ou quando o Leader com ela seria o Leader de outra instalação. Uma escolha recusada não muda nada. Só você pode mudá-la, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, escolha a tecla que você tinha.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: tecla acrescentada, modificador, shift, atalho, teclado, configurações, mudar",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chord.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.chord.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --chord <text, from 1 to 16 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `settings.chords.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chords.read — Read the chords"),
    (
        "cli.md",
        "Show each key pressed with the Leader held and what it runs.",
    ),
    (
        "cli.md",
        "Answers every key that acts while the Leader is held in this installation of Noctua, each beside the command it runs and whether the added key is held as well: Noctua's own keys, with your choices over them. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: chords, held, shortcuts, keys, keyboard, settings",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chords.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.chords.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chords.read — Ver as combinações"),
    (
        "cli.md",
        "Mostrar cada tecla pressionada com o Leader e o que ela executa.",
    ),
    (
        "cli.md",
        "Responde todas as teclas que agem enquanto o Leader está pressionado nesta instalação do Noctua, cada uma ao lado do comando que executa e se a tecla acrescentada também é pressionada: as teclas do próprio Noctua, com as suas escolhas por cima. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: combinações, pressionado, atalhos, teclas, teclado, configurações",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chords.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.chords.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `settings.chords.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chords.set — Change the chords"),
    (
        "cli.md",
        "Choose each key pressed with the Leader held and what it runs.",
    ),
    (
        "cli.md",
        "Replaces every key that acts while the Leader is held in this installation of Noctua, and in no other, with the keys you give, each beside the command it runs and whether the added key is held as well. A command you leave out has no chord. One key runs one command, so a key given twice the same way is refused, and a refused choice changes nothing. Only you can change them, asking directly, never from inside another action. Undo stops at this step: to go back, give the keys you had.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: chords, held, shortcuts, keys, keyboard, settings, change, rebind",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chords.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.chords.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --chords <a JSON object>  (repeated, once for each item, at most 146 items)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.chords.set — Mudar as combinações"),
    (
        "cli.md",
        "Escolher cada tecla pressionada com o Leader e o que ela executa.",
    ),
    (
        "cli.md",
        "Substitui todas as teclas que agem enquanto o Leader está pressionado nesta instalação do Noctua, e em nenhuma outra, pelas teclas que você informar, cada uma ao lado do comando que executa e se a tecla acrescentada também é pressionada. Um comando que você deixar de fora fica sem combinação. Uma tecla executa um comando só, então uma tecla informada duas vezes do mesmo jeito é recusada, e uma escolha recusada não muda nada. Só você pode mudá-las, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, informe as teclas que você tinha.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: combinações, pressionado, atalhos, teclas, teclado, configurações, mudar, reatribuir",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.chords.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.chords.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --chords <a JSON object>  (repeated, once for each item, at most 146 items)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `settings.leader.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.leader.read — Read the Leader"),
    ("cli.md", "Show the keys that reach Noctua from anywhere."),
    (
        "cli.md",
        "Answers the Leader in use in this installation of Noctua: the keys that show Noctua from anywhere on the computer, written as their places on the keyboard. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader, shortcut, keys, keyboard, settings, hotkey",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.leader.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.leader.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.leader.read — Ver o Leader"),
    (
        "cli.md",
        "Mostrar as teclas que chamam o Noctua de qualquer lugar.",
    ),
    (
        "cli.md",
        "Responde qual é o Leader em uso nesta instalação do Noctua: as teclas que mostram o Noctua de qualquer lugar do computador, escritas pela posição delas no teclado. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader, atalho, teclas, teclado, configurações, tecla de atalho",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.leader.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.leader.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `settings.leader.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.leader.set — Change the Leader"),
    ("cli.md", "Choose the keys that reach Noctua from anywhere."),
    (
        "cli.md",
        "Changes the Leader of this installation of Noctua, and of no other. The Leader needs at least one of Control, Alt, Shift and Super held with its key, and it is refused when it would clash with the key added to it or with another installation's Leader, or when the system does not give it to Noctua. A refused choice changes nothing. Only you can change it, asking directly, never from inside another action. Undo stops at this step: to go back, choose the Leader you had.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader, shortcut, keys, keyboard, settings, hotkey, change",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.leader.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.leader.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --leader <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.leader.set — Mudar o Leader"),
    (
        "cli.md",
        "Escolher as teclas que chamam o Noctua de qualquer lugar.",
    ),
    (
        "cli.md",
        "Muda o Leader desta instalação do Noctua, e de nenhuma outra. O Leader precisa de pelo menos uma entre Control, Alt, Shift e Super pressionada junto com a tecla dele, e é recusado quando conflitaria com a tecla acrescentada a ele ou com o Leader de outra instalação, ou quando o sistema não o entrega ao Noctua. Uma escolha recusada não muda nada. Só você pode mudá-lo, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, escolha o Leader que você tinha.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader, atalho, teclas, teclado, configurações, tecla de atalho, mudar",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.leader.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.leader.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --leader <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `settings.mcp.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.mcp.read — Read whether MCP is on"),
    (
        "cli.md",
        "Show whether this installation of Noctua lets MCP clients in.",
    ),
    (
        "cli.md",
        "Answers whether MCP is on in this installation of Noctua. It is off until a person turns it on, and turning it on here turns it on nowhere else. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: mcp, agents, settings, model context protocol, ai tools",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.mcp.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.mcp.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.mcp.read — Ver se o MCP está ligado"),
    (
        "cli.md",
        "Mostrar se esta instalação do Noctua deixa clientes MCP entrarem.",
    ),
    (
        "cli.md",
        "Responde se o MCP está ligado nesta instalação do Noctua. Ele fica desligado até que uma pessoa o ligue, e ligá-lo aqui não o liga em nenhuma outra. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: mcp, agentes, configurações, ferramentas de ia, protocolo de contexto",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.mcp.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.mcp.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `settings.mcp.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.mcp.set — Switch MCP on or off"),
    (
        "cli.md",
        "Let MCP clients act here as agents, or stop them.",
    ),
    (
        "cli.md",
        "Turns MCP on or off in this installation of Noctua, and in no other. While it is off, no MCP client is let in; while it is on, a client acts only as an agent you made and granted, never as you. Only you can switch it, asking directly, never from inside another action, and switching it on waits for you to confirm it in the Noctua window. Undo puts it back as it was; when that would switch it on, the undo waits for you to confirm it in the Noctua window too.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: mcp, agents, settings, turn on, turn off, model context protocol, ai tools",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.mcp.set [arguments]"),
    ("cli.md", "  noctua settings.mcp.set -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --on  (a boolean: true when given alone, `=false` says false)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "settings.mcp.set — Ligar ou desligar o MCP"),
    (
        "cli.md",
        "Deixar clientes MCP agirem aqui como agentes, ou impedi-los.",
    ),
    (
        "cli.md",
        "Liga ou desliga o MCP nesta instalação do Noctua, e em nenhuma outra. Enquanto está desligado, nenhum cliente MCP entra; enquanto está ligado, um cliente age só como um agente que você criou e autorizou, nunca como você. Só você pode mudá-lo, pedindo diretamente, nunca de dentro de outra ação, e ligá-lo aguarda você confirmar na janela do Noctua. Desfazer o deixa como estava; quando isso o ligaria, o desfazer também aguarda você confirmar na janela do Noctua.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: mcp, agentes, configurações, ligar, desligar, ferramentas de ia, protocolo de contexto",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.mcp.set [arguments]"),
    ("cli.md", "  noctua settings.mcp.set -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --on  (a boolean: true when given alone, `=false` says false)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `settings.menu-keys.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "settings.menu-keys.read — Read the leader menu's keys",
    ),
    (
        "cli.md",
        "Show each key of the leader menu and what it runs.",
    ),
    (
        "cli.md",
        "Answers every key of the leader menu in use in this installation of Noctua, each beside the command it runs: Noctua's own keys, with your choices over them. It changes nothing.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader menu, keys, shortcuts, keyboard, settings, menu",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.menu-keys.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.menu-keys.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "settings.menu-keys.read — Ver as teclas do menu do Leader",
    ),
    (
        "cli.md",
        "Mostrar cada tecla do menu do Leader e o que ela executa.",
    ),
    (
        "cli.md",
        "Responde todas as teclas do menu do Leader em uso nesta instalação do Noctua, cada uma ao lado do comando que executa: as teclas do próprio Noctua, com as suas escolhas por cima. Não muda nada.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: menu do leader, teclas, atalhos, teclado, configurações, menu",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.menu-keys.read [arguments]"),
    (
        "cli.md",
        "  noctua settings.menu-keys.read -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `settings.menu-keys.set`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "settings.menu-keys.set — Change the leader menu's keys",
    ),
    (
        "cli.md",
        "Choose each key of the leader menu and what it runs.",
    ),
    (
        "cli.md",
        "Replaces every key of the leader menu in this installation of Noctua, and in no other, with the keys you give, each beside the command it runs. A command you leave out is not in the menu. One key runs one command, so a key given twice is refused, and a refused choice changes nothing. Only you can change them, asking directly, never from inside another action. Undo stops at this step: to go back, give the keys you had.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: leader menu, keys, shortcuts, keyboard, settings, menu, change, rebind",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.menu-keys.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.menu-keys.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --keys <a JSON object>  (repeated, once for each item, at most 73 items)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "settings.menu-keys.set — Mudar as teclas do menu do Leader",
    ),
    (
        "cli.md",
        "Escolher cada tecla do menu do Leader e o que ela executa.",
    ),
    (
        "cli.md",
        "Substitui todas as teclas do menu do Leader nesta instalação do Noctua, e em nenhuma outra, pelas teclas que você informar, cada uma ao lado do comando que executa. Um comando que você deixar de fora não aparece no menu. Uma tecla executa um comando só, então uma tecla informada duas vezes é recusada, e uma escolha recusada não muda nada. Só você pode mudá-las, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, informe as teclas que você tinha.",
    ),
    ("cli.md", "Categories: noctua.settings"),
    (
        "cli.md",
        "Tags: menu do leader, teclas, atalhos, teclado, configurações, menu, mudar, reatribuir",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua settings.menu-keys.set [arguments]"),
    (
        "cli.md",
        "  noctua settings.menu-keys.set -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --keys <a JSON object>  (repeated, once for each item, at most 73 items)  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `shell.drag`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.drag — Say what a seat drags"),
    (
        "cli.md",
        "Record what a seat is dragging, or that it stopped.",
    ),
    (
        "cli.md",
        "Records what one seat is dragging in the interface, so every window shows it being dragged; naming nothing says the drag ended.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: drag, pointer, seat, interface, move"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.drag [arguments]"),
    ("cli.md", "  noctua shell.drag -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --target <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.drag — Dizer o que um assento arrasta"),
    (
        "cli.md",
        "Registrar o que um assento está arrastando, ou que parou de arrastar.",
    ),
    (
        "cli.md",
        "Registra o que um assento está arrastando na interface, para que todas as janelas mostrem isso sendo arrastado; sem indicar nada, diz que o arraste terminou.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: arrastar, ponteiro, assento, interface, mover",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.drag [arguments]"),
    ("cli.md", "  noctua shell.drag -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --target <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "## `shell.focus`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.focus — Move a seat's focus"),
    (
        "cli.md",
        "Give a seat's keyboard to a part of an interface window.",
    ),
    (
        "cli.md",
        "Moves the keyboard focus of one seat — the system's own unless you name another — to a part of a window: its workspace area, the drawer, the prompt line, the sidebar, the info line or the modeline. Every window of the interface follows it. A keyboard on a drawer that shows nothing open goes back to the prompt line, whichever command left the drawer so.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: focus, keyboard, seat, interface, move focus",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.focus [arguments]"),
    ("cli.md", "  noctua shell.focus -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --region <one of: workspace, drawer, prompt, sidebar, info, modeline>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.focus — Mover o foco de um assento"),
    (
        "cli.md",
        "Levar o teclado de um assento para uma parte de uma janela da interface.",
    ),
    (
        "cli.md",
        "Move o foco do teclado de um assento — o do próprio sistema, a menos que você indique outro — para uma parte de uma janela: a área de trabalho, a gaveta, a linha do prompt, a barra lateral, a linha de informações ou a linha de modo. Todas as janelas da interface acompanham. Um teclado numa gaveta que não mostra nada aberto volta para a linha do prompt, seja qual for o comando que a deixou assim.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: foco, teclado, assento, interface, mudar o foco",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.focus [arguments]"),
    ("cli.md", "  noctua shell.focus -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --region <one of: workspace, drawer, prompt, sidebar, info, modeline>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `shell.hover`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.hover — Say what a seat points at"),
    (
        "cli.md",
        "Record what a seat's pointer hovers, or what its keyboard selection is on.",
    ),
    (
        "cli.md",
        "Records what one seat's pointer is over, or what its keyboard selection is on, so the info line can say what it is; naming nothing clears it. The info line shows whichever of the two changed last.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: hover, pointer, selection, info line, seat"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.hover [arguments]"),
    ("cli.md", "  noctua shell.hover -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --by <one of: pointer, keyboard>  required"),
    ("cli.md", "  --target <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.hover — Dizer para onde um assento aponta"),
    (
        "cli.md",
        "Registrar o que está sob o ponteiro de um assento, ou a seleção do teclado dele.",
    ),
    (
        "cli.md",
        "Registra o que está sob o ponteiro de um assento, ou onde está a seleção do teclado dele, para que a linha de informações diga o que é; sem indicar nada, limpa o registro. A linha de informações mostra o que mudou por último entre os dois.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: sobrevoar, ponteiro, seleção, linha de informações, assento",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.hover [arguments]"),
    ("cli.md", "  noctua shell.hover -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "  --by <one of: pointer, keyboard>  required"),
    ("cli.md", "  --target <text, from 1 to 64 characters>"),
    ("cli.md", "```"),
    ("cli.md", "## `shell.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.read — Read the interface's state"),
    (
        "cli.md",
        "Show all the interface holds: its windows, pages, drawer, prompt and seats.",
    ),
    (
        "cli.md",
        "Answers the whole state of the desktop interface as Noctua keeps it: each window and the pages open in it, the drawer, the sidebar, the footer, the info line, the prompt and what each seat is on, with the revision it is at. Asked from inside a command that changed the interface, it answers that change instead, which is not Noctua's until the command ends, and with no revision. It changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: interface, window, state, layout, what is open",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.read [arguments]"),
    ("cli.md", "  noctua shell.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "shell.read — Ler o estado da interface"),
    (
        "cli.md",
        "Mostrar tudo o que a interface guarda: janelas, páginas, gaveta e prompt.",
    ),
    (
        "cli.md",
        "Responde o estado completo da interface como o Noctua o guarda: cada janela e as páginas abertas nela, a gaveta, a barra lateral, o rodapé, a linha de informações, o prompt e onde cada assento está, com a revisão em que está. Pedido de dentro de um comando que mudou a interface, responde essa mudança, que só é do Noctua quando o comando termina, e sem revisão. Não altera nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: interface, janela, estado, disposição, o que está aberto",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua shell.read [arguments]"),
    ("cli.md", "  noctua shell.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `sidebar.cycle`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "sidebar.cycle — Show, focus or hide the sidebar"),
    (
        "cli.md",
        "Do what Leader Space does: show the sidebar, focus it, or hide it.",
    ),
    (
        "cli.md",
        "Shows the sidebar when it is hidden, puts a seat's keyboard on it when it is shown, and hides it when the keyboard is already there.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: sidebar, leader space, toggle, focus, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.cycle [arguments]"),
    ("cli.md", "  noctua sidebar.cycle -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "sidebar.cycle — Mostrar, focar ou esconder a barra",
    ),
    (
        "cli.md",
        "Fazer o que Leader Espaço faz: mostrar a barra lateral, focá-la ou escondê-la.",
    ),
    (
        "cli.md",
        "Mostra a barra lateral quando está escondida, leva o teclado de um assento até ela quando está visível, e a esconde quando o teclado já está nela.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: barra lateral, leader espaço, alternar, foco, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.cycle [arguments]"),
    ("cli.md", "  noctua sidebar.cycle -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --seat <whole number, from 1 to 4>"),
    ("cli.md", "```"),
    ("cli.md", "## `sidebar.hide`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "sidebar.hide — Hide the sidebar"),
    ("cli.md", "Hide the list of what is open in the interface."),
    (
        "cli.md",
        "Hides the sidebar; a seat whose keyboard was on it goes back to the workspace area.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: sidebar, hide, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.hide [arguments]"),
    ("cli.md", "  noctua sidebar.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "sidebar.hide — Esconder a barra lateral"),
    (
        "cli.md",
        "Esconder a lista do que está aberto na interface.",
    ),
    (
        "cli.md",
        "Esconde a barra lateral; um assento cujo teclado estava nela volta para a área de trabalho.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: barra lateral, esconder, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.hide [arguments]"),
    ("cli.md", "  noctua sidebar.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `sidebar.show`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "sidebar.show — Show the sidebar"),
    ("cli.md", "Show the list of what is open in the interface."),
    (
        "cli.md",
        "Shows the sidebar, the list of everything open in the interface.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: sidebar, show, list, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.show [arguments]"),
    ("cli.md", "  noctua sidebar.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "sidebar.show — Mostrar a barra lateral"),
    ("cli.md", "Mostrar a lista do que está aberto na interface."),
    (
        "cli.md",
        "Mostra a barra lateral, a lista de tudo o que está aberto na interface.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: barra lateral, mostrar, lista, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua sidebar.show [arguments]"),
    ("cli.md", "  noctua sidebar.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `surfaces.activate`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "surfaces.activate — Press something on a surface"),
    (
        "cli.md",
        "Run what a button or a toggle on a surface runs, as a click would.",
    ),
    (
        "cli.md",
        "Does what clicking a button or a toggle on a surface does: runs the action the surface itself names for it, with what you typed into the fields it uses, and nothing you name yourself. Something an agent drew is never run this way, since it would run with your authority. What it runs is decided and recorded as itself, and undo treats it as that command's own step.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: surface, click, press, button, submit, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.activate [arguments]"),
    (
        "cli.md",
        "  noctua surfaces.activate -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --surface <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --node <text, at most 512 characters>  required",
    ),
    (
        "cli.md",
        "  --inputs <a JSON object>  (repeated, once for each item, at most 2000 items)",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    (
        "cli.md",
        "surfaces.activate — Acionar algo em uma superfície",
    ),
    (
        "cli.md",
        "Executar o que um botão ou chave de uma superfície executa, como um clique.",
    ),
    (
        "cli.md",
        "Faz o que clicar em um botão ou chave de uma superfície faz: executa a ação que a própria superfície indica, com o que você digitou nos campos que ela usa, e nada que você mesmo indique. Algo desenhado por um agente nunca é executado assim, pois rodaria com a sua autoridade. O que ele executa é decidido e registrado como si mesmo, e desfazer o trata como um passo daquele comando.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: superfície, clicar, acionar, botão, enviar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.activate [arguments]"),
    (
        "cli.md",
        "  noctua surfaces.activate -- '<one JSON object>'",
    ),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --surface <text, from 1 to 64 characters>  required",
    ),
    (
        "cli.md",
        "  --node <text, at most 512 characters>  required",
    ),
    (
        "cli.md",
        "  --inputs <a JSON object>  (repeated, once for each item, at most 2000 items)",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `surfaces.list`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "surfaces.list — List the surfaces open"),
    (
        "cli.md",
        "Show the surfaces plugins have open, and who drew what each shows.",
    ),
    (
        "cli.md",
        "Lists every surface the plugins hold open: its name, where it is shown, its title, the plugin that holds it, and who drew what it shows now — the person the plugin acted for, and the agent that acted for them when one did. It does not show what each surface holds; read one surface for that. It changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: surface, panel, plugin, view, list, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.list [arguments]"),
    ("cli.md", "  noctua surfaces.list -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "surfaces.list — Listar as superfícies abertas"),
    (
        "cli.md",
        "Mostrar as superfícies abertas pelos plugins e quem desenhou cada uma.",
    ),
    (
        "cli.md",
        "Lista cada superfície que os plugins mantêm abertas: seu nome, onde aparece, seu título, o plugin que a mantém e quem desenhou o que ela mostra agora — a pessoa por quem o plugin agiu e o agente que agiu por ela, quando foi um agente. Não mostra o conteúdo de cada superfície; leia uma superfície para isso. Não altera nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: superfície, painel, plugin, visão, listar, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.list [arguments]"),
    ("cli.md", "  noctua surfaces.list -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `surfaces.read`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "surfaces.read — Read a surface"),
    (
        "cli.md",
        "Show one surface a plugin has open, with what it shows and who drew it.",
    ),
    (
        "cli.md",
        "Shows the surface named: where it is shown, its title, the plugin that holds it, everything it shows now, and who drew that — the person the plugin acted for, and the agent that acted for them when one did. A surface no longer open is refused. It changes nothing.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: surface, panel, plugin, view, read, contents",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.read [arguments]"),
    ("cli.md", "  noctua surfaces.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --surface <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "surfaces.read — Ler uma superfície"),
    (
        "cli.md",
        "Mostrar uma superfície aberta, com o que ela mostra e quem o desenhou.",
    ),
    (
        "cli.md",
        "Mostra a superfície indicada: onde aparece, seu título, o plugin que a mantém, tudo o que ela mostra agora e quem desenhou isso — a pessoa por quem o plugin agiu e o agente que agiu por ela, quando foi um agente. Uma superfície que não está mais aberta é recusada. Não altera nada.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: superfície, painel, plugin, visão, ler, conteúdo",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua surfaces.read [arguments]"),
    ("cli.md", "  noctua surfaces.read -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    (
        "cli.md",
        "  --surface <text, from 1 to 64 characters>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `window.close`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.close — Close a window"),
    (
        "cli.md",
        "Close a window of the interface, or hide it when it is the last one.",
    ),
    (
        "cli.md",
        "Closes a window of the desktop interface — the one you are using, unless you name another — with the pages only it showed. The last window is hidden instead, so the interface is always ready to show again. Nothing that runs in Noctua stops.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: window, close, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.close [arguments]"),
    ("cli.md", "  noctua window.close -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.close — Fechar uma janela"),
    (
        "cli.md",
        "Fechar uma janela da interface, ou escondê-la quando for a última.",
    ),
    (
        "cli.md",
        "Fecha uma janela da interface — a que você está usando, a menos que indique outra — com as páginas que só ela mostrava. A última janela é escondida em vez de fechada, para que a interface esteja sempre pronta para aparecer. Nada do que roda no Noctua para.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: janela, fechar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.close [arguments]"),
    ("cli.md", "  noctua window.close -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `window.hide`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.hide — Hide a window"),
    (
        "cli.md",
        "Hide a window of the interface, keeping everything in it.",
    ),
    (
        "cli.md",
        "Hides a window of the desktop interface — the one you are using, unless you name another. Nothing stops: what was open in it is kept, and showing it again brings it all back.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: window, hide, minimize, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.hide [arguments]"),
    ("cli.md", "  noctua window.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.hide — Esconder uma janela"),
    (
        "cli.md",
        "Esconder uma janela da interface, mantendo tudo o que há nela.",
    ),
    (
        "cli.md",
        "Esconde uma janela da interface — a que você está usando, a menos que indique outra. Nada para: o que estava aberto nela é mantido, e mostrá-la de novo traz tudo de volta.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: janela, esconder, minimizar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.hide [arguments]"),
    ("cli.md", "  noctua window.hide -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "## `window.move`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.move — Move a window"),
    (
        "cli.md",
        "Place a window of the interface at a position on the screen.",
    ),
    (
        "cli.md",
        "Moves a window of the desktop interface — the one you are using, unless you name another — so its top left corner is at the position given, in the screen's own units. Under a tiling window manager, the manager may place it elsewhere.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: window, move, position, place, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.move [arguments]"),
    ("cli.md", "  noctua window.move -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --x <whole number, from -32768 to 32767>  required",
    ),
    (
        "cli.md",
        "  --y <whole number, from -32768 to 32767>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.move — Mover uma janela"),
    (
        "cli.md",
        "Colocar uma janela da interface numa posição da tela.",
    ),
    (
        "cli.md",
        "Move uma janela da interface — a que você está usando, a menos que indique outra — para que o canto superior esquerdo fique na posição indicada, nas unidades da própria tela. Num gerenciador de janelas em mosaico, ele pode colocá-la em outro lugar.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: janela, mover, posição, lugar, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.move [arguments]"),
    ("cli.md", "  noctua window.move -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --x <whole number, from -32768 to 32767>  required",
    ),
    (
        "cli.md",
        "  --y <whole number, from -32768 to 32767>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `window.open`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.open — Open another window"),
    (
        "cli.md",
        "Open one more window of the interface, showing the same state.",
    ),
    (
        "cli.md",
        "Opens another window of the desktop interface, shown at once. Every window shows the same drawer, prompt and footer, and each keeps a stack of pages of its own. The interface holds only so many windows.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: window, new window, open, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.open [arguments]"),
    ("cli.md", "  noctua window.open -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.open — Abrir outra janela"),
    (
        "cli.md",
        "Abrir mais uma janela da interface, mostrando o mesmo estado.",
    ),
    (
        "cli.md",
        "Abre mais uma janela da interface, já visível. Todas as janelas mostram a mesma gaveta, o mesmo prompt e o mesmo rodapé, e cada uma tem a sua própria pilha de páginas. A interface comporta um número limitado de janelas.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: janela, nova janela, abrir, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.open [arguments]"),
    ("cli.md", "  noctua window.open -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  none"),
    ("cli.md", "```"),
    ("cli.md", "## `window.resize`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.resize — Resize a window"),
    (
        "cli.md",
        "Give a window of the interface a new width and height.",
    ),
    (
        "cli.md",
        "Resizes a window of the desktop interface — the one you are using, unless you name another — in CSS pixels. A window is never made smaller than the smallest size every page is built for.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: window, resize, size, width, height, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.resize [arguments]"),
    ("cli.md", "  noctua window.resize -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --width <whole number, from 320 to 65535>  required",
    ),
    (
        "cli.md",
        "  --height <whole number, from 240 to 65535>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.resize — Redimensionar uma janela"),
    (
        "cli.md",
        "Dar a uma janela da interface uma nova largura e uma nova altura.",
    ),
    (
        "cli.md",
        "Redimensiona uma janela da interface — a que você está usando, a menos que indique outra — em pixels CSS. Uma janela nunca fica menor que o menor tamanho para o qual todas as páginas são feitas.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: janela, redimensionar, tamanho, largura, altura, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.resize [arguments]"),
    ("cli.md", "  noctua window.resize -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    (
        "cli.md",
        "  --width <whole number, from 320 to 65535>  required",
    ),
    (
        "cli.md",
        "  --height <whole number, from 240 to 65535>  required",
    ),
    ("cli.md", "```"),
    ("cli.md", "## `window.show`"),
    ("cli.md", "In `en`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.show — Show a window"),
    ("cli.md", "Show a window of the interface that is hidden."),
    (
        "cli.md",
        "Shows a window of the desktop interface — the one you are using, unless you name another — exactly as it was when it was hidden.",
    ),
    ("cli.md", "Categories: none"),
    ("cli.md", "Tags: window, show, bring back, interface"),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.show [arguments]"),
    ("cli.md", "  noctua window.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    ("cli.md", "In `pt-BR`:"),
    ("cli.md", "```text"),
    ("cli.md", "window.show — Mostrar uma janela"),
    (
        "cli.md",
        "Mostrar uma janela da interface que está escondida.",
    ),
    (
        "cli.md",
        "Mostra uma janela da interface — a que você está usando, a menos que indique outra — exatamente como estava quando foi escondida.",
    ),
    ("cli.md", "Categories: none"),
    (
        "cli.md",
        "Tags: janela, mostrar, trazer de volta, interface",
    ),
    ("cli.md", "Usage:"),
    ("cli.md", "  noctua window.show [arguments]"),
    ("cli.md", "  noctua window.show -- '<one JSON object>'"),
    ("cli.md", "Arguments:"),
    ("cli.md", "  --window <whole number, at least 1>"),
    ("cli.md", "```"),
    (
        "commands.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "commands.md",
        "<!-- Source: catalog/commands.toml, catalog/i18n/ and catalog/wiki/. Run `cargo xtask docs`. -->",
    ),
    ("commands.md", "# Commands"),
    (
        "commands.md",
        "Every command the catalog declares — the kernel's own — with what a dispatcher decides on: the Cedar action it is authorized as, how its resource is extracted, whether it changes state, and whether it can be undone; and with what a person is shown of it in each locale: its display name, its line description, its paragraph and the tags a search finds it by.",
    ),
    (
        "commands.md",
        "**The kernel registers every one of them and decides each at its decision point**, as it decides a plugin's, and runs each one it allows itself. A plugin's commands are not on this page: a plugin declares them at `activate`, and the kernel holds each to the judge that holds these.",
    ),
    (
        "commands.md",
        "**`Action` is what the policy pack decides on, and the pack has rules of its own.** `policy/core/quorum.cedar` forbids `Delete` unless the context carries `quorum_met`, and the schema makes that attribute required — so *every* command whose action is `Delete` needs a quorum, whatever else it declares. A reader designing a scheduled caller for one has to read the pack as well as this page.",
    ),
    ("commands.md", "## `agents.create`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"label\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"budget\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":100000}},\"required\":[\"label\",\"budget\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `agents.grant`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"minutes\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":43200},\"reason\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":256}},\"required\":[\"agent\",\"feature\",\"minutes\",\"reason\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `agents.list`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `agents.revoke`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"agent\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `confirmations.list`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `confirmations.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"serial\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"serial\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `drawer.back`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `drawer.collapse`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `drawer.expand`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `drawer.resize`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"share\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":49}},\"required\":[\"share\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `drawer.toward`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]}},\"required\":[\"region\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `footer.hide`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `footer.show`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `info.clear`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `info.collapse`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `info.expand`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `info.say`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `journal.verify`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"from\":{\"type\":\"integer\",\"minimum\":0}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.find`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"query\":{\"type\":\"string\",\"maxLength\":64},\"limit\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":32}},\"required\":[\"query\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.history`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.redo`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `kernel.undo`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `leader.tap`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `limits.prune`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"table\":{\"type\":\"string\"},\"dry_run\":{\"type\":\"boolean\"}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `naming.propose`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"title\":{\"type\":\"string\",\"maxLength\":1024},\"signature\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"keywords\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"extension\":{\"type\":\"string\",\"maxLength\":255},\"id\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":13}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `naming.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":255}},\"required\":[\"name\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.actions`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.back`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.close`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.dev`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.focus`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"page\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.help`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.into`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"topic\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.next`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.open`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"kind\":{\"type\":\"string\",\"enum\":[\"home\",\"settings\",\"help\",\"dev\",\"surface\",\"leader\",\"commands\",\"shortcuts\",\"actions\"]},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"kind\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.previous`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.scroll`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"to\":{\"type\":\"integer\",\"minimum\":0,\"maximum\":4294967295}},\"required\":[\"to\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.settings`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `page.shortcuts`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `prompt.ask`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"parameter\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"command\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `prompt.clear`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `prompt.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.chord.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.chord.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"chord\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"chord\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.chords.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.chords.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"chords\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16},\"chord\":{\"type\":\"boolean\"}},\"required\":[\"command\",\"key\",\"chord\"],\"additionalProperties\":false},\"maxItems\":146}},\"required\":[\"chords\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.leader.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.leader.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"leader\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"leader\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.mcp.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.mcp.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"on\":{\"type\":\"boolean\"}},\"required\":[\"on\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.menu-keys.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `settings.menu-keys.set`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"keys\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"command\",\"key\"],\"additionalProperties\":false},\"maxItems\":73}},\"required\":[\"keys\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `shell.drag`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `shell.focus`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"window\":{\"type\":\"integer\",\"minimum\":1},\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\",\"prompt\",\"sidebar\",\"info\",\"modeline\"]}},\"required\":[\"region\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `shell.hover`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"by\":{\"type\":\"string\",\"enum\":[\"pointer\",\"keyboard\"]},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"by\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `shell.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `sidebar.cycle`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `sidebar.hide`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `sidebar.show`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `surfaces.activate`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"node\":{\"type\":\"string\",\"maxLength\":512},\"inputs\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"value\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"name\",\"value\"],\"additionalProperties\":false},\"maxItems\":2000}},\"required\":[\"surface\",\"node\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `surfaces.list`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `surfaces.read`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"surface\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.close`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.hide`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.move`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"x\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767},\"y\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767}},\"required\":[\"x\",\"y\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.open`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.resize`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"width\":{\"type\":\"integer\",\"minimum\":320,\"maximum\":65535},\"height\":{\"type\":\"integer\",\"minimum\":240,\"maximum\":65535}},\"required\":[\"width\",\"height\"],\"additionalProperties\":false}",
    ),
    ("commands.md", "```"),
    ("commands.md", "## `window.show`"),
    ("commands.md", "```json"),
    (
        "commands.md",
        "{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false}",
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
        "external-identifiers.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "external-identifiers.md",
        "<!-- Source: catalog/external-identifiers.toml, catalog/i18n/ and catalog/wiki/. Run `cargo xtask docs`. -->",
    ),
    ("external-identifiers.md", "# External identifiers"),
    (
        "external-identifiers.md",
        "Every thing outside the platform the catalog identifies — an application, a binary or a configuration file — with what a person is shown of it in each locale and what identifies it on each system it is identified on (`NOCTUA.md §20.4`). Each value is written as its system writes it, copied from a measurement on that system or from the product's own sources, and a value neither confirms is left out: filling the registry is a pending task (`NOCTUA-PLAN.md`, Stage 27+). **Identification is data**: nothing in the platform runs, expands or trusts a value, and a match against a system is a hint, never proof of what was found. `noctua help external-identifiers` lists them with no daemon, in the reader's language.",
    ),
    ("external-identifiers.md", "## `chromium`"),
    ("external-identifiers.md", "## `google-chrome`"),
    (
        "keybindings.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "keybindings.md",
        "<!-- Source: crates/nc-schema/src/keys.rs, crates/nc-channel/src/lib.rs, catalog/commands.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("keybindings.md", "# Keybindings"),
    (
        "keybindings.md",
        "The keys the interface is driven by (`NOCTUA.md §12.8`): the three forms a binding takes, the keys one may name, each channel's default Leader and chord modifier on each keyboard, every binding the catalog declares, and the chords each system keeps for itself.",
    ),
    (
        "keybindings.md",
        "**These are the defaults, and no person's choice.** A person's own keys are a file in their channel's configuration directory, `<config>/keybindings.json`, judged against the other channels' Leaders; `noctua help keybindings` shows the keys in force and why a choice was refused, with no daemon.",
    ),
    ("keybindings.md", "## Forms"),
    (
        "keybindings.md",
        "A key is a physical key, named as the W3C's `KeyboardEvent.code` names it, so neither Shift nor a keyboard layout turns one key into another; `Chord` names a role, so a binding holds whatever modifier its channel makes the chord modifier. A binding has one spelling, and the build refuses any other.",
    ),
    ("keybindings.md", "## Keys"),
    ("keybindings.md", "## Leaders"),
    (
        "keybindings.md",
        "Each Leader held with its channel's chord modifier, which a binding's `Chord` names:",
    ),
    ("keybindings.md", "## Bindings"),
    ("keybindings.md", "## System shortcuts"),
    (
        "keybindings.md",
        "The chords each system keeps for itself, each from the source that lists it. A Leader or a binding pressing one is kept with a warning that names the system, never refused.",
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
        "**Declared is not the same as enforced, and each part of this page has its own answer.** The kernel holds every plugin to every bound under **Kernel**, and the daemon and the IPC channel it is reached through hold to every bound under **Daemon**. A `local` table is held by `crates/nc-local`, which refuses a unit at a `refuse-at-limit` bound as it commits and an agent or a grant as it keeps one, and by the pruner, which `Kernel::pump` and the `limits.prune` command run over the local database — the daemon runs the first every `pump_every_ms` (**Daemon**, below), and the second runs when it is dispatched. Nothing here prunes a `server` table, and there is no rate limiter and no key derivation, as their own sections say. This paragraph said *nothing on this page has a runner* while every `refuse-at-limit` bound under **Tables** was refused at already. One thing is enforced and is not a number — `access_records`' ordering column, which a trigger assigns (`migrations/0006_access_records_orders_by_insertion.sql`), because a bound that orders by a value a caller writes is a bound a caller can move.",
    ),
    (
        "limits.md",
        "**A table is in one of two databases**, and its `Database` column says which: `server` is PostgreSQL, built by `migrations/`, and `local` is the SQLite file on the user's own machine, built by `local-migrations/`. A table whose strategy is `refuse-at-limit` is one whose oldest rows are not the pruner's to take — user state, what a plugin keeps, a blob a unit pinned or the agents a person made and every grant they gave them, or a unit's images and notes, which an undo needs whole and which leave with their unit — so nothing prunes it by its own count: its `Maximum rows` is where its writer refuses a new row, and the refusal is the bound. `Counted over` says whose rows that number counts: a table counted over each plugin's rows in each realm holds that many for every plugin installed in every realm the machine serves, and one counted over each unit's rows that many for every unit the store holds, so its own size is the number times its owners, not the number.",
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
        "The bounds the local kernel holds every plugin to (`§11.3`), every agent a person makes (`§14.6`) and the interface's own state (`§12.1`), read from `[kernel]` and from nowhere else: a plugin's manifest may lower some of them for itself and raise none. How many keys one plugin may keep and how many writes one unit may record are not here — they are `store_entries` and `store_writes` under **Tables**, where their writer refuses at them, so each is one number held in one place.",
    ),
    (
        "limits.md",
        "**Every one is enforced by `crates/nc-kernel`.** It reads them as the ceiling a plugin's manifest may lower, and holds a plugin's instance to its memory, instances, tables, stack and fuel, each call into it to its epoch deadline — the plugin's own running, which a ticker advances every `epoch_tick_ms` — and to the deadline of a whole call tree, every host call it makes but a dispatch to `host_call_deadline_ms`, its kills but those at a call tree's deadline to `restart_budget` inside `restart_window_ms`, its declarations to `commands_per_plugin` and each one's texts to `entry_bytes`, a call tree's dispatches to its deadline, depth and count, each question the decision point asks the directory to `host_call_deadline_ms`, a confirmation or an approval, and a call's wait for a person's confirmation, to `confirmation_ttl_ms` and the calls waiting for one to `confirmations_pending_max`, a dispatch's arguments and answer to `payload_bytes` (`agents.list`'s answer apart, below), the store's keys, values and listings to theirs, what one unit stages to `store_writes`' bound in keys and to `unit_bytes` in bytes, a blob to `blob_bytes`, each read of one to `blob_read_chunk_bytes`, the blob readers and writers a plugin holds to `blob_handles`, the units it holds begun inside one command to `unit_handles`, what the blobs' sweep keeps of a blob no unit holds to `blob_orphan_grace_ms`, how far back an undo and a redo look in a person's history to `undo_history`, a plugin's network request to `net_header_count` headers, `net_request_head_bytes` of URL and headers and `net_request_body_bytes` of body and its answer to `net_header_count` and `net_response_body_bytes`, the transport a daemon sends it with to `net_timeout_ms`, the timers a plugin holds to `timers_per_plugin`, each to `timer_min_interval_ms` and `timer_max_delay_ms`, the events waiting for delivery to `event_queue` and `event_queue_bytes`, the surfaces a plugin holds to `surfaces_per_plugin` and each tree it renders to `ui_tree_nodes`, `ui_tree_depth` and `ui_text_bytes`, a plugin's own log to `log_ring_lines` lines of `log_line_bytes` each, and what a person gives an agent to spend to `agent_budget_max` and how long a grant to one runs to `agent_grant_minutes_max`, through the schemas of `agents.create` and `agents.grant`, which a test holds to them; and the interface's own state (`§12.1`) to `shell_windows_max` windows, `shell_pages_max` pages open, `shell_drawer_pages_max` of them in the drawer, each `shell_trail_max` sub-pages deep at most, `shell_seats_max` seats, a prompt's draft and an info line's message of `shell_text_bytes` each, a window no smaller than `shell_window_min_width` by `shell_window_min_height` and a drawer of at most `shell_drawer_share_max` percent of the height it shares with the workspace area. **`agents.list`'s answer is not held to `payload_bytes`**: the widest list of agents a profile of the shipped world can hold is longer than a payload, and a test holds it within one frame, so a plugin that dispatches it can be handed more than it may answer with itself.",
    ),
    ("limits.md", "## Daemon"),
    (
        "limits.md",
        "The bounds the daemon holds itself to, read from `[daemon]` and from nowhere else (`§11.1`, `§16.2`).",
    ),
    (
        "limits.md",
        "**The mark's lease is enforced by `apps/daemon`.** It raises the identifiers' persisted ceiling `mark_lease_ms` past the clock before it hands out a millisecond past the one it holds, so a daemon stopped at any instant has minted nothing above what its next start resumes at, and a start after a crash waits at most that long for the clock to pass it. The compiler holds the lease below a day, the furthest ahead a minter takes a mark to be early rather than corrupt.",
    ),
    (
        "limits.md",
        "**The `ipc_` bounds are enforced by `crates/nc-ipc`**, which reads them itself: a frame longer than `ipc_frame_max_bytes` is refused from its length prefix, before anything is allocated for it; a handshake not finished within `ipc_handshake_ms` is dropped; each side pings after `ipc_heartbeat_ms` of its own silence and drops a peer silent for `ipc_silent_after_ms`; and the listener it binds closes a connection past `ipc_connections_max` as soon as it is accepted. The compiler holds a frame to the kernel's `payload_bytes` and 64 KiB more, the silence to more than two heartbeats, and the connections to 252, two fewer than a Windows pipe's instances.",
    ),
    (
        "limits.md",
        "**The schedule and the stop are enforced by `apps/daemon`.** It pumps its kernel every `pump_every_ms` — the timers due, the events queued and the pruner over the local database — and sweeps the blobs every `sweep_every_ms`, each between call trees; a stop begins no pump and no sweep, and waits `stop_drain_ms` at most for the dispatches already running, for a pump or a sweep it finds inside a run, and for each connection to write its last answer, giving up on what still runs then. The compiler holds the pump to the kernel's `timer_min_interval_ms`, the sweep to its `blob_orphan_grace_ms`, and the drain to at least its `dispatch_deadline_ms`, the longest the kernel lets one call tree run: a stop waits out any one tree, and gives up only on trees run one after another — a pump's callbacks, or a dispatch that waited for a pump — that outlast the drain together.",
    ),
    (
        "limits.md",
        "**Process control is enforced by `apps/cli`.** `start` waits `start_within_ms` for the daemon it started to answer the handshake, and calls the start failed when it ends first or does not answer by then; `stop` waits `stop_within_ms` for a daemon that answered it to end, and `kill_within_ms` for one it signalled — asked to end, then killed — through the process its PID record was held to. The compiler holds the start above `ipc_handshake_ms`, so a handshake the daemon is allowed is never cut short, and the stop to at least twice `stop_drain_ms`, which a stopping daemon waits once for what it still runs and once more for its runtime's last work.",
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
        "**`nc-kernel` and `nc-daemon` are the crates outside `nc-log` that log.** The kernel emits every `kernel.*` event below, `net.refused`, `journal.verified` and `journal.broken`; the daemon emits `ids.clock_backwards`, `ipc.refused`, `daemon.started`, `daemon.stopped`, `daemon.plugin_refused`, `daemon.prune_stuck`, `daemon.leader_refused`, `daemon.session_opened`, `daemon.session_ended` and `daemon.session_refused`; every other event is declared for a later stage, and `cargo xtask check` counts the crates that may log beside their logging call sites. The types and the guarantee are real today.",
    ),
    ("log-events.md", "## `daemon.leader_refused`"),
    (
        "log-events.md",
        "The daemon, where it is the process that asks the system for the channel's Leader, was refused it, or lost the registration it held; logged by its code, which the interface's info line says and the status shows, and the daemon runs on without the Leader",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `daemon.plugin_refused`"),
    (
        "log-events.md",
        "A plugin installed in the profile was not loaded, by the code of what refused it; the daemon runs without it",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `daemon.prune_stuck`"),
    (
        "log-events.md",
        "The pruner was refused on every pump of a run of them — a table skipped every tick is a refusal wearing a wait's clothes — logged at two pumps and at each doubling after",
    ),
    ("log-events.md", "Level: `error`"),
    ("log-events.md", "## `daemon.session_ended`"),
    (
        "log-events.md",
        "An agent's session ended with its connection, which the daemon or its client closed, or which was lost",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `daemon.session_opened`"),
    (
        "log-events.md",
        "A connection opened the session of an agent the profile's person made: from then on each dispatch on it is decided as that agent, for that person",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `daemon.session_refused`"),
    (
        "log-events.md",
        "A session was refused as it opened — the daemon not running its kernel yet or stopping, past `sessions_max`, MCP off or its switch unreadable, an agent the person did not make, or a directory that did not answer — or, once open, asked what no session is answered, or was ended because MCP was off or its switch unreadable; logged by its code, since no command was dispatched, and nothing of it is journaled",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `daemon.started`"),
    (
        "log-events.md",
        "The daemon reached running: its kernel is open, its plugins loaded or refused, and its schedule begun",
    ),
    ("log-events.md", "Level: `info`"),
    ("log-events.md", "## `daemon.stopped`"),
    (
        "log-events.md",
        "The daemon stopped when it was asked to: its dispatches and its schedule drained or given up, its mark settled and its endpoint gone",
    ),
    ("log-events.md", "Level: `info`"),
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
    ("log-events.md", "## `ipc.refused`"),
    (
        "log-events.md",
        "The daemon refused a connection before it read a request — a peer not shown to be its user, one past the bound, or a handshake that did not prove the token — or closed a proven one that broke the protocol, fell silent, took nothing or was lost",
    ),
    ("log-events.md", "Level: `warn`"),
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
    ("log-events.md", "## `kernel.shell_discarded`"),
    (
        "log-events.md",
        "The kernel, opening, set the interface's saved state aside and began from the first run's: a state of another shape, one of its own shape breaking a rule it holds the state to, or one the store could not read",
    ),
    ("log-events.md", "Level: `warn`"),
    ("log-events.md", "## `kernel.shell_unkept`"),
    (
        "log-events.md",
        "The kernel could not save the interface's state to the local store: a pump tries again at the next one, and a stop loses what changed since the last save",
    ),
    ("log-events.md", "Level: `warn`"),
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
        "mcp-tools.md",
        "<!-- Generated by nc-catalog-compiler — do not edit. -->",
    ),
    (
        "mcp-tools.md",
        "<!-- Source: catalog/commands.toml and catalog/i18n/. Run `cargo xtask docs`. -->",
    ),
    ("mcp-tools.md", "# MCP tools"),
    (
        "mcp-tools.md",
        "Every command the catalog declares, as the tool definition an MCP session is handed (`NOCTUA.md §20.3`): the tool's name is the command's with each dot written as an underscore, because model APIs refuse a dot in a tool's name, and a call naming it is read back to the command through the one parser of a command's name; its description is the command's line and paragraph; its input schema is the command's argument schema, as the kernel holds a dispatch to it; and its one annotation, `readOnlyHint`, is true exactly when the command is not `mutating` — a read, which makes no step. Each is written once for each locale, by the one function the MCP server lists its tools with.",
    ),
    (
        "mcp-tools.md",
        "**A session sees only the tools it may dispatch**: a session lists those of these whose resource is known without their arguments, that are not irreversible, and that it may dispatch now, decided when it asks, for the principal it is, as each of its calls is decided (`§20.3`, `§14.6`). **A plugin's tools come from the daemon**: a plugin declares its commands at `activate`, so no page lists them, and the daemon's registry holds them while the plugin is loaded (`§15.3`). **`noctua mcp <agent>` serves them**: to an MCP client on its standard input and output, as the agent it names, in the language its environment names, once MCP is switched on in its channel (`§20.3`).",
    ),
    ("mcp-tools.md", "## `agents.create`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_create\",\"description\":\"Make an agent that acts for you, with a budget of its own.\\n\\nMakes a new agent of yours: an identity of its own that an MCP client acts as, for you alone. You give it a label and a budget, which each of its sessions spends one at a time, and it can do nothing until you grant it a feature. Only you can make one, asking directly, never from inside another action. An agent is never deleted, so undo stops at the step that made it.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"label\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"budget\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":100000}},\"required\":[\"label\",\"budget\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_create\",\"description\":\"Criar um agente que age por você, com um orçamento próprio.\\n\\nCria um novo agente seu: uma identidade própria, com a qual um cliente MCP age só por você. Você lhe dá um nome e um orçamento, que cada sessão dele gasta uma unidade por vez, e ele não pode fazer nada até que você lhe dê acesso a um recurso. Só você pode criar um, pedindo diretamente, nunca de dentro de outra ação. Um agente nunca é apagado, então desfazer para no passo que o criou.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"label\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"budget\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":100000}},\"required\":[\"label\",\"budget\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `agents.grant`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_grant\",\"description\":\"Let one of your agents use a feature, for a reason and a time you give.\\n\\nLets one of your agents, named by the identifier Noctua gave it, use one feature for as many minutes as you say, within the longest a grant may run, and keeps why. A new grant on the same feature ends the one before it, and the agent still does only what you yourself may do. Only you can grant it, asking directly, never from inside another action. To take it back, revoke it: undo stops at a grant.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"minutes\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":43200},\"reason\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":256}},\"required\":[\"agent\",\"feature\",\"minutes\",\"reason\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_grant\",\"description\":\"Deixar um agente seu usar um recurso, por um motivo e um prazo que você dá.\\n\\nDeixa um agente seu, indicado pelo identificador que o Noctua lhe deu, usar um recurso por quantos minutos você disser, até o prazo mais longo que uma permissão pode ter, e guarda o motivo. Uma nova permissão no mesmo recurso encerra a anterior, e o agente continua fazendo só o que você mesmo pode fazer. Só você pode dar a permissão, pedindo diretamente, nunca de dentro de outra ação. Para retirá-la, revogue: desfazer para numa permissão dada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"minutes\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":43200},\"reason\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":256}},\"required\":[\"agent\",\"feature\",\"minutes\",\"reason\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `agents.list`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_list\",\"description\":\"Show the agents you made, each with what you granted it and until when.\\n\\nAnswers every agent you made, in the order you made them, with its label, its budget and each grant you gave it and have not ended, even one whose time is up: its feature, why you gave it, and from when until when. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_list\",\"description\":\"Mostrar os agentes que você criou, cada um com o que lhe deu e até quando.\\n\\nResponde cada agente que você criou, na ordem em que os criou, com o nome, o orçamento e cada permissão que você lhe deu e não encerrou, mesmo a que já venceu: o recurso, o motivo e de quando até quando. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `agents.revoke`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_revoke\",\"description\":\"End what you granted one of your agents: one feature, or all of them.\\n\\nEnds the grant one of your agents holds on the feature you name, or every grant it holds when you name none, from its very next action. The agent itself is kept, with its budget, for you to grant again. Only you can revoke, asking directly, never from inside another action, and undo stops at a revocation.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"agent\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"agents_revoke\",\"description\":\"Encerrar o que você deu a um agente seu: um recurso, ou todos.\\n\\nEncerra a permissão que um agente seu tem no recurso que você indicar, ou todas as que ele tem se você não indicar nenhum, a partir da próxima ação dele. O agente continua guardado, com o orçamento, para você lhe dar acesso de novo. Só você pode revogar, pedindo diretamente, nunca de dentro de outra ação, e desfazer para numa revogação.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"agent\":{\"type\":\"string\",\"minLength\":12,\"maxLength\":13},\"feature\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"agent\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `confirmations.list`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"confirmations_list\",\"description\":\"Show every call waiting for a person to confirm it in the Noctua window.\\n\\nLists every call Noctua holds until a person confirms it in the Noctua window — one that cannot be undone, or one that switches MCP on: its number, the command, whether it can be undone, the person whose confirmation it waits for and the agent that asked for it when an agent did, and when the wait ends. Only the Noctua window can confirm or decline one; this command only shows them, and changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"confirmations_list\",\"description\":\"Mostrar cada chamada que aguarda uma pessoa confirmá-la na janela do Noctua.\\n\\nLista cada chamada que o Noctua segura até que uma pessoa a confirme na janela do Noctua — uma que não pode ser desfeita, ou uma que liga o MCP: seu número, o comando, se pode ser desfeita, a pessoa cuja confirmação ela aguarda e o agente que a pediu, quando foi um agente, e quando a espera termina. Só a janela do Noctua pode confirmar ou recusar uma delas; este comando apenas as mostra e não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `confirmations.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"confirmations_read\",\"description\":\"Show one call waiting for a confirmation, with the arguments it would run with.\\n\\nShows the call Noctua holds under the number given until a person confirms it in the Noctua window: the command, the arguments it would run with, the person whose confirmation it waits for, the agent that asked for it when an agent did, and when the wait ends. A number nothing waits under any more is refused. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"serial\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"serial\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"confirmations_read\",\"description\":\"Mostrar uma chamada que aguarda confirmação, com os argumentos com que rodaria.\\n\\nMostra a chamada que o Noctua segura sob o número dado até que uma pessoa a confirme na janela do Noctua: o comando, os argumentos com que rodaria, a pessoa cuja confirmação ela aguarda, o agente que a pediu, quando foi um agente, e quando a espera termina. Um número sob o qual nada mais aguarda é recusado. Não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"serial\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"serial\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `drawer.back`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_back\",\"description\":\"Leave a sub-page of the drawer's page, or close that page if it has none.\\n\\nDoes what Esc does in the drawer: it goes back one step inside the drawer's page in focus or, when that page is not inside a sub-page, closes it in the drawer; a seat whose keyboard was on a drawer left showing nothing goes back to the prompt line.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_back\",\"description\":\"Sair de uma subpágina da página da gaveta, ou fechar a página se não houver.\\n\\nFaz o que o Esc faz na gaveta: volta um passo dentro da página em foco na gaveta ou, quando essa página não está numa subpágina, fecha-a na gaveta; um assento cujo teclado estava numa gaveta que fica sem mostrar nada volta para a linha do prompt.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `drawer.collapse`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_collapse\",\"description\":\"Hide the whole drawer, keeping every page open in it.\\n\\nHides the drawer below the footer, keeping the pages open in it; the end of the modeline then shows how many there are. Expanding it brings everything back.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_collapse\",\"description\":\"Esconder a gaveta inteira, mantendo abertas todas as páginas dela.\\n\\nEsconde a gaveta abaixo do rodapé, mantendo abertas as páginas que há nela; o fim da linha de modo passa a mostrar quantas são. Expandi-la traz tudo de volta.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `drawer.expand`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_expand\",\"description\":\"Show the drawer again, with everything that was open in it.\\n\\nShows the drawer below the footer again, as it was when it was collapsed.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_expand\",\"description\":\"Mostrar a gaveta de novo, com tudo o que estava aberto nela.\\n\\nMostra de novo a gaveta abaixo do rodapé, como estava quando foi recolhida.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `drawer.resize`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_resize\",\"description\":\"Set how much of the height the drawer takes, below half.\\n\\nSets how much of the height the drawer shares with the workspace area it takes, in percent. It always stays shorter than the workspace area.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"share\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":49}},\"required\":[\"share\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_resize\",\"description\":\"Definir quanto da altura a gaveta ocupa, sempre menos da metade.\\n\\nDefine quanto da altura que a gaveta divide com a área de trabalho ela ocupa, em porcentagem. Ela sempre fica mais baixa que a área de trabalho.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"share\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":49}},\"required\":[\"share\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `drawer.toward`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_toward\",\"description\":\"Open the leader menu's pages in the workspace area or in the drawer.\\n\\nChooses where a page opened from the leader menu goes: the workspace area, where they go at first, or the drawer. The choice is kept until you change it.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]}},\"required\":[\"region\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"drawer_toward\",\"description\":\"Abrir as páginas do menu do Leader na área de trabalho ou na gaveta.\\n\\nEscolhe para onde vai uma página aberta pelo menu do Leader: a área de trabalho, para onde elas vão no início, ou a gaveta. A escolha fica guardada até você mudá-la.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]}},\"required\":[\"region\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `footer.hide`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"footer_hide\",\"description\":\"Let the footer hide again where the setting hides it.\\n\\nLets the footer hide again where it is set to hide. Where it is set to always show, it stays.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"footer_hide\",\"description\":\"Deixar o rodapé se esconder de novo onde a configuração o esconde.\\n\\nDeixa o rodapé se esconder de novo onde ele está configurado para isso. Onde está configurado para aparecer sempre, ele fica.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `footer.show`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"footer_show\",\"description\":\"Show the footer's three lines now, even where the setting hides it.\\n\\nShows the footer's three lines now. Where the footer is set to hide, it stays until nothing holds it there any more.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"footer_show\",\"description\":\"Mostrar agora as três linhas do rodapé, mesmo onde a configuração o esconde.\\n\\nMostra agora as três linhas do rodapé. Onde o rodapé está configurado para se esconder, ele fica até que nada mais o segure ali.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `info.clear`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_clear\",\"description\":\"Take the message off the info line.\\n\\nTakes the message off the info line, which goes back to saying what each seat is on.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_clear\",\"description\":\"Tirar a mensagem da linha de informações.\\n\\nTira a mensagem da linha de informações, que volta a dizer onde cada assento está.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `info.collapse`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_collapse\",\"description\":\"Keep the info line to one line again.\\n\\nKeeps the info line to one line again, cutting what does not fit.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_collapse\",\"description\":\"Manter a linha de informações numa linha só de novo.\\n\\nMantém a linha de informações numa linha só de novo, cortando o que não couber.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `info.expand`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_expand\",\"description\":\"Let the info line grow to show all it holds, each hover on a line of its own.\\n\\nLets the info line wrap and grow to show everything it holds, with what the pointer is over and what the keyboard is on each on a line of its own.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_expand\",\"description\":\"Deixar a linha de informações crescer para mostrar tudo o que tem.\\n\\nDeixa a linha de informações quebrar e crescer para mostrar tudo o que tem, com o que está sob o ponteiro e onde está o teclado, cada um numa linha própria.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `info.say`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_say\",\"description\":\"Show a short message on the info line until it is cleared.\\n\\nShows a message on the info line, the middle line of the footer, in place of the one before it.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"info_say\",\"description\":\"Mostrar uma mensagem curta na linha de informações até que seja limpa.\\n\\nMostra uma mensagem na linha de informações, a linha do meio do rodapé, no lugar da anterior.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `journal.verify`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"journal_verify\",\"description\":\"Check that every recorded action is intact and in order.\\n\\nReads the activity history from its start, or from the entry you name, and checks that each recorded action is still linked to the one before it. It changes nothing, and answers how many entries it checked, or the first one that does not hold and why.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"from\":{\"type\":\"integer\",\"minimum\":0}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"journal_verify\",\"description\":\"Conferir que toda ação registrada está intacta e em ordem.\\n\\nLê o histórico de atividades desde o início, ou desde a entrada que você indicar, e confere se cada ação registrada continua ligada à anterior. Não muda nada, e responde quantas entradas conferiu, ou a primeira que não confere e por quê.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"from\":{\"type\":\"integer\",\"minimum\":0}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `kernel.find`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_find\",\"description\":\"Find commands by any word of their names or tags, in English or Portuguese.\\n\\nLooks through every command Noctua can run, its own and every plugin's, for those whose names and tags, in English and in Portuguese, hold every word you type, each as letters in order, with accents and capitals ignored. It answers the best matches first, at most as many as you ask for, with where each word was found, and how many were found in all. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"query\":{\"type\":\"string\",\"maxLength\":64},\"limit\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":32}},\"required\":[\"query\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_find\",\"description\":\"Encontrar comandos pelas palavras do nome ou das etiquetas, nas duas línguas.\\n\\nProcura, entre todos os comandos que o Noctua pode executar, os dele e os de cada plugin, aqueles cujos nomes e etiquetas, em inglês e em português, contêm cada palavra digitada, com as letras na ordem, sem distinguir acentos nem maiúsculas. Responde primeiro os melhores resultados, no máximo quantos você pedir, com onde cada palavra foi encontrada e quantos foram encontrados ao todo. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"query\":{\"type\":\"string\",\"maxLength\":64},\"limit\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":32}},\"required\":[\"query\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `kernel.history`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_history\",\"description\":\"Name the step your next undo takes back, and whether an agent took it.\\n\\nShows the step your next undo would take back: its name, when it was taken, whether undo stops there, and the agent that took it when one of your agents did — an agent's steps are yours to undo, as your own are. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_history\",\"description\":\"Dizer qual passo o seu próximo desfazer desfaz, e se um agente o deu.\\n\\nMostra o passo que o seu próximo desfazer desfaria: seu nome, quando foi dado, se o desfazer para ali e o agente que o deu, quando foi um dos seus agentes — os passos de um agente são seus para desfazer, como os seus. Não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `kernel.redo`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_redo\",\"description\":\"Make again the last step you took back.\\n\\nPuts back the newest step you took back, as long as you have done nothing new since, after checking again that you may do each thing it did. A step whose undo needed corrections of its own cannot be redone.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_redo\",\"description\":\"Fazer de novo o último passo que você desfez.\\n\\nRefaz o passo mais recente que você desfez, desde que você não tenha feito nada novo depois, conferindo de novo que você pode fazer cada coisa que ele fez. Um passo cujo desfazer precisou de correções próprias não pode ser refeito.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `kernel.undo`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_undo\",\"description\":\"Take back your last step: what it changed in Noctua, and the files it made.\\n\\nFinds your newest step that can still be taken back and puts back every value it changed and the files it made, after checking again that you may do each thing it did. It stops at a step whose effects left this device, and at one whose values changed since.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"kernel_undo\",\"description\":\"Desfazer o seu último passo: o que ele mudou no Noctua e os arquivos que criou.\\n\\nEncontra o seu passo mais recente que ainda pode ser desfeito e devolve cada valor que ele mudou e os arquivos que criou, depois de conferir de novo que você pode fazer cada coisa que ele fez. Para em um passo cujos efeitos saíram deste aparelho, e em um cujos valores mudaram desde então.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `leader.tap`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"leader_tap\",\"description\":\"Do what one tap of the Leader key does in the interface.\\n\\nDoes what a tap of the Leader does: it shows the interface and its footer and puts the keyboard on the prompt line; tapped again there, it opens the leader menu in the drawer; and tapped while the menu is open in a window shown, it sends the next page opened to the other region — the drawer instead of the workspace area, or the other way round.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"leader_tap\",\"description\":\"Fazer o que um toque da tecla Leader faz na interface.\\n\\nFaz o que um toque da tecla Leader faz: mostra a interface e o rodapé e leva o teclado para a linha do prompt; tocada de novo ali, abre o menu do Leader na gaveta; e tocada com o menu aberto numa janela à vista, manda a próxima página aberta para a outra região — a gaveta em vez da área de trabalho, ou o contrário.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `limits.prune`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"limits_prune\",\"description\":\"Remove the oldest records from any table that has passed its limit.\\n\\nDeletes the oldest records from each table of this device's database that has passed its declared limit, or only from the table you name. A dry run counts what it would remove and removes nothing. What it removes cannot be brought back, so undo stops at the step that ran it.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"table\":{\"type\":\"string\"},\"dry_run\":{\"type\":\"boolean\"}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"limits_prune\",\"description\":\"Remover os registros mais antigos de qualquer tabela que passou do limite.\\n\\nApaga os registros mais antigos de cada tabela do banco de dados deste aparelho que passou do limite declarado, ou só da tabela que você indicar. Um ensaio conta o que removeria e não remove nada. O que ele remove não pode ser recuperado, então desfazer para no passo que o executou.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"table\":{\"type\":\"string\"},\"dry_run\":{\"type\":\"boolean\"}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `naming.propose`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"naming_propose\",\"description\":\"Build a file name from a title, signature, keywords and extension.\\n\\nWrites the name Noctua gives a saved file: its signature, title, keywords, identifier and extension, each cleaned of what a file name cannot hold. A file being renamed keeps the identifier you give, so it stays the same file; without one, a new identifier is made. It saves nothing and changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"title\":{\"type\":\"string\",\"maxLength\":1024},\"signature\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"keywords\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"extension\":{\"type\":\"string\",\"maxLength\":255},\"id\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":13}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"naming_propose\",\"description\":\"Montar o nome de um arquivo com título, assinatura, palavras-chave e extensão.\\n\\nEscreve o nome que o Noctua dá a um arquivo salvo: a assinatura, o título, as palavras-chave, o identificador e a extensão, cada um limpo do que um nome de arquivo não comporta. Um arquivo renomeado mantém o identificador que você informar e continua sendo o mesmo arquivo; sem ele, um identificador novo é criado. Não salva nada e não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"title\":{\"type\":\"string\",\"maxLength\":1024},\"signature\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"keywords\":{\"type\":\"array\",\"items\":{\"type\":\"string\",\"maxLength\":255},\"maxItems\":128},\"extension\":{\"type\":\"string\",\"maxLength\":255},\"id\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":13}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `naming.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"naming_read\",\"description\":\"Read a file name back into its identifier, signature, title and keywords.\\n\\nReads a name written in Noctua's file-naming scheme and answers what it holds: the identifier, the signature, the title, the keywords and the extension. It also says whether the name is already written exactly as Noctua writes it, and gives the spelling Noctua would use. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":255}},\"required\":[\"name\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"naming_read\",\"description\":\"Ler um nome de arquivo: identificador, assinatura, título e palavras-chave.\\n\\nLê um nome escrito no esquema de nomes de arquivo do Noctua e responde o que ele contém: o identificador, a assinatura, o título, as palavras-chave e a extensão. Também diz se o nome já está escrito exatamente como o Noctua escreve, e dá a grafia que o Noctua usaria. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":255}},\"required\":[\"name\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.actions`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_actions\",\"description\":\"Open the actions on the window and the page in focus, in the drawer.\\n\\nOpens, in the drawer, the list of what can be done to the window — hide it, close it, move it, resize it — as Leader w does. Choosing one runs it, asking for what it needs on the prompt line.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_actions\",\"description\":\"Abrir na gaveta as ações sobre a janela e a página em foco.\\n\\nAbre, na gaveta, a lista do que se pode fazer com a janela — escondê-la, fechá-la, movê-la, redimensioná-la — como Leader w faz. Escolher uma a executa, pedindo na linha de comando o que ela precisar.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.back`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_back\",\"description\":\"Return from a sub-page to the page it was opened from.\\n\\nGoes back one step inside a page — the one in focus, unless you name another. A page that is not inside a sub-page stays as it is.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_back\",\"description\":\"Sair de uma subpágina e voltar para a página de onde ela foi aberta.\\n\\nVolta um passo dentro de uma página — a que está em foco, a menos que você indique outra. Uma página que não está numa subpágina fica como está.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.close`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_close\",\"description\":\"Close a page of the interface, in one place or everywhere it is shown.\\n\\nCloses the page in focus in a window's workspace area or in the drawer, or the page you name — from the place you say, or from everywhere it is shown. A page nothing shows any more is closed for good; anything it started keeps running.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_close\",\"description\":\"Fechar uma página da interface, num lugar ou em todos os lugares onde aparece.\\n\\nFecha a página em foco na área de trabalho de uma janela ou na gaveta, ou a página que você indicar — do lugar que você disser, ou de todos os lugares onde aparece. Uma página que nada mais mostra é fechada de vez; o que ela começou continua rodando.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.dev`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_dev\",\"description\":\"Open the pages for developing and running Noctua.\\n\\nOpens the list of dev pages, as Leader d does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Each dev page is authorized as any command is.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_dev\",\"description\":\"Abrir as páginas para desenvolver e operar o Noctua.\\n\\nAbre a lista de páginas de dev, como Leader d faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Cada página de dev é autorizada como qualquer comando.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.focus`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_focus\",\"description\":\"Show a page that is open, in the workspace area or in the drawer.\\n\\nBrings a page that is already open to the front of the place that shows it: a window's workspace area, or the drawer, which is expanded if it was collapsed.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"page\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_focus\",\"description\":\"Mostrar uma página aberta, na área de trabalho ou na gaveta.\\n\\nTraz para a frente uma página que já está aberta, no lugar que a mostra: a área de trabalho de uma janela, ou a gaveta, que é expandida se estava recolhida.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"page\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.help`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_help\",\"description\":\"Open the help pages, where the leader menu's direction says.\\n\\nOpens the list of help pages, as Leader h does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Help already open is shown again there.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_help\",\"description\":\"Abrir as páginas de ajuda, onde a direção do menu do Leader indicar.\\n\\nAbre a lista de páginas de ajuda, como Leader h faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Se a ajuda já está aberta, aparece de novo ali.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.into`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_into\",\"description\":\"Open a sub-page inside a page, such as one topic of help.\\n\\nGoes one step deeper inside a page — the one in focus, unless you name another — to the topic given, so going back returns to where it was. A page goes only so many steps deep.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"topic\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_into\",\"description\":\"Abrir uma subpágina dentro de uma página, como um tópico da ajuda.\\n\\nAvança um passo para dentro de uma página — a que está em foco, a menos que você indique outra — até o tópico indicado, para que voltar retorne a onde estava. Uma página só desce um número limitado de passos.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"topic\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.next`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_next\",\"description\":\"Bring the next page of a window's stack to the front.\\n\\nBrings to the front the page after the one in focus in a window's workspace area. At the last page, nothing changes.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_next\",\"description\":\"Trazer para a frente a próxima página da pilha de uma janela.\\n\\nTraz para a frente a página que vem depois da que está em foco na área de trabalho de uma janela. Na última página, nada muda.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.open`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_open\",\"description\":\"Open a page of the interface: settings, help, dev, a surface or a list.\\n\\nOpens a page in the workspace area of a window or in the drawer — where the drawer's direction says, unless you choose. A page already open is shown again where it is asked for, as one more view of the same page. Opening a page closes the leader menu.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"kind\":{\"type\":\"string\",\"enum\":[\"home\",\"settings\",\"help\",\"dev\",\"surface\",\"leader\",\"commands\",\"shortcuts\",\"actions\"]},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"kind\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_open\",\"description\":\"Abrir uma página: configurações, ajuda, dev, uma superfície ou uma lista.\\n\\nAbre uma página na área de trabalho de uma janela ou na gaveta — onde a direção da gaveta indicar, a menos que você escolha. Uma página que já está aberta aparece de novo onde for pedida, como mais uma vista da mesma página. Abrir uma página fecha o menu do Leader.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"kind\":{\"type\":\"string\",\"enum\":[\"home\",\"settings\",\"help\",\"dev\",\"surface\",\"leader\",\"commands\",\"shortcuts\",\"actions\"]},\"topic\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1}},\"required\":[\"kind\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.previous`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_previous\",\"description\":\"Bring the previous page of a window's stack to the front.\\n\\nBrings to the front the page before the one in focus in a window's workspace area. At the first page, nothing changes.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_previous\",\"description\":\"Trazer para a frente a página anterior da pilha de uma janela.\\n\\nTraz para a frente a página que vem antes da que está em foco na área de trabalho de uma janela. Na primeira página, nada muda.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.scroll`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_scroll\",\"description\":\"Set how far a page is scrolled, so it reopens where it was.\\n\\nSets how far down a page is scrolled, in CSS pixels — the page in focus, unless you name another — so every window shows it there and reopening it brings you back to the same place.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"to\":{\"type\":\"integer\",\"minimum\":0,\"maximum\":4294967295}},\"required\":[\"to\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_scroll\",\"description\":\"Definir até onde uma página está rolada, para que reabra onde estava.\\n\\nDefine até onde uma página está rolada para baixo, em pixels CSS — a página em foco, a menos que você indique outra — para que todas as janelas a mostrem ali e reabri-la leve você de volta ao mesmo ponto.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"page\":{\"type\":\"integer\",\"minimum\":1},\"in\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\"]},\"window\":{\"type\":\"integer\",\"minimum\":1},\"to\":{\"type\":\"integer\",\"minimum\":0,\"maximum\":4294967295}},\"required\":[\"to\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.settings`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_settings\",\"description\":\"Open the settings, where the leader menu's direction says.\\n\\nOpens the list of settings, as Leader s does: in the workspace area, or in the drawer when the leader menu's direction points there or a Leader tap flipped it for this page. Settings already open is shown again there.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_settings\",\"description\":\"Abrir as configurações, onde a direção do menu do Leader indicar.\\n\\nAbre a lista de configurações, como Leader s faz: na área de trabalho, ou na gaveta quando a direção do menu do Leader aponta para lá ou um toque no Leader a inverteu para esta página. Se as configurações já estão abertas, aparecem de novo ali.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `page.shortcuts`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_shortcuts\",\"description\":\"Open the shortcuts of the page in focus, in the drawer.\\n\\nOpens, in the drawer, the list of the shortcuts the page in focus keeps for itself, as Leader / does. A page keeps its own shortcuts there rather than in the global bindings.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"page_shortcuts\",\"description\":\"Abrir na gaveta os atalhos da página em foco.\\n\\nAbre, na gaveta, a lista dos atalhos que a página em foco guarda para si, como Leader / faz. Uma página guarda ali os seus próprios atalhos, e não nos atalhos globais.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `prompt.ask`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_ask\",\"description\":\"Keep the prompt line on a command, asking for its parameters.\\n\\nKeeps the prompt line on the command you chose, offering its parameters — on the one you name, when you name one — so what you type next is a parameter's value.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"parameter\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"command\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_ask\",\"description\":\"Manter a linha do prompt num comando, pedindo os parâmetros dele.\\n\\nMantém a linha do prompt no comando que você escolheu, oferecendo os parâmetros dele — no que você indicar, quando indicar um — para que o que você digitar em seguida seja o valor de um parâmetro.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"parameter\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"command\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `prompt.clear`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_clear\",\"description\":\"Empty the prompt line and stop asking for anything.\\n\\nEmpties the prompt line and stops asking for a command or its parameters, closing the list of commands in the drawer.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_clear\",\"description\":\"Esvaziar a linha do prompt e parar de pedir qualquer coisa.\\n\\nEsvazia a linha do prompt e para de pedir um comando ou os parâmetros dele, fechando a lista de comandos na gaveta.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `prompt.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_set\",\"description\":\"Set what the prompt line holds, as if it were typed there.\\n\\nSets the text of the prompt line, the bottom line of the footer. Text that begins with a semicolon or a colon opens the list of commands in the drawer, and the rest of it filters the list.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"prompt_set\",\"description\":\"Definir o que a linha do prompt contém, como se fosse digitado ali.\\n\\nDefine o texto da linha do prompt, a última linha do rodapé. Um texto que começa com ponto e vírgula ou dois-pontos abre a lista de comandos na gaveta, e o resto dele filtra a lista.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"text\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"text\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.chord.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chord_read\",\"description\":\"Show the key held with the Leader for more shortcuts.\\n\\nAnswers the key that, held with the Leader, reaches a second set of shortcuts in this installation of Noctua. It is Shift until you choose another. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chord_read\",\"description\":\"Mostrar a tecla pressionada com o Leader para mais atalhos.\\n\\nResponde qual tecla, pressionada junto com o Leader, alcança um segundo conjunto de atalhos nesta instalação do Noctua. É Shift até você escolher outra. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.chord.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chord_set\",\"description\":\"Choose the key held with the Leader for more shortcuts.\\n\\nChanges the key that, held with the Leader, reaches a second set of shortcuts, in this installation of Noctua and in no other: one of Control, Alt, Shift and Super. It is refused when the Leader already holds it, or when the Leader held with it would be another installation's Leader. A refused choice changes nothing. Only you can change it, asking directly, never from inside another action. Undo stops at this step: to go back, choose the key you had.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"chord\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"chord\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chord_set\",\"description\":\"Escolher a tecla pressionada com o Leader para mais atalhos.\\n\\nMuda a tecla que, pressionada junto com o Leader, alcança um segundo conjunto de atalhos, nesta instalação do Noctua e em nenhuma outra: uma entre Control, Alt, Shift e Super. É recusada quando o Leader já a inclui, ou quando o Leader com ela seria o Leader de outra instalação. Uma escolha recusada não muda nada. Só você pode mudá-la, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, escolha a tecla que você tinha.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"chord\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"chord\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.chords.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chords_read\",\"description\":\"Show each key pressed with the Leader held and what it runs.\\n\\nAnswers every key that acts while the Leader is held in this installation of Noctua, each beside the command it runs and whether the added key is held as well: Noctua's own keys, with your choices over them. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chords_read\",\"description\":\"Mostrar cada tecla pressionada com o Leader e o que ela executa.\\n\\nResponde todas as teclas que agem enquanto o Leader está pressionado nesta instalação do Noctua, cada uma ao lado do comando que executa e se a tecla acrescentada também é pressionada: as teclas do próprio Noctua, com as suas escolhas por cima. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.chords.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chords_set\",\"description\":\"Choose each key pressed with the Leader held and what it runs.\\n\\nReplaces every key that acts while the Leader is held in this installation of Noctua, and in no other, with the keys you give, each beside the command it runs and whether the added key is held as well. A command you leave out has no chord. One key runs one command, so a key given twice the same way is refused, and a refused choice changes nothing. Only you can change them, asking directly, never from inside another action. Undo stops at this step: to go back, give the keys you had.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"chords\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16},\"chord\":{\"type\":\"boolean\"}},\"required\":[\"command\",\"key\",\"chord\"],\"additionalProperties\":false},\"maxItems\":146}},\"required\":[\"chords\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_chords_set\",\"description\":\"Escolher cada tecla pressionada com o Leader e o que ela executa.\\n\\nSubstitui todas as teclas que agem enquanto o Leader está pressionado nesta instalação do Noctua, e em nenhuma outra, pelas teclas que você informar, cada uma ao lado do comando que executa e se a tecla acrescentada também é pressionada. Um comando que você deixar de fora fica sem combinação. Uma tecla executa um comando só, então uma tecla informada duas vezes do mesmo jeito é recusada, e uma escolha recusada não muda nada. Só você pode mudá-las, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, informe as teclas que você tinha.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"chords\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16},\"chord\":{\"type\":\"boolean\"}},\"required\":[\"command\",\"key\",\"chord\"],\"additionalProperties\":false},\"maxItems\":146}},\"required\":[\"chords\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.leader.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_leader_read\",\"description\":\"Show the keys that reach Noctua from anywhere.\\n\\nAnswers the Leader in use in this installation of Noctua: the keys that show Noctua from anywhere on the computer, written as their places on the keyboard. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_leader_read\",\"description\":\"Mostrar as teclas que chamam o Noctua de qualquer lugar.\\n\\nResponde qual é o Leader em uso nesta instalação do Noctua: as teclas que mostram o Noctua de qualquer lugar do computador, escritas pela posição delas no teclado. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.leader.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_leader_set\",\"description\":\"Choose the keys that reach Noctua from anywhere.\\n\\nChanges the Leader of this installation of Noctua, and of no other. The Leader needs at least one of Control, Alt, Shift and Super held with its key, and it is refused when it would clash with the key added to it or with another installation's Leader, or when the system does not give it to Noctua. A refused choice changes nothing. Only you can change it, asking directly, never from inside another action. Undo stops at this step: to go back, choose the Leader you had.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"leader\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"leader\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_leader_set\",\"description\":\"Escolher as teclas que chamam o Noctua de qualquer lugar.\\n\\nMuda o Leader desta instalação do Noctua, e de nenhuma outra. O Leader precisa de pelo menos uma entre Control, Alt, Shift e Super pressionada junto com a tecla dele, e é recusado quando conflitaria com a tecla acrescentada a ele ou com o Leader de outra instalação, ou quando o sistema não o entrega ao Noctua. Uma escolha recusada não muda nada. Só você pode mudá-lo, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, escolha o Leader que você tinha.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"leader\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"leader\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.mcp.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_mcp_read\",\"description\":\"Show whether this installation of Noctua lets MCP clients in.\\n\\nAnswers whether MCP is on in this installation of Noctua. It is off until a person turns it on, and turning it on here turns it on nowhere else. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_mcp_read\",\"description\":\"Mostrar se esta instalação do Noctua deixa clientes MCP entrarem.\\n\\nResponde se o MCP está ligado nesta instalação do Noctua. Ele fica desligado até que uma pessoa o ligue, e ligá-lo aqui não o liga em nenhuma outra. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.mcp.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_mcp_set\",\"description\":\"Let MCP clients act here as agents, or stop them.\\n\\nTurns MCP on or off in this installation of Noctua, and in no other. While it is off, no MCP client is let in; while it is on, a client acts only as an agent you made and granted, never as you. Only you can switch it, asking directly, never from inside another action, and switching it on waits for you to confirm it in the Noctua window. Undo puts it back as it was; when that would switch it on, the undo waits for you to confirm it in the Noctua window too.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"on\":{\"type\":\"boolean\"}},\"required\":[\"on\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_mcp_set\",\"description\":\"Deixar clientes MCP agirem aqui como agentes, ou impedi-los.\\n\\nLiga ou desliga o MCP nesta instalação do Noctua, e em nenhuma outra. Enquanto está desligado, nenhum cliente MCP entra; enquanto está ligado, um cliente age só como um agente que você criou e autorizou, nunca como você. Só você pode mudá-lo, pedindo diretamente, nunca de dentro de outra ação, e ligá-lo aguarda você confirmar na janela do Noctua. Desfazer o deixa como estava; quando isso o ligaria, o desfazer também aguarda você confirmar na janela do Noctua.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"on\":{\"type\":\"boolean\"}},\"required\":[\"on\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.menu-keys.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_menu-keys_read\",\"description\":\"Show each key of the leader menu and what it runs.\\n\\nAnswers every key of the leader menu in use in this installation of Noctua, each beside the command it runs: Noctua's own keys, with your choices over them. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_menu-keys_read\",\"description\":\"Mostrar cada tecla do menu do Leader e o que ela executa.\\n\\nResponde todas as teclas do menu do Leader em uso nesta instalação do Noctua, cada uma ao lado do comando que executa: as teclas do próprio Noctua, com as suas escolhas por cima. Não muda nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `settings.menu-keys.set`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_menu-keys_set\",\"description\":\"Choose each key of the leader menu and what it runs.\\n\\nReplaces every key of the leader menu in this installation of Noctua, and in no other, with the keys you give, each beside the command it runs. A command you leave out is not in the menu. One key runs one command, so a key given twice is refused, and a refused choice changes nothing. Only you can change them, asking directly, never from inside another action. Undo stops at this step: to go back, give the keys you had.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"keys\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"command\",\"key\"],\"additionalProperties\":false},\"maxItems\":73}},\"required\":[\"keys\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"settings_menu-keys_set\",\"description\":\"Escolher cada tecla do menu do Leader e o que ela executa.\\n\\nSubstitui todas as teclas do menu do Leader nesta instalação do Noctua, e em nenhuma outra, pelas teclas que você informar, cada uma ao lado do comando que executa. Um comando que você deixar de fora não aparece no menu. Uma tecla executa um comando só, então uma tecla informada duas vezes é recusada, e uma escolha recusada não muda nada. Só você pode mudá-las, pedindo diretamente, nunca de dentro de outra ação. Desfazer para neste passo: para voltar, informe as teclas que você tinha.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"keys\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"command\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":96},\"key\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":16}},\"required\":[\"command\",\"key\"],\"additionalProperties\":false},\"maxItems\":73}},\"required\":[\"keys\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `shell.drag`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_drag\",\"description\":\"Record what a seat is dragging, or that it stopped.\\n\\nRecords what one seat is dragging in the interface, so every window shows it being dragged; naming nothing says the drag ended.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_drag\",\"description\":\"Registrar o que um assento está arrastando, ou que parou de arrastar.\\n\\nRegistra o que um assento está arrastando na interface, para que todas as janelas mostrem isso sendo arrastado; sem indicar nada, diz que o arraste terminou.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `shell.focus`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_focus\",\"description\":\"Give a seat's keyboard to a part of an interface window.\\n\\nMoves the keyboard focus of one seat — the system's own unless you name another — to a part of a window: its workspace area, the drawer, the prompt line, the sidebar, the info line or the modeline. Every window of the interface follows it. A keyboard on a drawer that shows nothing open goes back to the prompt line, whichever command left the drawer so.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"window\":{\"type\":\"integer\",\"minimum\":1},\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\",\"prompt\",\"sidebar\",\"info\",\"modeline\"]}},\"required\":[\"region\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_focus\",\"description\":\"Levar o teclado de um assento para uma parte de uma janela da interface.\\n\\nMove o foco do teclado de um assento — o do próprio sistema, a menos que você indique outro — para uma parte de uma janela: a área de trabalho, a gaveta, a linha do prompt, a barra lateral, a linha de informações ou a linha de modo. Todas as janelas da interface acompanham. Um teclado numa gaveta que não mostra nada aberto volta para a linha do prompt, seja qual for o comando que a deixou assim.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"window\":{\"type\":\"integer\",\"minimum\":1},\"region\":{\"type\":\"string\",\"enum\":[\"workspace\",\"drawer\",\"prompt\",\"sidebar\",\"info\",\"modeline\"]}},\"required\":[\"region\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `shell.hover`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_hover\",\"description\":\"Record what a seat's pointer hovers, or what its keyboard selection is on.\\n\\nRecords what one seat's pointer is over, or what its keyboard selection is on, so the info line can say what it is; naming nothing clears it. The info line shows whichever of the two changed last.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"by\":{\"type\":\"string\",\"enum\":[\"pointer\",\"keyboard\"]},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"by\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_hover\",\"description\":\"Registrar o que está sob o ponteiro de um assento, ou a seleção do teclado dele.\\n\\nRegistra o que está sob o ponteiro de um assento, ou onde está a seleção do teclado dele, para que a linha de informações diga o que é; sem indicar nada, limpa o registro. A linha de informações mostra o que mudou por último entre os dois.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4},\"by\":{\"type\":\"string\",\"enum\":[\"pointer\",\"keyboard\"]},\"target\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"by\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `shell.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_read\",\"description\":\"Show all the interface holds: its windows, pages, drawer, prompt and seats.\\n\\nAnswers the whole state of the desktop interface as Noctua keeps it: each window and the pages open in it, the drawer, the sidebar, the footer, the info line, the prompt and what each seat is on, with the revision it is at. Asked from inside a command that changed the interface, it answers that change instead, which is not Noctua's until the command ends, and with no revision. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"shell_read\",\"description\":\"Mostrar tudo o que a interface guarda: janelas, páginas, gaveta e prompt.\\n\\nResponde o estado completo da interface como o Noctua o guarda: cada janela e as páginas abertas nela, a gaveta, a barra lateral, o rodapé, a linha de informações, o prompt e onde cada assento está, com a revisão em que está. Pedido de dentro de um comando que mudou a interface, responde essa mudança, que só é do Noctua quando o comando termina, e sem revisão. Não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `sidebar.cycle`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_cycle\",\"description\":\"Do what Leader Space does: show the sidebar, focus it, or hide it.\\n\\nShows the sidebar when it is hidden, puts a seat's keyboard on it when it is shown, and hides it when the keyboard is already there.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_cycle\",\"description\":\"Fazer o que Leader Espaço faz: mostrar a barra lateral, focá-la ou escondê-la.\\n\\nMostra a barra lateral quando está escondida, leva o teclado de um assento até ela quando está visível, e a esconde quando o teclado já está nela.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"seat\":{\"type\":\"integer\",\"minimum\":1,\"maximum\":4}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `sidebar.hide`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_hide\",\"description\":\"Hide the list of what is open in the interface.\\n\\nHides the sidebar; a seat whose keyboard was on it goes back to the workspace area.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_hide\",\"description\":\"Esconder a lista do que está aberto na interface.\\n\\nEsconde a barra lateral; um assento cujo teclado estava nela volta para a área de trabalho.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `sidebar.show`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_show\",\"description\":\"Show the list of what is open in the interface.\\n\\nShows the sidebar, the list of everything open in the interface.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"sidebar_show\",\"description\":\"Mostrar a lista do que está aberto na interface.\\n\\nMostra a barra lateral, a lista de tudo o que está aberto na interface.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `surfaces.activate`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_activate\",\"description\":\"Run what a button or a toggle on a surface runs, as a click would.\\n\\nDoes what clicking a button or a toggle on a surface does: runs the action the surface itself names for it, with what you typed into the fields it uses, and nothing you name yourself. Something an agent drew is never run this way, since it would run with your authority. What it runs is decided and recorded as itself, and undo treats it as that command's own step.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"node\":{\"type\":\"string\",\"maxLength\":512},\"inputs\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"value\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"name\",\"value\"],\"additionalProperties\":false},\"maxItems\":2000}},\"required\":[\"surface\",\"node\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_activate\",\"description\":\"Executar o que um botão ou chave de uma superfície executa, como um clique.\\n\\nFaz o que clicar em um botão ou chave de uma superfície faz: executa a ação que a própria superfície indica, com o que você digitou nos campos que ela usa, e nada que você mesmo indique. Algo desenhado por um agente nunca é executado assim, pois rodaria com a sua autoridade. O que ele executa é decidido e registrado como si mesmo, e desfazer o trata como um passo daquele comando.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"node\":{\"type\":\"string\",\"maxLength\":512},\"inputs\":{\"type\":\"array\",\"items\":{\"type\":\"object\",\"properties\":{\"name\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64},\"value\":{\"type\":\"string\",\"maxLength\":4096}},\"required\":[\"name\",\"value\"],\"additionalProperties\":false},\"maxItems\":2000}},\"required\":[\"surface\",\"node\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":false}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `surfaces.list`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_list\",\"description\":\"Show the surfaces plugins have open, and who drew what each shows.\\n\\nLists every surface the plugins hold open: its name, where it is shown, its title, the plugin that holds it, and who drew what it shows now — the person the plugin acted for, and the agent that acted for them when one did. It does not show what each surface holds; read one surface for that. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_list\",\"description\":\"Mostrar as superfícies abertas pelos plugins e quem desenhou cada uma.\\n\\nLista cada superfície que os plugins mantêm abertas: seu nome, onde aparece, seu título, o plugin que a mantém e quem desenhou o que ela mostra agora — a pessoa por quem o plugin agiu e o agente que agiu por ela, quando foi um agente. Não mostra o conteúdo de cada superfície; leia uma superfície para isso. Não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `surfaces.read`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_read\",\"description\":\"Show one surface a plugin has open, with what it shows and who drew it.\\n\\nShows the surface named: where it is shown, its title, the plugin that holds it, everything it shows now, and who drew that — the person the plugin acted for, and the agent that acted for them when one did. A surface no longer open is refused. It changes nothing.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"surface\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"surfaces_read\",\"description\":\"Mostrar uma superfície aberta, com o que ela mostra e quem o desenhou.\\n\\nMostra a superfície indicada: onde aparece, seu título, o plugin que a mantém, tudo o que ela mostra agora e quem desenhou isso — a pessoa por quem o plugin agiu e o agente que agiu por ela, quando foi um agente. Uma superfície que não está mais aberta é recusada. Não altera nada.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"surface\":{\"type\":\"string\",\"minLength\":1,\"maxLength\":64}},\"required\":[\"surface\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.close`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_close\",\"description\":\"Close a window of the interface, or hide it when it is the last one.\\n\\nCloses a window of the desktop interface — the one you are using, unless you name another — with the pages only it showed. The last window is hidden instead, so the interface is always ready to show again. Nothing that runs in Noctua stops.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_close\",\"description\":\"Fechar uma janela da interface, ou escondê-la quando for a última.\\n\\nFecha uma janela da interface — a que você está usando, a menos que indique outra — com as páginas que só ela mostrava. A última janela é escondida em vez de fechada, para que a interface esteja sempre pronta para aparecer. Nada do que roda no Noctua para.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.hide`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_hide\",\"description\":\"Hide a window of the interface, keeping everything in it.\\n\\nHides a window of the desktop interface — the one you are using, unless you name another. Nothing stops: what was open in it is kept, and showing it again brings it all back.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_hide\",\"description\":\"Esconder uma janela da interface, mantendo tudo o que há nela.\\n\\nEsconde uma janela da interface — a que você está usando, a menos que indique outra. Nada para: o que estava aberto nela é mantido, e mostrá-la de novo traz tudo de volta.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.move`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_move\",\"description\":\"Place a window of the interface at a position on the screen.\\n\\nMoves a window of the desktop interface — the one you are using, unless you name another — so its top left corner is at the position given, in the screen's own units. Under a tiling window manager, the manager may place it elsewhere.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"x\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767},\"y\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767}},\"required\":[\"x\",\"y\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_move\",\"description\":\"Colocar uma janela da interface numa posição da tela.\\n\\nMove uma janela da interface — a que você está usando, a menos que indique outra — para que o canto superior esquerdo fique na posição indicada, nas unidades da própria tela. Num gerenciador de janelas em mosaico, ele pode colocá-la em outro lugar.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"x\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767},\"y\":{\"type\":\"integer\",\"minimum\":-32768,\"maximum\":32767}},\"required\":[\"x\",\"y\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.open`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_open\",\"description\":\"Open one more window of the interface, showing the same state.\\n\\nOpens another window of the desktop interface, shown at once. Every window shows the same drawer, prompt and footer, and each keeps a stack of pages of its own. The interface holds only so many windows.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_open\",\"description\":\"Abrir mais uma janela da interface, mostrando o mesmo estado.\\n\\nAbre mais uma janela da interface, já visível. Todas as janelas mostram a mesma gaveta, o mesmo prompt e o mesmo rodapé, e cada uma tem a sua própria pilha de páginas. A interface comporta um número limitado de janelas.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.resize`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_resize\",\"description\":\"Give a window of the interface a new width and height.\\n\\nResizes a window of the desktop interface — the one you are using, unless you name another — in CSS pixels. A window is never made smaller than the smallest size every page is built for.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"width\":{\"type\":\"integer\",\"minimum\":320,\"maximum\":65535},\"height\":{\"type\":\"integer\",\"minimum\":240,\"maximum\":65535}},\"required\":[\"width\",\"height\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_resize\",\"description\":\"Dar a uma janela da interface uma nova largura e uma nova altura.\\n\\nRedimensiona uma janela da interface — a que você está usando, a menos que indique outra — em pixels CSS. Uma janela nunca fica menor que o menor tamanho para o qual todas as páginas são feitas.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1},\"width\":{\"type\":\"integer\",\"minimum\":320,\"maximum\":65535},\"height\":{\"type\":\"integer\",\"minimum\":240,\"maximum\":65535}},\"required\":[\"width\",\"height\"],\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "## `window.show`"),
    ("mcp-tools.md", "In `en`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_show\",\"description\":\"Show a window of the interface that is hidden.\\n\\nShows a window of the desktop interface — the one you are using, unless you name another — exactly as it was when it was hidden.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
    ("mcp-tools.md", "In `pt-BR`:"),
    ("mcp-tools.md", "```json"),
    (
        "mcp-tools.md",
        "{\"name\":\"window_show\",\"description\":\"Mostrar uma janela da interface que está escondida.\\n\\nMostra uma janela da interface — a que você está usando, a menos que indique outra — exatamente como estava quando foi escondida.\",\"inputSchema\":{\"type\":\"object\",\"properties\":{\"window\":{\"type\":\"integer\",\"minimum\":1}},\"additionalProperties\":false},\"annotations\":{\"readOnlyHint\":true}}",
    ),
    ("mcp-tools.md", "```"),
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

/// Every table heading of every page, with how many tables carry it.
///
/// Compared as a whole by the test below. Checked
/// with `any` over a list that misses a table, the rate-limit one — ten rows
/// by eight columns, the largest on these pages — is named nowhere. Measured,
/// with such a list, four of its labels exchanged in
/// the emitter and the pages regenerated: `17 passed`, `check passed`, and
/// every verdict line byte-identical to the green run, while the page told a
/// reader that `device_pin`'s lockout threshold was its consequence and its
/// scope was its budget. `any` cannot see a table that was never written
/// down, so the count is held here too and the comparison is an equality: a
/// table that arrives unpinned fails, and so does one that leaves.
const HEADINGS: [(&str, usize, &str); 30] = [
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
    ("limits.md", 2, "| Limit | Value | What it bounds |"),
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
    ("log-events.md", 33, "| Field | Kind | Optional |"),
    ("notifications.md", 1, "| | Key | en | pt-BR |"),
    ("emails.md", 1, "| | Key | en | pt-BR |"),
    ("commands.md", 68, "| | |"),
    ("commands.md", 68, "| | en | pt-BR |"),
    ("categories.md", 3, "| | |"),
    ("categories.md", 3, "| | en | pt-BR |"),
    (
        "categories.md",
        1,
        "| Category | Standard name | Display name (en) | Line description (en) | Display name (pt-BR) | Line description (pt-BR) |",
    ),
    (
        "categories.md",
        1,
        "| Category | Standard name | Role | Display name (en) | Line description (en) | Display name (pt-BR) | Line description (pt-BR) |",
    ),
    (
        "categories.md",
        1,
        "| Category | Display name (en) | Line description (en) | Display name (pt-BR) | Line description (pt-BR) |",
    ),
    ("external-identifiers.md", 2, "| | |"),
    ("external-identifiers.md", 2, "| | en | pt-BR |"),
    ("external-identifiers.md", 2, "| System | Field | Values |"),
    ("cli.md", 1, "| Verb | What it does |"),
    ("cli.md", 1, "| Code | What a run ending with it means |"),
    ("keybindings.md", 1, "| Form | Pressed as |"),
    ("keybindings.md", 1, "| Key | Linux | Windows | macOS |"),
    ("keybindings.md", 1, "| Channel | Linux | Windows | macOS |"),
    (
        "keybindings.md",
        1,
        "| Binding | Command | Display name (en) | Display name (pt-BR) |",
    ),
    (
        "keybindings.md",
        1,
        "| Channel | Chord modifier | Linux | Windows | macOS |",
    ),
    (
        "keybindings.md",
        1,
        "| Keyboard | System | Chord | Source |",
    ),
];

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
            TAKEN_BACK_WHOLE.contains(&entry.name),
            AS_WHAT_IT_RUNS.contains(&entry.name),
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
    // **And the commands undo stops at, by name, on the page.** The kernel
    // makes a step that pruned a table or changed a person's agents a barrier
    // (`leaves_the_machine` in `crates/nc-kernel/src/history.rs`), and a row
    // saying "yes" there is a sentence in the set, answered for the wrong
    // command.
    for name in [
        "limits.prune",
        "agents.create",
        "agents.grant",
        "agents.revoke",
    ] {
        let stopped_at = the_undo_answer(section(&page, name), name);
        assert_eq!(
            stopped_at, REFUSALS[1],
            "`{name}` is a step undo stops at, and the page answers `{stopped_at}`"
        );
    }
    // **And the switch undo takes back, by name**: the kernel leaves
    // `settings.mcp.set` out of what leaves the machine, so the person's undo
    // puts the value back, and a row saying undo stops at it tells a reader
    // the opposite of what pressing undo does.
    let switched = the_undo_answer(section(&page, "settings.mcp.set"), "settings.mcp.set");
    assert_eq!(
        switched, REFUSALS[3],
        "`settings.mcp.set` is a step undo takes back, and the page answers `{switched}`"
    );
    // **And the activation, by name**: no step of its own, so a row saying
    // undo stops at it tells a reader a click cannot be taken back.
    let activated = the_undo_answer(section(&page, "surfaces.activate"), "surfaces.activate");
    assert_eq!(
        activated, REFUSALS[4],
        "`surfaces.activate` is no step of its own, and the page answers `{activated}`"
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
        Unit::Minutes => format!("{value} minutes"),
        Unit::CssPixels => format!("{value} CSS pixels"),
        Unit::Percent => format!("{value} percent"),
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
fn every_daemon_limit_is_published_in_the_unit_it_is_counted_in() {
    let catalog = catalog();
    let table = rows(&page("limits.md"));
    let mut counted = Counted::default();
    for limit in catalog.limits.daemon.each() {
        let row = row(&table, &format!("`{}`", limit.name));
        counted.same(&row[1], &kernel_value(limit.value, limit.unit), limit.name);
        counted.same(&row[2], &cell_text(limit.bounds), limit.name);
    }
    assert_eq!(
        kernel_value(catalog.limits.daemon.mark_lease_ms, Unit::Milliseconds),
        format!("{} milliseconds", catalog.limits.daemon.mark_lease_ms)
    );
    counted.floor(catalog.limits.daemon.each().len() * 2, "limits.md's daemon");
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
                    TAKEN_BACK_WHOLE.contains(&entry.name.as_str()),
                    AS_WHAT_IT_RUNS.contains(&entry.name.as_str()),
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
        // **The keybindings row exists only when there is a keybinding**, which
        // is why it is not in the list above: otherwise `Keybindings | — |`
        // sits on every row of the shipped page, because nothing declares one.
        if entry.keybindings.is_empty() {
            assert!(
                !mine.iter().any(|row| row[0] == "Keybindings"),
                "`{}` declares no keybinding and the page publishes a row for one",
                entry.name
            );
        } else {
            let written: Vec<String> = entry
                .keybindings
                .iter()
                .map(|binding| format!("`{binding}`"))
                .collect();
            assert!(
                has("Keybindings", &written.join(", ")),
                "`{}` declares {:?} and the page does not publish them",
                entry.name,
                entry.keybindings
            );
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

/// Every command publishes what a person is shown of it, in each locale under
/// that locale's column: its display name, line, paragraph and tags, and its
/// glyph, categories and wiki text where it has them.
///
/// **The tags in the order written, joined as the page joins them**: a list
/// published in another order, or one locale's under the other's heading, is a
/// list a translator cannot check against the file.
#[test]
fn every_command_publishes_its_metadata_in_both_locales() {
    let catalog = catalog();
    let text = page("commands.md");
    let mut counted = Counted::default();
    for entry in &catalog.commands {
        let mine = rows(section(&text, &entry.name));
        let header = mine
            .iter()
            .find(|row| row.first().is_some_and(String::is_empty) && row.len() > 2)
            .unwrap_or_else(|| panic!("`{}` has no table of its texts", entry.name));
        for locale in &catalog.locales {
            let tag = locale.meta.bcp47.as_str();
            let column = header
                .iter()
                .position(|cell| cell == tag)
                .unwrap_or_else(|| panic!("`{}`'s texts have no `{tag}` column", entry.name));
            for (label, key) in [
                ("Display name", &entry.title),
                ("Line description", &entry.description),
                ("Paragraph", &entry.paragraph),
            ] {
                let declared = locale
                    .message
                    .get(key)
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"));
                counted.same(
                    &row(&mine, label)[column],
                    &cell_text(declared),
                    &format!("`{}`'s {label} under `{tag}`", entry.name),
                );
            }
            let words = locale
                .tags
                .get(&entry.tags)
                .unwrap_or_else(|| panic!("`{}` has no `{tag}` tags", entry.name));
            let joined: Vec<String> = words.iter().map(|word| cell_text(word)).collect();
            counted.same(
                &row(&mine, "Tags")[column],
                &joined.join(", "),
                &format!("`{}`'s tags under `{tag}`", entry.name),
            );
        }
        let held = Held {
            registry: "commands",
            name: &entry.name,
            categories: &entry.categories,
            glyph: entry.glyph.as_deref(),
        };
        the_rows_only_an_entry_holding_them_has(&catalog, &held, &mine, &mut counted);
    }
    counted.floor(
        catalog.commands.len() * catalog.locales.len() * 4,
        "commands.md's texts",
    );
}

/// The part of a page under one of its own headings, which carry no
/// backticks: from the heading to the next one.
///
/// [`section`] is for a registry's entries; this is for the parts a page
/// writes in its own words, such as `## Verbs` and `## Leaders`.
fn part<'a>(page: &'a str, heading: &str) -> &'a str {
    let opening = format!("## {heading}\n");
    let at = page
        .find(&opening)
        .unwrap_or_else(|| panic!("no part headed `{heading}`"));
    let rest = &page[at + opening.len()..];
    rest.find("\n## ").map_or(rest, |end| &rest[..end])
}

/// The fenced block that follows `opening` in `text`, without its fences.
///
/// **The fence is read, not assumed**: a block is fenced with more backticks
/// than any run inside it, so a block holding three is fenced with four.
fn block_after<'a>(text: &'a str, opening: &str) -> &'a str {
    let at = text
        .find(opening)
        .unwrap_or_else(|| panic!("nothing follows `{opening}`"));
    let rest = text[at + opening.len()..].trim_start_matches('\n');
    let fence: String = rest
        .chars()
        .take_while(|character| *character == '`')
        .collect();
    assert!(
        fence.len() >= 3,
        "`{opening}` is not followed by a fenced block"
    );
    let inside = &rest[rest.find('\n').expect("the fence's line ends") + 1..];
    let end = inside
        .find(&format!("\n{fence}\n"))
        .unwrap_or_else(|| panic!("the block after `{opening}` is not closed"));
    &inside[..=end]
}

/// The CLI's page names every verb, every exit code and every command.
///
/// Each verb with what it does, each exit code with what a run ending with
/// it means, and every command the catalog declares, each once.
#[test]
fn the_cli_page_names_every_verb_every_exit_code_and_every_command() {
    let catalog = catalog();
    let text = page("cli.md");
    let mut counted = Counted::default();
    let verbs = rows(part(&text, "Verbs"));
    for verb in nc_cli::args::VERBS {
        counted.same(
            &row(&verbs, &format!("`{verb}`"))[1],
            &cell_text(nc_cli::help::what_it_does(verb)),
            &format!("what the verb `{verb}` does"),
        );
    }
    let codes = rows(part(&text, "Exit codes"));
    for exit in nc_cli::exit::Exit::ALL {
        counted.same(
            &row(&codes, &exit.code().to_string())[1],
            &cell_text(exit.meaning()),
            &format!("what exit code {} means", exit.code()),
        );
    }
    // A row the CLI does not have is a row the page made up.
    assert_eq!(
        verbs.len() + codes.len(),
        2 + nc_cli::args::VERBS.len() + nc_cli::exit::Exit::ALL.len(),
        "the page lists a verb or an exit code the CLI does not have"
    );
    counted.floor(
        nc_cli::args::VERBS.len() + nc_cli::exit::Exit::ALL.len(),
        "cli.md's verbs and exit codes",
    );
    let sections: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("## `"))
        .map(|heading| heading.trim_end_matches('`'))
        .collect();
    let mut declared: Vec<&str> = catalog
        .commands
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    declared.sort_unstable();
    assert!(!declared.is_empty(), "the catalog declares no command");
    assert_eq!(
        sections, declared,
        "docs/cli.md does not hold every command the catalog declares, once and in order"
    );
}

/// The CLI's page says of each command what help says of it, in each locale.
///
/// The block under it is what `noctua help <command>` prints when the daemon
/// sends the catalog's texts in that locale, through the function help prints
/// with; and the grammar is what help prints of it.
#[test]
fn the_cli_page_says_what_help_says() {
    let catalog = catalog();
    let text = page("cli.md");
    assert_eq!(
        block_after(part(&text, "Usage"), ""),
        nc_cli::help::usage("noctua"),
        "docs/cli.md's grammar is not what help prints"
    );
    let mut compared = 0_usize;
    for entry in &catalog.commands {
        let schema = nc_schema::Schema::compile(&entry.args_schema)
            .unwrap_or_else(|refused| panic!("`{}`'s schema: {refused}", entry.name));
        let mine = section(&text, &entry.name);
        for locale in &catalog.locales {
            let tag = locale.meta.bcp47.as_str();
            let said = |key: &String| {
                locale
                    .message
                    .get(key)
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"))
                    .clone()
            };
            let sent = nc_cli::client::Listed {
                name: entry.name.clone(),
                display_name: said(&entry.title),
                line: said(&entry.description),
                paragraph: said(&entry.paragraph),
                tags: locale
                    .tags
                    .get(&entry.tags)
                    .unwrap_or_else(|| panic!("`{}` has no `{tag}` tags", entry.name))
                    .clone(),
                categories: entry.categories.clone(),
                args_schema: entry.args_schema.clone(),
            };
            assert_eq!(
                block_after(mine, &format!("In `{tag}`:")),
                nc_cli::help::command("noctua", &sent, &schema),
                "docs/cli.md does not say of `{}` in `{tag}` what help says",
                entry.name
            );
            compared += 1;
        }
    }
    assert!(
        compared >= catalog.commands.len() * 2 && compared > 0,
        "only {compared} block(s) were compared"
    );
}

/// The MCP page projects every command as a tool, in every locale, and says
/// what a session sees.
///
/// A tool is the command's name with each dot an underscore, which reads back
/// to that command, its line and paragraph with a blank line between, its
/// argument schema as written, and `readOnlyHint` exactly when it changes
/// nothing — in that order, and nothing else; and the page's lead, held whole,
/// says how a name reads back, which commands a session lists, that a
/// plugin's tools come from the daemon, and what serves them.
#[test]
fn the_mcp_page_projects_every_command_as_a_tool_and_says_what_a_session_sees() {
    let catalog = catalog();
    let text = page("mcp-tools.md");
    for said in [
        "Every command the catalog declares, as the tool definition an MCP session is handed \
         (`NOCTUA.md §20.3`): the tool's name is the command's with each dot written as an \
         underscore, because model APIs refuse a dot in a tool's name, and a call naming it is \
         read back to the command through the one parser of a command's name; its description \
         is the command's line and paragraph; its input schema is the command's argument \
         schema, as the kernel holds a dispatch to it; and its one annotation, `readOnlyHint`, \
         is true exactly when the command is not `mutating` — a read, which makes no step. \
         Each is written once for each locale, by the one function the MCP server lists its \
         tools with.\n\n",
        "\n\n**A session sees only the tools it may dispatch**: a session lists those of these \
         whose resource is known without their arguments, that are not irreversible, and that \
         it may dispatch now, decided when it asks, for the principal it is, as each of its \
         calls is decided (`§20.3`, `§14.6`). **A plugin's tools come from the daemon**: a \
         plugin declares its commands at `activate`, so no page lists them, and the daemon's \
         registry holds them while the plugin is loaded (`§15.3`). **`noctua mcp <agent>` \
         serves them**: to an MCP client on its standard input and output, as the agent it \
         names, in the language its environment names, once MCP is switched on in its \
         channel (`§20.3`).\n\n",
    ] {
        assert!(
            text.contains(said),
            "docs/mcp-tools.md does not say `{said}`"
        );
    }
    let mut counted = Counted::default();
    for entry in &catalog.commands {
        let schema: serde_json::Value = serde_json::from_str(&entry.args_schema)
            .unwrap_or_else(|refused| panic!("`{}`'s schema: {refused}", entry.name));
        let mine = section(&text, &entry.name);
        for locale in &catalog.locales {
            let tag = locale.meta.bcp47.as_str();
            let said = |key: &String| {
                locale
                    .message
                    .get(key)
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"))
                    .clone()
            };
            let written = block_after(mine, &format!("In `{tag}`:"));
            let tool: serde_json::Value = serde_json::from_str(written)
                .unwrap_or_else(|refused| panic!("`{}` in `{tag}`: {refused}", entry.name));
            let fields: Vec<&str> = tool
                .as_object()
                .unwrap_or_else(|| panic!("`{}`'s tool is not an object", entry.name))
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(
                fields,
                ["name", "description", "inputSchema", "annotations"],
                "`{}`'s tool in `{tag}`",
                entry.name
            );
            let name = tool["name"].as_str().unwrap_or_default();
            counted.same(
                name,
                &nc_schema::entry::tool_name(&entry.name),
                &format!("`{}`'s tool name in `{tag}`", entry.name),
            );
            counted.same(
                &nc_schema::entry::command_of_tool(name).unwrap_or_default(),
                &entry.name,
                &format!("`{}`'s tool name read back in `{tag}`", entry.name),
            );
            counted.same(
                &tool["annotations"].to_string(),
                &serde_json::json!({ "readOnlyHint": !entry.mutating }).to_string(),
                &format!("`{}`'s annotations in `{tag}`", entry.name),
            );
            counted.same(
                tool["description"].as_str().unwrap_or_default(),
                &format!("{}\n\n{}", said(&entry.description), said(&entry.paragraph)),
                &format!("`{}`'s tool description in `{tag}`", entry.name),
            );
            counted.same(
                &tool["inputSchema"].to_string(),
                &schema.to_string(),
                &format!("`{}`'s input schema in `{tag}`", entry.name),
            );
        }
    }
    counted.floor(
        catalog.commands.len() * catalog.locales.len() * 5,
        "mcp-tools.md's tools",
    );
    // **Both answers of the hint are published**, so a page that wrote one of
    // them for every command is refused even if the catalog's flags agreed.
    for read_only in [true, false] {
        let hint = format!("\"annotations\":{{\"readOnlyHint\":{read_only}}}");
        assert!(
            text.contains(&hint),
            "no tool on docs/mcp-tools.md is marked `{hint}`"
        );
    }
}

/// The keybindings page holds each channel's Leader on each keyboard, and
/// each Leader held with its channel's chord modifier.
///
/// Against `nc_channel`, which holds them, with the columns' keyboards
/// written out here.
#[test]
fn the_keybindings_page_holds_each_channels_leader_on_each_keyboard() {
    use nc_schema::keys::Keyboard;

    let text = page("keybindings.md");
    let leaders = rows(part(&text, "Leaders"));
    let mut counted = Counted::default();
    for channel in channels() {
        let named = format!("`{}`", channel.word());
        let mine: Vec<&Vec<String>> = leaders.iter().filter(|row| row[0] == named).collect();
        let [alone, held] = mine.as_slice() else {
            panic!("{named} is not on one row of each table: {mine:?}");
        };
        let chord = channel.chord_modifier();
        counted.same(
            &held[1],
            &format!("`{}`", chord.word()),
            &format!("{named}'s chord modifier"),
        );
        for (column, keyboard) in [
            (1, Keyboard::Linux),
            (2, Keyboard::Windows),
            (3, Keyboard::Mac),
        ] {
            counted.same(
                &alone[column],
                &cell_text(&channel.leader().written_for(keyboard)),
                &format!("{named}'s Leader on {keyboard:?}"),
            );
            counted.same(
                &held[column + 1],
                &cell_text(&channel.leader().with(chord).written_for(keyboard)),
                &format!("{named}'s Leader with its chord modifier on {keyboard:?}"),
            );
        }
    }
    assert_eq!(
        leaders.len(),
        2 + 2 * channels().len(),
        "the page lists a Leader no channel has"
    );
    counted.floor(channels().len() * 7, "keybindings.md's Leaders");
}

/// The keybindings page holds every key a binding may name, as each keyboard
/// labels it, and every chord a system keeps, with the source that says so.
#[test]
fn the_keybindings_page_holds_every_key_and_every_chord_a_system_keeps() {
    use nc_schema::keys::{Key, Keyboard, SYSTEM_SHORTCUTS};

    let text = page("keybindings.md");
    let keys = rows(part(&text, "Keys"));
    let mut counted = Counted::default();
    for key in Key::ALL {
        let mine = row(&keys, &format!("`{}`", key.code()));
        for (column, keyboard) in [
            (1, Keyboard::Linux),
            (2, Keyboard::Windows),
            (3, Keyboard::Mac),
        ] {
            counted.same(
                &mine[column],
                &cell_text(key.written_for(keyboard)),
                &format!("`{}` on {keyboard:?}", key.code()),
            );
        }
    }
    assert_eq!(
        keys.len(),
        1 + Key::ALL.len(),
        "a key the vocabulary lacks is listed"
    );
    let kept: Vec<Vec<String>> = rows(part(&text, "System shortcuts"))
        .into_iter()
        .skip(1)
        .collect();
    let named = |keyboard: Keyboard| match keyboard {
        Keyboard::Linux => "Linux",
        Keyboard::Windows => "Windows",
        Keyboard::Mac => "macOS",
    };
    let expected: Vec<Vec<String>> = SYSTEM_SHORTCUTS
        .iter()
        .map(|shortcut| {
            vec![
                named(shortcut.keyboard).to_owned(),
                cell_text(shortcut.system),
                cell_text(&shortcut.chord.written_for(shortcut.keyboard)),
                cell_text(shortcut.source),
            ]
        })
        .collect();
    assert!(!expected.is_empty(), "no system keeps a chord");
    assert_eq!(
        kept, expected,
        "docs/keybindings.md does not list every chord a system keeps, in order, with its \
         source"
    );
    counted.floor(Key::ALL.len() * 3, "keybindings.md's keys");
}

/// Every category is on the categories page under its set.
///
/// With what its standard calls it and ranks it, and its texts under each
/// locale's columns; and every set says what defines it, in its own words in
/// each locale.
#[test]
fn every_category_is_on_the_categories_page_under_its_set() {
    let catalog = catalog();
    let text = page("categories.md");
    let mut counted = Counted::default();
    for set in &catalog.category_sets {
        let mine = rows(section(&text, &set.name));
        counted.same(
            &row(&mine, "Standard")[1],
            if set.standard {
                "yes — a standard outside the platform"
            } else {
                "no — the platform's own"
            },
            &format!("`{}`'s standing", set.name),
        );
        counted.same(
            &row(&mine, "Defined by")[1],
            &cell_text(set.defined_by.trim()),
            &format!("what defines `{}`", set.name),
        );
        match set.version.as_deref() {
            Some(version) => counted.same(
                &row(&mine, "Version")[1],
                &cell_text(version),
                &format!("`{}`'s version", set.name),
            ),
            None => assert!(
                !mine.iter().any(|row| row[0] == "Version"),
                "`{}` names no version and the page publishes a row for one",
                set.name
            ),
        }
        let texts = mine
            .iter()
            .find(|row| row.first().is_some_and(String::is_empty) && row.len() > 2)
            .unwrap_or_else(|| panic!("`{}` has no table of its texts", set.name));
        let listing = mine
            .iter()
            .find(|row| row.first().is_some_and(|cell| cell == "Category"))
            .unwrap_or_else(|| panic!("`{}` has no table of its categories", set.name));
        let column = |header: &[String], label: &str| {
            header
                .iter()
                .position(|cell| cell == label)
                .unwrap_or_else(|| panic!("`{}` has no `{label}` column", set.name))
        };
        for locale in &catalog.locales {
            let tag = locale.meta.bcp47.as_str();
            let said = |key: &str| {
                cell_text(
                    locale
                        .message
                        .get(key)
                        .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message")),
                )
            };
            counted.same(
                &row(&mine, "Display name")[column(texts, tag)],
                &said(&set.title),
                &format!("`{}`'s display name under `{tag}`", set.name),
            );
            counted.same(
                &row(&mine, "Line description")[column(texts, tag)],
                &said(&set.description),
                &format!("`{}`'s line description under `{tag}`", set.name),
            );
            for category in catalog
                .categories
                .iter()
                .filter(|category| category.set == set.name)
            {
                let published = row(&mine, &format!("`{}`", category.name));
                counted.same(
                    &published[column(listing, &format!("Display name ({tag})"))],
                    &said(&category.title),
                    &format!("`{}`'s display name under `{tag}`", category.name),
                );
                counted.same(
                    &published[column(listing, &format!("Line description ({tag})"))],
                    &said(&category.description),
                    &format!("`{}`'s line description under `{tag}`", category.name),
                );
            }
        }
        what_a_standard_says_is_on_the_page(&catalog, &set.name, &mine, listing, &mut counted);
    }
    counted.floor(
        catalog.categories.len() * catalog.locales.len() * 2,
        "categories.md's texts",
    );
}

/// Each category of a set, on its row of the page, with what its standard
/// calls it and how it ranks it, where it has them.
///
/// Split out of `every_category_is_on_the_categories_page_under_its_set` for
/// its line budget.
fn what_a_standard_says_is_on_the_page(
    catalog: &Catalog,
    set: &str,
    mine: &[Vec<String>],
    listing: &[String],
    counted: &mut Counted,
) {
    let column = |label: &str| {
        listing
            .iter()
            .position(|cell| cell == label)
            .unwrap_or_else(|| panic!("`{set}` has no `{label}` column"))
    };
    for category in catalog
        .categories
        .iter()
        .filter(|category| category.set == set)
    {
        let published = row(mine, &format!("`{}`", category.name));
        if let Some(named) = category.standard_name.as_deref() {
            counted.same(
                &published[column("Standard name")],
                &format!("`{}`", cell_text(named)),
                &format!("`{}`'s standard name", category.name),
            );
        }
        // The role as the registry writes it, spelled out here.
        if let Some(role) = category.role {
            counted.same(
                &published[column("Role")],
                match role {
                    nc_catalog_compiler::model::Role::Main => "main",
                    nc_catalog_compiler::model::Role::Additional => "additional",
                    nc_catalog_compiler::model::Role::Reserved => "reserved",
                },
                &format!("`{}`'s role", category.name),
            );
        }
    }
}

/// Every external identifier is on its page with each system's
/// identification.
///
/// Its texts under each locale's column, its categories, and one row for each
/// field of each system it is identified on, every value as its system
/// writes it — and no row of identification the registry does not declare,
/// in the order the registry writes them.
#[test]
fn every_external_identifier_is_on_its_page_with_each_systems_identification() {
    let catalog = catalog();
    let text = page("external-identifiers.md");
    let mut counted = Counted::default();
    for entry in &catalog.external_identifiers {
        let mine = rows(section(&text, &entry.name));
        let header = mine
            .iter()
            .find(|row| row.first().is_some_and(String::is_empty) && row.len() > 2)
            .unwrap_or_else(|| panic!("`{}` has no table of its texts", entry.name));
        for locale in &catalog.locales {
            let tag = locale.meta.bcp47.as_str();
            let column = header
                .iter()
                .position(|cell| cell == tag)
                .unwrap_or_else(|| panic!("`{}`'s texts have no `{tag}` column", entry.name));
            for (label, key) in [
                ("Display name", &entry.title),
                ("Line description", &entry.description),
                ("Paragraph", &entry.paragraph),
            ] {
                let declared = locale
                    .message
                    .get(key)
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"));
                counted.same(
                    &row(&mine, label)[column],
                    &cell_text(declared),
                    &format!("`{}`'s {label} under `{tag}`", entry.name),
                );
            }
            let words = locale
                .tags
                .get(&entry.tags)
                .unwrap_or_else(|| panic!("`{}` has no `{tag}` tags", entry.name));
            let joined: Vec<String> = words.iter().map(|word| cell_text(word)).collect();
            counted.same(
                &row(&mine, "Tags")[column],
                &joined.join(", "),
                &format!("`{}`'s tags under `{tag}`", entry.name),
            );
        }
        let held = Held {
            registry: "external-identifiers",
            name: &entry.name,
            categories: &entry.categories,
            glyph: entry.glyph.as_deref(),
        };
        the_rows_only_an_entry_holding_them_has(&catalog, &held, &mine, &mut counted);
        let published: Vec<&Vec<String>> = mine
            .iter()
            .filter(|row| ["Linux", "macOS", "Windows"].contains(&row[0].as_str()))
            .collect();
        let declared = identification_rows(entry);
        assert_eq!(
            published.len(),
            declared.len(),
            "`{}` publishes {} row(s) of identification and declares {}",
            entry.name,
            published.len(),
            declared.len()
        );
        for (published, declared) in published.iter().zip(&declared) {
            counted.same(
                &published.join(" | "),
                &declared.join(" | "),
                &format!("`{}`'s identification", entry.name),
            );
        }
    }
    counted.floor(
        catalog.external_identifiers.len() * (catalog.locales.len() * 4 + 4),
        "external-identifiers.md's cells",
    );
}

/// The rows of identification a page publishes for an entry, as the registry
/// declares them: the system, the field and every value written as code, for
/// each field with a value, in the registry's order.
///
/// Split out of
/// `every_external_identifier_is_on_its_page_with_each_systems_identification`
/// for its line budget.
fn identification_rows(
    entry: &nc_catalog_compiler::model::ExternalIdentifierEntry,
) -> Vec<[String; 3]> {
    let mut declared: Vec<[String; 3]> = Vec::new();
    let mut each = |system: &str, fields: &[(&str, &[String])]| {
        for (field, values) in fields {
            if values.is_empty() {
                continue;
            }
            let written: Vec<String> = values.iter().map(|value| format!("`{value}`")).collect();
            declared.push([system.to_owned(), format!("`{field}`"), written.join(", ")]);
        }
    };
    if let Some(on) = &entry.linux {
        each(
            "Linux",
            &[
                ("executables", &on.executables),
                ("desktop_ids", &on.desktop_ids),
                ("flatpak_ids", &on.flatpak_ids),
                ("snap_names", &on.snap_names),
                ("paths", &on.paths),
            ],
        );
    }
    if let Some(on) = &entry.macos {
        each(
            "macOS",
            &[
                ("bundle_ids", &on.bundle_ids),
                ("app_paths", &on.app_paths),
                ("executables", &on.executables),
            ],
        );
    }
    if let Some(on) = &entry.windows {
        each(
            "Windows",
            &[
                ("executables", &on.executables),
                ("app_paths_keys", &on.app_paths_keys),
                ("paths", &on.paths),
            ],
        );
    }
    declared
}

/// What an entry may carry beyond its texts, and the directory its registry's
/// wiki texts sit under.
struct Held<'a> {
    registry: &'a str,
    name: &'a str,
    categories: &'a [String],
    glyph: Option<&'a str>,
}

/// The rows an entry's table has only when the entry holds what they say: its
/// categories, its glyph and its wiki text.
///
/// **Both ways, for every registry whose entries carry them**: what the entry
/// holds is published as the registry wrote it, and what it does not hold has
/// no row at all, so a page that dropped the row and one that wrote it for
/// everyone are each a failure here.
fn the_rows_only_an_entry_holding_them_has(
    catalog: &Catalog,
    entry: &Held<'_>,
    mine: &[Vec<String>],
    counted: &mut Counted,
) {
    match entry.glyph {
        Some(glyph) => counted.same(
            &row(mine, "Glyph")[1],
            &format!("`{glyph}`"),
            &format!("`{}`'s glyph", entry.name),
        ),
        None => assert!(
            !mine.iter().any(|row| row[0] == "Glyph"),
            "`{}` declares no glyph and the page publishes a row for one",
            entry.name
        ),
    }
    if entry.categories.is_empty() {
        assert!(
            !mine.iter().any(|row| row[0] == "Categories"),
            "`{}` names no category and the page publishes a row for one",
            entry.name
        );
    } else {
        let named: Vec<String> = entry
            .categories
            .iter()
            .map(|name| format!("`{name}`"))
            .collect();
        counted.same(
            &row(mine, "Categories")[1],
            &named.join(", "),
            &format!("`{}`'s categories", entry.name),
        );
    }
    the_wiki_row_links_every_locale(catalog, entry, mine, counted);
}

/// An entry's wiki row, when it has a wiki text, links each locale's file
/// under its registry and says its format; with none, there is no row.
fn the_wiki_row_links_every_locale(
    catalog: &Catalog,
    entry: &Held<'_>,
    mine: &[Vec<String>],
    counted: &mut Counted,
) {
    let Some(wiki) = catalog
        .wiki
        .iter()
        .find(|wiki| wiki.registry == entry.registry && wiki.name == entry.name)
    else {
        assert!(
            !mine.iter().any(|row| row[0] == "Wiki"),
            "`{}` has no wiki text and the page publishes a row for one",
            entry.name
        );
        return;
    };
    let published = &row(mine, "Wiki")[1];
    let extension = wiki.format.extension();
    for locale in &catalog.locales {
        let tag = &locale.meta.bcp47;
        let path = format!(
            "catalog/wiki/{tag}/{}/{}.{extension}",
            entry.registry, entry.name
        );
        let link = format!("[{tag}](../{path})");
        assert!(
            published.contains(&link),
            "`{}`'s wiki row does not link its `{tag}` text: {published}",
            entry.name
        );
        assert!(
            root().join(&path).is_file(),
            "`{}`'s wiki link names a file that is not there",
            entry.name
        );
        counted.0 += 1;
    }
    assert!(
        published.contains(&format!("(`.{extension}`)")),
        "`{}`'s wiki row does not say its format: {published}",
        entry.name
    );
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

    // And `commands.md`, whose sentences sit in a table of their own under
    // each command, one column per locale.
    let commands = page("commands.md");
    for entry in &catalog.commands {
        let mine = rows(section(&commands, &entry.name));
        let header = mine
            .iter()
            .find(|row| row.first().is_some_and(String::is_empty) && row.len() > 2)
            .unwrap_or_else(|| panic!("`{}` has no table of its texts", entry.name));
        for (label, key) in [
            ("Display name", &entry.title),
            ("Line description", &entry.description),
            ("Paragraph", &entry.paragraph),
        ] {
            let published = row(&mine, label);
            for tag in &locales {
                let declared = catalog
                    .locales
                    .iter()
                    .find(|locale| locale.meta.bcp47 == *tag)
                    .and_then(|locale| locale.message.get(key))
                    .unwrap_or_else(|| panic!("`{key}` has no `{tag}` message"));
                assert_eq!(
                    &published[column_of(header, tag)],
                    &cell_text(declared),
                    "`{}`'s {label} under `{tag}` is not what the registry declares",
                    entry.name
                );
                compared += 1;
            }
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
            "commands.md" | "cli.md" | "mcp-tools.md" => {
                catalog.commands.iter().map(|e| e.name.clone()).collect()
            }
            "log-events.md" => catalog.events.iter().map(|e| e.name.clone()).collect(),
            "notifications.md" => catalog.notifications.iter().map(|e| e.id.clone()).collect(),
            "emails.md" => catalog.emails.iter().map(|e| e.id.clone()).collect(),
            "categories.md" => catalog
                .category_sets
                .iter()
                .map(|e| e.name.clone())
                .collect(),
            "external-identifiers.md" => catalog
                .external_identifiers
                .iter()
                .map(|e| e.name.clone())
                .collect(),
            other => panic!("no registry for {other}"),
        }
    };
    let mut checked = 0_usize;
    for name in [
        "commands.md",
        "cli.md",
        "mcp-tools.md",
        "log-events.md",
        "notifications.md",
        "emails.md",
        "categories.md",
        "external-identifiers.md",
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
    checked += every_row_of_the_categories_page_is_declared(&catalog);

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

/// Every category row `docs/categories.md` publishes is a category the
/// registry declares, under the set it is in.
///
/// Split out for the caller's line budget, as the two walks beside it
/// are.
fn every_row_of_the_categories_page_is_declared(catalog: &Catalog) -> usize {
    let mut judged = 0_usize;
    let mut section = String::new();
    for line in page("categories.md").lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            heading.trim().trim_matches('`').clone_into(&mut section);
            continue;
        }
        let Some(name) = line
            .strip_prefix("| `")
            .and_then(|rest| rest.split('`').next())
        else {
            continue;
        };
        assert!(
            catalog
                .categories
                .iter()
                .any(|category| category.name == name && category.set == section),
            "docs/categories.md publishes `{name}` under `{section}`, which \
             catalog/categories.toml does not declare there"
        );
        judged += 1;
    }
    assert_eq!(
        judged,
        catalog.categories.len(),
        "{judged} row(s) of `docs/categories.md` were judged against what the registry \
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
    // And the daemon's, by the key `[daemon]` writes each under.
    let daemon: Vec<&str> = catalog
        .limits
        .daemon
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
            "Daemon" => daemon.as_slice(),
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
        (
            "commands.md",
            "catalog/commands.toml, catalog/i18n/ and catalog/wiki/",
        ),
        ("log-events.md", "catalog/log-events.toml"),
        (
            "capabilities.md",
            "catalog/capabilities.toml and catalog/i18n/",
        ),
        ("categories.md", "catalog/categories.toml and catalog/i18n/"),
        (
            "external-identifiers.md",
            "catalog/external-identifiers.toml, catalog/i18n/ and catalog/wiki/",
        ),
        ("mcp-tools.md", "catalog/commands.toml and catalog/i18n/"),
        (
            "cli.md",
            "apps/cli/src/args.rs, apps/cli/src/exit.rs, apps/cli/src/help.rs, \
             apps/cli/src/render.rs, catalog/commands.toml and catalog/i18n/",
        ),
        (
            "keybindings.md",
            "crates/nc-schema/src/keys.rs, crates/nc-channel/src/lib.rs, catalog/commands.toml \
             and catalog/i18n/",
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
        for named in source.split(" and ").flat_map(|part| part.split(", ")) {
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

    every_reversibility_is_published_as_the_answer_it_means();

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
    let published = reversibility_word(Reversibility::Irreversible, false, false, false);
    assert!(
        REFUSALS.contains(&published),
        "an irreversible command would be published as `{published}`, which is not one \
         of the answers this repository is allowed to give — undo stops at such a \
         command rather than reversing it"
    );
}

/// Each reversibility, and each kind of `write` command, is published as the
/// answer it means to "can it be undone".
///
/// Its own function because the test that calls it is at clippy's line limit.
#[track_caller]
fn every_reversibility_is_published_as_the_answer_it_means() {
    for reversibility in [
        Reversibility::Read,
        Reversibility::Write,
        Reversibility::Irreversible,
    ] {
        for (the_history, taken_back_whole, runs_another) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let meant = match reversibility {
                Reversibility::Read => {
                    "nothing to undo — it makes no step, so undo passes over it (§11.5)"
                }
                Reversibility::Write if the_history => {
                    "yes, for what the kernel keeps — its `store` writes and the blobs it made \
                     — through `kernel.undo` and `kernel.redo`; a step that left the machine or \
                     cannot be undone, with no compensation registered for it, is one undo stops \
                     at, and a timer it armed or an event it published stays as it is (§11.5)"
                }
                Reversibility::Write if taken_back_whole => {
                    "yes — its one effect is the value it writes, which `kernel.undo` puts back \
                     as it was and `kernel.redo` writes again — each waiting for a person's \
                     confirmation when it would switch MCP on (§11.5)"
                }
                Reversibility::Write if runs_another => {
                    "as what it runs — no step of its own: the command it dispatches is a step \
                     of its own when it changes state, which `kernel.undo` takes back as that \
                     command's (§11.5)"
                }
                Reversibility::Write => "no — a step that ran it is one undo stops at (§11.5)",
                Reversibility::Irreversible => "**no**, and a human confirms it each time (§14.6)",
            };
            assert_eq!(
                reversibility_word(reversibility, the_history, taken_back_whole, runs_another),
                meant,
                "`{reversibility:?}` is published as `{}`",
                reversibility_word(reversibility, the_history, taken_back_whole, runs_another)
            );
        }
    }
}

/// The events the sources under `directory` record, read as the names the
/// catalog gives them, once at least `floor` sources are found there.
fn the_events_recorded_under(
    directory: &str,
    floor: usize,
    catalog: &Catalog,
) -> std::collections::BTreeSet<String> {
    fn sources(directory: &Path, into: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(directory)
            .expect("a logger's sources are readable")
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
    let mut files = Vec::new();
    sources(&root().join(directory), &mut files);
    assert!(
        files.len() >= floor,
        "{} source(s) of `{directory}` were found, and it holds at least {floor}",
        files.len()
    );
    let text: String = files
        .iter()
        .map(|file| std::fs::read_to_string(file).expect("a source is readable"))
        .collect();
    catalog
        .events
        .iter()
        .map(|entry| entry.name.clone())
        .filter(|name| {
            let named = format!("nc_log::events::{}", type_of(name));
            text.match_indices(&named).any(|(at, _)| {
                !text[at + named.len()..].starts_with(|c: char| c.is_alphanumeric() || c == '_')
            })
        })
        .collect()
}

/// The events `docs/log-events.md` says each crate that logs emits are the
/// events its sources record, both ways.
///
/// **A pin cannot see an omission.** A sentence naming every `kernel.*` event,
/// `journal.verified` and `journal.broken` while
/// `crates/nc-kernel/src/dispatch.rs` records `net.refused` beside them is held
/// as written by the prose pin above, omission and all. Measured, with that
/// sentence on the page: this test red, with `net.refused` on the kernel's side
/// alone. So the names are read from both sides, for the kernel and for the
/// daemon each: every `nc_log::events::` type its sources name, put back into
/// the dotted name the catalog gives it, against what the sentence says it
/// emits.
#[test]
fn the_events_the_page_says_each_logger_emits_are_the_ones_it_records() {
    let catalog = catalog();
    let published = page("log-events.md");
    let sentence = published
        .lines()
        .find(|line| {
            line.starts_with(
                "**`nc-kernel` and `nc-daemon` are the crates outside `nc-log` that log.**",
            )
        })
        .expect("the page says which crates log");
    let emitted = |by: &str| -> Vec<&str> {
        let (named, _) = sentence
            .split_once(by)
            .and_then(|(_, rest)| rest.split_once(';'))
            .unwrap_or_else(|| panic!("the sentence says what {by:?}: {sentence}"));
        named.split('`').skip(1).step_by(2).collect()
    };
    // **A floor for each logger, from outside what it reads**: how many
    // sources it holds at least, and one event it is known to record — so a
    // reader that finds a handful of files, or none of the events, fails here
    // rather than comparing two empty sets.
    for (by, directory, wildcard, floor, known) in [
        (
            "The kernel emits ",
            "crates/nc-kernel/src",
            Some("kernel."),
            11,
            "kernel.plugin_killed",
        ),
        (
            "the daemon emits ",
            "apps/daemon/src",
            None,
            6,
            "ids.clock_backwards",
        ),
    ] {
        let named = emitted(by);
        let recorded = the_events_recorded_under(directory, floor, &catalog);
        assert!(
            recorded.contains(known),
            "`{known}` was not found recorded in `{directory}`, so the reader is not \
             reading what that logger records"
        );
        for name in &named {
            assert!(
                name.ends_with(".*") || catalog.events.iter().any(|entry| entry.name == *name),
                "the page says {by:?}`{name}`, which the catalog does not declare"
            );
        }
        let said: std::collections::BTreeSet<String> = catalog
            .events
            .iter()
            .map(|entry| entry.name.clone())
            .filter(|name| {
                wildcard.is_some_and(|prefix| {
                    named.contains(&format!("{prefix}*").as_str()) && name.starts_with(prefix)
                }) || named.contains(&name.as_str())
            })
            .collect();
        assert_eq!(
            said, recorded,
            "`docs/log-events.md` says {by:?}the first set and `{directory}` records the \
             second: an event on one side only is a sentence a reader trusts about a log \
             line that is, or is not, there"
        );
    }
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
