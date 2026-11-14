//! Reads `catalog/` and validates it.
//!
//! Every check in here returns a `Problem` rather than stopping, and the caller
//! gets all of them at once. A compiler that reports one problem per run turns a
//! ten-minute edit into ten runs, and the registries are edited in batches by
//! their nature — a stage adds a dozen error codes, not one.
//!
//! # Two classes of check, and why the second exists
//!
//! The first class is about the platform: a code that does not match the scheme,
//! a table with no bound, a command that reads and mutates. Those are the ones
//! `NOCTUA.md` asks for.
//!
//! The second class is about **what the emitter will do with the value**, and
//! without it the failure lands where nobody can act on it. Two error codes
//! whose message keys differ only in a hyphen produce one Rust variant name, and
//! unchecked, `cargo xtask build` would report `ok` before rustc failed inside a
//! generated file whose header forbids editing it — with no line in any
//! registry and no fix.
//! A notification called `2fa.enabled` does worse: rustfmt refuses the emitted
//! source and the location it gives is a byte offset into a stream the developer
//! cannot open. So every name that becomes an identifier is checked **here**,
//! where the file and the line exist.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::de::DeserializeOwned;

use crate::model::{
    Budget, CapabilityEntry, CapabilityFile, Catalog, CommandEntry, CommandFile, DaemonLimits,
    Email, EmailFile, ErrorFile, EventEntry, EventFile, KernelLimits, Limits, Locale, LocaleFile,
    Notification, NotificationFile, Reversibility, Strategy, Table, UngatedEntry,
};
use crate::problem::{Problem, line_of};

/// The locale every other locale is checked against, and the one a key must be
/// added to first.
pub const SOURCE_LOCALE: &str = "en";

/// Reads and validates the whole catalog.
///
/// # Errors
///
/// Returns every problem found, located, rather than the first.
#[must_use]
pub fn load(root: &Path) -> (Option<Catalog>, Vec<Problem>) {
    let mut problems = Vec::new();

    let errors_text = read(root, "catalog/errors.toml", &mut problems);
    let notifications_text = read(root, "catalog/notifications.toml", &mut problems);
    let emails_text = read(root, "catalog/emails.toml", &mut problems);
    let limits_text = read(root, "catalog/limits.toml", &mut problems);
    let commands_text = read(root, "catalog/commands.toml", &mut problems);
    let events_text = read(root, "catalog/log-events.toml", &mut problems);
    let capabilities_text = read(root, "catalog/capabilities.toml", &mut problems);

    let (
        Some(errors_text),
        Some(notifications_text),
        Some(emails_text),
        Some(limits_text),
        Some(commands_text),
        Some(events_text),
        Some(capabilities_text),
    ) = (
        errors_text,
        notifications_text,
        emails_text,
        limits_text,
        commands_text,
        events_text,
        capabilities_text,
    )
    else {
        return (None, problems);
    };

    let error_file: Option<ErrorFile> = parse("catalog/errors.toml", &errors_text, &mut problems);
    let notification_file: Option<NotificationFile> = parse(
        "catalog/notifications.toml",
        &notifications_text,
        &mut problems,
    );
    let email_file: Option<EmailFile> = parse("catalog/emails.toml", &emails_text, &mut problems);
    let limits: Option<Limits> = parse("catalog/limits.toml", &limits_text, &mut problems);
    let command_file: Option<CommandFile> =
        parse("catalog/commands.toml", &commands_text, &mut problems);
    let event_file: Option<EventFile> =
        parse("catalog/log-events.toml", &events_text, &mut problems);
    let capability_file: Option<CapabilityFile> = parse(
        "catalog/capabilities.toml",
        &capabilities_text,
        &mut problems,
    );

    let locales = locales(root, &mut problems);

    let (
        Some(error_file),
        Some(notification_file),
        Some(email_file),
        Some(limits),
        Some(command_file),
        Some(event_file),
        Some(capability_file),
    ) = (
        error_file,
        notification_file,
        email_file,
        limits,
        command_file,
        event_file,
        capability_file,
    )
    else {
        return (None, problems);
    };

    check_errors(&error_file, &errors_text, &mut problems);
    check_notifications(
        &notification_file.notifications,
        &notifications_text,
        &mut problems,
    );
    check_emails(&email_file.emails, &emails_text, &mut problems);
    check_limits(&limits, &limits_text, &mut problems);
    check_commands(&command_file.commands, &commands_text, &mut problems);
    check_events(&event_file.events, &events_text, &mut problems);
    check_capabilities(&capability_file, &capabilities_text, &mut problems);

    let mut catalog = Catalog {
        areas: error_file.areas,
        errors: error_file.errors,
        notifications: notification_file.notifications,
        emails: email_file.emails,
        limits,
        commands: command_file.commands,
        events: event_file.events,
        capabilities: capability_file.capabilities,
        ungated: capability_file.ungated,
        locales,
        wiki: Vec::new(),
    };
    catalog.wiki = crate::metadata::wiki(root, &catalog, &mut problems);

    check_one_owner_per_key(&catalog, &mut problems);
    crate::messages::check(root, &catalog, &mut problems);
    crate::metadata::check(root, &catalog, &commands_text, &mut problems);

    problems.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    (Some(catalog), problems)
}

fn read(root: &Path, relative: &str, problems: &mut Vec<Problem>) -> Option<String> {
    match std::fs::read_to_string(root.join(relative)) {
        Ok(text) => Some(text),
        Err(error) => {
            problems.push(Problem::in_file(
                relative,
                format!("could not be read: {error}"),
                "every registry is required, and an absent one is not an empty one: \
                 a missing file would silently remove every code, message and limit it \
                 declared",
                "registry-unreadable",
            ));
            None
        }
    }
}

fn parse<T: DeserializeOwned>(
    relative: &str,
    text: &str,
    problems: &mut Vec<Problem>,
) -> Option<T> {
    match toml::from_str::<T>(text) {
        Ok(parsed) => Some(parsed),
        Err(error) => {
            let line = error
                .span()
                .map(|span| text[..span.start.min(text.len())].lines().count().max(1));
            problems.push(Problem {
                file: relative.to_owned(),
                line,
                what: error.message().to_owned(),
                fix: "the registry's shape is fixed by the compiler, and it refuses a \
                      field it does not know: a typo in a key is a control that silently \
                      stops applying. The comment at the top of the file states what each \
                      entry carries"
                    .to_owned(),
                check: "registry-malformed",
            });
            None
        }
    }
}

/// Whether a name survives becoming a Rust identifier.
///
/// Checked here rather than discovered by rustc inside a generated file.
///
/// **Every keyword, in both cases this function is handed.** A list written for
/// `camel`-cased names — `Self`, `Super`, `Crate`, `Box`, `Type`, `Fn` — and
/// compared case-sensitively misses its *second* caller, which hands it
/// `crate::snake(&field.name)`. Through such a list `type`, the plainest name a
/// log field can have, reaches the emitter untouched and the build dies inside
/// rustfmt:
///
/// ```text
/// error rustfmt refused the emitted source: error: expected identifier, found keyword `type`
///          --> <stdin>:170:9
///       168 | pub struct JournalVerified {
///       170 |     pub type: u64,
///       …. Its positions are offsets into a stream, not into a file
/// ```
///
/// which is the outcome this module's header says these checks exist to
/// prevent, and its own message admits the position is useless. `ref`, `match`,
/// `move`, `in`, `use`, `mod`, `let`, `loop`, `const`, `static`, `mut`, `async`,
/// `await`, `dyn` and `where` are all the same, and several of them are
/// plausible names for a field.
///
/// So the list is Rust's, plus the prelude names a `camel`-cased name can
/// take — which is what `Box` stands for, and what would otherwise need a
/// second list in the log-event registry. Compared exactly, because
/// `camel` and `snake` have already decided the case: a variant called `Self`
/// generates `Self` and is refused, and one called `self` generates `Self` too.
fn identifier(name: &str) -> Result<(), String> {
    /// Every word Rust will not take as an identifier, and the prelude names a
    /// generated type would shadow.
    ///
    /// Strict and reserved keywords from the 2015, 2018 and 2024 editions, plus
    /// `Self`; then the prelude. `crate`, `super` and `self` appear in their own
    /// spelling and in `camel`'s, because both callers reach this.
    const RESERVED: [&str; 69] = [
        "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
        "mut", "pub", "ref", "return", "self", "Self", "static", "struct", "super", "trait",
        "true", "type", "union", "unsafe", "use", "where", "while", "abstract", "become", "box",
        "do", "final", "gen", "macro", "override", "priv", "try", "typeof", "unsized", "virtual",
        "yield", "Box", "Option", "Result", "String", "Vec", "Some", "None", "Ok", "Err",
        "Iterator", "Default", "Clone", "Copy", "Drop", "From", "Into",
    ];

    if name.is_empty() {
        return Err("becomes an empty identifier".to_owned());
    }
    if name.starts_with(|character: char| character.is_ascii_digit()) {
        return Err(format!("becomes `{name}`, which starts with a digit"));
    }
    if let Some(bad) = name
        .chars()
        .find(|character| !(character.is_ascii_alphanumeric() || *character == '_'))
    {
        return Err(format!("becomes `{name}`, which holds `{bad}`"));
    }
    if RESERVED.contains(&name) {
        return Err(format!(
            "becomes `{name}`, which Rust reserves or the prelude already names"
        ));
    }
    Ok(())
}

/// Registers a generated identifier, reporting a collision against what claimed
/// it first.
struct Names<'a> {
    file: &'a str,
    text: &'a str,
    /// The key each entry of this registry is written under.
    ///
    /// **Not a bare substring**, because every name in these registries appears
    /// in more than one place: `level` is a field of every event, `event` opens
    /// every table, `field` names a column. Measured with a bare substring, by
    /// appending an event to `catalog/log-events.toml`, a 182-line file, and
    /// reading the line each collision reported:
    ///
    /// ```text
    /// `level` written on line 185, reported at 75   -> `level = "debug"`
    /// `event` written on line 185, reported at 72   -> `[[event]]`
    /// `field` written on line 185, reported at 76   -> `fields = [`
    /// ```
    ///
    /// The author is sent a hundred lines up, into an entry they never touched.
    /// Every sibling rule in this file anchors — `code = "…"`, `name = "…"` —
    /// and this module's own header is *"every name that becomes an identifier
    /// is checked here, where the file and the line exist"*.
    anchor: &'a str,
    taken: BTreeMap<String, String>,
}

/// Whether a value says nothing a reader would see.
///
/// **Not `trim().is_empty()`, which the sibling rule in this crate does not
/// ask either.** `messages::no_key_is_blank` asks
/// [`crate::tells_two_sentences_apart`] because *"a value of three `U+2063
/// INVISIBLE SEPARATOR` characters was accepted: the published cell is empty"* —
/// and every rule here asking `trim().is_empty()` is one code point away from
/// the same thing. Measured that way against the shipped catalog, one value at a
/// time replaced by a single `U+2063`: the `[areas]` description, a command's
/// `name`, a limit's `surface`, a store's `enforced_by`, a log event's
/// `description` and a limit's `order_by` are all **accepted with no problem at
/// all**, and `surface` carries through to two published budget rows for a
/// surface with no name — verbatim the outcome `rate-limit-unnamed` refuses one
/// spelling away.
///
/// Narrower than [`crate::shows_nothing`] on purpose, for the reason
/// `tells_two_sentences_apart` gives: the characters in `BLANK_RANGES` sit
/// inside ordinary words, so a rule built on the wider set would refuse values
/// this catalog legitimately holds.
fn says_nothing(value: &str) -> bool {
    !value
        .chars()
        .any(|shown| !shown.is_whitespace() && crate::tells_two_sentences_apart(shown))
}

/// The identifiers `crates/nc-log/src/events.rs` already holds before a single
/// event is written into it.
///
/// **A registry name has to differ from more than the other registry names**:
/// the file it generates opens with imports of its own:
///
/// ```text
/// use nc_catalog::Level;
/// use crate::{Event, Field, LogValue as _};
/// ```
///
/// So without this an event called `field` generates `pub struct Field`,
/// `cargo xtask build` says `ok  15 generated file(s), 3 rewritten`, and the next
/// `cargo build` fails *inside generated source* — the exact outcome this module's header
/// says these checks exist to prevent:
///
/// ```text
/// error[E0255]: the name `Field` is defined multiple times
///   --> crates/nc-log/src/events.rs:44:1
///    |
/// 14 | use crate::{Event, Field, LogValue as _};
///    |                    ----- previous import of the type `Field` here
/// ```
///
/// `event` and `level` collide the same way. `vec` and `option` are worse — they
/// shadow the prelude rather than colliding, so the failure surfaces as an
/// inscrutable `E0107` on the `fields` method written for every event — and
/// they are refused one rule earlier, by `identifier`, which knows the prelude.
///
/// Written down rather than parsed out of the emitter: the preamble is four
/// lines of a string literal in `emit.rs`, and a reader that recovered these
/// names from it would agree with whatever that literal said, including after
/// somebody removed an import.
///
/// **`LogValue` is not here, because it is not a collision.** The preamble
/// imports it `as _`, which brings the trait in for method resolution and binds
/// no name — so a generated `pub struct LogValue` compiles. Measured with
/// exactly that appended to `crates/nc-log/src/events.rs`: `cargo build -p
/// nc-log` finishes and clippy says nothing, while the same line written
/// `pub struct Field` gives `error[E0255]: the name `Field` is defined multiple
/// times`. Listed, it would refuse a name rustc takes — the *too strong* half of
/// the shape `OWNED_BY_THE_LOG` guards against with
/// `every_name_here_needs_this_list_to_be_refused`. A fixture that asserts the
/// compiler refuses these names drives the rule rather than the hole, which is
/// a restatement of the constant.
///
/// What holds it is `every_import_here_is_a_name_the_preamble_binds`,
/// which reads the emitter's preamble and asks whether each entry is a name it
/// actually binds. The list stays hand-written — a reader that recovered the
/// names would agree with whatever the literal said — and the *check* is
/// against the literal, which is the half that can disagree.
const ALREADY_IN_THE_GENERATED_FILE: [&str; 3] = ["Event", "Field", "Level"];

impl<'a> Names<'a> {
    fn new(file: &'a str, anchor: &'a str, text: &'a str) -> Self {
        Self {
            file,
            text,
            anchor,
            taken: BTreeMap::new(),
        }
    }

    /// A `Names` that also refuses what the generated file already imports.
    ///
    /// Only `log-events.toml` needs it: it is the registry whose entries become
    /// *types* in a file with imports of its own. The other three generate
    /// enum variants and constants, which do not collide with a `use`.
    fn in_the_log_events_file(file: &'a str, text: &'a str) -> Self {
        let mut names = Self::new(file, "name", text);
        for held in ALREADY_IN_THE_GENERATED_FILE {
            names
                .taken
                .insert((*held).to_owned(), format!("the import `{held}`"));
        }
        names
    }

    /// Checks `generated` and records it against `owner`.
    fn claim(&mut self, owner: &str, generated: &str, problems: &mut Vec<Problem>) {
        // The entry's own key first, and the bare name only if this registry
        // does not write one — so a differently shaped file still gets a line
        // rather than none, and the ordinary one gets the right line.
        // **Both spacings, and no fallback to the bare name.** The anchored
        // form is `name = "x"`, which is what `cargo fmt`'s equivalent for TOML
        // writes and what every registry here holds; a file written `name="x"`
        // is legal TOML, and falling back to the bare needle for it restores
        // exactly the misdirection this anchoring prevents — `level` reported a
        // hundred lines above the entry that holds it. When neither spelling is
        // found the honest answer is no line at all, which every diagnostic in
        // this file already renders as a bare filename.
        let line = [
            format!("{} = \"{owner}\"", self.anchor),
            format!("{}=\"{owner}\"", self.anchor),
        ]
        .into_iter()
        .find_map(|shape| line_of(self.text, &shape, 1))
        // The bare name last, because a registry holds names this anchoring
        // cannot see — a field's, written inside its own table — and a line
        // in the right file beats none at all. What it may not do is answer
        // *first*, which is what sent `level` a hundred lines above the
        // entry that holds it.
        .or_else(|| line_of(self.text, owner, 1));
        if let Err(why) = identifier(generated) {
            push(
                problems,
                self.file,
                line,
                format!("`{owner}` {why}"),
                "the compiler turns this name into a Rust item, and rustc would report \
                 the failure inside a generated file whose header forbids editing it — \
                 with no line here and no fix. Use letters, digits, `.`, `-` and `_`, \
                 and do not start with a digit",
                "name-not-an-identifier",
            );
            return;
        }
        if let Some(first) = self.taken.get(generated) {
            push(
                problems,
                self.file,
                line,
                format!("`{owner}` and {first} both become `{generated}`"),
                "they would generate one Rust item twice, and the build would fail inside \
                 generated source. Note that `-` and `_` become the same thing here, so \
                 two names differing only in one of them collide — and that the generated \
                 file opens with imports of its own, which a name cannot take either",
                "name-collision",
            );
            return;
        }
        self.taken
            .insert(generated.to_owned(), format!("`{owner}`"));
    }
}

/// Every key of `[areas]`, judged where it is declared.
///
/// **Read here, where they are declared.** They are the alphabet every code is
/// checked against, so a key outside it is an area no code can ever name —
/// declared, published on `docs/errors.md` as available, and otherwise refused
/// only at the far end in a message about the *code*. `P2P` is such a key unless
/// this reads it.
///
/// Its own function because inside `check_errors` it would cross clippy's line
/// limit.
fn every_area_is_one_a_code_could_name(file: &ErrorFile, text: &str, problems: &mut Vec<Problem>) {
    for (area, what_it_covers) in &file.areas {
        // **And it says what it covers.** `docs/errors.md` publishes that
        // sentence in a column whose own prose explains what it is for;
        // without this, `CANON = ""` fills it with nothing, with no diagnostic.
        if says_nothing(what_it_covers) {
            push(
                problems,
                "catalog/errors.toml",
                line_of(text, &format!("{area} "), 1).or_else(|| line_of(text, area, 1)),
                format!("`{area}` says nothing about what it covers"),
                "the area list is what a developer reads to choose one, and \
                 `docs/errors.md` publishes the description beside the prefix. An area \
                 with none is a prefix somebody has to guess the meaning of",
                "area-undescribed",
            );
        }
        if !is_an_area(area) {
            push(
                problems,
                "catalog/errors.toml",
                line_of(text, &format!("{area} "), 1).or_else(|| line_of(text, area, 1)),
                format!("`{area}` cannot be an area"),
                "an area is the middle of `NC-<AREA>-<NNNN>` (NOCTUA.md §15.1): uppercase \
                 letters and digits, and nothing else. As written, no code could ever \
                 name it — and `docs/errors.md` publishes it as one a code may use",
                "area-not-an-area",
            );
        }
    }
}

/// Every registry has to hold something.
///
/// `NOCTUA.md §19.3`'s first rule: assert that every gate's count of things
/// checked is non-zero. Without it, emptying four registries leaves the catalog
/// step green at "0 notification(s), 0 email(s), 0 command(s), 0 log event(s)".
fn floor(count: usize, file: &str, what: &str, problems: &mut Vec<Problem>) {
    if count == 0 {
        problems.push(Problem::in_file(
            file,
            format!("declares no {what}"),
            "an empty registry means every gate that reads it checks nothing, and every \
             surface generated from it documents nothing. If this is deliberate, the \
             entry that says so is still an entry",
            "empty-registry",
        ));
    }
}

// --- errors.toml ---

/// An error's message key is in the `error.` namespace.
///
/// **The namespace, because a generated name is decided here.**
/// `emit::error_variant` strips `error.` from the message key and camels
/// what is left; a key outside that namespace has no name to give. Measured
/// without this rule, a code whose message key is `probe.p2p.thing`:
/// `ACCEPTED, 0 problem(s)`, and the generated crate grows
/// `ErrorCode::ProbeP2pThing`, a public name for an error that does not read
/// as one, through a fallback no test reaches.
///
/// Refused here rather than tested there, which is this file's own rule:
/// every generated name is decided at the registry. The shipped catalog
/// uses the namespace throughout, and `error_variant` can say so.
fn the_message_key_is_in_its_namespace(
    entry: &crate::model::ErrorEntry,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    if entry.message.starts_with("error.") {
        return;
    }
    push(
        problems,
        "catalog/errors.toml",
        line,
        format!(
            "{}'s message key `{}` is outside the `error.` namespace",
            entry.code, entry.message
        ),
        "the key is a code's identity in the ledger and, with `error.` taken off it, the \
         name of its generated variant — so a key outside the namespace names a public \
         variant that does not read as an error. Write it as `error.<area>.<what>`",
        "error-message-namespace",
    );
}

fn check_errors(file: &ErrorFile, text: &str, problems: &mut Vec<Problem>) {
    floor(
        file.errors.len(),
        "catalog/errors.toml",
        "error codes",
        problems,
    );
    floor(file.areas.len(), "catalog/errors.toml", "areas", problems);
    // **The keys of `[areas]`, checked for their shape.** They are the alphabet
    // every code is checked against, and a key outside it would be an area no
    // code can ever name — declared, published as available, and refused only
    // at the far end in a message about the code.
    every_area_is_one_a_code_could_name(file, text, problems);

    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut messages: BTreeMap<&str, &str> = BTreeMap::new();
    let mut names = Names::new("catalog/errors.toml", "code", text);

    for entry in &file.errors {
        let anchor = format!("code = \"{}\"", entry.code);
        let count = seen.entry(entry.code.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            // The *n*-th occurrence: the first is the innocent one, and pointing
            // a duplicate report at it sends the reader to the wrong line.
            push(
                problems,
                "catalog/errors.toml",
                line_of(text, &anchor, *count),
                format!("{} is declared twice", entry.code),
                "a code identifies one error and nothing else; two declarations mean a \
                 log query returns two different failures under one name",
                "duplicate-code",
            );
        }
        let line = line_of(text, &anchor, 1);
        match area_of(&entry.code) {
            None => push(
                problems,
                "catalog/errors.toml",
                line,
                format!("{} is not of the form NC-<AREA>-<NNNN>", entry.code),
                "the scheme is fixed by NOCTUA.md §15.1: `NC-`, a declared area, then \
                 exactly four digits — such as NC-AUTH-0031",
                "malformed-code",
            ),
            Some(area) if !file.areas.contains_key(area) => push(
                problems,
                "catalog/errors.toml",
                line,
                format!(
                    "{} names the area `{area}`, which [areas] does not declare",
                    entry.code
                ),
                "the area list is fixed. Adding one is a decision — declare it in \
                 [areas] with what it covers, or use an area that already exists",
                "unknown-area",
            ),
            Some(_) => {}
        }
        // **Trimmed, like the two rules beside it.** `store.enforced_by` and
        // `pack_reason` in this same file trim before they test; untrimmed, one
        // space satisfies "the description is the only English an operator
        // reading the log gets" and reaches the generated struct, the `///`
        // above it and the page.
        if says_nothing(&entry.description) {
            push(
                problems,
                "catalog/errors.toml",
                line,
                format!("{} has an empty description", entry.code),
                "the description is what a developer reads in the log; without one the \
                 code is a number with no meaning attached",
                "empty-description",
            );
        }
        the_message_key_is_in_its_namespace(entry, line, problems);
        if let Some(first) = messages.insert(entry.message.as_str(), entry.code.as_str()) {
            push(
                problems,
                "catalog/errors.toml",
                line,
                format!(
                    "{} and {first} share the message \"{}\"",
                    entry.code, entry.message
                ),
                "the message key is a code's identity in the ledger and the name of its \
                 generated variant, so two codes cannot share one. Give this error a \
                 sentence of its own — if a user really should see the same words, that \
                 is one error with one code",
                "shared-message",
            );
        }
        // **Only for a key the rule above accepted.** `error_variant` is what
        // turns the key into the name being claimed, and it says plainly
        // that the namespace is there — so claiming a name for a key already
        // refused would panic inside the check that refused it.
        if entry.message.starts_with("error.") {
            names.claim(
                &entry.code,
                &crate::emit::error_variant(&entry.message),
                problems,
            );
        }
    }
}

/// The area part of a well-formed code, or `None` if the code is malformed.
///
/// **A digit is allowed in an area.** `NOCTUA.md §15.1` names the areas —
/// "`AUTH`, `ACCT`, `APPR`, `KERN`, `PLUG`, `BLOB`, `P2P`, `AI`, `GW`, `DB`,
/// `VAL`, `LOG`, …" — and `catalog/errors.toml` declares `P2P`, which
/// `docs/errors.md` publishes as available. `b'2'` is not
/// `is_ascii_uppercase`, so on letters alone the first peer-to-peer code is
/// refused as *malformed*:
///
/// ```text
/// error catalog/errors.toml:414 — NC-P2P-0001 is not of the form NC-<AREA>-<NNNN>
///       the scheme is fixed by NOCTUA.md §15.1: `NC-`, a declared area, then
///       exactly four digits — such as NC-AUTH-0031
/// ```
///
/// Whoever wrote it would be told their code does not match a shape it matches
/// exactly, while the real cause — an area the reader could not spell — sits
/// thirty lines above and is never mentioned. The four digits still fix the
/// number, and `rsplit_once` still splits at the last `-`, so nothing here
/// becomes ambiguous.
pub(crate) fn area_of(code: &str) -> Option<&str> {
    let rest = code.strip_prefix("NC-")?;
    let (area, number) = rest.rsplit_once('-')?;
    if !is_an_area(area) || number.len() != 4 || !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some(area)
}

/// Whether a word can be an area: uppercase letters and digits, at least one.
///
/// One reader, because the shape is decided in `area_of` and the `[areas]`
/// table it is checked against is validated by this — otherwise a key nobody
/// could ever use would be declared, published on `docs/errors.md` as
/// available, and only refused at the far end, in a message about the code
/// rather than about the area.
fn is_an_area(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

// --- notifications.toml and emails.toml ---

fn check_notifications(entries: &[Notification], text: &str, problems: &mut Vec<Problem>) {
    floor(
        entries.len(),
        "catalog/notifications.toml",
        "notifications",
        problems,
    );
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut names = Names::new("catalog/notifications.toml", "id", text);
    for entry in entries {
        let anchor = format!("id = \"{}\"", entry.id);
        let count = seen.entry(entry.id.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                "catalog/notifications.toml",
                line_of(text, &anchor, *count),
                format!("`{}` is declared twice", entry.id),
                "one id is one notification; the generated handle would name whichever \
                 declaration the compiler read last",
                "duplicate-notification",
            );
        }
        names.claim(&entry.id, &crate::camel(&entry.id), problems);
    }
}

fn check_emails(entries: &[Email], text: &str, problems: &mut Vec<Problem>) {
    floor(entries.len(), "catalog/emails.toml", "emails", problems);
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut names = Names::new("catalog/emails.toml", "id", text);
    for entry in entries {
        let anchor = format!("id = \"{}\"", entry.id);
        let count = seen.entry(entry.id.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                "catalog/emails.toml",
                line_of(text, &anchor, *count),
                format!("`{}` is declared twice", entry.id),
                "one id is one email; the generated handle would name whichever \
                 declaration the compiler read last",
                "duplicate-email",
            );
        }
        names.claim(&entry.id, &crate::camel(&entry.id), problems);
    }
}

// --- limits.toml ---

fn check_limits(limits: &Limits, text: &str, problems: &mut Vec<Problem>) {
    check_tables(limits, text, problems);
    check_hashing(limits, text, problems);
    check_rate_limits(limits, text, problems);
    check_kernel_nonzero(&limits.kernel, text, problems);
    check_kernel(&limits.kernel, text, problems);
    check_daemon(limits.daemon, text, problems);
    check_daemon_ipc(limits.daemon, &limits.kernel, text, problems);
    check_daemon_schedule(limits.daemon, &limits.kernel, text, problems);
    check_daemon_control(limits.daemon, text, problems);
}

/// What an IPC frame carries around a payload — its type, a dispatch's command
/// name, a refusal's code, kind and sentence — written as a bound of its own.
///
/// **Generous on purpose**: a frame refuses what it cannot hold rather than
/// cutting it, so an envelope larger than this is a frame refused, never a
/// payload half sent; `crates/nc-ipc`'s frames suite writes a dispatch's
/// answer at the kernel's whole payload and reads it back.
const IPC_ENVELOPE_BYTES: u64 = 65_536;

/// How many instances one Windows named pipe may have, unlimited apart: the
/// constant `PIPE_UNLIMITED_INSTANCES` is 255, and every number below it is a
/// bound.
///
/// **Written here and not asked of `nc-platform`**, for the reason [`A_DAY_MS`]
/// gives; a test in `crates/nc-ipc`,
/// `the_catalogs_connections_fit_the_pipes_instances`, holds the generated
/// value to `nc_platform::ipc::CONNECTIONS_MAX`.
const PIPE_INSTANCES: u64 = 254;

/// The IPC channel's limits keep the bounds outside `[daemon]` they answer to.
///
/// **A frame holds the kernel's whole payload with its envelope**, or a
/// dispatch the kernel accepts would be one the channel cannot carry; **a peer
/// is dropped only after more than two heartbeats of silence**, or one ping
/// late — a busy machine, a scheduler's pause — would drop a peer that is
/// there; and **the connections fit a pipe's instances** with two more — the
/// one always waiting for the next client, and the connection past the bound,
/// held until the next is waiting — or a daemon on Windows could not take the
/// connection its own bound allows.
fn check_daemon_ipc(
    daemon: DaemonLimits,
    kernel: &KernelLimits,
    text: &str,
    problems: &mut Vec<Problem>,
) {
    let at = |name: &str| line_of(text, &format!("{name} = "), 1).or(line_of(text, "[daemon]", 1));
    if daemon.ipc_frame_max_bytes < kernel.payload_bytes.saturating_add(IPC_ENVELOPE_BYTES) {
        push(
            problems,
            "catalog/limits.toml",
            at("ipc_frame_max_bytes"),
            format!(
                "`[daemon] ipc_frame_max_bytes` is {}, below `[kernel] payload_bytes`, {}, \
                 and the {IPC_ENVELOPE_BYTES} bytes a frame wraps a payload in",
                daemon.ipc_frame_max_bytes, kernel.payload_bytes
            ),
            "a dispatch's arguments and its answer are each one payload, carried inside a \
             frame with its type and a command's name or a refusal's sentence around it, so \
             a frame smaller than both together refuses a dispatch the kernel would accept. \
             Make `ipc_frame_max_bytes` at least `payload_bytes` and 64 KiB more",
            "ipc-frame-below-a-payload",
        );
    }
    if daemon.ipc_silent_after_ms <= daemon.ipc_heartbeat_ms.saturating_mul(2) {
        push(
            problems,
            "catalog/limits.toml",
            at("ipc_silent_after_ms"),
            format!(
                "`[daemon] ipc_silent_after_ms` is {}, which is not more than two \
                 `ipc_heartbeat_ms`, {}",
                daemon.ipc_silent_after_ms, daemon.ipc_heartbeat_ms
            ),
            "each side pings after a heartbeat of its own silence, so a peer dropped after \
             two heartbeats or fewer is dropped for one ping a busy machine delivered late. \
             Make `ipc_silent_after_ms` more than twice `ipc_heartbeat_ms`",
            "ipc-silence-within-two-heartbeats",
        );
    }
    if u64::from(daemon.ipc_connections_max).saturating_add(2) > PIPE_INSTANCES {
        push(
            problems,
            "catalog/limits.toml",
            at("ipc_connections_max"),
            format!(
                "`[daemon] ipc_connections_max` is {}, which with the two instances a \
                 listener keeps beside its connections is more than a Windows pipe's \
                 {PIPE_INSTANCES}",
                daemon.ipc_connections_max
            ),
            "on Windows each connection is an instance of the daemon's pipe, one more always \
             waits for the next client and one more is the connection past the bound, held \
             until the next is waiting, so a bound a pipe cannot hold is one the daemon \
             cannot keep there. Make `ipc_connections_max` at most 252",
            "ipc-connections-past-a-pipe",
        );
    }
}

/// The daemon's schedule keeps the kernel's bounds it runs beside.
///
/// **A pump at most as far apart as the shortest timer**, or a timer a plugin
/// may set is fired a pump late rather than within its own interval; **a
/// sweep at most as far apart as an orphan's grace**, or an orphan outlives
/// two graces before it goes; and **a stop that waits at least as long as a
/// dispatch may run**, or a stop cuts short a call tree the kernel would let
/// finish and leaves its unit for the next start to recover.
fn check_daemon_schedule(
    daemon: DaemonLimits,
    kernel: &KernelLimits,
    text: &str,
    problems: &mut Vec<Problem>,
) {
    let at = |name: &str| line_of(text, &format!("{name} = "), 1).or(line_of(text, "[daemon]", 1));
    if daemon.pump_every_ms > kernel.timer_min_interval_ms {
        push(
            problems,
            "catalog/limits.toml",
            at("pump_every_ms"),
            format!(
                "`[daemon] pump_every_ms` is {}, above `[kernel] timer_min_interval_ms`, {}",
                daemon.pump_every_ms, kernel.timer_min_interval_ms
            ),
            "a timer fires only when the daemon pumps, so pumps further apart than the \
             shortest interval a plugin may set fire that timer a pump late, every time. Make \
             `pump_every_ms` at most `timer_min_interval_ms`",
            "pump-slower-than-the-shortest-timer",
        );
    }
    if daemon.sweep_every_ms > kernel.blob_orphan_grace_ms {
        push(
            problems,
            "catalog/limits.toml",
            at("sweep_every_ms"),
            format!(
                "`[daemon] sweep_every_ms` is {}, above `[kernel] blob_orphan_grace_ms`, {}",
                daemon.sweep_every_ms, kernel.blob_orphan_grace_ms
            ),
            "an orphaned blob goes only when a sweep finds its grace over, so sweeps further \
             apart than the grace keep it for more than two graces. Make `sweep_every_ms` at \
             most `blob_orphan_grace_ms`",
            "sweep-slower-than-the-orphan-grace",
        );
    }
    if daemon.stop_drain_ms < kernel.dispatch_deadline_ms {
        push(
            problems,
            "catalog/limits.toml",
            at("stop_drain_ms"),
            format!(
                "`[daemon] stop_drain_ms` is {}, below `[kernel] dispatch_deadline_ms`, {}",
                daemon.stop_drain_ms, kernel.dispatch_deadline_ms
            ),
            "the kernel lets a call tree run until its deadline, so a stop that waits less \
             cuts short a dispatch the kernel would let finish and leaves its unit for the \
             next start to recover. Make `stop_drain_ms` at least `dispatch_deadline_ms`",
            "stop-drain-shorter-than-a-dispatch",
        );
    }
}

/// The bounds the CLI's process control waits by keep the daemon's own.
///
/// **A start that waits longer than a handshake may take**, or a daemon that
/// answers within its own bound is a start the CLI calls failed; and **a stop
/// that waits at least twice the drain**, because a stopping daemon waits
/// `stop_drain_ms` for what it still runs and as long again for its runtime's
/// last work, so a stop that waited less gives up on a daemon still ending
/// well.
fn check_daemon_control(daemon: DaemonLimits, text: &str, problems: &mut Vec<Problem>) {
    let at = |name: &str| line_of(text, &format!("{name} = "), 1).or(line_of(text, "[daemon]", 1));
    if daemon.start_within_ms <= daemon.ipc_handshake_ms {
        push(
            problems,
            "catalog/limits.toml",
            at("start_within_ms"),
            format!(
                "`[daemon] start_within_ms` is {}, not above `ipc_handshake_ms`, {}",
                daemon.start_within_ms, daemon.ipc_handshake_ms
            ),
            "a start waits for the daemon it started to answer the handshake, and the \
             handshake alone may take `ipc_handshake_ms`, so a start that waits no longer \
             calls failed a daemon that answers within its own bound. Make `start_within_ms` \
             more than `ipc_handshake_ms`",
            "start-shorter-than-a-handshake",
        );
    }
    if daemon.stop_within_ms < daemon.stop_drain_ms.saturating_mul(2) {
        push(
            problems,
            "catalog/limits.toml",
            at("stop_within_ms"),
            format!(
                "`[daemon] stop_within_ms` is {}, below twice `stop_drain_ms`, {}",
                daemon.stop_within_ms,
                daemon.stop_drain_ms.saturating_mul(2)
            ),
            "a stopping daemon waits `stop_drain_ms` for what it still runs and as long again \
             for its runtime's last work, so a stop that waits less gives up on a daemon still \
             ending well. Make `stop_within_ms` at least twice `stop_drain_ms`",
            "stop-shorter-than-the-daemons-own",
        );
    }
}

/// How far ahead of the clock a persisted mark may be before a minter reads
/// it as corrupt rather than early: `nc-ids`' plausible drift, a day.
///
/// **Written here and not asked of `nc-ids`**, because this compiler depends on
/// nothing of the workspace that is built from what it writes, and `nc-ids` is;
/// `the_lease_is_inside_the_drift_a_minter_waits_out` in `apps/daemon` holds the
/// two to each other through the generated value.
const A_DAY_MS: u64 = 24 * 60 * 60 * 1_000;

/// The daemon's limits are numbers above zero, and the mark's lease is one a
/// restart waits out rather than refuses.
///
/// **A lease of a day or more is a daemon that will not start after a crash.**
/// The ceiling is written `mark_lease_ms` past the clock, and a minter resuming
/// at a mark further ahead of the clock than a day takes it for a corrupt
/// number and refuses (`MintError::MarkOutOfRange`), so a crash right after a
/// raise would leave a profile no restart opens until somebody repairs it.
fn check_daemon(daemon: DaemonLimits, text: &str, problems: &mut Vec<Problem>) {
    let line = line_of(text, "[daemon]", 1);
    for limit in daemon.each() {
        if limit.value == 0 {
            push(
                problems,
                "catalog/limits.toml",
                line_of(text, &format!("{} = ", limit.name), 1).or(line),
                format!("`[daemon] {}` is zero", limit.name),
                "a daemon limit of zero is a bound that stops everything it counts — a \
                 lease of no time is a write before every identifier — so it is never what \
                 was meant. Give it the number the daemon should hold to (NOCTUA.md §16.2)",
                "daemon-limit-zero",
            );
        }
    }
    if daemon.mark_lease_ms >= A_DAY_MS {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!(
                "`[daemon] mark_lease_ms` is {}, which is not below a day, {A_DAY_MS}",
                daemon.mark_lease_ms
            ),
            "the ceiling is written that far past the clock, and a minter resuming at a mark \
             more than a day ahead takes it for a corrupt number and refuses, so a crash \
             right after a raise would leave a daemon that never starts again. Make \
             `mark_lease_ms` well below a day",
            "mark-lease-past-a-day",
        );
    }
}

/// The kernel's limits are numbers above zero.
fn check_kernel_nonzero(kernel: &KernelLimits, text: &str, problems: &mut Vec<Problem>) {
    let line = line_of(text, "[kernel]", 1);
    for limit in kernel.each() {
        if limit.value == 0 {
            push(
                problems,
                "catalog/limits.toml",
                line_of(text, &format!("{} = ", limit.name), 1).or(line),
                format!("`[kernel] {}` is zero", limit.name),
                "a kernel limit of zero is a plugin that can do none of what the limit \
                 counts — no memory, no fuel, no dispatch — so it is never what was meant. \
                 Give it the number the kernel should stop at (NOCTUA.md §11.3)",
                "kernel-limit-zero",
            );
        }
    }
}

/// The kernel's limits that bound each other do.
///
/// **A relation is checked where the numbers are written**, because the
/// kernel would meet it as a behaviour nobody chose. A host call allowed to run
/// longer than the call tree it is part of is a deadline the tree's own
/// deadline always reaches first, so `host_call_deadline_ms` would bound
/// nothing; a network timeout longer than the host call that waits for it is
/// the same one level down; an epoch deadline past the dispatch deadline is a
/// plugin that spins for the whole tree's time before the epoch notices; a
/// payload larger than a store value is a dispatch whose arguments a plugin
/// cannot keep as one value; a unit allowed fewer bytes than one store value
/// under the longest key is a unit that refuses a write the store would take;
/// and an event queue allowed fewer bytes than one payload under the longest
/// topic is a publish the kernel takes the arguments of and never queues.
///
/// Each relation is its own check, so the diagnostic says which one broke and
/// the two numbers it compared.
fn check_kernel(kernel: &KernelLimits, text: &str, problems: &mut Vec<Problem>) {
    let line = line_of(text, "[kernel]", 1);
    let relations = [
        (
            kernel.dispatch_deadline_ms > kernel.host_call_deadline_ms,
            format!(
                "`[kernel] dispatch_deadline_ms` is {} and `host_call_deadline_ms` is {}, \
                 so a host call may outlast the call tree it is part of",
                kernel.dispatch_deadline_ms, kernel.host_call_deadline_ms
            ),
            "a host call is one step of a call tree, and the tree's deadline is reached \
             first whenever the step's is longer — so the host-call deadline would bound \
             nothing. Make `dispatch_deadline_ms` larger than `host_call_deadline_ms`",
            "dispatch-deadline-within-a-host-call",
        ),
        (
            kernel.host_call_deadline_ms > kernel.net_timeout_ms,
            format!(
                "`[kernel] host_call_deadline_ms` is {} and `net_timeout_ms` is {}, so a \
                 network request may outlast the host call waiting for it",
                kernel.host_call_deadline_ms, kernel.net_timeout_ms
            ),
            "`net.fetch` is a host call, so a request allowed longer than the call is one \
             the host-call deadline kills the plugin over rather than one the request's \
             own timeout answers as `unavailable`. Make `host_call_deadline_ms` larger \
             than `net_timeout_ms`",
            "host-call-deadline-within-a-request",
        ),
        (
            kernel
                .epoch_tick_ms
                .checked_mul(kernel.epoch_deadline_ticks)
                .is_some_and(|epoch| epoch < kernel.dispatch_deadline_ms),
            format!(
                "`[kernel] epoch_tick_ms` × `epoch_deadline_ticks` is {} × {}, which does \
                 not come before `dispatch_deadline_ms`, {}",
                kernel.epoch_tick_ms, kernel.epoch_deadline_ticks, kernel.dispatch_deadline_ms
            ),
            "the epoch is what stops a plugin that spins, and a deadline it reaches only \
             after the whole call tree's is one that never stops anything: the tree's \
             deadline is checked between dispatches, not inside a plugin's loop. Make \
             the product smaller than `dispatch_deadline_ms`",
            "epoch-deadline-past-the-dispatch-deadline",
        ),
        (
            kernel.payload_bytes <= kernel.store_value_bytes,
            format!(
                "`[kernel] payload_bytes` is {} and `store_value_bytes` is {}, so a \
                 dispatch may carry arguments no store value can hold",
                kernel.payload_bytes, kernel.store_value_bytes
            ),
            "a plugin keeps what it is handed in its store, and a payload larger than a \
             value is one it can be given and never keep whole. Make `payload_bytes` no \
             larger than `store_value_bytes`",
            "payload-larger-than-a-store-value",
        ),
        (
            kernel.unit_bytes
                >= kernel
                    .store_value_bytes
                    .saturating_mul(2)
                    .saturating_add(kernel.store_key_bytes),
            format!(
                "`[kernel] unit_bytes` is {} and twice `store_value_bytes` + \
                 `store_key_bytes` is 2 × {} + {}, so a unit may refuse the one write it \
                 was asked to stage",
                kernel.unit_bytes, kernel.store_value_bytes, kernel.store_key_bytes
            ),
            "a unit counts the key of every write it stages, the value it writes and the \
             value it writes over, which its undo puts back, and a bound below one longest \
             key and two largest values is a write the store would take and the unit never \
             can. Make `unit_bytes` no smaller than their sum",
            "unit-smaller-than-a-store-write",
        ),
        (
            kernel.event_queue_bytes
                >= kernel.payload_bytes.saturating_add(
                    u64::try_from(nc_schema::descriptor::NAME_MAX).unwrap_or(u64::MAX),
                ),
            format!(
                "`[kernel] event_queue_bytes` is {} and `payload_bytes` + the longest topic \
                 is {} + {}, so the queue may refuse the one event it was asked to hold",
                kernel.event_queue_bytes,
                kernel.payload_bytes,
                nc_schema::descriptor::NAME_MAX
            ),
            "an event is its topic and its data, and the data may be a whole payload: a \
             queue that cannot hold one of those under the longest topic is a publish the \
             kernel accepts the arguments of and can never queue. Make `event_queue_bytes` \
             no smaller than their sum",
            "event-queue-smaller-than-an-event",
        ),
    ];
    for (holds, what, fix, check) in relations {
        if !holds {
            push(problems, "catalog/limits.toml", line, what, fix, check);
        }
    }
}

/// Every table declares a bound the pruner can act on, or a writer refuses at.
fn check_tables(limits: &Limits, text: &str, problems: &mut Vec<Problem>) {
    floor(
        limits.tables.len(),
        "catalog/limits.toml",
        "tables",
        problems,
    );
    floor(
        limits.stores.len(),
        "catalog/limits.toml",
        "stores",
        problems,
    );

    // **A store's three values have rules too.** `[tables.*]` is validated
    // seven ways and `[rate_limit]` nine; with `[stores.*]` counted and never
    // read, `retention_days = 0`, `max_disk_bytes = 0` and an empty
    // `enforced_by` each compile, generate, and publish themselves on
    // `docs/limits.md` as though they meant something. The two numbers are what
    // `cargo xtask check` compares the running log store's flags against, so a
    // zero is a bound the gate then certifies the store as honouring.
    for (name, store) in &limits.stores {
        let line = line_of(text, &format!("[stores.{name}]"), 1);
        if store.retention_days == 0 {
            push(
                problems,
                "catalog/limits.toml",
                line,
                format!("`{name}` keeps its data for no days"),
                "a retention of zero is not a bound, it is a store that is asked to \
                 discard everything the moment it arrives — and `cargo xtask check` \
                 compares the running store's `-retentionPeriod` against this number",
                "store-keeps-nothing",
            );
        }
        if store.max_disk_bytes == 0 {
            push(
                problems,
                "catalog/limits.toml",
                line,
                format!("`{name}` is given no disk at all"),
                "the disk ceiling is what `§18.5` bounds the store by, and zero is not a \
                 ceiling — give it the number the deployment actually has",
                "store-has-no-disk",
            );
        }
        if says_nothing(&store.enforced_by) {
            push(
                problems,
                "catalog/limits.toml",
                line,
                format!("`{name}` says nothing about what enforces its numbers"),
                "a limit nothing is named as enforcing is a number in a table. Name the \
                 file or the mechanism that applies it, the way the log store names \
                 `docker-compose.yml` — `xtask/tests/the_stack_enforces_the_declared_\
                 bounds.rs` reads that name and checks the thing it names",
                "store-enforced-by-nobody",
            );
        }
    }

    // The pruner needs at least one table it can actually prune. Every table on
    // `policy-pack` is one it must not touch, so a registry of nothing but those
    // leaves the pruner and its gate with no data at all — green, and proving
    // nothing.
    if !limits.tables.is_empty()
        && !limits
            .tables
            .values()
            .any(|table| table.strategy == Strategy::OldestFirst)
    {
        problems.push(Problem::in_file(
            "catalog/limits.toml",
            "declares no table the pruner can prune",
            "every table here is bounded by a policy pack, which the pruner must not \
             touch — so it has nothing to act on and its tests prove nothing. Declare \
             the row-bounded tables this platform has",
            "nothing-prunable",
        ));
    }

    for (name, table) in &limits.tables {
        let line = line_of(text, &format!("[tables.{name}]"), 1);
        if says_nothing(&table.order_by) {
            push(
                problems,
                "catalog/limits.toml",
                line,
                format!("`{name}` names no ordering column"),
                "pruning removes the oldest rows, and without a column to order by \
                 there is no oldest — name the column that increases with age",
                "no-order-column",
            );
        }
        check_one_strategy(name, table, line, problems);
        only_a_writer_counts_per_owner(name, table, line, problems);
    }
}

/// A limit counted per owner belongs to a table whose writer refuses at it.
///
/// **The pruner counts a whole table**, so `per` beside `oldest-first` is a
/// bound the pruner applies to more rows than the entry says; and a
/// `policy-pack` table has no number for it to divide. Under `refuse-at-limit`
/// the writer is what counts, and it can count one owner's rows.
fn only_a_writer_counts_per_owner(
    name: &str,
    table: &crate::model::TableLimit,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    if table.per.is_some() && table.strategy != Strategy::RefuseAtLimit {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!(
                "`{name}` counts its limit per owner and is `{}`",
                table.strategy.written()
            ),
            "`per` says whose rows a writer counts when it refuses a new one, and only a \
             `refuse-at-limit` table has a writer that refuses — the pruner counts a whole \
             table, and a policy-pack table has no number. Remove `per`, or make the table \
             `refuse-at-limit` and have its writer count that way",
            "per-owner-without-a-refusal",
        );
    }
}

/// `§15.8` says prune in *large* batches, and this is the floor.
///
/// Its own function because inside `check_one_strategy` it would cross clippy's
/// hundred-line bound — which is the ordinary cost of a clause rather than a
/// reason not to have it.
fn the_batch_is_a_large_one(
    name: &str,
    prune_batch: u64,
    max_rows: u64,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    // **And a floor, which `§15.8`'s sentence is about.** *"Prune in
    // large batches, not row by row"* is `[CLOSED]`, and with only the
    // two rules that bound the batch at zero and from above,
    // `prune_batch = 1` against `max_rows = 500_000` is a legal
    // catalog that passes the whole gate. On a table receiving writes
    // that never converges, and
    // every tick reports `Ok`; the load problem §15.8 names is
    // exactly deleting one row twenty thousand times.
    //
    // **A hundredth, and the number is a choice rather than a
    // measurement — which is why it is written out here.** At a
    // hundredth, a table at twice its limit comes back under it in a
    // hundred statements; at the measured `1` against `500_000` it
    // takes five hundred thousand. What is shipped is a tenth
    // (`50_000` of `500_000`), an order of magnitude clear of this,
    // so the rule refuses nothing this repository writes. A table
    // that genuinely wants a smaller batch says so by raising this
    // number and saying why, which is a two-place edit and a
    // reviewer's eye — the shape every other bound here takes.
    if prune_batch.saturating_mul(100) < max_rows {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!(
                "`{name}` prunes {prune_batch} row(s) at a time out of \
                     {max_rows}, which is row by row at this scale"
            ),
            "deleting 20,000 rows once is cheap and deleting one row 20,000 \
                 times is a load problem that arrives when the system is busiest, \
                 so prune in large batches (NOCTUA.md §15.8). A batch below a \
                 hundredth of the limit never converges on a table that is being \
                 written to",
            "batch-below-a-hundredth",
        );
    }
}

/// What one table's strategy requires of it.
///
/// Split from [`check_tables`] because with the written reason the
/// `policy-pack` exemption carries, like every other escape hatch in this tree,
/// it would cross a hundred lines.
fn check_one_strategy(
    name: &str,
    table: &crate::model::TableLimit,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    {
        match table.strategy {
            Strategy::PolicyPack => {
                if table.max_rows.is_some() || table.prune_batch.is_some() {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!(
                            "`{name}` is bounded by the policy pack and also names a row limit"
                        ),
                        "a global constant beside a per-realm bound is a number that will \
                         one day prune below what a pack promised (NOCTUA.md §15.8) — \
                         remove max_rows and prune_batch",
                        "pack-and-rows",
                    );
                }
                // **And a written reason, like every other escape hatch here.**
                // `policy-pack` exempts a table from the one gate `§15.8` is:
                // it declares no bound and the pruner reports it `left_alone`.
                // Measured without a reason required — a `login_attempts` table
                // with three lines of TOML enters the tree unbounded, never
                // pruned, with `2 table(s) created in the migrations were
                // checked` and `check passed`.
                //
                // `nc:bounded:` requires a non-empty reason, is tested for the
                // bare-marker case, and is counted. This is the same rule for
                // this hatch.
                //
                // **And a non-empty reason is not a reason.** Measured with only
                // `trim().is_empty()` asked, `"x"`, `"TODO"`, `"policy-pack"`,
                // `"pack_reason"` and the name of a *different* declared table
                // each compile clean — three spellings of empty are not a test
                // of a reason. The rule is `a_reason_names_what_it_exempts`,
                // which both hatches call.
                let missing = table.pack_reason.as_ref().map_or_else(
                    || Some("says no why".to_owned()),
                    |reason| {
                        crate::a_reason_names_what_it_exempts(name, reason)
                            .err()
                            .map(|why| format!("says a why that {why}"))
                    },
                );
                if let Some(missing) = missing {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!("`{name}` is exempt from the row limit and {missing}"),
                        "`strategy = \"policy-pack\"` means no number here bounds this table \
                         and the pruner never touches it (NOCTUA.md §15.8) — which is the \
                         gate this file exists to be, switched off for one table. Write \
                         `pack_reason = \"…\"` saying which pack property bounds it and what \
                         happens until one does, the way `nc:bounded:` has to",
                        "pack-without-a-reason",
                    );
                }
            }
            Strategy::OldestFirst => {
                // The other direction: a reason for an exemption that is not one
                // reads as a bound nobody applied.
                if table.pack_reason.is_some() {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!("`{name}` prunes oldest-first and gives a pack reason"),
                        "`pack_reason` explains why a table is exempt from the row limit, \
                         and this one is not exempt — it names max_rows and prune_batch, \
                         and the pruner enforces them. Remove it, or make the table \
                         `strategy = \"policy-pack\"` and mean it",
                        "reason-without-a-pack",
                    );
                }
                let (Some(max_rows), Some(prune_batch)) = (table.max_rows, table.prune_batch)
                else {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!(
                            "`{name}` prunes oldest-first without both max_rows and prune_batch"
                        ),
                        "both are required: max_rows is when pruning starts and prune_batch \
                         is how much one run removes",
                        "incomplete-row-limit",
                    );
                    return;
                };
                if prune_batch == 0 {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!("`{name}` has a prune_batch of zero"),
                        "a batch of zero is a pruner that runs forever and frees nothing; \
                         prune in large batches (NOCTUA.md §15.8)",
                        "zero-batch",
                    );
                }
                if prune_batch > max_rows {
                    push(
                        problems,
                        "catalog/limits.toml",
                        line,
                        format!("`{name}` would remove more rows than the limit it enforces"),
                        "one run would empty the table; make prune_batch a fraction of \
                         max_rows",
                        "batch-past-limit",
                    );
                }
                the_batch_is_a_large_one(name, prune_batch, max_rows, line, problems);
            }
            Strategy::RefuseAtLimit => the_refusal_is_the_bound(name, table, line, problems),
        }
    }
}

/// What `refuse-at-limit` requires: a limit, and nothing that says pruning.
///
/// Its own function because `check_one_strategy` is at clippy's hundred-line
/// bound, and because the three rules are one argument: the table is user
/// state, nothing deletes from it, and the number is where its writer refuses.
fn the_refusal_is_the_bound(
    name: &str,
    table: &crate::model::TableLimit,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    if table.pack_reason.is_some() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{name}` refuses at its limit and gives a pack reason"),
            "`pack_reason` explains why a table has no number, and this one has one: its \
             writer refuses a new row at `max_rows`. Remove it",
            "reason-without-a-pack",
        );
    }
    if table.prune_batch.is_some() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{name}` refuses at its limit and names a prune batch"),
            "the pruner never deletes from a table that refuses at its limit — its oldest \
             rows are somebody's data, or a unit's — so a batch here is a number nothing \
             applies, and it reads as a bound. \
             Remove prune_batch, or make the table `oldest-first` and mean that its oldest \
             rows may go",
            "refuse-and-batch",
        );
    }
    match table.max_rows {
        None => push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{name}` refuses at its limit and names no limit"),
            "`max_rows` is the count at which the writer refuses a new row, and without it \
             nothing bounds the table at all (NOCTUA.md §15.8)",
            "refuse-without-a-limit",
        ),
        Some(0) => push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{name}` refuses at a limit of zero"),
            "a limit of zero refuses the first row, so the table can never hold anything — \
             give it the number its writer should stop at",
            "refuse-at-zero",
        ),
        Some(_) => {}
    }
}

/// The key-derivation parameters are still key derivation.
fn check_hashing(limits: &Limits, text: &str, problems: &mut Vec<Problem>) {
    // Floors, not exact values: these are meant to be tuned as hardware changes,
    // and a check that pinned them would have to be edited every time somebody
    // did the tuning it exists to permit. What it refuses is a configuration
    // that is not key derivation any more.
    //
    // **There is no floor on `iterations`.** RFC 9106's *first* recommended
    // configuration is `t=1, m=2 GiB, p=4`, so a floor of two would reject the
    // stronger of the two configurations the catalog's own comment cites.
    // Memory is what carries the cost; a pass count is a trade against it.
    for (which, argon, memory_floor) in [
        ("password", limits.argon2id.password, 19_456),
        // The PIN is four to eight digits, so the secret is enumerable and the
        // only defence that scales is per-guess cost. It runs once, at unlock,
        // on the user's own machine.
        ("device_pin", limits.argon2id.device_pin, 65_536),
    ] {
        let line = line_of(text, &format!("[argon2id.{which}]"), 1);
        for (name, value, floor_value, why) in [
            (
                "memory_kib",
                argon.memory_kib,
                memory_floor,
                "memory is the parameter that costs an attacker with a GPU",
            ),
            (
                "iterations",
                argon.iterations,
                1,
                "zero passes is not a hash: Argon2id must make at least one pass over \
                 its memory",
            ),
            (
                "parallelism",
                argon.parallelism,
                1,
                "zero lanes is not a configuration: at least one lane has to do the \
                 derivation",
            ),
            (
                "output_length",
                argon.output_length,
                16,
                "a derived key shorter than 128 bits is collidable",
            ),
            (
                "salt_length",
                argon.salt_length,
                16,
                "a salt shorter than 128 bits repeats across users",
            ),
        ] {
            if value < floor_value {
                push(
                    problems,
                    "catalog/limits.toml",
                    line,
                    format!("argon2id.{which} {name} is {value}, below the floor of {floor_value}"),
                    why,
                    "hashing-below-floor",
                );
            }
        }
    }
}

/// Every authentication surface escalates, on both budgets.
fn check_rate_limits(limits: &Limits, text: &str, problems: &mut Vec<Problem>) {
    floor(
        limits.rate_limits.len(),
        "catalog/limits.toml",
        "rate limits",
        problems,
    );
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for (handled, limit) in limits.rate_limits.iter().enumerate() {
        let anchor = format!("surface = \"{}\"", limit.surface);
        // **The surface, checked.** It is the key `nc_catalog::rate_limit()` is
        // given and the first column of the rate-limit table on
        // `docs/limits.md` — the same role `entry.name` plays for a command,
        // which the rule below this file's commands check holds after the
        // identical measurement. Measured without this rule, with
        // `surface = ""`: `cargo xtask build` reports
        // `ok  15 generated file(s), 2 rewritten`, `cargo xtask check --offline`
        // exits 0 at `check passed`, and the published page carries two budget
        // rows for a surface with no name while `rate_limit("")` answers `Some`.
        //
        // Trimmed, like every other emptiness rule here: one space is a name
        // nobody typed on purpose.
        //
        // **Written once**: twice, byte for byte, one nameless surface would
        // produce the same diagnostic twice and the second copy would decide
        // nothing — one rule written twice, in the file that decides which
        // catalogs this repository accepts, which is the shape this tree names
        // most often. And located: the anchor is `[[rate_limit]]`, the TOML
        // table's own name, not `[[rate_limits]]` (`model.rs` renames it), or
        // `line_of` never matches and the message reaches a developer as a bare
        // filename in a 196-line file holding five of these blocks. Every
        // sibling rule here is located.
        //
        // The occurrence is `handled + 1` and not `max(1)`: `handled` is how
        // many blocks have been read *before* this one, so the block being
        // judged is the next.
        if says_nothing(&limit.surface) {
            push(
                problems,
                "catalog/limits.toml",
                line_of(text, "[[rate_limit]]", handled + 1),
                "a rate limit declares no surface".to_owned(),
                "the surface is the key `nc_catalog::rate_limit()` is given and the \
                 first column of the table on `docs/limits.md` — an empty one is a \
                 budget nothing can look up and everything can match by accident",
                "rate-limit-unnamed",
            );
        }
        let count = seen.entry(limit.surface.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                "catalog/limits.toml",
                line_of(text, &anchor, *count),
                format!("`{}` has two rate limits", limit.surface),
                "the gateway reads one per surface; two means whichever it reads first \
                 silently wins",
                "duplicate-surface",
            );
        }
        let line = line_of(text, &anchor, 1);
        check_budget(
            &limit.surface,
            "per_identifier",
            &limit.per_identifier,
            line,
            problems,
        );
        check_budget(
            &limit.surface,
            "per_address",
            &limit.per_address,
            line,
            problems,
        );
    }
}

/// The three rules about a budget's *magnitudes*, as opposed to its shape.
///
/// Lifted out of [`check_budget`] because the three together cross clippy's
/// line limit. Each judges a value that is written down and published on
/// `docs/limits.md`.
fn the_bounds_a_budget_only_states(
    surface: &str,
    which: &str,
    budget: &Budget,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    // **A curve that ends at a millisecond is not a defence, and "ends non-zero"
    // is not the whole rule.** `curve-ends-free` refuses a final `0` because the
    // last value repeats for ever; a final `1` satisfies it and repeats just as
    // freely. Measured without this rule, `login` — whose own comment says *the
    // escalating delay is what carries this one* — given
    // `delay_ms = [0, 0, 0, 0, 0, 0, 1]`: `cargo xtask check` green, and
    // `docs/limits.md` republishing the curve.
    // Ten attempts free and then one millisecond each is an unlimited surface
    // with a table about it.
    //
    // What makes a repeating tail a bound is that it costs an attacker time. A
    // second per attempt caps a surface at 86,400 tries a day, which is a number
    // somebody can reason about; below that the curve is decoration. A budget
    // that locks out has a real end and is exempt, which is the same shape
    // `curve-ends-free` already draws.
    //
    // **And the exemption is the lockout *arriving*, not the lockout existing.**
    // Written as `lockout_after.is_none()`, any lockout at all buys the
    // exemption — so `lockout_after = 1_000_000` with `delay_ms = [0, 0, 0, 1]`
    // passes, and what an attacker actually pays is a million tries at a
    // millisecond each: about seventeen minutes, then the surface locks. The
    // complaint this rule makes is that *the last value repeats for ever*, and a
    // lockout answers it only when it fires before the repeating starts —
    // `delay_ms.len()` failures in. That is the same boundary `curve-ends-free`
    // draws for a final `0`, and no budget this catalog ships is near it: every
    // curve here ends at a second or more.
    let lockout_lands_before_the_tail_repeats = budget.lockout_after.is_some_and(|lockout| {
        usize::try_from(lockout).is_ok_and(|at| at <= budget.delay_ms.len())
    });
    if !lockout_lands_before_the_tail_repeats
        && budget
            .delay_ms
            .last()
            .is_some_and(|last| *last < A_SECOND_APART)
    {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} curve ends at a delay that bounds nothing"),
            "the last value repeats for ever and nothing stops it before it does, so it \
             is the only thing an attacker pays. Under a second per attempt is tens of \
             thousands of tries a day — give the curve a final delay of at least \
             1000 ms, or a lockout that fires within the curve's own length",
            "curve-ends-cheap",
        );
    }
    if budget.window_seconds == 0 {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} counts attempts in a window of no length"),
            "every other number in this budget is a count within the window, so a \
             window of zero seconds is a budget that limits nothing. Give it the \
             period the attempts are meant to be counted over",
            "window-of-no-length",
        );
    }
    // **And `cumulative`, whose own documentation says what it needs.** It is
    // "whether the lockout count survives the window" — so with no lockout to
    // survive it is the same shape as `on_lockout` above: a control that never
    // fires, set on purpose, reading as a defence to everyone who sees it in
    // the registry or on the generated page. `§10.6`'s wipe is the reason the
    // flag exists, and the wipe is a lockout.
    if budget.cumulative && budget.lockout_after.is_none() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!(
                "`{surface}`'s {which} keeps a lockout count across windows and never locks out"
            ),
            "cumulative says the lockout count survives the window, so without \
             lockout_after there is no count for it to carry — remove it, or give the \
             budget the lockout it is written for",
            "cumulative-without-lockout",
        );
    }
}

/// A command's declaration, held to the rule the kernel will hold a plugin's to.
///
/// **One rule, two callers.** [`nc_schema::judge`] is what `NOCTUA.md §11.7`
/// has the kernel ask of every command a plugin declares at `activate`, and it
/// is what this asks of every command the catalog ships: a name in the command
/// grammar, a title, an argument schema inside the closed subset, an action, a
/// selector the schema backs, and `mutating` agreeing with `reversibility` —
/// with nothing in the schema's text or the title that a reader sees nothing of.
/// Checked here separately, the two would agree until a keyword was accepted
/// by one and refused by the other — and the catalog is the one an agent reads.
///
/// **What shows a reader nothing is missing**, which is this registry's rule
/// and wider than the judge's: a name, an action or a selector made of
/// invisible characters is handed over as the empty string it looks like, so
/// it is reported as absent rather than as malformed. Measured without it, one
/// `U+2063` in each of the three is accepted with no problem at all.
///
/// Every problem the judge finds is reported under a check of its own, named
/// as `the_gates_fire.rs` asserts on it.
fn what_a_dispatcher_needs(entry: &CommandEntry, line: Option<usize>, problems: &mut Vec<Problem>) {
    let declared = nc_schema::Declared {
        name: as_shown(&entry.name),
        title: &entry.title,
        args_schema: &entry.args_schema,
        action: as_shown(&entry.action),
        resource_selector: as_shown(&entry.resource_selector),
        mutating: entry.mutating,
        reversibility: match entry.reversibility {
            Reversibility::Read => nc_schema::Reversibility::Read,
            Reversibility::Write => nc_schema::Reversibility::Write,
            Reversibility::Irreversible => nc_schema::Reversibility::Irreversible,
        },
    };
    let Err(found) = nc_schema::judge(&declared) else {
        return;
    };
    let subject = if declared.name.is_empty() {
        "a command".to_owned()
    } else {
        format!("`{}`", entry.name)
    };
    for problem in found {
        let (check, fix) = the_check_for(&problem);
        push(
            problems,
            "catalog/commands.toml",
            line,
            format!("{subject} {problem}"),
            fix,
            check,
        );
    }
}

/// A value that shows a reader nothing, handed on as the nothing it shows.
fn as_shown(value: &str) -> &str {
    if says_nothing(value) { "" } else { value }
}

/// The check a descriptor problem is reported under, and what fixes it.
///
/// Written out per problem, schema faults included, so a fault the judge
/// gains is a decision made here rather than one inherited from a wildcard.
const fn the_check_for(problem: &nc_schema::DescriptorProblem) -> (&'static str, &'static str) {
    use nc_schema::{DescriptorProblem as Judged, Field, SelectorError};
    match problem {
        Judged::Missing(Field::Name) => (
            "command-unnamed",
            "the name is how a command is invoked, how the page titles its section and what \
             `nc_catalog::command()` is given — an empty one is a command nothing can reach \
             and everything can reach by accident",
        ),
        Judged::Missing(Field::Action | Field::ResourceSelector) => (
            "command-unauthorizable",
            "both are required and a command without either is rejected at activate \
             (NOCTUA.md §11.7) — the PDP has nothing to evaluate without them",
        ),
        Judged::Missing(Field::Title) => (
            "command-untitled",
            "the title is the message key the command list shows; write \
             `command.<name>.title` and give it a sentence in both locales",
        ),
        Judged::Missing(Field::ArgsSchema) => (
            "schema-missing",
            "a command that takes nothing still declares so: \
             `{\"type\":\"object\",\"properties\":{},\"additionalProperties\":false}`",
        ),
        Judged::NameMalformed => (
            "command-name-malformed",
            "a command name is lower-case segments joined by `.`, the first its namespace — \
             `journal.verify` — and the kernel will refuse any other at activate",
        ),
        Judged::TitleTooLong => (
            "command-title-too-long",
            "the title is a message key; name it `command.<name>.title` and put the words \
             in the locales",
        ),
        Judged::TitleInvisibleCharacter(_) => (
            "command-title-invisible-character",
            "the title is what a person reads before running the command, and a character \
             they cannot see can show one command under another's words; the title here is \
             a message key, so write it in ASCII and put the words in the locales",
        ),
        Judged::ReadThatMutates => (
            "read-that-mutates",
            "a read that mutates is one of the two the PDP would let past a read-only \
             allowlist; pick the one that is true",
        ),
        Judged::MutationUndeclared => (
            "mutation-undeclared",
            "undo, the journal and the allowlist all read `mutating`; set it to true",
        ),
        Judged::Schema(error) => the_check_for_a_schema(&error.fault),
        Judged::Selector(SelectorError::Malformed(_)) => (
            "selector-malformed",
            "a resource selector is `realm`, `argument:<name>` naming one of the \
             command's own arguments, or `feature:<id>` naming a feature of the platform \
             — each name `[a-z][a-z0-9_]*`",
        ),
        Judged::Selector(
            SelectorError::NotDeclared(_)
            | SelectorError::NotRequired(_)
            | SelectorError::NotAnEntity(_)
            | SelectorError::Unbounded(_),
        ) => (
            "selector-names-no-entity",
            "the argument a selector reads must be declared, required, \
             `\"type\": \"string\"` with an `x-entity` naming its Cedar type, and \
             `maxLength` at most 128 — or select `realm`",
        ),
        Judged::Selector(SelectorError::RealmThroughAnArgument(_)) => (
            "selector-realm-through-an-argument",
            "the realm a command acts on is the one the kernel serves, selected by `realm`; \
             an argument naming `Realm` would let whoever dispatches choose which realm is \
             authorized",
        ),
    }
}

/// The check a schema fault is reported under, and what fixes it.
///
/// Its own function because [`the_check_for`] is at clippy's line limit; the
/// same rule holds here, one arm per fault and no wildcard.
const fn the_check_for_a_schema(fault: &nc_schema::SchemaFault) -> (&'static str, &'static str) {
    use nc_schema::SchemaFault as Fault;
    match fault {
        // **The one registry value written to a published page with no
        // escaper**, and what exposes the command to an AI agent: every
        // description reaches `docs/` through `cell`, and the schema goes
        // into a fenced JSON block verbatim. The rule is the judge's, so
        // a plugin's schema answers to it at `activate` as this one does.
        Fault::InvisibleCharacter(_) => (
            "schema-invisible-character",
            "the schema is published verbatim and is what exposes this command to an \
             AI agent, so an invisible character reads one way to a person and another \
             to whatever consumes it. Remove it",
        ),
        Fault::NotJson(_) => (
            "schema-unparsable",
            "the schema is stored verbatim and handed to a validator; it has to parse, \
             with no key written twice",
        ),
        Fault::NotAnObjectAtTheTop => (
            "schema-not-an-object",
            "arguments arrive as a named map, so the schema's top level is \
             `\"type\": \"object\"` — and this is what exposes the command to an AI \
             agent, so a malformed one silently removes the tool",
        ),
        Fault::UnknownKeyword(_) => (
            "schema-unknown-keyword",
            "the platform validates a closed subset — `nc_schema::KEYWORDS` lists it — and \
             a keyword outside it would be ignored while it reads as a constraint; \
             express the rule with the subset or drop it",
        ),
        Fault::KeywordNotForType { .. }
        | Fault::WrongKeywordType(_)
        | Fault::TypeMissing
        | Fault::TypeUnsupported(_)
        | Fault::OpenObject
        | Fault::RequiredNamesNoProperty(_)
        | Fault::BoundsInverted(_)
        | Fault::EntityOnNonString
        | Fault::EntityNotATypeName(_)
        | Fault::EnumEmptyOrDuplicate
        | Fault::ItemsMissing
        | Fault::TooDeep => (
            "schema-outside-the-subset",
            "the schema is validated before the command runs, by the subset \
             `crates/nc-schema` documents; rewrite the part named above inside it",
        ),
    }
}

/// The smallest final delay that bounds a surface with no lockout, in
/// milliseconds.
///
/// One second per attempt is 86,400 tries a day — a number an operator can
/// reason about. It is a floor and not a recommendation: every budget this
/// catalog ships ends far above it.
const A_SECOND_APART: u32 = 1_000;

fn check_budget(
    surface: &str,
    which: &str,
    budget: &Budget,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    if budget.delay_ms.is_empty() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}` declares no delay curve for {which}"),
            "§15.5 requires an ESCALATING delay on every authentication surface; an \
             empty curve is no delay at all",
            "no-delay-curve",
        );
    }
    // The property, not a consequence of it. Checking only that the curve is
    // non-empty, under a message saying "escalating", lets `[0, 0, 0, 0]` pass a
    // gate whose text describes something else.
    if budget.delay_ms.windows(2).any(|pair| pair[1] < pair[0]) {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} delay curve goes down"),
            "the curve is the delay after each successive failure, and §15.5 requires it \
             to escalate. A curve that drops rewards an attacker for trying more",
            "curve-descends",
        );
    }
    // **And it has to rise, which "does not fall" is not.** The rule above
    // refuses a curve that *drops*; a flat one satisfies it, and §15.5 asks for
    // an escalating delay. `model.rs`'s own doc names the case a "does not
    // fall" check accepts — *"a flat curve satisfies 'has a curve' while
    // defending nothing"* — and a test asserting `>=` holds only that. Measured
    // without this rule, `email_confirmation`'s per-identifier curve replaced by
    // `[1_000, 1_000, 1_000, 1_000, 1_000]`:
    // `ok 15 generated file(s), 2 rewritten`, `check passed` with no warn line,
    // and `docs/limits.md` republishes the flat curve four lines under its own
    // sentence *"after that the delay curve applies, escalating"*.
    //
    // `login` carries no lockout by decision and its registry comment says *"the
    // escalating delay is what carries this one"*: a flat 1 s curve there is
    // 86 400 attempts a day instead of 8 640, for ever. Every curve this catalog
    // ships ends far above where it starts, so what this refuses is nothing that
    // is here — and a one-value curve is refused too, because its one value
    // repeats for ever and repeating is not escalating.
    if budget
        .delay_ms
        .last()
        .zip(budget.delay_ms.first())
        .is_some_and(|(last, first)| last <= first)
    {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} delay curve never rises"),
            "the last value repeats for ever, so a curve that ends where it began answers \
             every attempt after the first few at one fixed speed. §15.5 asks for an \
             escalating delay, and not falling is not rising",
            "curve-is-flat",
        );
    }
    if budget.delay_ms.last().is_some_and(|last| *last == 0) && budget.lockout_after.is_none() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} curve ends at no delay and never locks out"),
            "the last value repeats forever, so this surface answers an unlimited number \
             of attempts at full speed. Give the curve a final delay, or a lockout",
            "curve-ends-free",
        );
    }
    if budget.max_attempts == 0 {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}` allows zero {which} attempts"),
            "a surface nobody may use is not rate limiting; give it the number of \
             attempts a real person needs",
            "zero-attempts",
        );
    }
    if let Some(lockout) = budget.lockout_after
        && lockout < budget.max_attempts
    {
        {
            push(
                problems,
                "catalog/limits.toml",
                line,
                format!("`{surface}`'s {which} locks out before it finishes rate limiting"),
                "lockout_after must be at least max_attempts, or the escalating delay \
                 never runs and the curve above it is decoration",
                "lockout-before-limit",
            );
        }
    }
    if budget.on_lockout.is_some() && budget.lockout_after.is_none() {
        push(
            problems,
            "catalog/limits.toml",
            line,
            format!("`{surface}`'s {which} says what happens at a lockout it never reaches"),
            "on_lockout without lockout_after is a control that never fires — the device \
             credential would never be wiped (NOCTUA.md §10.6)",
            "lockout-unreachable",
        );
    }
    // **The window itself, which every other rule here is stated inside.**
    // `max_attempts`, `delay_ms` and `lockout_after` are all counts *in the
    // window*, so a zero-length one empties every one of them at once:
    // attempts are counted in no time at all, which is either no limit or a
    // permanent one depending on how a reader rounds. Without this,
    // `window_seconds = 0` passes the compiler, generates the page and
    // publishes `Window 0s` on `docs/limits.md`.
    the_bounds_a_budget_only_states(surface, which, budget, line, problems);
}

// --- commands.toml ---

fn check_commands(entries: &[CommandEntry], text: &str, problems: &mut Vec<Problem>) {
    floor(entries.len(), "catalog/commands.toml", "commands", problems);
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut bindings: BTreeMap<&str, &str> = BTreeMap::new();
    for entry in entries {
        let anchor = format!("name = \"{}\"", entry.name);
        let count = seen.entry(entry.name.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                "catalog/commands.toml",
                line_of(text, &anchor, *count),
                format!("`{}` is declared twice", entry.name),
                "a command name is the only way to invoke it; two declarations mean a \
                 dispatch reaches whichever the registry read last",
                "duplicate-command",
            );
        }
        let line = line_of(text, &anchor, 1);
        what_a_dispatcher_needs(entry, line, problems);
        check_keybinding(entry, &mut bindings, line, problems);
    }
}

/// A binding the interface can parse, and one command per chord.
fn check_keybinding<'a>(
    entry: &'a CommandEntry,
    bindings: &mut BTreeMap<&'a str, &'a str>,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    if let Some(binding) = &entry.keybinding {
        if let Some(first) = bindings.insert(binding.as_str(), entry.name.as_str()) {
            push(
                problems,
                "catalog/commands.toml",
                line,
                format!(
                    "`{}` and `{first}` are both bound to `{binding}`",
                    entry.name
                ),
                "one of them would silently shadow the other, and which one depends on \
                     registration order (NOCTUA.md §12.8)",
                "duplicate-keybinding",
            );
        }
        if let Err(why) = chord(binding) {
            push(
                problems,
                "catalog/commands.toml",
                line,
                format!("`{}`'s keybinding `{binding}` {why}", entry.name),
                "a binding is space-separated chords, each of modifiers and one key: \
                     `C-x C-s`, `SPC b b`, `F2`. A binding the shell cannot parse is a \
                     command nobody can reach and nothing reports",
                "malformed-keybinding",
            );
        }
    }
}

/// Whether a keybinding is a sequence of chords.
fn chord(binding: &str) -> Result<(), String> {
    if says_nothing(binding) {
        return Err("is empty".to_owned());
    }
    for part in binding.split(' ') {
        if part.is_empty() {
            return Err("has two spaces in a row".to_owned());
        }
        let key = part.rsplit('-').next().unwrap_or_default();
        if key.is_empty() {
            return Err(format!("ends `{part}` with a modifier and no key"));
        }
        for modifier in part.split('-').rev().skip(1) {
            if !["C", "M", "S", "H"].contains(&modifier) {
                return Err(format!("uses `{modifier}` as a modifier"));
            }
        }
    }
    Ok(())
}

/// A field's name, judged as the struct field it becomes.
///
/// Two rules with one subject: the gate runs clippy with `-D warnings`, so a
/// capital fails `non_snake_case` inside generated source; and `identifier`
/// answers whether rustc would take it at all. Its own function because
/// `check_events` crossed clippy's line limit.
fn a_field_name_the_struct_can_carry(
    event: &str,
    field: &crate::model::EventField,
    becomes: &str,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    // **And lowercase, because the gate denies what rustc allows.**
    // `identifier` checks Rust's *syntax*; `cargo clippy -- -D warnings`
    // is what the field has to survive, and a capitalised name gets
    // `structure field `Applied` should have a snake case name` in
    // generated source. Measured without this rule: `cargo xtask build` says
    // `ok`.
    if field.name.chars().any(char::is_uppercase) {
        push(
            problems,
            "catalog/log-events.toml",
            line,
            format!("`{event}`'s field `{}` has a capital in it", field.name),
            "it becomes a field of a generated struct, and the gate runs clippy \
             with `-D warnings` — `non_snake_case` then fails the build inside a \
             file nobody may edit. Write it in lower case",
            "field-not-snake-case",
        );
    }
    if let Err(why) = identifier(becomes) {
        push(
            problems,
            "catalog/log-events.toml",
            line,
            format!("`{event}`'s field `{}` {why}", field.name),
            "it becomes a field of a generated struct, and rustc would report the \
             failure inside a file whose header forbids editing it",
            "name-not-an-identifier",
        );
    }
}

// --- log-events.toml ---

/// The keys `nc-log` writes on every line, which an event may not also declare.
///
/// **`_msg` is not here.** The clause that reads this list is
/// `OWNED_BY_THE_LOG.contains(…) || field.name.starts_with('_')`, and `_msg`
/// starts with `_` — so as an entry it would decide nothing: removing it from
/// the list would change no verdict on any input, and neither would removing
/// the underscore disjunct for the one name they share. Two clauses that overlap are one
/// clause and a decoration, and the decoration is the half a later edit deletes
/// believing the other still covers it.
///
/// What is left is the three names the underscore rule cannot reach.
/// `every_name_here_needs_this_list_to_be_refused` is what keeps them apart.
/// The diagnostic still names `_msg` in its prose, because that is what a person
/// writing the field has to read.
const OWNED_BY_THE_LOG: [&str; 3] = ["level", "service", "version"];

fn check_events(entries: &[EventEntry], text: &str, problems: &mut Vec<Problem>) {
    floor(
        entries.len(),
        "catalog/log-events.toml",
        "log events",
        problems,
    );
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut names = Names::in_the_log_events_file("catalog/log-events.toml", text);
    for entry in entries {
        let anchor = format!("name = \"{}\"", entry.name);
        let count = seen.entry(entry.name.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                "catalog/log-events.toml",
                line_of(text, &anchor, *count),
                format!("`{}` is declared twice", entry.name),
                "one name is one event; two declarations generate two structs with the \
                 same name and the build fails somewhere less obvious than here",
                "duplicate-event",
            );
        }
        let line = line_of(text, &anchor, 1);
        if says_nothing(&entry.description) {
            push(
                problems,
                "catalog/log-events.toml",
                line,
                format!("`{}` has an empty description", entry.name),
                "the description is the only English an operator reading the log gets; \
                 without one the event is a name and a number",
                "empty-description",
            );
        }
        // **Keyed on the name the struct gets, not on the one that was written.**
        // `snake` turns `.` and `-` into `_`, so deduped on `field.name`,
        // `max_rows` beside `max-rows` passes the compiler, generates
        // `pub max_rows: u64` twice, and `cargo build` fails inside a file whose
        // header forbids editing it with `error[E0124]: field `max_rows` is
        // already declared`. `Names::claim` does exactly this for every
        // top-level registry name, and says so in its own diagnostic — "`-` and
        // `_` become the same thing here" — and the field loop does not reach it.
        // Both spellings are reported, because the pair is the finding.
        let mut fields: BTreeMap<String, &str> = BTreeMap::new();
        for field in &entry.fields {
            let becomes = crate::snake(&field.name);
            if let Some(first) = fields.insert(becomes.clone(), field.name.as_str()) {
                push(
                    problems,
                    "catalog/log-events.toml",
                    line,
                    format!(
                        "`{}`'s fields `{first}` and `{}` both become `{becomes}`",
                        entry.name, field.name
                    ),
                    "the generated struct would have two fields of one name, and rustc \
                     would report it inside a file whose header forbids editing it. Note \
                     that `.` and `-` both become `_` here, so two names differing only \
                     in one of them collide",
                    "duplicate-field",
                );
            }
            a_field_name_the_struct_can_carry(&entry.name, field, &becomes, line, problems);
            // **`id` is refused until `§5.1` says what an opaque identifier is.**
            // The kind generates a field of type `nc_ids::OpaqueId`, and
            // `OpaqueId::new` accepts any 1..=128 bytes of `[A-Za-z0-9._:-]` —
            // which covers a username, a document number, a phone number and an
            // email local-part. Measured without this refusal: one registry
            // line, an ordinary call site with no `format!`, no `to_string()`,
            // no leak and no macro, and
            // `{"actor":"ana.paula.silva.123.456.789-00"}` reaches the store
            // with every compile-fail fixture green and `check passed`.
            //
            // That is `§15.4`'s central claim — *a dynamic string cannot compile
            // into a log event* — false as shipped, by a kind the registry
            // itself offers. `FieldType::Id`'s own doc records the risk and
            // defers it to `NOCTUA.md §5.1`, which is `[OPEN]`.
            //
            // **This does not decide §5.1.** It declines to depend on a rule
            // that does not exist yet: while the shape of an opaque identifier is
            // undecided, an event field cannot rest on it. The stage that closes
            // §5.1 deletes this refusal, and the kind is still here for it.
            if field.kind == crate::model::FieldType::Id {
                push(
                    problems,
                    "catalog/log-events.toml",
                    line,
                    format!(
                        "`{}`'s field `{}` is declared `id`, and `id` is refused",
                        entry.name, field.name
                    ),
                    "`OpaqueId::new` accepts anything in its character set, so a caller \
                     can mint one out of a person's name or number and the type wall \
                     cannot tell. What an opaque identifier may hold is NOCTUA.md §5.1 and \
                     it is [OPEN]; until it is closed, use `snowflake` for a minted \
                     identifier or `literal` for a name from the source",
                    "field-kind-is-open",
                );
            }
            // **The four the log writes itself.** An event declaring one of them,
            // accepted and emitted, collides with the log's own in one map — so
            // a line could carry a level its event does not have, or name a
            // service it did not come from. `nc-log` writes its own last so it
            // wins; this is the half that says so where somebody would write
            // the field.
            // **And the whole `_` prefix, not four names.** The store reserves
            // every field beginning with an underscore, and `_time` is the one
            // that matters: it is the record's own timestamp, so an event
            // declaring it writes a line the store *accepts with `200 OK` and
            // then drops* — measured against the running store, the shipper
            // counts it delivered, nothing reaches stderr, `lost()` stays 0,
            // and the record is nowhere, because the value lands the line in
            // 1970 and 1970 is outside `-retentionPeriod=30d`. Without this, only
            // `clippy::pub_underscore_fields` on the emitted struct refuses it,
            // which is a lint anybody may allow, and not the rule whose whole
            // subject this is.
            if OWNED_BY_THE_LOG.contains(&field.name.as_str()) || field.name.starts_with('_') {
                push(
                    problems,
                    "catalog/log-events.toml",
                    line,
                    format!(
                        "`{}` declares a field called `{}`, which the log writes itself",
                        entry.name, field.name
                    ),
                    "`_msg`, `level`, `service` and `version` are the line's own, and \
                     `NOCTUA.md §15.4` puts the last two on the log rather than on the \
                     event — an event that had to declare them is an event somebody \
                     forgets to. Every name starting with `_` belongs to the store as \
                     well: it takes such a line, answers 200, and drops it. Give the \
                     field a name of its own",
                    "field-the-log-owns",
                );
            }
        }
        names.claim(&entry.name, &crate::camel(&entry.name), problems);
    }
}

// --- capabilities.toml ---

/// The registry the checks below read, and the file each of their problems is
/// filed against.
const FILE: &str = "catalog/capabilities.toml";

/// Whether `text` is a name the world could write: lower-case words joined by
/// hyphens, each starting with a letter.
///
/// WIT's own grammar for an identifier, which is what an interface and a
/// function of `crates/nc-kernel/wit/noctua-kernel.wit` are spelled in.
fn a_world_name(text: &str) -> bool {
    !text.is_empty()
        && text.split('-').all(|word| {
            word.starts_with(|character: char| character.is_ascii_lowercase())
                && word
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

/// Whether `text` is a function as the world's parser names one: a name, or a
/// resource's method written `[method]<resource>.<name>`.
fn a_world_function(text: &str) -> bool {
    match text.strip_prefix("[method]") {
        Some(method) => method
            .split_once('.')
            .is_some_and(|(resource, name)| a_world_name(resource) && a_world_name(name)),
        None => a_world_name(text),
    }
}

/// Whether `name` is a capability's name for `interface`: the interface itself,
/// or the interface, a dot and one lower-case word.
fn names_its_interface(name: &str, interface: &str) -> bool {
    name == interface
        || name.strip_prefix(interface).is_some_and(|rest| {
            rest.strip_prefix('.').is_some_and(|word| {
                word.starts_with(|character: char| character.is_ascii_lowercase())
                    && word.chars().all(|character| character.is_ascii_lowercase())
            })
        })
}

/// Every capability is a name, an interface and the functions of it it grants,
/// each granted once; every interface given to every plugin says why.
///
/// **What this cannot see is the world**, which is a file of another crate:
/// whether each interface and function exists there, and whether every
/// function of the world is in exactly one capability or in the functions of
/// one ungated interface, is `crates/nc-kernel/tests/capabilities.rs`, which
/// reads the world with its own parser. What is checked here is what the
/// registry says about itself — the shape a name must have to be one the world
/// could hold, and that no function is granted twice, which would make one
/// call answer to two grants.
fn check_capabilities(file: &CapabilityFile, text: &str, problems: &mut Vec<Problem>) {
    floor(file.capabilities.len(), FILE, "capabilities", problems);
    let mut names = Names::new(FILE, "name", text);
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut granted: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for entry in &file.capabilities {
        let anchor = format!("name = \"{}\"", entry.name);
        let count = seen.entry(entry.name.as_str()).or_default();
        *count += 1;
        if *count > 1 {
            push(
                problems,
                FILE,
                line_of(text, &anchor, *count),
                format!("`{}` is declared twice", entry.name),
                "one name is one capability: a manifest asking for it would be granted \
                 whichever declaration the compiler read last, and the other's functions \
                 would answer to no grant",
                "duplicate-capability",
            );
        }
        let line = line_of(text, &anchor, 1);
        check_one_capability(entry, line, problems);
        for function in &entry.functions {
            if let Some(first) = granted.insert((&entry.interface, function), &entry.name) {
                push(
                    problems,
                    FILE,
                    line,
                    format!(
                        "`{}#{function}` is granted by `{first}` and by `{}`",
                        entry.interface, entry.name
                    ),
                    "a function answers to one grant: granted twice, a manifest holding \
                     either capability reaches it, so denying the other denies nothing. \
                     Keep it in the one capability it belongs to",
                    "function-granted-twice",
                );
            }
        }
        names.claim(&entry.name, &crate::camel(&entry.name), problems);
    }
    check_ungated(file, text, problems);
}

/// One capability's own shape: its name, its interface and each function.
///
/// Its own function because `check_capabilities` crossed clippy's line limit
/// with it inline.
fn check_one_capability(entry: &CapabilityEntry, line: Option<usize>, problems: &mut Vec<Problem>) {
    if !a_world_name(&entry.interface) {
        push(
            problems,
            FILE,
            line,
            format!(
                "`{}` names the interface `{}`, which the world cannot spell",
                entry.name, entry.interface
            ),
            "an interface of the world is lower-case words joined by hyphens, as \
             `crates/nc-kernel/wit/noctua-kernel.wit` writes it — `store`, `blobs`, \
             `commands`. Write the interface whose functions this grants",
            "interface-malformed",
        );
    } else if !names_its_interface(&entry.name, &entry.interface) {
        push(
            problems,
            FILE,
            line,
            format!(
                "`{}` does not name its interface `{}`",
                entry.name, entry.interface
            ),
            "a capability is written as its interface, or as its interface, a dot and one \
             lower-case word — `store`, `blobs.read` — so a manifest's reader sees which \
             part of the world it reaches. Rename it",
            "capability-outside-its-interface",
        );
    }
    for function in &entry.functions {
        if !a_world_function(function) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}` grants `{function}`, which the world cannot name",
                    entry.name
                ),
                "a function is written as the world's parser names it: a lower-case \
                 hyphenated name, or `[method]<resource>.<name>` for a resource's method, \
                 such as `[method]reader.read`",
                "function-malformed",
            );
        }
    }
}

/// Every interface given to every plugin is one interface, given once, not also
/// a capability, with a reason, and with functions the world could name.
///
/// **The reason is the whole of the exemption**, which is the rule every
/// escape hatch in this tree is held to, `pack_reason` among them: an interface
/// here is one whose functions no manifest is asked about, so what it can do
/// has to be written where a reader of the registry meets it.
fn check_ungated(file: &CapabilityFile, text: &str, problems: &mut Vec<Problem>) {
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut listed: BTreeSet<(&str, &str)> = BTreeSet::new();
    for entry in &file.ungated {
        let anchor = format!("interface = \"{}\"", entry.interface);
        let count = seen.entry(entry.interface.as_str()).or_default();
        *count += 1;
        let line = line_of(
            text,
            &anchor,
            *count
                + file
                    .capabilities
                    .iter()
                    .filter(|capability| capability.interface == entry.interface)
                    .count(),
        );
        if *count > 1 {
            push(
                problems,
                FILE,
                line,
                format!("`{}` is given to every plugin twice", entry.interface),
                "one interface is one entry: two reasons for one exemption are two sentences \
                 a reader cannot tell which to believe. Keep one",
                "duplicate-ungated",
            );
        }
        if !a_world_name(&entry.interface) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}` is given to every plugin, and the world cannot spell it",
                    entry.interface
                ),
                "an interface of the world is lower-case words joined by hyphens, as \
                 `crates/nc-kernel/wit/noctua-kernel.wit` writes it. Write the interface",
                "interface-malformed",
            );
        }
        if let Some(gated) = file
            .capabilities
            .iter()
            .find(|capability| capability.interface == entry.interface)
        {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}` is given to every plugin and is also the interface of `{}`",
                    entry.interface, gated.name
                ),
                "an interface is either granted by a capability or given to every plugin: \
                 both is a grant the manifest is asked for and a gift that makes the asking \
                 mean nothing. Remove one",
                "interface-gated-and-ungated",
            );
        }
        if says_nothing(&entry.reason) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}` is given to every plugin and says no why",
                    entry.interface
                ),
                "an interface given to every plugin is one no manifest is asked about, so \
                 the reason is the whole of the exemption. Write what it can and cannot do, \
                 the way `log` says it reaches only the plugin's own ring",
                "ungated-without-a-reason",
            );
        }
        check_ungated_functions(file, entry, line, &mut listed, problems);
    }
}

/// Each function an interface gives every plugin is one the world can name,
/// written once, and granted by no capability.
///
/// **The list is what the reason covers.** Without one, the kernel's coverage
/// test could only take every function of the interface as given, so a
/// function the world gains there reaches every plugin under a reason
/// written before it existed. Its own function because `check_ungated`, with
/// it inline, is over clippy's line limit.
fn check_ungated_functions<'a>(
    file: &CapabilityFile,
    entry: &'a UngatedEntry,
    line: Option<usize>,
    listed: &mut BTreeSet<(&'a str, &'a str)>,
    problems: &mut Vec<Problem>,
) {
    for given in &entry.functions {
        if !a_world_function(given) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}` gives every plugin `{given}`, which the world cannot name",
                    entry.interface
                ),
                "a function is written as the world's parser names it: a lower-case \
                 hyphenated name, or `[method]<resource>.<name>` for a resource's method, \
                 such as `[method]reader.read`",
                "function-malformed",
            );
        }
        if let Some(granting) = file.capabilities.iter().find(|capability| {
            capability.interface == entry.interface && capability.functions.contains(given)
        }) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}#{given}` is granted by `{}` and given to every plugin",
                    entry.interface, granting.name
                ),
                "a function answers to one grant or to none: given to every plugin as well, \
                 a manifest denied the capability reaches it anyway, so denying it denies \
                 nothing. Keep it in one place",
                "function-granted-twice",
            );
        } else if !listed.insert((&entry.interface, given)) {
            push(
                problems,
                FILE,
                line,
                format!(
                    "`{}#{given}` is given to every plugin twice",
                    entry.interface
                ),
                "a function is written once: listed twice, deleting the line that gives it \
                 leaves it given by the other, and a reader of this file believes it taken \
                 back. Keep one",
                "function-granted-twice",
            );
        }
    }
}

// --- i18n ---

/// A locale's tag as an identifier, and its direction as one of two words.
///
/// Both reach generated code — the tag becomes a variant of `Locale`, the
/// direction is what `Locale::dir()` answers — and this module's header says
/// every name that becomes an identifier is checked here. Its own function
/// because inside `locales` it would cross clippy's line limit.
fn the_tag_is_a_name_and_a_direction(
    file: &LocaleFile,
    stem: &str,
    text: &str,
    generated: &str,
    problems: &mut Vec<Problem>,
) {
    // **And it has to be an identifier**, like every other generated name —
    // this file's header says "every name that becomes an identifier is
    // checked here, where the file and the line exist". Measured without this,
    // a locale file named `2x` with `bcp47 = "2x"`: accepted, and
    // `pub enum Locale { 2x, … }` emitted.
    if let Err(why) = identifier(generated) {
        push(
            problems,
            &format!("catalog/i18n/{stem}.toml"),
            line_of(text, "bcp47", 1),
            format!("`{}` {why}", file.meta.bcp47),
            "the tag becomes a variant of the generated `Locale`, and rustc would \
             report the failure inside a file whose header forbids editing it",
            "locale-not-an-identifier",
        );
    }
    // **And a direction is one of two words.** `Locale::dir()` is what the
    // emitter documents as going into a document's `dir` attribute; nothing
    // renders one yet, which is the cheapest moment to close it. Measured
    // without this, `dir = "sideways"`: accepted and compiled in.
    if file.meta.dir != "ltr" && file.meta.dir != "rtl" {
        push(
            problems,
            &format!("catalog/i18n/{stem}.toml"),
            line_of(text, "dir", 1),
            format!("`{}` is not a text direction", file.meta.dir),
            "`Locale::dir()` is written into a document's `dir` attribute, which HTML \
             defines as `ltr` or `rtl`. Anything else is a value a browser ignores \
             and a reader cannot see is wrong",
            "locale-direction",
        );
    }
    // **And the endonym, the third field.** `meta.name` reaches generated code
    // twice — the `///` line above each `Locale` variant and `Locale::name()`'s
    // own return value — and this function's own reason for existing is that
    // the other two fields reach generated code. Measured without this rule:
    //
    // ```text
    // name = ""     cargo xtask build: ok, no diagnostic
    //               crates/nc-catalog/src/i18n.rs:  ///\n    En,
    //               cargo xtask check: error: empty doc comment  -> exit 1
    // name = "-"    cargo xtask build: ok
    //               cargo xtask check: check passed, 9 warn line(s)
    //               Locale::name() answers "-" for English, for ever
    // ```
    //
    // The first is verbatim what this file's header says the second class of
    // checks exists to prevent: a build that says `ok` and a failure inside a
    // generated file whose header forbids editing it, with no line in any
    // registry to change. The second fails nowhere at all, and `model.rs`'s own
    // doc for the field says what it costs — *"a switcher that lists languages
    // in the language you are trying to leave is useless to the person who
    // needs it"*.
    //
    // A letter, not a length: `-`, `--` and `0` are what a length alone lets
    // through, and the reason rule one crate over refuses them for the same
    // reason — a name is made of letters.
    let endonym = file.meta.name.trim();
    if !endonym.chars().any(char::is_alphabetic) {
        push(
            problems,
            &format!("catalog/i18n/{stem}.toml"),
            line_of(text, "name", 1),
            format!("`{}` is not a language's name for itself", file.meta.name),
            "it is the `///` line above the generated `Locale` variant and what \
             `Locale::name()` answers, so a blank one is a doc comment rustc refuses \
             inside a file nobody may edit, and a dash is a switcher entry naming \
             nothing. Write the language's name in that language",
            "locale-unnamed",
        );
    }
}

/// Every locale file in `catalog/i18n/`, source locale first.
///
/// Split out to keep `locales` inside its line budget, and it earns the split:
/// the ordering is load-bearing. The source locale's key order and placeholders
/// are what every other locale is checked against, so it has to be read first.
///
/// **Not filtered on being a file.** A directory named like a locale reaches
/// `read_to_string` and is reported as unreadable, which is what it is — the
/// alternative is skipping it silently and losing a locale from every
/// completeness check below.
fn locale_files(entries: std::fs::ReadDir) -> Vec<(String, std::path::PathBuf)> {
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // **Whatever case the extension is written in.** Compared exactly,
        // `fr.TOML` is silently not a locale — accepted with no problem at all,
        // and gone from every completeness check below. The paragraph above
        // says the alternative to reporting a stray file is *"skipping it
        // silently and losing a locale"*, and that is what an uppercase
        // extension would do. `xtask/src/build.rs` asks the same question with
        // `eq_ignore_ascii_case` one crate over.
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        {
            let stem = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            files.push((stem, path));
        }
    }
    files.sort_by(|a, b| in_locale_order(&a.0).cmp(&in_locale_order(&b.0)));
    files
}

/// Where a locale's stem sorts: the source locale first, then by name.
///
/// **Its own function so that each half is decided by something.**
/// `catalog/i18n/` holds exactly two files — `en` and `pt-BR` — so the
/// source-first half alone fixes the whole order *and* a plain alphabetical
/// sort produces byte-identical output, since `en` sorts before `pt-BR`
/// anyway. Measured with the key written inline: dropping the name tie-break,
/// and replacing the whole key with `a.0.cmp(&b.0)`, each leave
/// `cargo test -p nc-catalog-compiler` green —
/// `two_directory_orders_produce_the_same_bytes`, the test written for
/// criterion 4, among them — and the second is invisible to the drift gate as
/// well, because the emitted bytes do not move.
///
/// The order is load-bearing: `Locale::ALL`, the enum's variant order,
/// `Locale::index()`, every column of `MESSAGES`, four `docs/*.md` pages,
/// `emit::table`'s `locales.first()` as the source locale, and `docs::tags`
/// documented "source first" all come from it. Named here so
/// `the_order_is_the_source_first_then_by_name` can drive it with a third
/// locale, which is the arrangement this repository does not ship.
fn in_locale_order(stem: &str) -> (bool, &str) {
    (stem != SOURCE_LOCALE, stem)
}

fn locales(root: &Path, problems: &mut Vec<Problem>) -> Vec<Locale> {
    let directory = root.join("catalog/i18n");
    // The marker sits on its own short line because rustfmt moves a trailing
    // comment off a long one, and the gate reads the line the call is on.
    let listing = std::fs::read_dir(&directory); // allow-nondeterminism: this `read_dir` is sorted by `locale_files`, which `the_order_on_disk_stays_on_disk` proves
    let entries = match listing {
        Ok(entries) => entries,
        Err(error) => {
            problems.push(Problem::in_file(
                "catalog/i18n/",
                format!("could not be read: {error}"),
                "every user-facing string lives here, in both locales",
                "locales-unreadable",
            ));
            return Vec::new();
        }
    };

    let files = locale_files(entries);

    let mut locales = Vec::new();
    let mut names = BTreeMap::new();
    for (stem, path) in files {
        let relative = format!("catalog/i18n/{stem}.toml");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                problems.push(Problem::in_file(
                    &relative,
                    format!("could not be read: {error}"),
                    "every locale in this directory is loaded, so a file here that \
                     cannot be opened is a locale silently missing from every \
                     completeness check below. Fix its permissions, or take it out of \
                     catalog/i18n/ if it is not a locale",
                    "locale-unreadable",
                ));
                continue;
            }
        };
        let Some(file) = parse::<LocaleFile>(&relative, &text, problems) else {
            continue;
        };
        if file.meta.bcp47 != stem {
            problems.push(Problem::at(
                &relative,
                line_of(&text, "bcp47", 1).unwrap_or(1),
                format!(
                    "declares bcp47 `{}` in a file named `{stem}`",
                    file.meta.bcp47
                ),
                "the filename is what selects the locale at runtime and the tag is what \
                 reaches the document's `lang`; the two disagreeing means a page served \
                 under one name announces another",
                "locale-tag-mismatch",
            ));
        }
        let generated = crate::camel(&file.meta.bcp47);
        the_tag_is_a_name_and_a_direction(&file, &stem, &text, &generated, problems);
        if let Some(first) = names.insert(generated.clone(), file.meta.bcp47.clone()) {
            problems.push(Problem::in_file(
                &relative,
                format!(
                    "`{}` and `{first}` both become `{generated}`",
                    file.meta.bcp47
                ),
                "they would generate one enum variant twice",
                "name-collision",
            ));
        }
        floor(file.message.len(), &relative, "messages", problems);
        locales.push(Locale {
            meta: file.meta,
            message: file.message,
            tags: file.tags,
        });
    }

    if !locales
        .iter()
        .any(|locale| locale.meta.bcp47 == SOURCE_LOCALE)
    {
        problems.push(Problem::in_file(
            "catalog/i18n/",
            format!("has no `{SOURCE_LOCALE}` locale"),
            "English is the source locale: it is what every other locale's keys and \
             placeholders are checked against, so without it nothing is checked",
            "no-source-locale",
        ));
    }
    if locales.len() < 2 {
        problems.push(Problem::in_file(
            "catalog/i18n/",
            "carries fewer than two locales",
            "the platform ships English and Brazilian Portuguese (NOCTUA.md §10.9), and \
             a completeness check with one locale compares nothing to nothing",
            "too-few-locales",
        ));
    }
    locales
}

fn push(
    problems: &mut Vec<Problem>,
    file: &str,
    line: Option<usize>,
    what: impl Into<String>,
    fix: impl Into<String>,
    check: &'static str,
) {
    problems.push(Problem {
        file: file.to_owned(),
        line,
        what: what.into(),
        fix: fix.into(),
        check,
    });
}

/// One registry entry's claim on a key.
///
/// **Whose it is and which of its texts**, so a key missing from a locale is
/// reported as the text it is — *command kernel.undo's display name has no
/// pt-BR text* — rather than as a key a reader has to search the registries
/// for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Claim {
    /// The entry, as a diagnostic names it: `command kernel.undo`.
    pub(crate) owner: String,
    /// Which of its texts the key is: `display name`.
    pub(crate) slot: &'static str,
    /// Where the key is looked up.
    pub(crate) table: Table,
}

/// Every key any registry entry points at, and the first claim on it.
pub(crate) fn referenced_keys(catalog: &Catalog) -> BTreeMap<String, Claim> {
    every_reference(catalog)
        .into_iter()
        .filter_map(|(key, claims)| claims.into_iter().next().map(|claim| (key, claim)))
        .collect()
}

/// The owners of a key, without repeating one that claims it twice.
///
/// What a diagnostic says: `error NC-DB-0001 and error NC-DB-0001` reads as a
/// bug in the gate, and `error NC-DB-0001 (twice)` is what happened.
fn named(owners: &[String]) -> String {
    let mut said: Vec<String> = Vec::new();
    for owner in owners {
        match said.last_mut() {
            Some(last) if last == owner => *last = format!("{owner} (twice)"),
            _ => said.push(owner.clone()),
        }
    }
    said.join(" and ")
}

/// Every message key, with **all** the entries that claim it.
///
/// `referenced_keys` keeps one owner per key because that is all a "this key
/// does not exist" diagnostic needs — and a second claimant silently replaces
/// the first there, which makes a key owned twice structurally invisible to it.
///
/// Two owners is a defect: `check_errors` already refuses two error codes
/// sharing a key, on the grounds that the key is a code's identity in the
/// ledger, and this says the same across registries. Measured without it —
/// pointing a notification's title at `error.db.unreachable` compiles, and the
/// notification then owns the sentence `NC-DB-0001` resolves to. Editing it "for the
/// notification" changes what the error says, and the ledger sees nothing,
/// because the code's key and description never moved.
pub(crate) fn every_reference(catalog: &Catalog) -> BTreeMap<String, Vec<Claim>> {
    let mut keys: BTreeMap<String, Vec<Claim>> = BTreeMap::new();
    // **Every claim, including two from one entry.** Deduped on the owner's
    // *name*, a notification whose `title` and `body` are the same key would
    // have one owner and never trip `key-owned-twice` — while the gate's
    // own reasoning applies to it word for word: rewording that sentence for
    // the title silently rewords the body, and neither slot can be edited for
    // its own purpose. Two slots are two claims whoever holds them.
    //
    // **One key, one owner, whichever table it is in**: a command's tags key
    // spelled as another entry's sentence is still one key two entries hold.
    let mut claim = |key: &str, owner: String, slot: &'static str, table: Table| {
        keys.entry(key.to_owned())
            .or_default()
            .push(Claim { owner, slot, table });
    };
    for entry in &catalog.errors {
        let owner = format!("error {}", entry.code);
        claim(&entry.message, owner, "message", Table::Message);
    }
    for entry in &catalog.notifications {
        let owner = format!("notification {}", entry.id);
        claim(&entry.title, owner.clone(), "title", Table::Message);
        claim(&entry.body, owner, "body", Table::Message);
    }
    for entry in &catalog.emails {
        let owner = format!("email {}", entry.id);
        claim(&entry.subject, owner.clone(), "subject", Table::Message);
        claim(&entry.body, owner, "body", Table::Message);
    }
    for entry in &catalog.commands {
        let owner = format!("command {}", entry.name);
        claim(&entry.title, owner.clone(), "display name", Table::Message);
        claim(
            &entry.description,
            owner.clone(),
            "line description",
            Table::Message,
        );
        claim(&entry.paragraph, owner.clone(), "paragraph", Table::Message);
        claim(&entry.tags, owner, "tags", Table::Tags);
    }
    for entry in &catalog.capabilities {
        let owner = format!("capability {}", entry.name);
        claim(&entry.description, owner, "description", Table::Message);
    }
    keys
}

/// No message key is claimed by two registry entries.
pub(crate) fn check_one_owner_per_key(catalog: &Catalog, problems: &mut Vec<Problem>) {
    for (key, claims) in every_reference(catalog) {
        let owners: Vec<String> = claims.into_iter().map(|claim| claim.owner).collect();
        if owners.len() > 1 {
            problems.push(Problem::in_file(
                "catalog/",
                format!("`{key}` is the message of {}", named(&owners)),
                "one sentence with two owners is a sentence neither of them can be \
                 edited for: rewording it for one silently rewords the other, and the \
                 error-code ledger sees nothing because no code moved. Give each entry \
                 its own key, even where the two sentences are identical today",
                "key-owned-twice",
            ));
        }
    }
}

/// One registry entry's parameter declaration, and every message it governs.
///
/// Grouped by entry rather than flattened to one row per message, because the
/// two questions worth asking have different shapes. **A message may only use
/// what its entry declares** — a placeholder nothing can supply reaches the
/// reader as literal braces. **An entry may only declare what some message
/// uses** — a parameter every caller is forced to supply and no sentence shows
/// is a rename that half-landed. The first is per message; the second is per
/// entry, across all of them.
#[derive(Debug)]
pub(crate) struct Declaration {
    /// How the entry is named in a diagnostic.
    pub(crate) owner: String,
    /// What it says it interpolates.
    pub(crate) declared: BTreeSet<String>,
    /// The message keys it governs.
    pub(crate) keys: Vec<String>,
}

/// Every declaration in the catalog.
///
/// An email declares one set of params and has **two** messages; so does a
/// notification. Both are checked on both messages, because on the body alone a
/// subject could interpolate a name the declaration omits and the build stays
/// green.
pub(crate) fn declared_params(catalog: &Catalog) -> Vec<Declaration> {
    let mut declared = Vec::new();
    for entry in &catalog.errors {
        declared.push(Declaration {
            owner: format!("error {}", entry.code),
            declared: entry.params.iter().cloned().collect(),
            keys: vec![entry.message.clone()],
        });
    }
    for entry in &catalog.notifications {
        declared.push(Declaration {
            owner: format!("notification {}", entry.id),
            declared: entry.params.iter().cloned().collect(),
            keys: vec![entry.title.clone(), entry.body.clone()],
        });
    }
    for entry in &catalog.emails {
        declared.push(Declaration {
            owner: format!("email {}", entry.id),
            declared: entry.params.iter().cloned().collect(),
            keys: vec![entry.subject.clone(), entry.body.clone()],
        });
    }
    // **A command's three sentences take no parameters, and that is checked
    // rather than assumed.** `CommandEntry` has no `params` field: a command's
    // display name, line and paragraph are shown in a palette and in the
    // generated developer surfaces (`§15.3`), where there is no caller to supply
    // a value. Declaring the empty set here is what turns "there is nowhere to
    // fill this in" into a build failure rather than an empty placeholder on a
    // screen.
    for entry in &catalog.commands {
        declared.push(Declaration {
            owner: format!("command {}", entry.name),
            declared: BTreeSet::new(),
            keys: vec![
                entry.title.clone(),
                entry.description.clone(),
                entry.paragraph.clone(),
            ],
        });
    }
    // A capability's sentence is shown beside a manifest's grants, where there is
    // no caller to supply a value either — the same reason as a command's.
    for entry in &catalog.capabilities {
        declared.push(Declaration {
            owner: format!("capability {}", entry.name),
            declared: BTreeSet::new(),
            keys: vec![entry.description.clone()],
        });
    }
    declared
}

#[cfg(test)]
mod tests {

    /// The locale order is the source first, then by name.
    ///
    /// **Two locales cannot decide this and this repository ships two.** With
    /// `en` the source and `pt-BR` beside it, the source-first half alone fixes
    /// the order, and so does a plain alphabetical sort — so both halves of the
    /// key are free. The third locale here sorts *before* the source, which is
    /// the one arrangement that tells the three candidate keys apart.
    #[test]
    fn the_order_is_the_source_first_then_by_name() {
        let mut stems = vec!["pt-BR", "de", super::SOURCE_LOCALE, "ar"];
        stems.sort_by(|a, b| super::in_locale_order(a).cmp(&super::in_locale_order(b)));
        assert_eq!(
            stems,
            vec![super::SOURCE_LOCALE, "ar", "de", "pt-BR"],
            "the source locale is first and the rest are in name order: a key that only \
             floats the source leaves the rest wherever `read_dir` put them, and a plain \
             alphabetical key puts `ar` and `de` in front of the source that every reader \
             of `locales.first()` takes on trust"
        );
    }
    use super::{OWNED_BY_THE_LOG, area_of, chord, identifier};

    /// Neither half of the owned-field clause is a decoration on the other.
    ///
    /// The clause is `OWNED_BY_THE_LOG.contains(&field.name) ||
    /// field.name.starts_with('_')`. An entry beginning with `_` is already
    /// refused by the second half, so it decides nothing — it can be deleted
    /// with every test green, and so can the disjunct beside it as far as that
    /// name is concerned. `_msg` would be exactly that.
    ///
    /// Asserted here rather than left to reading, because the next name added to
    /// this list is added by someone copying the ones above it.
    #[test]
    fn every_import_here_is_a_name_the_preamble_binds() {
        // The preamble the generated crate carries, read from the emitter that
        // writes it. A `use` that ends `as _` binds no name, so a generated item
        // of that name is not a collision and refusing it is a rule that turns
        // away something rustc accepts.
        let preamble = crate::emit::the_preamble_for_tests();
        for named in super::ALREADY_IN_THE_GENERATED_FILE {
            let bound = preamble.lines().any(|line| {
                let line = line.trim();
                line.starts_with("use ")
                    && line
                        .split(|character: char| !character.is_alphanumeric() && character != '_')
                        .any(|word| word == named)
                    && !line.contains(&format!("{named} as _"))
            });
            assert!(
                bound,
                "`{named}` is refused as a name the generated file already holds, and the \
                 preamble binds no such name — so this entry turns away an item rustc \
                 accepts. `LogValue` was here for exactly that reason: it is imported \
                 `as _`, which binds nothing"
            );
        }
        let distinct: std::collections::BTreeSet<&str> =
            super::ALREADY_IN_THE_GENERATED_FILE.into_iter().collect();
        assert_eq!(
            distinct.len(),
            super::ALREADY_IN_THE_GENERATED_FILE.len(),
            "a name written twice is refused once and read as two rules"
        );
        // The floor: a preamble this could not read would bind nothing and the
        // loop above would be a walk over an empty claim.
        assert!(
            preamble
                .lines()
                .filter(|line| line.trim().starts_with("use "))
                .count()
                >= 2,
            "the preamble read here holds fewer than two imports, so this test is about \
             a string it did not find: {preamble}"
        );
    }

    #[test]
    fn every_name_here_needs_this_list_to_be_refused() {
        for owned in OWNED_BY_THE_LOG {
            assert!(
                !owned.starts_with('_'),
                "`{owned}` is refused by the `starts_with('_')` disjunct beside this \
                 list, so naming it here decides nothing and one of the two clauses is \
                 dead"
            );
            assert!(
                !owned.is_empty(),
                "an empty name matches no field and refuses nothing"
            );
        }
        let distinct: std::collections::BTreeSet<&str> = OWNED_BY_THE_LOG.into_iter().collect();
        assert_eq!(
            distinct.len(),
            OWNED_BY_THE_LOG.len(),
            "a name written twice is refused once and read as two rules"
        );
    }

    #[test]
    fn a_well_formed_code_yields_its_area() {
        assert_eq!(area_of("NC-AUTH-0031"), Some("AUTH"));
        assert_eq!(area_of("NC-DB-0001"), Some("DB"));
    }

    #[test]
    fn a_code_the_scheme_does_not_describe_yields_nothing() {
        // Each of these is a shape somebody would plausibly write, and every one
        // of them would sort or match differently from the scheme §15.1 fixes.
        for malformed in [
            "NC-AUTH-31",    // not four digits
            "NC-AUTH-00031", // five
            "NC-auth-0031",  // lowercase area
            "AUTH-0031",     // no prefix
            "NC--0031",      // no area
            "NC-AUTH-003a",  // not digits
            "NC-AUTH",       // no number
        ] {
            assert_eq!(area_of(malformed), None, "{malformed} should be rejected");
        }
    }

    #[test]
    fn a_name_that_is_not_a_rust_identifier_is_caught_here_and_not_by_rustc() {
        // A notification called `2fa.enabled` would otherwise reach rustfmt,
        // which reports a byte offset into a stream the developer cannot open.
        assert!(identifier("2faEnabled").is_err());
        assert!(identifier("").is_err());
        assert!(identifier("has space").is_err());
        assert!(identifier("Self").is_err());
        assert!(identifier("DbUnreachable").is_ok());
        assert!(identifier("_private").is_ok());
    }

    #[test]
    fn a_keybinding_is_chords_of_modifiers_and_a_key() {
        for good in ["C-x C-s", "SPC b b", "F2", "M-S-w", "C-c"] {
            assert!(chord(good).is_ok(), "`{good}` should parse");
        }
        // **And a binding that shows a reader nothing**, which is the one
        // emptiness this rule asks `says_nothing` about: written as
        // `trim().is_empty()`, it would accept `keybinding = "\u{2063}"` as a
        // chord, publish it as a key nobody can press, and count it as one
        // command's binding for the purposes of the two-commands-one-chord
        // rule. The shipped catalog declares no binding, so this is where that
        // call site is driven.
        for bad in [
            "",
            "C-",
            "X-a",
            "C-x  C-s",
            "\u{2063}",
            "\u{200b}\u{200b}",
            "\u{feff}",
        ] {
            assert!(chord(bad).is_err(), "`{bad}` should be refused");
        }
    }
}
