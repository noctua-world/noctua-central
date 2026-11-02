//! Turns a validated catalog into the source of `nc-catalog`.
//!
//! Every name a consumer types comes from here, and two shapes recur because
//! they are what make the catalog's promise true:
//!
//! * **A handle is an enum variant, never a string.** `Notification::Foo` that
//!   no longer exists is a compile error; `send("foo")` is a runtime surprise on
//!   somebody's phone.
//! * **A table is sorted, always.** The message table is searched by bisection,
//!   so its order is a correctness property rather than a nicety; the others are
//!   sorted because a generated file whose order came from whatever iterated
//!   last would rewrite itself on an unrelated change. The lookups over the
//!   short tables — a command, a rate limit, an error code by its text — are
//!   linear scans over a handful of entries, which is faster than the bisection
//!   would be and does not need the invariant.

use std::fmt::Write as _;

use crate::model::{Catalog, Database, FieldType, Strategy};
use crate::{CommentStyle, EmittedFile, camel, doc, header, quoted};

const SPEC: &str = "catalog/";

/// Every file the catalog produces, unsorted.
pub(crate) fn all(catalog: &Catalog) -> Vec<EmittedFile> {
    vec![
        EmittedFile::new("crates/nc-catalog/src/lib.rs", lib()),
        EmittedFile::new("crates/nc-catalog/src/errors.rs", errors(catalog)),
        EmittedFile::new(
            "crates/nc-catalog/src/notifications.rs",
            notifications(catalog),
        ),
        EmittedFile::new("crates/nc-catalog/src/emails.rs", emails(catalog)),
        EmittedFile::new("crates/nc-catalog/src/limits.rs", limits(catalog)),
        EmittedFile::new("crates/nc-catalog/src/commands.rs", commands(catalog)),
        EmittedFile::new("crates/nc-catalog/src/events.rs", events(catalog)),
        EmittedFile::new("crates/nc-catalog/src/i18n.rs", i18n(catalog)),
        EmittedFile::new(
            "crates/nc-catalog/src/capabilities.rs",
            capabilities(catalog),
        ),
        EmittedFile::new("crates/nc-log/src/events.rs", typed_events(catalog)),
    ]
}

/// What the generated events file carries above its first struct.
///
/// **Named, because `load.rs` refuses the names it binds and had to be held to
/// it.** `ALREADY_IN_THE_GENERATED_FILE` lists the names a generated item may
/// not take, and one of the four was `LogValue` — imported `as _`, which binds
/// nothing, so that entry refused a name rustc accepts. The list stays
/// hand-written; what reads this is the test that asks whether each entry is a
/// name this text actually binds.
const PREAMBLE: &str = "//!\n\
     //! One struct per event, and every field's type implements `LogValue`. A\n\
     //! field the registry could declare as text does not exist, because the\n\
     //! registry has no way to name one — which is why a dynamic string cannot\n\
     //! compile into a log event (`NOCTUA.md §15.4`).\n\
     \n\
     use nc_catalog::Level;\n\
     \n\
     use crate::{Event, Field, LogValue as _};\n\
     \n";

/// [`PREAMBLE`], for the rule in `load.rs` that is held against it.
///
/// `const fn`, which is what the log's type wall asks of any function handing
/// back a `&'static str` it was not given: a literal returned from one is a
/// literal the compiler wrote.
#[cfg(test)]
pub(crate) const fn the_preamble_for_tests() -> &'static str {
    PREAMBLE
}

/// The typed event structs, into `nc-log`.
///
/// **Not into `nc-catalog`**, and the split is the reason `nc-catalog` has no
/// dependencies: these structs are bounded by `LogValue`, which is implemented
/// for `nc-ids`' identifier types — so a crate holding them holds those too, and
/// every service and app in the platform would inherit them for the sake of a
/// log. `nc-catalog` keeps the *descriptors*, which are data; `nc-log` gets the
/// types, which are a guarantee.
fn typed_events(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/log-events.toml");
    out.push_str(PREAMBLE);

    let mut sorted = catalog.events.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in &sorted {
        let name = camel(&entry.name);
        let _ = write!(
            out,
            "{}",
            // Backticked: a dotted event name reads to clippy as an item path,
            // and it is one — a reader should see it as a name, not as prose.
            doc(&format!("`{}` — {}", entry.name, entry.description))
        );
        out.push_str("#[derive(Debug, Clone)]\n");
        let _ = writeln!(out, "pub struct {name} {{");
        for field in &entry.fields {
            let kind = field.kind.rust();
            let rust = if field.optional {
                format!("Option<{kind}>")
            } else {
                kind.to_owned()
            };
            let label = field.kind.label();
            let _ = write!(
                out,
                "    {}",
                doc(&format!("{} {label}.", an_article_for(label)))
            );
            let _ = writeln!(out, "    pub {}: {rust},", crate::snake(&field.name));
        }
        out.push_str("}\n\n");

        // The seal. `Event` has a private supertrait, so an event outside this
        // crate cannot exist — which is what makes the type wall a wall rather
        // than a convention that a foreign `impl Event` walks around.
        let _ = writeln!(out, "impl crate::sealed::Sealed for {name} {{}}\n");
        let _ = writeln!(out, "impl Event for {name} {{");
        let _ = writeln!(
            out,
            "    fn name(&self) -> &'static str {{\n        {}\n    }}",
            quoted(&entry.name)
        );
        let _ = writeln!(
            out,
            "    fn level(&self) -> Level {{\n        Level::{}\n    }}",
            level_variant(entry.level)
        );
        out.push_str("    fn fields(&self) -> Vec<(&'static str, Field)> {\n        vec![\n");
        for field in &entry.fields {
            let _ = writeln!(
                out,
                "            ({}, self.{}.to_field()),",
                quoted(&field.name),
                crate::snake(&field.name)
            );
        }
        out.push_str("        ]\n    }\n}\n\n");
    }
    out
}

/// The variant name an error code is known by in Rust.
///
/// Derived from the message key rather than from the code, because
/// `ErrorCode::DbUnreachable` says what went wrong and `ErrorCode::NcDb0001`
/// makes every call site a lookup in a table. The code is still what travels
/// across a boundary and what a log is queried by — it is one method away.
///
/// # Panics
///
/// When `message_key` is outside the `error.` namespace, which
/// `error-message-namespace` refuses at load time — so reaching this needs a
/// caller that did not load the catalog.
#[must_use]
pub fn error_variant(message_key: &str) -> String {
    // **Not `unwrap_or(message_key)`.** That was a live branch nothing reached:
    // a code whose message key is outside the namespace compiled, and the
    // generated crate grew a public variant named after it. `load.rs`'s
    // `error-message-namespace` refuses that key now, so this can say what is
    // true rather than carry a fallback a reader believes in.
    camel(
        message_key
            .strip_prefix("error.")
            .expect("`error-message-namespace` refuses a message key outside it"),
    )
}

fn lib() -> String {
    let mut out = header(CommentStyle::Line("//!"), SPEC);
    out.push_str(
        "//!\n\
         //! The shared core of the platform, compiled from `catalog/`. Every error\n\
         //! code, notification, email, limit, command, log event, capability and\n\
         //! message lives here, and nothing outside `catalog/` may add one.\n\
         \n\
         // One lint fights generated code specifically. `match_same_arms` fires on\n\
         // every exhaustive match where two variants share a value — every error\n\
         // code in one area returns the same area string — and collapsing those\n\
         // arms would make the emitted list depend on which values happen to be\n\
         // equal today, so a new code could silently reshape an unrelated match.\n\
         #![allow(clippy::match_same_arms)]\n\
         \n\
         // And `too_many_lines`, for the same kind of reason. Every accessor of\n\
         // `ErrorCode` is one `match` with an arm per code, so its length is the\n\
         // registry's — reserving a stage's codes put `description` past clippy's\n\
         // hundred lines — and splitting a match to fit would make the emitted\n\
         // shape depend on how many codes there happen to be.\n\
         #![allow(clippy::too_many_lines)]\n\
         \n\
         // Every crate in this workspace denies the three printing lints at its\n\
         // own root, because a lint level does not cross a crate boundary and\n\
         // `print_stdout` is `restriction` — which neither `all` nor `pedantic`\n\
         // turns on. A generated crate is a crate, so the emitter writes it.\n\
         #![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]\n\
         // Every doctest of this crate forbids `unsafe` as the crate does: a\n\
         // doctest is a crate of its own, outside the root's `forbid`.\n\
         #![doc(test(attr(forbid(unsafe_code))))]\n\
         \n\
         pub mod capabilities;\n\
         pub mod commands;\n\
         pub mod emails;\n\
         pub mod errors;\n\
         pub mod events;\n\
         pub mod i18n;\n\
         pub mod limits;\n\
         pub mod notifications;\n\
         \n\
         pub use capabilities::{capability, Capability, CapabilitySpec, Ungated, CAPABILITIES, UNGATED};\n\
         pub use commands::{command, Command, Reversibility, COMMANDS};\n\
         pub use emails::Email;\n\
         pub use errors::{ErrorCode, Severity, AREAS};\n\
         pub use events::{EventSpec, FieldKind, FieldSpec, Level, EVENTS};\n\
         pub use i18n::{message, render, Locale, KEYS};\n\
         pub use limits::{rate_limit, Argon2Id, Budget, Database, KernelLimits, OnLockout, RateLimit, StoreLimit, Strategy, TableLimit, ARGON2ID_DEVICE_PIN, ARGON2ID_PASSWORD, KERNEL, RATE_LIMITS, STORES, TABLES};\n\
         pub use notifications::Notification;\n",
    );
    out
}

fn errors(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/errors.toml");
    out.push_str(
        "//!\n\
         //! Every error the platform can produce. The code travels through every\n\
         //! layer and is what the central log is queried by, so an error whose\n\
         //! identity changed as it crossed a boundary could not be traced.\n\
         \n",
    );

    out.push_str(&severity());

    let mut sorted = catalog.errors.clone();
    sorted.sort_by(|a, b| a.code.cmp(&b.code));

    out.push_str("/// Every error code this platform declares.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    // Deliberately NOT `#[non_exhaustive]`. Nothing here is published, so the
    // usual reason does not apply — and it would force `_ =>` in every match
    // outside this crate, which is every match there is. A new error code
    // becoming a compile error at every site that handles them is the catalog's
    // whole promise (`§15.2`), and that attribute is what would break it.
    out.push_str("pub enum ErrorCode {\n");
    for entry in &sorted {
        let _ = writeln!(
            out,
            "    {}    {},",
            doc(&format!("{} — {}", entry.code, entry.description)),
            error_variant(&entry.message)
        );
    }
    out.push_str("}\n\n");

    out.push_str("impl ErrorCode {\n");
    out.push_str("    /// Every code, in code order.\n");
    out.push_str("    pub const ALL: &'static [Self] = &[\n");
    for entry in &sorted {
        let _ = writeln!(out, "        Self::{},", error_variant(&entry.message));
    }
    out.push_str("    ];\n\n");

    out.push_str(&accessors(&sorted));

    out.push_str(
        "    /// The code a `NC-…` string names, if any.\n\
         \x20   ///\n\
         \x20   /// This is the boundary crossing: a code arrives from another service as\n\
         \x20   /// text, and everything after this point has it as a type again.\n\
         \x20   #[must_use]\n\
         \x20   pub fn from_code(code: &str) -> Option<Self> {\n\
         \x20       Self::ALL.iter().copied().find(|candidate| candidate.code() == code)\n\
         \x20   }\n\
         }\n\n\
         impl core::fmt::Display for ErrorCode {\n\
         \x20   fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {\n\
         \x20       formatter.write_str(self.code())\n\
         \x20   }\n\
         }\n\n",
    );

    out.push_str(&areas(catalog));
    out
}

/// The fixed area list, which is the other half of what makes a code checkable.
fn areas(catalog: &Catalog) -> String {
    let mut out = String::from("/// The fixed area list: the area, and what it covers.\n");
    out.push_str("pub const AREAS: &[(&str, &str)] = &[\n");
    for (area, description) in &catalog.areas {
        let _ = writeln!(out, "    ({}, {}),", quoted(area), quoted(description));
    }
    out.push_str("];\n");
    out
}

/// The six accessors every code answers to.
fn accessors(sorted: &[crate::model::ErrorEntry]) -> String {
    let mut out = String::new();
    method(
        &mut out,
        "code",
        "&'static str",
        "The stable `NC-<AREA>-<NNNN>` identity.",
        sorted,
        |entry| quoted(&entry.code),
    );
    method(
        &mut out,
        "area",
        "&'static str",
        "The area the code belongs to.",
        sorted,
        |entry| {
            // Through `load`'s own reader rather than a second parse here. This
            // crate computes nothing, and a scheme understood in two places is a
            // scheme that will one day be understood two ways.
            quoted(
                crate::load::area_of(&entry.code)
                    .expect("`malformed-code` refuses a code that has no area"),
            )
        },
    );
    method(
        &mut out,
        "description",
        "&'static str",
        "One line, English, for a developer. Never shown to a user.",
        sorted,
        |entry| quoted(&entry.description),
    );
    method(
        &mut out,
        "message_key",
        "&'static str",
        "The i18n key of what a user is shown.",
        sorted,
        |entry| quoted(&entry.message),
    );
    method(
        &mut out,
        "severity",
        "Severity",
        "How bad it is.",
        sorted,
        |entry| format!("Severity::{}", entry.severity.variant()),
    );
    method(
        &mut out,
        "retryable",
        "bool",
        "Whether trying the same thing again could succeed.",
        sorted,
        |entry| entry.retryable.to_string(),
    );
    // **The one the registry declared and the code did not carry.** `params` was
    // validated against the sentence by `load::declared_params` and then dropped:
    // `Notification::params()` and `Email::params()` both exist, and a caller
    // holding an `ErrorCode` had no way to learn that `NC-VAL-0001`'s message
    // needs `field`, `min` and `max`. `§10.9` makes a response a key **plus
    // parameters**, and for errors that was unenforceable.
    method(
        &mut out,
        "params",
        "&'static [&'static str]",
        "The placeholders `message_key`'s sentence interpolates.",
        sorted,
        |entry| list(&entry.params),
    );

    out
}

/// The indefinite article a label takes.
///
/// **Lifted out of `typed_events` so a test can reach it**, which is the repair
/// this file has already been given twice — see `on_lockout_variant` and
/// `docs::what_a_lockout_does` — and for the same reason: one of its two arms is
/// reached by no catalog this compiler will accept. The only `FieldType` whose
/// `label()` begins with a vowel is `Id`, and `load.rs` refuses `kind = "id"`
/// unconditionally, so the `"An"` arm is generated by nothing and the comparison
/// that holds every other value in this file — the emitter against its own
/// committed output — is true by construction for it. Measured on 2026-09-08,
/// the whole rule replaced by `"A"`:
///
/// ```text
/// ok    15 generated file(s) match the catalog
/// ok    two generations are byte-identical
/// check passed              ok 1219 test(s)
/// ```
///
/// `emit::all` is `pub(crate)` with one caller, on an already-validated catalog,
/// so no scratch registry can reach the branch either — which is why
/// `the_emitter_carries_values_the_shipped_registry_happens_not_to_use`, the
/// test written for exactly this class, cannot cover this one.
///
/// The day `§5.1` closes and that refusal goes — `load.rs`'s own comment
/// promises it — the first `id` field writes `A opaque identifier…` into a
/// generated file whose header forbids editing, which is verbatim the sentence
/// this rule was written to fix.
///
/// `const fn` over the bytes rather than `starts_with`, which is what the log's
/// type wall asks of any function handing back a `&'static str` it was not
/// given: a literal returned from a `const fn` is one the compiler wrote, not
/// one produced at run time. Every label in `model.rs` is ASCII, so the first
/// byte is the first letter.
const fn an_article_for(label: &str) -> &'static str {
    match label.as_bytes() {
        [b'a' | b'e' | b'i' | b'o' | b'u', ..] => "An",
        _ => "A",
    }
}

/// Every argument name a command's JSON Schema declares, in declaration order.
///
/// Read by [`nc_schema::Schema`], the reader `load` judged the schema with, so
/// the names published are the properties validated — one parse, where there
/// were two that agreed only while no key was written twice. `load` has
/// already refused a schema outside the subset, so anything reaching here
/// compiles; a schema with no `properties` declares no arguments, which is a
/// command that takes none.
fn argument_names(schema: &str) -> Vec<String> {
    nc_schema::Schema::compile(schema).map_or_else(
        |_| Vec::new(),
        |compiled| {
            compiled
                .properties()
                .into_iter()
                .map(str::to_owned)
                .collect()
        },
    )
}

/// The severity enum, which is `errors.toml`'s only shape that is not a list.
fn severity() -> String {
    let mut out = String::from("/// How much attention something deserves.\n");
    out.push_str("///\n");
    out.push_str("/// One scale for errors and for notifications, deliberately: a reader\n");
    out.push_str("/// deciding whether to open a notification and an operator deciding\n");
    out.push_str("/// whether to wake somebody are grading the same axis, and two\n");
    out.push_str("/// vocabularies for one judgement is how they drift apart.\n");
    out.push_str("///\n");
    out.push_str("/// **Least severe first, because the derived `Ord` is the scale.**\n");
    out.push_str("/// Written the other way round, `Severity::Critical > Severity::Info`\n");
    out.push_str("/// was `false` and `.max()` answered `Info`: the obvious paging filter,\n");
    out.push_str("/// `severity() >= Severity::Error`, kept `IdsNameTooLong` and dropped\n");
    out.push_str("/// `JournalChainBroken` — the code that means the audit chain is\n");
    out.push_str("/// broken. Declaration order *is* the comparison for a derived `Ord`,\n");
    out.push_str("/// so the list below is the ranking and reordering it changes what\n");
    out.push_str("/// every comparison in the platform means.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    out.push_str("pub enum Severity {\n");
    for (variant, description) in [
        ("Info", "Worth recording, nothing failed."),
        ("Warning", "The operation continued, degraded."),
        ("Error", "The operation failed."),
        ("Critical", "Something a person should be woken for."),
    ] {
        let _ = writeln!(out, "    {}    {variant},", doc(description));
    }
    out.push_str("}\n\n");
    as_str(
        &mut out,
        "Severity",
        &[
            ("Info", "info"),
            ("Warning", "warning"),
            ("Error", "error"),
            ("Critical", "critical"),
        ],
    );
    out
}

/// The `as_str` a plain enum gets, over the words the registry writes.
///
/// **No enum in the generated crate had a text form**, so every consumer that
/// needed one wrote its own: `xtask`'s pages test has a `severity_word` and
/// `nc-log` had a `level_name`, while the ledger, `docs/errors.md` and
/// `docs/notifications.md` all write `critical` and `{:?}` gives `Critical`.
/// `Severity`'s own doc names the hazard — "two vocabularies for one judgement
/// is how they drift apart" — and there were three.
///
/// The test in `xtask` keeps its own list on purpose: a comparison relates two
/// things, and one of them has to be independent of the emitter.
fn as_str(out: &mut String, name: &str, variants: &[(&str, &str)]) {
    let _ = writeln!(out, "impl {name} {{");
    out.push_str("    /// The word the registry, the ledger and the pages write.\n");
    out.push_str("    #[must_use]\n");
    out.push_str("    pub const fn as_str(self) -> &'static str {\n");
    out.push_str("        match self {\n");
    for (variant, word) in variants {
        let _ = writeln!(out, "            Self::{variant} => \"{word}\",");
    }
    out.push_str("        }\n    }\n}\n\n");
    let _ = write!(
        out,
        "impl core::fmt::Display for {name} {{\n\
         \x20   fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {{\n\
         \x20       formatter.write_str(self.as_str())\n\
         \x20   }}\n\
         }}\n\n"
    );
}

/// One `const fn` over every variant, written as a match.
fn method(
    out: &mut String,
    name: &str,
    returns: &str,
    description: &str,
    entries: &[crate::model::ErrorEntry],
    value: impl Fn(&crate::model::ErrorEntry) -> String,
) {
    let _ = write!(
        out,
        "    /// {description}\n    #[must_use]\n    pub const fn {name}(self) -> {returns} {{\n        match self {{\n"
    );
    for entry in entries {
        let _ = writeln!(
            out,
            "            Self::{} => {},",
            error_variant(&entry.message),
            value(entry)
        );
    }
    out.push_str("        }\n    }\n\n");
}

fn notifications(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/notifications.toml");
    out.push_str(
        "//!\n\
         //! Every notification a user can receive. Sending one takes a handle from\n\
         //! this enum rather than a string, so a notification that was removed is a\n\
         //! compile error rather than a message nobody ever sees again.\n\
         \n\
         use crate::errors::Severity;\n\
         \n",
    );
    let mut sorted = catalog.notifications.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    out.push_str("/// Every notification this platform declares.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    // Not `#[non_exhaustive]`, for the reason `ErrorCode` gives.
    out.push_str("pub enum Notification {\n");
    for entry in &sorted {
        let _ = writeln!(out, "    {}    {},", doc(&entry.id), camel(&entry.id));
    }
    out.push_str("}\n\n");

    out.push_str("impl Notification {\n");
    out.push_str("    /// Every notification, in id order.\n");
    out.push_str("    pub const ALL: &'static [Self] = &[\n");
    for entry in &sorted {
        let _ = writeln!(out, "        Self::{},", camel(&entry.id));
    }
    out.push_str("    ];\n\n");

    for (name, returns, description, value) in [
        ("id", "&'static str", "The stable id.", 0),
        ("title_key", "&'static str", "The i18n key of the title.", 1),
        ("body_key", "&'static str", "The i18n key of the body.", 2),
        (
            "params",
            "&'static [&'static str]",
            "The placeholders the body interpolates.",
            3,
        ),
        ("severity", "Severity", "How prominently it is shown.", 4),
    ] {
        let _ = write!(
            out,
            "    /// {description}\n    #[must_use]\n    pub const fn {name}(self) -> {returns} {{\n        match self {{\n"
        );
        for entry in &sorted {
            let rendered = match value {
                0 => quoted(&entry.id),
                1 => quoted(&entry.title),
                2 => quoted(&entry.body),
                3 => list(&entry.params),
                _ => format!("Severity::{}", entry.severity.variant()),
            };
            let _ = writeln!(out, "            Self::{} => {rendered},", camel(&entry.id));
        }
        out.push_str("        }\n    }\n\n");
    }
    out.push_str("}\n");
    out
}

fn emails(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/emails.toml");
    out.push_str(
        "//!\n\
         //! Every transactional email. Same contract as the notifications: a handle,\n\
         //! not a string, and both message keys resolve in every locale.\n\
         \n",
    );
    let mut sorted = catalog.emails.clone();
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    out.push_str("/// Every transactional email this platform declares.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    // Not `#[non_exhaustive]`, for the reason `ErrorCode` gives.
    out.push_str("pub enum Email {\n");
    for entry in &sorted {
        let _ = writeln!(out, "    {}    {},", doc(&entry.id), camel(&entry.id));
    }
    out.push_str("}\n\n");

    out.push_str("impl Email {\n");
    out.push_str("    /// Every email, in id order.\n");
    out.push_str("    pub const ALL: &'static [Self] = &[\n");
    for entry in &sorted {
        let _ = writeln!(out, "        Self::{},", camel(&entry.id));
    }
    out.push_str("    ];\n\n");

    for (name, returns, description, which) in [
        ("id", "&'static str", "The stable id.", 0),
        (
            "subject_key",
            "&'static str",
            "The i18n key of the subject.",
            1,
        ),
        ("body_key", "&'static str", "The i18n key of the body.", 2),
        (
            "params",
            "&'static [&'static str]",
            "The placeholders the body interpolates.",
            3,
        ),
    ] {
        let _ = write!(
            out,
            "    /// {description}\n    #[must_use]\n    pub const fn {name}(self) -> {returns} {{\n        match self {{\n"
        );
        for entry in &sorted {
            let rendered = match which {
                0 => quoted(&entry.id),
                1 => quoted(&entry.subject),
                2 => quoted(&entry.body),
                _ => list(&entry.params),
            };
            let _ = writeln!(out, "            Self::{} => {rendered},", camel(&entry.id));
        }
        out.push_str("        }\n    }\n\n");
    }
    out.push_str("}\n");
    out
}

fn limits(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/limits.toml");
    out.push_str(
        "//!\n\
         //! Every safety limit and tunable. `TABLES` is what the one pruner reads to\n\
         //! enforce every table's bound, in either database, and what the gate over\n\
         //! `migrations/` and `local-migrations/` requires an entry in.\n\
         \n",
    );

    out.push_str("/// How a table's rows are chosen for removal.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum Strategy {\n");
    out.push_str("    /// The oldest rows go first, in batches.\n    OldestFirst,\n");
    out.push_str("    /// The pruner does not touch this table: its retention is a policy-pack property, per realm.\n    PolicyPack,\n");
    out.push_str("    /// The pruner does not touch this table: its oldest rows are user state or a unit's, and its writer refuses a new row at `max_rows`.\n    RefuseAtLimit,\n");
    out.push_str("}\n\n");

    out.push_str("/// Which database a table lives in.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum Database {\n");
    out.push_str("    /// PostgreSQL, built by `migrations/`.\n    Server,\n");
    out.push_str("    /// The SQLite file on the user's machine, built by `local-migrations/`: `database = \"local\"`.\n    LocalFile,\n");
    out.push_str("}\n\n");

    out.push_str("/// What bounds one table, in either database.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub struct TableLimit {\n");
    out.push_str("    /// The table's name.\n    pub name: &'static str,\n");
    out.push_str("    /// Which database holds it.\n    pub database: Database,\n");
    out.push_str("    /// How rows are chosen for removal.\n    pub strategy: Strategy,\n");
    out.push_str("    /// The column that orders the table for pruning. Oldest is smallest.\n    pub order_by: &'static str,\n");
    out.push_str(
        "    /// The row count above which pruning runs, or, under `RefuseAtLimit`, at which a new row is refused.\n    pub max_rows: Option<u64>,\n",
    );
    out.push_str(
        "    /// How many rows one pruning run removes.\n    pub prune_batch: Option<u64>,\n",
    );
    out.push_str("}\n\n");

    out.push_str("/// Every table that declares a bound, in either database, in name order.\n");
    out.push_str("pub const TABLES: &[TableLimit] = &[\n");
    for (name, table) in &catalog.limits.tables {
        let strategy = match table.strategy {
            Strategy::OldestFirst => "Strategy::OldestFirst",
            Strategy::PolicyPack => "Strategy::PolicyPack",
            Strategy::RefuseAtLimit => "Strategy::RefuseAtLimit",
        };
        let database = match table.database {
            Database::Server => "Database::Server",
            Database::LocalFile => "Database::LocalFile",
        };
        let _ = writeln!(
            out,
            "    TableLimit {{ name: {}, database: {database}, strategy: {strategy}, order_by: {}, max_rows: {}, prune_batch: {} }},",
            quoted(name),
            quoted(&table.order_by),
            option(table.max_rows),
            option(table.prune_batch)
        );
    }
    out.push_str("];\n\n");

    out.push_str("/// What bounds one store that is not a PostgreSQL table.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct StoreLimit {\n");
    out.push_str("    /// The store's name.\n    pub name: &'static str,\n");
    out.push_str("    /// How long data is kept.\n    pub retention_days: u32,\n");
    out.push_str("    /// The disk ceiling, in bytes.\n    pub max_disk_bytes: u64,\n");
    out.push_str("    /// What actually enforces the two numbers above.\n    pub enforced_by: &'static str,\n");
    out.push_str("}\n\n");

    out.push_str("/// Every store outside PostgreSQL that declares a bound, in name order.\n");
    out.push_str("pub const STORES: &[StoreLimit] = &[\n");
    for (name, store) in &catalog.limits.stores {
        let _ = writeln!(
            out,
            "    StoreLimit {{ name: {}, retention_days: {}, max_disk_bytes: {}, enforced_by: {} }},",
            quoted(name),
            number(store.retention_days),
            number(store.max_disk_bytes),
            quoted(&store.enforced_by)
        );
    }
    out.push_str("];\n\n");

    out.push_str(&hashing(catalog));
    out.push_str(&rates(catalog));
    out.push_str(&kernel(catalog));
    out
}

/// The kernel's limits, as one struct and the one constant of it.
///
/// **Sealed against construction, like `Command`.** The kernel holds every
/// plugin to these numbers and a manifest may only lower them, so a
/// `KernelLimits` a caller could write itself would be a second source of the
/// bound — measured on `Command`, where a forged value read back as read-only.
/// `#[non_exhaustive]` refuses a struct literal outside this crate and the
/// update syntax with it, so `KERNEL` is the only value there is.
fn kernel(catalog: &Catalog) -> String {
    let limits = catalog.limits.kernel.each();
    let mut out = String::new();
    out.push_str("\n/// The bounds the local kernel holds every plugin to (`NOCTUA.md §11.3`).\n");
    out.push_str("///\n");
    out.push_str("/// `KERNEL` is the value the kernel reads, and the only one this crate\n");
    out.push_str("/// builds. The type is sealed against a struct literal, not against an\n");
    out.push_str("/// edited copy of `KERNEL`: the kernel's bounds stay the catalog's, lowered\n");
    out.push_str("/// only by a plugin's manifest, because the kernel takes no value of this\n");
    out.push_str("/// type from its caller.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct KernelLimits {\n");
    for limit in &limits {
        let sentence = limit.unit.in_words().map_or_else(
            || format!("{}.", limit.bounds),
            |unit| format!("{}, in {unit}.", limit.bounds),
        );
        let _ = write!(out, "    {}", doc(&sentence));
        let _ = writeln!(
            out,
            "    pub {}: {},",
            limit.name,
            if limit.wide { "u64" } else { "u32" }
        );
    }
    out.push_str("}\n\n");
    out.push_str(
        "/// The kernel's limits, as `catalog/limits.toml` declares them under `[kernel]`.\n",
    );
    out.push_str("pub const KERNEL: KernelLimits = KernelLimits {\n");
    for limit in &limits {
        let _ = writeln!(out, "    {}: {},", limit.name, number(limit.value));
    }
    out.push_str("};\n");
    out
}

/// The `Level` enum, with the note that its order is the scale.
///
/// Its own function because `events` crossed clippy's line limit when `Level`
/// gained an ordering — and it is a self-contained piece of the file: five
/// variants and the paragraph saying why they are in that order.
fn the_level_enum() -> String {
    let mut out = String::new();
    // **Ordered, like `Severity`, and for the same reason.** A minimum-level
    // filter is the first thing a log consumer writes, and `Level` had no
    // ordering at all: `level >= Level::Warn` did not compile, while
    // `Severity::Critical >= Severity::Error` did and is documented three
    // hundred lines up as *the scale itself*. Least severe first, so the derived
    // `Ord` is that scale here too.
    out.push_str("/// The level an event is emitted at.\n");
    out.push_str("///\n");
    out.push_str("/// **Least noisy first, because the derived `Ord` is the scale.** A\n");
    out.push_str("/// consumer's first filter is `level >= Level::Warn`, and it has to mean\n");
    out.push_str("/// what it says: written the other way round it would keep `Trace` and\n");
    out.push_str("/// drop `Error`. `Severity` carries the same note for the same reason.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    out.push_str("pub enum Level {\n");
    for (variant, description) in [
        ("Trace", "Very fine detail."),
        ("Debug", "Detail a developer wants while diagnosing."),
        ("Info", "Something worth a line in normal operation."),
        ("Warn", "Degraded, but working."),
        ("Error", "Failed."),
    ] {
        let _ = writeln!(out, "    {}    {variant},", doc(description));
    }
    out.push_str("}\n\n");
    as_str(
        &mut out,
        "Level",
        &[
            ("Trace", "trace"),
            ("Debug", "debug"),
            ("Info", "info"),
            ("Warn", "warn"),
            ("Error", "error"),
        ],
    );
    out
}

/// The Argon2id parameters, as one struct and two constants.
fn hashing(catalog: &Catalog) -> String {
    let hashing = catalog.limits.argon2id;
    let mut out = String::new();
    out.push_str("/// One Argon2id configuration.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct Argon2Id {\n");
    out.push_str("    /// Memory cost, in kibibytes. The parameter that costs an attacker.\n    pub memory_kib: u32,\n");
    out.push_str("    /// Time cost.\n    pub iterations: u32,\n");
    out.push_str("    /// Degree of parallelism.\n    pub parallelism: u32,\n");
    out.push_str("    /// Length of the derived key, in bytes.\n    pub output_length: u32,\n");
    out.push_str("    /// Length of the salt, in bytes.\n    pub salt_length: u32,\n");
    out.push_str("}\n\n");
    for (name, doc, argon) in [
        (
            "ARGON2ID_PASSWORD",
            "What an account password is hashed with.",
            hashing.password,
        ),
        (
            "ARGON2ID_DEVICE_PIN",
            "What a device PIN's key is derived with. Deliberately harder than a password's: a PIN is four to eight digits, so the secret is enumerable and the only defence that scales is per-guess cost.",
            hashing.device_pin,
        ),
    ] {
        let _ = write!(
            out,
            "/// {doc}\npub const {name}: Argon2Id = Argon2Id {{ memory_kib: {}, iterations: {}, parallelism: {}, output_length: {}, salt_length: {} }};\n\n",
            number(argon.memory_kib),
            number(argon.iterations),
            number(argon.parallelism),
            number(argon.output_length),
            number(argon.salt_length)
        );
    }
    out
}

/// The rate-limit table, with both budgets per surface.
fn rates(catalog: &Catalog) -> String {
    let mut out = String::new();
    out.push_str("/// What happens when a budget is exhausted.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum OnLockout {\n");
    out.push_str("    /// Stop answering until the window passes.\n    Refuse,\n");
    out.push_str("    /// Destroy the stored device credential, forcing a full re-authentication.\n    WipeCredential,\n");
    out.push_str("}\n\n");

    out.push_str("/// One rate-limit budget.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct Budget {\n");
    out.push_str("    /// The window attempts are counted in.\n    pub window_seconds: u32,\n");
    out.push_str("    /// Attempts allowed in the window.\n    pub max_attempts: u32,\n");
    out.push_str("    /// The escalating delay after each failure. The last value repeats.\n    pub delay_ms: &'static [u32],\n");
    out.push_str("    /// The failure count at which the surface stops answering at all.\n    pub lockout_after: Option<u32>,\n");
    out.push_str(
        "    /// Whether the lockout count survives the window.\n    pub cumulative: bool,\n",
    );
    out.push_str("    /// What happens at lockout, when it is more than refusing.\n    pub on_lockout: Option<OnLockout>,\n");
    out.push_str("}\n\n");

    out.push_str("/// One authentication surface's two budgets.\n");
    out.push_str("///\n");
    out.push_str("/// Two, because they defend against different attacks: one person grinding\n");
    out.push_str("/// one account, and one address spraying one password across many.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct RateLimit {\n");
    out.push_str("    /// Which surface it guards.\n    pub surface: &'static str,\n");
    out.push_str("    /// What one identifier may attempt.\n    pub per_identifier: Budget,\n");
    out.push_str("    /// What one client address may attempt.\n    pub per_address: Budget,\n");
    out.push_str("}\n\n");

    let mut rate_limits = catalog.limits.rate_limits.clone();
    rate_limits.sort_by(|a, b| a.surface.cmp(&b.surface));
    out.push_str("/// Every rate limit, in surface order.\n");
    out.push_str("pub const RATE_LIMITS: &[RateLimit] = &[\n");
    for limit in &rate_limits {
        let _ = writeln!(
            out,
            "    RateLimit {{ surface: {}, per_identifier: {}, per_address: {} }},",
            quoted(&limit.surface),
            budget(&limit.per_identifier),
            budget(&limit.per_address)
        );
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// The limits for a surface, if it declares any.\n\
         #[must_use]\n\
         pub fn rate_limit(surface: &str) -> Option<&'static RateLimit> {\n\
         \x20   RATE_LIMITS.iter().find(|candidate| candidate.surface == surface)\n\
         }\n",
    );
    out
}

fn budget(budget: &crate::model::Budget) -> String {
    let delays = budget
        .delay_ms
        .iter()
        .map(|delay| number(*delay))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Budget {{ window_seconds: {}, max_attempts: {}, delay_ms: &[{delays}], lockout_after: {}, cumulative: {}, on_lockout: {} }}",
        number(budget.window_seconds),
        number(budget.max_attempts),
        option(budget.lockout_after),
        budget.cumulative,
        on_lockout_variant(budget.on_lockout)
    )
}

/// How a budget's `on_lockout` is written into the generated file.
///
/// **Lifted out of `budget` so a test can reach it**, which is the same repair
/// `docs::what_a_lockout_does` was given and for the same reason: one of its
/// two arms is reached by no catalog this repository has.
///
/// `catalog/limits.toml` declares four budgets with a `lockout_after` and
/// exactly one `on_lockout`, and it is `wipe-credential`. So `Refuse` is
/// generated by nothing, and the comparison that holds every other value in
/// this file — the emitter against its own committed output — is true by
/// construction for that arm. Measured, `Refuse => "NoSuchVariant"`:
///
/// ```text
/// ok    15 generated file(s) match the catalog
/// check passed
/// ```
///
/// The same edit on the live arm is `1 generated file(s) out of sync`. The day
/// a budget spells `refuse`, `cargo xtask build` writes a variant the generated
/// crate does not declare and that crate stops compiling — which is a build
/// broken by a one-word registry edit that nothing here would have caught.
fn on_lockout_variant(on_lockout: Option<crate::model::OnLockout>) -> String {
    on_lockout.map_or_else(
        || "None".to_owned(),
        |value| {
            format!(
                "Some(OnLockout::{})",
                match value {
                    crate::model::OnLockout::Refuse => "Refuse",
                    crate::model::OnLockout::WipeCredential => "WipeCredential",
                }
            )
        },
    )
}

fn commands(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/commands.toml");
    out.push_str(
        "//!\n\
         //! Every command, with everything a dispatcher and a policy decision point\n\
         //! need. `args_schema` is also the command's AI tool interface: there is no\n\
         //! separate tools subsystem, and building one would create commands that\n\
         //! bypass the enforcement point.\n\
         \n",
    );

    out.push_str("/// Whether a command can be undone.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum Reversibility {\n");
    out.push_str("    /// Changes nothing.\n    Read,\n");
    // **What the page says, because they are one claim.** `docs.rs` publishes
    // the same answer for this variant, and the kernel compiles against this
    // crate and not against the page, so this is the sentence its author reads:
    // `kernel.undo` takes back the store's writes and the blobs a step made,
    // stops at a step that left the machine or cannot be undone with no
    // compensation registered, and leaves what the step set going as it is.
    // **A reviewer found it saying more**: that undo stopped at a step that
    // did *anything* else, while a step that armed a timer or published an
    // event was undone past, the timer firing after.
    out.push_str(
        "    /// Changes state; `kernel.undo` takes back what a step of it kept in the \
         store and the blobs it made, stops at a step that left the machine or cannot be \
         undone with no compensation registered, and leaves a timer it armed or an event \
         it published as it is.\n    \
         Write,\n",
    );
    out.push_str("    /// Cannot be reversed, and therefore requires a human confirmation the kernel issues.\n    Irreversible,\n");
    out.push_str("}\n\n");

    out.push_str("/// One command.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    // **Sealed against construction, unlike `TableLimit` below.** Every field is
    // `pub` and the type is `Copy`, so a caller could take a real command and
    // hand a dispatcher `forged.action = "Read"; forged.mutating = false;` —
    // measured, a `Delete` command read back as read-only, and an invented name
    // waved through. `nc-prune` records the same shape on `TableLimit`, where a
    // consumer could ask the pruner to empty the append-only journal.
    //
    // The *enums* are deliberately not `#[non_exhaustive]`, with the reason
    // written where they are emitted: it would force a `_ =>` arm on every
    // match, which is the thing a closed enum exists to prevent. That argument
    // is about matching and does not apply to construction, which is what this
    // closes.
    //
    // `TableLimit` keeps its literal because twelve fixtures in `nc-prune` build
    // one, and those fixtures are the tests that prove `the_catalog_disagrees` —
    // the runtime guard written after a reviewer emptied `access_records`
    // through it. The guard is what enforces that rule; sealing the type would
    // cost the tests that keep the guard honest.
    out.push_str("#[non_exhaustive]\npub struct Command {\n");
    out.push_str("    /// Dotted, and unique platform-wide.\n    pub name: &'static str,\n");
    out.push_str("    /// The i18n key of the human name.\n    pub title_key: &'static str,\n");
    out.push_str("    /// The i18n key of one line saying what it does.\n    pub description_key: &'static str,\n");
    out.push_str("    /// The Cedar action name the policy decision point evaluates.\n    pub action: &'static str,\n");
    out.push_str("    /// How the kernel extracts the resource from the arguments.\n    pub resource_selector: &'static str,\n");
    out.push_str("    /// Whether it can be undone.\n    pub reversibility: Reversibility,\n");
    out.push_str("    /// Whether it changes state at all.\n    pub mutating: bool,\n");
    out.push_str(
        "    /// The JSON Schema for its arguments.\n    pub args_schema: &'static str,\n",
    );
    out.push_str("    /// The default keybinding, where it has one.\n    pub keybinding: Option<&'static str>,\n");
    out.push_str(
        "    /// Every argument name `args_schema` declares, in the order it declares them.\n\
         \x20   ///\n\
         \x20   /// **Read out of the schema here rather than re-parsed by every caller.**\n\
         \x20   /// `limits.prune` was declared with `table` and `dry_run` and its\n\
         \x20   /// implementation took neither, and the two tests that now compare them were\n\
         \x20   /// each parsing this JSON by hand — one of them wrongly, so the direction\n\
         \x20   /// that rots (a property declared with nothing behind it) went unnoticed.\n\
         \x20   /// The compiler already parses the schema to validate it; this is the same\n\
         \x20   /// parse, kept.\n\
         \x20   pub arguments: &'static [&'static str],\n",
    );
    out.push_str("}\n\n");

    let mut sorted = catalog.commands.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    out.push_str("/// Every command, in name order.\n");
    out.push_str("pub const COMMANDS: &[Command] = &[\n");
    for entry in &sorted {
        let _ = writeln!(
            out,
            "    Command {{ name: {}, title_key: {}, description_key: {}, action: {}, resource_selector: {}, reversibility: Reversibility::{}, mutating: {}, args_schema: {}, keybinding: {}, arguments: {} }},",
            quoted(&entry.name),
            quoted(&entry.title),
            quoted(&entry.description),
            quoted(&entry.action),
            quoted(&entry.resource_selector),
            entry.reversibility.variant(),
            entry.mutating,
            quoted(&entry.args_schema),
            entry.keybinding.as_ref().map_or_else(
                || "None".to_owned(),
                |value| format!("Some({})", quoted(value))
            ),
            list(&argument_names(&entry.args_schema))
        );
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// The command a name refers to, if any.\n\
         #[must_use]\n\
         pub fn command(name: &str) -> Option<&'static Command> {\n\
         \x20   COMMANDS.iter().find(|candidate| candidate.name == name)\n\
         }\n",
    );
    out
}

fn events(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/log-events.toml");
    out.push_str(
        "//!\n\
         //! What the central log can receive, as data. The typed structs a caller\n\
         //! actually constructs are generated into `nc-log`, where the `LogValue`\n\
         //! trait that bounds their fields lives; these descriptors are what the\n\
         //! developer surfaces render, and what a test reads to prove the two agree.\n\
         \n",
    );

    out.push_str(&the_level_enum());

    out.push_str("/// What a log-event field may be.\n");
    out.push_str("///\n");
    out.push_str("/// There is no variant for a string that came from anywhere but the source.\n");
    out.push_str("/// That absence is the type wall: a dynamic string is the shape every\n");
    out.push_str("/// accidental leak of user data takes, and this enum cannot name one.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("pub enum FieldKind {\n");
    for (variant, description) in [
        ("U64", "An unsigned 64-bit count."),
        ("I64", "A signed 64-bit number."),
        ("U32", "An unsigned 32-bit count."),
        ("Bool", "A flag."),
        ("Duration", "A span of time."),
        ("ErrorCode", "A code from the error registry."),
        (
            "Id",
            // **The identifier reaches the log whole**, which is what `§15.4`'s
            // own table says the central log contains. This said the opposite,
            // in generated *public* documentation — the sentence a developer
            // reads immediately before writing `work_unit: OpaqueId::new(email)`.
            // `model.rs` was corrected thirty lines from here and this copy was
            // not, which is the drift the catalog exists to prevent.
            "An opaque identifier, written to the log as it is. It is not free \
             text — but nothing stops a caller minting one *out of* personal \
             data, so derive it, never copy it (NOCTUA.md §5.1, which is OPEN \
             and owns that risk).",
        ),
        (
            "Snowflake",
            "A Snowflake, written as its Crockford base32 text.",
        ),
        ("Literal", "A literal in the source, never input."),
        (
            "Enum",
            "A variant of an enum the platform declares — a state, never a name.",
        ),
    ] {
        let _ = writeln!(out, "    {}    {variant},", doc(description));
    }
    out.push_str("}\n\n");

    out.push_str("/// One field of a log event.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct FieldSpec {\n");
    out.push_str("    /// The field name.\n    pub name: &'static str,\n");
    out.push_str("    /// Its type, which is the whole guarantee.\n    pub kind: FieldKind,\n");
    out.push_str("    /// Whether it may be absent.\n    pub optional: bool,\n");
    out.push_str("}\n\n");

    out.push_str("/// One event the central log can receive.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct EventSpec {\n");
    out.push_str("    /// Dotted, and what the generated struct is named after.\n    pub name: &'static str,\n");
    out.push_str(
        "    /// One line, English, for a developer.\n    pub description: &'static str,\n",
    );
    out.push_str("    /// The level it is emitted at.\n    pub level: Level,\n");
    out.push_str("    /// What it carries.\n    pub fields: &'static [FieldSpec],\n");
    out.push_str("}\n\n");

    let mut sorted = catalog.events.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    out.push_str("/// Every event, in name order.\n");
    out.push_str("pub const EVENTS: &[EventSpec] = &[\n");
    for entry in &sorted {
        let fields = entry
            .fields
            .iter()
            .map(|field| {
                format!(
                    "FieldSpec {{ name: {}, kind: FieldKind::{}, optional: {} }}",
                    quoted(&field.name),
                    kind_variant(field.kind),
                    field.optional
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "    EventSpec {{ name: {}, description: {}, level: Level::{}, fields: &[{fields}] }},",
            quoted(&entry.name),
            quoted(&entry.description),
            level_variant(entry.level)
        );
    }
    out.push_str("];\n");
    out
}

/// Every capability, as an enum, the table of what each grants, and the
/// interfaces given to every plugin.
///
/// **The enum is what the kernel writes, and the table is what it reads.** A
/// host function names the capability it needs as a variant, so a capability
/// that is removed is a compile error in every function that asked for it,
/// and `Capability::spec` is the one door from a variant to the functions and
/// the sentence the catalog gives it.
fn capabilities(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/capabilities.toml");
    // One line at a time, so the module's prose is written here as the text of
    // calls rather than as a block of doc lines this file does not own.
    for line in [
        "",
        "Every capability a plugin can be granted, and every interface of the",
        "plugin world given to every plugin. A manifest's `capabilities` names",
        "these and nothing else (`NOCTUA.md §11.3`), and the kernel's coverage",
        "test holds the list to the world: every function the world imports is",
        "in exactly one capability, or in the functions of an interface given to",
        "every plugin.",
    ] {
        if line.is_empty() {
            out.push_str("//!\n");
        } else {
            let _ = writeln!(out, "//! {line}");
        }
    }
    out.push('\n');
    let mut sorted = catalog.capabilities.clone();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));

    out.push_str("/// One capability a plugin can be granted.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    out.push_str("pub enum Capability {\n");
    for entry in &sorted {
        let _ = write!(out, "    {}", doc(&format!("`{}`", entry.name)));
        let _ = writeln!(out, "    {},", camel(&entry.name));
    }
    out.push_str("}\n\n");

    out.push_str("impl Capability {\n");
    out.push_str("    /// What the catalog declares about it.\n");
    out.push_str("    #[must_use]\n");
    out.push_str("    pub const fn spec(self) -> &'static CapabilitySpec {\n");
    out.push_str("        match self {\n");
    for (index, entry) in sorted.iter().enumerate() {
        let _ = writeln!(
            out,
            "            Self::{} => &CAPABILITIES[{index}],",
            camel(&entry.name)
        );
    }
    out.push_str("        }\n    }\n}\n\n");

    out.push_str("/// What the catalog declares about one capability.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct CapabilitySpec {\n");
    out.push_str("    /// The capability itself.\n    pub capability: Capability,\n");
    out.push_str("    /// What a manifest writes to ask for it.\n    pub name: &'static str,\n");
    out.push_str("    /// The interface of the world whose functions it grants.\n    pub interface: &'static str,\n");
    out.push_str("    /// Every function of that interface it grants, as the world's parser names them.\n    pub functions: &'static [&'static str],\n");
    out.push_str("    /// The i18n key of the sentence saying what it lets a plugin do.\n    pub description_key: &'static str,\n");
    out.push_str("}\n\n");

    out.push_str("/// Every capability, in name order.\n");
    out.push_str("pub const CAPABILITIES: &[CapabilitySpec] = &[\n");
    for entry in &sorted {
        let _ = writeln!(
            out,
            "    CapabilitySpec {{ capability: Capability::{}, name: {}, interface: {}, functions: {}, description_key: {} }},",
            camel(&entry.name),
            quoted(&entry.name),
            quoted(&entry.interface),
            list(&entry.functions),
            quoted(&entry.description)
        );
    }
    out.push_str("];\n\n");
    out.push_str(
        "/// The capability a manifest names, if any.\n\
         #[must_use]\n\
         pub fn capability(name: &str) -> Option<&'static CapabilitySpec> {\n\
         \x20   CAPABILITIES.iter().find(|candidate| candidate.name == name)\n\
         }\n\n",
    );

    out.push_str("/// One interface of the world every plugin is given without asking.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\n");
    out.push_str("#[non_exhaustive]\npub struct Ungated {\n");
    out.push_str("    /// The interface.\n    pub interface: &'static str,\n");
    out.push_str("    /// Every function of that interface every plugin is given, as the world's parser names them.\n    pub functions: &'static [&'static str],\n");
    out.push_str("    /// Why giving it to every plugin grants nothing a plugin could misuse.\n    pub reason: &'static str,\n");
    out.push_str("}\n\n");
    let mut ungated = catalog.ungated.clone();
    ungated.sort_by(|a, b| a.interface.cmp(&b.interface));
    out.push_str("/// Every interface given to every plugin, in interface order.\n");
    out.push_str("pub const UNGATED: &[Ungated] = &[\n");
    for entry in &ungated {
        let _ = writeln!(
            out,
            "    Ungated {{ interface: {}, functions: {}, reason: {} }},",
            quoted(&entry.interface),
            list(&entry.functions),
            quoted(entry.reason.trim())
        );
    }
    out.push_str("];\n");
    out
}

fn i18n(catalog: &Catalog) -> String {
    let mut out = header(CommentStyle::Line("//!"), "catalog/i18n/");
    out.push_str(
        "//!\n\
         //! Every user-facing sentence, in every locale. A key missing from either\n\
         //! locale fails the build, so a lookup here either finds every locale's\n\
         //! version or the key does not exist at all.\n\
         //!\n\
         //! The table is sorted and searched rather than hashed: the crate then has\n\
         //! no map to build at startup and allocates nothing.\n\
         \n",
    );

    let locales = &catalog.locales;
    out.push_str("/// The locales this platform ships.\n");
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]\n");
    out.push_str("pub enum Locale {\n");
    for locale in locales {
        let _ = writeln!(
            out,
            "    {}    {},",
            doc(&locale.meta.name),
            camel(&locale.meta.bcp47)
        );
    }
    out.push_str("}\n\n");

    out.push_str("impl Locale {\n");
    out.push_str("    /// Every locale, source locale first.\n");
    out.push_str("    pub const ALL: &'static [Self] = &[\n");
    for locale in locales {
        let _ = writeln!(out, "        Self::{},", camel(&locale.meta.bcp47));
    }
    out.push_str("    ];\n\n");

    for (name, description, which) in [
        ("tag", "The BCP 47 tag, for the document's `lang`.", 0),
        ("name", "The language's name, written in that language.", 1),
        ("dir", "`ltr` or `rtl`, for the document's `dir`.", 2),
    ] {
        let _ = write!(
            out,
            "    /// {description}\n    #[must_use]\n    pub const fn {name}(self) -> &'static str {{\n        match self {{\n"
        );
        for locale in locales {
            let value = match which {
                0 => &locale.meta.bcp47,
                1 => &locale.meta.name,
                _ => &locale.meta.dir,
            };
            let _ = writeln!(
                out,
                "            Self::{} => {},",
                camel(&locale.meta.bcp47),
                quoted(value)
            );
        }
        out.push_str("        }\n    }\n\n");
    }

    out.push_str("    /// This locale's column in the message table.\n    #[must_use]\n    pub const fn index(self) -> usize {\n        match self {\n");
    for (index, locale) in locales.iter().enumerate() {
        let _ = writeln!(
            out,
            "            Self::{} => {index},",
            camel(&locale.meta.bcp47)
        );
    }
    out.push_str("        }\n    }\n\n");

    out.push_str(
        "    /// The locale a BCP 47 tag names, if this platform ships it.\n\
         \x20   ///\n\
         \x20   /// Compared without regard to case, because BCP 47 defines tags\n\
         \x20   /// that way: `pt-BR`, `pt-br` and `PT-BR` are one tag. An exact\n\
         \x20   /// comparison answered `None` for two of those three, and a\n\
         \x20   /// gateway reading `Accept-Language: pt-br` then serves the\n\
         \x20   /// source locale without a word.\n\
         \x20   #[must_use]\n\
         \x20   pub fn from_tag(tag: &str) -> Option<Self> {\n\
         \x20       Self::ALL\n\
         \x20           .iter()\n\
         \x20           .copied()\n\
         \x20           .find(|locale| locale.tag().eq_ignore_ascii_case(tag))\n\
         \x20   }\n\
         }\n\n",
    );

    out.push_str(&table(catalog));
    out
}

/// The sorted message table and the search over it.
fn table(catalog: &Catalog) -> String {
    let locales = &catalog.locales;
    let mut out = String::new();
    // **Not `unwrap_or_default`.** An empty key set writes an empty `MESSAGES`,
    // and every row and column of that table is built from it — so the whole
    // message surface would go silently, which is the fall-through the loop
    // below already refuses by name. `too-few-locales` is what makes it
    // unreachable: `compile` emits nothing while a problem stands, and a catalog
    // carrying fewer than two locales is a problem.
    let source = locales
        .first()
        .expect("`too-few-locales` refuses a catalog with no source locale");
    let keys: Vec<&String> = source.message.keys().collect();
    let _ = writeln!(
        out,
        "/// Every message, sorted by key. One row per key, one column per locale.\nconst MESSAGES: &[(&str, [&str; {}])] = &[",
        locales.len()
    );
    for key in &keys {
        let row = locales
            .iter()
            .map(|locale| {
                // **Not `map_or("", …)`, because no input can take that arm.**
                // `compile` returns `Err` on the first problem *before* `emit::all`
                // runs, and `messages::check` pushes `missing-key` for every source
                // key absent from a locale and `orphan-key` for every locale key
                // absent from the source. The two together force every locale's key
                // set equal on any catalog that reaches this function, and `keys`
                // here is the source locale's. An empty cell was a fallback a reader
                // believes in and a catalog cannot produce; the rule that makes it
                // impossible is named instead.
                quoted(
                    locale
                        .message
                        .get(*key)
                        .expect("`missing-key` and `orphan-key` make every locale carry every key"),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(out, "    ({}, [{row}]),", quoted(key));
    }
    out.push_str("];\n\n");

    out.push_str("/// Every message key, sorted.\n");
    out.push_str("pub const KEYS: &[&str] = &[\n");
    for key in &keys {
        let _ = writeln!(out, "    {},", quoted(key));
    }
    out.push_str("];\n\n");

    out.push_str(
        "/// The sentence a key resolves to in a locale.\n\
         ///\n\
         /// `None` means the key does not exist — never that this locale is missing\n\
         /// it, because a key missing from one locale fails the build.\n\
         #[must_use]\n\
         pub fn message(key: &str, locale: Locale) -> Option<&'static str> {\n\
         \x20   MESSAGES\n\
         \x20       .binary_search_by(|(candidate, _)| (*candidate).cmp(key))\n\
         \x20       .ok()\n\
         \x20       .map(|row| MESSAGES[row].1[locale.index()])\n\
         }\n\n",
    );
    out.push_str(RENDER);
    out
}

/// The renderer, emitted verbatim beside `message`.
///
/// **`params` was published metadata that nothing consumed.** Every registry
/// declares the placeholders its sentence interpolates, `messages.rs` refuses a
/// declaration that disagrees with either locale, and `ErrorCode::params` hands
/// the list to a caller — and there was no function anywhere in this tree that
/// turned a key and a set of values into a sentence. `NC-VAL-0001` is raised in
/// production by `MachineFlake::from_value`, and what a person would have been
/// shown is `{field} must be between {min} and {max}.` with the braces in it.
///
/// A declaration whose consumer does not exist is the `§19.3` shape: three
/// mechanisms agreeing about placeholders, and the thing they agree about
/// reaching nobody.
///
/// **And this closes the declaration's half, not the whole chain.** Nothing in
/// this repository mints an identifier in production, so `MachineFlake::from_value`
/// has no non-test caller and neither does `render`: what a person hitting
/// `NC-VAL-0001` sees today is still whatever the first thing to catch it
/// decides to print. The call now exists and refuses to be skipped quietly,
/// which is the part a library can own; the stage that first raises the code in
/// anger owns the rest. Saying which half is done is the point — a comment that
/// declared the defect fixed would send the next reader past the half that is
/// not.
///
/// It lives here rather than in a hand-written module because every file in
/// `nc-catalog` is generated, and a hand-written one beside them would be the
/// first file in that crate where "do not edit" is false.
const RENDER: &str = r#"/// The sentence a key resolves to in a locale, with its placeholders filled.
///
/// `None` means the key does not exist, exactly as `message`. The inner
/// `Err` is a placeholder no value was supplied for — rendering refuses
/// rather than leaving the braces in a sentence somebody reads. `§9.9` is
/// "one key per complete message, with interpolated parameters", and a
/// literal `{field}` is the half-rendered state that rule exists to forbid:
/// visibly broken to a reader, and silently fine to a caller that never
/// looked.
///
/// A caller that passes what the registry's `params` names cannot reach that
/// `Err`, because the compiler refuses a declaration that disagrees with
/// either locale's sentence.
///
/// **The refusal is the placeholder alone, and not a struct carrying the key
/// too.** The key is the caller's own first argument, so a type pairing them
/// would hand back half of what it was given — and it would put two public
/// fields into this crate that no registry decides, which is a category the
/// generated-value gate has no way to check and would have to be told to
/// ignore. The name is borrowed from the message table, so it costs nothing.
///
/// Values are a slice rather than a map so this stays allocation-free until
/// it builds the result, and so the caller writes them in the order the
/// sentence reads.
#[must_use]
pub fn render(
    key: &str,
    locale: Locale,
    values: &[(&str, &str)],
) -> Option<Result<String, &'static str>> {
    Some(fill(message(key, locale)?, values))
}

/// `fill`, for a test that has to drive the real thing.
///
/// **The gate that compares this with the compiler's scanner ran a hand-written
/// copy of it.** `render` takes a catalog *key*, so a test over arbitrary
/// sentences could not reach the real loop — and the mirror it used instead was
/// guarded by three `contains` assertions over the generated file. Measured: the
/// emitted `fill` changed to refuse an unmatched `{`, which this crate's own doc
/// forbids, and `cargo xtask check` passed with the corpus green; applying the
/// identical edit to the mirror made the corpus fail immediately. The corpus was
/// right and the wiring was not.
///
/// `nc-journal` opens the same kind of door for `unavailable`, for the same
/// reason: a mapping a test cannot reach is a mapping nothing checks.
#[doc(hidden)]
pub fn fill_for_tests<'a>(sentence: &'a str, values: &[(&str, &str)]) -> Result<String, &'a str> {
    fill(sentence, values)
}

/// Substitutes `{name}` for the value supplied under `name`.
///
/// The error is the placeholder that had no value.
///
/// **A brace pair holding anything but a bare name is prose**, copied through
/// untouched — a JSON fragment in an example, a set written out, `{}` itself.
/// This is the compiler's rule, character for character: `messages.rs`'s
/// `placeholders` accepts `[A-Za-z0-9_]+` and nothing else, and its own test
/// blesses `{"type": "object"}`, `{}` and `{ spaced }` as legitimate sentence
/// text.
///
/// **The first draft did not have this, and claimed in this comment that it
/// did.** Taking everything between `{` and the next `}` as a name made the
/// renderer refuse a whole sentence the compiler had just validated:
///
/// ```text
/// message = Some("… such as {\"table\": \"journal_entries\"}.")
/// render  = Some(Err("\"table\": \"journal_entries\""))
/// ```
///
/// `{{name}}` — a translator reaching for Handlebars escaping — was the shape
/// most likely to arrive in real life, and it slipped both sides: the compiler
/// read it as prose and the renderer refused `{name`. One predicate, written
/// once here and once there, is the reason this is a copied rule rather than a
/// second opinion.
///
/// An unmatched `{` is text too, for the same reason: no locale can be trusted
/// never to hold one, and refusing would make such a sentence unrenderable.
fn fill<'a>(sentence: &'a str, values: &[(&str, &str)]) -> Result<String, &'a str> {
    let mut out = String::with_capacity(sentence.len());
    let mut rest = sentence;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push('{');
            rest = after;
            continue;
        };
        let name = &after[..close];
        if !is_a_placeholder(name) {
            // **Past the closing brace, which is where the compiler goes.** This
            // advanced past the `{` alone and rescanned inside, so the two agreed
            // on the predicate and disagreed on the loop: `{{name}}` — a
            // translator reaching for Handlebars escaping, and the shape this
            // comment calls the likeliest to arrive — was prose to the compiler
            // and `Err("name")` here. Brute-forced over every string of length
            // eight in `{`, `}`, `a`, `b`, `"`: 37,546 sentences where the
            // renderer demanded more than the compiler declared, and none the
            // other way, so the harm was a build break blaming the registry for a
            // divergence in the scan.
            out.push('{');
            out.push_str(&after[..=close]);
            rest = &after[close + 1..];
            continue;
        }
        match values.iter().find(|(supplied, _)| *supplied == name) {
            Some((_, value)) => out.push_str(value),
            None => return Err(name),
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// What a brace pair has to hold to be a placeholder rather than prose.
///
/// The same predicate `nc-catalog-compiler`'s `placeholders` applies when it
/// decides what a sentence declares. The two have to agree, or the compiler
/// validates a sentence the renderer refuses.
fn is_a_placeholder(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}
"#;

const fn kind_variant(kind: FieldType) -> &'static str {
    match kind {
        FieldType::U64 => "U64",
        FieldType::I64 => "I64",
        FieldType::U32 => "U32",
        FieldType::Bool => "Bool",
        FieldType::Duration => "Duration",
        FieldType::ErrorCode => "ErrorCode",
        FieldType::Id => "Id",
        FieldType::Snowflake => "Snowflake",
        FieldType::Literal => "Literal",
        FieldType::Enum => "Enum",
    }
}

const fn level_variant(level: crate::model::Level) -> &'static str {
    match level {
        crate::model::Level::Trace => "Trace",
        crate::model::Level::Debug => "Debug",
        crate::model::Level::Info => "Info",
        crate::model::Level::Warn => "Warn",
        crate::model::Level::Error => "Error",
    }
}

fn option<T: Into<u64> + Copy>(value: Option<T>) -> String {
    value.map_or_else(
        || "None".to_owned(),
        |value| format!("Some({})", number(value)),
    )
}

/// A numeric literal with underscore separators.
///
/// Not decoration: clippy's `unreadable_literal` is on under `pedantic`, and a
/// generated crate that fails the workspace's own lints is a crate nobody can
/// make green without editing generated output.
fn number<T: Into<u64> + Copy>(value: T) -> String {
    let digits = value.into().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            out.push('_');
        }
        out.push(digit);
    }
    out
}

fn list(values: &[String]) -> String {
    let rendered = values
        .iter()
        .map(|value| quoted(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{rendered}]")
}

#[cfg(test)]
mod tests {
    use super::{
        an_article_for, argument_names, kind_variant, level_variant, number, on_lockout_variant,
    };
    use crate::model::{FieldType, Level};

    /// Every field kind's label takes the article this file writes for it.
    ///
    /// The fourth one-line-per-variant table, and the arm it exists for is the
    /// one no catalog reaches: `Id` is the only label beginning with a vowel and
    /// `load.rs` refuses `kind = "id"`. Written out per variant rather than
    /// derived from the rule, which would be the same predicate on both sides
    /// and agree with itself however it changed. See [`an_article_for`].
    #[test]
    fn every_labels_article_is_the_one_it_reads_with() {
        let mut seen = 0_usize;
        for (kind, article) in [
            (FieldType::U64, "A"),
            (FieldType::I64, "A"),
            (FieldType::U32, "A"),
            (FieldType::Bool, "A"),
            (FieldType::Duration, "A"),
            (FieldType::ErrorCode, "A"),
            (FieldType::Id, "An"),
            (FieldType::Snowflake, "A"),
            (FieldType::Literal, "A"),
            (FieldType::Enum, "A"),
        ] {
            assert_eq!(
                an_article_for(kind.label()),
                article,
                "`{kind:?}` writes `{} {}.` into a generated file nobody may edit",
                an_article_for(kind.label()),
                kind.label()
            );
            seen += 1;
        }
        assert_eq!(
            seen, 10,
            "this table stopped naming every kind, so the arm the registry cannot reach \
             is once again reached by nothing"
        );
        // And the rule over the vowel it is written for, rather than over the
        // one label that happens to carry one: a repair narrowed to `Id` alone
        // would pass the table above and be the identity on every kind added
        // after it.
        for (label, article) in [
            ("apple", "An"),
            ("egg", "An"),
            ("ingot", "An"),
            ("opaque identifier", "An"),
            ("underscore", "An"),
            ("count", "A"),
            ("flag", "A"),
        ] {
            assert_eq!(an_article_for(label), article, "{label}");
        }
    }

    /// A command that declares no arguments publishes none.
    ///
    /// **The other arm of this file that no catalog reaches.** Both shipped
    /// commands declare `properties`, so whatever the empty-schema arm returns
    /// becomes `Command::arguments` for a command nobody has written yet:
    /// measured on 2026-09-08, `return Vec::new()` replaced by
    /// `return vec!["ghost".to_owned()]`, `ok 15 generated file(s) match the
    /// catalog`, `check passed` at `ok 1219 test(s)`. `args_schema` is also the
    /// AI tool interface (`§11.4`), and `arguments` is what a dispatcher and the
    /// declared-command tests read — the first argument-less command would
    /// publish a phantom one.
    #[test]
    fn a_schema_with_no_arguments_declares_none() {
        assert_eq!(
            argument_names(r#"{"type":"object","additionalProperties":false}"#),
            Vec::<String>::new(),
            "a command that takes no arguments publishes one anyway"
        );
        assert_eq!(
            argument_names(r#"{"type":"object","properties":{},"additionalProperties":false}"#),
            Vec::<String>::new(),
            "an empty `properties` is no arguments, not one"
        );
        // The floor: a reader that answered nothing for everything would pass
        // both assertions above by refusing to read at all.
        assert_eq!(
            argument_names(
                r#"{"type":"object","properties":{"since":{"type":"integer"},"until":{"type":"integer"}},"additionalProperties":false}"#
            ),
            vec!["since".to_owned(), "until".to_owned()],
            "the arguments a schema declares are not read back in declaration order"
        );
        // And what `load` has already refused, which reaches here as no
        // arguments rather than as a panic.
        assert_eq!(argument_names("not json"), Vec::<String>::new());
    }

    /// Every field kind generates the variant it names, all ten.
    ///
    /// **The third of the four one-line-per-variant tables `model.rs` names**,
    /// and it sits here: `kind_variant` is what writes `FieldKind::Id` into the
    /// generated crate, and the registry declares 6 of these 10 kinds — so four
    /// arms are dead and the drift comparison, which reads the emitter against
    /// its own output, cannot see them. Measured, wrong strings on those four:
    /// `check passed`, byte-identical. The same corruption on a used kind gives
    /// `2 generated file(s) out of sync` and EXIT=1.
    ///
    /// Exhaustive, so a variant added to `FieldType` is a compile error here
    /// rather than a row that quietly stops being checked.
    #[test]
    fn every_field_kind_generates_the_variant_it_names() {
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
                FieldType::U64 => "U64",
                FieldType::I64 => "I64",
                FieldType::U32 => "U32",
                FieldType::Bool => "Bool",
                FieldType::Duration => "Duration",
                FieldType::ErrorCode => "ErrorCode",
                FieldType::Id => "Id",
                FieldType::Snowflake => "Snowflake",
                FieldType::Literal => "Literal",
                FieldType::Enum => "Enum",
            };
            assert_eq!(
                kind_variant(kind),
                meant,
                "`{kind:?}` generates the variant `{}`, which is not a name anything \
                 downstream can write",
                kind_variant(kind)
            );
        }
    }

    /// Every level generates the variant it names, all five.
    ///
    /// **The fourth arm of this shape, and the one the test above did not
    /// import.** `level_variant` sits fifteen lines below `kind_variant`, is the
    /// same one-line-per-variant `match` over strings, and `catalog/
    /// log-events.toml` declares no `trace` event — so that arm is unreachable
    /// from the registry, and the drift comparison, which reads the emitter
    /// against its own generated output, cannot see it either. Measured,
    /// `Level::Trace => "Traec"`:
    ///
    /// ```text
    /// cargo xtask check  ->  check passed, every verdict line identical
    /// ```
    ///
    /// A generated `Level::Traec` is a variant nobody downstream can name, and
    /// the day the registry declares its first trace event the generated crate
    /// stops compiling — with the mistake fifteen lines from a test written for
    /// exactly it.
    ///
    /// Exhaustive, so a level added to `Level` is a compile error here rather
    /// than a row that quietly stops being checked.
    #[test]
    fn every_level_generates_the_variant_it_names() {
        for level in [
            Level::Trace,
            Level::Debug,
            Level::Info,
            Level::Warn,
            Level::Error,
        ] {
            let meant = match level {
                Level::Trace => "Trace",
                Level::Debug => "Debug",
                Level::Info => "Info",
                Level::Warn => "Warn",
                Level::Error => "Error",
            };
            assert_eq!(
                level_variant(level),
                meant,
                "`{level:?}` generates the variant `{}`, which is not a name anything \
                 downstream can write",
                level_variant(level)
            );
        }
    }

    /// Digits are grouped in threes, from the right.
    ///
    /// **The one rule in the emitter that nothing read.** `% 3` is what makes a
    /// generated literal readable *and* what makes it pass `unreadable_literal`,
    /// which is on under `pedantic` for the generated crate; `% 4` compiles, is
    /// formatted by rustfmt without complaint, and passes the whole gate once
    /// `cargo xtask build` has written it to disk — the drift check compares the
    /// emitter against its own output, so the two agree by construction:
    ///
    /// ```text
    /// crates/nc-catalog/src/limits.rs:  max_disk_bytes: 85_8993_4592,
    ///   cargo xtask build  ->  ok  15 generated file(s), 1 rewritten
    ///   cargo xtask check  ->  check passed
    /// ```
    ///
    /// A reader checking that number against `limits.toml` reads eight and a
    /// half *billion* as eighty-five *billion*, and the value is the disk the
    /// log store is allowed to fill.
    #[test]
    fn digits_are_grouped_in_threes_from_the_right() {
        assert_eq!(number(0_u32), "0");
        assert_eq!(number(999_u32), "999");
        // The first separator, and the boundary `% 4` moves.
        assert_eq!(number(1_000_u32), "1_000");
        assert_eq!(number(65_536_u32), "65_536");
        // The value that named the defect: `% 4` renders it `85_8993_4592`.
        assert_eq!(number(8_589_934_592_u64), "8_589_934_592");
        // A leading group of one, which is where an off-by-one in the index
        // shows up as `_8_589_934_592` or as `8589_934_592`.
        assert_eq!(number(1_234_567_u32), "1_234_567");
        assert_eq!(number(u64::MAX), "18_446_744_073_709_551_615");
    }
    /// Every lockout the model can spell generates the variant it names.
    ///
    /// See [`super::on_lockout_variant`] for the arm no catalog reaches and what
    /// it costs. Driven through the real function, one case per variant, the
    /// way `docs.rs`'s `every_lockout_is_stated_as_what_it_does` drives its
    /// sibling.
    ///
    /// The expectation is written as an exhaustive `match`, so a third variant
    /// added to the model is a compile error here rather than a row this test
    /// silently stops covering.
    #[test]
    fn every_lockout_generates_the_variant_it_names() {
        use crate::model::OnLockout;

        for on in [OnLockout::Refuse, OnLockout::WipeCredential] {
            let named = match on {
                OnLockout::Refuse => "Refuse",
                OnLockout::WipeCredential => "WipeCredential",
            };
            assert_eq!(
                on_lockout_variant(Some(on)),
                format!("Some(OnLockout::{named})"),
                "`{on:?}` generates a variant name the generated crate does not declare, \
                 so `cargo xtask build` would write a file that does not compile"
            );
        }
        // And the absence of one, which is what every budget but `device_pin`
        // declares — and the value the four `lockout_after` rows without an
        // `on_lockout` are emitted with.
        assert_eq!(on_lockout_variant(None), "None");
    }
}
