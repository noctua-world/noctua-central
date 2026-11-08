//! Every value a registry declares, against the constant it became.
//!
//! **This is the gap drift cannot see.** `cargo xtask check` compares the files
//! under `crates/nc-catalog/src/` with what the emitter produces *now*, so an
//! emitter that carries a field wrongly agrees with itself and the check is
//! green. What stands between the registry and the code is this file.
//!
//! It lives in `xtask` rather than in `nc-catalog` for one reason: `nc-catalog`
//! has **no dependencies**, deliberately, because the kernel and every app take
//! it. `xtask` already holds both the compiler and the generated crate, and a
//! parser for the registries, so nothing new is pulled in to check them.
//!
//! The failure this exists for: without it, an emitter patched to write every
//! severity as `Info`, every command as `Reversibility::Read`, every `mutating`
//! as `false`, every field kind as `U64` and every store bound as zero passes
//! `cargo xtask check` — drift, determinism, the ledger and the whole test
//! suite — with nothing red. `mutating` is what undo, the journal and the
//! allowlist read; `Reversibility::Irreversible` is what forces a human
//! confirmation; `FieldKind` **is** the `§15.4` type wall.

use std::path::{Path, PathBuf};

use nc_catalog_compiler::compile;
use nc_catalog_compiler::ledger::Bootstrap;
use nc_catalog_compiler::model::{
    Catalog, Database as ModelDatabase, FieldType, Level, Severity as ModelSeverity,
    Strategy as ModelStrategy,
};

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).to_path_buf()
}

fn catalog() -> Catalog {
    compile(&root(), Bootstrap::Refuse)
        .expect("the catalog this repository ships compiles")
        .catalog
}

fn severity_matches(model: ModelSeverity, generated: nc_catalog::Severity) -> bool {
    matches!(
        (model, generated),
        (ModelSeverity::Info, nc_catalog::Severity::Info)
            | (ModelSeverity::Warning, nc_catalog::Severity::Warning)
            | (ModelSeverity::Error, nc_catalog::Severity::Error)
            | (ModelSeverity::Critical, nc_catalog::Severity::Critical)
    )
}

/// The top-level keys of a JSON Schema's `properties`, in the order written.
///
/// A brace-depth scan over the text rather than a parse, because a parse is
/// exactly what this exists to be independent of: `serde_json` sorts object
/// keys unless a feature says otherwise, and the emitter and any parser-based
/// expectation would then sort together and agree about the wrong thing.
fn properties_in_order(schema: &str) -> Vec<String> {
    let Some(after) = schema.split_once("\"properties\"") else {
        return Vec::new();
    };
    let text: Vec<char> = after.1.chars().collect();
    let mut found = Vec::new();
    let mut depth = 0_usize;
    let mut index = 0_usize;
    while index < text.len() {
        match text[index] {
            '{' => depth += 1,
            '}' => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            '"' if depth == 1 => {
                let mut name = String::new();
                index += 1;
                while index < text.len() && text[index] != '"' {
                    name.push(text[index]);
                    index += 1;
                }
                found.push(name);
            }
            _ => {}
        }
        index += 1;
    }
    found
}

/// The word each enum answers is the word its registry entry carries.
///
/// **`Severity` and `Level` carry their own text form**, the generated `as_str`,
/// because without one every consumer writes its own — the pages test's
/// `severity_word` is one — and `{:?}` gives `Critical` where the ledger and both
/// pages write `critical`. `Severity`'s own doc names the hazard — "two
/// vocabularies for one judgement is how they drift apart".
///
/// Held against the *loaded* registry, which is what this file is for: the
/// generated `as_str` on one side and the catalog's own value on the other.
#[test]
fn every_word_an_enum_answers_is_the_registrys_own() {
    let catalog = catalog();
    let mut compared = 0_usize;
    for entry in &catalog.errors {
        let code = nc_catalog::ErrorCode::ALL
            .iter()
            .find(|code| code.code() == entry.code)
            .expect("every declared code reaches the crate");
        let recorded = match entry.severity {
            nc_catalog_compiler::model::Severity::Info => "info",
            nc_catalog_compiler::model::Severity::Warning => "warning",
            nc_catalog_compiler::model::Severity::Error => "error",
            nc_catalog_compiler::model::Severity::Critical => "critical",
        };
        assert_eq!(
            code.severity().as_str(),
            recorded,
            "`{}` answers a severity the registry does not declare",
            entry.code
        );
        compared += 1;
    }
    for entry in &catalog.events {
        let spec = nc_catalog::EVENTS
            .iter()
            .find(|spec| spec.name == entry.name)
            .expect("every declared event reaches the crate");
        assert_eq!(
            spec.level.as_str(),
            level_word(entry.level),
            "`{}` answers a level the registry does not declare",
            entry.name
        );
        compared += 1;
    }
    assert!(compared > 20, "only {compared} value(s) were compared");
}

/// What each level must be spelled as in the generated crate.
///
/// **Written out here, not asked of `Level::written`**, for the reason
/// [`rust_type`] gives: a mirror that consults the thing it mirrors agrees with
/// it whatever either says.
///
/// **And outside the loop that uses it**, so a test can reach it. Inside
/// `every_declared_value_reaches_the_crate` it would be driven only by the
/// levels `catalog/log-events.toml` declares — and it declares `debug`, `info`,
/// `warn` and `error`, so the `Trace` arm would be reached by nothing and could
/// hold any word at all.
const fn level_word(level: Level) -> &'static str {
    match level {
        Level::Trace => "trace",
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warn => "warn",
        Level::Error => "error",
    }
}

/// Every variant of both enums is spelled as the word it names, all fifteen.
///
/// **Three of this file's mirror tables have arms the registry never reaches.**
/// The registry declares 6 of the 10 field kinds and 4 of the 5 levels, so the
/// `I64`, `Bool`, `Id` and `Enum` rows of [`rust_type`] and of [`label`], and
/// the `Trace` row of [`level_word`], are driven by nothing at all: measured,
/// seven distinct wrong spellings across the three tables gave `check passed`
/// byte-identical to a clean run, while one wrong spelling on a *live* arm of
/// each fires at once.
///
/// No shipped word is unprotected by it today — `model.rs`'s
/// `every_field_kind_is_spelled_as_the_thing_it_names` and `nc-log`'s
/// `every_level_has_a_name_the_store_reads` pin the originals these mirror. What
/// a dead arm is, is an expectation nobody has read: the day the registry
/// declares an `i64` field or a `trace` event, the checker built to catch a
/// wrong emitter is itself wrong, and the two hand tables agree about a value
/// neither is driven with.
///
/// Written out as exhaustive `match`es, the shape
/// `every_variant_is_published_as_the_word_it_names` uses in
/// `the_pages_reach_the_catalog.rs`: a variant added to either enum is a compile
/// error in this file rather than a row that quietly stops being checked.
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
            "`{level:?}` is compared against `{}`, and the level a store reads is the \
             registry's own spelling",
            level_word(level)
        );
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
        let (meant_label, meant_type) = match kind {
            FieldType::U64 => ("count", "u64"),
            FieldType::I64 => ("signed number", "i64"),
            FieldType::U32 => ("whole number, at most 32 bits wide", "u32"),
            FieldType::Bool => ("flag", "bool"),
            FieldType::Duration => ("span of time", "core::time::Duration"),
            FieldType::ErrorCode => ("code from the error registry", "nc_catalog::ErrorCode"),
            FieldType::Id => (
                "opaque identifier, written to the log exactly as it is",
                "nc_ids::OpaqueId",
            ),
            FieldType::Snowflake => (
                "Snowflake, written as its Crockford base32 text",
                "nc_ids::Snowflake",
            ),
            FieldType::Literal => (
                "fixed text chosen when the code was written, never at run time",
                "crate::Literal",
            ),
            FieldType::Enum => (
                "variant of an enum the platform declares — a state, never a name",
                "crate::Variant",
            ),
        };
        assert_eq!(
            label(kind),
            meant_label,
            "`{kind:?}` is compared against the doc comment `{}`, which is not what this \
             kind holds",
            label(kind)
        );
        assert_eq!(
            rust_type(kind),
            meant_type,
            "`{kind:?}` is compared against the Rust type `{}`, and every type here is \
             one that implements `LogValue` — which is the whole wall (§15.4)",
            rust_type(kind)
        );
    }

    // And no two variants of either enum are compared against one word: a
    // table of strings collapses as easily as it mis-spells, and every
    // assertion above still holds if two written-out arms move together.
    let words: std::collections::BTreeSet<&str> = [
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
    ]
    .into_iter()
    .map(rust_type)
    .collect();
    assert_eq!(words.len(), 10, "two field kinds share a Rust type");
}

/// Every doc line the emitter derives from the registry says what the registry
/// says.
///
/// **The accessor is compared and the line beside it is not.**
/// `every_generated_value_is_compared.rs` requires a comparison for every public
/// *field and accessor*; a doc comment above a variant is neither, so the
/// emitter could write anything there. Measured, one expression changed so that
/// every code documents itself with the first code's description:
///
/// ```text
/// /// NC-JRNL-0001 — A payload is not the canonical form this platform carries
/// JournalChainBroken,
/// /// NC-JRNL-0002 — A payload is not the canonical form this platform carries
/// JournalEntryTampered,
///   ->  check passed
/// ```
///
/// Every code reads as `NC-CANON-0001`. That line is what an IDE shows on hover and
/// what `cargo doc` publishes, so it is the description most readers actually
/// meet.
///
/// Compared against the registry rather than pinned, because it *is* derived:
/// a written list would have to be re-written whenever a description is edited,
/// which is how a list stops being read.
#[test]
fn every_derived_doc_line_says_what_the_registry_says() {
    let catalog = catalog();
    let mut checked = 0_usize;

    let errors = std::fs::read_to_string(root().join("crates/nc-catalog/src/errors.rs"))
        .expect("the crate is generated");
    // **Where the line stands, in this loop too**, as in the events loop below,
    // whose comment explains that a permutation satisfies an existence
    // assertion. Measured with membership here: every code documented as the
    // next code in the sorted order, and both locales exchanged —
    // `check passed`, with `docs/errors.md` still correct, so the published
    // page and the crate disagreed with no other gate comparing them.
    for entry in &catalog.errors {
        let expected = format!("/// {} — {}", entry.code, entry.description);
        let variant = nc_catalog_compiler::error_variant(&entry.message);
        let at = errors
            .find(&format!("\n    {variant},"))
            .unwrap_or_else(|| panic!("`{variant}` is a variant of the generated enum"));
        let above = the_doc_block_above(&errors, at);
        assert!(
            the_summary_line_of(&above) == expected,
            "`{variant}` is documented as something other than `{}`: the accessor is \
             compared and the line a reader hovers is not",
            entry.description
        );
        checked += 1;
    }

    let events = std::fs::read_to_string(root().join("crates/nc-log/src/events.rs"))
        .expect("the crate is generated");
    // **Where the line stands, not whether the file holds it.**
    // `lines().any(…)` over the whole file is satisfied by a *permutation*: the
    // multiset is unchanged. Measured — every struct documented as the next
    // event in the sorted order, with the registry, the pages and
    // `docs/log-events.md` all correct — `check passed` under `any`. That `///`
    // line is what rustdoc publishes and what an editor shows on hover; it is
    // the only prose a developer meets when picking an event to record.
    //
    // The two other readers of this file take their block from `find("pub
    // struct {name} {")`, and a doc comment sits *above* `pub struct`, outside
    // every block they read, so attachment is decided here or nowhere.
    for entry in &catalog.events {
        let expected = format!("/// `{}` — {}", entry.name, entry.description);
        let name = nc_catalog_compiler::camel(&entry.name);
        let declares = format!("pub struct {name} {{");
        let at = events
            .find(&declares)
            .unwrap_or_else(|| panic!("`{name}` is declared in the generated crate"));
        let above = the_doc_block_above(&events, at);
        assert!(
            the_summary_line_of(&above) == expected,
            "`{name}` is documented as something other than `{}` — `{expected}` is not in \
             its own doc block",
            entry.description
        );
        checked += 1;
    }

    let i18n = std::fs::read_to_string(root().join("crates/nc-catalog/src/i18n.rs"))
        .expect("the crate is generated");
    for locale in &catalog.locales {
        let expected = format!("/// {}", locale.meta.name);
        let variant = nc_catalog_compiler::camel(&locale.meta.bcp47.replace('-', "_"));
        let at = i18n
            .find(&format!("\n    {variant},"))
            .unwrap_or_else(|| panic!("`{variant}` is a variant of the generated enum"));
        let above = the_doc_block_above(&i18n, at);
        assert!(
            the_summary_line_of(&above) == expected,
            "`{variant}` is documented with an endonym that is not `{}` — the two were \
             exchanged and the message says so",
            locale.meta.name
        );
        checked += 1;
    }

    // **And the two registries whose generated enums this test reads too.**
    // The doc on `the_enums_a_doc_line_is_the_whole_meaning_of_are_read_whole`
    // says `Email` and `Notification` are held here, and this loop holds the
    // doc line above each variant of `notifications.rs` and `emails.rs` to the
    // registry's id — measured, an id line changed in the emitter leaves
    // `check passed` with a byte-identical transcript when this loop is gone.
    // Each is one variant today, which is exactly when a loop is cheap to leave
    // out and cheap to add.
    for (file, ids) in [
        (
            "crates/nc-catalog/src/notifications.rs",
            catalog
                .notifications
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<String>>(),
        ),
        (
            "crates/nc-catalog/src/emails.rs",
            catalog
                .emails
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<String>>(),
        ),
    ] {
        let generated = std::fs::read_to_string(root().join(file)).expect("the crate is generated");
        for id in &ids {
            let expected = format!("/// {id}");
            let variant = nc_catalog_compiler::camel(&id.replace('.', "_"));
            let at = generated
                .find(&format!("\n    {variant},"))
                .unwrap_or_else(|| panic!("`{variant}` is a variant of the generated enum"));
            let above = the_doc_block_above(&generated, at);
            assert!(
                the_summary_line_of(&above) == expected,
                "`{variant}` in `{file}` is documented as something other than `{id}`"
            );
            checked += 1;
        }
    }

    // **The loops added up, not a floor under their sum.** A floor such as
    // `checked > 30` is cleared by the errors alone, so any other loop could be
    // made to read nothing with the suite green — and the events loop is what
    // holds those `///` lines to the registry. Measured against
    // that floor: `.take(0)` on the events loop, then on the locales loop, both
    // **green**; the same on the errors loop is the control, and it is red.
    //
    // A sum of what the catalog declares cannot be cleared by two thirds of the
    // work.
    let declared = catalog.errors.len()
        + catalog.events.len()
        + catalog.locales.len()
        + catalog.notifications.len()
        + catalog.emails.len();
    assert_eq!(
        checked, declared,
        "{checked} derived doc line(s) were read and this catalog declares {declared}: a \
         loop that reads nothing is a loop that proves nothing"
    );
}

/// The doc block that lands on the item beginning at `at`, in source order.
///
/// **One reader for every loop, because three readers drift narrower.** A loop
/// that stops at the first line that is not `///` lets an attribute between a
/// block and its item hide the block from it, while a sibling loop carries the
/// two clauses alone. Written three times, the strictest copy is the one nobody
/// notices is missing.
///
/// The blank-line skip is for an item that is *indented*: the slice then ends
/// mid-line and the last thing `lines()` yields is that indentation. Generated
/// items stand at column zero, which is why the clause decides nothing here and
/// why the corpus beside this drives it.
///
/// An attribute between a doc block and its item does not detach the block:
/// both runs accumulate on the item below. That is the same rule
/// `the_documented_items_are_the_ones_written_down` is built on.
fn the_doc_block_above(text: &str, at: usize) -> Vec<&str> {
    // **A wrapped attribute is still one attribute.** `take_while` stops at the
    // first line that is neither `///` nor `#[`, and rustfmt breaks a long
    // attribute across lines whose continuations are neither — so the block
    // above one is never read, and a reader of this shape then fails with a
    // message that is false — measured with a plant in `nc-journal`, whose two
    // readers go through its own copy of `past_the_attributes` for that reason.
    // This copy is cold today because every generated item stands at column
    // zero with no attribute above it, and being cold is not being right.
    let lines: Vec<&str> = text[..at].lines().collect();
    let mut item = lines.len();
    while item > 0 && lines[item - 1].trim().is_empty() {
        item -= 1;
    }
    let mut opens = past_the_attributes(&lines, item);
    while opens > 0 {
        let trimmed = lines[opens - 1].trim_start();
        if trimmed.starts_with("///") || trimmed.starts_with("#[") {
            opens -= 1;
        } else {
            break;
        }
    }
    lines[opens..item].to_vec()
}

/// Whether a line opens more brackets than it closes.
///
/// A wrapped attribute's first line does; a doc line ending in an intra-doc
/// link, a `//` comment ending in `]`, and a balanced macro call `thing![a, b]`
/// do not. That is what separates a continuation from a coincidence.
fn more_opens_than_closes(line: &str) -> bool {
    let mut depth = 0_i32;
    for letter in line.chars() {
        match letter {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

/// The line a doc block starts at, walking up past an item's attributes.
///
/// The last line of a rustfmt-wrapped attribute closes with `]` and its first
/// begins `#[`, so a bounded walk back to that first line is what joins the two.
/// The bound is what stops a stray `]` — an array closing above an item —
/// walking the reader out of the block it is reading.
///
/// **The third copy, with the same repair.** A bare `]` test is too weak — an
/// intra-doc link ends a doc line in `]`, so the walk leaves the item and lands
/// on a neighbour's attribute — and the same reader lives in
/// `xtask/src/gates.rs` and `crates/nc-journal/src/lib.rs` as well as here. One
/// rule written three times is one rule with three answers, so
/// `every_copy_of_the_attribute_walk_reads_the_same` in `xtask/src/check.rs`
/// compares the three bodies and fails on the next divergence.
fn past_the_attributes(lines: &[&str], mut at: usize) -> usize {
    const WRAPPED_AT_MOST: usize = 12;
    while at > 0 {
        let line = lines[at - 1].trim_end();
        if line.trim_start().starts_with("#[") {
            at -= 1;
            continue;
        }
        let is_a_comment = |text: &str| text.trim_start().starts_with("//");
        if line.ends_with(']') && !is_a_comment(line) {
            let mut back = at - 1;
            let mut spent = 0;
            while back > 0 && spent < WRAPPED_AT_MOST {
                if lines[back].trim_start().starts_with("#[") {
                    break;
                }
                back -= 1;
                spent += 1;
            }
            let opens_unclosed = lines.get(back).is_some_and(|first| {
                first.trim_start().starts_with("#[") && more_opens_than_closes(first)
            });
            if opens_unclosed && !is_a_comment(lines[back]) {
                at = back;
                continue;
            }
        }
        break;
    }
    at
}

/// The block reader decides every clause it carries.
///
/// **Without this, both of them are reached by nothing.** Measured: the blank
/// skip and the attribute clause each deleted with the whole suite green,
/// because every generated item stands at column zero with nothing but `///`
/// above it. A clause no case reaches is a clause nobody can tell is wrong, and
/// this one is the difference between reading an item's documentation and
/// reading none.
#[test]
fn the_block_reader_is_not_stopped_by_a_blank_line_or_an_attribute() {
    let indented =
        "impl Registry {\n    /// Says a thing.\n    #[must_use]\n    pub fn probe() {}\n";
    let at = indented.find("pub fn probe").expect("the item is here");
    assert_eq!(
        the_doc_block_above(indented, at),
        ["    /// Says a thing.", "    #[must_use]"],
        "an indented item's block ends at the indentation the slice stops in, or at the \
         attribute between the block and the item"
    );
    let plain = "/// Says a thing.\npub struct Probe;\n";
    let at = plain.find("pub struct").expect("the item is here");
    assert_eq!(the_doc_block_above(plain, at), ["/// Says a thing."]);
    let detached = "/// Another item's line.\npub struct Other;\n\npub struct Probe;\n";
    let at = detached.rfind("pub struct").expect("the item is here");
    let seen = the_doc_block_above(detached, at);
    assert!(
        seen.is_empty(),
        "a block belonging to the item above was read as this one's: {seen:?}"
    );
}

/// The first `///` line of a doc block — the one rustdoc publishes as the
/// item's summary.
///
/// **"Where the line stands" is not membership.** A loop checking that the
/// registry's sentence is *somewhere* in the block misses that rustdoc takes
/// the **first** line as the description a reader meets on hover and in every
/// generated index. Measured with membership: every error code documented
/// first as the *next* code in the sorted order, with the real sentence still
/// present two lines down — `check passed`. That is the events loop's
/// permutation defect, one step along: a permutation satisfies a membership
/// test, and so does a prefix.
fn the_summary_line_of(block: &[&str]) -> String {
    block
        .iter()
        .map(|line| line.trim())
        .find(|line| line.starts_with("///"))
        .unwrap_or_default()
        .to_owned()
}

/// Every constant whose doc line claims an order is in that order.
///
/// **Each of these says so, and this is what reads the claim.** The doc line is
/// emitted beside the sort, and the other comparisons of these constants in this
/// file locate entries by key — `KEYS` is compared as a `BTreeSet`, which
/// discards order explicitly — so without this, reversing each comparator leaves
/// `cargo xtask build && cargo xtask check` green with the constant running
/// backwards under a sentence saying it does not:
///
/// ```text
/// KEYS         /// Every message key, sorted.
/// TABLES       /// … in name order.
/// RATE_LIMITS  /// Every rate limit, in surface order.
/// EVENTS       /// Every event, in name order.
/// COMMANDS     /// Every command, in name order.
/// AREAS        (the fixed area list)
/// ```
///
/// `ErrorCode::ALL`'s order *is* pinned, which is what makes these six a
/// precedent rather than an oversight: a consumer building a locale file or a
/// command list from `KEYS` or `COMMANDS` gets the reverse of what the doc
/// promises.
///
/// Read through the public surface rather than off the emitted text, so what is
/// checked is the order a caller sees.
#[test]
fn every_constant_that_claims_an_order_is_in_it() {
    let sorted = |names: Vec<&str>, what: &str| {
        let mut expected = names.clone();
        expected.sort_unstable();
        assert_eq!(
            names, expected,
            "`{what}`'s doc line says it is in order and it is not — the sentence beside \
             the sort is the contract a consumer reads"
        );
        assert!(
            names.len() > 1,
            "`{what}` holds {} entr(ies), so its order says nothing",
            names.len()
        );
    };
    sorted(nc_catalog::KEYS.to_vec(), "KEYS");
    sorted(
        nc_catalog::TABLES.iter().map(|table| table.name).collect(),
        "TABLES",
    );
    sorted(
        nc_catalog::RATE_LIMITS
            .iter()
            .map(|limit| limit.surface)
            .collect(),
        "RATE_LIMITS",
    );
    sorted(
        nc_catalog::events::EVENTS
            .iter()
            .map(|event| event.name)
            .collect(),
        "EVENTS",
    );
    sorted(
        nc_catalog::commands::COMMANDS
            .iter()
            .map(|command| command.name)
            .collect(),
        "COMMANDS",
    );
    sorted(
        nc_catalog::AREAS.iter().map(|(name, _)| *name).collect(),
        "AREAS",
    );
}

/// `ErrorCode`'s variants are in code order, and `ALL` says so.
///
/// **The emitter sorts, and nothing checked which way.** `emit.rs` writes the
/// variants `sorted.sort_by(|a, b| a.code.cmp(&b.code))` and the generated doc
/// line above `ALL` reads "Every code, in code order" — reverse the comparison
/// and `cargo xtask build && cargo xtask check` is green with `ALL` running
/// backwards. The published page's ordering *is* asserted
/// (`the_pages_reach_the_catalog.rs`), so the page and the enum could disagree
/// again, which the emitter's own comment records having happened once.
///
/// It matters beyond the constant: `ErrorCode` derives `Ord`, and for a derived
/// `Ord` declaration order **is** the comparison — the paragraph the emitter
/// writes twenty lines further down says exactly that about `Severity`, whose
/// order is pinned. This is the same claim for the enum that has 28 variants
/// and no pin.
#[test]
fn every_error_code_is_declared_in_code_order() {
    let catalog = catalog();
    let mut declared: Vec<&str> = catalog
        .errors
        .iter()
        .map(|entry| entry.code.as_str())
        .collect();
    assert!(
        declared.len() > 20,
        "{} code(s) in the registry, so this compares almost nothing",
        declared.len()
    );
    declared.sort_unstable();

    // Read from the generated file rather than from `ErrorCode::ALL`, because
    // what is being checked is the order the *emitter wrote*, and iterating the
    // constant would compare the emitter's output with itself.
    let text = std::fs::read_to_string(root().join("crates/nc-catalog/src/errors.rs"))
        .expect("the crate is generated");
    let block = text
        .split_once("pub const ALL: &'static [Self] = &[")
        .expect("`ALL` is emitted")
        .1
        .split_once("];")
        .expect("`ALL` closes")
        .0;
    let variants: Vec<&str> = block
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix("Self::"))
        .filter_map(|rest| rest.strip_suffix(','))
        .collect();
    assert_eq!(
        variants.len(),
        declared.len(),
        "`ALL` holds {} variant(s) and the registry declares {}",
        variants.len(),
        declared.len()
    );

    // The variant *names* are the emitter's own spelling, so comparing them
    // against a reconstruction of that spelling would compare the emitter with
    // itself. What the doc line claims is a property — ascending by code — and
    // `ErrorCode::code()` is the independent side of it: the accessor is
    // compared against the registry elsewhere in this file, so reading order
    // through it relates the enum to the registry rather than to the emitter.
    let published: Vec<&str> = nc_catalog::errors::ErrorCode::ALL
        .iter()
        .map(|code| code.code())
        .collect();
    assert_eq!(
        published, declared,
        "`ErrorCode::ALL` is not in code order, and the doc line above it says it is — \
         which is also the comparison a derived `Ord` gives every consumer"
    );
    // And the sentence itself, because a claim nobody makes is not one this
    // test is holding anything to.
    assert!(
        text.contains("/// Every code, in code order.\n    pub const ALL"),
        "the line above `ALL` no longer states the order this test holds it to"
    );
}

/// The doc lines the generated crate carries that state a contract.
///
/// **The emitter writes prose, and nothing read any of it.**
/// `every_generated_value_is_compared.rs` requires a comparison for every public
/// *field and accessor*; a doc line above one is neither, so the emitter could
/// say anything. Measured, three lines changed in `emit.rs` and
/// `cargo xtask build && cargo xtask check` green:
///
/// ```text
/// crates/nc-catalog/src/limits.rs:28  /// … Oldest is largest.     <- backwards
/// crates/nc-catalog/src/events.rs:34  /// A signed 64-bit number.  <- on U64
/// crates/nc-catalog/src/events.rs:36  /// An unsigned 64-bit count. <- on I64
/// ```
///
/// The first is the pruner's published contract — `nc-prune` issues
/// `ORDER BY … ASC` and deletes what it finds, so "oldest is smallest" is the
/// sentence a reader of `catalog/limits.toml` relies on when they choose an
/// ordering column, and reversed it tells them to choose one that deletes the
/// newest rows. The other two label the two types a `LogValue` field can be
/// that differ only in sign, which is the pair a reader is most likely to have
/// to look up.
///
/// A written list rather than a sweep, because most doc lines are prose and
/// only some are a promise. Each entry says which promise.
#[test]
fn the_generated_crate_states_its_contracts() {
    let generated = root().join("crates/nc-catalog/src");
    let mut checked = 0_usize;
    for (file, above, line, why) in [
        (
            "limits.rs",
            "pub order_by: &'static str,",
            "/// The column that orders the table for pruning. Oldest is smallest.",
            "`nc-prune` issues `ORDER BY … ASC LIMIT n` and deletes those rows, so this \
             sentence is what tells a person choosing an ordering column which end of it \
             is deleted",
        ),
        (
            "events.rs",
            "U64,",
            "/// An unsigned 64-bit count.",
            "the two 64-bit variants differ only in sign, and the label is the only place \
             a reader is told which is which",
        ),
        (
            "events.rs",
            "I64,",
            "/// A signed 64-bit number.",
            "the same pair, the other way round",
        ),
        // **`§10.6`'s consequence, and the pair that inverts it.** These two
        // exchanged in the emitter leave `cargo xtask build && cargo xtask
        // check` green without this row, measured: `OnLockout::Refuse` then
        // documents itself as destroying the stored device credential. `device_pin` is the one
        // surface that declares `WipeCredential`, and this enum is the public
        // API a later stage's lockout handler is written against — a developer
        // reading the wrong line wipes a credential on an ordinary refusal.
        (
            "limits.rs",
            "Refuse,",
            "/// Stop answering until the window passes.",
            "a refusal waits; it does not destroy anything",
        ),
        (
            "limits.rs",
            "WipeCredential,",
            "/// Destroy the stored device credential, forcing a full re-authentication.",
            "`NOCTUA.md §10.6`'s consequence, and the reason `cumulative` exists",
        ),
    ] {
        let text = std::fs::read_to_string(generated.join(file)).expect("the crate is generated");
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let at = lines
            .iter()
            .position(|written| *written == above)
            .unwrap_or_else(|| panic!("`{file}` no longer declares `{above}`"));
        assert!(at > 0, "`{above}` is the first line of `{file}`");
        assert_eq!(
            lines[at - 1],
            line,
            "the line above `{above}` in the generated `{file}` is not what the emitter \
             is supposed to write, and {why}"
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "not every contract line was read");
}

/// The enums whose bodies are compared whole, and what each must say.
///
/// Written beside the test rather than inside it, because a body holding all
/// of them crosses the hundred-line budget `clippy.toml` sets — and a budget
/// met by reading fewer enums is the shape this test exists to refuse.
const READ_WHOLE: [(&str, &str, &[&str]); 7] = [
    (
        "commands.rs",
        "pub enum Reversibility {",
        &[
            "/// Changes nothing.",
            "Read,",
            "/// Changes state; `kernel.undo` takes back what a step of it kept in the store \
                 and the blobs it made, stops at a step that left the machine or cannot be \
                 undone with no compensation registered, and leaves a timer it armed or an \
                 event it published as it is.",
            "Write,",
            "/// Cannot be reversed, and therefore requires a human confirmation the \
                 kernel issues.",
            "Irreversible,",
        ],
    ),
    (
        "limits.rs",
        "pub enum Strategy {",
        &[
            "/// The oldest rows go first, in batches.",
            "OldestFirst,",
            "/// The pruner does not touch this table: its retention is a policy-pack \
                 property, per realm.",
            "PolicyPack,",
            "/// The pruner does not touch this table: its oldest rows are user state or a \
                 unit's, and its writer refuses a new row at `max_rows`.",
            "RefuseAtLimit,",
        ],
    ),
    // Which directory of migrations builds a table, and so which target the
    // pruner asks about it: the two lines are all a reader of the crate is told.
    (
        "limits.rs",
        "pub enum Database {",
        &[
            "/// PostgreSQL, built by `migrations/`.",
            "Server,",
            "/// The SQLite file on the user's machine, built by `local-migrations/`: \
                 `database = \"local\"`.",
            "LocalFile,",
        ],
    ),
    (
        "events.rs",
        "pub enum Level {",
        &[
            "/// Very fine detail.",
            "Trace,",
            "/// Detail a developer wants while diagnosing.",
            "Debug,",
            "/// Something worth a line in normal operation.",
            "Info,",
            "/// Degraded, but working.",
            "Warn,",
            "/// Failed.",
            "Error,",
        ],
    ),
    (
        "events.rs",
        "pub enum FieldKind {",
        &[
            "/// An unsigned 64-bit count.",
            "U64,",
            "/// A signed 64-bit number.",
            "I64,",
            "/// An unsigned 32-bit count.",
            "U32,",
            "/// A flag.",
            "Bool,",
            "/// A span of time.",
            "Duration,",
            "/// A code from the error registry.",
            "ErrorCode,",
            "/// An opaque identifier, written to the log as it is. It is not free text \
                 — but nothing stops a caller minting one *out of* personal data, so derive \
                 it, never copy it (NOCTUA.md \u{a7}5.1, which is OPEN and owns that risk).",
            "Id,",
            "/// A Snowflake, written as its Crockford base32 text.",
            "Snowflake,",
            "/// A literal in the source, never input.",
            "Literal,",
            "/// A variant of an enum the platform declares — a state, never a name.",
            "Enum,",
        ],
    ),
    (
        "limits.rs",
        "pub enum OnLockout {",
        &[
            "/// Stop answering until the window passes.",
            "Refuse,",
            "/// Destroy the stored device credential, forcing a full re-authentication.",
            "WipeCredential,",
        ],
    ),
    (
        "errors.rs",
        "pub enum Severity {",
        &[
            "/// Worth recording, nothing failed.",
            "Info,",
            "/// The operation continued, degraded.",
            "Warning,",
            "/// The operation failed.",
            "Error,",
            "/// Something a person should be woken for.",
            "Critical,",
        ],
    ),
];

/// The generated enums whose whole meaning is the line above each variant.
///
/// **A named pin describes one sentence and says nothing about the one beside
/// it.** The list above holds five lines by name; rotating the three doc lines
/// of `Reversibility` touches none of them, and leaves `Read` documented as
/// requiring a human confirmation and `Irreversible` as *"Changes nothing."*.
/// Measured twice, each rotation written into `emit.rs` and the crate rebuilt:
/// every other test in this file passed both times, the named list above
/// included, because not one of the five lines it holds is in either enum. So
/// these two bodies are compared *whole* — line for line, in order, including
/// the variant names — and a rotation, an inserted variant, a dropped doc line
/// and a reworded promise are one failure rather than four rules.
///
/// **`Strategy` too.** Measured with only `Severity` and `Reversibility`
/// compared whole: `Strategy`'s two lines exchanged and `check passed`, with
/// `OldestFirst` documenting itself as the strategy that never prunes — on the
/// enum `nc-prune` reads to decide whether a table is touched at all.
///
/// **And not three of the enums but every one this applies to.** Measured the
/// same way: `Level`'s `Trace` and `Error` doc lines rotated, in the emitter and
/// the generated crate together, as `cargo xtask build` would write them, and
/// the `check` transcript comes back **byte-identical** to a clean run — on the
/// enum whose own generated header argues that the derived `Ord` *is* the scale
/// and that written the other way round it would keep `Trace` and drop `Error`.
/// `FieldKind` is the same: the named list above holds only some of its
/// variants' lines, so exchanging `Id`'s and `Literal`'s leaves `check passed`
/// and the type wall telling a developer that `Id` is a literal in the source
/// and hanging the §5.1 personal-data warning on `Literal`.
///
/// **`Database` is the seventh**: its two lines are the whole of which
/// directory builds a table and which of the pruner's targets is asked about
/// it.
///
/// Seven, because in each the doc line is the entire published semantics. The
/// variants carry no data and no accessor says what they mean: `Severity` is the
/// scale an operator filters paging by, `Level` is the one a consumer's first
/// filter compares against, and `Reversibility::Irreversible` is what makes the
/// kernel stop and ask a person before it acts. The other four the compiler
/// emits — `ErrorCode`, `Email`, `Notification` and `Locale` — have a variant
/// per registry row and are held line by line against the registry itself by
/// [`every_derived_doc_line_says_what_the_registry_says`], which is the right
/// rule for a body nobody writes by hand.
#[test]
fn the_enums_a_doc_line_is_the_whole_meaning_of_are_read_whole() {
    let generated = root().join("crates/nc-catalog/src");
    let mut read = 0_usize;
    for (file, opening, body) in READ_WHOLE {
        let text = std::fs::read_to_string(generated.join(file)).expect("the crate is generated");
        let lines: Vec<&str> = text.lines().map(str::trim).collect();
        let at = lines
            .iter()
            .position(|written| *written == opening)
            .unwrap_or_else(|| panic!("`{file}` no longer opens `{opening}`"));
        let length = lines[at + 1..]
            .iter()
            .position(|written| *written == "}")
            .unwrap_or_else(|| panic!("`{opening}` in `{file}` is never closed"));
        assert_eq!(
            &lines[at + 1..at + 1 + length],
            body,
            "the body of `{opening}` in the generated `{file}` is not what the emitter is \
             supposed to write, and in both of these enums the line above a variant is the \
             whole of what that variant means"
        );
        read += body.len();
    }
    assert_eq!(read, 58, "not every line of all seven enum bodies was read");
}

#[test]
fn every_error_carries_its_own_severity_and_retryability() {
    let catalog = catalog();
    assert!(!catalog.errors.is_empty(), "nothing was compared");
    for entry in &catalog.errors {
        let generated = nc_catalog::ErrorCode::ALL
            .iter()
            .find(|code| code.code() == entry.code)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.code));
        assert_eq!(generated.message_key(), entry.message, "{}", entry.code);
        assert_eq!(generated.description(), entry.description, "{}", entry.code);
        assert_eq!(generated.retryable(), entry.retryable, "{}", entry.code);
        // **The parameters, which reach the code and are read here.**
        // `Notification::params()` and `Email::params()` are both compared here,
        // and so is `ErrorCode::params()`: emitted and never looked at, it would
        // let the emitter be flattened to `&[]` for every code with the whole
        // gate passing, measured.
        // `§10.9` makes a response a key **plus parameters**, and for errors the
        // published page alone would enforce that.
        assert_eq!(
            generated.params(),
            entry
                .params
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
            "{}",
            entry.code
        );
        assert!(
            severity_matches(entry.severity, generated.severity()),
            "{} declares {:?} and carries {:?}",
            entry.code,
            entry.severity,
            generated.severity()
        );
        // The area is a real prefix of the code, which is what `NC-<AREA>-<NNNN>`
        // means. Asserting that the code *contains* the area is the vacuous form:
        // every string contains the empty one, so an emitter writing `""` passes.
        assert!(!generated.area().is_empty(), "{} has no area", entry.code);
        assert!(
            generated
                .code()
                .starts_with(&format!("NC-{}-", generated.area())),
            "{} does not begin with its own area `{}`",
            generated.code(),
            generated.area()
        );
    }
    // **And a registry with only empty parameter lists would make the
    // comparison above vacuous**, so at least one code has to declare some.
    assert!(
        catalog.errors.iter().any(|entry| !entry.params.is_empty()),
        "no error declares a parameter, so the comparison proves nothing"
    );
}

#[test]
fn every_notification_and_email_carries_its_keys_and_its_params() {
    let catalog = catalog();
    assert!(
        !catalog.notifications.is_empty() && !catalog.emails.is_empty(),
        "nothing was compared"
    );
    for entry in &catalog.notifications {
        let generated = nc_catalog::Notification::ALL
            .iter()
            .find(|handle| handle.id() == entry.id)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.id));
        assert_eq!(generated.title_key(), entry.title, "{}", entry.id);
        assert_eq!(generated.body_key(), entry.body, "{}", entry.id);
        assert_eq!(generated.params(), entry.params, "{}", entry.id);
        assert!(
            severity_matches(entry.severity, generated.severity()),
            "{} declares {:?}",
            entry.id,
            entry.severity
        );
    }
    for entry in &catalog.emails {
        let generated = nc_catalog::Email::ALL
            .iter()
            .find(|handle| handle.id() == entry.id)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.id));
        assert_eq!(generated.subject_key(), entry.subject, "{}", entry.id);
        assert_eq!(generated.body_key(), entry.body, "{}", entry.id);
        assert_eq!(generated.params(), entry.params, "{}", entry.id);
    }
}

#[test]
fn every_command_carries_what_a_dispatcher_decides_on() {
    // `mutating` and `reversibility` are read by undo, by the journal and by the
    // agent allowlist. An emitter that flattened both to the safe-looking value
    // would make every command look like a read that needs no confirmation.
    let catalog = catalog();
    assert!(!catalog.commands.is_empty(), "nothing was compared");
    for entry in &catalog.commands {
        let generated = nc_catalog::COMMANDS
            .iter()
            .find(|command| command.name == entry.name)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.name));
        assert_eq!(generated.title_key, entry.title, "{}", entry.name);
        assert_eq!(
            generated.description_key, entry.description,
            "{}",
            entry.name
        );
        assert_eq!(generated.action, entry.action, "{}", entry.name);
        assert_eq!(
            generated.resource_selector, entry.resource_selector,
            "{}",
            entry.name
        );
        assert_eq!(generated.mutating, entry.mutating, "{}", entry.name);
        // **The argument schema, compared and not sniffed.** Asserting only that
        // it contains `"object"` lets an emitter writing one constant
        // `{"type":"object"}` for every command pass — and `load.rs`
        // says in its own words that a wrong schema "silently removes the
        // command's AI tool interface, which nothing else would notice".
        assert_eq!(generated.args_schema, entry.args_schema, "{}", entry.name);
        assert_eq!(
            generated.keybinding,
            entry.keybinding.as_deref(),
            "{}",
            entry.name
        );
        assert_eq!(
            format!("{:?}", generated.reversibility).to_lowercase(),
            format!("{:?}", entry.reversibility).to_lowercase(),
            "{}",
            entry.name
        );
        // **The argument names, against the schema they are derived from.**
        // `arguments` is the one emitted value the registry does not carry
        // directly — the emitter reads it out of `args_schema`'s `properties`,
        // in declaration order — and a test that reads the generated constant
        // and never the catalog compares it only against itself. So the
        // derivation is what is checked here: the same keys, in the same order.
        //
        // **From the schema's text, not through `serde_json::Map`.** An
        // expectation of `schema["properties"].as_object().keys()` is the
        // emitter's own route — and a `serde_json::Map` is a sorted `BTreeMap`
        // unless `preserve_order` is on, so both sides sort together and the
        // comparison holds whatever the order is. `cedar-policy-core`, reached
        // through `nc-policy`, turns the feature on for the whole graph; built
        // without that in it, the emitter writes `["dry_run", "table"]` against
        // a committed `["table", "dry_run"]` with every gate green, measured.
        // `Cargo.toml` declares the feature itself, and this reads the order a
        // person sees in `catalog/commands.toml` so that the declaration is
        // checked rather than assumed.
        let declared = properties_in_order(&entry.args_schema);
        assert_eq!(
            generated.arguments, declared,
            "`{}`'s arguments are not the properties its schema declares, in the order \
             the schema declares them",
            entry.name
        );
    }
    // And at least one command takes arguments, or the comparison above is
    // satisfied by two empty lists.
    assert!(
        catalog
            .commands
            .iter()
            .any(|entry| !entry.args_schema.contains("\"properties\": {}")
                && entry.args_schema.contains("properties")),
        "no command in the registry declares an argument, so the comparison above \
         compares nothing"
    );
    // And the registry has to hold more than one answer, or every assertion above
    // could be satisfied by a constant.
    let mutating: Vec<bool> = catalog.commands.iter().map(|c| c.mutating).collect();
    assert!(
        mutating.contains(&true) && mutating.contains(&false),
        "every command in the registry is {mutating:?}, so a constant would pass"
    );
    // The same floor for reversibility, which would otherwise be left to luck:
    // the commands happen to differ today, and this says they have to.
    let reversibility: std::collections::BTreeSet<String> = catalog
        .commands
        .iter()
        .map(|command| format!("{:?}", command.reversibility))
        .collect();
    assert!(
        reversibility.len() > 1,
        "every command in the registry is {reversibility:?}, so a constant would pass"
    );
}

#[test]
fn the_key_derivation_parameters_are_the_ones_the_registry_declares() {
    // **The one place where the value is the guarantee and not the shape**
    // (`NOCTUA.md §10.6`). Floors — `iterations >= 1`, `salt_length >= 16` — let
    // an emitter writing `1` where the registry says `3` pass every gate, drift
    // and determinism included, and the device PIN's cost silently falls by two
    // thirds, measured.
    let catalog = catalog();
    for (name, declared, generated) in [
        (
            "password",
            catalog.limits.argon2id.password,
            nc_catalog::ARGON2ID_PASSWORD,
        ),
        (
            "device_pin",
            catalog.limits.argon2id.device_pin,
            nc_catalog::ARGON2ID_DEVICE_PIN,
        ),
    ] {
        assert_eq!(generated.memory_kib, declared.memory_kib, "{name}");
        assert_eq!(generated.iterations, declared.iterations, "{name}");
        assert_eq!(generated.parallelism, declared.parallelism, "{name}");
        assert_eq!(generated.output_length, declared.output_length, "{name}");
        assert_eq!(generated.salt_length, declared.salt_length, "{name}");
    }
    // And the two are not the same configuration, so neither of the above can be
    // satisfied by an emitter that writes one of them twice.
    assert_ne!(
        nc_catalog::ARGON2ID_PASSWORD,
        nc_catalog::ARGON2ID_DEVICE_PIN,
        "a password and a device PIN are hashed for different threats (§10.5, §10.6)"
    );
}

#[test]
fn every_sentence_the_platform_shows_is_the_one_the_registry_declares() {
    // **The widest hole this file exists to close.** The surface test asserts
    // every key resolves, is non-empty, and that the two locales differ from
    // each other, and none of that compares message *text* to `catalog/i18n/`.
    // Without this, replacing every emitted sentence with
    // `format!("PLACEHOLDER {tag}")` passes the entire gate — drift,
    // determinism, citations — with every key reading "PLACEHOLDER en" and
    // "PLACEHOLDER pt-BR".
    //
    // `§15.2` makes the catalog the sole source of user-facing text. This is the
    // assertion that makes that true of the text and not only of the keys.
    let catalog = catalog();
    let mut compared = 0_usize;
    for locale in &catalog.locales {
        let generated = nc_catalog::Locale::ALL
            .iter()
            .find(|candidate| candidate.tag() == locale.meta.bcp47)
            .unwrap_or_else(|| panic!("`{}` never reached the code", locale.meta.bcp47));
        assert!(
            !locale.message.is_empty(),
            "{} declares nothing",
            locale.meta.bcp47
        );
        for (key, declared) in &locale.message {
            assert_eq!(
                nc_catalog::message(key, *generated),
                Some(declared.as_str()),
                "`{key}` in {}",
                locale.meta.bcp47
            );
            compared += 1;
        }
    }
    assert!(compared > 20, "only {compared} sentence(s) were compared");

    // And the key list itself, which is emitted from the same registry —
    // uncompared, truncating it to about half its keys is green, and the two
    // tests that cover the message table both *iterate* `KEYS`, so shrinking
    // it silently shrinks them.
    let declared: std::collections::BTreeSet<&str> = catalog
        .locales
        .iter()
        .flat_map(|locale| locale.message.keys().map(String::as_str))
        .collect();
    let carried: std::collections::BTreeSet<&str> = nc_catalog::KEYS.iter().copied().collect();
    assert_eq!(carried, declared, "KEYS is not the registry's key set");
}

#[test]
fn every_area_and_every_locale_reaches_the_code_as_written() {
    // Without this, emitting `AREAS` as an empty slice leaves the suite green,
    // measured — and a locale's `name()` is the endonym a language
    // picker shows, so an emitter writing the BCP 47 tag there would offer
    // "pt-BR" to somebody looking for "Português do Brasil".
    let catalog = catalog();
    assert!(!catalog.areas.is_empty(), "nothing was compared");
    assert_eq!(nc_catalog::AREAS.len(), catalog.areas.len());
    for (area, covers) in &catalog.areas {
        let found = nc_catalog::AREAS
            .iter()
            .find(|(name, _)| name == area)
            .unwrap_or_else(|| panic!("`{area}` never reached the code"));
        assert_eq!(found.1, covers, "{area}");
    }

    assert!(!catalog.locales.is_empty(), "no locale was compared");
    assert_eq!(nc_catalog::Locale::ALL.len(), catalog.locales.len());
    for locale in &catalog.locales {
        let generated = nc_catalog::Locale::ALL
            .iter()
            .find(|candidate| candidate.tag() == locale.meta.bcp47)
            .unwrap_or_else(|| panic!("`{}` never reached the code", locale.meta.bcp47));
        assert_eq!(generated.name(), locale.meta.name, "{}", locale.meta.bcp47);
        assert_eq!(generated.dir(), locale.meta.dir, "{}", locale.meta.bcp47);
        assert_ne!(
            generated.name(),
            generated.tag(),
            "the endonym is what a person picking a language reads, and this is the tag"
        );
    }
}

/// The store half of [`every_limit_carries_its_own_numbers`]: each declared
/// store against the one the compiler generated, and both counts held.
///
/// Lifted out whole — loop, counter and both equalities — when the table half
/// gained a strategy and a database and the test crossed clippy's line limit.
/// The comparison still reads `catalog.` in its own body, which is what the
/// rule in `every_generated_value_is_compared.rs` asks of a comparing chunk.
fn every_store_carries_its_own_numbers(catalog: &Catalog) {
    // **The third loop keeps a counter too**, like the table and rate-limit
    // loops of its caller, in a file whose own header names *every store bound
    // as zero* among the defects it exists to catch: without it,
    // `.iter().take(0)` here leaves the suite green, while the same narrowing
    // on either sibling fails at once.
    let mut stores = 0_usize;
    for (name, entry) in &catalog.limits.stores {
        stores += 1;
        let generated = nc_catalog::STORES
            .iter()
            .find(|store| store.name == name)
            .unwrap_or_else(|| panic!("`{name}` never reached the code"));
        assert_eq!(generated.retention_days, entry.retention_days, "{name}");
        assert_eq!(generated.max_disk_bytes, entry.max_disk_bytes, "{name}");
        assert_eq!(generated.enforced_by, entry.enforced_by, "{name}");
    }
    assert_eq!(
        stores,
        catalog.limits.stores.len(),
        "the store loop compared {stores} of {} declared store(s)",
        catalog.limits.stores.len()
    );
    assert_eq!(
        stores,
        nc_catalog::STORES.len(),
        "the registry declares {stores} store(s) and the generated crate carries {}",
        nc_catalog::STORES.len()
    );
}

#[test]
fn every_limit_carries_its_own_numbers() {
    let catalog = catalog();
    assert!(!catalog.limits.tables.is_empty(), "nothing was compared");
    // **What was compared, not what there was to compare.** The floor above
    // reads the registry's own length, so a filter *inside* either loop below
    // leaves it satisfied and the comparison covers nothing — measured against
    // that floor alone, either loop narrowed to no rows leaves the whole suite
    // green, and this test is where a generated limit is held against the file
    // it came from. Both
    // counts are held against both sides, so a row missing from either is a
    // failure rather than a smaller number.
    let mut tables = 0_usize;
    for (name, entry) in &catalog.limits.tables {
        tables += 1;
        let generated = nc_catalog::TABLES
            .iter()
            .find(|table| table.name == name)
            .unwrap_or_else(|| panic!("`{name}` never reached the code"));
        assert_eq!(generated.order_by, entry.order_by, "{name}");
        assert_eq!(generated.max_rows, entry.max_rows, "{name}");
        assert_eq!(generated.prune_batch, entry.prune_batch, "{name}");
        let same = matches!(
            (entry.strategy, generated.strategy),
            (
                ModelStrategy::OldestFirst,
                nc_catalog::Strategy::OldestFirst
            ) | (ModelStrategy::PolicyPack, nc_catalog::Strategy::PolicyPack)
                | (
                    ModelStrategy::RefuseAtLimit,
                    nc_catalog::Strategy::RefuseAtLimit
                )
        );
        assert!(same, "{name} declares {:?}", entry.strategy);
        // **And which database**, which decides which target the pruner asks.
        let database = matches!(
            (entry.database, generated.database),
            (ModelDatabase::Server, nc_catalog::Database::Server)
                | (ModelDatabase::LocalFile, nc_catalog::Database::LocalFile)
        );
        assert!(database, "{name} declares {:?}", entry.database);
    }
    every_store_carries_its_own_numbers(&catalog);
    assert!(
        !catalog.limits.rate_limits.is_empty(),
        "no rate limit was compared"
    );
    assert_eq!(
        tables,
        catalog.limits.tables.len(),
        "the table loop compared {tables} of {} declared table(s)",
        catalog.limits.tables.len()
    );
    assert_eq!(
        tables,
        nc_catalog::TABLES.len(),
        "the registry declares {tables} table(s) and the generated crate carries {}",
        nc_catalog::TABLES.len()
    );
    let mut surfaces = 0_usize;
    for entry in &catalog.limits.rate_limits {
        surfaces += 1;
        let surface = &entry.surface;
        let generated = nc_catalog::RATE_LIMITS
            .iter()
            .find(|limit| limit.surface == surface)
            .unwrap_or_else(|| panic!("`{surface}` never reached the code"));
        for (declared, carried) in [
            (&entry.per_identifier, &generated.per_identifier),
            (&entry.per_address, &generated.per_address),
        ] {
            assert_eq!(carried.window_seconds, declared.window_seconds, "{surface}");
            assert_eq!(carried.max_attempts, declared.max_attempts, "{surface}");
            assert_eq!(carried.delay_ms, declared.delay_ms.as_slice(), "{surface}");
            assert_eq!(carried.lockout_after, declared.lockout_after, "{surface}");
            assert_eq!(carried.cumulative, declared.cumulative, "{surface}");
            // **`on_lockout`, compared like every field above it.** Without
            // this the dangerous direction is the one that passes: emitting
            // `Some(OnLockout::WipeCredential)` for every budget is green,
            // measured, and writes it into every budget the registry declares as
            // `None` — so
            // exhausting the activation-code or per-address budget would destroy
            // a user's stored device credential.
            assert_eq!(
                format!("{:?}", carried.on_lockout),
                format!("{:?}", declared.on_lockout),
                "{surface}"
            );
        }
    }
    assert_eq!(
        surfaces,
        catalog.limits.rate_limits.len(),
        "the rate-limit loop compared {surfaces} of {} declared surface(s)",
        catalog.limits.rate_limits.len()
    );
    assert_eq!(
        surfaces,
        nc_catalog::RATE_LIMITS.len(),
        "the registry declares {surfaces} surface(s) and the generated crate carries {}",
        nc_catalog::RATE_LIMITS.len()
    );
}

#[test]
fn every_capability_carries_its_interface_its_functions_and_its_sentence() {
    // **A manifest is read against these, and a host function names one.** An
    // emitter that carried a function under the wrong capability, or dropped
    // one, would grant a plugin what its manifest did not ask for — and the
    // kernel's coverage test reads `CAPABILITIES`, so it would agree with the
    // emitter and not with `catalog/capabilities.toml`. This holds the second.
    let catalog = catalog();
    let mut compared = 0_usize;
    for entry in &catalog.capabilities {
        let generated = nc_catalog::capability(&entry.name)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.name));
        assert_eq!(generated.name, entry.name);
        assert_eq!(generated.interface, entry.interface, "{}", entry.name);
        assert_eq!(
            generated.functions,
            entry.functions.as_slice(),
            "{}",
            entry.name
        );
        assert_eq!(
            generated.description_key, entry.description,
            "{}",
            entry.name
        );
        // **And the variant is the one whose spec this is**, both ways: a host
        // function asks for `Capability::X`, and `X.spec()` is what it is
        // refused by — so one variant is this capability's and no other's.
        let variant = generated.capability;
        assert_eq!(
            variant.spec(),
            generated,
            "`{}`'s variant answers another capability's spec",
            entry.name
        );
        assert_eq!(
            nc_catalog::CAPABILITIES
                .iter()
                .filter(|spec| spec.capability == variant)
                .count(),
            1,
            "`{}`'s variant is another capability's too",
            entry.name
        );
        compared += 1;
    }
    assert_eq!(
        compared,
        nc_catalog::CAPABILITIES.len(),
        "the registry declares {compared} capabilities and the generated crate carries {}",
        nc_catalog::CAPABILITIES.len()
    );
    assert!(compared > 1, "{compared} capability(ies) were compared");
    let variants: std::collections::BTreeSet<nc_catalog::Capability> = nc_catalog::CAPABILITIES
        .iter()
        .map(|spec| spec.capability)
        .collect();
    assert_eq!(
        variants.len(),
        compared,
        "two capabilities share one variant, so one of them cannot be asked for"
    );

    let mut given = 0_usize;
    for entry in &catalog.ungated {
        let generated = nc_catalog::UNGATED
            .iter()
            .find(|ungated| ungated.interface == entry.interface)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.interface));
        assert_eq!(
            generated.functions,
            entry.functions.as_slice(),
            "{}",
            entry.interface
        );
        assert_eq!(generated.reason, entry.reason.trim(), "{}", entry.interface);
        given += 1;
    }
    assert_eq!(given, nc_catalog::UNGATED.len());
    assert!(given > 0, "no interface given to every plugin was compared");
}

#[test]
fn the_kernels_limits_are_the_ones_the_registry_declares() {
    // **Written out field by field, like the key-derivation costs above.** The
    // kernel holds every plugin to `KERNEL` and to nothing else, and the type
    // cannot be built outside the generated crate — so an emitter writing one
    // limit's number under another's name would be the only source of the
    // wrong bound, with no second one to disagree with it.
    let catalog = catalog();
    let declared = catalog.limits.kernel;
    let generated = nc_catalog::KERNEL;
    assert_eq!(generated.memory_bytes, declared.memory_bytes);
    assert_eq!(generated.instances, declared.instances);
    assert_eq!(generated.tables, declared.tables);
    assert_eq!(generated.table_elements, declared.table_elements);
    assert_eq!(generated.wasm_stack_bytes, declared.wasm_stack_bytes);
    assert_eq!(generated.fuel_per_dispatch, declared.fuel_per_dispatch);
    assert_eq!(generated.fuel_per_activate, declared.fuel_per_activate);
    assert_eq!(generated.epoch_tick_ms, declared.epoch_tick_ms);
    assert_eq!(
        generated.epoch_deadline_ticks,
        declared.epoch_deadline_ticks
    );
    assert_eq!(
        generated.host_call_deadline_ms,
        declared.host_call_deadline_ms
    );
    assert_eq!(
        generated.dispatch_deadline_ms,
        declared.dispatch_deadline_ms
    );
    assert_eq!(generated.dispatch_depth, declared.dispatch_depth);
    assert_eq!(generated.dispatches_per_tree, declared.dispatches_per_tree);
    assert_eq!(generated.restart_budget, declared.restart_budget);
    assert_eq!(generated.restart_window_ms, declared.restart_window_ms);
    assert_eq!(generated.payload_bytes, declared.payload_bytes);
    assert_eq!(generated.commands_per_plugin, declared.commands_per_plugin);
    assert_eq!(generated.store_key_bytes, declared.store_key_bytes);
    assert_eq!(generated.store_value_bytes, declared.store_value_bytes);
    assert_eq!(generated.unit_bytes, declared.unit_bytes);
    assert_eq!(generated.store_list_max, declared.store_list_max);
    assert_eq!(generated.blob_bytes, declared.blob_bytes);
    assert_eq!(
        generated.blob_read_chunk_bytes,
        declared.blob_read_chunk_bytes
    );
    assert_eq!(
        generated.blob_orphan_grace_ms,
        declared.blob_orphan_grace_ms
    );
    assert_eq!(generated.blob_handles, declared.blob_handles);
    assert_eq!(generated.unit_handles, declared.unit_handles);
    assert_eq!(generated.net_timeout_ms, declared.net_timeout_ms);
    assert_eq!(
        generated.net_request_head_bytes,
        declared.net_request_head_bytes
    );
    assert_eq!(
        generated.net_request_body_bytes,
        declared.net_request_body_bytes
    );
    assert_eq!(
        generated.net_response_body_bytes,
        declared.net_response_body_bytes
    );
    assert_eq!(generated.net_header_count, declared.net_header_count);
    assert_eq!(generated.timers_per_plugin, declared.timers_per_plugin);
    assert_eq!(
        generated.timer_min_interval_ms,
        declared.timer_min_interval_ms
    );
    assert_eq!(generated.timer_max_delay_ms, declared.timer_max_delay_ms);
    assert_eq!(generated.event_queue, declared.event_queue);
    assert_eq!(generated.event_queue_bytes, declared.event_queue_bytes);
    assert_eq!(generated.surfaces_per_plugin, declared.surfaces_per_plugin);
    assert_eq!(generated.ui_tree_nodes, declared.ui_tree_nodes);
    assert_eq!(generated.ui_tree_depth, declared.ui_tree_depth);
    assert_eq!(generated.ui_text_bytes, declared.ui_text_bytes);
    assert_eq!(generated.log_ring_lines, declared.log_ring_lines);
    assert_eq!(generated.log_line_bytes, declared.log_line_bytes);
    assert_eq!(generated.undo_history, declared.undo_history);
    assert_eq!(generated.confirmation_ttl_ms, declared.confirmation_ttl_ms);
    // **And every one of them is a row the compiler hands the emitter**, so a
    // limit this test reads and `KernelLimits::each` dropped is not a limit
    // written out above and never published.
    assert_eq!(
        declared.each().len(),
        44,
        "a kernel limit was added or lost"
    );
}

#[test]
fn the_daemons_limits_are_the_ones_the_registry_declares() {
    // **Field by field, as the kernel's are**, for the same reason: the daemon
    // reads `DAEMON` and nothing else, and the type cannot be built outside the
    // generated crate.
    let catalog = catalog();
    let declared = catalog.limits.daemon;
    let generated = nc_catalog::DAEMON;
    assert_eq!(generated.mark_lease_ms, declared.mark_lease_ms);
    assert_eq!(declared.each().len(), 1, "a daemon limit was added or lost");
}

/// What each field kind must become in the generated struct.
///
/// **Written out here, not asked of the compiler.** The obvious version calls
/// `FieldType::rust_type` — and then the test compares the emitter's output
/// against the same function the emitter used, so swapping two kinds' types
/// leaves both sides agreeing. `NOCTUA.md §19.3`'s rule is that a check must not
/// inspect its own fresh output, and a mapping consulted from the thing under
/// test is that in miniature.
///
/// Every one of these implements `LogValue`, and `String` is not among them —
/// which is the whole wall (`§15.4`). Changing a row here is changing what the
/// log can carry, and it should take a deliberate edit in two places.
const fn rust_type(kind: FieldType) -> &'static str {
    match kind {
        FieldType::U64 => "u64",
        FieldType::I64 => "i64",
        FieldType::U32 => "u32",
        FieldType::Bool => "bool",
        FieldType::Duration => "core::time::Duration",
        FieldType::ErrorCode => "nc_catalog::ErrorCode",
        FieldType::Id => "nc_ids::OpaqueId",
        FieldType::Snowflake => "nc_ids::Snowflake",
        FieldType::Literal => "crate::Literal",
        FieldType::Enum => "crate::Variant",
    }
}

#[test]
fn every_typed_event_struct_says_what_the_registry_declared() {
    // **The descriptors and the structs are two emissions from one registry, and
    // both are compared.** `nc_catalog::EVENTS` is data; the structs
    // in `crates/nc-log/src/events.rs` are what a service actually constructs and
    // what reaches the central log (`§15.4`) — and drift proves only that the
    // emitter agrees with itself.
    //
    // Without this, five mutations pass the whole gate at once: every `level()`
    // returning `Info` (so `journal.broken` and `ids.clock_backwards`, declared
    // `error`, record `info`), every `name()` one constant, every field key `"field"`,
    // the `optional` branch dropped, and `error_code` and `id` swapped to each
    // other's Rust type.
    //
    // Read as text, the way `the_emitter_carries_values…` reads `commands.rs`:
    // constructing one here would need `nc-log` as a dependency of `xtask`, and
    // the emitted source is the artifact under test.
    let catalog = catalog();
    let emitted = std::fs::read_to_string(root().join("crates/nc-log/src/events.rs"))
        .expect("the typed events are generated and committed");
    assert!(!catalog.events.is_empty(), "nothing was compared");

    let mut rust_types = std::collections::BTreeSet::new();
    for entry in &catalog.events {
        let name = nc_catalog_compiler::camel(&entry.name);
        let start = emitted
            .find(&format!("pub struct {name} {{"))
            .unwrap_or_else(|| panic!("`{}` has no struct in the emitted source", entry.name));
        // Its struct, then its `impl Event` — everything up to the next struct.
        let block = &emitted[start..];
        let block = block
            .find("\npub struct ")
            .map_or(block, |next| &block[..next]);

        assert!(
            block.contains(&format!("        {:?}\n", entry.name)),
            "`{}` does not return its own name",
            entry.name
        );
        assert!(
            block.contains(&format!("Level::{:?}", entry.level)),
            "`{}` is declared {:?} and its struct does not say so",
            entry.name,
            entry.level
        );
        for field in &entry.fields {
            let rust = rust_type(field.kind);
            rust_types.insert(rust);
            let declared = if field.optional {
                format!("Option<{rust}>")
            } else {
                rust.to_owned()
            };
            assert!(
                block.contains(&format!(
                    "pub {}: {declared},",
                    nc_catalog_compiler::snake(&field.name)
                )),
                "`{}`'s `{}` should be `{declared}`",
                entry.name,
                field.name
            );
            assert!(
                block.contains(&format!(
                    "({:?}, self.{}",
                    field.name,
                    nc_catalog_compiler::snake(&field.name)
                )),
                "`{}` does not carry `{}` under its own key",
                entry.name,
                field.name
            );
        }
    }

    // The coverage floor: with one Rust type across the registry, every
    // assertion above is satisfiable by a constant.
    assert!(
        rust_types.len() >= 3,
        "the registry uses only {rust_types:?}, so a constant type would pass"
    );
    // And an optional field has to exist, or the `Option<…>` half is never tried.
    assert!(
        catalog
            .events
            .iter()
            .flat_map(|event| event.fields.iter())
            .any(|field| field.optional),
        "no field is optional, so the `Option<…>` branch is never exercised"
    );
}

#[test]
fn every_log_field_carries_the_kind_the_registry_gave_it() {
    // The type wall from the data's side. Without it, flattening every kind to
    // `U64` passes everything else, measured, and it would also satisfy a test
    // that only checked the kinds were among the ones that exist.
    let catalog = catalog();
    assert!(!catalog.events.is_empty(), "nothing was compared");
    let mut kinds = std::collections::BTreeSet::new();
    for entry in &catalog.events {
        let generated = nc_catalog::EVENTS
            .iter()
            .find(|event| event.name == entry.name)
            .unwrap_or_else(|| panic!("`{}` never reached the code", entry.name));
        assert_eq!(generated.description, entry.description, "{}", entry.name);
        assert_eq!(
            format!("{:?}", generated.level).to_lowercase(),
            format!("{:?}", entry.level).to_lowercase(),
            "{}",
            entry.name
        );
        assert_eq!(
            generated.fields.len(),
            entry.fields.len(),
            "{} lost a field on the way",
            entry.name
        );
        for (declared, carried) in entry.fields.iter().zip(generated.fields) {
            assert_eq!(carried.name, declared.name, "{}", entry.name);
            assert_eq!(carried.optional, declared.optional, "{}", entry.name);
            let same = matches!(
                (declared.kind, carried.kind),
                (FieldType::U64, nc_catalog::FieldKind::U64)
                    | (FieldType::I64, nc_catalog::FieldKind::I64)
                    | (FieldType::U32, nc_catalog::FieldKind::U32)
                    | (FieldType::Bool, nc_catalog::FieldKind::Bool)
                    | (FieldType::Duration, nc_catalog::FieldKind::Duration)
                    | (FieldType::ErrorCode, nc_catalog::FieldKind::ErrorCode)
                    | (FieldType::Id, nc_catalog::FieldKind::Id)
                    | (FieldType::Snowflake, nc_catalog::FieldKind::Snowflake)
                    | (FieldType::Literal, nc_catalog::FieldKind::Literal)
                    | (FieldType::Enum, nc_catalog::FieldKind::Enum)
            );
            assert!(
                same,
                "{}'s `{}` declares {:?} and carries {:?}",
                entry.name, declared.name, declared.kind, carried.kind
            );
            kinds.insert(format!("{:?}", carried.kind));
        }
    }
    // The coverage floor. Every assertion above holds trivially if the registry
    // only ever uses one kind, so the registry itself has to exercise the wall.
    assert!(
        kinds.len() >= 3,
        "the registry uses only {kinds:?}, so a constant kind would pass every \
         assertion here"
    );
}

/// What a field's doc comment says it holds.
///
/// **Written out, not asked of `FieldType::label`.** Flattening that function to
/// a constant `"count"` made every field in the generated `nc-log` source read
/// `/// A count.` with the whole gate green — including a `DurationMs` and an
/// `ErrorCode`. A test that consulted `label()` would have agreed with it.
const fn label(kind: FieldType) -> &'static str {
    match kind {
        FieldType::U64 => "count",
        FieldType::I64 => "signed number",
        // **Not "small count".** The one `u32` this registry declares is an
        // HTTP status, which is not a count of anything — and this label is
        // the field's doc comment in the generated crate as well as its cell
        // on the page.
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

/// The generated event structs are in the order the emitter sorts them into.
///
/// **The one generated artefact whose order nothing else pins.** Every `.md`
/// page has `every_page_lists_its_entries_in_order`, and `nc-catalog`'s own
/// events module has a doc line claiming an order and a test holding it; this
/// file has neither, and every reader finds a struct with
/// `find("pub struct {name} {")`, which is order-blind. Measured: the emitter's
/// `sort_by` deleted, the structs come out in registry file order, and
/// `check passed` without this test.
///
/// It costs a reader rather than a caller — which is why it is a small finding
/// and still a finding: the tree decided this property was worth holding and
/// held it everywhere else.
#[test]
fn the_generated_events_are_in_the_order_they_are_sorted_into() {
    let catalog = catalog();
    let source = std::fs::read_to_string(root().join("crates/nc-log/src/events.rs"))
        .expect("the generated events module is on disk");
    let mut names: Vec<String> = catalog
        .events
        .iter()
        .map(|entry| nc_catalog_compiler::camel(&entry.name))
        .collect();
    assert!(names.len() > 5, "{} event(s) declared", names.len());
    let mut sorted: Vec<&String> = names.iter().collect();
    sorted.sort_by(|one, other| one.as_str().cmp(other.as_str()));
    // The emitter sorts by the registry's dotted name; the camel names are in
    // the same order because the mapping keeps it, and asserting both ways is
    // what makes this a comparison rather than a restatement.
    names.sort_by(|one, other| one.as_str().cmp(other.as_str()));
    let mut at = 0_usize;
    for name in &names {
        let declares = format!("pub struct {name} {{");
        let found = source[at..]
            .find(&declares)
            .unwrap_or_else(|| panic!("`{name}` is declared after the one before it"));
        at += found + declares.len();
    }
}

#[test]
fn every_generated_field_doc_says_what_that_kind_holds() {
    let catalog = catalog();
    let source = std::fs::read_to_string(root().join("crates/nc-log/src/events.rs"))
        .expect("the generated events module is on disk");
    let mut labels = std::collections::BTreeSet::new();
    let mut checked = 0_usize;
    for entry in &catalog.events {
        for field in &entry.fields {
            // "An" before a vowel, which the emitter derives from the label's
            // own first letter — a fixed "A" produces "A opaque identifier" in
            // a file nobody may edit.
            let words = label(field.kind);
            let article = if words.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "An"
            } else {
                "A"
            };
            let expected = format!("/// {article} {words}.");
            // **Above this field, not somewhere in the file.** `contains` over
            // the whole source is satisfied by a permutation, and the mutation
            // this test's own header records — flattening every label to one
            // word — moves the *set*, which is why a set catches it and misses a
            // rotation. Measured: every field documented with the next field's
            // label, so a `Literal` reads "A count." and a `Duration` reads "A
            // fixed text chosen when the code was written" — the generated
            // crate, whose header says do not edit, telling a developer that a
            // duration field is the safe place to put a literal.
            let name = nc_catalog_compiler::camel(&entry.name);
            let declares = format!("pub struct {name} {{");
            let opens = source
                .find(&declares)
                .unwrap_or_else(|| panic!("`{name}` is declared"));
            let block = &source[opens..];
            let ends = block.find("\n}").unwrap_or(block.len());
            let holds = format!("pub {}:", field.name);
            let field_at = block[..ends]
                .find(&holds)
                .unwrap_or_else(|| panic!("`{name}` declares `{}`", field.name));
            let above: Vec<&str> = block[..field_at]
                .lines()
                .rev()
                .skip_while(|line| line.trim().is_empty())
                .take_while(|line| line.trim_start().starts_with("///"))
                .collect();
            // **The first line, not somewhere in the block.** rustdoc publishes
            // the summary — the block's opening line — and `any` accepts the
            // true sentence anywhere under a false one, which is why the three
            // loops beside this one take the first line too. Measured with `any`
            // here: a second summary written above the true one (with a blank
            // `///` between, which walks past the fused-summary rule) makes
            // every field of the generated log-event surface publish
            // *"A count."* — a `Duration`, an `ErrorCode` and an `Id` alike —
            // with `check passed`.
            //
            // `above` is collected from the bottom up, so the summary is its
            // last element.
            let summary = above.last().map(|line| line.trim()).unwrap_or_default();
            assert_eq!(
                summary, expected,
                "`{}`.`{}` is {:?} and the line rustdoc publishes for it does not read \
                 `{expected}`",
                entry.name, field.name, field.kind
            );
            labels.insert(label(field.kind));
            checked += 1;
        }
    }
    assert!(checked >= 20, "only {checked} field(s) were compared");
    assert!(
        labels.len() >= 3,
        "the registry uses only {labels:?}, so a constant label would pass"
    );
}

#[test]
fn every_generated_module_names_the_registry_it_came_from() {
    // The `Source:` line is what tells a reader which registry to edit. Setting
    // every one of them to the bare `catalog/` directory leaves the rest of the
    // gate green: `header()` is tested in isolation and nothing else asserts
    // which file names which source.
    for (module, source) in [
        ("errors.rs", "catalog/errors.toml"),
        ("notifications.rs", "catalog/notifications.toml"),
        ("emails.rs", "catalog/emails.toml"),
        ("limits.rs", "catalog/limits.toml"),
        ("commands.rs", "catalog/commands.toml"),
        ("events.rs", "catalog/log-events.toml"),
        ("i18n.rs", "catalog/i18n/"),
        ("capabilities.rs", "catalog/capabilities.toml"),
    ] {
        let path = root().join("crates/nc-catalog/src").join(module);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{module} is not readable: {error}"));
        assert!(
            text.contains(&format!("//! Source: {source}\n")),
            "{module} does not name {source} as its source"
        );
        assert!(
            root().join(source.trim_end_matches('/')).exists(),
            "{module} names {source}, which does not exist"
        );
    }
}

/// What `impl Event` requires, which is not declared per event and is the same
/// three for all of them.
const FROM_THE_TRAIT: [&str; 3] = ["name", "level", "fields"];

#[test]
fn no_event_struct_holds_a_member_the_registry_does_not_declare() {
    // **The other direction, for the one file a whole-file excuse covers.**
    // `every_event_is_a_struct_with_the_fields_it_declares` walks the registry
    // and asserts *declared ⊆ emitted*; without *emitted ⊆ declared*, a value
    // the compiler writes that no registry asked for is compared by nobody.
    // Measured: `pub const fn shard(&self) -> u8 { 7 }` emitted onto every
    // event struct, `check passed` — a fabricated value on every generated log
    // event.
    //
    // `every_generated_value_is_compared.rs` excuses this whole file through
    // `BY_ITERATION`, and an excuse for a whole file has to cover the whole
    // file. This is the half that makes that true.
    let catalog = catalog();
    let emitted = std::fs::read_to_string(root().join("crates/nc-log/src/events.rs"))
        .expect("the typed events are generated and committed");
    assert!(!catalog.events.is_empty(), "nothing was compared");

    let mut undeclared = Vec::new();
    for entry in &catalog.events {
        let name = nc_catalog_compiler::camel(&entry.name);
        let start = emitted
            .find(&format!("pub struct {name} {{"))
            .unwrap_or_else(|| panic!("`{}` has no struct in the emitted source", entry.name));
        let block = &emitted[start..];
        let block = block
            .find("\npub struct ")
            .map_or(block, |next| &block[..next]);

        let declared: Vec<String> = entry
            .fields
            .iter()
            .map(|field| nc_catalog_compiler::snake(&field.name))
            .collect();
        for line in block.lines().map(str::trim) {
            let member = line
                .strip_prefix("pub ")
                .and_then(|rest| rest.split(':').next().map(str::trim))
                .filter(|_| line.contains(':'))
                .or_else(|| {
                    line.strip_prefix("pub const fn ")
                        .or_else(|| line.strip_prefix("pub fn "))
                        .and_then(|rest| rest.split('(').next())
                })
                .or_else(|| {
                    line.strip_prefix("fn ")
                        .and_then(|rest| rest.split('(').next())
                });
            let Some(member) = member else { continue };
            if declared.iter().any(|field| field == member) || FROM_THE_TRAIT.contains(&member) {
                continue;
            }
            undeclared.push(format!("{}::{member}", entry.name));
        }
    }
    assert!(
        undeclared.is_empty(),
        "the emitter writes these onto event structs and no registry declares them, so \
         each could be emitted wrong with the whole gate green: {undeclared:#?}"
    );
}

#[test]
fn the_code_holds_nothing_the_registry_does_not_declare() {
    // **The other direction.** The registry-to-code tests in this file walk the
    // registry and ask "is it in the code"; this asks "is everything in the code
    // in the registry". Measured without it: an emitter that appends
    // one `Command` and one `RateLimit` that no registry declares publishes
    //
    //     Command { name: "audit.disable", action: "Read", mutating: false, … }
    //     RateLimit { surface: "admin_backdoor", max_attempts: 9999, … }
    //
    // with `cargo xtask check` green, `docs/commands.md` clean, and `build` still
    // reporting the declared count. A command is the authorization surface and
    // the AI tool surface (`§11.4`: commands are the only way to mutate state),
    // and an undeclared rate-limit surface is a `§15.5` hole. The pages have
    // their twin (`every_section_on_every_page_is_an_entry_the_registry_declares`),
    // and this is the generated Rust's.
    //
    // A length comparison rather than a set difference, because every one of
    // these already has a per-entry test walking the other way: the two together
    // are a bijection, and either alone is not.
    let catalog = catalog();
    let counted: [(&str, usize, usize); 7] = [
        (
            "commands",
            nc_catalog::COMMANDS.len(),
            catalog.commands.len(),
        ),
        (
            "error codes",
            nc_catalog::ErrorCode::ALL.len(),
            catalog.errors.len(),
        ),
        (
            "notifications",
            nc_catalog::Notification::ALL.len(),
            catalog.notifications.len(),
        ),
        ("emails", nc_catalog::Email::ALL.len(), catalog.emails.len()),
        (
            "bounded tables",
            nc_catalog::TABLES.len(),
            catalog.limits.tables.len(),
        ),
        (
            "stores",
            nc_catalog::STORES.len(),
            catalog.limits.stores.len(),
        ),
        (
            "rate limits",
            nc_catalog::RATE_LIMITS.len(),
            catalog.limits.rate_limits.len(),
        ),
    ];
    for (what, in_code, declared) in counted {
        assert_eq!(
            in_code, declared,
            "the generated code holds {in_code} {what} and the registry declares {declared}"
        );
        assert!(declared > 0, "no {what} were compared");
    }
}

/// The severity scale compares in the direction it reads in.
///
/// **A derived `Ord` is the declaration order.** `Severity` derives `Ord`, so
/// declared as `Critical, Error, Warning, Info` it runs backwards:
/// `Severity::Critical > Severity::Info` is `false`, `.max()` over every code
/// answers `Info`, and the obvious paging filter
///
/// ```text
/// codes.filter(|code| code.severity() >= Severity::Error)
/// ```
///
/// keeps `IdsNameTooLong` and drops `JournalChainBroken` — the code whose
/// whole meaning is that the audit chain has been broken. In a tree where no
/// other code compares two severities, nothing would be red; this is public
/// generated API, and the first comparison written against it would be wrong
/// with no way to tell.
///
/// The filter below is the one that failure was measured with, so this test
/// fails the moment the order is reversed.
#[test]
fn the_severity_scale_runs_upwards() {
    use nc_catalog::{ErrorCode, Severity};

    assert!(Severity::Critical > Severity::Error);
    assert!(Severity::Error > Severity::Warning);
    assert!(Severity::Warning > Severity::Info);
    assert_eq!(
        [Severity::Critical, Severity::Info, Severity::Error]
            .into_iter()
            .max(),
        Some(Severity::Critical),
        "the most severe of a set is not the one this scale calls greatest"
    );

    let serious: Vec<ErrorCode> = ErrorCode::ALL
        .iter()
        .copied()
        .filter(|code| code.severity() >= Severity::Error)
        .collect();
    assert!(
        serious.contains(&ErrorCode::JournalChainBroken),
        "a filter for `>= Error` excluded the code that means the chain is broken"
    );
    // The floor: a scale where everything compares greater than everything would
    // satisfy the line above by keeping every code.
    assert!(
        serious.len() < ErrorCode::ALL.len(),
        "`>= Error` kept every one of the {} codes, so it filtered nothing",
        ErrorCode::ALL.len()
    );
}

/// The compiler and the renderer decide "is this a placeholder" the same way.
///
/// **Two rules for one question disagree unless something holds them to one.**
/// `messages.rs`'s `placeholders` accepts `[A-Za-z0-9_]+` between the braces and
/// calls everything else prose — its own test blesses `{"type": "object"}`, `{}`
/// and `{ spaced }` — and an emitted `fill` that takes whatever sits between `{`
/// and the next `}` as a name makes the compiler validate a sentence the
/// renderer refuses the whole of, measured:
///
/// ```text
/// message = Some("… such as {\"table\": \"journal_entries\"}.")
/// render  = Some(Err("\"table\": \"journal_entries\""))
/// ```
///
/// with `cargo xtask check` green, because no catalog sentence holds a brace
/// today and nothing else compares the two rules.
///
/// The two live in different crates — one is a private function of the
/// compiler, the other a private function of what the compiler writes — and the
/// shipped catalog cannot drive them: a sentence with prose braces there would
/// be a product string bent to suit a test. So this drives both over a corpus
/// of its own.
#[test]
fn the_two_placeholder_rules_are_one_rule() {
    // **Behaviour, not source text.** An assertion that the two files hold the
    // same predicate *expression* survives deleting the branch that calls it:
    // measured, the prose-brace arm replaced with `return Err(name)` leaves
    // `check passed` against such an assertion, with `is_a_placeholder` still
    // defined and still called and only its result acted on differently. A text
    // comparison cannot see that, and the shipped catalog cannot either: no
    // sentence in it holds a prose brace, and
    // `every_declared_key_renders_with_nothing_left_in_braces` forbids one from
    // being added.
    //
    // So the corpus is here, and it drives the *emitted* renderer against the
    // compiler's own scanner. Each row is a sentence and what a reader must be
    // shown; where the two disagree, one of them validates a sentence the other
    // refuses.
    const SENTENCES: [&str; 18] = [
        // Prose, all of it — the compiler's own test blesses these three.
        // Non-ASCII, which is where the two predicates came apart: the compiler
        // asks `is_ascii_alphanumeric` and a renderer asking `is_alphanumeric`
        // calls these placeholders and refuses the sentence.
        "a name in Portuguese: {ação}",
        "a name in Japanese: {名前}",
        // A dotted path, which neither side accepts and both must call prose.
        "a path: {user.name}",
        // **Names with `_` and a digit**, which is where the two predicates can
        // disagree.
        "{user_id} was here",
        "{arg1} and {arg2}",
        "such as {\"type\": \"object\"}.",
        "an empty pair {} in a sentence",
        "{ spaced } is not a name",
        // The Handlebars escaping a translator reaches for. Prose to the
        // compiler, and `Err(\"name\")` to a renderer whose loop disagrees.
        "A translator wrote {{name}} here.",
        "{{}}",
        // A nested opener, and a stray closer.
        "{a{b}",
        "{name}}",
        "} with no opener",
        // An unmatched `{` is text: no locale can be trusted never to hold one.
        "{name} and { unmatched",
        // The ordinary cases, at both ends of the string.
        "{name} at the start",
        "and at the end: {name}",
        "{a}{b} adjacent",
        // The one that made the two arms of the loop below contradict each
        // other: `a` is declared, `{{a}}` is prose, and a substring search for
        // the first finds the second.
        "{a} and {{a}}",
    ];
    let mut wrong = Vec::new();
    for sentence in SENTENCES {
        // Everything the compiler declares, supplied — so a renderer that agrees
        // about what a placeholder is has nothing left to refuse.
        let declared = placeholders_the_compiler_sees(sentence);
        let values: Vec<(&str, &str)> = declared.iter().map(|name| (name.as_str(), "X")).collect();
        match render_through_the_emitted_code(sentence, &values) {
            Ok(shown) => {
                // Every brace pair the compiler called prose survives verbatim.
                // **Counted, not `contains`.** A sentence holding two prose pairs
                // passes `contains` when the renderer writes one of them:
                // `contains` answers "somewhere", and what is being checked is
                // that each survives.
                // **And every name the compiler *declared* is substituted.**
                // Checking only that prose survives leaves a renderer *stricter*
                // than the compiler — one that declares a placeholder and then
                // writes it through as prose — invisible. Measured:
                // the emitted predicate narrowed to `is_ascii_alphabetic`, and
                // `{user_id}` and `{arg1}` came back with their braces in them,
                // `check passed`. That is the half-rendered state `§9.9` forbids,
                // and the corpus could not see it because no sentence in it or in
                // the catalog holds a name with `_` or a digit.
                // **Read with the scanner, not searched for as a substring.**
                // `shown.contains("{a}")` is true of the string `{{a}}`, which
                // the compiler reads as prose and the renderer is *right* to
                // write through — so with a substring search the sentence
                // `"{a} and {{a}}"` reports the renderer for doing exactly what the
                // other arm of this loop requires of it, and the two arms
                // contradict each other on one input. Counting rather than
                // `contains` does not fix it: the
                // prose pair holds an occurrence of the name's brace-form too.
                // What answers is asking the same question of the output that was
                // asked of the input.
                for name in placeholders_the_compiler_sees(&shown) {
                    if declared.contains(&name) {
                        wrong.push(format!(
                            "  {sentence:?} — the compiler declares `{name}` and the \
                             renderer wrote it through with its braces on: {shown:?}"
                        ));
                    }
                }
                for prose in prose_pairs(sentence) {
                    if shown.matches(prose.as_str()).count()
                        != sentence.matches(prose.as_str()).count()
                    {
                        wrong.push(format!(
                            "  {sentence:?} — the compiler reads `{prose}` as prose and the \
                             renderer did not write it through: {shown:?}"
                        ));
                    }
                }
            }
            Err(refused) => wrong.push(format!(
                "  {sentence:?} — the compiler declares {declared:?} and the renderer \
                 still wanted `{refused}`, so one of them validates what the other refuses"
            )),
        }
    }
    assert!(
        wrong.is_empty(),
        "the compiler's placeholder scan and the emitted renderer disagree:\n{}",
        wrong.join("\n")
    );
}

/// The placeholders `nc-catalog-compiler` declares a sentence to have.
///
/// **The real scanner, not a copy of it.** A copy, justified on the grounds that
/// the function is private, is the identical wiring defect this test's own
/// header records on the renderer side. Measured against a copy: the compiler's
/// predicate widened to accept `.`, so it declares `{user.name}` a placeholder
/// while the renderer calls it prose, and `check passed` with the corpus
/// holding a row for that very sentence.
fn placeholders_the_compiler_sees(sentence: &str) -> Vec<String> {
    nc_catalog_compiler::messages::placeholders_for_tests(sentence)
}

/// Every brace pair the compiler reads as prose, spelled as it appears.
///
/// Derived from the scanner above rather than restating its predicate, for the
/// same reason.
fn prose_pairs(sentence: &str) -> Vec<String> {
    let declared = placeholders_the_compiler_sees(sentence);
    let mut found = Vec::new();
    let mut rest = sentence;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        if !declared.iter().any(|declared| declared == name) {
            found.push(format!("{{{name}}}"));
        }
        rest = &after[close + 1..];
    }
    found
}

/// One sentence through the renderer the compiler emitted.
///
/// **The real one, not a hand-written mirror of `fill`.** A mirror guarded by
/// `contains` assertions over the generated file misses a divergence that
/// leaves those substrings intact. Measured against a mirror: the emitted
/// `fill` changed to refuse an unmatched `{`, which `RENDER`'s own doc forbids,
/// and `cargo xtask check` passed with this corpus green; the identical edit
/// applied to the mirror made the corpus fail at once. The corpus is right;
/// what has to be right too is the wiring.
fn render_through_the_emitted_code(
    sentence: &str,
    values: &[(&str, &str)],
) -> Result<String, String> {
    nc_catalog::i18n::fill_for_tests(sentence, values).map_err(std::borrow::ToOwned::to_owned)
}

/// A wrapped attribute is one attribute, and a stray `]` is not one at all.
///
/// **The case none of the three copies' trees holds.** Without it, the
/// wrapped-attribute branch can be neutered in all three copies with every
/// suite green — `nc-journal` has no multi-line attribute in it, and neither
/// does the tree this copy reads, so the branch decides nothing anywhere else. The bound is
/// what stops a stray `]` above an item walking the reader out of the block it
/// is reading, and it is written down here too.
#[test]
fn a_doc_block_is_found_past_a_wrapped_attribute() {
    fn lines(text: &str) -> Vec<&str> {
        text.lines().collect()
    }
    // **Assembled, because this file is walked.** A literal `#[` inside a
    // string is an attribute to the gate that reads this tree for a blanket
    // allow, and one that never closes stops it reading the file.
    let opener = ["#", "[doc("].join("");
    let text = format!(
        "/// The summary.\n///\n/// And the rest of it.\n{opener}\n    alias = \"one\",\n    alias = \"two\"\n)]\nfn item() {{}}"
    );
    let wrapped = lines(&text);
    assert_eq!(
        past_the_attributes(&wrapped, wrapped.len() - 1),
        3,
        "the walk stopped at the continuation of a wrapped attribute rather than at \
         its first line"
    );
    let one = format!(
        "/// The summary.\n{}\nfn item() {{}}",
        ["#", "[inline]"].join("")
    );
    let plain = lines(&one);
    assert_eq!(past_the_attributes(&plain, plain.len() - 1), 1);
    // A closing bracket that belongs to an array, not to an attribute: the
    // walk stops rather than climbing out of the item it is reading.
    let array = lines("const A: [u8; 1] = [\n    1,\n];\nfn item() {}");
    assert_eq!(
        past_the_attributes(&array, array.len() - 1),
        array.len() - 1
    );
}
