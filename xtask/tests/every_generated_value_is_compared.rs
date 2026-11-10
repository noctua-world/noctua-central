//! A value the compiler emits that nothing compares can be emitted wrong.
//!
//! **A field can reach the generated code with no comparison behind it**, and
//! these are the shape, each measured with the comparison that holds it
//! removed:
//!
//! * `ErrorCode::params()` — flattening the emitter to `&[]` for every code
//!   passes the whole gate, and `§10.9` makes a response a key *plus
//!   parameters*;
//! * `StoreLimit::enforced_by` — the published page can name the wrong
//!   enforcer for the log store's retention, green;
//! * `Command::arguments` — compared only by tests that read the generated
//!   constant, never the registry it came from.
//!
//! A field compared in *one* direction or on *one* surface is what makes the gap
//! hard to see: the registry is walked and the code is walked, and a field
//! simply falls between them.
//!
//! So this walks the generated crate itself and requires every public field and
//! every public accessor to be named by a comparison. It is the same argument as
//! `§19.3`'s count floor, one level up: a gate that inspects a *type* has to know
//! what that type holds.
//!
//! # The hole, declared
//!
//! Coverage can be indirect: `Locale::index()` is exercised through
//! `nc_catalog::message`, which is what a caller actually uses. `INDIRECT` names
//! those, with the call that covers them and the mutation that proved it — and
//! `the_indirect_list_is_not_a_place_to_hide` keeps that list honest.

use std::path::{Path, PathBuf};

/// The files a comparison may live in.
///
/// Each walks the registry or a published page and asserts against the generated
/// code. A field named nowhere in any of them is a field nothing checks.
const COMPARISONS: [&str; 7] = [
    "xtask/tests/the_registry_reaches_the_code.rs",
    "xtask/tests/the_pages_reach_the_catalog.rs",
    "crates/nc-catalog/tests/the_generated_surface.rs",
    "crates/nc-prune/src/lib.rs",
    "crates/nc-journal/tests/the_declared_command.rs",
    "crates/nc-kernel/src/history.rs",
    "crates/nc-kernel/src/builtin.rs",
];

/// Generated files whose every value is compared by *iterating the registry*.
///
/// **A comparison need not name a value to compare it.**
/// `crates/nc-log/src/events.rs` holds a struct and its fields for every event
/// `catalog/log-events.toml` declares, all emitted from it — and every one is
/// checked, by a test that walks `catalog.events` and builds each field's
/// expected declaration with `snake(&field.name)`. No type name and no field
/// name is ever written as a literal, so the substring match below cannot see
/// any of it, and without this list the file would sit outside a gate called
/// "every generated value is compared".
///
/// This is not an exclusion list: naming a file here is a claim that a specific
/// test derives its names from the catalog rather than writing them, and
/// [`the_iterating_comparisons_really_iterate`] checks that claim against the
/// test's own source. A file listed here whose comparison stops iterating fails.
///
/// **The third field is the registry collection the covering test walks, and it
/// is what ties the excuse to the file it excuses.** The other checks on this
/// list are properties of the *covering* file — that it is read by this gate,
/// that the reason is long enough, that its text walks `catalog.events` — and
/// none of them asks whether that walk covers the file being let off. So
/// without this field, a second row naming any generated file, pointed at this
/// same comparison, passes the lot: measured, both comparisons of
/// `StoreLimit::enforced_by` deleted and `crates/nc-catalog/src/limits.rs`
/// added here leave the suite green, with the excuse plainly false in the green
/// run. The collection is checked against the
/// generated file's own stem, so excusing `limits.rs` is a claim that the
/// comparison walks `catalog.limits`, and the needle built from it is what
/// refuses the claim.
const BY_ITERATION: [(&str, &str, &str, &str); 1] = [(
    "crates/nc-log/src/events.rs",
    "xtask/tests/the_registry_reaches_the_code.rs",
    "events",
    "every event struct and field is built from `catalog.events` and compared \
     against the emitted source, so each is covered by construction rather than \
     by name — which is stronger than a list of names, and invisible to a \
     substring match",
)];

/// What is covered through another call rather than by name, and how.
///
/// An exclusion list is a claim and needs its own test; the one below fires each
/// entry by mutation-free reasoning it can check — the covering call has to be
/// named by a comparison too.
const INDIRECT: [(&str, &str, &str, &str); 1] = [(
    "index",
    "Locale",
    "message",
    "the column a locale's sentences are in. Exercised through `nc_catalog::message`, \
     which is what a caller uses; setting `PtBR => 0` fails \
     `every_sentence_the_platform_shows_is_the_one_the_registry_declares`",
)];

fn root() -> PathBuf {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/..")).to_path_buf()
}

/// Every way a comparison can be *about* one generated type.
///
/// The type's own name, plus every `pub const` whose declared type mentions it
/// and every `pub fn` whose return type does. A comparison of `EventSpec`'s
/// fields walks `EVENTS`; one of `Command`'s calls `command(…)`. Requiring the
/// struct's name would have called all of those uncompared, and hand-listing the
/// aliases would be another list needing its own test — so they are read out of
/// the generated source, which is where the relationship is written.
fn names_for(type_: &str) -> Vec<String> {
    let mut found = vec![type_.to_owned()];
    let generated = root().join("crates/nc-catalog/src");
    let Ok(files) = std::fs::read_dir(&generated) else {
        return found;
    };
    for entry in files.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            let mentions = line.contains(type_);
            if !mentions {
                continue;
            }
            if let Some(rest) = line.strip_prefix("pub const ")
                && let Some(name) = rest.split(':').next()
                // Digits included: `ARGON2ID_PASSWORD` is a constant of this
                // shape, and an uppercase-only test rejects it and then reports
                // fields as uncompared that assertions do compare.
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
            {
                found.push(name.to_owned());
            }
            for opening in ["pub const fn ", "pub fn "] {
                if let Some(rest) = line.strip_prefix(opening)
                    && let Some(name) = rest.split('(').next()
                    && line.contains("->")
                {
                    found.push(format!("{name}("));
                }
            }
            // And a *field* of another type: `FieldSpec` is reached through
            // `EventSpec::fields`, and `Argon2Id` through `Limits::password`.
            // A comparison of what those hold walks the field, and naming the
            // inner struct is not something it has any reason to do.
            if let Some(rest) = line.strip_prefix("pub ")
                && let Some(name) = rest.split(':').next()
                && rest.contains(':')
                && !rest.starts_with("fn ")
                && !rest.starts_with("const ")
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !name.is_empty()
            {
                found.push(format!(".{name}"));
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Source with every `/* … */` spent, newlines kept so line numbers do not move.
///
/// Block comments nest, as Rust's do, and a `/*` inside a string literal opens
/// nothing. The line reader below cannot see either shape — a comment that
/// starts on one line and ends on another is not a line — which is why this runs
/// first.
fn without_block_comments(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut at = 0_usize;
    let mut depth = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    while at < characters.len() {
        let here = characters[at];
        let next = characters.get(at + 1).copied();
        if depth > 0 {
            if here == '/' && next == Some('*') {
                depth += 1;
                at += 2;
                continue;
            }
            if here == '*' && next == Some('/') {
                depth -= 1;
                at += 2;
                continue;
            }
            if here == '\n' {
                out.push('\n');
            }
            at += 1;
            continue;
        }
        if quoted {
            out.push(here);
            match here {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => quoted = false,
                _ => {}
            }
            at += 1;
            continue;
        }
        // **A character literal is not a string opener.** `'"'` is one
        // character and its quote is data; read as an opener it leaves this
        // scanner inside a string for everything that follows, so the next
        // `/* … */` is kept whole and the line reader below judges a comment as
        // code. `'\''` and `'\\'` are the same shape, and a lifetime — which
        // has no closing quote — is left alone by `char_literal_at`, or
        // `&'static str` would swallow the rest of the file.
        if here == '\''
            && let Some(width) = char_literal_at(&characters, at)
        {
            for offset in 0..width {
                out.push(characters[at + offset]);
            }
            at += width;
            continue;
        }
        if here == '"' {
            quoted = true;
            out.push(here);
            at += 1;
            continue;
        }
        // A line comment runs to the newline and may hold a `/*` that opens
        // nothing.
        if here == '/' && next == Some('/') {
            while at < characters.len() && characters[at] != '\n' {
                out.push(characters[at]);
                at += 1;
            }
            continue;
        }
        if here == '/' && next == Some('*') {
            depth = 1;
            at += 2;
            continue;
        }
        out.push(here);
        at += 1;
    }
    out
}

/// A line with its comment removed, respecting string literals.
///
/// A `//` inside a string is not a comment, and this crate's own comparisons
/// name endpoints like `http://127.0.0.1:9428` that would otherwise blind the
/// rest of the line.
///
/// **A character literal is one quote and toggles nothing.** Without that arm,
/// `let quote = '"';` leaves a reader of this shape believing a string is open
/// for the rest of the line, after which its comment is read as code. The
/// readers in `gates.rs` have the arm too; this one writes it again because it
/// cannot share theirs: `xtask` is a binary, an
/// integration test cannot reach `crate::gates`, and the alternative — a
/// dev-dependency on the gate — would put the gate inside what it judges
/// (`AGENTS.md` rule 3). So the arm is written again here, with a corpus of its
/// own beneath it rather than a promise that the two agree.
fn without_comment(line: &str) -> &str {
    let characters: Vec<char> = line.chars().collect();
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0;
    let mut at = 0;
    while index < characters.len() {
        let here = characters[index];
        match here {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            // `'"'`, `'\''` and `'\\'` are one character each, and a
            // lifetime — `&'static str` — has no closing quote at all, so it is
            // left alone rather than read as an opener.
            '\'' if !quoted => {
                if let Some(width) = char_literal_at(&characters, index) {
                    for offset in 0..width {
                        at += characters[index + offset].len_utf8();
                    }
                    index += width;
                    continue;
                }
            }
            '/' if !quoted && characters.get(index + 1) == Some(&'/') => return &line[..at],
            _ => {}
        }
        at += here.len_utf8();
        index += 1;
    }
    line
}

/// The width of the character literal opening at `index`, if one opens there.
///
/// A lifetime is not a literal: `'static` has no closing quote, and reading one
/// as an opener is how a scanner runs out of phase for the rest of the line.
fn char_literal_at(characters: &[char], index: usize) -> Option<usize> {
    if characters.get(index) != Some(&'\'') {
        return None;
    }
    let escaped = characters.get(index + 1) == Some(&'\\');
    let mut at = index + if escaped { 3 } else { 2 };
    // `'\u{1F600}'` and its like: the escape runs to its own closing brace.
    if escaped && characters.get(index + 2) == Some(&'u') {
        at = index + 3;
        while at < characters.len() && characters[at] != '}' {
            at += 1;
        }
        at += 1;
    }
    if characters.get(at) == Some(&'\'') {
        Some(at + 1 - index)
    } else {
        None
    }
}

/// One function's body, from its signature onwards, bounded by its own braces.
///
/// The first `{` after the signature opens it and the matching `}` closes it —
/// which is what "this function reads that value" has to mean. Reading to the
/// next `"\n}"` instead reached the end of the enclosing `impl`, so any value
/// touched anywhere in the type satisfied a claim about one method.
fn function_body(after_the_name: &str) -> String {
    let Some(open) = after_the_name.find('{') else {
        return String::new();
    };
    let mut depth = 0_i32;
    for (index, character) in after_the_name[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return after_the_name[open..=open + index].to_owned();
                }
            }
            _ => {}
        }
    }
    String::new()
}

/// The generated crate's source, as text.
///
/// Read whole rather than per file: an excuse names a call, and the call and the
/// value it reads need not be in the same generated file.
fn emitted_sources() -> Vec<String> {
    let mut found = Vec::new();
    for directory in ["crates/nc-catalog/src", "crates/nc-log/src"] {
        let Ok(entries) = std::fs::read_dir(root().join(directory)) else {
            continue;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if path.extension().is_some_and(|kind| kind == "rs")
                && let Ok(text) = std::fs::read_to_string(&path)
            {
                found.push(text);
            }
        }
    }
    assert!(!found.is_empty(), "the generated crate was not read");
    found
}

/// Every comparison, as (file, one test's body).
///
/// **One test, not one file.** Kept whole, a file holding `ErrorCode`'s
/// comparison and `Email`'s satisfied a check for either — so deleting the error
/// one left the gate green on the strength of the email one two hundred lines
/// away. A comparison is about the type its own test names.
// allow-logging: `comparisons` hands back a list of type names written in this file
fn comparisons() -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    for relative in COMPARISONS {
        let text = std::fs::read_to_string(root().join(relative))
            .unwrap_or_else(|error| panic!("{relative} is readable: {error}"));
        // **Code, not prose.** The match below is a substring test, and a
        // comment is text: *mentioning* a type anywhere in a chunk that happens
        // to call the accessor satisfies it. Measured — one line inside the test
        // that compares `Notification` and `Email` params:
        //
        // ```text
        // // ErrorCode is compared elsewhere.
        // ```
        //
        // and the same in the file's own `//!` header, chunk zero. Read raw,
        // either one restores green after the real `ErrorCode::params`
        // assertion is deleted, and with the emitter then flattened to `=> &[]`
        // for every code, `check passed`. That is the exact defect this file's
        // header opens with, at a cost of one comment.
        // **And a block comment, which a line reader cannot see at all**, the
        // spelling `gates.rs`'s `without_comments` also refuses:
        // without this, the single comparison of `ErrorCode::params` wrapped in
        // `/* … */` leaves the whole `check` transcript byte-identical to a
        // clean run, where the same lines behind `//` are reported at once,
        // measured.
        let text = without_block_comments(&text);
        let text: String = text
            .lines()
            .map(without_comment)
            .collect::<Vec<_>>()
            .join("\n");
        // Cut at each attribute that opens a test, and at each free `fn`. What
        // matters is that two tests never share a chunk.
        let mut chunk = String::new();
        for line in text.lines() {
            let opens = line.trim_start().starts_with("#[test]")
                || line.trim_start().starts_with("#[tokio::test]")
                || line.starts_with("fn ")
                || line.starts_with("async fn ");
            if opens && !chunk.trim().is_empty() {
                found.push((relative, core::mem::take(&mut chunk)));
            }
            chunk.push_str(line);
            chunk.push('\n');
        }
        if !chunk.trim().is_empty() {
            found.push((relative, chunk));
        }
    }
    found
}

/// Whether a parameter list is just the receiver.
///
/// `self)`, `&self)`, `&self,` and a `&self` at the end of a line rustfmt
/// wrapped all say the same thing; `self, index: usize` does not. The comma case
/// is what a wrapped signature looks like, and a reader without it misses one.
fn takes_only_the_receiver(parameters: &str) -> bool {
    let rest = parameters.trim_start().trim_start_matches('&').trim_start();
    let Some(after) = rest.strip_prefix("self") else {
        return false;
    };
    // What follows the receiver, up to the close: nothing yet (the line wrapped),
    // the close itself, or a trailing comma before it. Anything else is a second
    // parameter, so the method takes an argument and is not an accessor this
    // gate is about — and what comes *after* the close is a return type and a
    // brace, which say nothing about the parameters.
    let after = after.trim_start();
    let after = after.strip_prefix(',').unwrap_or(after).trim_start();
    after.is_empty() || after.starts_with(')')
}

/// Every public field and accessor the generated crate exposes:
/// `(file, type, name, is_accessor)`.
///
/// **The type matters.** Names are shared across two or more generated types,
/// and a check that compared bare names treated one comparison as covering all
/// of them.
///
/// **How many is not written here.** A count written once is the list a reader
/// made once, and it misses exactly the collapse this paragraph warns about —
/// `code`, shared by `ErrorCode` and generated event structs. The number moves
/// with the registry and nothing computes it, so the argument is stated without
/// it.
fn emitted() -> Vec<(String, String, String, bool)> {
    let mut found = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("crates/nc-catalog/src"))
        .expect("the generated crate is there")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "rs"))
        .collect();
    // **And the generated log events**, which the compiler emits from
    // `catalog/log-events.toml` into another crate. A walk reading
    // `crates/nc-catalog/src` alone leaves the event structs and their public
    // fields — every one of them a value the compiler wrote — outside a gate
    // whose name is "every generated value is compared". They are compared,
    // by `the_registry_reaches_the_code.rs`, and that is a thing this gate
    // should be checking rather than a thing it happens to be right about.
    files.push(root().join("crates/nc-log/src/events.rs"));
    files.sort();
    for path in files {
        let text = std::fs::read_to_string(&path).expect("readable");
        // **The path, not the file name.** `crates/nc-catalog/src/events.rs` and
        // `crates/nc-log/src/events.rs` are both `events.rs`, and `BY_ITERATION`
        // keys on this — so keyed on the file name, excusing the log's file
        // excuses the catalog's too: over a third of this gate's subject,
        // including every field of `EventSpec` and `FieldSpec`. Demonstrated
        // with a `pub const fn shard()` emitted onto `EventSpec` and compared by
        // nothing: `check passed`.
        let file = path
            .strip_prefix(root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        // Whichever `pub struct` or `pub enum` is open, which is what a field or
        // an accessor below it belongs to.
        let mut owner = String::new();
        for line in text.lines() {
            let line = line.trim();
            for opening in ["pub struct ", "pub enum "] {
                if let Some(rest) = line.strip_prefix(opening)
                    && let Some(name) = rest.split([' ', '{', '(', '<']).next()
                {
                    owner.clear();
                    owner.push_str(name);
                }
            }
            if let Some(rest) = line.strip_prefix("impl ")
                && let Some(name) = rest.split([' ', '{', '<']).next()
            {
                owner.clear();
                owner.push_str(name);
            }
            if owner.is_empty() {
                continue;
            }
            // **One reader, because a clause proven against a copy is not
            // proven.** With these clauses written out here and again in a copy
            // that the test keeping the digit clause alive drives, the copy can
            // differ from this walk and the test holds the copy: measured, the
            // clause deleted from this walk leaves `check passed` and
            // `an_accessor_is_recognised_however_its_signature_is_written`
            // green. So there is one predicate, and the test drives it.
            if let Some(rest) = line.strip_prefix("pub ")
                && let Some(name) = the_field_declared_by(rest)
            {
                found.push((file.clone(), owner.clone(), name.to_owned(), false));
            }
            for opening in ["pub const fn ", "pub fn "] {
                // **`&self` too, and a signature rustfmt wrapped.** Asking for
                // the literal `(self)` on one line leaves an accessor taking a
                // reference — which is how any accessor returning a borrow has
                // to be written — and one whose signature crosses 100 columns
                // both outside the gate. What identifies an accessor is
                // that it takes the receiver and nothing else, and `(self` with
                // the next character not a name character says that whichever
                // way the receiver is spelled and wherever the line breaks.
                if let Some(rest) = line.strip_prefix(opening)
                    && let Some(name) = rest.split('(').next()
                    && let Some(after) = rest.split_once('(')
                    && takes_only_the_receiver(after.1)
                {
                    found.push((file.clone(), owner.clone(), name.to_owned(), true));
                }
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// How one emitted value is spelled where a comparison reads it.
///
/// **An accessor is compared as a call.** `.params` matches `entry.params` —
/// the *model* side, which every comparison reads to say what the generated
/// value should be — so deleting `generated.params()` left the gate green
/// against its own model reference. A generated accessor is only read by
/// calling it; a field is read as a field, or named in a struct literal.
fn reads_for(name: &str, is_accessor: bool) -> Vec<String> {
    if is_accessor {
        vec![format!(".{name}()")]
    } else {
        vec![format!(".{name}"), format!("{name}:")]
    }
}

/// Whether one chunk compares one emitted value.
///
/// Three questions, and each closed a different way of passing without
/// comparing anything:
///
/// * it is **about the type** — a substring over pooled text made
///   `ErrorCode::params` and `Email::params` one string, and seven names are
///   shared across two or more generated types (how many is not written down:
///   it moves with the registry and nothing computes it);
/// * it **reads the registry** — `catalog.`, because an assertion comparing two
///   generated constants to each other says nothing about what was declared;
/// * and it **relates the value to something else**, which is
///   [`held_by_an_equality`], and is the one the other two cannot do.
fn compares_this_value(text: &str, names: &[String], reads: &[String]) -> bool {
    names.iter().any(|alias| text.contains(alias.as_str()))
        && text.contains("catalog.")
        && reads.iter().any(|read| text.contains(read.as_str()))
        && held_by_an_equality(text, reads)
}

/// Whether some assertion in `text` holds one of `reads` against something else.
///
/// **The unit is not the chunk, because a chunk is a whole test.** Requiring a
/// chunk to read the loaded catalog is satisfied by the *loop header* — `for
/// entry in &catalog.commands` — which says nothing about the assertion inside
/// it. Measured: the command emitter changed to write the title key into
/// `description_key`, and the one real assertion replaced by the exact shape
/// this gate's own doc names as the defect —
///
/// ```text
/// -   assert_eq!(generated.description_key, entry.description, "{}", entry.name);
/// +   assert!(!generated.description_key.is_empty(), "{}", entry.name);
/// ```
///
/// — after which every command in the platform resolved its one-line
/// description to its own title, in both locales, in the command list and in
/// the AI tool interface, with `check passed` and no test deleted. The chunk
/// still named the type, still read the field, and still mentioned `catalog.`.
///
/// So the unit is the assertion, and the question is not what shape it is but
/// **whether it relates two things**. `!generated.description_key.is_empty()`
/// touches one; `assert_eq!(generated.description_key, entry.description)`,
/// `severity_matches(entry.severity, generated.severity())` and
/// `entry.code.starts_with(&format!("NC-{}-", generated.area()))` each touch
/// two. Requiring an `==` would have called the last two uncompared; requiring
/// nothing let the first one count.
fn held_by_an_equality(text: &str, reads: &[String]) -> bool {
    // A lookup that panics is a comparison too: half the real comparisons here
    // find the generated entry *by* the value, and the value is held against
    // the registry by whether the lookup succeeds.
    // **`assert_ne!` is not on this list.** An assertion that two
    // things *differ* never pins either of them: `assert_ne!(generated.k,
    // entry.name)` relates two operands and establishes nothing about `k`. Both
    // uses in this tree are sanity checks sitting beside a real `assert_eq!`
    // — that two Argon2id profiles are not the same configuration, and that a
    // locale's endonym is not its tag — so neither loses its coverage.
    const COMPARES: [(&str, usize); 5] = [
        ("assert_eq!", 2),
        ("assert!", 1),
        (".find(", 1),
        (".position(", 1),
        (".any(", 1),
    ];
    // **A `for` that pairs the two sides is a comparison too.** The rate limits
    // destructure `[(&entry.per_identifier, &generated.per_identifier), …]` and
    // then compare the halves field by field — the read appears only in the
    // loop header, which is not an assertion and is exactly where the relation
    // is written.
    if pairs_the_two_sides(text, reads) {
        return true;
    }
    let mut rest = text;
    while let Some((at, arity)) = COMPARES
        .iter()
        .filter_map(|(head, arity)| rest.find(head).map(|at| (at, *arity)))
        .min()
    {
        let after = &rest[at..];
        let Some(open) = after.find('(') else {
            break;
        };
        let arguments = balanced(&after[open..]);
        let predicate: String = top_level_arguments(arguments)
            .into_iter()
            .take(arity)
            .collect::<Vec<_>>()
            .join(",");
        // **A predicate that is one name is that name's definition.** The
        // limits comparison builds `let same = matches!((entry.strategy,
        // generated.strategy), …)` and then asserts `same`; the relation is
        // real and one line up. Following it is the same move as reading the
        // chunk for its bindings, and it is bounded to one hop.
        let predicate = match predicate.trim() {
            single if is_a_name(single) => defined_as(text, single).unwrap_or(predicate.clone()),
            _ => predicate.clone(),
        };
        // **One conjunct, not the whole predicate.** Counting operands across a
        // whole `assert!` is beaten by five words: `!generated.k.is_empty() &&
        // !entry.k.is_empty()` names two operands and relates nothing, and
        // measured, it ships every command's description as its own title with
        // the gate's output byte-identical to a clean run. Two things are
        // related when they meet in **one** comparison, so the conjuncts are
        // split and each is asked on its own.
        if predicate
            .split("&&")
            .flat_map(|conjunct| conjunct.split("||"))
            .any(|conjunct| {
                reads.iter().any(|read| whole_operand(conjunct, read)) && operands(conjunct) > 1
            })
        {
            return true;
        }
        rest = &after[open + 1..];
    }
    false
}

/// Whether a `for` header holds one of `reads` against something else.
///
/// The right-hand side runs to the `{` that opens the body and may span lines,
/// so it is read by bracket depth rather than to the end of a line.
fn pairs_the_two_sides(text: &str, reads: &[String]) -> bool {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    for (index, line) in lines.iter().enumerate() {
        let Some(rest) = line.strip_prefix("for ") else {
            continue;
        };
        let Some((pattern, head)) = rest.split_once(" in ") else {
            continue;
        };
        // **A tuple, because one binding cannot compare two things.** Accepting
        // any `for` header that mentions the read and names two identifiers,
        // without looking at the body, lets
        //
        // ```rust
        // for value in [entry.description.as_str(), generated.description_key] {
        //     assert!(!value.is_empty(), "{}", entry.name);
        // }
        // ```
        //
        // pass — measured, shipping every command's description as its own
        // title with the gate's output byte-identical to a clean run. The real shape
        // destructures a *pair* and compares the halves; a single binding walks
        // the two sides one at a time and can only ever inspect each.
        if pattern
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .filter(|name| !name.is_empty() && *name != "mut" && *name != "ref")
            .count()
            < 2
        {
            continue;
        }
        let mut header = String::from(head);
        let mut depth = bracket_depth(head);
        let mut next = index + 1;
        while depth > 0 && next < lines.len() {
            header.push(' ');
            header.push_str(lines[next]);
            depth += bracket_depth(lines[next]);
            next += 1;
        }
        if reads.iter().any(|read| header.contains(read.as_str())) && operands(&header) > 1 {
            return true;
        }
    }
    false
}

/// How much deeper the brackets go by the end of `line`.
fn bracket_depth(line: &str) -> i32 {
    line.chars().fold(0, |depth, character| match character {
        '[' | '(' => depth + 1,
        ']' | ')' => depth - 1,
        _ => depth,
    })
}

/// The text from an opening bracket to the one that closes it, inclusive.
fn balanced(from: &str) -> &str {
    let mut depth = 0_i32;
    for (offset, character) in from.char_indices() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => {
                depth -= 1;
                if depth == 0 {
                    return &from[..=offset];
                }
            }
            _ => {}
        }
    }
    from
}

/// One macro's arguments, split on the commas that are not inside anything.
fn top_level_arguments(arguments: &str) -> Vec<&str> {
    let inner = arguments
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or(arguments);
    let mut found = Vec::new();
    let mut depth = 0_i32;
    let mut quoted = false;
    let mut start = 0;
    for (offset, character) in inner.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' | '[' | '{' if !quoted => depth += 1,
            ')' | ']' | '}' if !quoted => depth -= 1,
            ',' if depth == 0 && !quoted => {
                found.push(&inner[start..offset]);
                start = offset + 1;
            }
            _ => {}
        }
    }
    found.push(&inner[start..]);
    found
}

/// Whether a predicate is a single bare identifier.
fn is_a_name(predicate: &str) -> bool {
    !predicate.is_empty()
        && predicate
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_')
        && predicate.chars().next().is_some_and(char::is_alphabetic)
}

/// What `let <name> = …` gives `name`, within one chunk.
fn defined_as(text: &str, name: &str) -> Option<String> {
    let at = text.find(&format!("let {name} = "))?;
    let rest = &text[at + format!("let {name} = ").len()..];
    Some(
        rest.split_once(';')
            .map_or(rest, |(head, _)| head)
            .to_owned(),
    )
}

/// Whether `read` appears in `text` as a value in its own right.
///
/// **A conjunction is not needed to beat the conjunct rule.** One conjunct with
/// two operands that relates nothing does it:
///
/// ```rust
/// assert_eq!(
///     generated.description_key.is_empty(),
///     entry.description.is_empty(),
///     "{}", entry.name
/// );
/// ```
///
/// One conjunct, the read present, two operands — and true of every string the
/// emitter could write. Measured, it ships every command's description as its
/// own title with the gate's output byte-identical to a clean run.
///
/// What separates that from a real comparison is not the shape of the assertion
/// but what is compared: `f(A) == f(B)` compares two *summaries*, and a summary
/// is where a value goes to be forgotten. So the value has to reach the
/// comparison whole — followed by a comma, a bracket, a semicolon or a
/// comparison operator, and not by another `.` that collapses it.
///
/// `severity_matches(entry.severity, generated.severity())` still passes: the
/// read is an argument, and both sides reach the *same* call.
fn whole_operand(text: &str, read: &str) -> bool {
    let mut from = 0;
    while let Some(at) = text[from..].find(read) {
        let after = from + at + read.len();
        let rest = text[after..].trim_start();
        if rest.is_empty()
            || rest.starts_with([',', ')', ']', '}', ';'])
            || rest.starts_with("==")
            || rest.starts_with("!=")
        {
            return true;
        }
        from = after;
    }
    false
}

/// How many distinct things a predicate reaches for.
///
/// **Two, and the second is what makes it a comparison.** An *operand* is a
/// name the predicate uses in its own right — a receiver like `entry` in
/// `entry.code.starts_with(…)`, or a bare value like `surface` in
/// `limit.surface == surface`. Names reached *through* a dot are not operands:
/// they are parts of the thing already counted.
///
/// `!generated.description_key.is_empty()` reaches for one thing;
/// `assert_eq!(generated.description_key, entry.description)`,
/// `severity_matches(entry.severity, generated.severity())` and
/// `limit.surface == surface` each reach for two. Rust's own words and the
/// literals in a message are not operands.
fn operands(predicate: &str) -> usize {
    const NOT_AN_OPERAND: [&str; 10] = [
        "true", "false", "as", "if", "else", "match", "let", "ref", "mut", "move",
    ];
    let bytes: Vec<char> = predicate.chars().collect();
    let mut found: Vec<String> = Vec::new();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        if bytes[index] == '"' {
            quoted = !quoted;
            index += 1;
            continue;
        }
        if !quoted && (bytes[index].is_alphabetic() || bytes[index] == '_') {
            let start = index;
            while index < bytes.len() && (bytes[index].is_alphanumeric() || bytes[index] == '_') {
                index += 1;
            }
            let through_a_dot = start > 0 && bytes[start - 1] == '.';
            let a_path = bytes.get(index) == Some(&':');
            let name: String = bytes[start..index].iter().collect();
            if !through_a_dot
                && !a_path
                && !NOT_AN_OPERAND.contains(&name.as_str())
                && !found.contains(&name)
            {
                found.push(name);
            }
        } else {
            index += 1;
        }
    }
    found.len()
}

/// Both generated sources are walked, and each one holds values.
///
/// **The walk's own coverage.** `emitted()` reads the generated catalog crate
/// from a directory listing and the log's events from one pushed line, and
/// deleting that line left every test in this file green while the gate's
/// subject shrank by eleven event structs and their fields — a gate named
/// "every generated value is compared" quietly comparing fewer of them.
///
/// A floor for each source, so losing either is a failure here rather than a
/// smaller number nobody reads.
#[test]
fn the_walk_reaches_both_generated_sources() {
    let found = emitted();
    let events = "crates/nc-log/src/events.rs";
    let from_the_log = found.iter().filter(|(file, ..)| file == events).count();
    let from_the_catalog = found
        .iter()
        .filter(|(file, ..)| file.starts_with("crates/nc-catalog/src/"))
        .count();
    assert!(
        from_the_log > 0,
        "the generated log events are not in the walk, so nothing here judges them"
    );
    assert!(
        from_the_catalog > 0,
        "the generated catalog is not in the walk"
    );
    // And the file is named by its path, which is what `BY_ITERATION` keys on.
    assert!(
        found.iter().all(|(file, ..)| file.contains('/')),
        "a source was named without its directory, and two of them are `events.rs`"
    );
}

/// The generated surface this gate walks is the size it is written down as.
///
/// **An expectation that shrinks with the thing it measures.** The rule below
/// compares what its loop judged against `emitted()`'s own length, so a filter
/// inside `emitted()` moves both sides at once and that rule cannot see it:
/// measured, dropping `Email`, `Notification`, `Command` and three limit types
/// — including `StoreLimit::enforced_by`, one of the values this file's header
/// opens with — clears that rule and the `> 20` floor alike.
///
/// So the walk's own reach is written down. This is the test
/// `xtask/src/check.rs`'s exclusion table names as the reason this file is
/// floored rather than mutated, and the `INDIRECT` rule in this same file says
/// one screen down why the table must name one — *an excuse has to point at a
/// test, and that test has to exist*.
///
/// A number, because the surface is generated: it moves when the registry
/// gains a value, and that is an edit somebody makes deliberately and records
/// here in the same change.
#[test]
fn the_generated_surface_is_judged_whole() {
    let emitted = emitted();
    assert_eq!(
        emitted.len(),
        190,
        "the walk over the generated crate found {} value(s) rather than the number \
         written down. A filter inside it narrows what every rule in this file judges \
         and moves the comparison's own expectation with it",
        emitted.len()
    );
}

#[test]
fn every_field_and_accessor_the_compiler_emits_is_named_by_a_comparison() {
    let comparisons = comparisons();
    let emitted = emitted();
    // The floor, for the reason every gate here carries one: a walk that read no
    // generated file would pass having compared nothing. Its size is pinned by
    // [`the_generated_surface_is_judged_whole`], because a floor of twenty
    // against the whole surface is mostly slack — and the equality below
    // compares the loop against *this* list, so narrowing the walk moves both
    // numbers together. Measured: six generated types dropped out of `emitted`
    // clear this floor and that equality alike.
    assert!(
        emitted.len() > 20,
        "only {} emitted values were found, so the walk read almost nothing",
        emitted.len()
    );

    // **The type, not the bare name.** As a bare substring over pooled text,
    // `ErrorCode::params` and `Email::params` are one string — deleting the
    // error comparison *and* flattening the emitter leaves the gate green,
    // because `entry.params` on the model side still spells `.params`
    // somewhere. Names are shared across two or more generated types — `params`,
    // `description`, `name`, `severity`, `id`, `title_key`, `body_key`, `code`
    // and more — and all of them collapse to one. The count is not written: a
    // count written here drifts from the surface it counts.
    //
    // A comparison has to name the *type* it is about, in the same file. That is
    // what every real one already does: `ErrorCode::ALL`, `Notification::ALL`,
    // `nc_catalog::COMMANDS`.
    let mut uncompared = Vec::new();
    let mut judged = 0_usize;
    let mut excused = 0_usize;
    for (file, type_, name, is_accessor) in &emitted {
        if BY_ITERATION
            .iter()
            .any(|(generated, _, _, _)| generated == file)
        {
            excused += 1;
            continue;
        }
        if INDIRECT
            .iter()
            .any(|(indirect, on, _, _)| indirect == name && on == type_)
        {
            excused += 1;
            continue;
        }
        // **Counted here, below the skips, because a counter above them counts
        // iterations rather than judgements.** `judged += 1` at the top of this
        // loop is the shape `gates.rs`'s own `everything_reached_a_verdict`
        // exists to refuse — its fix text says it in these words: *a filter
        // between the counter at the top of the loop and the judgement at the
        // bottom leaves every printed number whole while the rule is never
        // asked*. Measured with the counter there: `if !is_accessor { continue; }`
        // inserted under it stops every emitted field being judged with the
        // equality still true — so it is incremented at the verdict itself,
        // below, where `compared` is
        // decided. A filter anywhere above that skips the increment with it.
        let names = names_for(type_);
        // **An accessor is compared as a call.** `.params` matches
        // `entry.params` — the *model* side, which every comparison reads to say
        // what the generated value should be — so deleting `generated.params()`
        // left the gate green against its own model reference. A generated
        // accessor is only read by calling it.
        let reads = reads_for(name, *is_accessor);
        // **And it reads the registry, which is what makes it a comparison.**
        // Naming the type and calling the accessor is satisfied by an assertion
        // that cannot tell one value from another. Measured: the emitter changed
        // to write `locales[0].meta.name` for every locale — so
        // `Locale::PtBR.name()` returned `"English"`, the endonym a person
        // picking a language reads — and the two real assertions deleted:
        //
        // ```text
        // check passed
        // ```
        //
        // with the printed count unchanged. What keeps it green is
        // `assert!(!locale.name().is_empty())` in a chunk that mentions
        // `Locale`, which is true of every string the emitter could write.
        //
        // A generated value's source is `catalog/`, so a comparison of one has
        // to read the loaded registry — `catalog.locales`, `catalog.commands`
        // — and hold the generated side against it. Over every comparison but
        // the one against `args_schema` in
        // `xtask/tests/the_registry_reaches_the_code.rs`, asking for that refuses
        // exactly one value across the whole surface, measured:
        // `Command::arguments`, which a test reading only the generated constant
        // compares against itself.
        let compared = comparisons
            .iter()
            .any(|(_, text)| compares_this_value(text, &names, &reads));
        judged += 1;
        if !compared {
            uncompared.push(format!("{file}::{type_}::{name}"));
        }
    }
    // **What the loop reached, not what the walk found.** The floor above reads
    // `emitted`, which the loop then iterates — so narrowing the iteration
    // itself is invisible to it: `emitted.iter().take(25)` removes most of this
    // gate's subject with the floor still holding, and without the comparison
    // below `cargo xtask check` prints the same count and says `check passed`,
    // measured. It is the shape this file's own `held_by_an_equality` detector
    // exists for, one level up.
    //
    // This file is a rule rather than a suite, and it is the one integration
    // target `xtask/src/check.rs`'s exclusion table names as floored rather than
    // mutated. This is that floor.
    // **And a floor on `judged` itself, because the equality above is not one.**
    // `judged + excused == emitted.len()` holds however the two are divided, so
    // moving values into `excused` — which is what a new row in `BY_ITERATION`
    // does — keeps it true while the number of values actually judged collapses.
    // `xtask/src/check.rs`'s exclusion table names
    // `the_generated_surface_is_judged_whole` as this file's floor, and that
    // test pins what the *walk* found, which is a different number; this is the
    // one the table describes. Measured: one row added to `BY_ITERATION` for
    // `crates/nc-catalog/src/limits.rs` takes every value of that file out of
    // the judging with the equality above still green.
    assert_eq!(
        judged,
        127,
        "{judged} of {} emitted value(s) were judged. A value moved into an excuse \
         list leaves the equality below true and this number smaller, so what the \
         gate judges is written down here as well",
        emitted.len()
    );
    assert_eq!(
        judged + excused,
        emitted.len(),
        "the walk found {} emitted value(s); {judged} were judged and {excused} were \
         excused by name. Every value is one or the other, so a filter between the \
         walk and the judgement leaves the rest of the generated surface decided by \
         nothing while the count this gate prints does not move",
        emitted.len()
    );
    assert!(
        uncompared.is_empty(),
        "the compiler emits these and no comparison reads them, so each could be \
         emitted wrong with the whole gate green: {uncompared:#?}. Compare it in one of \
         {COMPARISONS:?}, or add it to INDIRECT with the call that covers it"
    );
}

/// What counts as comparing a value, and what only looks like it.
///
/// **Both rules this gate leans on, tested.** Without this, the requirement
/// that a comparison read the loaded catalog, and the requirement that an
/// accessor be *called* rather than named on the model side, can each be
/// deleted with the whole suite green — and the first is also defeatable
/// without deleting it, when it is asked of the chunk, because every real chunk
/// satisfies it in its loop header.
#[test]
fn an_assertion_that_inspects_one_value_is_not_a_comparison() {
    let read = |name: &str| vec![format!(".{name}")];

    // The shape that shipped a wrong value: true of every string the emitter
    // could write.
    let vacuous = "assert!(!generated.description_key.is_empty(), \"{}\", entry.name);";
    assert!(!held_by_an_equality(vacuous, &read("description_key")));
    for weak in [
        "assert!(generated.name.is_some());",
        "assert!(!generated.name.is_empty());",
        "assert!(generated.name.len() > 0);",
        // A hard-coded expectation is not the registry.
        "assert_eq!(generated.name, \"English\");",
        // **Two operands, related to nothing.** Each half inspects one value,
        // and a rule counting operands across the whole predicate reads that as
        // a comparison: measured, these five words then ship every command's
        // description as its own title with the gate's output byte-identical to
        // a clean run.
        "assert!(!generated.name.is_empty() && !entry.name.is_empty());",
        "assert!(!entry.name.is_empty() || generated.name.is_some());",
        // An assertion that two things *differ* pins neither of them.
        "assert_ne!(generated.name, entry.tag);",
        // **Two operands inside one conjunct, relating nothing.** A conjunction
        // is not needed to beat the conjunct rule: `f(A) == f(B)` compares two
        // summaries, and a summary is where a value goes to be forgotten.
        "assert_eq!(generated.name.is_empty(), entry.name.is_empty());",
        "assert_eq!(generated.name.len(), entry.name.len());",
    ] {
        assert!(
            !held_by_an_equality(weak, &read("name")),
            "this was counted as a comparison: {weak}"
        );
    }

    // And every shape a real one takes in this tree.
    for real in [
        "assert_eq!(generated.description_key, entry.description, \"{}\", entry.name);",
        "assert!(severity_matches(entry.severity, generated.severity), \"x\");",
        "assert!(entry.code.starts_with(&format!(\"NC-{}-\", generated.area)), \"x\");",
        ".find(|limit| limit.surface == surface)",
        "let same = matches!((entry.strategy, generated.strategy), (a, b));\nassert!(same, \"x\");",
        "for (declared, carried) in [(&entry.per_address, &generated.per_address)] {",
    ] {
        let name = [
            "description_key",
            "severity",
            "area",
            "surface",
            "strategy",
            "per_address",
        ]
        .into_iter()
        .find(|name| real.contains(name))
        .expect("each fixture names the value it compares");
        assert!(
            held_by_an_equality(real, &read(name)),
            "this real comparison was not counted: {real}"
        );
    }

    // **A `for` over both sides is not a comparison unless it pairs them.** A
    // single binding walks the two sides one at a time and can only inspect
    // each; the shape this rule exists for destructures a pair.
    assert!(!held_by_an_equality(
        "for value in [entry.name.as_str(), generated.name] { assert!(!value.is_empty()); }",
        &read("name")
    ));
    assert!(held_by_an_equality(
        "for (declared, carried) in [(&entry.name, &generated.name)] { assert_eq!(carried, declared); }",
        &read("name")
    ));

    // **An accessor is compared as a call.** `.params` matches `entry.params` —
    // the *model* side, which every comparison reads to say what the generated
    // value should be — so deleting `generated.params()` left the gate green
    // against its own model reference.
    assert_eq!(reads_for("params", true), vec![".params()".to_owned()]);
    assert_eq!(
        reads_for("params", false),
        vec![".params".to_owned(), "params:".to_owned()]
    );
    let names = vec!["ErrorCode".to_owned()];
    let accessor = reads_for("params", true);
    let model_side_only =
        "for entry in &catalog.errors { assert_eq!(ErrorCode::x.y, entry.params, \"x\"); }";
    assert!(
        !compares_this_value(model_side_only, &names, &accessor),
        "an accessor was counted as read because the model side spells it"
    );
    assert!(compares_this_value(
        "for entry in &catalog.errors { assert_eq!(ErrorCode::x.params(), entry.params, \"x\"); }",
        &names,
        &accessor
    ));

    // **And a comparison reads the registry.** Two generated constants held
    // against each other say nothing about what the catalog declared, and the
    // relation above cannot tell that apart on its own.
    assert!(
        !compares_this_value(
            "assert_eq!(ErrorCode::A.params(), ErrorCode::B.params(), \"x\");",
            &names,
            &accessor
        ),
        "a comparison that never opens the registry was counted"
    );

    // **And it is about this type.** The match is a substring over pooled text,
    // and seven names — `params`, `description`, `name`, `severity`, `id`,
    // `title_key`, `body_key`, `code` and others — are shared across two or more
    // generated types:
    // deleting the error comparison *and* flattening the emitter left the gate
    // green, because `entry.params` on the model side still spelled `.params`
    // somewhere.
    let elsewhere =
        "for entry in &catalog.emails { assert_eq!(Email::x.params(), entry.params, \"x\"); }";
    assert!(
        compares_this_value(elsewhere, &["Email".to_owned()], &accessor),
        "this fixture is meant to compare Email::params"
    );
    assert!(
        !compares_this_value(elsewhere, &names, &accessor),
        "a comparison of another type's value was counted as this one's"
    );
}

/// A function's body ends at its own closing brace, not at the first line-start one.
///
/// **`"\n}"` is the end of the enclosing `impl`, not of the function.**
/// Generated code is indented, so the first unindented `}` after a function is
/// the block it lives in — and `emitted_sources()` pools every generated file,
/// most of the tree's `impl`s among them. Measured, bounded by `"\n}"`: an
/// entry in `INDIRECT` claiming `ErrorCode::description()` reads `code`, which
/// it does not, passes every assertion in
/// `the_indirect_list_is_not_a_place_to_hide` because `candidate.code()`
/// appears far further down the same `impl`, and a faithful replay finds other
/// false triples that pass, including `ErrorCode::code` through four different
/// calls.
#[test]
fn a_function_body_stops_at_its_own_brace() {
    // The shape generated code actually has: an indented function inside an
    // `impl`, with a sibling below it that mentions what the first does not.
    let source = "\
 (&self) -> &str {
        match self {
            Self::A => \"a\",
        }
    }

    pub const fn other(&self) -> bool {
        self.code().is_empty()
    }
}
";
    let body = function_body(source);
    assert!(body.contains("Self::A"), "{body:?}");
    assert!(
        !body.contains(".code()"),
        "the body reached into the function below it: {body:?}"
    );
    // Nested braces inside the body are counted, not stopped at.
    assert!(
        function_body(" () { if x { y } else { z } }\n").contains("else"),
        "a nested block ended the body early"
    );
    // And a signature with no body at all is empty rather than everything after
    // it — a trait method's declaration, which generated code also carries.
    assert_eq!(function_body("(&self) -> bool;\n"), "");
}

#[test]
fn the_iterating_comparisons_really_iterate() {
    // **`BY_ITERATION` is a claim, and a claim needs its own test.** Saying a
    // file is compared by iteration is worth nothing if the test that iterates
    // stops doing it — the entry would then silence every value in a generated
    // file, which is the exclusion-list failure this whole file is written
    // against.
    //
    // What is checked is what makes the coverage real: the comparison is one
    // this gate reads at all, it walks the catalog's own entries, and it builds
    // the names it asserts on rather than writing them.
    assert!(
        BY_ITERATION.len() <= 2,
        "{} file(s) are excused as covered by iteration. Each one silences every \
         value in a generated file, so this list is bounded the way `INDIRECT` is \
         — growing it is a conversation, not an edit",
        BY_ITERATION.len()
    );
    for (generated, comparison, collection, why) in BY_ITERATION {
        // **The collection is the generated file's own stem.** Without this the
        // third field is a free string, and a row could claim `events` while
        // excusing `limits.rs` — which is the hole this list exists to close.
        let stem = std::path::Path::new(generated)
            .file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or_default();
        assert_eq!(
            stem, collection,
            "`{generated}` is excused as covered by a walk over `catalog.{collection}`, \
             and the file the compiler writes from that collection is `{stem}.rs`. One \
             of the two is wrong, and either way this row excuses a file the named walk \
             does not reach"
        );
        assert!(
            COMPARISONS.contains(&comparison),
            "`{comparison}` covers `{generated}` and is not one of the files this gate \
             reads, so nothing checks it"
        );
        assert!(
            why.len() > 40,
            "`{generated}`'s excuse is too short to be a reason: {why}"
        );
        assert!(
            root().join(generated).is_file(),
            "`{generated}` is excused and the compiler emits no such file"
        );
        // **The path, not the name.** Two generated files are called
        // `events.rs`, and keying this list on the name excused both — 30 of 83
        // emitted values, including every field of `EventSpec` and `FieldSpec`,
        // which the excuse's own reason says nothing about.
        assert!(
            generated.contains('/'),
            "`{generated}` is a bare file name, and two generated files share one"
        );
        // **Code, not prose — the same rule `comparisons` applies.** Needles
        // tested against the raw file let a comment stand in for a test, inside
        // the one file whose own doc comment explains at length why that is not
        // enough. Measured: the covering test deleted outright, replaced by the
        // single line
        //
        // ```text
        // // Moved: no_event_struct_holds_a_member_the_registry_does_not_declare
        // //        now lives with the compiler's own suite.
        // ```
        //
        // and `pub const fn shard(&self) -> u8 { 7 }` emitted onto every
        // generated event struct: `check passed`, with a comment carrying the
        // whole *emitted ⊆ declared* guarantee.
        let text: String = std::fs::read_to_string(root().join(comparison))
            .unwrap_or_else(|error| panic!("{comparison} is readable: {error}"))
            .lines()
            .map(without_comment)
            .collect::<Vec<_>>()
            .join("\n");
        // **Built from the row, not written here.** As a literal this needle is
        // a property of the covering file alone: it is satisfied by any row
        // pointed at a test that walks events, whatever that row excuses.
        let walks = format!("for entry in &catalog.{collection}");
        assert!(
            text.contains(&walks),
            "`{comparison}` does not walk `catalog.{collection}`, so `{generated}` is \
             excused by a test that does not cover it"
        );
        // The derived name, which is what makes it cover a field nobody wrote
        // down. Written from two halves so this line is not itself the match.
        let derives = format!("snake(&field.{})", "name");
        assert!(
            text.contains(&derives),
            "`{comparison}` no longer builds field names from the catalog, so it \
             compares whatever it happens to spell"
        );

        // **And it checks the other direction, which is what excuses the file.**
        // The comparison walks the registry and asserts *declared ⊆ emitted* —
        // every entry has a struct, every field has a line. That says nothing
        // about *emitted ⊆ declared*, so without the other direction a value the
        // compiler writes that the registry never asked for is compared by
        // nobody. Measured: a `pub const fn shard(&self) -> u8 { 7 }` emitted
        // onto every event struct gave `check passed` — a fabricated value on
        // every generated log event, which is the demonstration this gate's own
        // header opens with.
        //
        // An excuse for a whole file has to cover the whole file, so the covering
        // comparison has to say that nothing else is in it.
        assert!(
            text.contains("no_event_struct_holds_a_member_the_registry_does_not_declare"),
            "`{comparison}` checks that every declared value is emitted and not that \
             every emitted value is declared, so `{generated}` can carry a value \
             nothing asked for"
        );
    }
}

#[test]
fn an_accessor_is_recognised_however_its_signature_is_written() {
    // **Two shapes a narrower walk cannot see.** Requiring the literal `(self)`
    // on one line leaves a `&self` accessor — how any accessor returning a
    // borrow has to be written — and one whose signature rustfmt wrapped past
    // 100 columns outside the gate; and requiring field names to be all
    // lowercase letters makes a generated field called `argon2id` or `sha256`
    // invisible, which is a shape `catalog/limits.toml` declares.
    //
    // Neither shape exists in the tree today, which is precisely why they need a
    // test: a fix nothing exercises is one that can be deleted with the suite
    // green, and the day the emitter writes one of these is not the day to find
    // out the walk cannot read it.
    for parameters in ["self)", "&self)", "& self)", "self,)", "&self,", "&self"] {
        assert!(
            takes_only_the_receiver(parameters),
            "`fn x({parameters}` is an accessor and was not read as one"
        );
    }
    for parameters in [
        "self, index: usize)",
        "&self, locale: Locale)",
        "index: usize)",
    ] {
        assert!(
            !takes_only_the_receiver(parameters),
            "`fn x({parameters}` takes an argument and was read as an accessor"
        );
    }

    // **And a digit is part of a field name.** `argon2id` and `sha256` are the
    // shapes `catalog/limits.toml` declares, and the walk required every
    // character to be a lowercase letter or an underscore.
    let source = "pub struct Probe {\n    pub argon2id: u32,\n    pub sha256: u32,\n}\n";
    let scratch = std::env::temp_dir().join(format!("nc-fields-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch).expect("writable");
    std::fs::write(scratch.join("probe.rs"), source).expect("writable");
    let found = fields_of(&std::fs::read_to_string(scratch.join("probe.rs")).expect("readable"));
    assert!(
        found.contains(&"argon2id".to_owned()) && found.contains(&"sha256".to_owned()),
        "a field with a digit in its name was invisible: {found:?}"
    );
    let _ = std::fs::remove_dir_all(&scratch);
}

/// Every `pub` field name one piece of source declares.
///
/// `emitted()`'s own predicate, driven over three lines rather than over a whole
/// generated crate on disk — which is what the walk needs and what a test of one
/// clause should not.
fn fields_of(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            the_field_declared_by(line.trim().strip_prefix("pub ")?).map(str::to_owned)
        })
        .collect()
}

/// The field a `pub ` declaration names, or `None` when it declares something
/// else.
///
/// **The one place the rule is written**, for `emitted()` and [`fields_of`]
/// both. With a copy in each, the copy the tests drive need not be the one the
/// walk calls: the digit clause can then be deleted from the live walk with the
/// suite green, including the test written to keep it alive.
///
/// **A digit is part of a field name.** A rule requiring every character to be
/// a lowercase letter or an underscore makes a generated field called
/// `argon2id` or `sha256` invisible to the gate — and `catalog/limits.toml`
/// declares exactly that shape.
///
/// **And no exclusions by keyword.** Refusing `pub fn `, `pub const `,
/// `pub struct `, `pub enum `, `pub mod ` and `pub use ` by name decides
/// nothing — every one of those keywords ends in a space, which the character
/// class below already refuses, because a field name is lowercase letters,
/// digits and underscores. Measured: all six deleted, the workspace suite and
/// `check` green. A clause that decides nothing is a clause nobody can tell is
/// wrong, and a list of them under a doc saying each is load-bearing is worse
/// than none.
fn the_field_declared_by(rest: &str) -> Option<&str> {
    let name = rest.split(':').next()?;
    (rest.contains(':')
        && !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
    .then_some(name)
}

#[test]
fn a_comparison_is_code_and_a_comment_is_not() {
    // **The hole, tested rather than the rule.** Mentioning a type in a comment
    // inside a chunk that happens to call the accessor satisfies a match over
    // text, so one line of prose plus one deleted assertion ships
    // `ErrorCode::params()` as `&[]` for every code with `check passed`.
    assert_eq!(
        without_comment("    // ErrorCode is compared elsewhere."),
        "    "
    );
    assert_eq!(
        without_comment("//! ErrorCode's `.params()` is compared below."),
        ""
    );
    assert_eq!(
        without_comment("    assert_eq!(ErrorCode::ALL.len(), 9); // and the count"),
        "    assert_eq!(ErrorCode::ALL.len(), 9); "
    );
    // A `//` inside a string is not a comment, and this crate's comparisons name
    // endpoints that hold one.
    assert_eq!(
        without_comment(r#"    let e = "http://127.0.0.1:9428"; // the store"#),
        r#"    let e = "http://127.0.0.1:9428"; "#
    );
    // An escaped quote must not flip the state back and swallow the line.
    assert_eq!(
        without_comment(r#"let a = "x\" // y"; let b = 1;"#),
        r#"let a = "x\" // y"; let b = 1;"#
    );
    // **And a block comment, which a line reader cannot see at all.** "A
    // comment pays for a call" is closed for `/* … */` as well as for `//`:
    // without this, the single comparison of an accessor wrapped in one leaves
    // the whole transcript byte-identical, measured.
    assert_eq!(
        without_block_comments("let a = 1; /* ErrorCode::params() */ let b = 2;"),
        "let a = 1;  let b = 2;"
    );
    assert_eq!(
        without_block_comments("a /* one /* two */ still */ b"),
        "a  b"
    );
    assert_eq!(
        without_block_comments("let s = \"/* not a comment */\"; let b = 1;"),
        "let s = \"/* not a comment */\"; let b = 1;"
    );
    // A newline inside one survives, so a line number is still a line number.
    assert_eq!(without_block_comments("a /* one\ntwo */ b"), "a \n b");
    // **A character literal is one quote and opens nothing.** Without that,
    // `'"'` leaves this reader believing a string is open, after which the
    // comment behind it is read as code — the shape that, planted in `wall.rs`'s
    // copy, turns a comment into a logging call the gate then refuses for being
    // too long.
    assert_eq!(
        without_comment(r#"    let quote = '"'; // `.params()` writes one"#),
        r#"    let quote = '"'; "#
    );
    // An escaped one, and the backslash literal itself.
    assert_eq!(
        without_comment(r"    let tab = '\t'; // and a comment"),
        r"    let tab = '\t'; "
    );
    assert_eq!(
        without_comment(r"    let back = '\\'; // and a comment"),
        r"    let back = '\\'; "
    );
    // A lifetime has no closing quote, so it is not a literal — reading one as
    // an opener is how a scanner runs out of phase for the rest of the line.
    assert_eq!(
        without_comment("fn f<'a>(x: &'a str) -> &'a str { x } // the tail"),
        "fn f<'a>(x: &'a str) -> &'a str { x } "
    );

    // And no comment line survives into what the match reads, or the assertions
    // above are about a function nothing calls. A `//` that is *inside* a string
    // does survive, correctly — `the_registry_reaches_the_code.rs` builds the
    // text `format!("/// {article} {words}.")` — so what this asserts is that no
    // line is a comment, not that no line holds two slashes.
    let mut stripped = 0_usize;
    for (file, text) in comparisons() {
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with("//"),
                "{file} reached the match with a comment line: {line}"
            );
        }
        stripped += 1;
    }
    assert!(stripped > 10, "only {stripped} chunk(s) were read");
}

/// A character literal's quote does not open a string for the block stripper.
///
/// **A char-literal arm in the block stripper too, not only in the line
/// reader.** Without it the stripper, which runs *before* `without_comment`,
/// takes `'"'` — one character, written in this very file — as opening a string
/// for everything after it: the next `/* … */` comes through whole and the line
/// reader judges a comment as code. A lifetime is the other half, and reading
/// one as an opener is how a scanner runs out of phase for a whole file.
#[test]
fn a_char_literal_does_not_open_a_string_for_the_block_stripper() {
    let quote = ['\'', '"', '\''].iter().collect::<String>();
    let spent =
        without_block_comments(&format!("let c = {quote}; /* a comment */ let kept = 1;\n"));
    assert!(
        !spent.contains("a comment"),
        "a block comment after a char literal was kept: {spent}"
    );
    assert!(
        spent.contains("let kept = 1;"),
        "the code after the comment was lost: {spent}"
    );
    // And a lifetime, which has no closing quote: reading one as a literal
    // swallows the characters after it, and reading it as a string opener
    // swallows the rest of the file.
    let spent = without_block_comments("fn f(s: &'static str) { /* gone */ }\nlet kept = 2;\n");
    assert!(
        !spent.contains("gone"),
        "a lifetime left the stripper unable to see a block comment: {spent}"
    );
    assert!(
        spent.contains("&'static str") && spent.contains("let kept = 2;"),
        "a lifetime was read as a literal and took the code with it: {spent}"
    );
}

#[test]
fn the_indirect_list_is_not_a_place_to_hide() {
    // **A list that asks too little silences any name.** Asking only that the
    // value be emitted, that the covering call appear *anywhere* in the pooled
    // text, and that the excuse exceed forty characters — all trivially true
    // for any name — lets one three-line entry plus a broken emitter leave
    // `NC-JRNL-0001` saying `retryable: true` with `check passed`, measured.
    //
    // So an excuse has to name the type, and the covering call has to be
    // compared *against that type* — which is the same bar every real
    // comparison already meets.
    let comparisons = comparisons();
    let emitted = emitted();
    for (name, type_, through, why) in INDIRECT {
        assert!(
            emitted
                .iter()
                .any(|(_, owner, emitted, _)| owner == type_ && emitted == name),
            "INDIRECT names `{type_}::{name}` and the compiler emits no such value"
        );
        // The covering call is itself compared, against the same type, in one
        // file — or the excuse is a chain that ends nowhere.
        assert!(
            comparisons
                .iter()
                .any(|(_, text)| { text.contains(&format!("{through}(")) && text.contains(type_) }),
            "`{type_}::{name}` is excused as covered through `{through}`, and no \
             comparison calls `{through}` against `{type_}`"
        );
        // **And it names the test that would fail.** A length check lets any
        // sentence past: one three-line entry excuses `ErrorCode::retryable`
        // and a broken emitter ships `NC-JRNL-0001` as retryable with
        // `check passed`, measured. So an excuse has to point at a test, and
        // that test has to exist.
        let named = why
            .split_whitespace()
            .map(|word| word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_'))
            .find(|word| word.contains('_') && word.len() > 20)
            .unwrap_or_default();
        assert!(
            !named.is_empty(),
            "`{type_}::{name}`'s entry names no test that would fail if the value were \
             wrong: {why}"
        );
        let body = comparisons
            .iter()
            .find(|(_, text)| text.contains(&format!("fn {named}(")))
            .map(|(_, text)| text.as_str());
        assert!(
            body.is_some(),
            "`{type_}::{name}` is excused by `{named}`, and no comparison declares a test \
             of that name"
        );
        // **And that test reads the excused value.** Every assertion above is
        // about the *chain* — the value is emitted, the covering call is
        // compared, a test of that name exists — and none of them is about the
        // value itself. Measured, without this: one entry excusing `ErrorCode::retryable`
        // through `description`, which is genuinely compared, naming a test that
        // genuinely exists, plus the deletion of the one assertion that read
        // `.retryable()` and an emitter flattened to `true` — and
        // `NC-JRNL-0001`, "the journal's hash chain is broken", ships as
        // retryable to every caller with `check passed`, while the published
        // page still says `no`.
        //
        // An excuse is a claim that *this test would fail if the value were
        // wrong*, and that only holds if the covering call **reads** the value.
        // Checked in the generated source rather than in the test, because that
        // is where the reading happens: `message()`'s body is
        // `MESSAGES[row].1[locale.index()]`, so a wrong `index` returns the
        // wrong sentence and the test fails. `description()`'s body does not
        // mention `retryable`, so no test of `description` can fail for a wrong
        // `retryable` — which is exactly the chain the exploit walked through.
        // **And the test named actually calls it.** This looked the body up and
        // threw it away — `let _ = body;` — directly under a comment explaining
        // what reading it would prove. A test that never calls `{through}`
        // cannot fail when `{through}` returns the wrong thing, whatever the
        // excuse says.
        let body = body.unwrap_or_default();
        assert!(
            body.contains(&format!("{through}(")),
            "`{type_}::{name}` is excused because `{named}` would fail, and `{named}` \
             never calls `{through}`"
        );
        // **Bounded by braces, not by `"\n}"`.** Generated code is indented, so
        // the first `"\n}"` after a function is the end of the whole `impl`
        // block — and `emitted_sources()` pools every file, most of them
        // hand-written. Measured, bounded by `"\n}"`: an entry claiming
        // `ErrorCode::description()` reads `code`, which it does not, satisfies
        // every assertion here because `candidate.code()` appears far further
        // down the same impl, and a faithful replay finds other false triples
        // that pass, including `ErrorCode::code` through four different calls.
        // **On that type, and as the shape it is.** A read of `.{name}(` *or*
        // `.{name}` is an `||` whose second half matches everything the first
        // does, which makes the first half dead: a substring, not a read — and
        // it does not ask which type the covering call is about. Measured with
        // that read: false triples still pass, one verified end to end —
        // `("name", "Locale", "command", …)` added to this list leaves every
        // test in this file green, because
        // `fn command(name: &str)`'s body holds `candidate.name == name` and
        // that is all a substring test asks. Nothing about `command()` can fail
        // when a locale's endonym is wrong.
        //
        // So an accessor has to be *called*, a field has to be *read*, and the
        // covering function has to be about the type — which its signature says,
        // even when its body reaches the value through a binding.
        let is_accessor = emitted
            .iter()
            .any(|(_, owner, member, accessor)| owner == type_ && member == name && *accessor);
        let generated: String = emitted_sources().join("\n");
        let reaches = generated
            .split(&format!("fn {through}"))
            .skip(1)
            .any(|after| {
                let signature = after.split('{').next().unwrap_or_default();
                let body = function_body(after);
                let about_the_type = signature.contains(type_) || body.contains(type_);
                let read = if is_accessor {
                    body.contains(&format!(".{name}("))
                } else {
                    body.contains(&format!(".{name}"))
                };
                about_the_type && read
            });
        assert!(
            reaches,
            "`{type_}::{name}` is excused as reached through `{through}`, and `{through}`'s \
             own implementation never reads `{name}` — so no test of `{through}` can fail \
             when `{name}` is wrong, which is what the excuse claims"
        );
    }
    // And the list stays short: an exclusion that grows is a gate being turned
    // off one entry at a time.
    assert!(
        INDIRECT.len() <= 3,
        "INDIRECT has {} entries; each is a value nothing names, and the list was \
         written for the one case where a caller reaches the value through another \
         call",
        INDIRECT.len()
    );
}
