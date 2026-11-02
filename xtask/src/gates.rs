//! Gates over the source tree.
//!
//! Three rules shape every gate here, and each was paid for elsewhere in this
//! fleet:
//!
//! **A finding is a returned value, not an error.** One pass reports everything
//! it found, because fixing one problem to be shown the next is a miserable loop.
//!
//! **A finding says what to do, in its own field.** `what` is the defect and
//! `fix` is the remedy, and they are separate because a single `message` can only
//! be checked by its length — which a reviewer duly defeated by writing a long
//! sentence containing no remedy at all. Two fields make "there is a remedy" a
//! structural property instead of a word count.
//!
//! **An exclusion list is a claim and needs its own test.** `SKIP` is exactly how
//! a gate gets quietly turned off for the code it exists to guard, so `COVERED`
//! names the directories the gates are *for* and two tests prove `SKIP` does not
//! swallow them — one at the string level and one by actually walking.

use std::path::{Path, PathBuf};

use crate::relative_to;

/// One thing a gate objected to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Finding {
    /// Repository-relative, forward slashes.
    pub(crate) file: String,
    /// 1-indexed.
    pub(crate) line: usize,
    /// What is wrong. One sentence, naming the offending thing.
    pub(crate) what: String,
    /// What to do about it. Never a restatement of `what`.
    pub(crate) fix: String,
}

/// Directory names that are build output or somebody else's tree, matched at
/// **any** depth.
///
/// Any depth and not anchored at the root, because a nested `target/` is still <!-- allow-reference: `target/` is build output, which CARGO_TARGET_DIR may put elsewhere -->
/// build output. `noctua-design` learned this the hard way: a second package
/// created a nested `node_modules`,
/// its root-anchored entry did not match, and the gates began walking vendored
/// output through a symlink.
const SKIP: [&str; 3] = [".git", "target", "node_modules"];

/// The only three names `SKIP` may hold.
///
/// **`COVERED` cannot see this class of hole and had no chance to.** It lists
/// directories by *path* — `crates/nc-journal/src`, `catalog`, `policy` — and a
/// skip is matched by *name at any depth*. Adding `"tests"` to `SKIP` therefore
/// passed both of `COVERED`'s guards while switching the type-wall source gate
/// off over every integration test in the repository: 72 source files became
/// 61, a live `String::leak()` in `crates/nc-journal/tests/` went unreported,
/// and `check passed`.
///
/// So `SKIP` is closed rather than guarded. Every entry is build output or
/// somebody else's tree; there is no fourth kind, and a change here has to be a
/// deliberate edit to this list with a reason beside it.
#[cfg(test)]
const NOTHING_ELSE_MAY_BE_SKIPPED: [&str; 3] = [".git", "target", "node_modules"];

/// The directories the gates exist for.
///
/// Not used at run time — it is the subject of the two tests that keep `SKIP`
/// honest. Adding a directory here is how a later stage says "the gates must
/// reach this too", and a directory holding no files yet cannot be listed: the
/// walk-level test would fail for the absence rather than for a skip.
#[cfg(test)]
const COVERED: [&str; 20] = [
    "crates/nc-authz/src",
    "crates/nc-canon/src",
    "crates/nc-catalog/src",
    "crates/nc-catalog-compiler/src",
    "crates/nc-guest/src",
    "crates/nc-ids/src",
    "crates/nc-journal/src",
    "crates/nc-kernel/src",
    "crates/nc-local/src",
    "crates/nc-log/src",
    "crates/nc-policy/src",
    "crates/nc-prune/src",
    "crates/nc-schema/src",
    "xtask/src",
    "catalog",
    "policy",
    // The plugins, which are members and ship as the components the kernel
    // loads.
    "plugins",
    // **The directory the unbounded-table gate is entirely about.** It was not
    // here, so `SKIP` could be given `"migrations"` and the section-citation
    // gate dropped fifteen citations with `check passed`.
    "migrations",
    // And the local database's, which the same gate reads in its own dialect.
    "local-migrations",
    // **And the generated developer surfaces**, which are normative (`§15.3`).
    // `"docs"` in `SKIP` took six committed files out of the `.env` walk and out
    // of the section-citation gate — 130 paths to 124, 522 citations to 516 —
    // with both of the tests that exist to keep `SKIP` honest still green.
    "docs",
];

/// Every manifest in the tree must reach the colour system through the design
/// system.
///
/// **Why this gate and not a dependency.** `noctua-design` does not depend on
/// `noctua-colors` as a crate at all: it *vendors* the generated colour output
/// and ships it to consumers as the stylesheets it renders against
/// (`NOCTUA.md §8.6`). A direct dependency here would therefore not be a
/// shortcut to the same bytes — it would be a **second copy of the colour
/// system**, at whatever version this repository pinned, beside the one the
/// design system actually draws. The two agree on the day they are written and
/// never again.
pub(crate) fn colours_come_through_the_design_system(file: &str, text: &str) -> Vec<Finding> {
    // **Parsed, not scanned.** Two spellings walked past a text scan in
    // succession: a `#` inside a quoted value cut the line at the wrong place,
    // and then `"noctua\u002Dcolors" = "0.4"` held no literal `noctua-colors`
    // anywhere in the file at all. Measured, that key added to the root
    // `[workspace.dependencies]`:
    // `ok 10 manifest(s) reach the colour system through the design system`,
    // `check passed` — while `tomllib` and cargo both read the key as
    // `noctua-colors`. `Cargo.toml`'s own comment says this gate fails if any
    // manifest here names it.
    //
    // A scanner narrower than the language it reads is this file's most common
    // defect, and TOML has escapes, quoting, dotted keys and `package` renames.
    // The `toml` crate is already a dependency here — `design_dependency` uses
    // it — so the manifest is read the way cargo reads it.
    let mut findings = Vec::new();
    let Ok(manifest) = toml::from_str::<toml::Value>(text) else {
        // A manifest that does not parse is cargo's failure to report, not
        // this rule's, and every manifest in this tree is built in the same run.
        return findings;
    };
    let named: Vec<String> = the_dependencies_of(&manifest);
    // **Contains, not equals.** The scan this replaced reported
    // `noctua-colors-tokens` too, and a crate whose name is the colour system's
    // with something on the end is the same second copy under a different
    // package — narrowing to an exact match would have been a repair that took
    // a rule away. The parser is what closes the escape and the rename; the
    // breadth is kept.
    if named.iter().any(|name| name.contains("noctua-colors")) {
        findings.push(Finding {
            file: file.to_owned(),
            // The line the name is written on when it is written plainly, and
            // the top of the file when it is spelled some other way — the
            // finding names the manifest either way.
            line: text
                .lines()
                .position(|line| line.contains("noctua-colors"))
                .map_or(1, |at| at + 1),
            what: "this manifest names `noctua-colors` directly".to_owned(),
            fix: "the design system does not depend on the colour crate — it vendors \
                  the generated output and ships it as the stylesheets it renders \
                  against (NOCTUA.md §8.6), so this is not a shortcut to the same \
                  bytes but a second copy of the colour system at a version nothing \
                  keeps in step. Take the tokens from what the design system draws"
                .to_owned(),
        });
    }
    findings
}

/// Every crate a manifest declares a dependency on, by the name cargo resolves.
///
/// The key, and the `package` value when one renames it — `colors = { package =
/// "noctua-colors" }` is a dependency on the colour crate under another name.
/// Every table cargo reads: the three kinds, the workspace's own, and the
/// target-specific ones.
fn the_dependencies_of(manifest: &toml::Value) -> Vec<String> {
    let kinds = ["dependencies", "dev-dependencies", "build-dependencies"];
    let mut tables: Vec<&toml::Value> = Vec::new();
    for kind in kinds {
        if let Some(table) = manifest.get(kind) {
            tables.push(table);
        }
        if let Some(table) = manifest.get("workspace").and_then(|it| it.get(kind)) {
            tables.push(table);
        }
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for (_, target) in targets {
                if let Some(table) = target.get(kind) {
                    tables.push(table);
                }
            }
        }
    }
    let mut named = Vec::new();
    for table in tables {
        let Some(table) = table.as_table() else {
            continue;
        };
        for (key, value) in table {
            named.push(key.clone());
            if let Some(renamed) = value.get("package").and_then(toml::Value::as_str) {
                named.push(renamed.to_owned());
            }
        }
    }
    named
}

/// One step of a `/* … */`, which nests and spans lines.
///
/// Newlines are kept so every finding's line number still counts from the top of
/// the file — a stripper that shortened the text would report the wrong line for
/// everything after the first comment. Block comments are Rust's alone, and they
/// nest, so the caller carries the depth rather than a flag.
fn inside_a_block_comment(
    bytes: &[char],
    index: usize,
    block: &mut usize,
    out: &mut String,
) -> usize {
    let here = bytes[index];
    let next = bytes.get(index + 1).copied();
    if here == '*' && next == Some('/') {
        *block -= 1;
        return index + 2;
    }
    if here == '/' && next == Some('*') {
        *block += 1;
        return index + 2;
    }
    if here == '\n' {
        out.push('\n');
    }
    index + 1
}

/// Blanks the raw string starting at `index`, or answers `None` if there is none.
///
/// Lifted out of [`code_without_text`] so that function stays inside clippy's
/// line limit — splitting it is the only reason, and the arm is written here
/// whole rather than spread across two.
#[cfg(test)]
fn blank_raw_string(source: &[char], mut index: usize, out: &mut String) -> Option<usize> {
    let blank = |out: &mut String, character: char| {
        out.push(if character == '\n' { '\n' } else { ' ' });
    };
    let mut hashes = 0_usize;
    while source.get(index + 1 + hashes) == Some(&'#') {
        hashes += 1;
    }
    if source.get(index + 1 + hashes) != Some(&'"') {
        return None;
    }
    for _ in 0..=hashes + 1 {
        blank(out, source[index]);
        index += 1;
    }
    while index < source.len() {
        if source[index] == '"'
            && (0..hashes).all(|step| source.get(index + 1 + step) == Some(&'#'))
        {
            for _ in 0..=hashes {
                blank(out, source[index]);
                index += 1;
            }
            break;
        }
        blank(out, source[index]);
        index += 1;
    }
    Some(index)
}
#[cfg(test)]
mod what_ships_is_not_the_tests {
    /// What [`super::without_test_modules`] hands back holds no test.
    ///
    /// **Nothing decided the wrapper at all.** Replacing its whole body with
    /// `source.to_owned()` left `check passed` — and with it the defect it
    /// exists for came straight back: the one production `CARGO_TERM_COLOR`
    /// call changed to `"auto"` with the needle written only inside a test's
    /// assertion message, green. Measured by a reviewer on 2026-09-12.
    ///
    /// The two directions, because a wrapper that blanks *everything* also
    /// holds no test: what ships must still carry the item each caller is
    /// written to find.
    #[test]
    fn what_ships_holds_no_test_and_still_holds_the_code() {
        let here = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        for (named, shipped) in [
            ("check.rs", "fn run("),
            ("gates.rs", "pub(crate) fn walk("),
            ("dev.rs", "fn compose_command("),
        ] {
            let text = std::fs::read_to_string(here.join(named)).expect("a source of this crate");
            let ships = super::without_test_modules(&text);
            let marker = ["#[", "test]"].join("");
            // A line whose whole content is the attribute — prose naming it is
            // prose, and `check.rs` carries a paragraph about "the census over
            // `#[test]` attributes" that is not one.
            let survivor = ships
                .lines()
                .position(|line| line.trim() == marker)
                .map(|at| at + 1);
            assert!(
                survivor.is_none(),
                "`{named}` still holds a test at line {survivor:?} after its test modules \
                 were blanked, so what a source assertion reads as the code that ships is \
                 most of the file's own tests"
            );
            assert!(
                ships.contains(shipped),
                "`{named}` lost `{shipped}` as well, so this blanked the file rather \
                 than its tests and every needle below reads nothing"
            );
        }
    }
}

/// The file with every `#[cfg(test)]` module blanked out, character for
/// character.
///
/// **Neither the first `#[cfg(test)]` nor the last is where production ends.**
/// Two source assertions in `check.rs` cut the file at
/// `rfind("#[cfg(test)]")` under a comment saying that puts test code where a
/// source assertion cannot see it. It does the opposite: the last one in that
/// file opens a module fourteen thousand lines below where production stops, so
/// the "production" half carried every test in the file, those two assertions
/// included. Measured on 2026-09-11 — the one production
/// `CARGO_TERM_COLOR` call set to `"auto"` and the needle written into an
/// ordinary `//` comment inside a test — `check passed`, `1313 test(s)`. The
/// *first* occurrence is no better: that file holds a test-only helper module
/// eight hundred lines above its own tests, which is why the cut was moved in
/// the first place.
///
/// **And the first answer to that was a third copy of a question this file
/// already answers.** It scanned lines, armed on a bare `#[cfg(test)]` and took
/// the next line as the item — so an attribute standing above a *doc block*, or
/// above a `const X: […] = [`, armed and disarmed without blanking anything.
/// Measured by a reviewer on 2026-09-12: 43 of `check.rs`'s 63 `#[cfg(test)]`
/// items survived into what it called the code that ships, `WHAT_THE_GATE_CAN_
/// SAY`'s 728 literals among them — and replacing the whole body with
/// `source.to_owned()` left `check passed`, so nothing decided it either.
///
/// [`the_test_modules`] is the reader that answers this correctly, with the
/// four failed attempts recorded in its own doc and a cross-check beside it.
/// This is a thin wrapper over it, and the test below is what says the wrapper
/// cut anything at all.
///
/// Blanked rather than deleted, so an offset here is an offset in the file and
/// an indented needle still matches its own indentation. A file whose modules
/// never close comes back **empty** rather than whole: a caller looking for a
/// needle then fails, which is the direction that gets read.
#[cfg(test)]
pub(crate) fn without_test_modules(source: &str) -> String {
    let Some(modules) = the_test_modules(source) else {
        return String::new();
    };
    source
        .char_indices()
        .map(|(at, character)| {
            if modules.iter().any(|module| module.contains(&at)) {
                if character == '\n' { '\n' } else { ' ' }
            } else {
                character
            }
        })
        .collect()
}

/// The Rust in `text`, with every comment and every string literal blanked.
///
/// **A needle can be spelled as data.** `without_comments` keeps string
/// literals — deliberately, because a gate that reads endpoints and error codes
/// needs them — so a test that looks for a *call* in its own source is
/// satisfied by any literal holding the same characters. Measured: the §15.1
/// walk's assertion deleted and this one line added beside the list that
/// records it as covered,
///
/// ```text
/// const _: &str = r#"reached_every_one(&findings, &planted, "the §15.1 walk")"#;
/// ```
///
/// left `every_walk_that_needs_the_fixture_still_calls_it` green, and a filter
/// naming `crates/nc-journal` then hid a hand-written error code with every
/// printed number at its baseline. Removing that one line, with the call still
/// deleted, is what turned the test red — the literal was the whole difference.
///
/// Blanked rather than removed, so a *character* offset into the result still
/// points at the same place in the source — one character in, one out. Not a
/// byte offset: a multi-byte character becomes one space, and this file alone
/// drifts 863 bytes. The one caller indexes `Vec<char>` on both sides for that
/// reason. Raw strings are followed by their hash count,
/// which is the form the defeat used; block comments nest, as Rust's do; and a
/// `'` that is a lifetime is not the start of a character literal, which is why
/// the character arm looks ahead for the closing quote.
#[cfg(test)]
pub(crate) fn code_without_text(text: &str) -> String {
    let source: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0_usize;
    let blank =
        |out: &mut String, character: char| out.push(if character == '\n' { '\n' } else { ' ' });
    while index < source.len() {
        let here = source[index];
        let next = source.get(index + 1).copied();
        // A line comment, to the end of the line.
        if here == '/' && next == Some('/') {
            while index < source.len() && source[index] != '\n' {
                blank(&mut out, source[index]);
                index += 1;
            }
            continue;
        }
        // A block comment, which nests.
        if here == '/' && next == Some('*') {
            let mut depth = 0_usize;
            while index < source.len() {
                if source[index] == '/' && source.get(index + 1) == Some(&'*') {
                    depth += 1;
                    blank(&mut out, source[index]);
                    blank(&mut out, source[index + 1]);
                    index += 2;
                    continue;
                }
                if source[index] == '*' && source.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    blank(&mut out, source[index]);
                    blank(&mut out, source[index + 1]);
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                blank(&mut out, source[index]);
                index += 1;
            }
            continue;
        }
        // A raw string: `r`, then some hashes, then a quote. It ends at a quote
        // followed by the same number of hashes.
        if here == 'r'
            && let Some(after) = blank_raw_string(&source, index, &mut out)
        {
            index = after;
            continue;
        }
        // An ordinary string literal, with escapes.
        if here == '"' {
            blank(&mut out, here);
            index += 1;
            while index < source.len() {
                if source[index] == '\\' {
                    blank(&mut out, source[index]);
                    if index + 1 < source.len() {
                        blank(&mut out, source[index + 1]);
                    }
                    index += 2;
                    continue;
                }
                let done = source[index] == '"';
                blank(&mut out, source[index]);
                index += 1;
                if done {
                    break;
                }
            }
            continue;
        }
        // A character literal, told from a lifetime by looking for the close.
        //
        // **`char_literal_at` and not a second reading of the same rule.** This
        // was written out again here and got it wrong in one place: for `'\''`
        // the search began at the *escaped* quote, so the literal closed one
        // character early and the real terminator was re-scanned as a fresh
        // apostrophe. From there the lexer is one quote out of phase — in
        // `matches!(here, '\''|'"')` the stray quote pairs with the `'` two
        // positions along, swallows the `|`, and leaves a `"` to open a phantom
        // string — after which this function reports code as blanked and text as
        // code, which is precisely backwards for the guard that depends on it.
        // Measured, with the §15.1 walk's assertion deleted and two lines
        // planted:
        //
        // ```text
        // const _: bool = matches!('a', '\''|'"');
        // const _: &str = r#"reached_every_one(&findings, &planted, "…")"#;
        //   ->  ok  32745 source line(s) name no error code as text (§15.1)
        //   ->  check passed
        // ```
        //
        // The same `(2..8)` window also missed `'\u{100}'` and every longer
        // escape, which were then emitted whole as code. One reader for one
        // rule: the escape offsets and the eleven-character limit live in
        // `char_literal_at`, which the raw-string and byte-string scanners
        // beside it already use.
        if here == '\''
            && let Some(width) = char_literal_at(&source, index)
        {
            for _ in 0..width {
                blank(&mut out, source[index]);
                index += 1;
            }
            continue;
        }
        out.push(here);
        index += 1;
    }
    out
}

/// Source with its comments removed, respecting string literals.
///
/// **Three strippers in this tree read a `//` inside a string as a comment**, and
/// each one was measured being walked past. `wall.rs` and
/// `every_generated_value_is_compared.rs` are string-aware; these two were not,
/// so a URL in a string literal followed by a compile-time query reported
/// nothing, and a variable "read" only by a `/* … */` block gave
/// `ok 7 declared setting(s) are read somewhere`.
///
/// `line_comment` is `//` for Rust and `#` for TOML, YAML and shell; `--` for
/// SQL. Which characters open a string depends on the same answer, because SQL,
/// shell and YAML quote with `'` and Rust uses it for a character literal.
pub(crate) fn without_comments(text: &str, line_comment: &str) -> String {
    let mut out = String::with_capacity(text.len());
    // **The delimiter that opened the current string, not a flag.** A `bool`
    // cannot tell one quote from another, and in SQL both are quotes with
    // different jobs: `'` opens a string and `"` opens an identifier. Measured
    // with a flag, on a migration:
    //
    // ```text
    // INSERT INTO t VALUES ('a -- b');  -> "INSERT INTO t VALUES ('a \n"
    // SELECT 'the " char'; -- PROBE     -> the `-- PROBE` survives
    // ```
    //
    // — real content cut out of one, and in the other the exact hole the
    // migration-comment fix closed, reopened by one `"` in a string.
    let mut quoted: Option<char> = None;
    let mut escaped = false;
    let mut block = 0_usize;
    // How many `#` the open raw string used; `None` when not inside one.
    let mut raw: Option<usize> = None;
    let rust = line_comment == "//";
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        let here = bytes[index];
        let next = bytes.get(index + 1).copied();
        if block > 0 {
            index = inside_a_block_comment(&bytes, index, &mut block, &mut out);
            continue;
        }
        if escaped {
            escaped = false;
            out.push(here);
            index += 1;
            continue;
        }
        // **A raw string ends at its own hash count, not at the next quote.**
        // `r#"see "https://x" for details"#` holds bare quotes, so counting them
        // left the scan *outside* a string when it met the `//` of `https://` —
        // and the rest of the line, including a `sqlx::query!`, was dropped as a
        // comment. Measured: 1 finding on the plain line, **0** with the raw
        // string in front of it.
        //
        // **The whole opener is consumed, prefix and quote together.** It used
        // to push the `r` and step one character, so for `r"…"` — no hashes —
        // the very next character was the *opening* quote and
        // `closes_raw(_, _, 0)` is `(0..0).all(…)`, which is true: the string
        // closed where it opened. Measured, `r"http://x"` followed by a
        // `sqlx::query!` on the same line gave **0** findings against 1 for the
        // plain-string version. The fix for one shape broke the shape one hash
        // shorter, which is the argument for consuming the opener whole rather
        // than re-reading its last character as content.
        // **Rust's syntax, on Rust.** `char_literal_at` below is guarded by
        // `rust` and this was not, so a raw-string opener was recognised in
        // YAML, shell, TOML and SQL — where `r"` is ordinary text. A raw string
        // deliberately does not reset at a newline, so one `grep -r"x` in a
        // workflow or a compose file put the scanner inside a string for the
        // **rest of the file**:
        //
        // ```text
        // run: grep -r"x
        // # NC_AUDIT_PROBE is prose here.     <- read as code
        // run: cargo test
        // ```
        //
        // Which is `no_unread_variable`'s own stated failure — a gate whose own
        // prose satisfies it is wired to nothing — reintroduced by one
        // character, and costing the whole file rather than one line.
        if rust
            && quoted.is_none()
            && raw.is_none()
            && let Some((opener, hashes)) = raw_string_at(&bytes, index)
        {
            raw = Some(hashes);
            for offset in 0..opener {
                out.push(bytes[index + offset]);
            }
            index += opener;
            continue;
        }
        if let Some(hashes) = raw {
            if here == '"' && closes_raw(&bytes, index + 1, hashes) {
                index += 1 + hashes; // the quote and its hashes
                raw = None;
                // **As wide as it opened.** The opener is copied whole a few
                // lines above, so a closer of one `"` leaves `r#"…"` — a raw
                // string that never ends. This output is not read as prose: five
                // callers hand it to `without_string_contents`, which is itself
                // raw-string aware, finds no close, and blanks the **rest of the
                // file**. Measured: one `r#"…"#` anywhere in `check.rs` took ten
                // of the sixteen floors out of
                // `no_floor_is_handed_a_count_that_cannot_be_zero`, and the
                // count it reported instead read as "a floor was added", which
                // points a maintainer at moving the number.
                out.push('"');
                for _ in 0..hashes {
                    out.push('#');
                }
                continue;
            }
            out.push(here);
            index += 1;
            continue;
        }
        // **A char literal is not a quote.** `let c = '"';` toggled the state
        // and left every `//` on every following line unrecognised — this file
        // holds that literal, so the stripper walked past its own doc comments
        // and reported them. The newline reset below bounds that to one line,
        // and a line is exactly where a `//` lives, so the literal is read as
        // what it is. A lifetime — `'a`, with no closing quote — is not one, and
        // falls through to being ordinary text.
        if rust
            && quoted.is_none()
            && here == '\''
            && let Some(width) = char_literal_at(&bytes, index)
        {
            for offset in 0..width {
                out.push(bytes[index + offset]);
            }
            index += width;
            continue;
        }
        // `"` everywhere; `'` wherever it is a string delimiter rather than a
        // char literal, which is every file type here except Rust.
        let opens_a_string = here == '"' || (!rust && here == '\'');
        match here {
            '\\' if quoted.is_some() => {
                escaped = true;
                out.push(here);
            }
            _ if opens_a_string => {
                quoted = match quoted {
                    Some(delimiter) if delimiter == here => None,
                    Some(delimiter) => Some(delimiter),
                    None => Some(here),
                };
                out.push(here);
            }
            // **Rust's block comment, and only Rust's.** `/* … */` is not a
            // comment in YAML or shell, and treating it as one would cut real
            // content out of those files.
            '/' if quoted.is_none() && rust && next == Some('*') => {
                block = 1;
                index += 2;
                continue;
            }
            _ if quoted.is_none() && text_at(&bytes, index, line_comment) => {
                // The rest of the line is prose.
                while index < bytes.len() && bytes[index] != '\n' {
                    index += 1;
                }
                continue;
            }
            // **A newline does not end a quote, and the sentence here said it
            // did.** *"Rust has no unescaped newline inside a string literal"*
            // is false — a bare newline between quotes is part of the string,
            // and the `\` at a line end is the *continuation* that removes one.
            // [`blanked`]'s own comment already recorded that this premise was
            // wrong about `without_string_contents`; it was wrong here too, and
            // here it cuts text rather than blanking it. Measured:
            //
            // ```text
            // let s = "one
            // // not a comment, this is inside the string
            // two";
            // ```
            //
            // came back as `let s = "one\n\ntwo";` — a line of the *string*
            // deleted as though it were prose, from the function that decides
            // what is code for half the gates in this file.
            //
            // **In Rust only.** This function also reads YAML and SQL, where a
            // bare `"` is ordinary text — `run: grep -r"x` in a workflow is the
            // case the test beside this one pins — and there the reset is what
            // bounds a stray quote to its own line. A `.rs` file that does not
            // parse is a compile error in the same run, so the bound buys
            // nothing there and costs the line above.
            '\n' if !rust => {
                quoted = None;
                out.push(here);
            }
            _ => out.push(here),
        }
        index += 1;
    }
    out
}

/// The opener of a raw string starting at `index`: its width, and its hash count.
///
/// **Every prefix Rust gives one.** This looked for a bare `r` and rejected one
/// preceded by a name character — which is right for `for`, and wrong for `br`
/// and `cr`: a byte raw string was not recognised at all, so
/// `br#"a "http://x" b"#` left the scan outside a string at the `//` and the
/// rest of the line was dropped as a comment. Measured: **0** findings against
/// 1 for the same line with a plain string. The same class as the fix beside
/// it, one prefix along.
pub(crate) fn raw_string_at(chars: &[char], index: usize) -> Option<(usize, usize)> {
    // `r`, `br` and `cr` open one; `b"` and `c"` are ordinary strings with a
    // prefix, and the quote below handles those unchanged.
    let prefix = match (chars.get(index), chars.get(index + 1)) {
        (Some('r'), _) => 1,
        (Some('b' | 'c'), Some('r')) => 2,
        _ => return None,
    };
    // Not a raw string if what precedes it is part of a name: `for` ends in `r`,
    // and `substr` ends in `r` after a `b`.
    if index > 0 && (chars[index - 1].is_alphanumeric() || chars[index - 1] == '_') {
        return None;
    }
    let mut hashes = 0;
    while chars.get(index + prefix + hashes) == Some(&'#') {
        hashes += 1;
    }
    (chars.get(index + prefix + hashes) == Some(&'"')).then_some((prefix + hashes + 1, hashes))
}

/// The width of a Rust char literal at `index`, or `None` for a lifetime.
///
/// `'x'` is three characters, `'\n'` four, `'\u{1F600}'` ten. A lifetime is an
/// apostrophe followed by a name and no closing quote, and reading one as a
/// literal would swallow real code — so anything that does not close inside a
/// literal's width is not one.
pub(crate) fn char_literal_at(chars: &[char], index: usize) -> Option<usize> {
    let escaped = chars.get(index + 1) == Some(&'\\');
    // The longest literal Rust has is `'\u{10FFFF}'`, at eleven characters.
    let limit = if escaped { 11 } else { 3 };
    let mut offset = if escaped { 3 } else { 2 };
    while offset <= limit {
        match chars.get(index + offset) {
            Some('\'') => return Some(offset + 1),
            Some('\n') | None => return None,
            Some(_) => offset += 1,
        }
    }
    None
}

/// Whether `hashes` `#` characters begin at `index`.
fn closes_raw(chars: &[char], index: usize, hashes: usize) -> bool {
    (0..hashes).all(|offset| chars.get(index + offset) == Some(&'#'))
}

/// Whether `needle` begins at `index`.
fn text_at(chars: &[char], index: usize, needle: &str) -> bool {
    needle
        .chars()
        .enumerate()
        .all(|(offset, wanted)| chars.get(index + offset).copied() == Some(wanted))
}

/// The `sqlx` macros that need a database to compile.
///
/// **The promise was written as a feature and is a fact about usage.**
/// `Cargo.toml` says *"Runtime queries only — no `query!` macros — so nothing
/// needs `DATABASE_URL` at build time and there is no `.sqlx` cache to keep in
/// sync with the schema"*, and `AGENTS.md` restated that as the `macros`
/// feature being off. It is not off and cannot be: `sqlx::migrate!` is how
/// `migrations/` is embedded, it needs no database, and it comes from `macros`
/// — removing the feature fails as ``cannot find `migrate` in `sqlx` ``.
///
/// So a feature list cannot say what the promise says. What can is this: the
/// `query!` family checks SQL against a live database *while compiling*, and
/// each of these is one of them. Measured with one added to
/// `crates/nc-prune/src/postgres.rs` and `DATABASE_URL` unset:
///
/// ```text
/// error: set DATABASE_URL to use query macros online, or run cargo sqlx prepare
/// ```
///
/// which is the build-time coupling the sentence promises there is none of, and
/// which would stop `cargo xtask check --offline` from compiling at all.
///
/// **Built from halves, so no needle's spelling sits in this file.** The gate
/// reads every Rust file in the tree, which includes this one — and
/// written out, the list reported itself six times. The alternative is an
/// exclusion for `xtask/src/gates.rs`, which is a hole in the one file where a
/// hole would be least visible; `wall.rs`'s tests build their needles the same
/// way for the same reason.
pub(crate) const COMPILE_TIME_QUERIES: [&str; 12] = [
    concat!("query", "!"),
    concat!("query_as", "!"),
    concat!("query_scalar", "!"),
    concat!("query_file", "!"),
    concat!("query_file_as", "!"),
    concat!("query_file_scalar", "!"),
    // **And the `_unchecked` half of each pair.** `sqlx` ships both forms and
    // both need `DATABASE_URL` at build time — the coupling this list exists to
    // refuse — so naming six of twelve left the other six free. `_unchecked`
    // skips the *type* checking and keeps the connection: it is the one a
    // developer reaches for when the checked form will not compile, which is
    // exactly when the pressure to use it is highest.
    concat!("query_unchecked", "!"),
    concat!("query_as_unchecked", "!"),
    concat!("query_scalar_unchecked", "!"),
    concat!("query_file_unchecked", "!"),
    concat!("query_file_as_unchecked", "!"),
    concat!("query_file_scalar_unchecked", "!"),
];

/// A test in this tree is a test that runs.
///
/// **A suite that is present but inert is announced exactly like one that ran.**
/// `cargo test` prints `Running tests/tampering.rs` either way, so naming the
/// suites — which is what the floor beside the test count does — cannot see it.
/// Measured: `#[ignore = "flaky under parallel load; re-enable once the pool is
/// bounded"]` on all nineteen tests of the journal's tamper-detection suite gave
///
/// ```text
/// ok    603 test(s), and the ones that need the workspace above this repository did not run
/// check passed
/// ```
///
/// with acceptance criterion 3 — the thing the journal exists for — enforced by
/// nothing. The only trace was the total falling from 622, which the count's own
/// doc calls a number that can only ever say "smaller".
///
/// A bare `#[ignore]` is caught by clippy's `ignore_without_reason`; adding the
/// reason, which is what anybody would write anyway, gets past it. So the rule
/// is here instead: this repository's own skips are a `return` that prints why
/// and still counts as a pass, and an attribute that hides a test from the run
/// is an argument to have rather than a line to add in a hurry.
///
/// Doc fences are not this: a ```` ```ignore ```` block is a snippet that must
/// not compile, and `value.rs` uses three to show the leaks the type wall
/// refuses. This reads `#[ignore`, which is an attribute and not a fence.
pub(crate) fn no_ignored_test(file: &str, text: &str) -> Vec<Finding> {
    let stripped = without_comments(text, "//");
    let lines: Vec<&str> = stripped.lines().collect();
    // **Not a list of spellings — the position.** Every version of this gate
    // before it was four spellings long, and the fifth was free:
    // `#[cfg(target_os = "vxworks")]` above a test was invisible to this and to
    // the census in `check.rs`, so hiding a test moved no number.
    //
    // **And not a list of *line shapes* either.** Reading the attribute run as
    // "lines that start with `#[`" left ten more spellings free — a blank line
    // between two attributes, a doc comment between the attribute and the item,
    // a `cfg_attr` `cargo fmt` wrapped across four lines — none of them
    // ingenious, all of them things an author or a formatter writes. So the run
    // is read as text with its brackets balanced, and blank lines inside it stay
    // inside it.
    let attribute_runs = runs(&lines);
    let has_a_test = lines
        .iter()
        .any(|line| is_a_test_attribute(line.trim_start()));
    let mut findings = Vec::new();
    for line in &lines {
        let code = line.trim_start();
        // An inner attribute governs the whole file, so there is no item to look
        // ahead to: it hides every test below it.
        if (code.starts_with("#![cfg(") || code.starts_with("#![cfg_attr(")) && has_a_test {
            let at = lines.iter().position(|other| other == line).unwrap_or(0);
            findings.push(hidden(file, at, "a condition on the whole file"));
        }
    }
    for run in attribute_runs {
        let joined = run
            .attributes
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let about_a_test = run.attributes.iter().any(|(_, text)| is_a_test_attribute(text))
            // **What is in the module, not what it is called.** See
            // `holds_a_test` for the thirty modules this tree has whose names do
            // not begin with `tests`, and for the two tests of slack that made
            // hiding one of them silent everywhere else too.
            || opens_a_module(&run.item) && holds_a_test(&lines, run.at)
            // A `mod x;` declares a whole file, and a condition on it deletes
            // every test in that file without touching it.
            //
            // **Through the same visibility strip as the clause above it**,
            // which this did not use while sitting three lines below one that
            // did. `pub mod entry;` is not `mod `, so one token turned the rule
            // off: measured, `#[cfg(unix)]` above `mod docs;` in
            // `nc-catalog-compiler` is `error … this test is hidden from the run
            // by a condition`, and the same attribute above `pub mod entry;` in
            // `nc-journal` is `check passed, with 4 warn line(s)` with that file
            // named nowhere in the run.
            || {
                let bare = without_a_visibility(run.item.trim_start());
                bare.starts_with("mod ") && bare.ends_with(';')
            };
        if !about_a_test {
            continue;
        }
        for (at, attribute) in &run.attributes {
            let what = if attribute.starts_with("#[ignore") {
                "`#[ignore]`"
            } else if attribute.starts_with("#[cfg_attr(") {
                "a `cfg_attr`, which can add `#[ignore]` under a condition"
            } else if attribute.starts_with("#[cfg(") {
                // `#[cfg(test)]` on a test module is how one is written, and it
                // is the condition `cargo test` sets.
                // **Whatever the item is.** `cfg(test)` is the condition
                // `cargo test` sets, so it hides nothing from the run — this
                // was narrowed to a module called `tests`, then to any module,
                // and both narrowings were a false finding waiting for someone
                // to write `#[cfg(test)] #[test] fn t()`. Any *other* condition
                // in the same run is still reported on its own line.
                if attribute.starts_with("#[cfg(test)]") {
                    continue;
                }
                "a condition"
            } else {
                continue;
            };
            let _ = &joined;
            findings.push(hidden(file, *at, what));
        }
    }
    findings
}

/// One item and the attributes attached to it.
struct Run {
    attributes: Vec<(usize, String)>,
    item: String,
    /// The line the item is written on, so its body can be read.
    at: usize,
}

/// Every item in a file with its attribute run, brackets balanced.
///
/// An attribute may span lines — `cargo fmt` wraps a long `cfg_attr` across four
/// — and blank lines and doc comments may sit inside a run. Both were what made
/// a line-shaped reading of the run miss ten spellings.
fn runs(lines: &[&str]) -> Vec<Run> {
    let mut found = Vec::new();
    let mut attributes: Vec<(usize, String)> = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (at, line) in lines.iter().enumerate() {
        let code = line.trim();
        if let Some((began, mut text)) = pending.take() {
            text.push(' ');
            text.push_str(code);
            if balanced(&text) {
                attributes.push((began, flatten(&text)));
            } else {
                pending = Some((began, text));
            }
            continue;
        }
        if code.is_empty() {
            continue;
        }
        if code.starts_with("#[") || code.starts_with("#![") {
            if balanced(code) {
                attributes.push((at, flatten(code)));
            } else {
                pending = Some((at, code.to_owned()));
            }
            continue;
        }
        if !attributes.is_empty() {
            found.push(Run {
                attributes: std::mem::take(&mut attributes),
                item: code.to_owned(),
                at,
            });
        }
    }
    found
}

/// Whether this item opens a module with a body.
fn opens_a_module(item: &str) -> bool {
    let bare = without_a_visibility(item.trim_start());
    bare.starts_with("mod ") && bare.ends_with('{')
}

/// Whether the module opening at `from` holds a test anywhere inside it.
///
/// **The gate was keyed on the name `tests` and this tree has thirty modules
/// that are not called that** — `query_tests` (50 tests), `the_frozen_path_rule`
/// (52), `hostile_store_tests` (18), `every_report_wire` (16), `narrowing` (7)
/// and more. None of them was `about_a_test`, so a second `#[cfg]` on one was
/// invisible here. Measured, `#[cfg(test)] #[cfg(not(test))]` written above
/// `mod width_tests` in `crates/nc-ids/src/base32.rs`: `cargo test -p xtask`
/// gave 401 passed and `cargo run -p xtask -- check` printed `check passed`,
/// the only trace being `1072 test(s)` becoming `1070`. The census in
/// `check.rs` is the only other thing that could see it, and it carries two
/// tests of slack because doctests count towards `passed` — so hiding one or
/// two tests moved nothing at all.
///
/// What makes a module a test module is what is in it, so that is what is read.
///
/// **Spent once over the whole text, not a line at a time.** A literal may span
/// lines, so asking a whole-file stripper for one line at a time blanks the
/// first line of one and hands back every line after it as code — measured, a
/// three-line `\`-continued fixture whose tail is `}` written inside an
/// existing helper in `crates/nc-policy/src/problem.rs`, above the module's
/// first `#[test]`: with `#[cfg(unix)]` on that module and no fixture the gate
/// says `this test is hidden from the run by a condition`; with the fixture,
/// `check passed` and every number identical to a clean run. The census cannot
/// stand in for it, because a condition that is *true here* hides nothing from
/// this build and everything from another.
///
/// [`blanked`] spends comments and literals character for character and keeps
/// the lines, so it is asked once and indexed after.
fn holds_a_test(lines: &[&str], from: usize) -> bool {
    let spent = blanked(&lines.join("\n"));
    let spent: Vec<&str> = spent.lines().collect();
    let mut depth = 0_usize;
    let mut opened = false;
    for (at, line) in lines.iter().enumerate().skip(from) {
        if opened && is_a_test_attribute(line.trim_start()) {
            return true;
        }
        for character in spent.get(at).copied().unwrap_or_default().chars() {
            match character {
                '{' => {
                    depth += 1;
                    opened = true;
                }
                '}' => {
                    depth = depth.saturating_sub(1);
                    if opened && depth == 0 {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// Whether an attribute's brackets and parentheses close.
fn balanced(text: &str) -> bool {
    let mut depth = 0_i32;
    // **A bracket inside a string literal is text.** Counting one reaches the
    // same never-closing accumulator with no `//` involved at all, which is the
    // other half of the same defect.
    for character in without_string_contents(text).chars() {
        match character {
            '[' | '(' => depth += 1,
            ']' | ')' => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// The text between an opening `(` already consumed and its matching `)`.
///
/// Depth-counted, so `concat!(env!("X"), "/y.rs")` comes back whole rather than
/// truncated at the first `)`.
fn argument_of(after: &str) -> String {
    let mut depth = 1_i32;
    let mut out = String::new();
    for character in after.chars() {
        match character {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        out.push(character);
    }
    out
}

/// The name an argument holds when it is exactly one string literal.
///
/// `None` for anything else — a `concat!`, an `env!`, a macro, a path — which is
/// the point: a name that is not written here is a name no walk can follow.
fn single_literal(argument: &str) -> Option<String> {
    let trimmed = argument.trim().trim_end_matches(',').trim();
    let inner = trimmed.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains('"')).then(|| inner.to_owned())
}

/// Every run of whitespace as one space, so a wrapped attribute reads as one.
fn flatten(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether an attribute makes the item below it a test.
///
/// **`#[tokio::test(flavor = "multi_thread")]` is one, and matching `]` missed
/// it.** Measured: that spelling over `changing_any_entry_breaks_verification_at
/// _that_entry` — acceptance criterion 3's own test — with
/// `#[cfg(all(unix, not(unix)))]` above it, gave `ok 717 test(s)` and
/// `check passed`, while the identical `cfg` over a plain `#[tokio::test]` was
/// reported. The configuration parentheses were the whole difference between a
/// red run and a green one.
pub(crate) fn is_a_test_attribute(attribute: &str) -> bool {
    let attribute = attribute.trim_start();
    // **A path, not a prefix list.** `tokio::test` was accepted and
    // `::tokio::test` was not — a leading `::` is ordinary Rust, `rustc --test`
    // runs it, and it was invisible to this *and* to the census that shares this
    // predicate, so hiding a test lowered both sides of the comparison meant to
    // catch it. Measured on acceptance criterion 3's own test:
    // `ok 726 test(s)`, `check passed`, and `cargo test -- --list` no longer
    // naming it.
    //
    // So the rule is what an attribute *resolves to*: its last path segment.
    // `#[test]`, `#[tokio::test]`, `#[::tokio::test]`, `#[std::prelude::v1::test]`
    // all end in `test`; `#[test_case]` and `#[testing]` do not.
    let names_test = |rest: &str| {
        let rest = rest.trim_start();
        let path: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == ':')
            .collect();
        path.rsplit("::").next() == Some("test")
    };
    if let Some(rest) = attribute.strip_prefix("#[")
        && names_test(rest)
    {
        return true;
    }
    // **A `cfg_attr`'s arguments resolved, not three spellings of them.** The
    // bare form was made to resolve a path and this stayed a substring list, so
    // `#[cfg_attr(all(), ::tokio::test)]` was a test to the compiler and not to
    // this — measured on acceptance criterion 3's own test, with an `#[ignore]`
    // beside it that `no_ignored_test` then did not judge either, `ok 732
    // test(s)`, `check passed`, and `cargo test -- --list` naming it as ignored.
    // Both sides of the floor moved together, which is what the floor was for.
    if let Some(inside) = attribute
        .strip_prefix("#[cfg_attr(")
        .and_then(|rest| rest.strip_suffix(")]"))
    {
        // The condition may itself hold commas — `all(unix, feature = "x")` —
        // so the attributes are what follows the last comma at depth zero.
        let mut depth = 0_i32;
        let mut last = 0_usize;
        for (at, character) in inside.char_indices() {
            match character {
                '(' | '[' => depth += 1,
                ')' | ']' => depth -= 1,
                ',' if depth == 0 => last = at + 1,
                _ => {}
            }
        }
        return inside[last..]
            .split(',')
            .any(|named| names_test(&format!("{}]", named.trim())));
    }
    false
}

/// One finding, with the same fix whatever hid the test.
fn hidden(file: &str, index: usize, by: &str) -> Finding {
    Finding {
        file: file.to_owned(),
        line: index + 1,
        what: format!("this test is hidden from the run by {by}"),
        fix: "a suite that is present but inert is announced exactly like one that \
              ran, so the floor that names the suites cannot see it — nineteen \
              ignored tamper tests left `check passed` with acceptance criterion 3 \
              enforced by nothing. If a test cannot run here, make it say so and \
              return, the way the workspace-document tests do"
            .to_owned(),
    }
}

/// Whether `§15.1` judges a file, named once for the walk, its floor and its fixture.
///
/// **It was written out three times.** The rule lived inline in the walk;
/// `check.rs` kept `SCOPED_OUT`, a hand-made list of the two exclusions that lie
/// inside the scope, so the floor could subtract them; and the fixture had no
/// copy at all, which is why it planted in `crates/nc-catalog/` and expected a
/// finding there. Two hand-kept copies of one rule is the shape `§19.3`
/// condemns, and here the second copy was what a floor subtracted — so a crate
/// added to one and not the other is slack in the floor rather than a failure.
///
/// The scope: the crates the platform runs, the plugins it hosts and the schema
/// it runs against. `xtask` is out because it is the tool that reads these
/// codes, `catalog/` and `docs/` because they are generated from the registry,
/// and the registry's own crate and the compiler that reads and writes
/// reservation lines are out because a rule refusing the parser its own subject
/// is one nobody can satisfy.
///
/// **`plugins/` is inside, and was not.** A plugin is a member cargo builds like
/// any crate, and a code it spells out is one the registry cannot renumber just
/// the same — measured, `NC-JRNL-0003` written into a title in
/// `plugins/reference/src/behaviour.rs` left this step's line identical to a
/// clean run, because the scope was the two directories the tree had before
/// the plugin tree arrived.
pub(crate) fn in_the_error_code_scope(relative: &str) -> bool {
    if !(relative.starts_with("crates/")
        || relative.starts_with("migrations/")
        || relative.starts_with("local-migrations/")
        || relative.starts_with("plugins/"))
    {
        return false;
    }
    !OUTSIDE_THE_ERROR_CODE_SCOPE
        .iter()
        .any(|excluded| relative.starts_with(excluded))
}

/// The crates `§15.1` must judge, written out — one entry per member crate.
///
/// **The exclusion list was pinned only against itself.** Every consumer derived
/// its expectation from `OUTSIDE_THE_ERROR_CODE_SCOPE`: `check.rs`'s
/// `scoped_out()` subtracts from it, and the planted-fixture test filters its
/// expectation through `in_the_error_code_scope`. So adding one line to the list
/// was a change nothing could disagree with — measured, `"crates/nc-journal/"`
/// added and a hand-written `NC-JRNL-0003` left in the audit crate's own tamper
/// suite:
///
/// ```text
/// ok    21707 source line(s) name no error code as text (§15.1)   (baseline 31188)
/// ok    829 test(s)
/// check passed
/// ```
///
/// The canary lives in `nc-canon` and the fixture plants under a pid-named
/// directory, so neither is in the excluded crate; the line floor stayed exact
/// because it subtracted the same crate in the same edit. Deriving the floor
/// from the list is what `xtask/canaries/README.md` presented as keeping the
/// list honest, and it is what made the list unfalsifiable.
///
/// This is the other half, and it is the rule that file already states for
/// another list: *a case per thing it could grow to include — one per workspace
/// member.* A member added here and to the exclusion list in the same edit is a
/// contradiction the test names.
#[cfg(test)]
const JUDGED_BY_THE_ERROR_CODE_RULE: [&str; 12] = [
    "crates/nc-authz/",
    "crates/nc-canon/",
    "crates/nc-guest/",
    "crates/nc-ids/",
    "crates/nc-journal/",
    "crates/nc-kernel/",
    "crates/nc-local/",
    "crates/nc-log/",
    "crates/nc-policy/",
    "crates/nc-prune/",
    "crates/nc-schema/",
    "plugins/reference/",
];

/// The directories `§15.1` does not judge, named rather than patterned.
///
/// `catalog/`, `docs/` and `xtask/` are here for completeness of the sentence
/// above; `crates/ + migrations/ + plugins/` does not contain them, which is why
/// `check.rs`'s floor must subtract only the two that *are* inside the scope —
/// subtracting a directory the scope never held is how another floor saturated
/// to zero and stopped being a floor at all. `xtask/` is a workspace member, and
/// the census that holds every member to one of these two lists reads it here.
pub(crate) const OUTSIDE_THE_ERROR_CODE_SCOPE: [&str; 5] = [
    "catalog/",
    "crates/nc-catalog/",
    "crates/nc-catalog-compiler/",
    "docs/",
    "xtask/",
];

/// No error code is written out at a call site.
///
/// **`NOCTUA.md §15.1`: "Codes are declared in `catalog/errors.toml`, never
/// invented at the call site."** Four things are required there and one had a
/// gate — the append-only ledger. This is the second. Measured before it
/// existed: a probe file in `nc-canon` returning the literal
/// `"NC-KERN-4242"`, a code in no registry, and `ok 20 error code(s) reserved,
/// and every code this catalog declares is one of them` — because the
/// reservation gate reads `catalog/`, and a call site is not `catalog/`.
///
/// The generated crate is where the codes legitimately are, and the registry and
/// its ledger are where they are declared; everywhere else, an `NC-` code in a
/// string literal is one somebody wrote by hand.
pub(crate) fn no_code_is_written_at_a_call_site(
    root: &Path,
    declared: &[String],
) -> Result<(Vec<Finding>, usize, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    // **And the lines as the file holds them, which is what a floor can
    // compare.** What this gate judges is the text with comments blanked,
    // continuations joined and `concat!` glued — 326 fewer lines than the files
    // hold, on this tree — so an exact floor against a count of the tree was
    // short by that much on a correct run, and the ratio that covered the
    // difference covered a whole crate with it. Measured: one line removing
    // `crates/nc-journal/` — the audit log — took 31 % of the walk with a real
    // `NC-KERN-4242` in it, under a floor that allowed half. Two numbers: this
    // one answers "did the walk reach every file", exactly; the planted-fixture
    // test answers "did it judge the lines", which no count can.
    let mut raw = 0_usize;
    walk(root, &mut |path| {
        let relative = relative_to(root, path);
        // Where the codes belong: the registry, its ledger, the crate generated
        // from them, and the pages generated from that.
        // **The running platform, which is what `§15.1` is about.** Its own
        // header says build-time diagnostics from the catalog compiler and the
        // gates are not errors this registry declares — so `xtask/` does not use
        // `ErrorCode` at all, and its tests about the ledger have to be able to
        // write a reservation line. What is left is the crates the platform runs
        // and the schema it runs against.
        if !in_the_error_code_scope(&relative) {
            return;
        }
        if path
            .extension()
            .is_none_or(|kind| kind != "rs" && kind != "sql")
        {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        // **The comment marker its own file type uses.** `//` was applied to a
        // migration too, so `-- raised as NC-KERN-9001 when this fails` — prose
        // in SQL — was a finding, while the byte-identical sentence in a Rust
        // `//` comment was not. `references::Comments` already knows which is
        // which.
        let comment = if path.extension().is_some_and(|kind| kind == "sql") {
            "--"
        } else {
            "//"
        };
        // **A string continued with `\` is one line to the compiler.** Written
        // across two, `"NC-KERN-\` / `4243"` put the digits on a line whose
        // area is empty and the area on a line whose digits are, and neither
        // half is a code. Joining them first is what the compiler does.
        // **And a `concat!` `cargo fmt` wrapped.** The scan is per line, so
        // `concat!(\n "NC-",\n "KERN-4243",\n …)` held no whole code on any
        // line — and that wrapped form is what the formatter produces as soon as
        // the argument list is long, not something anybody had to write.
        raw += text.lines().count();
        let joined = joined_concats(&without_comments(&text, comment).replace("\\\n", ""));
        let mut judged_here = 0_usize;
        // **And one at the *end* of the body, because a filter goes between.**
        // The counter above says how many lines became this gate's to judge; a
        // `continue` one line below it leaves that number whole while the rule
        // is never asked — measured, `if relative.starts_with(…)` after the
        // increment, with a real `NC-KERN-4242` in the crate it skipped and
        // every printed number *up*. Two counters that must agree is the
        // cheapest thing that cannot be true when a line was skipped, because
        // the second one is the last statement a judged line reaches.
        let mut reached_a_verdict = 0_usize;
        for (index, line) in joined.lines().enumerate() {
            // **Counted at the top of the loop, which is where a line becomes
            // this gate's to judge.** Counting files left the number at full
            // strength when a filter sat inside the loop; counting at the
            // *bottom* counts only the lines that matched, which is a different
            // number again and one that falls to two on a correct run.
            //
            // What this catches, with the floor below it: a filter above the
            // loop, around the file, or anywhere in the walk. What it does not:
            // a filter written between here and the verdict. That one is caught
            // by the unit tests, which drive this function over text directly —
            // and saying so is better than a comment claiming a counter can do
            // what no position for it can.
            judged_here += 1;
            // **And the line as its pieces make it.** Four spellings assembled
            // the same code past a scan of the literal text: `concat!("NC-",
            // "KERN-4242")`, a `\`-continuation across two lines,
            // `"NC-KERN-424\x34"`, and `format!("NC-{}-{:04}", …)`. The first
            // three are string glue, and removing the glue is what reads them;
            // the fourth cannot be read here at all and is written down rather
            // than pretended about.
            // **And upper-cased at run time.** `"nc-kern-4245".to_ascii_uppercase()`
            // is a code a caller wrote by hand with one method call between it
            // and the scan. The upper-casing is applied only when the line asks
            // for it, so ordinary lowercase prose is untouched.
            let cased = if line.contains("to_ascii_uppercase") || line.contains("to_uppercase") {
                line.to_ascii_uppercase()
            } else {
                String::new()
            };
            for code in codes_in(line)
                .into_iter()
                .chain(codes_in(&glued(line)))
                .chain(codes_in(&cased))
                .chain(codes_in(&glued(&cased)))
                .collect::<std::collections::BTreeSet<_>>()
            {
                // **A declared code counts too.** The rule is that the identity
                // is the `ErrorCode` variant, not its spelling — a hand-written
                // `NC-DB-0001` is one the registry cannot renumber even though
                // it exists today. `declared` is here so the diagnostic can say
                // which of the two a reader is looking at.
                let known = if declared.contains(&code) {
                    "an error code written out as text"
                } else {
                    "an error code written out as text, and it is in no registry"
                };
                findings.push(Finding {
                    file: relative.clone(),
                    line: index + 1,
                    what: format!("`{code}` is {known}"),
                    fix: "an error's identity is data, not a string at a call site \
                          (NOCTUA.md §15.1). The code travels through every layer and is \
                          what the central log is queried by, so one spelled out here is \
                          one the registry cannot renumber, cannot translate and cannot \
                          publish. Use the `ErrorCode` variant `catalog/errors.toml` \
                          declares; if the code does not exist yet, declare it there"
                        .to_owned(),
                });
            }
            // The last statement a judged line reaches, so a `continue`
            // anywhere above it — including one written after the counter at
            // the top — makes the two disagree.
            reached_a_verdict += 1;
        }
        everything_reached_a_verdict(
            "line",
            &relative,
            judged_here,
            reached_a_verdict,
            &mut findings,
        );
        // **Counted per line, not per file.** Counting files left the number at
        // full strength when the filter was inside the line loop — measured, a
        // `if !line.contains("ErrorCode") { continue; }` with a hand-written
        // `NC-KERN-4242` in the tree and `ok 48 source file(s)`. What the gate
        // judges is lines, so lines are what it reports.
        read += judged_here;
    })?;
    Ok((findings, read, raw))
}

/// One file with each `concat!(...)` on a line of its own.
///
/// The arguments are joined and the newlines they occupied are kept, so every
/// later line still has the number it had. A `concat!` that does not close in
/// this file is left alone.
fn joined_concats(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("concat!(") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let mut depth = 0_i32;
        let mut ends = None;
        for (index, character) in after.char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        ends = Some(index + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(ends) = ends else {
            out.push_str(after);
            return out;
        };
        let call = &after[..ends];
        out.push_str(&call.replace('\n', " "));
        // The newlines the call spanned, so the lines after it keep their
        // numbers.
        for _ in 0..call.matches('\n').count() {
            out.push('\n');
        }
        rest = &after[ends..];
    }
    out.push_str(rest);
    out
}

/// One line with its string glue removed, so an assembled code reads as one.
///
/// `concat!("NC-", "KERN-4242")` and `"NC-KERN-424\x34"` are both a code a
/// caller wrote by hand; what stands between them and the scan is quotes,
/// commas and an escape. This removes exactly those, which is why it is applied
/// *beside* the literal scan rather than instead of it: joining everything makes
/// neighbouring unrelated strings read as one word, and the pair is what keeps
/// each finding a real one.
fn glued(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            // **The glue, and the whitespace touching it — not every space.**
            // Removing all whitespace made `NC - CANON - 0001` — an English
            // sentence with spaced hyphens, naming no code — read as one, and
            // the gate reported a doc attribute as a hand-written code. What
            // stands between a `concat!`'s pieces is a quote, a comma and the
            // space beside them; a space with prose on both sides is prose.
            '"' | ',' => {
                while out.ends_with(' ') || out.ends_with('\t') {
                    out.pop();
                }
                while characters.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
                    characters.next();
                }
            }
            // `\x34` is `4`, and so is `\u{34}`. Only ASCII, which is all a
            // code holds — and both spellings, because one of them was the
            // whole of a reviewer's third evasion.
            '\\' if matches!(characters.peek(), Some('x' | 'u')) => {
                let braced = characters.peek() == Some(&'u');
                characters.next();
                if braced && characters.peek() == Some(&'{') {
                    characters.next();
                }
                let hex: String = if braced {
                    let mut digits = String::new();
                    while characters.peek().is_some_and(char::is_ascii_hexdigit) {
                        digits.push(characters.next().unwrap_or('0'));
                    }
                    if characters.peek() == Some(&'}') {
                        characters.next();
                    }
                    digits
                } else {
                    characters.by_ref().take(2).collect()
                };
                if let Ok(byte) = u8::from_str_radix(&hex, 16)
                    && byte.is_ascii()
                {
                    out.push(char::from(byte));
                }
            }
            _ => out.push(character),
        }
    }
    out
}

/// Every `NC-<AREA>-<NNNN>` in one line of source, inside a string or not.
fn codes_in(line: &str) -> Vec<String> {
    let characters: Vec<char> = line.chars().collect();
    let mut found = Vec::new();
    for (at, _) in line.match_indices("NC-") {
        let from = line[..at].chars().count();
        let mut area = String::new();
        let mut cursor = from + 3;
        while characters.get(cursor).is_some_and(char::is_ascii_uppercase) {
            area.push(characters[cursor]);
            cursor += 1;
        }
        if area.is_empty() || characters.get(cursor) != Some(&'-') {
            continue;
        }
        cursor += 1;
        let mut digits = String::new();
        while characters.get(cursor).is_some_and(char::is_ascii_digit) {
            digits.push(characters[cursor]);
            cursor += 1;
        }
        if digits.len() == 4 {
            found.push(format!("NC-{area}-{digits}"));
        }
    }
    found
}

/// What the image gate inspected, and how much of it it could answer.
///
/// **A five-element tuple, and every corpus case destructured it `(findings, _,
/// _, _, _)`.** Four numbers nothing named, so nothing could be floored without
/// counting positions — and `compared`, the only one that says the digest
/// comparison ran at all, had no floor while its own comment said it was "the
/// count that says so is the one a floor can hold".
#[derive(Debug)]
pub(crate) struct Pins {
    /// Defects in the tree.
    pub findings: Vec<Finding>,
    /// Pins this machine holds no information about, announced as skipped steps.
    pub unanswerable: Vec<Finding>,
    /// `image:` lines in the projection `docker compose config` prints.
    pub read: usize,
    /// `image:` lines written across the compose files themselves.
    pub written: usize,
    /// Pins whose referent was compared against what the tag resolves to.
    pub compared: usize,
    /// Pins that carried a digest, and so became this comparison's to make.
    pub pinned: usize,
}

/// What `docker compose config` resolves the stack to, with every profile on.
///
/// Split out to carry the one `expect` this file needs: a child process that
/// inherits stdio is a print, so `Command::new` is disallowed by default —
/// docker here is *driven* and its output is read, never shown.
///
/// **This block carried [`every_image_is_pinned`]'s summary and its rationale**,
/// with its own summary stranded three paragraphs down as a tail clause — the
/// shape a deleted item between two doc blocks leaves, and the shape rustdoc
/// renders as this private wrapper being the rule about images. Neither the
/// `DOCUMENTED` census nor the fused-summary gate could see it: both counts are
/// conserved when one block simply holds another item's prose.
fn the_effective_stack(root: &Path) -> std::io::Result<std::process::Output> {
    #[expect(clippy::disallowed_methods, reason = "docker is driven, not printed")]
    std::process::Command::new("docker")
        .args(["compose", "--profile", "*", "config"])
        .current_dir(root)
        .output()
}

/// Why `docker compose config` produced nothing, in the reader's terms.
///
/// **Which of the two failures, because they send a reader to different
/// places.** One sentence covered both, and *the effective stack could not be
/// resolved* sends a reader to `docker compose config` when the answer is that
/// there is no `docker` on this machine — which is not a defect in this
/// repository at all. `--offline` is the flag for a machine without one, and it
/// announces the stack's own tests among the steps it skips.
fn docker_did_not_answer(problem: &std::io::Error) -> Finding {
    if problem.kind() == std::io::ErrorKind::NotFound {
        Finding {
            file: "docker-compose.yml".to_owned(),
            line: 1,
            what: "docker is not installed, so no image's pin was compared".to_owned(),
            fix: "the comparison needs the tool and this repository does not build with \
                  it. Install docker, or run `cargo xtask check --offline`, which reads \
                  what the compose files say and announces that it compared nothing"
                .to_owned(),
        }
    } else {
        Finding {
            file: "docker-compose.yml".to_owned(),
            line: 1,
            what: "the effective stack could not be resolved, so no image was checked".to_owned(),
            fix: "`docker compose config` is what applies overrides, `extends`, anchors \
                  and variables — the four ways an unpinned image reached the running \
                  stack past a scan of one file. Docker is here and would not answer: \
                  start the daemon, or run this step where the stack is brought up"
                .to_owned(),
        }
    }
}

/// The verdict when docker answered nothing: the findings, and four zeros.
///
/// Written once because the zeros are the assertion — a `Pins` whose counts were
/// left at whatever a partial walk reached would let a floor pass on a run that
/// compared nothing.
fn nothing_was_compared(findings: Vec<Finding>, unanswerable: Vec<Finding>) -> Pins {
    Pins {
        findings,
        unanswerable,
        read: 0,
        written: 0,
        compared: 0,
        pinned: 0,
    }
}

/// Every image the stack runs is pinned by digest.
///
/// **A tag is a name somebody else can repoint.** `postgres:18-alpine` was
/// defended in a comment against `latest`, which is a different question: the
/// tag still moves on every 18.x patch and every Alpine base rebuild, and a
/// `docker compose pull` takes whichever image is current. Two reviewers a day
/// apart both measured the same two unpinned tags and both reported them, which
/// is what a rule with no gate behind it looks like.
///
/// The tag is kept in front of the digest — `image:tag@sha256:…` — because a
/// reader needs to see which version this is, and a bare digest is unreadable.
pub(crate) fn every_image_is_pinned(root: &Path, offline: bool) -> Pins {
    // **`--offline` gives up the half that needs nothing.** The step returned
    // before this function was called at all, so the pure-text rules —
    // `pinned_by_digest`, `is_a_build_key`, the pass over the written files —
    // went with the docker comparison. Measured, `image: postgres:latest` in
    // `docker-compose.yml`: online, two errors saying a tag is not a pin;
    // `--offline`, no finding anywhere, the step printing only its skip line.
    // That is the defect `check.rs` records fixing one step over —
    // *"`cargo deny check` runs four checks and one of them wants the network;
    // this skipped all four"*.
    //
    // So only the resolution is skipped. What the files say is read either way.
    if offline {
        return only_what_the_files_say(root);
    }
    let mut findings = Vec::new();
    // Tags this machine has not pulled, so the comparison could not be made. Reported as skipped steps, not defects — see `judge_a_pin`.
    let mut unanswerable = Vec::new();
    // **What docker resolves, not what one file says.** The text scan read
    // `docker-compose.yml` and `dev.rs` runs `docker compose` with no `-f`, so
    // every one of these put an unpinned image into the running stack with the
    // gate printing `ok 2 image(s) … each pinned by digest`:
    // ```text
    // compose.yaml                       — outranks docker-compose.yml entirely
    // docker-compose.override.yaml       — merged; only the .yml spelling was checked
    // compose.override.yml / .yaml       — merged
    // extends: to a second file          — never opened
    // a YAML anchor and a merge key      — resolves to a different image
    // ${LOGS_IMAGE:-…@sha256:…}          — the default is pinned, the value is not
    // ```
    //
    // The last one leaves the message word for word intact. So the gate asks the
    // tool: `docker compose config` applies every override, every `extends`,
    // every anchor and every variable, and prints what would actually run.
    // **With every profile, because the default projection hides them.** A
    // service behind `profiles: ["debug"]` is absent from `docker compose
    // config` entirely — measured, an unpinned image and a `network_mode: host`
    // service accepted by this gate and by the loopback test both, while
    // `docker compose --profile debug up` brings them up. `--profile "*"`
    // enables all of them.
    let config = match the_effective_stack(root) {
        Ok(config) => config,
        Err(problem) => {
            findings.push(docker_did_not_answer(&problem));
            return nothing_was_compared(findings, unanswerable);
        }
    };
    if !config.status.success() {
        findings.push(Finding {
            file: "docker-compose.yml".to_owned(),
            line: 1,
            what: "the effective stack does not resolve".to_owned(),
            fix: format!(
                "`docker compose config` refused it: {}",
                String::from_utf8_lossy(&config.stderr)
                    .lines()
                    .take(3)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        });
        return nothing_was_compared(findings, unanswerable);
    }
    let resolved = String::from_utf8_lossy(&config.stdout);
    let mut read = 0_usize;
    // How many pins had their *referent* compared, which is not how many lines
    // were read: an image nobody has pulled cannot be compared, and the count
    // that says so is the one a floor can hold.
    let mut compared = 0_usize;
    // Pins that became the comparison's to make, so `compared` can be held
    // against something rather than against zero.
    let mut pinned = 0_usize;
    for (index, line) in resolved.lines().enumerate() {
        // **A built service, in the projection docker actually resolves.** The
        // file pass reads eight names in the repository root, so a service
        // declared in a file `include:` pulls in from anywhere else was in
        // neither pass — measured with such a file carrying `build:` and a
        // Dockerfile reading `FROM postgres:latest`:
        // `ok 4 image(s) in the stack, each pinned by digest`, with the stack
        // running an unpinned image. `docker compose config` prints the `build:`
        // key wherever the service came from, which is the one place that knows.
        if is_a_build_key(line.trim()) {
            findings.push(Finding {
                file: "docker-compose.yml".to_owned(),
                line: index + 1,
                what: "the resolved stack builds a service rather than pulling it, so no \
                       digest pins what it runs"
                    .to_owned(),
                fix: "nothing here reads a `FROM` line, so a Dockerfile naming an \
                      unpinned tag is invisible to this gate. This stack pulls two \
                      pinned images (`§18.4`, `§18.5`) and builds nothing; if that \
                      changes, the pin moves to the Dockerfile and this gate has to \
                      follow it there. The line number is in `docker compose config`'s \
                      output, which is what the stack actually runs"
                    .to_owned(),
            });
            continue;
        }
        let Some(image) = the_image_on(line) else {
            continue;
        };
        read += 1;
        let image = image.trim().trim_matches(['"', '\'']);
        if pinned_by_digest(image) {
            pinned += 1;
            judge_a_pin(
                root,
                image,
                index,
                &mut compared,
                &mut findings,
                &mut unanswerable,
            );
            continue;
        }
        findings.push(unpinned(root, image, index));
    }
    let mut written = 0_usize;
    written_pins(root, &mut findings, &mut written);
    Pins {
        findings,
        unanswerable,
        read,
        written,
        compared,
        pinned,
    }
}

/// The pins the compose files write down, without asking docker anything.
///
/// Its own function because [`every_image_is_pinned`] is at clippy's line limit
/// with it inline — the same reason its neighbours are functions.
fn only_what_the_files_say(root: &Path) -> Pins {
    let mut findings = Vec::new();
    let mut written = 0_usize;
    written_pins(root, &mut findings, &mut written);
    Pins {
        findings,
        unanswerable: Vec::new(),
        read: 0,
        written,
        compared: 0,
        pinned: 0,
    }
}

/// Where an image is actually written, for a finding a reader can jump to.
///
/// **The line number was an index into `docker compose config`'s output** and
/// the finding was filed under `docker-compose.yml`, so
/// `docker-compose.yml:16` pointed at a line of prose while the image sat on
/// 120. Two of the fix texts said the number was compose's; the two on
/// `judge_a_pin` did not, and every other gate in this tree emits a real
/// `file:line` — so an editor jumping to this one lands in the wrong place.
///
/// The resolved stack is still what is judged: this only asks the files where
/// the answer is written down. An image that exists only after resolution — one
/// an override or a variable produced — is not in any file, and then the
/// compose index is the honest answer and the fix says so.
fn where_it_is_written(root: &Path, image: &str, resolved: usize) -> (String, usize) {
    for name in COMPOSE_FILES {
        let Ok(text) = std::fs::read_to_string(root.join(name)) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('#') {
                continue;
            }
            // **Quotes stripped, as both passes strip them.** The resolved pass
            // trims `"` and `'` before matching and the file pass replaces them
            // — this compared the raw text, so `image: "postgres:18-alpine"`,
            // which is ordinary compose, never matched and fell back to the
            // `docker compose config` index. Measured, one image quoted and one
            // not, both unpinned: the unquoted one reported at its real line 120
            // and the quoted one at :44, which is a line of environment prose,
            // while the second pass reported the same image correctly at :37.
            // Two spellings of one file in one list of findings, one pointing at
            // something unrelated — verbatim the defect this function was added
            // to close.
            if the_image_on(trimmed)
                .is_some_and(|written| written.trim().trim_matches(['"', '\'']) == image)
            {
                return ((*name).to_owned(), index + 1);
            }
        }
    }
    ("docker-compose.yml".to_owned(), resolved)
}

/// The image a line names, whatever whitespace or quoting the key wears.
///
/// **`strip_prefix("image:")` was written three times and each copy read one
/// spelling.** YAML allows whitespace between a key and its colon and docker
/// resolves `image :` to the same service — measured, one space added before
/// the colon in `docker-compose.yml`:
/// `ok 2 image(s) in the resolved stack and 1 written in its files`, the written
/// count halved and the step still saying `ok`. Quoting is the other spelling,
/// and one of the three copies already knew about it.
///
/// **Flow style is a spelling this does not read, and that is measured rather
/// than assumed.** `a: {image: postgres:latest, restart: unless-stopped}` splits
/// at the first colon into the key `a`, so nothing here calls it an image and
/// none of the file pass's rules is applied to one — while `is_a_build_key`
/// below splits on `{` and `,`, because flow style is YAML too, which makes this
/// the narrower of two copies of one rule. What stops that being a hole is the
/// floor `xtask/src/check.rs` holds the file count under: the resolved
/// projection prints one image line per service whatever the file spelled, so a
/// spelling this cannot read lowers one count and not the other, and the step
/// refuses the run rather than reporting on it. Measured on a scratch tree, in
/// the shape where neither pass has a finding to make — a variable whose default
/// is pinned, written in flow style beside an image the file pass can read —
/// docker resolved 2 and the file pass read 1.
///
/// `xtask/src/corpus.rs`'s `a_flow_style_image_never_reaches_a_green_verdict` is
/// what keeps that true. It asserts the verdict and not the parser, so widening
/// this reader is a change that test is written to survive, and a widening that
/// pays the file count back without judging what it counted is the one it is
/// written to catch.
///
/// One function, so the next spelling is found once rather than in whichever
/// copy somebody remembers.
pub(crate) fn the_image_on(line: &str) -> Option<&str> {
    let (key, rest) = line.trim().split_once(':')?;
    (key.trim_end().trim_matches(['"', '\'']) == "image").then_some(rest)
}

/// The names docker reads a stack from, in the order it reads them.
const COMPOSE_FILES: [&str; 8] = [
    "docker-compose.yml",
    "docker-compose.yaml",
    "docker-compose.override.yml",
    "docker-compose.override.yaml",
    "compose.yml",
    "compose.yaml",
    "compose.override.yml",
    "compose.override.yaml",
];

/// A pinned image, held against what its own tag resolves to.
///
/// Split out of [`every_image_is_pinned`] only because that function grew past
/// the line limit.
fn judge_a_pin(
    root: &Path,
    image: &str,
    index: usize,
    compared: &mut usize,
    findings: &mut Vec<Finding>,
    announced: &mut Vec<Finding>,
) {
    let (file, line) = where_it_is_written(root, image, index + 1);
    match the_digest_is_the_tag(image) {
        Ok(true) => {
            // **Digests actually compared, counted apart from images
            // read.** `read` rises on every `image:` line whatever became
            // of it, so the number printed said nothing about whether any
            // referent had been checked at all.
            *compared += 1;
        }
        Ok(false) => findings.push(Finding {
            file: file.clone(),
            line,
            what: format!("`{image}` carries a digest, and it is not this tag's digest"),
            fix: "docker resolves `tag@sha256:…` by the digest and ignores the \
                          tag, so the two halves can name different images and only the \
                          half a person reads is wrong. Measured: 16-alpine's digest \
                          under 18-alpine's tag brought up PostgreSQL 16.14 with every \
                          printed line identical. Take the digest from \
                          `docker image inspect <tag> --format '{{index .RepoDigests 0}}'`"
                .to_owned(),
        }),
        // **A finding, because a skip here was free.** `finish` makes
        // any `skipped:`-prefixed warning cost nothing when nothing
        // announced it, so this printed one line and exited 0 — and a
        // reviewer shipped `postgres:17-alpine@sha256:…` carrying
        // **18**'s digest through a green run:
        //
        // ```text
        // warn  skipped: `postgres:17-alpine` is not pulled here, so its
        //                digest was not compared
        // ok    2 image(s) … each pinned by digest
        // check passed
        // ```
        //
        // `cargo xtask dev` would then pull by digest and bring up
        // PostgreSQL 18 under a tag reading 17 — the exact defect this
        // function exists to close, shipped green. A gate that cannot ask
        // its question has not answered it, and `check` already takes
        // that line about the database: fail, and name the command that
        // makes the answer available.
        // **Not pulled here is an absence of local information, not a defect in
        // the tree.** This was an error, on the reasoning that a free skip once
        // let `postgres:17-alpine` carrying 18's digest through a green run —
        // and the reasoning is right about the skip and wrong about the verdict.
        // Nothing pulls these images on a CI runner, so the gate was guaranteed
        // red there.
        //
        // **The second half of that reasoning said both tags had since been
        // repointed upstream, and it is not true.** Measured on 2026-09-15
        // without pulling anything — `docker manifest inspect <tag>` and
        // `docker manifest inspect <the pinned digest>` returned byte-identical
        // indexes for both images, so each tag still resolves to the digest
        // beside it and a host that pulled the tags would get a *green*
        // comparison. Whether that stays true is exactly what this comparison
        // is for; what it may not do is assert the answer in advance.
        //
        // What the pin guarantees is the bytes, and the digest does that alone.
        // What the comparison adds is that the *label* beside them is honest.
        //
        // **And `cargo xtask dev` does not make that comparison available**,
        // which this said for two rounds and is false. Compose brings the stack
        // up from `image:tag@sha256:…`, and docker resolves a reference
        // carrying a digest **by the digest**: the tag is discarded before the
        // store sees it. Measured on 2026-09-15 with an image the store did not
        // hold, so that nothing already pulled could answer for it:
        //
        // ```text
        // docker pull hello-world:latest@sha256:d1a8d0a4…
        //   ->  Downloaded newer image for hello-world@sha256:d1a8d0a4…
        // docker image inspect hello-world:latest       ->  exit 1, no output
        // docker image inspect hello-world@sha256:d1a8d0a4…
        //   ->  RepoTags=[hello-world@sha256:d1a8d0a4…]
        // ```
        //
        // So on a machine that has only ever run `cargo xtask dev`, this skip
        // is permanent: `docker image inspect <tag>` keeps failing however many
        // times the stack comes up. What makes the answer available is pulling
        // the **tag** — which is also what the compose file tells a reader to do
        // when moving a pin. The image was removed again; the store held none
        // before the measurement and holds none after.
        //
        // Here the honest report is that the question could not be asked —
        // announced, so the verdict counts it among the steps that did not run,
        // rather than silent.
        Err(why) => announced.push(Finding {
            file,
            line,
            what: why,
            fix: "pull the tag — `docker pull <the image before the `@`>`. \
                          `cargo xtask dev` does not do it: compose resolves \
                          `tag@sha256:…` by the digest and the tag never reaches the \
                          store, so bringing the stack up leaves this skip exactly \
                          where it was. `cargo xtask check --offline` says up front \
                          that it does not ask. The digest is what pins the bytes and \
                          it is checked whatever this says; what is missing is only \
                          whether the tag beside it still names them"
                .to_owned(),
        }),
    }
}

/// The finding an image that carries no digest gets.
fn unpinned(root: &Path, image: &str, index: usize) -> Finding {
    let (file, line) = where_it_is_written(root, image, index + 1);
    Finding {
        file,
        line,
        what: format!("the stack runs `{image}`, and a tag is not a pin"),
        fix: "a tag is a name whoever publishes the image can repoint, so the stack a \
              developer brings up today and the one CI brings up tomorrow are not the \
              same bytes. Write `image:tag@sha256:…` — the tag stays so a reader can \
              see the version, and the digest is what pins it. When the image is \
              written in a compose file the line above is that file's; an image \
              that only exists after resolution is not in any file, and the \
              number is then `docker compose config`'s"
            .to_owned(),
    }
}

/// The second pass: what the compose files say, rather than what docker resolves.
///
/// Split out of [`every_image_is_pinned`] only because that function grew past
/// the line limit.
fn written_pins(root: &Path, findings: &mut Vec<Finding>, written: &mut usize) {
    // **And the files themselves, because the resolved config cannot show
    // everything.** Three ways past a scan of `docker compose config` alone,
    // each measured accepted with `ok 1 image(s) in the stack, each pinned by
    // digest`:
    //
    // ```text
    // a service with `build:` and no `image:`   — its Dockerfile may say FROM postgres:latest
    // a service behind a `profiles:` key        — not in the default projection at all
    // image: ${IMG:-…@sha256:…}                 — resolved with *this process's* environment,
    //                                             so `check` validates one stack and a shell
    //                                             with the variable exported brings up another
    // ```
    //
    // The two passes catch what the other cannot: the resolved config applies
    // overrides, `extends` and anchors, and the files show what is written
    // regardless of profile, projection or environment.
    // **Two numbers, because they are two questions.** They shared one counter,
    // so `2 image(s)` came out as `4` — every image counted once by the
    // resolved projection and once by the file it is written in — and neither
    // pass could be caught having inspected nothing, because the other's count
    // carried it. `read` is what docker resolves; `written` is what the files
    // say; `compared` is how many pins had their referent checked.
    *written = in_the_compose_files(root, findings);
}

/// Whether an image reference carries a real digest.
///
/// **`contains("@sha256:")` is not a digest test.** Measured accepted:
/// `alpine:latest@sha256:deadbeef` (eight characters),
/// `alpine:latest@sha256:D3E1620B…` (upper case, which docker rejects) and
/// `alpine:latest@sha256:this-is-not-hex-and-not-64-chars`. A digest is exactly
/// sixty-four lowercase hex characters, and anything else is a string that
/// happens to contain the prefix.
fn pinned_by_digest(image: &str) -> bool {
    image.split_once("@sha256:").is_some_and(|(name, digest)| {
        !name.is_empty()
            && digest.len() == 64
            && digest
                .chars()
                .all(|character| character.is_ascii_digit() || ('a'..='f').contains(&character))
    })
}

/// A line with its trailing `//` comment removed, and paths left whole.
///
/// **`str::split("//")` is not a comment stripper.** A doubled separator inside
/// a path — `"..//canaries/x.rs"`, which cargo accepts — is two slashes, and
/// cutting there split the `canaries/` from the `#[path]` that reaches it. What
/// opens a comment in Rust is `//` at the start of a line or after whitespace;
/// what does not is `//` after any other character.
pub(crate) fn line_without_a_comment(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut at = 0_usize;
    while let Some(found) = line[at..].find("//") {
        let found = at + found;
        let opens = found == 0 || bytes[found - 1].is_ascii_whitespace();
        if opens {
            return &line[..found];
        }
        at = found + 2;
    }
    line
}

/// Whether this line opens more brackets than it closes.
///
/// An attribute that wraps leaves its first line unbalanced; one written whole
/// on a line does not. That is what tells a wrapped attribute's opening from a
/// line that merely begins `#[`, and it is why the walk below needs no rule
/// about blank lines — a list may legitimately hold one.
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
/// **A wrapped attribute is still one attribute.** A reader that steps over
/// lines beginning `#[` and stops at anything else stops at the *continuation*
/// of an attribute rustfmt has broken across lines — and the doc block above it
/// is never seen. Four reviewers named this over five rounds before one planted
/// it: a `#[doc(alias = …)]` in the house layout above `pub async fn begin` made
/// two of `nc-journal`'s tests fail with messages that are false, one of them
/// saying a function has no `# Errors` section while its section sits untouched
/// three lines up.
///
/// The last line of a wrapped attribute closes with `]` and its first begins
/// `#[`, so a bounded walk back to that first line is what joins the two. The
/// bound is what stops a stray `]` — an array closing above an item — walking
/// the reader out of the block it is reading.
///
/// **Not test-only any more.** `a_test_door_says_it_is_one` read a door's
/// prologue with its own walk — up while each line opens `///` or `#[` or is
/// blank — which a wrapped attribute ends on its *closing* line, so the
/// prologue came back empty and a door that **is** marked `#[doc(hidden)]` and
/// **does** carry lines saying what it reaches was told "a `_for_tests` door is
/// not marked as one. Mark it, and say above it what it is for". The correct
/// reader was in this file the whole time and behind `#[cfg(test)]` — one rule
/// written twice with the production copy the narrower, which is the shape this
/// campaign has now found in nine files. Found by a reviewer in wave 21.
pub(crate) fn past_the_attributes(lines: &[&str], mut at: usize) -> usize {
    const WRAPPED_AT_MOST: usize = 12;
    while at > 0 {
        let line = lines[at - 1].trim_end();
        if line.trim_start().starts_with("#[") {
            at -= 1;
            continue;
        }
        // **A comment ending in `]` is not a wrapped attribute's last line, and
        // a wrapped attribute's first line is unbalanced.** The bound was
        // written to stop "a stray `]` above an item walking the reader out of
        // the block it is reading", and it did not: an intra-doc link ends a doc
        // line in `]` — `/// See [`other`]`, the most ordinary line here — and
        // the walk left the item, crossed blank lines and whole other items, and
        // landed on a neighbour's `#[doc(hidden)]`, so an unmarked `_for_tests`
        // door borrowed a marker. Found in wave 24.
        //
        // **And the first repair was too strong.** It stopped the inner walk at
        // a blank line, which a wrapped attribute list may legitimately contain
        // — measured, `#[doc(\n alias = "a",\n\n alias = "b"\n)]` then lost its
        // doc block entirely. It was also still too weak for a `//` comment
        // ending in `]`, and for a macro call `thing![a, b]`. Found in wave 25,
        // by a skeptic measuring both directions of the repair.
        //
        // What separates them is not blankness: an attribute that *wraps* opens
        // brackets its first line does not close. So the line the walk lands on
        // must open `#[` **and** be unbalanced, and a comment is never either.
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

/// Whether the digest a pin carries is the digest of the tag beside it.
///
/// **A pin's *form* was checked and its *referent* never was.** `sha256:` plus
/// sixty-four lowercase hex is a shape, and docker resolves a
/// `tag@sha256:…` reference by the digest alone — the tag is a comment to the
/// daemon and load-bearing to every human reader and to `§18.4`. Measured, one
/// digest substituted for another with the tag left alone:
///
/// ```text
/// image: postgres:18-alpine@sha256:57c72fd2…   <- 16-alpine's digest
///   docker … --version  ->  postgres (PostgreSQL) 16.14
///   cargo xtask check   ->  ok 2 image(s) … each pinned by digest
///   every printed line byte-identical to baseline
///   cargo xtask dev     ->  PostgreSQL 16.14 on x86_64-pc-linux-musl
/// ```
///
/// None of the five mechanisms can see it: no predicate is weakened, no walk
/// narrowed, no text rewritten, no path cleared, no file kind excluded. The
/// rule reaches every line and asks the wrong question — it validates the form
/// of an assertion whose human-readable half is the part nobody checks. What
/// closes it is a different kind of evidence: ask the registry what the tag
/// resolves to, and compare.
///
/// **Skipped, loudly, when the image is not local.** This resolves through
/// `docker image inspect`, which reads what is already pulled — asking a
/// registry would put a gate on the network, and `check --offline` exists
/// precisely so it is not. A digest nobody has pulled is one this cannot
/// judge, and saying so is the honest answer.
///
/// **And `cargo xtask dev` is not the remedy**, which this said and which is
/// false: compose brings the stack up from `image:tag@sha256:…` and docker
/// resolves that by the digest, so the tag never reaches the store and this
/// keeps skipping however many times the stack comes up. What makes the
/// question answerable is pulling the tag — `docker pull <the image before the
/// `@`>`. The correction is written here as well as in the body because this is
/// the half rustdoc publishes, and the body comment that first carried it does
/// not reach a reader of the docs.
fn the_digest_is_the_tag(image: &str) -> Result<bool, String> {
    let Some((tag, digest)) = image.split_once('@') else {
        return Ok(true);
    };
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool this gate drives; its output is read, not printed"
    )]
    let inspected = std::process::Command::new("docker")
        .args(["image", "inspect", tag, "--format", "{{json .RepoDigests}}"])
        .output()
        .map_err(|error| format!("docker could not be asked what `{tag}` is: {error}"))?;
    if !inspected.status.success() {
        // Not pulled here, and `dev` does not pull it either — it pulls the
        // digest, which carries no tag into the store. `check` does not pull at
        // all, and a gate that failed on a cold cache would be a gate somebody
        // turns off.
        return Err(format!(
            "`{tag}` is not pulled here, so its digest was not compared"
        ));
    }
    let listed = String::from_utf8_lossy(&inspected.stdout);
    Ok(one_of_them_is(&listed, tag, digest))
}

/// Whether `digest` is one of the digests `listed` gives for `tag`'s repository.
///
/// **`listed.contains(digest)` was the whole test, and it is a substring test on
/// JSON.** `RepoDigests` is a list of `repository@sha256:…` entries and an image
/// can carry several — one per repository it has been tagged into — so a
/// substring match says "this digest appears somewhere in this list", which is
/// not "this tag resolves to this digest". A digest belonging to a *different*
/// repository in the same list passes it, and so does a digest that happens to
/// be a prefix of a longer string.
///
/// Written apart from the `docker` call so it can be decided by a corpus: the
/// call needs a daemon and a pulled image, and this is where the judgement is.
pub(crate) fn one_of_them_is(listed: &str, tag: &str, digest: &str) -> bool {
    // The repository is the tag without its version — `RepoDigests` never
    // carries a tag, only `repository@digest`.
    //
    // **A slash after the colon means the colon is a port.** A registry named
    // with a port, then a repository under it, is not a repository whose version
    // contains a slash — only the form that ends in a colon and a bare version
    // has a version to strip. <!-- allow-reference: the examples are image
    // references, not paths in this repository -->
    let repository = tag
        .rsplit_once(':')
        .filter(|(_, after)| !after.contains('/'))
        .map_or(tag, |(before, _)| before);
    listed
        .split(['"', ',', '[', ']'])
        .filter_map(|entry| entry.split_once('@'))
        .any(|(named, found)| named == repository && found == digest)
}

/// Every path an `include:` block names, in order.
///
/// Both spellings: a bare `- path` entry, and the long form with a `path:` key.
/// A file that names another is read too, so a chain of them is followed — the
/// list is walked with an index for exactly that.
fn includes(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("include:") {
            inside = true;
            continue;
        }
        if inside && !line.starts_with(' ') && !line.starts_with('-') && !trimmed.is_empty() {
            inside = false;
        }
        if !inside {
            continue;
        }
        let entry = trimmed
            .trim_start_matches('-')
            .trim()
            .trim_start_matches("path:")
            .trim()
            .trim_matches(['"', '\'']);
        if entry.is_empty() || entry.ends_with(':') || entry.starts_with('/') {
            continue;
        }
        found.push(entry.to_owned());
    }
    found
}

/// Whether a line declares a service is built rather than pulled.
///
/// **Flow style is YAML too.** The test was `== "build:"` or a `"build: "`
/// prefix, and `sneaky: {build: {context: "."}, command: […]}` is one line that
/// matches neither — measured accepted, with a `Dockerfile` reading
/// `FROM postgres:latest`, and `cargo xtask dev` builds and runs it. A key is a
/// key wherever the mapping opens.
fn is_a_build_key(line: &str) -> bool {
    // **Quotes come off first.** YAML permits a quoted key and compose
    // normalises it away, so `"build":` reached the running stack past a test
    // for `build:` — measured, a service built from a Dockerfile reading
    // `FROM postgres:latest` with `ok 4 image(s) … each pinned by digest`.
    line.split(['{', ','])
        .map(|piece| piece.trim_start().replace(['"', '\''], ""))
        .any(|piece| {
            // `build:{` is not here: `split(['{', ','])` guarantees no piece
            // holds a `{`, so that clause was unsatisfiable by construction —
            // found by the meta-gate, which reported a weakening of it as
            // surviving because nothing could tell the two apart. A clause no
            // input can decide is not a defence, it is a reason to think there
            // is one.
            piece == "build:" || piece.starts_with("build: ")
        })
}

/// Every compose file in the repository root, read as written.
fn in_the_compose_files(root: &Path, findings: &mut Vec<Finding>) -> usize {
    let mut read = 0_usize;
    // **And every file `include:` names**, because eight fixed names in the
    // root are not the set docker reads. Measured: `include: [stack/extra.yml]`
    // with `image: ${NC_IMG:-…@sha256:…}` in the included file gave
    // `ok 3 image(s) in the resolved stack and 2 written in its files, each
    // pinned by digest` and `check passed`, while
    // `NC_IMG=postgres:latest docker compose --profile "*" config` resolved to
    // `image: postgres:latest`. The resolved pass interpolates from *this*
    // process's environment; the file pass is the one that catches a variable,
    // and it never opened the file the variable was in.
    // **`COMPOSE_FILES`, because this list was written twice in one file.** The
    // const above and this literal held the same eight names for two readers —
    // `where_it_is_written`, which decides what a finding points at, and this
    // pass, which decides what is read at all — so a name added to one was a
    // file the other still ignored, with both counts printed and `check passed`.
    let mut named: Vec<String> = COMPOSE_FILES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    let mut at = 0_usize;
    while at < named.len() {
        let Ok(text) = std::fs::read_to_string(root.join(&named[at])) else {
            at += 1;
            continue;
        };
        for included in includes(&text) {
            if !named.contains(&included) {
                named.push(included);
            }
        }
        at += 1;
    }
    for named in named {
        let named = named.as_str();
        let Ok(text) = std::fs::read_to_string(root.join(named)) else {
            continue;
        };
        // **A block scalar's body is text, not keys.** This read every line of
        // every compose file with no guard, while its sibling `where_it_is_written`
        // skips a comment line — one rule, two copies, and the wider one is what
        // feeds the `written < read` floor. Measured, an `image:` line written
        // inside the healthcheck's folded scalar after a trailing hash so the
        // shell ignores it: `ok 2 image(s) in the resolved stack and 3 written in
        // its files`. Padding `written` buys room under the floor that exists
        // because a spelling the file pass cannot read is an image none of its
        // rules apply to.
        let mut scalar_at: Option<usize> = None;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            let indent = line.len() - line.trim_start().len();
            if let Some(opened) = scalar_at {
                if trimmed.is_empty() || indent > opened {
                    continue;
                }
                scalar_at = None;
            }
            // **The header line is still a key.** `image: >-` ends in a scalar
            // marker, so this skipped the `image:` line *and* its body — the
            // real pin vanished from `written`. Measured together with the hole
            // below: `ok 2 image(s) in the resolved stack and 2 written in its
            // files`, `check passed`, while `PG_IMAGE=postgres:latest docker
            // compose config` brings up an unpinned stack.
            let opens_a_scalar = matches!(
                trimmed.rsplit(' ').next().unwrap_or_default(),
                "|" | ">" | "|-" | ">-" | "|+" | ">+"
            );
            if opens_a_scalar {
                scalar_at = Some(indent);
                // Fall through: the key on this line is read before its body is
                // skipped.
            }
            // **A quoted scalar may span lines too.** The reader had no notion
            // of one, so the second line of `x-note: "why this block exists
            // <newline> image: <a pinned digest>"` was read as an `image:` key
            // and paid `written` back by hand — restoring the count the block
            // scalar had just taken, under the floor that exists because a
            // spelling this pass cannot read is an image none of its rules
            // apply to.
            //
            // **The two quote characters, counted apart.** They were summed into
            // one total, so one of each is even and the tracker never fired:
            // measured, a note inside the healthcheck block whose text holds an
            // ordinary English apostrophe, with a pinned image spelled on its
            // second line, gave `ok 2 image(s) in the resolved stack and 3
            // written in its files` — where the same note without the
            // apostrophe gave 2. One apostrophe pays `written` back by hand, and
            // `written` is the only count floored against `read`.
            if !opens_a_scalar {
                for quote in ['"', '\''] {
                    if trimmed.matches(quote).count() % 2 == 1 {
                        scalar_at = Some(indent);
                        break;
                    }
                }
            }
            // A comment is not a key either, which the sibling already knew.
            if trimmed.starts_with('#') {
                continue;
            }
            // **A quoted key is the same key**, which this pass did not know:
            // `"image":` is a YAML spelling compose normalises away, so the one
            // pass that reads a *variable* was turned off by two characters —
            // the attack it exists for. It stripped every quote on the line to
            // get past that; `the_image_on` strips the key's own, which is
            // where the quoting is, and leaves the value alone.
            if let Some(image) = the_image_on(trimmed) {
                read += 1;
                let image = image.trim().trim_matches(['"', '\'']);
                if pinned_by_digest(image) {
                    continue;
                }
                findings.push(Finding {
                    file: named.to_owned(),
                    line: index + 1,
                    what: format!("`{image}` is written here and is not pinned by digest"),
                    fix: "written `image:tag@sha256:<64 hex>`, the digest is what a \
                          reader and every machine agree on. A variable is not a pin \
                          either: it resolves against whichever environment ran the \
                          command, so this gate and `cargo xtask dev` would be looking \
                          at two different stacks"
                        .to_owned(),
                });
            // **The raw line, not the unquoted one.** Stripping here as well
            // made `is_a_build_key`'s own unquoting unreachable from both call
            // sites — `docker compose config` emits no quotes either — and the
            // meta-gate reported a weakening of it as surviving, because nothing
            // could tell the two apart. A defence with no reachable input is a
            // defence nothing keeps honest, so the normalisation is left where
            // the predicate can be judged on it.
            } else if is_a_build_key(trimmed) {
                findings.push(Finding {
                    file: named.to_owned(),
                    line: index + 1,
                    what: "this service is built here rather than pulled, so no digest \
                           pins what it runs"
                        .to_owned(),
                    fix: "`docker compose config` shows no `image:` for a built service, \
                          so nothing in this gate ever looks at its `FROM` line — a \
                          Dockerfile saying `FROM postgres:latest` is invisible. This \
                          stack pulls two pinned images (`§18.4`, `§18.5`) and builds \
                          nothing; if that changes, the pin has to move to the \
                          Dockerfile and this gate has to follow it there"
                        .to_owned(),
                });
            }
        }
    }
    read
}

/// The path below the member a file belongs to, when it is directly under one.
///
/// **Every member tree cargo resolves**: the ones [`inside_a_member`] reads —
/// `crates/*` and `plugins/*` — and `xtask`. It named two arms, `crates/*` and
/// `xtask`, after `plugins/*` joined the members, so a canary directory at a
/// plugin's root was none to the predicates over it while a crate's was. Used
/// by [`inside_a_canary_directory`] for the same reason [`an_integration_test`]
/// reads the directory directly under the member: a directory cargo treats
/// specially is one *at* a member's root, and the word anywhere in a path is a
/// different thing.
fn directly_under_a_member(relative: &str) -> Option<&str> {
    inside_a_member(relative)
        .map(|(_, inside)| inside)
        .or_else(|| relative.strip_prefix("xtask/"))
}

/// Whether a file is itself inside a `canaries/` directory.
///
/// Named so the walk and its fixture read one rule. A canary holds a real
/// registry code and a blanket allow *by design* — that is what makes it a
/// canary — so a canary naming another is not a defect, and the fixture that
/// plants in every real directory has to know the same thing the walk does
/// rather than restate it.
///
/// **`contains("/canaries/")` is not that, which is the `/tests/` defect
/// again.** [`an_integration_test`] was narrowed beside this — a module a
/// member calls `tests` under its own `src` is shipped code, cargo compiles it,
/// and the walk stepped over it — and the same sentence is true of the word
/// `canaries`: `mod canaries;` with a `canaries` module under a member's own
/// `src` is compiled into the library by cargo, is reached by no `#[path]`, and
/// was excluded from
/// the code-producer walk by this. The floor at `check.rs`'s
/// `every_code_is_what_the_registry_says_it_is` shares the predicate, so both
/// sides agreed about ground neither read. Held shut today only by the doc
/// ratchet and by a hand-pinned count of canary directories, neither of which
/// is this rule.
///
/// What the exclusion is for is a directory a person put beside a member's
/// `src` to hold code nothing compiles — the repository root's own, or one at a
/// member's root — and not the word anywhere in a path.
pub(crate) fn inside_a_canary_directory(relative: &str) -> bool {
    relative.starts_with("canaries/")
        || directly_under_a_member(relative).is_some_and(|inside| inside.starts_with("canaries/"))
}

/// Whether the canary walk reads this file.
///
/// **One rule, because the floor beside it read different ground.** The walk
/// takes `.rs` files *and* manifests — cargo compiles what a manifest names —
/// and the second count it was held against was `rust_lines`, which takes `.rs`
/// only. The manifests are 470 lines, so the floor carried 470 lines of
/// permanent slack and was written `<` rather than `!=` to survive it: measured,
/// a clause skipping `docs.rs` and `main.rs` together — 388 lines — left
/// `ok 149818 source line(s) compile no canary` and `check passed`, and the
/// same clause over a 505-line file was reported at once. The threshold between
/// them was exactly the manifests.
pub(crate) fn is_read_by_the_canary_walk(root: &Path, path: &Path) -> bool {
    let relative = relative_to(root, path);
    let manifest = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| CANARY_READS_BY_NAME.contains(&name));
    (path
        .extension()
        .and_then(|kind| kind.to_str())
        .is_some_and(|kind| CANARY_READS.contains(&kind))
        || manifest)
        && !inside_a_canary_directory(&relative)
}

/// The kinds the canary walk opens, by extension.
///
/// **Two lists rather than two literals, so a fixture can derive from them.**
/// The predicate said `kind == "rs"` and `name == "Cargo.toml"` inline, which
/// is not a thing anything can enumerate — and the only fixture driving the
/// walk's body planted `.rs` and nothing else, for as long as the manifest half
/// has existed. Measured in wave 38: a clause in the walk body dropping
/// manifests, plus five lines of `[[test]] path = "canaries/…"`, and the gate
/// whose whole title is "nothing cargo compiles reaches into a `canaries/`
/// directory" said `ok` with its own count *up* while cargo built a binary out
/// of one. The citation gates have had this shape for longer and it is why
/// their fixture could be widened at all.
pub(crate) const CANARY_READS: [&str; 1] = ["rs"];

/// The names it opens whatever their extension, which is the half that matters.
///
/// A canary is compiled in by a manifest naming it, so this list is the gate's
/// reason for existing rather than an afterthought beside the `.rs` half.
pub(crate) const CANARY_READS_BY_NAME: [&str; 1] = ["Cargo.toml"];

/// What the canary walk reads, asked of the predicate both its counts share.
#[cfg(test)]
mod what_the_canary_walk_reads {
    use std::path::Path;

    /// The predicate both readings ask, driven directly.
    ///
    /// **Sharing one predicate makes the two counts agree and makes the
    /// predicate itself unpinned.** Round forty-five replaced a one-sided floor
    /// with an equality where both sides call `is_read_by_the_canary_walk`, so
    /// a clause *inside* it moves both numbers together: measured,
    /// `"Cargo.toml"` written `"Cargo.tom"` left 520 tests passing and
    /// `check passed`, with the entire transcript diff being one count falling
    /// by 473 — the manifests, silently dropped. The manifest half is the half
    /// this walk exists for: five lines of `[[test]] path = "canaries/…"` are
    /// how a canary gets compiled.
    ///
    /// A shared predicate is still right. What it needs is a corpus of its own,
    /// which is this.
    #[test]
    fn the_predicate_reads_sources_and_manifests_and_nothing_else() {
        let root = Path::new("/nowhere");
        for (what, path) in [
            ("a source file", "/nowhere/xtask/src/check.rs"),
            ("a manifest", "/nowhere/crates/nc-log/Cargo.toml"),
            ("the workspace manifest", "/nowhere/Cargo.toml"),
        ] {
            assert!(
                super::is_read_by_the_canary_walk(root, Path::new(path)),
                "`{what}` is compiled and this walk does not read it"
            );
        }
        for (what, path) in [
            ("another toml", "/nowhere/catalog/errors.toml"),
            ("a document", "/nowhere/AGENTS.md"),
            ("a migration", "/nowhere/migrations/0001_x.sql"),
            ("a lock file", "/nowhere/Cargo.lock"),
            (
                "a source inside a canary directory",
                "/nowhere/xtask/canaries/shipped.rs",
            ),
            (
                "a manifest inside a canary directory",
                "/nowhere/xtask/canaries/Cargo.toml",
            ),
        ] {
            assert!(
                !super::is_read_by_the_canary_walk(root, Path::new(path)),
                "`{what}` is not compiled from this tree and this walk read it"
            );
        }
    }
}

/// How many lines the canary walk has to read, counted a second way.
///
/// The walk above reports what it read; this walks the tree again and says what
/// there was. Both are the same set by construction — they ask one predicate —
/// so the two numbers are compared for equality rather than for a floor.
///
/// # Errors
///
/// Returns why the tree could not be walked.
pub(crate) fn the_lines_the_canary_walk_reads(root: &Path, except: &Path) -> Result<usize, String> {
    let mut found = 0_usize;
    walk(root, &mut |path| {
        if path.starts_with(except) || !is_read_by_the_canary_walk(root, path) {
            return;
        }
        if let Ok(text) = std::fs::read_to_string(path) {
            found += text.lines().count();
        }
    })?;
    Ok(found)
}

/// No crate reaches into a `canaries/` directory.
///
/// **The exemption rests on the canary not being compiled, and nothing held
/// it.** `crates/nc-canon/canaries/a_code_written_by_hand.rs` opens *"Not
/// compiled: outside this crate's `src/`, so cargo never sees it"* — a promise,
/// not a rule. Measured: five lines in `crates/nc-canon/src/lib.rs`
///
/// ```rust
/// #[path = "../canaries/a_code_written_by_hand.rs"]
/// pub mod shipped;
/// ```
///
/// with the canary's constant changed to a real registry code, and
/// `strings target/debug/libnc_canon.rlib | grep -c NC-DB-0001` answered `2`.
/// The §15.1 gate found it and `without_the_canary` dropped it, because the
/// finding's path *is* the canary's — exactly one finding, so the
/// more-than-one guard never fired either. The canonicalisation crate the
/// journal hashes through shipped a hand-written error code, and every count
/// went up.
///
/// # Errors
///
/// Returns why the sources could not be read.
pub(crate) fn no_crate_compiles_a_canary(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    walk(root, &mut |path| {
        // **And manifests, because cargo compiles what one names.** The gate's
        // own title is "nothing cargo compiles reaches into a `canaries/`
        // directory", and it read `.rs` only — so five lines of `[[test]] path
        // = "canaries/…"` built one, with `check passed` and
        // `target/debug/deps/shipped-*` on disk. The rule is
        // `is_read_by_the_canary_walk` so the floor beside this can ask it too.
        if !is_read_by_the_canary_walk(root, path) {
            return;
        }
        let relative = relative_to(root, path);
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        // The file's own lines, so this count is the same quantity `rust_lines`
        // produces and the floor can be exact. `without_comments` drops lines,
        // which made the two disagree by construction.
        for (index, line) in text.lines().enumerate() {
            read += 1;
            // **A comment opens after whitespace, not inside a path.**
            // `split("//")` cut `#[path = "..//canaries/…"]` in half — a
            // doubled separator cargo accepts and compose of the two tokens
            // this gate needs — so the attack its own doc describes replayed
            // with both counts *up*: measured, `NC-DB-0001` in
            // `libnc_canon.rlib` and `check passed`.
            let line = line_without_a_comment(line);
            // **A module declaration, not a mention.** `check.rs` names each
            // canary in a constant and `gates.rs` reads them, which is the gate
            // working; what compiles one in is `#[path]`, `include!` and their
            // relatives. Matching the word alone reported the gate itself.
            // **`path =` anywhere, not `#[path` at the start.**
            // `#[cfg_attr(all(), path = "../canaries/…")]` is the same
            // attribute behind a predicate that is always true, and it compiled
            // a canary in with every count *up* — measured, `NC-KERN-9999`
            // twice in `libnc_canon.rlib` and `check passed`. The wrapped form
            // was caught only by rustfmt, which is not a gate.
            let reaches = [
                "path =",
                "path=",
                "include!",
                "include_str!",
                "include_bytes!",
            ];
            if !line.contains("canaries") || !reaches.iter().any(|form| line.contains(form)) {
                continue;
            }
            findings.push(Finding {
                file: relative.clone(),
                line: index + 1,
                what: "this reaches into a `canaries/` directory, so cargo compiles what \
                       is in it"
                    .to_owned(),
                fix: "a canary carries exactly what a gate refuses, and the gate is told \
                      to ignore findings in it — so anything cargo compiles from there \
                      ships the defect with the run still green. Measured: five lines \
                      of `#[path]` put a real registry code into `libnc_canon.rlib` and \
                      every printed count rose. A canary is read by `cargo xtask check` \
                      and by nothing else"
                    .to_owned(),
            });
        }
    })?;
    Ok((findings, read))
}

/// The file with every `const fn … -> ErrorCode { ErrorCode::X }` blanked.
///
/// **A function whose whole body is one code is a name for that code, not a
/// use of it.** `MachineFlake::exhausted` is three lines and has no caller
/// outside its own tests — pairing is Stage 24 — so nothing in this tree can
/// report flake exhaustion, and the registry says `reserved`. The gate's needle
/// is the variant's name, which that function supplies, so the two disagreed
/// about a fact neither could be wrong about on its own.
///
/// Narrow on purpose: only a body that is *exactly* one `ErrorCode::` path,
/// with nothing else in it. `Error::code()`'s match arms are not this — they
/// map a value a caller holds, which is a use — and neither is anything that
/// builds an error and returns it.
///
/// **And it read two literals while the finder five lines below resolved
/// four.** [`error_code_names`] exists because a needle list cannot resolve a
/// name, and this — the exclusion that decides *mapping table* from *raise* —
/// went on matching `-> ErrorCode {` and a body opening `ErrorCode::`. Under
/// any spelling that function resolves and this one does not, the exclusion
/// stops applying while the finder keeps working, and the gate then reports a
/// producer that is not one. Measured, one line in `crates/nc-ids/src/flake.rs`
/// — the import respelled and `MachineFlake::exhausted`'s body written as the
/// bare variant it then names:
///
/// ```text
/// use nc_catalog::ErrorCode::{self, IdsFlakeExhausted};
///   ->  error `NC-IDS-0003` is declared `reserved` and is produced by a crate
///       outside the catalog
/// ```
///
/// `catalog/errors.toml`'s own comment on that number says that exact function
/// is not a producer, so the gate contradicted the registry about a fact
/// neither can be wrong about on its own — and a gate wrong in *that* direction
/// teaches a reader to ignore it.
///
/// So the same two lists the finder resolves are passed in: `types` are the
/// names this file may write in front of a `::`, and `bare` the variants it
/// imported without one. A body that is exactly one of those is a name for a
/// code rather than a use of it, however the file spelled the import.
///
/// The *fully qualified* `-> nc_catalog::ErrorCode { nc_catalog::ErrorCode::X }`
/// is deliberately still a producer: `types` holds the names a path may open
/// with, and `nc_catalog` is not one. `FlakeOutOfRange::code`
/// (`crates/nc-ids/src/flake.rs`) is written that way and is the only non-test
/// producer of `NC-VAL-0001`, which is declared `raised` on the strength of it —
/// so widening this to qualified paths would move a registry verdict rather than
/// close a hole.
fn without_bare_returns(text: &str, types: &[String], bare: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len());
    let lines: Vec<&str> = text.lines().collect();
    let mut at = 0_usize;
    while at < lines.len() {
        let head = lines[at].trim_end();
        let opens = types
            .iter()
            .any(|name| head.ends_with(&format!("-> {name} {{")));
        let body = lines.get(at + 1).map(|line| line.trim());
        let closes = lines.get(at + 2).map(|line| line.trim());
        if opens
            && body.is_some_and(|body| {
                !body.contains(['(', ',', '{'])
                    && (types
                        .iter()
                        .any(|name| body.starts_with(&format!("{name}::")))
                        || bare.iter().any(|(written, _)| body == written))
            })
            && closes == Some("}")
        {
            for _ in 0..3 {
                out.push('\n');
            }
            at += 3;
            continue;
        }
        out.push_str(lines[at]);
        out.push('\n');
        at += 1;
    }
    out
}

/// The file with the *inside* of every Rust string literal blanked out.
///
/// **A `#[doc = "…"]` attribute was a producer.** `without_comments` strips
/// `//`, `///`, `//!` and `/* … */` — every *comment* — and a doc **attribute**
/// is not a comment, it is a string literal in an attribute position. So
/// `#[doc = "… nc_catalog::ErrorCode::LogUnreachable …"]` made the reserved
/// `NC-LOG-0001` "produced by a crate outside the catalog", and the same
/// spelling on the other side keeps a `raised` declaration true after the last
/// real producer is gone. Measured, one line added to `crates/nc-canon/src/lib.rs`:
///
/// ```text
/// error `NC-LOG-0001` is declared `reserved` and is produced by a crate outside the catalog
/// ```
///
/// Every ordinary string literal is the same hole one keyword narrower —
/// `let hint = "see ErrorCode::LogDropped";` — so what is blanked is the string
/// content and not the attribute, because `ErrorCode::X` is a **path
/// expression** and a path expression never occurs inside a string.
///
/// The quotes are kept and only their content goes, so nothing downstream that
/// counts delimiters is disturbed; the openers are recognised with the same
/// `raw_string_at` / `char_literal_at` / `closes_raw` this file already uses for
/// comments, so the two scanners cannot disagree about where a string is.
pub(crate) fn without_string_contents(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        let here = bytes[index];
        // A raw string: `r"…"`, `r#"…"#`, and the `br`/`cr` prefixes.
        if let Some((opener, hashes)) = raw_string_at(&bytes, index) {
            for offset in 0..opener {
                out.push(bytes[index + offset]);
            }
            index += opener;
            while index < bytes.len() {
                if bytes[index] == '"' && closes_raw(&bytes, index + 1, hashes) {
                    out.push('"');
                    index += 1 + hashes;
                    break;
                }
                // A raw string may span lines; the newline is kept so line
                // numbers downstream do not move.
                out.push(if bytes[index] == '\n' { '\n' } else { ' ' });
                index += 1;
            }
            continue;
        }
        // **A quote inside a comment is not a quote.** Nothing here knew what a
        // comment was, which the one-line bound below used to hide: an
        // apostrophe or a lone quote in prose opened a string that ended at the
        // newline, costing one line. With a literal free to span lines — which
        // it is — the same quote runs until the next one, and real code in
        // between is handed back to every caller as blanked text. Measured, the
        // moment the bound came off: `ui.rs`'s printer walk reported two of 505
        // bodies no longer ending at their own closing brace.
        //
        // Copied through rather than spent, because the callers that care about
        // comments strip them with `without_comments` first, and the ones that
        // do not are counting brackets, which a comment should not move.
        if here == '/' && bytes.get(index + 1) == Some(&'/') {
            while index < bytes.len() && bytes[index] != '\n' {
                out.push(bytes[index]);
                index += 1;
            }
            continue;
        }
        if here == '/' && bytes.get(index + 1) == Some(&'*') {
            let mut depth = 0_usize;
            while index < bytes.len() {
                if bytes[index] == '/' && bytes.get(index + 1) == Some(&'*') {
                    depth += 1;
                    out.push('/');
                    out.push('*');
                    index += 2;
                } else if bytes[index] == '*' && bytes.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    out.push('*');
                    out.push('/');
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            continue;
        }
        // A char literal is not a quote: `'"'` must not open a string, and `'a`
        // is a lifetime rather than a literal at all.
        if here == '\''
            && let Some(width) = char_literal_at(&bytes, index)
        {
            // **Blanked like a string, because a char literal holds text too.**
            // This copied the literal through verbatim, so `'{'` and `'}'` were
            // braces to every brace-counting scan downstream. Measured in
            // `ui.rs`'s counter-reader rule: a probe with `let at = "x}y".find('}');`
            // above the line it reads is invisible, while the identical probe
            // without the char literal is named. The quotes are kept so the
            // literal is still a literal; only what is between them is spent.
            out.push('\'');
            for offset in 1..width - 1 {
                let _ = offset;
                out.push(' ');
            }
            if width > 1 {
                out.push('\'');
            }
            index += width;
            continue;
        }
        if here == '"' {
            out.push('"');
            index += 1;
            let mut escaped = false;
            while index < bytes.len() {
                let inside = bytes[index];
                if escaped {
                    escaped = false;
                    // **The escaped character may be the newline itself.**
                    // `\`-at-end-of-line is Rust's continuation, which is the
                    // form every fixture in this repository is written with, and
                    // spending it as a space lost the line: measured, three
                    // lines in and two out. `blanked` keeps it, and
                    // `the_stripper_keeps_the_line_count` holds `blanked` and
                    // `without_comments` to that rule while this function was
                    // not in the test at all — so `check.rs`'s sentence "newlines
                    // are kept, so every line number below still counts from the
                    // top of the file" was false of the function it was written
                    // above.
                    out.push(if inside == '\n' { '\n' } else { ' ' });
                    index += 1;
                    continue;
                }
                match inside {
                    '\\' => {
                        escaped = true;
                        out.push(' ');
                    }
                    '"' => {
                        out.push('"');
                        index += 1;
                        break;
                    }
                    // **A newline does not end it, and the sentence here said
                    // it did.** Rust string literals may hold a bare newline —
                    // `let s = "one\ntwo";` written across two lines compiles,
                    // and so does the `\`-continued form this repository writes
                    // its fixtures with. So everything after the first line of
                    // such a literal was handed back to every caller as *code*.
                    // Measured, on `let source = "\<newline>fn wanted() {}
                    // <newline>#[test]<newline>fn other() {}<newline>";`:
                    //
                    // ```text
                    // "let source = \"                \n#[test]\nfn other() {}\n\" \n"
                    // ```
                    //
                    // — the first line spent and the rest live, which put
                    // `fn other` into the undocumented-item census as an item of
                    // `check.rs` and a `#[test]` into the test census as a test
                    // that does not exist. Both censuses are floors under
                    // numbers this gate prints.
                    //
                    // The newline is kept so line numbers downstream do not
                    // move, which is what the raw-string branch above already
                    // does for the same reason. An unbalanced quote is a compile
                    // error, and cargo builds every file this reads in the same
                    // run, so the bound this replaced was protecting against
                    // something that cannot reach here.
                    '\n' => out.push('\n'),
                    _ => out.push(' '),
                }
                index += 1;
            }
            continue;
        }
        out.push(here);
        index += 1;
    }
    out
}

/// One past the end of the block comment opening at `index`, nesting included.
///
/// The end of the file when it never closes, which spends the rest of it rather
/// than handing it back as code — the direction [`blanked`]'s doc names.
fn past_a_block_comment(chars: &[char], index: usize) -> usize {
    let mut depth = 0_usize;
    let mut at = index;
    while at < chars.len() {
        if chars[at] == '/' && chars.get(at + 1) == Some(&'*') {
            depth += 1;
            at += 2;
            continue;
        }
        if chars[at] == '*' && chars.get(at + 1) == Some(&'/') {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
            continue;
        }
        at += 1;
    }
    chars.len()
}

/// The file with every comment and every literal spent, character for character.
///
/// **The one thing [`without_string_contents`] and [`without_comments`] do not
/// give: a text whose offsets are still the original's.**
/// `without_string_contents` preserves length for an ordinary string and a char
/// literal and **loses it on a raw string** — the closing `"#` of `r#"…"#` is
/// written back as one character, so every offset after the first raw string in
/// a file is short by the number of hashes. `without_comments` does not
/// preserve length at all: it replaces a block comment with newlines. Neither
/// is wrong; both are read by callers that count delimiters or lines rather
/// than index back into the source.
///
/// This one is for the caller that has to *index*: whatever is here at
/// character `n` is at character `n` of the original, so a brace found in this
/// text is a brace in the file. Comments become spaces — including the `//`
/// and the `/*` that open them — string and raw-string contents become spaces,
/// and a char literal's content becomes a space, because `'{'` and `'}'` are
/// braces to anything that counts them. Newlines survive everywhere so line
/// numbers do not move either.
///
/// Block comments nest, as Rust's do. An unterminated comment or literal spends
/// the rest of the file rather than returning it as code, which is the safe
/// direction: a scanner that reads a comment as code is what this exists to
/// stop.
///
/// **Not test-only any more.** [`outside_the_tests`] counted braces on raw text
/// and this is the text a brace counter has to read; the two lived one file
/// apart with the shipped scanner using the naive one.
pub(crate) fn blanked(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    // A newline is kept and everything else is spent, so the count is unmoved.
    let spend = |out: &mut String, character: char| {
        out.push(if character == '\n' { '\n' } else { ' ' });
    };
    while index < chars.len() {
        let here = chars[index];
        // A line comment, `//` and `///` and `//!` alike: everything to the
        // newline, which is left where it is.
        if here == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                spend(&mut out, chars[index]);
                index += 1;
            }
            continue;
        }
        // A block comment, which nests.
        if here == '/' && chars.get(index + 1) == Some(&'*') {
            let past = past_a_block_comment(&chars, index);
            for character in &chars[index..past] {
                spend(&mut out, *character);
            }
            index = past;
            continue;
        }
        // A raw string: `r"…"`, `r#"…"#`, and the `br`/`cr` prefixes. The
        // closing hashes are spent rather than dropped, which is the whole
        // difference from `without_string_contents`.
        if let Some((opener, hashes)) = raw_string_at(&chars, index) {
            // **The opening hashes are spent too, so what comes back is still
            // Rust.** They were kept while the closing ones were spent, which
            // leaves `r#"x"#;` as `r#" " ;` — a raw string that opens and never
            // closes. Any reader that scans the blanked text then runs one
            // delimiter out of phase for the rest of the file: measured by a
            // reviewer on 2026-09-12, one `let _ = r#"x"#;` in `check.rs`
            // turned the floor census red with a list of garbage and dropped a
            // row from the census beside it, naming no floor and no defect.
            // `check.rs` holds no raw string in code today, which is the only
            // reason nobody had met it.
            //
            // With both sides spent the result reads as an ordinary empty
            // string, which is what it is, and the length is unmoved either
            // way.
            for offset in 0..opener {
                let character = chars[index + offset];
                out.push(if character == '#' { ' ' } else { character });
            }
            index += opener;
            while index < chars.len() {
                if chars[index] == '"' && closes_raw(&chars, index + 1, hashes) {
                    out.push('"');
                    for _ in 0..hashes {
                        out.push(' ');
                    }
                    index += 1 + hashes;
                    break;
                }
                spend(&mut out, chars[index]);
                index += 1;
            }
            continue;
        }
        // A char literal is not a quote: `'"'` must not open a string, and `'a`
        // is a lifetime rather than a literal at all.
        if here == '\''
            && let Some(width) = char_literal_at(&chars, index)
        {
            out.push('\'');
            for _ in 1..width - 1 {
                out.push(' ');
            }
            out.push('\'');
            index += width;
            continue;
        }
        if here == '"' {
            out.push('"');
            index += 1;
            let mut escaped = false;
            while index < chars.len() {
                let inside = chars[index];
                if escaped {
                    escaped = false;
                    spend(&mut out, inside);
                    index += 1;
                    continue;
                }
                match inside {
                    '\\' => {
                        escaped = true;
                        out.push(' ');
                    }
                    '"' => {
                        out.push('"');
                        index += 1;
                        break;
                    }
                    // **A string literal may span lines**, which the two
                    // strippers beside this one both used to get wrong: each
                    // carried the sentence "Rust has no unescaped newline in a
                    // string literal", and it is not so — a raw newline inside
                    // quotes is part of the string, and the `\` at a line end is
                    // the *continuation* that removes one. Both are repaired;
                    // this one was right from the start. Measured, on
                    // `xtask/tests/every_generated_value_is_compared.rs`, whose
                    // `a_function_body_stops_at_its_own_brace` holds a
                    // seven-line literal: ending the string at the first
                    // newline handed four of that literal's braces back as
                    // code, and the file stopped balancing.
                    _ => spend(&mut out, inside),
                }
                index += 1;
            }
            continue;
        }
        out.push(here);
        index += 1;
    }
    out
}

/// Where every module of tests in a file begins and ends, in byte offsets.
///
/// The whole item each time, `#[cfg(test)]` attribute included, so a caller can
/// cut them all out and be left with **everything the file ships** — the code
/// above the tests *and* the code below them, which is the half that had no
/// reader.
///
/// **Plural, and that is a finding rather than a generalisation.** The rule was
/// specified against "the last column-zero `#[cfg(test)]`", and `release.rs`
/// has two modules of tests — `mod tests` and `mod relock_tests` at the foot of
/// the file — so the last one alone leaves the first, four hundred lines of it,
/// counted as shipped code. Every module of tests is cut, and a caller that
/// wants to know it cut the right ones can check that what is left holds no
/// `#[test]`.
///
/// **Code below a file's test module is shipped code, and four attempts at
/// finding where a module ends did not bind.** `rfind("\n}")` finds an appended
/// item's own brace; counting braces on the raw text is skewed by a `{` inside
/// a string literal in the tests; blanking the strings with
/// [`without_string_contents`] shifts every offset after the first raw string,
/// because that function does not preserve length; and indexing a consistently
/// blanked text still does not close while char literals such as `'{'` survive.
/// [`blanked`] answers all four, and this is its caller.
///
/// The anchor is a column-zero `#[cfg(test)]` **found in the blanked text**, so
/// the same words written inside a string or a comment — which is what a
/// scanner reading its own source meets — are not an anchor. A `#[cfg(test)]`
/// item that is not a module is skipped rather than measured: `release.rs` has
/// two `#[cfg(test)] const`s, and a brace-matcher pointed at one of those finds
/// the *next* item's brace and swallows everything between.
///
/// Returns `None` when a module's braces never close, which is a file this
/// cannot answer for — and a caller that must not be told a wrong answer.
#[cfg(test)]
pub(crate) fn the_test_modules(text: &str) -> Option<Vec<core::ops::Range<usize>>> {
    let neutral: Vec<char> = blanked(text).chars().collect();
    let source: Vec<(usize, char)> = text.char_indices().collect();
    // The two are one coordinate system or this returns nothing: every offset
    // below is taken from `neutral` and handed back as a byte offset into
    // `text`, and `blanked` preserving length is what makes that legal.
    if neutral.len() != source.len() {
        return None;
    }
    let opener: Vec<char> = ["#[cfg(", "test)]"].join("").chars().collect();
    let mut anchors = Vec::new();
    let mut starts_a_line = true;
    for index in 0..neutral.len() {
        if starts_a_line
            && neutral.len() - index >= opener.len()
            && neutral[index..index + opener.len()] == opener[..]
        {
            anchors.push(index);
        }
        starts_a_line = neutral[index] == '\n';
    }
    let mut found = Vec::new();
    for anchor in anchors {
        // **A module, not any `#[cfg(test)]` item.** The head of the item runs
        // to the first of `{`, `;` and `=`; a `const` or a `static` reaches one
        // of the latter two, and matching braces from there would find the
        // *next* item's brace and cut out everything between.
        let Some(head) = (anchor..neutral.len())
            .find(|&at| matches!(neutral[at], '{' | ';' | '='))
            .filter(|&at| neutral[at] == '{')
        else {
            continue;
        };
        let declaration: String = neutral[anchor..head].iter().collect();
        if !declaration.contains(&["mod", " "].join("")) {
            continue;
        }
        let mut depth = 0_usize;
        let mut closed = false;
        for at in head..neutral.len() {
            match neutral[at] {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        // `}` is one byte, so the item ends one byte past it.
                        found.push(source[anchor].0..source[at].0 + 1);
                        closed = true;
                        break;
                    }
                }
                _ => {}
            }
        }
        if !closed {
            return None;
        }
    }
    Some(found)
}

/// No source file pulls in text from a file the gates do not read.
///
/// **A fifth shape, and it needed no edit to any gate.** The `§15.1` walk takes
/// `.rs` and `.sql`; its floor calls `lines_under` with the same two kinds. So a
/// file of any other extension is outside the walk *and* outside the second
/// count that exists to catch a narrowed walk — both sides narrow together,
/// which is exactly what a floor is built to make impossible. Measured:
///
/// ```text
/// crates/nc-journal/src/handwritten.in   pub const HAND_WRITTEN: &str = "NC-KERN-4242";
/// crates/nc-journal/src/lib.rs           include!("handwritten.in");
///
/// ok    27329 source line(s) name no error code as text (§15.1)   (baseline 27327)
/// ok    68837 source line(s) compile no canary                    (baseline 68835)
/// check passed
/// strings target/debug/libnc_journal.rlib | grep -c NC-KERN-4242  ->  2
/// ```
///
/// Every number up, every canary reported, both counters agreeing line for
/// line, and the audit-log crate shipping a hand-written registry code. No
/// predicate was weakened, so mutation coverage would not have seen it either.
///
/// The rule is narrow on purpose: what cargo pulls into a compiled crate has to
/// be a file the gates read, and `.rs` is the one they all read. A generated
/// file included by a build script is the shape this would meet next, and there
/// is none here — `§15.2` generates whole modules, committed and drift-checked.
///
/// # Errors
///
/// Returns why the sources could not be read.
pub(crate) fn nothing_includes_what_the_gates_do_not_read(
    root: &Path,
) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    walk(root, &mut |path| {
        if path.extension().is_none_or(|kind| kind != "rs") {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let relative = relative_to(root, path);
        for (index, line) in text.lines().enumerate() {
            read += 1;
            let code = line_without_a_comment(line);
            // **`include!` and not `include_str!`.** The first is *code the
            // compiler parses*, which is what walked past every gate; the
            // second is data, and `crates/nc-ids/tests/against_denote.rs`
            // legitimately embeds a corpus with it. A rule that refused both
            // would be an exclusion list growing to fit the gate.
            // **Found in the code, read from the line.** `include!(` appears
            // inside this gate's own string literals — its fix text and the
            // search below — and the positive rule reported both of them. The
            // blanked copy is the same length in characters, so a position found
            // there is the position here.
            // — which was true of the name and not of the function called:
            // `without_string_contents` writes the raw-string closer back as one
            // character, measured 30 chars in and 29 out, so a line holding a raw
            // literal before an `include!(` shifted every offset left by the hash
            // count. [`blanked`] is the length-preserving one, and it spends
            // comments too.
            let blanked: Vec<char> = blanked(code).chars().collect();
            let here: Vec<char> = code.chars().collect();
            let Some(at) = blanked
                .windows("include!(".len())
                .position(|window| window.iter().collect::<String>() == "include!(")
            else {
                continue;
            };
            // **The argument, not the first string literal in it.** Reading the
            // first literal and skipping when it held no `.` meant `concat!` walked
            // straight through: `include!(concat!("plan", "ted.in"))` yields
            // `"plan"`, and so does the canonical build-script idiom
            // `include!(concat!(env!("OUT_DIR"), "/generated.rs"))` — which put
            // every build-script-generated file outside every gate in this suite.
            // Measured, a hand-written registry code in the crate the journal
            // hashes through:
            //
            // ```text
            // ok    27991 source line(s) name no error code as text (§15.1)   <- up
            // ok    71507 source line(s) compile no canary                     <- up
            // check passed
            // $ strings target/debug/libnc_canon.rlib | grep -c 'NC-KERN-4242'
            // 2
            // ```
            //
            // So the rule is positive: one string literal naming a `.rs` file is
            // what passes, and everything else is reported. A test on the shape
            // of the first argument is a form check on an argument list, not a
            // check on what is included — the same distinction
            // `the_digest_is_the_tag` exists to make one gate over.
            let argument = argument_of(&here[at + "include!(".len()..].iter().collect::<String>());
            let named = single_literal(&argument);
            let is_rust = named.as_deref().is_some_and(|name| {
                Path::new(name)
                    .extension()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("rs"))
            });
            if is_rust {
                continue;
            }
            let Some(named) = named else {
                // A name with no literal at all is this gate reading its own
                // source; `argument_of` returns what it found, and an empty
                // argument means there was nothing to include.
                if argument.trim().is_empty() {
                    continue;
                }
                findings.push(Finding {
                    file: relative.clone(),
                    line: index + 1,
                    what: format!(
                        "this assembles what it includes — `{}` — so what reaches the \
                         compiler is not written in this file",
                        argument.trim()
                    ),
                    fix: "every gate in this suite reads the files on disk, and a name \
                          built by `concat!` or `env!` names none of them. Write the \
                          include as one string literal ending `.rs`, which is the only \
                          form a walk can follow"
                        .to_owned(),
                });
                continue;
            };
            findings.push(Finding {
                file: relative.clone(),
                line: index + 1,
                what: format!("this pulls `{named}` into a compiled crate, and no gate reads it"),
                fix: "the §15.1 walk, the type wall and the blanket-allow walk all read \
                      `.rs`, and their floors count the same kinds — so a file of another \
                      extension is outside the walk *and* outside the count that exists \
                      to catch a narrowed walk, and both narrow together. Measured, an \
                      `include!` of one such file put a hand-written error code into \
                      `libnc_journal.rlib` with every printed number higher than \
                      baseline. Name the file `.rs`, or put the text in the module that \
                      includes it"
                    .to_owned(),
            });
        }
    })?;
    Ok((findings, read))
}

/// Rust files beneath a named directory that `keep` admits, counted independently.
///
/// The floor for a walk whose own count is of something else. It shares only the
/// scope predicate — the recursion and the counting are its own, which is what
/// keeps it from being a mirror of the walk it floors.
pub(crate) fn rust_files_under(root: &Path, under: &str, keep: fn(&str) -> bool) -> usize {
    let mut found = 0_usize;
    let _ = walk(&root.join(under), &mut |path| {
        if path.extension().is_some_and(|kind| kind == "rs") && keep(&relative_to(root, path)) {
            found += 1;
        }
    });
    found
}

/// Which files decide what a code's `raising` says, named for the walk and its test.
///
/// **The walk had no mechanism of any kind** — no canary, no fixture, no corpus
/// entry, no test — and a floor of `raised.len() < 5`. One line in its skip
/// chain, `relative.starts_with("crates/nc-log/")`, left two codes the registry
/// declares `reserved` being produced by a public function, with every printed
/// number *up* and the reserved count unchanged at seven:
///
/// ```text
/// ok    28 code(s) are declared as what the tree does with them, and 7 of them
///       name a number nothing raises yet
/// check passed
/// ```
///
/// `docs/errors.md` is generated from `raising`, so that publishes a subsystem
/// as raising nothing while it raises. The registry's own crate and its compiler
/// are out because they *are* the declaration; tests and canaries are out
/// because a code named there is not a producer.
pub(crate) fn in_the_raising_scope(relative: &str) -> bool {
    !(relative.starts_with("crates/nc-catalog/")
        || relative.starts_with("crates/nc-catalog-compiler/")
        || an_integration_test(relative)
        // The same predicate the canary walk reads, and not a second spelling
        // of it: this was a bare `contains("/canaries/")`, which skips a
        // `canaries` module under a member's `src` that cargo compiles into the
        // library.
        || inside_a_canary_directory(relative))
}

/// Whether `relative` is one of a member's integration tests.
///
/// **`contains("/tests/")` is not that, and the floor beside it agreed.**
/// A module a member calls `tests` under its own `src` is shipped code — cargo
/// compiles it, it is
/// in the library, and its name is an author's choice — and the walk stepped
/// over it, while `rust_files_under` stepped over it too, so the equality that
/// floors the walk balanced with a whole directory missing from both sides.
/// Measured, a public documented producer of the reserved `NC-DB-0002` in
/// exactly that file: `ok 29 code(s) are declared as what the tree does with
/// them`, every number unmoved and no line about it anywhere.
///
/// What the exclusion is for is written in `codes_something_raises`: a file
/// under `crates/*/tests/` or `plugins/*/tests/` is *whole* a test, which
/// `outside_the_tests` cannot tell because such a file carries no
/// `#[cfg(test)]`. That is a fact about the directory cargo builds integration
/// tests from — the one directly under the member — and not about the word
/// "tests" anywhere in a path.
fn an_integration_test(relative: &str) -> bool {
    inside_a_member(relative).is_some_and(|(_, inside)| inside.starts_with("tests/"))
}

/// A file's member directory under `crates/` or `plugins/`, and its path inside
/// that member.
///
/// **Both member trees, in one place.** The raising walk, its integration-test
/// exclusion and its per-member name map each spelled `strip_prefix("crates/")`
/// on their own, so the plugin tree — a member tree since the plugin build
/// arrived — was outside all three at once.
pub(crate) fn inside_a_member(relative: &str) -> Option<(&str, &str)> {
    ["crates/", "plugins/"].iter().find_map(|tree| {
        let (member, inside) = relative.strip_prefix(tree)?.split_once('/')?;
        Some((&relative[..tree.len() + member.len()], inside))
    })
}

/// Which declared codes a crate outside the catalog actually constructs.
///
/// **`catalog/errors.toml` said "every error the platform can return" over
/// six codes nothing raises.** Measured by looking for each code's own
/// `ErrorCode::` variant outside the generated crate and outside `#[cfg(test)]`
/// regions: five had no producer at all and one was recorded rather than
/// returned. A registry that cannot tell a live error from a reserved number is
/// a registry a reader has to check by grep, which is what this does once.
///
/// The scope is `crates/` and `plugins/`, minus the catalog's own two: the
/// compiler parses these codes and the generated crate declares them, and
/// neither raises one.
/// Test modules are excluded because a test that names a code proves the
/// mapping compiles, not that anything reaches it.
///
/// # Errors
///
/// Returns why the sources could not be read.
pub(crate) fn codes_something_raises(root: &Path) -> Result<(Vec<String>, usize), String> {
    let mut found = Vec::new();
    // The files this walk actually judged, so a filter inside it is visible.
    let mut read = 0_usize;
    // **Read whole, then judged, because a name is a crate's and not a file's.**
    // This resolved the enum's local names per *file*, and rustc resolves them
    // per crate: `pub use nc_catalog::ErrorCode as Code;` at one member's root
    // and `crate::Code::Planted` in a sibling file of the same member is
    // `ErrorCode::Planted` to the compiler, and matched nothing here — the
    // sibling declares no alias of its own, so its `types` held the default
    // spelling alone and the walk answered `[]`. Every other spelling this
    // resolves was closed one at a time in `error_code_names`; this is the one
    // that cannot be closed inside a single file, because the declaration is
    // not in it.
    //
    // The shape `check.rs`'s `the_aliases_each_member_declares` already uses for
    // the static-string wall, and for the same reason.
    let mut sources: Vec<(String, String)> = Vec::new();
    let mut visit = |path: &Path| {
        let relative = relative_to(root, path);
        // **And `crates/*/tests/`, where the whole file is a test.**
        // `outside_the_tests` blanks `#[cfg(test)]` modules and an integration
        // test has none — so every `raised` journal code is named in that
        // crate's own integration tests, and one that lost its real producer
        // would stay green as long as a test still spelled it. `AGENTS.md` says
        // this gate looks "outside test modules"; an integration test is one.
        if !in_the_raising_scope(&relative) || path.extension().is_none_or(|kind| kind != "rs") {
            return;
        }
        read += 1;
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        sources.push((relative, text));
    };
    walk(&root.join("crates"), &mut visit)?;
    // **And the plugin tree, which cargo builds like `crates/`.** A plugin that
    // constructs a registry code produces it as surely as a crate does, and
    // `docs/errors.md` would publish that code as raised by nothing. Walked
    // when it is there: a tree with no plugin yet has nothing to read.
    let plugins = root.join("plugins");
    if plugins.is_dir() {
        walk(&plugins, &mut visit)?;
    }

    // The names each member gives the enum, from every file the member has.
    //
    // The *types* only: a name that may stand in front of a `::` is reachable
    // through `crate::` from anywhere in the crate. A variant imported bare is
    // not — that import is in scope in the one file that wrote it, and sharing
    // those would report a word as a producer in a file where it names nothing.
    let mut named_by_the_member: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for (relative, text) in &sources {
        let Some(member) = the_member_of(relative) else {
            continue;
        };
        let code = without_string_contents(&without_comments(&outside_the_tests(text), "//"));
        let (types, _) = error_code_names(&code);
        let held = named_by_the_member.entry(member).or_default();
        for name in types {
            if !held.contains(&name) {
                held.push(name);
            }
        }
    }

    for (relative, text) in &sources {
        // **And with comments gone.** A code named only in prose counted as a
        // producer, so a comment could keep a declaration true after the last
        // real use went.
        let code = without_string_contents(&without_comments(&outside_the_tests(text), "//"));
        // **A `const fn` that returns a code is not a producer.** The needle is
        // the variant's *name*, and a `pub const fn exhausted() -> ErrorCode`
        // names it — so `NC-IDS-0003` counted as raised while nothing in the
        // tree can detect flake exhaustion, which is Stage 24's pairing walk.
        // Declared honestly as `reserved`, the gate then contradicted the
        // registry. What the needle can tell apart is a code *returned by a
        // function whose whole body is that code* from a code used anywhere
        // else, and the first is a mapping table, not a raise.
        // **The names this file gives the enum, not the one name it is
        // declared under.** A needle list cannot resolve a name, which is the
        // lesson `main.rs`, `release.rs` and `determinism.rs` each record about
        // a different scan; here it was `ErrorCode::` alone, and
        // `use nc_catalog::ErrorCode as Code;` then `Code::PolicyUnreadable`
        // matched nothing at all.
        //
        // **Resolved first, because the exclusion above needs them too.** The
        // blanking ran before this and read two literals of its own, so a
        // mapping table written under any spelling this resolves stopped being
        // excluded while the finder below went on finding it. `use` statements
        // are not what the blanking touches, so reading the names off the
        // unblanked text is the same answer.
        let (mut types, bare) = error_code_names(&code);
        // And every name the rest of this member declares, which `crate::`
        // reaches from here.
        if let Some(held) =
            the_member_of(relative).and_then(|member| named_by_the_member.get(&member))
        {
            for name in held {
                if !types.contains(name) {
                    types.push(name.clone());
                }
            }
        }
        types.sort();
        types.dedup();
        // **The same widened list to the exclusion**, which this file's own
        // paragraph on `without_bare_returns` says must stay in step: a mapping
        // table written under a name the crate declares elsewhere would stop
        // being excluded while the finder below went on finding it.
        let code = without_bare_returns(&code, &types, &bare);
        // **An import is not a producer, and both halves must read that same
        // text.** The bare-variant half below already dropped the `use` lines;
        // the qualified-path loop did not, so `use nc_catalog::ErrorCode::X;`
        // made the import its own producer. Measured, on a refactor of
        // `nc-ids`'s `exhausted()` that is semantics-preserving and clippy
        // clean — importing the variant and returning it bare:
        // `error `NC-IDS-0003` is declared `reserved` and is produced by a
        // crate outside the catalog`, against `ok 29 code(s) …` for the same
        // function one token different. The exemption `without_bare_returns`
        // documents is only worth what this line is worth.
        let code = without_the_use_statements(&code);
        // **And the qualified-path form, `<…ErrorCode>::Variant`.** rustc reads
        // `<nc_catalog::ErrorCode>::LogDropped` as the same path with the same
        // meaning, and the needle ended in `::` — so it matched nothing.
        // Measured end to end in `crates/nc-journal/src/memory.rs`: the reserved
        // `NC-LOG-0002` produced by a public function, `ok 29 code(s) …`
        // unchanged and `check passed`. `use …::*;` is the fourth spelling and
        // is latent only, because clippy denies `enum_glob_use`.
        let needles: Vec<String> = types
            .iter()
            .flat_map(|name| [format!("{name}::"), format!("{name}>::")])
            .collect();
        for needle in &needles {
            for (at, _) in code.match_indices(needle.as_str()) {
                // **A name that merely ends in those letters is another type.**
                // `MyErrorCode::Planted` matched the bare needle, so a code was
                // reported as produced by a crate that names an unrelated enum.
                if code[..at]
                    .chars()
                    .next_back()
                    .is_some_and(|character| character.is_alphanumeric() || character == '_')
                {
                    continue;
                }
                let rest = &code[at + needle.len()..];
                let named: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !named.is_empty() && !found.contains(&named) {
                    found.push(named);
                }
            }
        }
        // And the variants imported bare, which carry no qualifier at their use
        // site at all. Read from the same `use`-free text as the loop above,
        // or the import that names one would be the producer of it.
        for (written, variant) in &bare {
            if names_it_bare(&code, written) && !found.contains(variant) {
                found.push(variant.clone());
            }
        }
    }
    Ok((found, read))
}

/// The workspace member a file under `crates/` or `plugins/` belongs to, as its
/// directory.
fn the_member_of(relative: &str) -> Option<String> {
    inside_a_member(relative).map(|(member, _)| member.to_owned())
}

/// The local names a source file gives `ErrorCode`, and the variants it imports
/// bare.
///
/// **The gate read one spelling, and rustc reads four.** `codes_something_raises`
/// looked for `ErrorCode::` and nothing else, so both halves of Rust's import
/// syntax walked past it. Measured, in `crates/nc-journal/src/memory.rs`:
///
/// ```text
/// use nc_catalog::ErrorCode as Code;   Code::PolicyUnreadable
///   ->  ok 29 code(s) are declared as what the tree does with them, …
///       (the step's baseline, unmoved)
/// use nc_catalog::ErrorCode;           ErrorCode::PolicyUnreadable
///   ->  error `NC-POL-0001` is declared `reserved` and is produced by a crate
///       outside the catalog
/// ```
///
/// `use nc_catalog::ErrorCode::{self, PolicyUnreadable};` and then the bare
/// `PolicyUnreadable` is the second spelling and was equally invisible.
/// `docs/errors.md` is generated from `raising`, so either one publishes a
/// subsystem as raising nothing while `nc-journal` returns its code.
///
/// The answer is the shape `wall.rs`'s `static_aliases` already uses: read the
/// file's own imports and look for the names *it* uses. Two lists come back —
/// the names that may stand in front of a `::`, and the pairs
/// `(written here, the variant it names)` for a variant imported without one,
/// because an import may rename a variant as well as the type.
///
/// The default is always in the first list: a file that imports nothing still
/// spells the enum `ErrorCode` when it writes the path out in full.
fn error_code_names(code: &str) -> (Vec<String>, Vec<(String, String)>) {
    let enum_name = ["Error", "Code"].join("");
    let mut types: Vec<String> = vec![enum_name.clone()];
    let mut bare: Vec<(String, String)> = Vec::new();
    // **`type X = …ErrorCode;`, which is the third way to give it a name.**
    // This pass read `use` statements only, and rustc reads a type alias as the
    // same rename: `type Held = nc_catalog::ErrorCode;` then `Held::DbMigrationFailed`
    // is `ErrorCode::DbMigrationFailed`, and it matched nothing here. Measured, a
    // public documented producer of the reserved `NC-DB-0002` written that way
    // in `crates/nc-canon/src/lib.rs` left the verdict line at its baseline —
    // `ok 29 code(s) are declared as what the tree does with them`, and
    // `check passed`.
    //
    // The right-hand side has to *end* in the enum's own name, so
    // `type Held = Vec<ErrorCode>;` — which names no code by itself — is not a
    // rename of it.
    for statement in type_statements(code) {
        let Some((left, right)) = statement.split_once('=') else {
            continue;
        };
        let named = identifier_at(left.trim_start().strip_prefix("type ").unwrap_or(""));
        let right = right.trim().trim_end_matches(';').trim();
        let Some(before) = right.strip_suffix(enum_name.as_str()) else {
            continue;
        };
        // A name that merely ends in those letters is another type, the same
        // rule the `use` half applies below.
        if before
            .chars()
            .next_back()
            .is_some_and(|character| character.is_alphanumeric() || character == '_')
        {
            continue;
        }
        if !named.is_empty() {
            types.push(named);
        }
    }
    for statement in use_statements(code) {
        let mut rest = statement.as_str();
        while let Some(at) = rest.find(enum_name.as_str()) {
            // A name that merely ends in those letters is another type.
            let starts_here = rest[..at]
                .chars()
                .next_back()
                .is_none_or(|before| !before.is_alphanumeric() && before != '_');
            rest = &rest[at + enum_name.len()..];
            if !starts_here {
                continue;
            }
            read_one_import(rest, &mut types, &mut bare);
        }
    }
    types.sort();
    types.dedup();
    bare.sort();
    bare.dedup();
    (types, bare)
}

/// What follows `ErrorCode` in one import, sorted into the two lists.
///
/// Its own function because [`error_code_names`] is at clippy's line limit with
/// it inline, which is why its neighbours are functions too.
fn read_one_import(rest: &str, types: &mut Vec<String>, bare: &mut Vec<(String, String)>) {
    let Some(after) = rest.strip_prefix("::") else {
        // `use …::ErrorCode as Code;` renames the type; anything else leaves it
        // under the name it is declared with, which is already in the list.
        if let Some(renamed) = rest.strip_prefix(" as ") {
            let name = identifier_at(renamed);
            if !name.is_empty() {
                types.push(name);
            }
        }
        return;
    };
    // `use …::ErrorCode::{self, PolicyUnreadable as Unreadable};` — a group,
    // whose items are the two kinds together.
    // **The rename outside a group was thrown away.** `identifier_at` stops at
    // the space, so `use …::ErrorCode::Planted as Refused;` came back as the
    // pair `(Planted, Planted)` and the local name `Refused` — the only
    // spelling the file actually writes — was in neither list. The test for
    // that spelling passed anyway, because the qualified loop was reading the
    // `use` line itself and finding `Planted` there: the import was the
    // producer. Fixing that loop to read use-free text is what made this
    // visible, which is what a repair that closes one hole is supposed to do.
    let group = after.strip_prefix('{').map_or_else(
        || after.split(';').next().unwrap_or(after).trim().to_owned(),
        |inside| {
            inside
                .split_once('}')
                .map_or(inside, |(held, _)| held)
                .to_owned()
        },
    );
    for item in group.split(',') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let (named, written) = item
            .split_once(" as ")
            .map_or((item, item), |(named, written)| {
                (named.trim(), written.trim())
            });
        let written = identifier_at(written);
        if named == "self" {
            types.push(written);
        } else if !written.is_empty() {
            bare.push((written, identifier_at(named)));
        }
    }
}

/// The identifier `text` opens with, or the empty string.
fn identifier_at(text: &str) -> String {
    text.chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect()
}

/// Every `use` statement in `code`, whole and with its whitespace squeezed.
///
/// The whole statement, because `use nc_catalog::ErrorCode\n    as Code;` is the
/// same import and rustfmt is free to wrap a long one; a bound on how far it
/// will look, because an unterminated `use` is a file that does not compile and
/// not this rule's business. The shape [`crate::release`]'s `the_imports_of`
/// already uses, one visibility simpler: what is wanted here is the path, and a
/// `pub use` carries the same one.
fn use_statements(code: &str) -> Vec<String> {
    statements_opening_with(code, "use ")
}

/// Every `type` alias in `code`, whole and with its whitespace squeezed.
///
/// The other half of [`use_statements`], and the same shape: an alias may wrap
/// and it ends at its semicolon.
fn type_statements(code: &str) -> Vec<String> {
    statements_opening_with(code, "type ")
}

/// A line with any leading visibility taken off it.
///
/// **A re-export is an import, and both readers below thought `pub ` was the
/// whole of a visibility.** `pub(crate) use nc_catalog::ErrorCode as Code;` is
/// an ordinary line to `strip_prefix("pub ")`, so the rename was invisible to
/// the name resolver and the import itself was left in the text the bare-variant
/// scan reads — two errors from one missing spelling. `crate::release`'s
/// `without_a_visibility` records the same defect against its own census, in the
/// same words.
///
/// `pub`, `pub(crate)`, `pub(super)` and `pub(in path)` are all of them.
/// Whitespace is required after the visibility, so an identifier that merely
/// begins with those three letters is left alone.
pub(crate) fn without_a_visibility(line: &str) -> &str {
    let Some(rest) = line.strip_prefix("pub") else {
        return line;
    };
    let rest = if let Some(inside) = rest.strip_prefix('(') {
        match inside.find(')') {
            Some(at) => &inside[at + 1..],
            None => return line,
        }
    } else {
        rest
    };
    if rest.starts_with(char::is_whitespace) {
        rest.trim_start()
    } else {
        line
    }
}

/// Every statement in `code` opening with `keyword`, whole and squeezed.
///
/// **One collector, because there are two keywords and they had one reader.**
/// `use … as X;` was read and `type X = …ErrorCode;` was not, and rustc gives a
/// type the same second name either way — so the two spellings of a rename
/// belong to one pass rather than to a pass and a hole. Splitting the walk out
/// is what makes the second keyword a line rather than a copy of this function.
fn statements_opening_with(code: &str, keyword: &str) -> Vec<String> {
    let lines: Vec<&str> = code.lines().collect();
    let mut found = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        let opens = without_a_visibility(line.trim_start());
        if !opens.starts_with(keyword) {
            continue;
        }
        let mut statement = opens.to_owned();
        let mut next = at + 1;
        while !statement.contains(';') && next < lines.len() && next - at < 20 {
            statement.push(' ');
            statement.push_str(lines[next].trim());
            next += 1;
        }
        found.push(statement.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    found
}

/// `code` with every `use` statement blanked, its newlines kept.
///
/// A variant imported bare is written once in the import that names it, and a
/// scan that counted that line would make every import its own producer.
fn without_the_use_statements(code: &str) -> String {
    let mut out = String::with_capacity(code.len());
    let mut inside = false;
    for line in code.lines() {
        let opens = without_a_visibility(line.trim_start());
        if inside || opens.starts_with("use ") {
            inside = !line.contains(';');
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Whether `code` writes `name` as an identifier of its own.
///
/// A variant used bare carries no qualifier, so the only thing telling it from
/// the tail of a longer name — or from the same word behind a path — is what
/// sits either side of it.
fn names_it_bare(code: &str, name: &str) -> bool {
    let mut at = 0_usize;
    while let Some(found) = code[at..].find(name) {
        let start = at + found;
        at = start + name.len();
        let before = code[..start].chars().next_back();
        let after = code[at..].chars().next();
        if before.is_some_and(|it| it.is_alphanumeric() || it == '_' || it == ':' || it == '.') {
            continue;
        }
        if after.is_some_and(|it| it.is_alphanumeric() || it == '_') {
            continue;
        }
        return true;
    }
    false
}

/// The file with every `#[cfg(test)]` module blanked out.
///
/// A test that names a code proves the mapping compiles. It does not prove
/// anything the platform does, and counting it would make this gate agree with
/// itself.
///
/// **The braces are counted in a text where a literal is not code.** This read
/// the raw file, and `'{'`, `"{"` and `format!("{{")` are braces to a counter
/// that does not know what a string is — so one test that writes a brace and no
/// matching one moves `depth` for ever, `in_a_test` never closes, and every
/// line of the file below that test is blanked. Measured, six lines added to
/// `crates/nc-canon/src/parse.rs`:
///
/// ```text
/// #[cfg(test)]
/// mod brace_probe {
///     #[test]
///     fn a_doubled_brace_is_one_brace() {
///         assert_eq!(format!("{{"), "{");
///     }
/// }
///
///   ->  error `NC-CANON-0002` is declared `raised` and is not produced by a
///       crate outside the catalog
/// ```
///
/// Five producers in that file went dark at once. That direction is loud; the
/// quiet one is the same hole read the other way — a `reserved` number produced
/// below such a test is produced by a region this walk cannot see, and the gate
/// says nothing at all. This repository writes `format!("{{")` and `'}'` all
/// over its own tests, so it is a live shape rather than a contrived one.
///
/// [`blanked`] spends every comment and every literal character for character
/// and keeps the newlines, so line `n` of it is line `n` of the file with the
/// braces that are code and no others. The `#[cfg(test)]` and the closing brace
/// are looked for in that text too: a `//` line naming the attribute used to
/// open a test region, because `without_comments` runs *after* this.
pub(crate) fn outside_the_tests(text: &str) -> String {
    let spent = blanked(text);
    let mut out = String::with_capacity(text.len());
    let mut depth = 0_i32;
    let mut nested = 0_i32;
    let mut in_a_test = false;
    let mut opened = (0_i32, 0_i32);
    for (line, code) in text.lines().zip(spent.lines()) {
        let trimmed = code.trim();
        if !in_a_test && trimmed.starts_with("#[cfg(test)]") {
            in_a_test = true;
            opened = (depth, nested);
        }
        for character in code.chars() {
            match character {
                '{' => depth += 1,
                '}' => depth -= 1,
                '(' | '[' => nested += 1,
                ')' | ']' => nested -= 1,
                _ => {}
            }
        }
        if in_a_test {
            // **An item is not always brace-delimited, and this closed only on
            // a brace.** The attribute governs one item, and `use …;`,
            // `const …;`, `static …;`, `type …;` and `mod x;` are all items
            // that end in a semicolon — so the region stayed open past them and
            // blanked everything down to the *next* item's closing brace, which
            // is shipped code. Measured, `#[cfg(test)] pub const PROBE: u32 = 3;`
            // in `crates/nc-journal/src/memory.rs` with a documented public
            // producer of the reserved `NC-POL-0001` under it:
            //
            // ```text
            // ok    29 code(s) are declared as what the tree does with them, …
            // ```
            //
            // — the producer invisible and every number unmoved. The control,
            // the same producer with those two lines gone, is
            // `error `NC-POL-0001` is declared `reserved` and is produced by a
            // crate outside the catalog`. The other direction is the one the
            // paragraph above this function names: a `raised` code whose only
            // producer sits under such a line goes dark and the gate
            // contradicts the registry.
            //
            // A semicolon at the depth the region opened at is that item
            // ending. Inside a brace-delimited one every line sits at least one
            // deeper, so a statement in a test module cannot close it.
            //
            // **And a comma**, one mark over: a match arm and a struct
            // literal's field end in one, and the attribute governs that arm or
            // that field. Closing only on `}` or `;`, the region ran to the
            // enclosing brace, and a reviewer hid behind a `#[cfg(test)]` arm a
            // production arm that dropped a trigger — the local SQL gate and the
            // raising rule both went blind there. The brackets are counted too,
            // so a comma inside a parameter list or an argument list, which
            // ends no item, closes nothing.
            let (at, within) = opened;
            let ends = trimmed.ends_with('}') || trimmed.ends_with(';') || trimmed.ends_with(',');
            if depth <= at && nested <= within && ends {
                in_a_test = false;
            }
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Whether a source file compiles a query against a live database.
///
/// The runtime forms — `sqlx::query`, `query_as`, `query_scalar` without the
/// `!` — are what this repository uses everywhere and are not findings; the
/// exclamation mark is the whole difference, and it is what each needle ends
/// with.
pub(crate) fn no_compile_time_query(file: &str, text: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let stripped = without_comments(text, "//");
    // **The renamed call, not the import.** `use sqlx::query as sql;` then
    // `sql!("SELECT 1")` names nothing on the list — the lesson
    // `determinism.rs` already learned about `process::Command`.
    //
    // What is flagged is the **call**, and that is the whole reason this is two
    // passes: `use sqlx::query` imports the runtime function *and* the macro,
    // which live in different namespaces and share a name, so reporting the
    // `use` would fire on every caller who renamed the ordinary function. A
    // `sql!(` — with the bang — is the macro and nothing else.
    // **The whole `use`, not the first three words of a line.** This read
    // `rest.split_whitespace()` and required `X`, `as`, `Y` in that order, which
    // is one spelling of an import out of several cargo and rustc accept.
    // `use sqlx::{query as sql};` — the braced form, and the more idiomatic one
    // — makes the first word `{query`, matching nothing, so `sql!("SELECT 1")`
    // was not a finding. Measured, the same file two ways:
    //
    // ```text
    // use sqlx::query as sql;    -> `sqlx::sql!` checks its SQL at build time
    // use sqlx::{query as sql};  -> no finding
    // ```
    //
    // A `use` ends at its semicolon and may span lines, so the statement is
    // what is read, and every identifier in it is a word — braces, commas and
    // newlines are separators like any other. That answers for the nested tree
    // and the multi-line form together, rather than for one shape at a time,
    // which is the mistake three other lists in this tool made first.
    //
    // A module rename needs nothing here: `use sqlx as db;` then `db::query!(…)`
    // still spells `query!`, and the needles below match it.
    let mut renamed: Vec<String> = Vec::new();
    let mut rest = stripped.as_str();
    // Every `use`, then whether it is about `sqlx` — rather than `use sqlx::`,
    // which `use ::sqlx::{…}` does not spell.
    while let Some((_, after)) = rest.split_once("use ") {
        let statement = after.split_once(';').map_or(after, |(head, _)| head);
        if !statement.contains("sqlx") {
            rest = &after[statement.len()..];
            continue;
        }
        let words: Vec<&str> = statement
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .filter(|word| !word.is_empty())
            .collect();
        for triple in words.windows(3) {
            if triple[1] == "as"
                && COMPILE_TIME_QUERIES
                    .iter()
                    .any(|needle| needle.trim_end_matches('!') == triple[0])
            {
                renamed.push(format!("{}!", triple[2]));
            }
        }
        rest = &after[statement.len()..];
    }
    for (index, line) in stripped.lines().enumerate() {
        let code = line;
        let looking_for: Vec<&str> = COMPILE_TIME_QUERIES
            .iter()
            .copied()
            .chain(renamed.iter().map(String::as_str))
            .collect();
        for needle in &looking_for {
            // `query!` is the tail of `query_as!` too, so the longest match is
            // the one reported — otherwise one line is two findings under two
            // names, which is the shape `determinism.rs` fixed for its own list.
            let named = looking_for
                .iter()
                .filter(|other| code.contains(**other))
                .max_by_key(|other| other.len());
            if !code.contains(needle) || named != Some(needle) {
                continue;
            }
            findings.push(Finding {
                file: file.to_owned(),
                line: index + 1,
                what: format!("`sqlx::{needle}` checks its SQL at build time"),
                fix: "that macro connects to the database *while compiling*, so this \
                      crate would need `DATABASE_URL` to build and a `.sqlx` cache to \
                      build without one — and `cargo xtask check --offline` could not \
                      compile at all. `Cargo.toml` promises neither exists. Use the \
                      runtime form, `sqlx::query(…)`, which every query here already does"
                    .to_owned(),
            });
        }
    }
    findings
}

#[cfg(test)]
mod query_tests {
    use super::{COMPILE_TIME_QUERIES, no_compile_time_query};

    #[test]
    fn prose_does_not_read_a_setting() {
        // **Four ways prose satisfied this gate**, each measured against a
        // scratch tree that gives one finding as a control:
        //
        // | what "read" the variable | findings |
        // |---|---|
        // | a Rust `/* … */` block comment | 0 |
        // | a Rust `/** … */` doc comment  | 0 |
        // | a `description = "…"` in a TOML file | 0 |
        // | `NC_PROBE` declared, `NC_PROBE_SOMETHING_ELSE` in code | 0 |
        //
        // The first two are why the stripper follows block comments; the third
        // is why TOML is not a file that reads a variable; the fourth is why the
        // match is on a whole name.
        let guard = crate::check::LeavesNothingBehind::new("scratch");
        let scratch = guard.root.clone();
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("crates/probe/src")).expect("writable");
        std::fs::write(scratch.join(".env.example"), "NC_PROBE=1\n").expect("writable");

        let findings = |source: &str, name: &str| {
            let path = scratch.join("crates/probe/src").join(name);
            for stale in ["lib.rs", "notes.toml", "probe.sql"] {
                let _ = std::fs::remove_file(scratch.join("crates/probe/src").join(stale));
            }
            std::fs::write(&path, source).expect("writable");
            super::no_unread_variable(&scratch)
                .expect("the tree is walkable")
                .0
                .len()
        };

        // The control: nothing reads it.
        assert_eq!(findings("fn main() {}\n", "lib.rs"), 1);
        // Prose, four ways.
        assert_eq!(findings("/* NC_PROBE is prose here. */\n", "lib.rs"), 1);
        assert_eq!(findings("/** NC_PROBE is prose here. */\n", "lib.rs"), 1);
        assert_eq!(findings("// NC_PROBE is prose here.\n", "lib.rs"), 1);
        // **SQL's comment is `--`**, and everything that was not Rust got `#` —
        // so a line of prose in a migration counted as a read.
        assert_eq!(
            findings(
                "-- NC_PROBE is prose here; this line is a comment.\n",
                "probe.sql"
            ),
            1
        );
        // And a real read in SQL is still a read.
        assert_eq!(findings("SELECT 'NC_PROBE';\n", "probe.sql"), 0);
        assert_eq!(
            findings("description = \"NC_PROBE is prose here\"\n", "notes.toml"),
            1
        );
        // A longer name that begins the same way is a different variable.
        assert_eq!(findings("let x = env(\"NC_PROBE_ELSE\");\n", "lib.rs"), 1);
        // And a real read, or every assertion above passes by the gate
        // reporting everything.
        assert_eq!(findings("let x = env(\"NC_PROBE\");\n", "lib.rs"), 0);

        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn every_macro_that_needs_a_database_is_listed() {
        // **The twelve, written out.** The stems below ask for eight of them
        // and the driver beside this test loops over the constant itself, so
        // four entries — the `query_file_as` and `query_file_scalar` pairs —
        // were held by the length alone: measured in round sixty-five, one
        // character changed in `query_file_as!` left `check passed`, and that
        // macro then walked through the gate that exists to stop a build-time
        // `DATABASE_URL`.
        assert_eq!(
            COMPILE_TIME_QUERIES,
            [
                concat!("query", "!"),
                concat!("query_as", "!"),
                concat!("query_scalar", "!"),
                concat!("query_file", "!"),
                concat!("query_file_as", "!"),
                concat!("query_file_scalar", "!"),
                concat!("query_unchecked", "!"),
                concat!("query_as_unchecked", "!"),
                concat!("query_scalar_unchecked", "!"),
                concat!("query_file_unchecked", "!"),
                concat!("query_file_as_unchecked", "!"),
                concat!("query_file_scalar_unchecked", "!"),
            ],
            "the macros that need a database at build time moved"
        );
        // **The list had no membership assertion**, so deleting an entry only
        // shortened the loop below and every test stayed green — the shape
        // `determinism.rs`'s own needle list was caught with. A macro added to
        // the family needs adding here and in the list, which is the two-place
        // edit that makes a removal visible.
        //
        // **And every one of them twice**, because `sqlx` ships each with an
        // `_unchecked` sibling and the list named six of the twelve. Both forms
        // want `DATABASE_URL` at build time — `_unchecked` skips the *type*
        // check and keeps the connection — so the half that was free is the half
        // a developer reaches for when the checked form will not compile.
        for needle in ["query", "query_as", "query_scalar", "query_file"] {
            for form in [needle.to_owned(), format!("{needle}_unchecked")] {
                assert!(
                    COMPILE_TIME_QUERIES
                        .iter()
                        .any(|listed| listed.starts_with(form.as_str())),
                    "`sqlx::{form}!` is not refused, and it wants a database at build time"
                );
            }
        }
        assert_eq!(COMPILE_TIME_QUERIES.len(), 12);
    }

    #[test]
    fn a_compile_time_query_is_a_finding_and_a_runtime_one_is_not() {
        for needle in COMPILE_TIME_QUERIES {
            let line = format!("    let n = sqlx::{needle}(\"SELECT 1\").fetch_one(&pool);\n");
            let findings = no_compile_time_query("src/x.rs", &line);
            assert_eq!(findings.len(), 1, "{needle}: {findings:#?}");
            assert!(
                findings[0].what.contains(needle),
                "the finding does not name what it refused: {}",
                findings[0].what
            );
        }
        // **The runtime forms**, which is every query in this repository. A gate
        // that refused these would be switched off within a day.
        for ordinary in [
            "    let row = sqlx::query(&statement).fetch_one(&self.pool).await?;\n",
            "    let n: bool = sqlx::query_scalar(SQL).bind(x).fetch_one(&pool).await?;\n",
            "    sqlx::migrate!(\"../../migrations\").run(&pool).await\n",
        ] {
            assert!(
                no_compile_time_query("src/x.rs", ordinary).is_empty(),
                "an ordinary runtime query was a finding: {ordinary}"
            );
        }
        // Prose about the rule is not a use of it, which is what a comment in
        // this file and in `Cargo.toml` both are. Built from halves, because a
        // `//` *inside a string* is correctly not a comment — so spelling the
        // needle here would be this test reporting itself.
        let prose = format!("    // never sqlx::{} here\n", COMPILE_TIME_QUERIES[0]);
        assert_eq!(
            no_compile_time_query("src/x.rs", &prose),
            [] as [crate::gates::Finding; 0]
        );
        // And the string case that made the first version blind: a `//` inside
        // a literal does not end the code, so a URL no longer hides the rest of
        // the line.
        let after_a_url = format!(
            "    let e = \"http://127.0.0.1:9428\"; let n = sqlx::{}(\"SELECT 1\");\n",
            COMPILE_TIME_QUERIES[0]
        );
        assert_eq!(no_compile_time_query("src/x.rs", &after_a_url).len(), 1);

        // **And the same URL inside a raw string**, which is the mirror of it:
        // `r#"…"#` holds bare quotes, so counting quotes left the scan outside a
        // string when it met the `//` of `https://` and dropped the rest of the
        // line — including the query — as a comment. Measured: 1 finding on the
        // plain line, **0** with the raw string in front of it.
        let after_a_raw_url = format!(
            "    let note = r#\"see \"https://example.invalid\" for details\"#; \
             let n = sqlx::{}(\"SELECT 1\");\n",
            COMPILE_TIME_QUERIES[0]
        );
        assert_eq!(
            no_compile_time_query("src/x.rs", &after_a_raw_url).len(),
            1,
            "a raw string holding a URL hid the rest of the line"
        );
        // A raw string with more hashes, and one that really does end.
        let ended = format!(
            "    let a = r##\"x\"# still inside\"##; let n = sqlx::{}(\"SELECT 1\");\n",
            COMPILE_TIME_QUERIES[0]
        );
        assert_eq!(no_compile_time_query("src/x.rs", &ended).len(), 1);
        // **A needle inside a string literal is still reported**, raw or not,
        // and that is deliberate rather than an oversight: a gate that ignored
        // string contents would be defeated by anything that builds source, and
        // a macro name inside a string is odd enough to be worth a look. What
        // the raw-string handling buys is not silence about the string — it is
        // that the string does not swallow the *rest of the line*.
        let prose_in_a_raw = format!(
            "    let a = r#\"// sqlx::{} in prose\"#;\n",
            COMPILE_TIME_QUERIES[0]
        );
        assert_eq!(no_compile_time_query("src/x.rs", &prose_in_a_raw).len(), 1);
        let prose_in_a_plain_string = format!(
            "    let a = \"sqlx::{} in prose\";\n",
            COMPILE_TIME_QUERIES[0]
        );
        assert_eq!(
            no_compile_time_query("src/x.rs", &prose_in_a_plain_string).len(),
            1,
            "the two string kinds have to behave the same"
        );
        // **And a renamed call.** `use sqlx::query as sql;` then `sql!(…)` names
        // nothing on the list — the lesson `determinism.rs` learned about
        // `process::Command`.
        let bare = COMPILE_TIME_QUERIES[0].trim_end_matches('!');
        let aliased = format!("use sqlx::{bare} as sql;\n    let n = sql!(\"SELECT 1\");\n");
        let findings = no_compile_time_query("src/x.rs", &aliased);
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert_eq!(findings[0].line, 2, "the finding points at the call");

        // **The import alone is not a finding**, and that is deliberate rather
        // than a gap: `use sqlx::query` brings in the runtime function *and* the
        // macro — different namespaces, one name — so reporting the `use` would
        // fire on every caller who renamed the ordinary function. The bang is
        // what distinguishes them, and the bang is at the call.
        assert_eq!(
            no_compile_time_query("src/x.rs", &format!("use sqlx::{bare} as sql;\n")),
            [] as [crate::gates::Finding; 0]
        );
        // And a rename of something else does not make its calls findings.
        assert_eq!(
            no_compile_time_query("src/x.rs", "use sqlx::Executor as sql;\n    sql!(1);\n"),
            [] as [crate::gates::Finding; 0]
        );

        // **Every shape a `use` comes in, not the one written above.** The
        // reader took the first three whitespace-separated words of the line, so
        // the braced form — the more idiomatic of the two — made the first word
        // `{query` and matched nothing:
        //
        // ```text
        // use sqlx::query as sql;    -> `sqlx::sql!` checks its SQL at build time
        // use sqlx::{query as sql};  -> no finding
        // ```
        //
        // A `use` ends at its semicolon and may span lines, and a nested tree
        // puts the rename several separators in. This is the fourth list of
        // spellings in this tool to be found short, so what is asked now is the
        // statement rather than a shape of it.
        let second = COMPILE_TIME_QUERIES[1].trim_end_matches('!');
        for import in [
            format!("use sqlx::{{{bare} as sql}};"),
            format!("use sqlx::{{self, {bare} as sql}};"),
            format!("use sqlx::{{{second} as other, {bare} as sql}};"),
            format!("use sqlx::{{\n    {bare} as sql,\n}};"),
            format!("use sqlx::{{Executor, {bare} as sql, Row}};"),
            format!("use ::sqlx::{{{bare} as sql}};"),
        ] {
            let source = format!("{import}\nfn f() {{ let n = sql!(\"SELECT 1\"); }}\n");
            let findings = no_compile_time_query("src/x.rs", &source);
            assert_eq!(findings.len(), 1, "this import hid the call:\n{source}");
        }

        // And the brace form of an unrelated rename is still not a finding, so
        // the loosened reader did not become a reader of everything.
        assert_eq!(
            no_compile_time_query(
                "src/x.rs",
                "use sqlx::{Executor as sql, Row};\n    sql!(1);\n"
            ),
            [] as [crate::gates::Finding; 0]
        );
    }
}

/// One file's statements told to `sqlx` that they are safe, and what they are built from.
///
/// **`sqlx` 0.9 runs no text built at run time through `query` until somebody
/// vouches for it.** Its `query`, `query_as`, `query_scalar` and `raw_sql`, and
/// an `Executor` handed a string, take `impl SqlSafeStr`, which only a
/// `&'static str` and `sqlx::AssertSqlSafe(..)` implement, so a statement with a
/// schema, a role or a table name `format!`ted into it does not compile until it
/// is wrapped. The wrap is a claim — *nothing in this text can come from outside
/// this repository* — and the compiler takes it on trust, so a reviewer has to
/// make it once per statement and this table is where each file's answer is
/// written: how many statements it vouches for, and every value their text is
/// built from.
///
/// **What the compiler leaves open, and how each is held.** Read off the public
/// functions of `sqlx-core`, `sqlx-postgres` and `sqlx-sqlite` 0.9 that take a
/// `&str` or a `Cow<'static, str>`, which is a reading and not a proof that
/// there is no other:
///
/// - `QueryBuilder`, whose `new` and `push` wrap whatever they are handed inside
///   `sqlx`: refused wherever it is named, by [`builders_named`].
/// - `copy_in_raw` and `copy_out_raw`, on a connection and on a pool through
///   `PgPoolCopyExt`, which take the whole `COPY` statement as a plain `&str`:
///   read the way a wrap is, by [`copied_statements`], so one built from values
///   is a statement this table counts and names the values of, and one written
///   as a literal needs no row.
/// - `SqliteConnectOptions::pragma`, whose key and value are any
///   `Into<Cow<'static, str>>` — a `String` built at run time among them —
///   and which `sqlx-sqlite` writes unquoted into `PRAGMA {key} = {value};`
///   and runs as one batch on every connection it opens, vouching for the
///   text itself: read the way a copy is, by [`pragmas_set`], so a pragma set
///   from values is a statement this table counts and names the values of,
///   and one set from literals needs no row. **Found by a reviewer**: a value
///   built from the environment, in the shipped `crates/nc-local/src/open.rs`,
///   passed every gate, and a probe showed text handed to it as a value run as
///   a statement of its own.
/// - A migrator built when it runs — `Migrator::new` over a path,
///   `with_migrations`, any `MigrationSource`, `ResolveWith` and
///   `resolve_blocking` — which reads `.sql` files then and wraps their text in
///   `AssertSqlSafe` inside `sqlx`: refused by name in the sources that open the
///   local database, by `crate::local`'s source gate, where the one migrator is
///   the `sqlx::migrate!` the crate embeds when it compiles. **Found by a
///   reviewer**: the shipped store migrating from a relative path passed every
///   gate, each suite running from the package's root, where that path is this
///   repository's own directory. The gates of this binary migrate scratch files
///   from this repository's directories, which no user's file is.
/// - A `String` leaked into a `&'static str`, which is a `&'static str`:
///   nothing here tells it from a literal.
/// - The migrator's own table and schema names — `Migrator`'s
///   `dangerous_set_table_name` and `create_schema`, and the public fields
///   behind them — which it writes into its statements unquoted: nothing here
///   holds them, and every migrator in this tree is `sqlx::migrate!` or
///   `Migrator::new` over a directory of this repository, neither of which sets
///   one.
#[derive(Debug)]
pub(crate) struct Asserted {
    /// Repository-relative, forward slashes.
    pub(crate) file: &'static str,
    /// How many statements the file vouches for: each `AssertSqlSafe` call, and
    /// each `COPY` statement and each pragma it builds from values.
    pub(crate) calls: usize,
    /// Every value those calls build their text from, sorted.
    pub(crate) from: &'static [&'static str],
}

/// Every statement this tree vouches for, by file and by what it is built from.
///
/// **Each row was read, not counted.** A value is here because its provenance
/// was followed to one of three places — a constant of this repository, a name
/// the catalog or the configuration supplies after [`crate::database::quoted_identifier`]
/// or `nc-prune`'s `identifier` has quoted it, or a name a test built for
/// itself — and the comment above each row says which. What the gate does is
/// hold the tree to the rows in both directions, so a new wrap, a wrap taken
/// away, and an old wrap built from a new value are each a finding naming the
/// file; what it **cannot** do is follow a value to where it came from, which
/// is the reading a row records and no walk makes.
pub(crate) const ASSERTED_STATEMENTS: [Asserted; 8] = [
    // `query` is `COLUMNS`, a constant of the same file, with a `FROM` and an
    // `ORDER BY` written beside it; nothing a caller passes reaches the text.
    Asserted {
        file: "crates/nc-journal/src/postgres.rs",
        calls: 2,
        from: &["query"],
    },
    // Schema and role names the suite builds from its own constants and the
    // process id, quoted where they are written; column names, expressions,
    // sequence numbers and statement lists the tests write out themselves.
    Asserted {
        file: "crates/nc-journal/tests/in_postgres.rs",
        calls: 24,
        from: &[
            "at",
            "column",
            "expression",
            "fresh",
            "name",
            "named",
            "naming",
            "role",
            "schema",
            "scope",
            "statement",
        ],
    },
    // `table`, `order_by` and `key` have each been through `identifier`, which
    // refuses anything but `[a-z_][a-z0-9_]*` in at most two parts and then
    // quotes every part; `only` is a `&'static str` `only_for` chose.
    Asserted {
        file: "crates/nc-prune/src/postgres.rs",
        calls: 2,
        from: &["key", "only", "order_by", "table"],
    },
    // `quoted`, `ordered` and `key` have each been through this file's
    // `identifier`, which refuses anything but `[a-z_][a-z0-9_]*` in one part
    // and then quotes it; `key` is the primary key's columns, each read from
    // `pragma_table_info` and put through the same function. `unheld` is the
    // `NOT EXISTS` of each table holding the table's rows by a `RESTRICT`
    // reference, every name in it read from `pragma_foreign_key_list` and put
    // through `identifier` too, and `key` again.
    Asserted {
        file: "crates/nc-prune/src/sqlite.rs",
        calls: 2,
        from: &["key", "ordered", "quoted", "unheld"],
    },
    // The decision point's schemas, one per test, each named from a literal,
    // the process id and the test's own word, dropped and created in one loop
    // over the two statements.
    Asserted {
        file: "crates/nc-kernel/tests/the_decision_point.rs",
        calls: 1,
        from: &["statement"],
    },
    // The suite's own schemas, roles and tables, named from its constants and
    // the process id; `action` is one of the `ON DELETE` actions it lists.
    Asserted {
        file: "crates/nc-prune/tests/in_postgres.rs",
        calls: 25,
        from: &[
            "action",
            "awkward",
            "child",
            "named",
            "plain",
            "schema",
            "statement",
            "table",
        ],
    },
    // `query` is `asking_about_the_protected_tables` over a constant; `named`
    // is the name PostgreSQL's own `regclass` printed; `path_statement` quotes
    // the schema `DATABASE_URL` names; the rest are the tests' scratch schemas
    // and roles, through `quoted_identifier`.
    Asserted {
        file: "xtask/src/database.rs",
        calls: 19,
        from: &[
            "grant",
            "named",
            "path_statement",
            "query",
            "quoted",
            "schema",
            "stale",
            "statement",
        ],
    },
    // `pinned` is `path_statement` over the schema `DATABASE_URL` names, quoted.
    Asserted {
        file: "xtask/src/dev.rs",
        calls: 1,
        from: &["pinned"],
    },
];

/// The wrapper's name, built from halves so that no line here reads as a call to it.
const ASSERT_SQL_SAFE: &str = concat!("AssertSql", "Safe");

/// The statement builder's name, built from halves for the same reason.
const QUERY_BUILDER: &str = concat!("Query", "Builder");

/// The two `COPY` doors' names, built from halves for the same reason.
const COPY_DOORS: [&str; 2] = [concat!("copy_in", "_raw"), concat!("copy_out", "_raw")];

/// The connection options' pragma door's name.
const PRAGMA_DOOR: &str = "pragma";

/// Each line whose code names `sqlx`'s statement builder.
///
/// **The builder vouches for its own text, where no row can read it.** Its
/// `new` and its `push` take any `Display` and wrap it in the wrapper inside
/// `sqlx`, so a statement `format!`ted into one compiles with no call in this
/// tree for [`ASSERTED_STATEMENTS`] to hold. Measured against `sqlx` 0.9 over
/// this lockfile, in a scratch crate: `new(format!("DELETE FROM {table}"))`, a
/// `push` of a second `format!` and `.build().execute(pool)` compiled — and,
/// appended to a file that has a row, left every test of this gate green.
/// Nothing in this tree builds a statement that way, so a mention is a finding
/// rather than a row: the builder's `push` is a method name any type may have,
/// and what it is handed cannot be told from text.
fn builders_named(text: &str) -> Vec<usize> {
    let code: Vec<char> = blanked(text).chars().collect();
    let needle: Vec<char> = QUERY_BUILDER.chars().collect();
    let word = |at: usize| {
        code.get(at)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
    };
    (0..code.len().saturating_sub(needle.len() - 1))
        .filter(|&at| {
            code[at..at + needle.len()] == needle[..]
                && (at == 0 || !word(at - 1))
                && !word(at + needle.len())
        })
        .map(|at| code[..at].iter().filter(|c| **c == '\n').count() + 1)
        .collect()
}

/// Each place a file names the wrapper: its line, and what the call is built from.
///
/// `None` in place of the names is a mention the gate cannot read as a call —
/// the constructor passed as a value, or a `use` that renames it — and it is
/// reported rather than skipped, because either one hides the statements it
/// vouches for from every row above.
///
/// What a call is built from is every value its argument names in code — not
/// a macro, not a path qualifier, not a method after a `.` — and every
/// name a string inside it interpolates as `{name}`.
fn asserted_calls(text: &str) -> Vec<(usize, Option<Vec<String>>)> {
    let original: Vec<char> = text.chars().collect();
    let code: Vec<char> = blanked(text).chars().collect();
    let needle: Vec<char> = ASSERT_SQL_SAFE.chars().collect();
    let word = |character: char| character.is_alphanumeric() || character == '_';
    let mut calls = Vec::new();
    let mut index = 0;
    while index + needle.len() <= code.len() {
        let at = index;
        index += 1;
        if code[at..at + needle.len()] != needle[..]
            || (at > 0 && word(code[at - 1]))
            || code.get(at + needle.len()).copied().is_some_and(word)
        {
            continue;
        }
        let line = original[..at].iter().filter(|c| **c == '\n').count() + 1;
        let after = at + needle.len();
        let next: String = code[after..].iter().take(4).collect();
        let renamed = next.trim_start().starts_with("as ");
        // An import that keeps the name is not a call and hides nothing: the
        // calls after it still spell the name.
        let statement_start = code[..at]
            .iter()
            .rposition(|c| *c == ';' || *c == '}')
            .map_or(0, |end| end + 1);
        let statement: String = code[statement_start..at].iter().collect();
        let statement = statement.trim_start();
        let imported = statement.starts_with("use ") || statement.contains(" use ");
        if imported && !renamed {
            continue;
        }
        if code.get(after) != Some(&'(') || renamed {
            calls.push((line, None));
            continue;
        }
        let mut depth = 0_usize;
        let mut close = after;
        for (offset, character) in code[after..].iter().enumerate() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = after + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        calls.push((
            line,
            Some(built_from(
                &original[after + 1..close],
                &code[after + 1..close],
            )),
        ));
    }
    calls
}

/// Each `COPY` statement a file builds from values: its line, what it is built
/// from, and which door it went through.
///
/// **The door the wrapper does not guard.** `sqlx-postgres` 0.9's `copy_in_raw`
/// and `copy_out_raw` take a plain `&str` rather than `impl SqlSafeStr`, so a
/// statement `format!`ted into one compiles with no wrap written anywhere.
/// Measured by a reviewer: `PgPoolCopyExt::copy_in_raw(&pool,
/// &format!("COPY {table} (id, at) FROM STDIN"))`, planted in a test of a file
/// with a row, ran against PostgreSQL while every walk here said nothing.
///
/// So a copy is read the way a wrap is. Its statement is its **last** argument —
/// a pool's copy called as `PgPoolCopyExt::copy_in_raw(&pool, ..)` takes the pool
/// first — and what that argument names is what it is built from. One that names
/// nothing, a string literal of any kind, is a `&'static str` by another route and
/// is not reported, which is how `crates/nc-journal/tests/in_postgres.rs` calls
/// it. `None` in place of the names is a mention that is not a call — the method
/// passed as a value — reported for the reason the wrapper's is. The door comes
/// back owned: the log wall refuses a function that hands back a `&'static str`
/// it was not given, whichever one it is.
fn copied_statements(text: &str) -> Vec<(usize, Option<Vec<String>>, String)> {
    let original: Vec<char> = text.chars().collect();
    let code: Vec<char> = blanked(text).chars().collect();
    let word = |character: char| character.is_alphanumeric() || character == '_';
    let mut copies = Vec::new();
    for door in COPY_DOORS {
        let needle: Vec<char> = door.chars().collect();
        for at in 0..code.len().saturating_sub(needle.len() - 1) {
            if code[at..at + needle.len()] != needle[..]
                || (at > 0 && word(code[at - 1]))
                || code.get(at + needle.len()).copied().is_some_and(word)
            {
                continue;
            }
            let line = original[..at].iter().filter(|c| **c == '\n').count() + 1;
            let open = code[at + needle.len()..]
                .iter()
                .position(|c| !c.is_whitespace())
                .map(|skipped| at + needle.len() + skipped)
                .filter(|open| code[*open] == '(');
            let Some(open) = open else {
                copies.push((line, None, door.to_owned()));
                continue;
            };
            // Every argument, split where no bracket but the call's own is open.
            let mut arguments = Vec::new();
            let mut depth = 0_usize;
            let mut start = open + 1;
            for (index, character) in code.iter().enumerate().skip(open) {
                match character {
                    '(' | '[' | '{' => depth += 1,
                    ')' | ']' | '}' => {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            arguments.push((start, index));
                            break;
                        }
                    }
                    ',' if depth == 1 => {
                        arguments.push((start, index));
                        start = index + 1;
                    }
                    _ => {}
                }
            }
            // The last that holds anything: a trailing comma leaves an empty one.
            let Some(&(from, to)) = arguments
                .iter()
                .rev()
                .find(|(from, to)| code[*from..*to].iter().any(|c| !c.is_whitespace()))
            else {
                copies.push((line, None, door.to_owned()));
                continue;
            };
            let names = built_from(&original[from..to], &code[from..to]);
            if !names.is_empty() {
                copies.push((line, Some(names), door.to_owned()));
            }
        }
    }
    copies.sort_unstable();
    copies
}

/// Each pragma a file sets from values: its line, what it is built from, and
/// the door it went through.
///
/// **`pragma` called as a method or as a path, read as a copy is**: what its
/// key and its value name is what the batch `sqlx-sqlite` runs is built from —
/// every argument of `.pragma(key, value)`, and the last two of
/// `SqliteConnectOptions::pragma(options, key, value)`, whose first is the
/// options it is called on — and a call whose key and value are literals names
/// nothing and is not reported, which is how every call in this tree is
/// written. One that is not a call this can read — named after a `.` or a
/// `::` and never called with brackets, the door passed as a value — is
/// reported rather than skipped. **A reviewer found the path call read as no
/// call at all**: `SqliteConnectOptions::pragma(options, "cache_size",
/// std::env::var(..))`, planted in the store's shipped options, met no
/// finding, nor did the door bound to a name and called through it.
fn pragmas_set(text: &str) -> Vec<(usize, Option<Vec<String>>, String)> {
    let original: Vec<char> = text.chars().collect();
    let code: Vec<char> = blanked(text).chars().collect();
    let word = |character: char| character.is_alphanumeric() || character == '_';
    let needle: Vec<char> = PRAGMA_DOOR.chars().collect();
    let mut set = Vec::new();
    for at in 0..code.len().saturating_sub(needle.len() - 1) {
        // **The name before what precedes it**, because what precedes it is a
        // walk back over whitespace: asked at every position, a file whose
        // comments blank to one long run of spaces cost the square of its
        // length, and the planted fixture's every-directory tree did not finish
        // in twenty minutes.
        if code[at..at + needle.len()] != needle[..]
            || code.get(at + needle.len()).copied().is_some_and(word)
        {
            continue;
        }
        let method = code[..at].iter().rev().find(|c| !c.is_whitespace()) == Some(&'.');
        let path = at >= 2 && code[at - 2..at] == [':', ':'];
        if !(method || path) {
            continue;
        }
        let line = original[..at].iter().filter(|c| **c == '\n').count() + 1;
        let open = code[at + needle.len()..]
            .iter()
            .position(|c| !c.is_whitespace())
            .map(|skipped| at + needle.len() + skipped)
            .filter(|open| code[*open] == '(');
        let Some(open) = open else {
            set.push((line, None, PRAGMA_DOOR.to_owned()));
            continue;
        };
        // Every argument, split where no bracket but the call's own is open.
        let mut arguments = Vec::new();
        let mut depth = 0_usize;
        let mut start = open + 1;
        for (index, character) in code.iter().enumerate().skip(open) {
            match character {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        arguments.push((start, index));
                        break;
                    }
                }
                ',' if depth == 1 => {
                    arguments.push((start, index));
                    start = index + 1;
                }
                _ => {}
            }
        }
        arguments.retain(|(from, to)| code[*from..*to].iter().any(|c| !c.is_whitespace()));
        let judged = if path {
            arguments.len().saturating_sub(2)
        } else {
            0
        };
        let names: Vec<String> = arguments[judged..]
            .iter()
            .flat_map(|(from, to)| built_from(&original[*from..*to], &code[*from..*to]))
            .collect::<std::collections::BTreeSet<String>>()
            .into_iter()
            .collect();
        if !names.is_empty() {
            set.push((line, Some(names), PRAGMA_DOOR.to_owned()));
        }
    }
    set
}

/// The values one argument names, from its text and the same text blanked.
fn built_from(original: &[char], code: &[char]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    let mut index = 0;
    while index < code.len() {
        let starts = code[index].is_alphabetic() || code[index] == '_';
        if !starts || (index > 0 && (code[index - 1].is_alphanumeric() || code[index - 1] == '_')) {
            index += 1;
            continue;
        }
        let end = code[index..]
            .iter()
            .position(|c| !(c.is_alphanumeric() || *c == '_'))
            .map_or(code.len(), |length| index + length);
        let name: String = code[index..end].iter().collect();
        let rest: String = code[end..].iter().collect();
        let rest = rest.trim_start();
        let method = code[..index].iter().rev().find(|c| !c.is_whitespace()) == Some(&'.');
        // A literal's prefix — `r"…"`, `br#"…"#` — is the literal, not a value;
        // blanking spends a raw string's hashes, so a space may stand between.
        let prefix = rest.starts_with('"') && ["r", "b", "br", "c", "cr"].contains(&name.as_str());
        if !method && !prefix && !rest.starts_with('!') && !rest.starts_with("::") {
            names.insert(name);
        }
        index = end;
    }
    // A placeholder is a brace the blanking took away: inside a string, or in
    // a comment beside one, and read either way.
    let mut index = 0;
    while index < original.len() {
        let hidden = original[index] == '{' && code[index] != '{';
        if !hidden || original.get(index + 1) == Some(&'{') {
            index += if hidden { 2 } else { 1 };
            continue;
        }
        let name: String = original[index + 1..]
            .iter()
            .take_while(|c| c.is_alphanumeric() || **c == '_')
            .collect();
        let closes = original.get(index + 1 + name.chars().count());
        if name.starts_with(|c: char| c.is_alphabetic() || c == '_')
            && (closes == Some(&'}') || closes == Some(&':'))
        {
            names.insert(name);
        }
        index += 1;
    }
    names.into_iter().collect()
}

/// What to do about a statement nobody has read, which is every finding of the walk below.
const UNREAD_STATEMENT: &str = "read what the statement is built from and follow each \
     value to where it is made: a constant of this repository, a name the catalog or the \
     configuration supplies that `quoted_identifier` or `identifier` has quoted, or a name \
     a test built for itself. Then write the file's row in `ASSERTED_STATEMENTS` in \
     `xtask/src/gates.rs` — its count, its values and a comment saying which of the three \
     each is. A value that can come from a request, a plugin or a person is the injection \
     `AssertSqlSafe` switches off the compiler's refusal of, and a `COPY` never had one: \
     bind it as a parameter instead, or write the `COPY` out as a literal";

/// Every statement a file tells `sqlx` is safe is one [`ASSERTED_STATEMENTS`] vouches for.
///
/// A file with no row and no call is not a finding; one with calls and no row
/// is one finding per call, because each is a statement nobody has read. A
/// file with a row is held to its count and to its values in both directions,
/// so a value no call uses any more is a row outliving what it described. A
/// call is an `AssertSqlSafe` wrap, a `COPY` statement built from values
/// ([`copied_statements`]) or a pragma set from values ([`pragmas_set`]), and
/// the three are counted together.
pub(crate) fn every_asserted_statement_is_named(file: &str, text: &str) -> Vec<Finding> {
    let calls: Vec<(usize, Option<Vec<String>>, String)> = asserted_calls(text)
        .into_iter()
        .map(|(line, names)| (line, names, ASSERT_SQL_SAFE.to_owned()))
        .chain(copied_statements(text))
        .chain(pragmas_set(text))
        .collect();
    let row = ASSERTED_STATEMENTS.iter().find(|row| row.file == file);
    let mut findings = Vec::new();
    let unread = |line: usize, what: String| Finding {
        file: file.to_owned(),
        line,
        what,
        fix: UNREAD_STATEMENT.to_owned(),
    };
    for (line, names, door) in &calls {
        let Some(names) = names else {
            findings.push(unread(
                *line,
                format!(
                    "`{door}` is named here other than as a call this gate can read — \
                     passed as a value, or renamed by a `use` — so what it vouches for \
                     is out of sight of every row"
                ),
            ));
            continue;
        };
        let Some(row) = row else {
            let how = if door == ASSERT_SQL_SAFE {
                format!("vouched for with `{door}`")
            } else {
                format!("built from values and handed to `{door}`, which runs any text")
            };
            findings.push(unread(
                *line,
                format!(
                    "this statement is {how}, and no row of `ASSERTED_STATEMENTS` names this file"
                ),
            ));
            continue;
        };
        for name in names {
            if !row.from.contains(&name.as_str()) {
                findings.push(unread(
                    *line,
                    format!(
                        "this statement is built from `{name}`, which the file's row in \
                         `ASSERTED_STATEMENTS` does not name"
                    ),
                ));
            }
        }
    }
    for line in builders_named(text) {
        findings.push(Finding {
            file: file.to_owned(),
            line,
            what: format!(
                "`{QUERY_BUILDER}` builds a statement from whatever its `new` and `push` \
                 are handed and vouches for it inside `sqlx`, where no row of \
                 `ASSERTED_STATEMENTS` can read what it was built from"
            ),
            fix: format!(
                "write the statement as a `&'static str` and bind its values as \
                 parameters, or build its text in one `{ASSERT_SQL_SAFE}(..)` call and \
                 give the file a row — the one form whose values this gate holds to what \
                 a reviewer read"
            ),
        });
    }
    if let Some(row) = row {
        if calls.len() != row.calls {
            findings.push(unread(
                1,
                format!(
                    "this file vouches for a statement {} time(s) — with `{ASSERT_SQL_SAFE}`, \
                     or as a `COPY` built from values — and its row in \
                     `ASSERTED_STATEMENTS` says {}: a statement was vouched for, or a \
                     vouched-for one went, without the row being read again",
                    calls.len(),
                    row.calls
                ),
            ));
        }
        for value in row.from {
            let used = calls
                .iter()
                .filter_map(|(_, names, _)| names.as_ref())
                .any(|names| names.iter().any(|name| name == value));
            if !used {
                findings.push(unread(
                    1,
                    format!(
                        "the file's row in `ASSERTED_STATEMENTS` names `{value}`, and no \
                         statement here is built from it any more"
                    ),
                ));
            }
        }
    }
    findings
}

/// The rows of [`ASSERTED_STATEMENTS`] whose file no walk handed over.
///
/// The half the per-file judgement cannot see: a file deleted or moved with
/// its row left behind is a row describing nothing, and a walk that reads no
/// files at all leaves every row here — which is what makes this table its
/// own floor.
pub(crate) fn asserted_files_unread(read: &[String]) -> Vec<Finding> {
    ASSERTED_STATEMENTS
        .iter()
        .filter(|row| !read.iter().any(|file| file == row.file))
        .map(|row| Finding {
            file: row.file.to_owned(),
            line: 1,
            what: format!(
                "`ASSERTED_STATEMENTS` vouches for {} statement(s) in this file and the \
                 walk read no such file",
                row.calls
            ),
            fix: "take the row out of `ASSERTED_STATEMENTS` in `xtask/src/gates.rs` if \
                  the file is gone, or move it with the file: a row nothing reads is a \
                  review nobody can check"
                .to_owned(),
        })
        .collect()
}

#[cfg(test)]
mod asserted_tests {
    use super::{
        ASSERT_SQL_SAFE, ASSERTED_STATEMENTS, COPY_DOORS, QUERY_BUILDER, asserted_calls,
        asserted_files_unread, copied_statements, every_asserted_statement_is_named, pragmas_set,
    };

    /// A statement, written the way this tree writes one.
    fn call(argument: &str) -> String {
        format!("    sqlx::query(sqlx::{ASSERT_SQL_SAFE}({argument}))\n")
    }

    #[test]
    fn a_call_is_built_from_its_values_and_its_placeholders() {
        let text = [
            call("format!(\"DROP SCHEMA {quoted} CASCADE\")"),
            call("&*statement"),
            call("crate::dev::path_statement(&schema)"),
            call("statement.as_str()"),
            call("format!(\"SELECT {{}} FROM {}\", other)"),
        ]
        .concat();
        let read: Vec<Vec<String>> = asserted_calls(&text)
            .into_iter()
            .map(|(_, names)| names.expect("each is a call"))
            .collect();
        assert_eq!(
            read,
            [
                vec!["quoted"],
                vec!["statement"],
                vec!["path_statement", "schema"],
                vec!["statement"],
                vec!["other"],
            ],
            "a macro, a path qualifier, a method and an escaped brace are not values"
        );
    }

    #[test]
    fn a_mention_in_a_comment_a_string_or_an_import_is_not_a_call() {
        let text = format!(
            "    // sqlx::{ASSERT_SQL_SAFE}(x)\n    let s = \"{ASSERT_SQL_SAFE}(y)\";\n\
             use sqlx::{ASSERT_SQL_SAFE};\nuse sqlx::{{{ASSERT_SQL_SAFE}, Row}};\n"
        );
        assert_eq!(
            asserted_calls(&text),
            [] as [(usize, Option<Vec<String>>); 0]
        );
    }

    #[test]
    fn a_mention_that_is_not_a_call_is_reported_rather_than_skipped() {
        let passed = format!("    names.into_iter().map(sqlx::{ASSERT_SQL_SAFE});\n");
        let renamed = format!("use sqlx::{ASSERT_SQL_SAFE} as Trusted;\n");
        for text in [passed, renamed] {
            let findings = every_asserted_statement_is_named("xtask/src/dev.rs", &text);
            assert!(
                findings
                    .iter()
                    .any(|finding| finding.what.contains("other than as a call")),
                "{text:?} hid what it vouches for: {findings:?}"
            );
        }
    }

    #[test]
    fn a_call_in_a_file_no_row_names_is_a_finding() {
        let findings =
            every_asserted_statement_is_named("crates/nc-canon/src/lib.rs", &call("&*query"));
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].what.contains("no row"), "{findings:?}");
        let left_over =
            every_asserted_statement_is_named("crates/nc-canon/src/lib.rs", "fn f() {}\n");
        assert!(
            left_over.is_empty(),
            "a file that vouches for nothing needs no row: {left_over:?}"
        );
    }

    #[test]
    fn a_row_holds_its_file_to_its_count_and_its_values_both_ways() {
        let file = "xtask/src/dev.rs";
        assert_eq!(
            every_asserted_statement_is_named(file, &call("&*pinned")),
            [] as [crate::gates::Finding; 0]
        );
        let twice = [call("&*pinned"), call("&*pinned")].concat();
        let counted = every_asserted_statement_is_named(file, &twice);
        assert!(
            counted
                .iter()
                .any(|finding| finding.what.contains("2 time(s)")),
            "a second statement was vouched for and the row still says one: {counted:?}"
        );
        let swapped = every_asserted_statement_is_named(file, &call("&*unread"));
        assert!(
            swapped
                .iter()
                .any(|finding| finding.what.contains("`unread`")),
            "a statement built from a value nobody read passed: {swapped:?}"
        );
        assert!(
            swapped
                .iter()
                .any(|finding| finding.what.contains("`pinned`")
                    && finding.what.contains("any more")),
            "the row's value outlived the statement it described: {swapped:?}"
        );
    }

    #[test]
    fn a_statement_builder_is_a_finding_even_in_a_file_with_a_row() {
        // The shape measured: a builder over `format!`ted text, which compiles
        // under `sqlx` 0.9 with no wrapper written anywhere.
        let text = format!(
            "fn wipe(table: &str) {{\n\
             \x20   let mut built = sqlx::{QUERY_BUILDER}::<sqlx::Postgres>::new(\n\
             \x20       format!(\"DELETE FROM {{table}}\"),\n\
             \x20   );\n\
             \x20   built.push(format!(\" WHERE owner = '{{table}}'\"));\n\
             }}\n\
             // a {QUERY_BUILDER} named in prose\n\
             const NOTE: &str = \"{QUERY_BUILDER}\";\n\
             struct {QUERY_BUILDER}Ish;\n"
        );
        for file in [
            "crates/nc-prune/src/postgres.rs",
            "crates/nc-canon/src/lib.rs",
        ] {
            let named: Vec<usize> = every_asserted_statement_is_named(file, &text)
                .iter()
                .filter(|finding| finding.what.contains(QUERY_BUILDER))
                .map(|finding| finding.line)
                .collect();
            assert_eq!(
                named,
                [2],
                "in `{file}`, the builder is the one line to report — not the prose, the \
                 string or a longer name"
            );
        }
    }

    /// A pragma set from values is a statement vouched for, and one set from
    /// literals is not.
    ///
    /// **Found by a reviewer**: `SqliteConnectOptions::pragma` takes any
    /// `Into<Cow<'static, str>>` and runs it on every connection, so a value
    /// built from the environment in the shipped `crates/nc-local/src/open.rs`
    /// reached SQLite with no wrap and no row, and every gate passed.
    #[test]
    fn a_pragma_set_from_values_is_a_vouched_statement_and_a_literal_one_is_not() {
        let no_row = "crates/nc-local/src/open.rs";
        let built =
            "        .pragma(\"cell_size_check\", std::env::var(\"NC\").unwrap_or_default())\n";
        let findings = every_asserted_statement_is_named(no_row, built);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].what.contains("`pragma`") && findings[0].what.contains("no row"),
            "{findings:?}"
        );
        let formatted = "    options\n        .pragma(\n            \"cache_size\",\n            \
                         format!(\"-{}\", file.as_os_str().len()),\n        )\n";
        assert_eq!(
            pragmas_set(formatted),
            [(2, Some(vec!["file".to_owned()]), "pragma".to_owned())],
            "the key and the value are both what the batch is built from"
        );
        for literal in [
            "        .pragma(\"trusted_schema\", \"OFF\")\n        .pragma(\"temp_store\", \"MEMORY\");\n",
            "    options.pragma(\"fullfsync\", \"ON\")\n",
            "    let pragma = 1; fn pragma() {}\n",
        ] {
            assert!(
                every_asserted_statement_is_named(no_row, literal).is_empty(),
                "a pragma set from literals, or a name that is no call, was read as built \
                 from values:\n{literal}"
            );
        }
        let passed = "    let door = SqliteConnectOptions::new().pragma;\n";
        assert!(
            every_asserted_statement_is_named(no_row, passed)
                .iter()
                .any(|finding| finding.what.contains("other than as a call")),
            "a pragma named and not called hid what it sets"
        );
        // **Called as a path**, and passed as a value to be called later: a
        // reviewer planted each in the store's shipped options and the walk
        // read neither.
        let as_a_path = "    let options = SqliteConnectOptions::pragma(options, \"cache_size\", \
                         std::env::var(\"NC_CACHE\").unwrap_or_default());\n";
        let findings = every_asserted_statement_is_named(no_row, as_a_path);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(
            pragmas_set("    sqlx::sqlite::SqliteConnectOptions::pragma(options, key, value)\n"),
            [(
                1,
                Some(vec!["key".to_owned(), "value".to_owned()]),
                "pragma".to_owned()
            )],
            "a path call's key and value are what the batch is built from, and not the \
             options it is called on"
        );
        let left_over = every_asserted_statement_is_named(
            no_row,
            "    SqliteConnectOptions::pragma(options, \"cache_size\", \"-2000\")\n",
        );
        assert!(
            left_over.is_empty(),
            "a path call of literals was read as built from values: {left_over:?}"
        );
        let bound = "    let set = SqliteConnectOptions::pragma;\n    set(options, key, value);\n";
        assert!(
            every_asserted_statement_is_named(no_row, bound)
                .iter()
                .any(|finding| finding.what.contains("other than as a call")),
            "the pragma door passed as a value hid what it sets"
        );
    }

    /// A `COPY` built from values is a statement vouched for, and one written
    /// as a literal is not.
    ///
    /// **Found by a reviewer**: `copy_in_raw` takes a plain `&str`, so a `COPY`
    /// built from values reached PostgreSQL with no wrap and no row, and this
    /// walk saw nothing. The literal form is how this tree calls it.
    #[test]
    fn a_copy_built_from_values_is_a_vouched_statement_and_a_literal_one_is_not() {
        let [copy_in, copy_out] = COPY_DOORS;
        let no_row = "crates/nc-canon/src/lib.rs";
        let built = format!("    pool.{copy_in}(&format!(\"COPY {{t}} FROM STDIN\"))\n");
        let findings = every_asserted_statement_is_named(no_row, &built);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].what.contains(copy_in) && findings[0].what.contains("no row"),
            "{findings:?}"
        );

        // The statement is the last argument: a pool's copy called through its
        // trait takes the pool first, and the pool is not what the text is
        // built from.
        let through_the_trait = format!(
            "    sqlx::postgres::PgPoolCopyExt::{copy_out}(\n        &pool,\n        \
             &format!(\"COPY {{table}} TO STDOUT\"),\n    )\n"
        );
        assert_eq!(
            copied_statements(&through_the_trait),
            [(1, Some(vec!["table".to_owned()]), copy_out.to_owned())]
        );

        // A literal of any kind names nothing, and needs no row.
        for literal in [
            format!(
                "    connection\n        .{copy_in}(\n            \"COPY t (a) \\\n             FROM STDIN\",\n        )\n"
            ),
            format!("    connection.{copy_out}(r#\"COPY t TO STDOUT\"#)\n"),
        ] {
            assert!(
                every_asserted_statement_is_named(no_row, &literal).is_empty(),
                "a literal was read as built from values:\n{literal}"
            );
        }

        // Passed as a value, the statement it will be handed is out of sight.
        let passed = format!("    let door = sqlx::PgConnection::{copy_in};\n");
        assert!(
            every_asserted_statement_is_named(no_row, &passed)
                .iter()
                .any(|finding| finding.what.contains("other than as a call")),
            "a copy passed as a value hid its statement"
        );

        // And in a file with a row, a copy is counted with the wraps and its
        // values are held to the row's.
        let file = "xtask/src/dev.rs";
        let counted = format!("    pool.{copy_in}(&*pinned)\n");
        assert_eq!(
            every_asserted_statement_is_named(file, &counted),
            [] as [crate::gates::Finding; 0]
        );
        let both = [call("&*pinned"), counted].concat();
        assert!(
            every_asserted_statement_is_named(file, &both)
                .iter()
                .any(|finding| finding.what.contains("2 time(s)")),
            "a copy beside the one wrap the row counts was not counted"
        );
    }

    #[test]
    fn a_row_whose_file_was_not_read_is_a_finding() {
        let every: Vec<String> = ASSERTED_STATEMENTS
            .iter()
            .map(|row| row.file.to_owned())
            .collect();
        assert_eq!(
            asserted_files_unread(&every),
            [] as [crate::gates::Finding; 0]
        );
        let missing = asserted_files_unread(&every[1..]);
        assert_eq!(missing.len(), 1, "{missing:?}");
        assert_eq!(missing[0].file, every[0]);
        assert_eq!(
            asserted_files_unread(&[]).len(),
            ASSERTED_STATEMENTS.len(),
            "a walk that read nothing left a row standing"
        );
    }

    /// Every row is the tree as it stands, read through the gate itself.
    #[test]
    fn every_row_is_the_file_it_names() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("xtask sits inside the repository");
        for row in ASSERTED_STATEMENTS {
            let text = std::fs::read_to_string(root.join(row.file))
                .unwrap_or_else(|error| panic!("`{}`: {error}", row.file));
            let findings = every_asserted_statement_is_named(row.file, &text);
            assert!(findings.is_empty(), "{findings:?}");
        }
        let mut sorted: Vec<&str> = ASSERTED_STATEMENTS.iter().map(|row| row.file).collect();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            ASSERTED_STATEMENTS.len(),
            "a file with two rows"
        );
    }
}

#[cfg(test)]
mod the_unsafe_rule {
    use super::{blanked, walk};

    /// The lines of `text` that write the `unsafe` keyword as code.
    ///
    /// Not in a comment and not inside a string: what is blanked is read as
    /// nothing.
    fn written_unsafe(text: &str) -> Vec<usize> {
        let code: Vec<char> = blanked(text).chars().collect();
        let word = |at: usize| {
            code.get(at)
                .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        };
        let needle: Vec<char> = "unsafe".chars().collect();
        (0..code.len().saturating_sub(needle.len() - 1))
            .filter(|&at| {
                code[at..at + needle.len()] == needle[..]
                    && (at == 0 || !word(at - 1))
                    && !word(at + needle.len())
            })
            .map(|at| code[..at].iter().filter(|c| **c == '\n').count() + 1)
            .collect()
    }

    #[test]
    fn the_keyword_in_code_is_found_and_the_word_in_prose_is_not() {
        let text = [
            "// an unsafe comment\n",
            "/// `unsafe` in a doc\n",
            "let s = \"unsafe\";\n",
            "let r = r#\"unsafe\"#;\n",
            "let unsafely = 1;\n",
            "unsafe { f() }\n",
            "pub unsafe fn g() {}\n",
        ]
        .concat();
        assert_eq!(written_unsafe(&text), [6, 7]);
    }

    /// Every `unsafe` written in a `.rs` file under `root`, each as a finding at
    /// its line, and every file read.
    ///
    /// **One walk, the tree's and the fixture's**, so a filter written in it is
    /// one `the_unsafe_walk_goes_everywhere` sees: the walk paid nothing that a
    /// counter could hold, and three files named by hand were its only floor.
    pub(super) fn unsafe_written_under(
        root: &std::path::Path,
    ) -> (Vec<super::Finding>, Vec<String>) {
        let mut read: Vec<String> = Vec::new();
        let mut written: Vec<super::Finding> = Vec::new();
        walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let relative = crate::relative_to(root, path);
            let text = std::fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("`{relative}` is readable: {error}"));
            for line in written_unsafe(&text) {
                written.push(super::Finding {
                    file: relative.clone(),
                    line,
                    what: "`unsafe` is written here".to_owned(),
                    fix: "no `unsafe` is written in this tree (`AGENTS.md`, invariant 17)"
                        .to_owned(),
                });
            }
            read.push(relative);
        })
        .expect("the tree walks");
        (written, read)
    }

    /// No `unsafe` is written in any Rust file of this tree.
    ///
    /// **`forbid(unsafe_code)` is the rule, and what it covers is narrower
    /// than it reads.** It refuses `unsafe` written in a crate and says
    /// nothing about `unsafe` a dependency's macro expands into one:
    /// wasmtime's `bindgen!` and `wit-bindgen`'s `generate!` and `export!` all
    /// expand to `unsafe` blocks, attributes and functions, and rustc does not
    /// report `unsafe_code` inside an external macro's expansion. So the rule
    /// this repository states is *no `unsafe` is written here*, and this is
    /// the test of that sentence rather than of the lint: every `.rs` the tree
    /// holds — members, plugins, fixtures, canaries and tests alike — with its
    /// comments and strings blanked, holds no `unsafe` keyword.
    ///
    /// It walks the files the gates walk and asserts it reached three of them
    /// by name, one per place a generated `unsafe` sits beside hand-written
    /// code, so a walk that stopped short cannot pass for one that looked.
    #[test]
    fn no_unsafe_is_written_in_any_rust_file_of_this_tree() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let (written, read) = unsafe_written_under(root);
        assert!(
            written.is_empty(),
            "`unsafe` is written in this tree, which `AGENTS.md` says it never is: \
             {written:?}"
        );
        for expected in [
            "crates/nc-kernel/src/bindings.rs",
            "crates/nc-guest/src/lib.rs",
            "plugins/reference/src/adapter.rs",
        ] {
            assert!(
                read.iter().any(|file| file == expected),
                "`{expected}` was not read, so this walk did not reach the code it is about"
            );
        }
    }
}

/// Every variable `.env.example` declares is read by something in the tree.
///
/// **A setting nothing reads is worse than an absent one.** `.env.example` is
/// this repository's answer to "what does the stack read", and it declared
/// `NC_LOG_ENDPOINT`, which nothing anywhere read: the log endpoint is an
/// argument to `Log::open`, defaulted by a constant in `ship.rs`. A developer
/// changing that line would have watched it have no effect, in the one file
/// whose job is to be believed about configuration.
///
/// A variable is read when its name appears in **code, with the comments
/// stripped**: `docker-compose.yml` substitutes most of them, and `xtask` and
/// the tests read the rest.
///
/// **Not "anywhere in the tree", which was the first version and was satisfied
/// by this very paragraph.** Written that way, the gate found `NC_LOG_ENDPOINT`
/// in the doc comment explaining why `NC_LOG_ENDPOINT` is not read, and passed.
/// A gate whose own prose satisfies it is wired to nothing — the same defect
/// `every_generated_value_is_compared.rs` was defeated by, one file over.
///
/// It is still a weak test, and deliberately: the strong version would need to
/// know how a shell expands a compose file. A name appearing in no code at all
/// is the case worth catching, and it is the case that happened.
///
/// # Errors
///
/// Returns why the tree could not be walked.
pub(crate) fn no_unread_variable(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let example = root.join(".env.example");
    let Ok(text) = std::fs::read_to_string(&example) else {
        return Err(".env.example is not there, so nothing says what the stack reads".to_owned());
    };
    let declared: Vec<(usize, String)> = text
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.starts_with('#') || line.is_empty() {
                return None;
            }
            line.split_once('=')
                .map(|(name, _)| (index + 1, name.trim().to_owned()))
        })
        .filter(|(_, name)| !name.is_empty())
        .collect();

    // Every file that could consume a variable, with its comments removed.
    let mut elsewhere = String::new();
    walk(root, &mut |path| {
        if path == example || !consumes_variables(path) {
            return;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        // **SQL's comment is `--`, and it was never stripped.** Everything that
        // was not Rust got `#`, so `-- NC_AUDIT_PROBE is not read here; this
        // line is prose.` in a migration counted as a read: 1 finding without
        // it, **0** with. That is this gate's own sentence — *a gate whose own
        // prose satisfies it is wired to nothing* — one file type along.
        let opener = match path.extension().and_then(|kind| kind.to_str()) {
            Some("rs") => "//",
            Some("sql") => "--",
            _ => "#",
        };
        elsewhere.push_str(&without_comments(&text, opener));
        elsewhere.push('\n');
    })?;

    let mut findings = Vec::new();
    for (line, name) in &declared {
        // **A whole name, not a substring.** `NC_PROBE` was "read" by a file
        // mentioning `NC_PROBE_SOMETHING_ELSE`, so declaring a prefix of an
        // existing variable satisfied this for free.
        if reads_the_name(&elsewhere, name) {
            continue;
        }
        findings.push(Finding {
            file: ".env.example".to_owned(),
            line: *line,
            what: format!("`{name}` is declared here and read nowhere"),
            fix: "this file is what says which settings the stack has, so a name in it \
                  that nothing reads is a line a developer will change and then watch do \
                  nothing. Read it where it belongs, or delete it and say in a comment \
                  what takes its place"
                .to_owned(),
        });
    }
    Ok((findings, declared.len()))
}

/// Whether `code` uses `name` as a whole name.
///
/// An environment variable's name is a word: `NC_PROBE` is not read by a file
/// that mentions `NC_PROBE_SOMETHING_ELSE`, and treating a substring as a use
/// let a declared prefix of an existing variable satisfy the gate for free.
fn reads_the_name(code: &str, name: &str) -> bool {
    let part_of_a_name = |c: char| c.is_alphanumeric() || c == '_';
    let mut from = 0;
    while let Some(at) = code[from..].find(name) {
        let here = from + at;
        from = here + name.len();
        let before = code[..here].chars().next_back().is_some_and(part_of_a_name);
        let after = code[from..].chars().next().is_some_and(part_of_a_name);
        if !before && !after {
            return true;
        }
    }
    false
}

/// Whether a file is one an environment variable could reach.
///
/// Rust, the compose file, the workflows, and shell — the four places a name in
/// `.env.example` is actually consumed. A Markdown file is prose about the
/// settings, and prose is what the first version of this gate mistook for a
/// reader.
fn consumes_variables(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    if matches!(name, "justfile" | "docker-compose.yml" | ".env") {
        return true;
    }
    // **Not `toml`.** A `description = "…"` in a registry is prose about a
    // setting, and reading it as a use of one made the gate satisfiable by
    // documentation — the same defect its first version had, one file type
    // along. Nothing in this tree reads an environment variable from a TOML
    // file; `docker-compose.yml`, the workflows, the shell and Rust do.
    path.extension()
        .and_then(|kind| kind.to_str())
        .is_some_and(|kind| matches!(kind, "rs" | "yml" | "yaml" | "sh" | "sql"))
}
/// The lints `[workspace.lints]` actually declares, one case each.
///
/// **Nothing pinned the contents.** `every_member_takes_the_workspace_lints`
/// checks that each member *opts in*; nothing checked there was anything to opt
/// into. Measured: `unsafe_code = "forbid"` deleted from the root manifest and a
/// raw-pointer dereference planted in `xtask/src/store.rs` — it compiles, which
/// this gate's own doc calls the proof a crate is outside the lints, and every
/// printed line was byte-identical to a clean run:
///
/// ```text
/// ok    9 member(s) take the workspace lints
/// ok    lints
/// check passed
/// ```
///
/// Written out rather than read from the manifest: a test that asked the file
/// what the file says would agree with any file. `unsafe_code = "forbid"` is the
/// one that cannot be overridden at crate level and is therefore the one whose
/// absence is invisible.
#[cfg(test)]
pub(crate) fn the_workspace_lints(root: &Path) -> Result<Vec<(String, String)>, String> {
    let text = std::fs::read_to_string(root.join("Cargo.toml"))
        .map_err(|error| format!("the root manifest could not be read: {error}"))?;
    let manifest = toml::from_str::<toml::Value>(&text)
        .map_err(|error| format!("the root manifest is not TOML: {error}"))?;
    let mut found = Vec::new();
    // **Four tables, and the set is cargo's, not the one somebody remembered.**
    // `cargo` accepts `cargo`, `clippy`, `rust` and `rustdoc` — it says so
    // itself, in the warning it prints for any other name:
    //
    // ```text
    // warning: unrecognized lint tool `lints.rustfmt` … supported tools: cargo, clippy, rust, rustdoc
    // ```
    //
    // The pin read two, then three. `[workspace.lints.cargo]` is inert on stable
    // today (`-Zcargo-lints` is nightly), so nothing is switched off through it
    // yet; it is pinned because a pin over a set enumerates the set, and the
    // stable release that lights it up is not a release anybody will re-derive
    // this list for. The `rustdoc` omission was not inert: two lines in the root
    // manifest,
    //
    // ```toml
    // [workspace.lints.rustdoc]
    // broken_intra_doc_links = "allow"
    // ```
    //
    // turned off the workspace's only reader of documentation with a real broken
    // link standing: `ok lints`, `ok documentation`, `check passed`, every count
    // up. `no_hidden_item_carries_an_intra_doc_link` exists because that gate is
    // the one thing reading intra-doc links at all.
    for tool in ["cargo", "clippy", "rust", "rustdoc"] {
        let Some(table) = manifest
            .get("lints")
            .and_then(|lints| lints.get("workspace"))
            .and_then(|workspace| workspace.get(tool))
            .or_else(|| {
                manifest
                    .get("workspace")
                    .and_then(|workspace| workspace.get("lints"))
                    .and_then(|lints| lints.get(tool))
            })
            .and_then(toml::Value::as_table)
        else {
            continue;
        };
        for (name, value) in table {
            // A lint is either `"warn"` or `{ level = "warn", priority = -1 }`.
            let level = value
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    value
                        .get("level")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                })
                .unwrap_or_default();
            found.push((format!("{tool}::{name}"), level));
        }
    }
    found.sort();
    Ok(found)
}

/// Every member opts in to the workspace lints.
///
/// **The lints are opt-in per crate, and nothing kept a crate inside them.**
/// `[workspace.lints]` sets `unsafe_code = "forbid"`, `missing_docs` and clippy
/// `pedantic`, and a member gets none of it without `[lints] workspace = true`.
/// Measured: deleting those two lines from `crates/nc-ids/Cargo.toml` and adding
/// a public undocumented function with an `unsafe` block gave `check passed` —
/// the workspace's strongest lint, off for a crate, with nothing reporting it.
/// A crate added without the stanza is unlinted from birth, which is the more
/// likely way it happens.
///
/// # Errors
///
/// Returns why the manifests could not be read.
pub(crate) fn every_member_takes_the_workspace_lints(
    root: &Path,
) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut checked = 0_usize;
    for path in manifests(root)? {
        let relative = relative_to(root, &path);
        // The workspace root declares the lints rather than taking them.
        if relative == "Cargo.toml" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        // **`workspace = true` *inside* `[lints]`, not two unrelated questions.**
        // This asked whether some line trimmed to `workspace = true` and,
        // separately, whether the text held `[lints]` anywhere — so deleting the
        // stanza and writing
        //
        //     # The workspace's [lints] table is declared at the root.
        //     [dev-dependencies.proptest]
        //     workspace = true
        //
        // satisfied both. Measured: a public undocumented function with an
        // `unsafe` block compiled, with `9 member(s) take the workspace lints`
        // and `check passed`. `unsafe_code = "forbid"` cannot be overridden at
        // crate level, so the compile succeeding *is* the proof the crate was
        // outside the lints. A crate written with table-form dependencies is
        // unlinted from birth.
        // **Asked of the parser, not of the lines.** `section_of` is a line
        // scanner, so a TOML multi-line string holding
        //
        // ```toml
        // description = """
        // [lints]
        // workspace = true
        // """
        // ```
        //
        // satisfied it — measured, with the real stanza deleted from
        // `crates/nc-prune/Cargo.toml`: `9 member(s) take the workspace lints`,
        // and a public undocumented function with an `unsafe` block compiling
        // clean in that crate. `unsafe_code = "forbid"` cannot be overridden at
        // crate level, so the compile succeeding *is* the proof the crate was
        // outside the lints. `is_a_build_directory` two hundred lines down
        // already asks `toml` this kind of question, and this crate already
        // depends on it: a manifest is TOML, and reading it as text is writing a
        // second parser that disagrees with cargo's.
        //
        // `toml::from_str`, not `str::parse` — `FromStr for Value` parses a
        // *value* and refuses a document at `line 1, column 10`, which is the
        // newline after `[package]`. Silently: the `.ok()` turned every manifest
        // in the tree into a finding, which at least fails loudly. A parse that
        // cannot fail quietly is worth more than one that reads the right thing
        // most of the time.
        let takes_them = toml::from_str::<toml::Value>(&text)
            .ok()
            .and_then(|manifest| manifest.get("lints")?.get("workspace")?.as_bool())
            .unwrap_or(false);
        if !takes_them {
            findings.push(Finding {
                file: relative,
                line: 1,
                what: "this member does not take the workspace lints".to_owned(),
                fix: "`[workspace.lints]` forbids unsafe, requires documentation on every \
                      public item and runs clippy's pedantic set — and a member gets none \
                      of it without `[lints]\nworkspace = true`. Two lines are the \
                      difference between a crate inside the rules and a crate outside \
                      them, and nothing else here would say which this is"
                    .to_owned(),
            });
        }
        // **The last statement a verdict reaches, not the first of the body.**
        // `checked` is floored against `members_of(root)`, and it stood above
        // the judgement — so `if relative == "xtask/Cargo.toml" { continue; }`
        // one line under it kept both numbers exact with the stanza deleted from
        // that manifest and an `unsafe` dereference compiling in its source:
        // `ok 9 member(s) take the workspace lints`, `ok lints`, `check passed`.
        // This document's own rule, unapplied in the file that states it.
        checked += 1;
    }
    Ok((findings, checked))
}

/// The lint groups a source file may not turn off for itself.
///
/// **The manifest gate above asks whether a crate takes the lints; this asks
/// whether it then puts them down.** One inner attribute at the top of a
/// `lib.rs` —
///
/// ```text
/// #![allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
/// ```
///
/// — with three real clippy violations underneath, left `ok lints` and
/// `check passed`. `[lints] workspace = true` was still there and still true;
/// what it configures is a *default* a crate can override in one line.
///
/// A single named lint is not on this list. `crates/nc-catalog/src/lib.rs`
/// allows `clippy::match_same_arms` in generated code, which is `AGENTS.md`'s
/// rule working: an allow arrives with the comment that earns it, and a *group*
/// is not one allow, it is all of them at once.
const LINT_GROUPS: [&str; 26] = [
    "warnings",
    "unused",
    // **The eleven other groups `rustc` has, which were missing.** The list
    // named two of the thirteen — measured against `rustc -W help`, whose table
    // is the authority — so a crate root could write
    // `#![allow(nonstandard_style)]`, or `future_incompatible`, and this gate
    // said nothing while the verdict below it counts warnings. Each is a group
    // by rustc's own definition of one, which is the whole rule here: an allow
    // is a judgement about a lint, and a group is all of them at once.
    //
    // They are held to that table rather than copied from it, by
    // `every_lint_group_rustc_names_is_on_this_list` — the day the toolchain
    // grows a group, a hand-written list is silently short again.
    "deprecated_safe",
    "future_incompatible",
    "keyword_idents",
    "let_underscore",
    "nonstandard_style",
    "refining_impl_trait",
    "rust_2018_compatibility",
    "rust_2018_idioms",
    "rust_2021_compatibility",
    "rust_2024_compatibility",
    "unknown_or_malformed_diagnostic_attributes",
    // **The three `[workspace.lints.rust]` names, which were missing.** This
    // list held `warnings`, `unused` and ten clippy groups, so a crate root
    // could write `#![allow(missing_docs)]` — or put `unreachable_pub` or
    // `missing_debug_implementations` down for its whole tree — and this gate
    // said nothing, while `Cargo.toml` sets all three to `warn` and the verdict
    // counts warnings. `unsafe_code` is not here because it is `forbid`, which
    // Rust does not let an inner attribute downgrade: allowing it is a compile
    // error rather than a silence, and listing it would be a rule about
    // something that cannot happen.
    "missing_docs",
    "unreachable_pub",
    "missing_debug_implementations",
    "clippy::all",
    "clippy::pedantic",
    "clippy::nursery",
    "clippy::cargo",
    "clippy::correctness",
    "clippy::style",
    "clippy::complexity",
    "clippy::perf",
    "clippy::suspicious",
    "clippy::restriction",
];

/// Every lint name an inner attribute puts down, however it is nested.
///
/// **`allow` and `expect` both, and inside a `cfg_attr` too.** `expect`
/// suppresses exactly as `allow` does — it adds a warning when *nothing* was
/// suppressed, which is the opposite of what this gate is looking for — and
/// `#![cfg_attr(all(), allow(clippy::all))]` is the same attribute with a
/// predicate that is always true in front of it. The same wrapper defeated the
/// test census and the ignored-test gate, each in its own round.
fn lints_put_down(attribute: &str) -> Vec<String> {
    let mut names = Vec::new();
    for opener in ["allow(", "expect("] {
        for (at, _) in attribute.match_indices(opener) {
            // `disallow(` ends in `allow(` and is not one.
            if attribute[..at]
                .chars()
                .next_back()
                .is_some_and(|before| before.is_alphanumeric() || before == '_')
            {
                continue;
            }
            let mut depth = 0_i32;
            let mut inside = String::new();
            for character in attribute[at + opener.len()..].chars() {
                match character {
                    '(' => depth += 1,
                    ')' if depth == 0 => break,
                    ')' => depth -= 1,
                    _ => {}
                }
                inside.push(character);
            }
            // Whitespace removed rather than trimmed: `clippy :: all` is the
            // same lint written with room around it, and `trim` leaves it.
            names.extend(
                inside
                    .split(',')
                    .map(|named| {
                        named
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect::<String>()
                    })
                    .filter(|named| !named.is_empty()),
            );
        }
    }
    names
}
/// Whether this line declares a module, whatever its visibility.
///
/// `pub mod`, `pub(crate) mod` and a bare `mod` are one item to this gate: an
/// attribute above any of them reaches everything the module holds, and for a
/// file-module that is another file entirely.
fn a_module_declaration(code: &str) -> bool {
    let code = code.trim_start();
    let code = code.strip_prefix("pub").map_or(code, str::trim_start);
    let code = code.strip_prefix('(').map_or(code, |rest| {
        rest.split_once(')')
            .map_or(rest, |(_, after)| after.trim_start())
    });
    code.starts_with("mod ")
}

/// Every lint group `attribute` turns off, as a finding each.
///
/// `reaches` names what the attribute covers, because that is the whole
/// difference between an allow on a function — which is the mechanism working —
/// and the same allow on a module, which is this gate switched off for a file.
fn put_the_lints_down(
    root: &Path,
    path: &Path,
    at: usize,
    attribute: &str,
    reaches: &str,
    findings: &mut Vec<Finding>,
) {
    for named in lints_put_down(&flatten(attribute)) {
        let named = named.as_str();
        if LINT_GROUPS.contains(&named) {
            findings.push(Finding {
                file: relative_to(root, path),
                line: at + 1,
                what: format!("this file turns off `{named}` for {reaches}"),
                fix: "`[workspace.lints]` is a default, and one attribute overrides it for \
                      a whole crate or a whole module — measured, with three real clippy \
                      violations underneath and `ok lints`. Allow the one lint you mean, on \
                      the item you mean it for, with the comment that earns it"
                    .to_owned(),
            });
        }
    }
}

/// The word every `allow` and `expect` in this tree has to carry.
///
/// `#[expect]` takes it as an attribute argument and `#[allow]` takes it the
/// same way, so one needle reads both.
const A_WRITTEN_REASON: &str = "reason =";

/// Whether `code` opens a public function item, in any spelling Rust allows.
///
/// The grammar is `pub … const? async? unsafe? extern "abi"? fn`, and the words
/// between `pub` and `fn` are what a prefix list keeps missing: `pub async fn`
/// was outside the first version of the door rule and `pub extern "C" fn` was
/// outside the second. So the words are stepped over rather than enumerated —
/// what remains has to be `fn`, and anything else is not a function item.
///
/// A visibility of `pub(crate)` or `pub(super)` is deliberately **not** a door:
/// it reaches no further than the crate, and the rule is about a door a
/// consumer can open.
fn opens_a_public_function(code: &str) -> bool {
    let Some(rest) = code.strip_prefix("pub ") else {
        return false;
    };
    let mut words = rest.split_whitespace();
    loop {
        match words.next() {
            Some("fn") => return true,
            Some("const" | "async" | "unsafe") => {}
            Some("extern") => {
                // `extern "C"` takes an ABI string; `extern fn` does not.
                let mut ahead = words.clone();
                if ahead.next().is_some_and(|next| next.starts_with('"')) {
                    words.next();
                }
            }
            _ => return false,
        }
    }
}

/// What a walk over the hatches has counted so far.
///
/// Two numbers rather than two `&mut usize` parameters, because the door check
/// grew a third question and eight arguments is where clippy stops reading.
struct Tally {
    hatches: usize,
    doors: usize,
}

/// The crates whose manifest declares a feature that carries their test doors.
///
/// **Read from the manifests, not written down here.** A crate that declares
/// such a feature has said in its own `Cargo.toml` what the feature is for, and
/// `nc-journal`'s says it plainly: *"Off by default, so a shipped library cannot
/// be tampered with through its own public surface"*. That sentence is a claim
/// about the default build, and it was false — `Entry::forged_for_tests`, the
/// constructor that takes the hash as an argument and so walks straight past
/// `#[non_exhaustive]`, was in the default public surface. Measured on
/// 2026-09-08 with a `const _:` binding under `#[cfg(not(any(test, feature =
/// "test-support")))]`: `cargo build -p nc-journal` compiled.
///
/// A crate with no such feature is not asked for one: `fill_for_tests`,
/// `cell_for_tests` and the rest are read-only helpers in crates that make no
/// claim about their default surface, and demanding a feature they do not
/// declare would be a rule that refuses an ordinary arrangement.
fn the_crates_that_shut_their_doors(root: &Path) -> std::collections::BTreeSet<String> {
    let mut shut = std::collections::BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(root.join("crates")) else {
        return shut;
    };
    for entry in entries.flatten() {
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        if text.contains("\n[features]") && text.contains(&format!("\n{DOORS_ARE_BEHIND} =")) {
            shut.insert(crate::relative_to(root, &entry.path()));
        }
    }
    shut
}

/// The feature a crate puts its test doors behind, where it declares one.
const DOORS_ARE_BEHIND: &str = "test-support";

/// A `_for_tests` door is marked as one and says why it exists.
///
/// Split from [`every_hatch_carries_its_reason`], which crossed clippy's
/// hundred-line bound when the second half of invariant 13 arrived. The two
/// halves are one walk on purpose: both are habits the tree keeps by hand, and
/// a walk that reads the tree twice is a walk that can disagree with itself
/// about what it read.
fn a_test_door_says_it_is_one(
    relative: &str,
    lines: &[&str],
    index: usize,
    code: &str,
    tally: &mut Tally,
    shut: &std::collections::BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    // A door a test reaches a private thing through.
    //
    // **Every spelling `pub fn` can wear, not two of them.** This asked for
    // `pub fn` or `pub const fn`, and `crates/nc-journal/src/postgres.rs`'s
    // `pub async fn why_nothing_landed_for_tests` was outside the gate written
    // to hold invariant 13: deleting its `#[doc(hidden)]` moved no number in
    // the run, because `hatches` is read only as `hatches == 0` and the lint
    // hatches keep it non-zero on their own. `async` is not hypothetical — it
    // is the shape every door into the PostgreSQL store must take, and Stage 11
    // adds more of them. This file's own header records the same class being
    // missed once before, by a scanner that "knew seven keywords and one
    // visibility".
    // **And the property, not a list of prefixes.** Six spellings replaced two,
    // and Rust's grammar is `const? async? unsafe? extern abi? fn` — so
    // `pub extern "C" fn`, the only `extern` spelling anybody writes, and
    // `pub const unsafe fn` were both outside the six. Measured: an
    // undocumented, unhidden `pub extern "C" fn a_reviewer_door_for_tests()`
    // added to `crates/nc-prune/src/postgres.rs` left `check passed` with the
    // door count unmoved at eleven. An enumeration of spellings is the shape
    // this predicate has now been wrong in twice; what a door *is* is a public
    // function item, and that is what is asked.
    if code.contains("_for_tests") && opens_a_public_function(code) {
        tally.hatches += 1;
        // **Counted apart, because `hatches` cannot say a door is gone.** It is
        // read only as `hatches == 0` and the lint hatches keep it non-zero on
        // their own, so a door falling out of the predicate — a `pub async fn`
        // did — moved no number in the run at all. Incremented *here* rather
        // than beside the call, because a second copy of this predicate is one
        // rule written twice and the narrower copy is always the one that stays.
        tally.doors += 1;
        // **Both are looked for in the prologue, not on the line
        // above.** A door's shape is `/// why it exists`, then
        // `#[doc(hidden)]`, then the item — so reading only the
        // line above finds the attribute and reports every
        // correctly written door as carrying no reason. Measured:
        // six of them, on the first run.
        // **Through a wrapped attribute, which this walk used to stop at.** See
        // [`past_the_attributes`]: a wrapped attribute's closing line is neither
        // `#[` nor `///`, so the `take_while` ended on it and the prologue came
        // back empty — after which a door that carried both its marker and its
        // reason was told to add them. Every line from where that reader says
        // the attributes begin is prologue, and the walk above it continues as
        // before.
        let opens = past_the_attributes(lines, index);
        let mut prologue: Vec<&str> = lines[opens..index].iter().map(|line| line.trim()).collect();
        prologue.extend(
            lines[..opens]
                .iter()
                .rev()
                .take(12)
                .map(|line| line.trim())
                .take_while(|line| {
                    line.starts_with("///") || line.starts_with("#[") || line.is_empty()
                }),
        );
        let marked = prologue
            .iter()
            .any(|line| line.starts_with("#[doc(hidden)]"));
        let said = prologue.iter().any(|line| line.starts_with("///"));
        // **And out of the default build, where the crate says it is.** The
        // prologue is where a `#[cfg]` sits, beside the `#[doc(hidden)]` that is
        // already read from it.
        // **Joined, because a wrapped attribute is one attribute.** Wave 21
        // made the prologue survive rustfmt's wrapping and repaired two of the
        // three questions asked of it: `marked` and `said` need only *some*
        // line, and this one needs `#[cfg(` and the feature name on the **same**
        // one. So a door written `#[cfg(any(\n    test,\n    feature =
        // "test-support"\n))]` — the exact spelling this rule's own remedy
        // tells its author to write — was judged as being in the default build,
        // and told to do what it had already done. One rule, three questions,
        // two of them repaired. Found in wave 22.
        // **One attribute at a time, because the join spanned all of them.**
        // Wave 22 joined the whole prologue so a rustfmt-wrapped `#[cfg(any(…))]`
        // would read as one attribute, and then `#[cfg(` and the feature name
        // could come from *two different* ones: the docs.rs idiom
        // `#[cfg_attr(docsrs, doc(cfg(feature = "test-support")))]` — which this
        // rule's own fixture carries, and which puts the item behind nothing —
        // supplies the name, any unrelated real `#[cfg(…)]` supplies the opener,
        // and a door genuinely in the default build reads as behind the feature.
        // Invariant 13's default-build half switched off, silently. `marked` and
        // `said` ask line by line and cannot be fed this way; only the joined
        // question could. Found in wave 23.
        let mut attributes: Vec<String> = Vec::new();
        for line in prologue.iter().filter(|line| !line.starts_with("///")) {
            match attributes.last_mut() {
                Some(whole) if !line.starts_with("#[") => {
                    whole.push(' ');
                    whole.push_str(line);
                }
                _ => attributes.push((*line).to_owned()),
            }
        }
        let behind = attributes
            .iter()
            .any(|whole| whole.contains("#[cfg(") && whole.contains(DOORS_ARE_BEHIND));
        // **The crate's directory, not a name a sibling's begins with.** This
        // read `starts_with(whose)` with no separator, so a crate whose
        // directory name merely *opens* with another's was claimed by it — and
        // a crate that declares no `test-support` feature would be told its door
        // contradicts one, with the remedy pointing at a manifest key that is
        // not there. Reported on 2026-09-14 with a scratch workspace driving the
        // shipped walk; the type wall in `xtask/src/wall.rs` writes its own
        // sibling case out, and this rule had it missing.
        let inside = |whose: &String| {
            relative
                .strip_prefix(whose.as_str())
                .is_some_and(|rest| rest.starts_with('/'))
        };
        if shut.iter().any(inside) && !behind {
            findings.push(Finding {
                file: relative.to_owned(),
                line: index + 1,
                what: format!(
                    "a `_for_tests` door is in the default build of a crate whose \
                     `{DOORS_ARE_BEHIND}` feature says its doors are not"
                ),
                fix: format!(
                    "this crate's `Cargo.toml` declares `{DOORS_ARE_BEHIND}` and says what \
                     it is for — that a shipped library cannot be tampered with through \
                     its own public surface. A door outside it makes that sentence false. \
                     Put `#[cfg(any(test, feature = \"{DOORS_ARE_BEHIND}\"))]` on it, or \
                     take the claim out of the manifest"
                ),
            });
        }
        if !marked {
            findings.push(Finding {
                file: relative.to_owned(),
                line: index + 1,
                what: "a `_for_tests` door is not marked as one".to_owned(),
                fix: "a private thing a test must reach gets a \
                          `#[doc(hidden)]` door with the reason written on it — a \
                          copy is not a door, it is a second implementation that \
                          agrees with itself. Mark it, and say above it what it \
                          is for"
                    .to_owned(),
            });
        } else if !said {
            findings.push(Finding {
                file: relative.to_owned(),
                line: index + 1,
                what: "a `_for_tests` door carries no reason".to_owned(),
                fix: "the door is marked and says nothing about why it \
                          exists. What a reader needs is which real thing it \
                          reaches, so that the next person writes a door rather \
                          than a mirror"
                    .to_owned(),
            });
        }
    }
}

/// Every lint put down carries a written reason, and every test door says it is
/// one.
///
/// **Invariant 15 said it had no gate, and invariant 13 said the same.** Both
/// are right about what a gate cannot do: whether a paragraph *earns* an allow
/// is a judgement, and so is whether a `#[doc(hidden)]` door is the real thing
/// or a second implementation that agrees with itself. What a gate can do is
/// refuse the case where there is no paragraph at all, and count what it read —
/// which is the difference between a habit a reader has to check and a habit
/// that leaves a number.
///
/// **What this does not judge, said plainly so it cannot teach a reader that a
/// written reason is a good reason:** it does not read the reason, does not ask
/// whether it is true, does not ask whether the lint should have been put down,
/// and does not ask whether a `_for_tests` door reaches the real implementation
/// or a copy of it. It asks that somebody wrote something, and that the door is
/// marked as one. `-D warnings` is what makes an allow the only way to keep a
/// lint quiet, so every one of them is visible in a diff; `#[expect]`, which
/// this repository prefers, fails the build when its reason stops applying.
///
/// The count is what makes it a gate rather than a habit: a walk that stopped
/// matching reads the same as a tree with nothing put down, which is the failure
/// `§19.3` names.
pub(crate) fn every_hatch_carries_its_reason(
    root: &Path,
) -> Result<(Vec<Finding>, usize, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    let mut tally = Tally {
        hatches: 0,
        doors: 0,
    };
    let shut = the_crates_that_shut_their_doors(root);
    // **`plugins/` as well**, because a plugin is a workspace member that clippy
    // judges at `-D warnings` like any crate, so a lint put down there is put
    // down in the same tree. This walked `crates/` and `xtask/` alone after the
    // plugin tree arrived — measured, `#[allow(clippy::must_use_candidate)]`
    // with no reason in `plugins/reference/src/behaviour.rs` left this step's
    // line byte-identical to a clean run.
    for tree in ["crates", "plugins", "xtask"] {
        let root_of = root.join(tree);
        if !root_of.is_dir() {
            continue;
        }
        if walk(&root_of, &mut |path| {
            let relative = crate::relative_to(root, path);
            // **The scope is one predicate**, asked here and asked again by the
            // fixture that plants in every real directory. Restating it in the
            // test would make the test agree with whatever this walk does; this
            // way a directory that leaves the scope leaves the expectation in
            // the same edit. Found in wave 29.
            if !in_the_hatch_scope(&relative) {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            let lines: Vec<&str> = text.lines().collect();
            // **A string literal is not an attribute, and a quote is not a
            // literal.** The question was asked as `!code.contains('"')`, so any
            // quote anywhere on the line took it out of the gate entirely —
            // measured, `#[allow(clippy::all)] // see the note in "§15.1"` above
            // a real item was neither counted nor judged, and so was every
            // single-line `#[expect(…, reason = "…")]` in the tree: three of the
            // thirty-one, which is a rule that stops asking the moment somebody
            // answers it inline. It also missed what it was written for. The
            // emitter's template in `crates/nc-catalog-compiler/src/emit.rs`
            // carries `#![allow(clippy::match_same_arms)]` *inside* a string
            // whose quotes are on other lines, and that text was counted as one
            // of this tree's hatches.
            //
            // Both are the same question — is this line code — and this tree
            // already answers it. `without_string_contents` keeps every line and
            // every newline, so the spent copy can be indexed by the original's
            // line numbers.
            let spent = without_string_contents(&text);
            let bare: Vec<&str> = spent.lines().collect();
            // **Lines, not files**, because `floor_on_the_rust_lines` compares
            // against a second count of the tree's lines — handing it a file
            // count made the two sides different quantities and the floor read
            // `88 against 121928`, which is a gate reporting a defect that is
            // its own arithmetic.
            //
            // **And counted at the last statement a verdict reaches**, not
            // before the loop that judges. `read += lines.len();` stood here,
            // which holds only that the file was opened: a filter written
            // between the counter and the rule left the printed number and the
            // exact floor under it both whole while the rule was never asked —
            // the failure `xtask/canaries/README.md` records verbatim for the
            // two walks that had no mechanism, and this was the third, with no
            // identity, no canary and no fixture. Found in wave 29.
            let mut reached_a_verdict = 0_usize;
            for index in 0..lines.len() {
                let code = bare.get(index).map_or("", |line| line.trim());
                // A lint put down, in either spelling, as an attribute rather
                // than as a string in a fixture: this tree's gates carry dozens
                // of the latter and every one of them is a test's input.
                let puts_it_down = code.starts_with("#[allow(")
                    || code.starts_with("#![allow(")
                    || code.starts_with("#[expect(")
                    || code.starts_with("#![expect(");
                if puts_it_down {
                    tally.hatches += 1;
                    // The attribute may wrap, so the reason may be on a later
                    // line: read to the line that closes it.
                    let mut whole = String::new();
                    let mut depth = 0_isize;
                    for later in lines.iter().skip(index) {
                        whole.push_str(later);
                        depth += isize::try_from(later.matches('[').count()).unwrap_or(0);
                        depth -= isize::try_from(later.matches(']').count()).unwrap_or(0);
                        if depth <= 0 {
                            break;
                        }
                    }
                    // **Either spelling of a written reason.** `#[expect]` takes
                    // one as an attribute argument, which is the form this tree
                    // prefers because it fails the build when it stops applying;
                    // a `#![allow]` at a crate root takes a comment above it,
                    // because a crate-level attribute has no item to sit on and
                    // the generated crates carry theirs that way. Requiring the
                    // argument alone reported a correctly justified allow —
                    // measured, in `crates/nc-catalog/src/lib.rs`, whose three
                    // lines above it say exactly why — and pushing a `reason =`
                    // into the emitter's template then broke the *other* gate,
                    // whose quote walk reads `\"` as an opening quote and spends
                    // the rest of the line. A repair that is too strong is still
                    // a repair that is wrong.
                    // **And a doc comment is not a reason.** `///` begins
                    // with `//`, so an allow written under the documentation of
                    // the item it sits on satisfied this — which is where an
                    // allow is usually written, and which says nothing about the
                    // lint. Measured over the tree's twenty-eight hatch lines at
                    // the time: two rely on a comment above, both ordinary `//`
                    // prose written for the allow, and the rule was off for
                    // every documented item in the repository. `//!` is the same
                    // shape one level up.
                    let said_above = index
                        .checked_sub(1)
                        .and_then(|at| lines.get(at))
                        .map(|line| line.trim())
                        .is_some_and(|line| {
                            line.starts_with("//")
                                && !line.starts_with("///")
                                && !line.starts_with("//!")
                        });
                    if !whole.contains(A_WRITTEN_REASON) && !said_above {
                        findings.push(Finding {
                            file: relative.clone(),
                            line: index + 1,
                            what: "a lint is put down here with no written reason".to_owned(),
                            fix: "`-D warnings` means an allow is the only way to keep a \
                                  lint quiet, so this line is the whole record of why one \
                                  is quiet. Write `reason = \"…\"` inside the attribute. \
                                  Nothing reads the reason — what it earns is a judgement \
                                  and no gate makes one — but a reader meets it here"
                                .to_owned(),
                        });
                    }
                }
                a_test_door_says_it_is_one(
                    &relative,
                    &lines,
                    index,
                    code,
                    &mut tally,
                    &shut,
                    &mut findings,
                );
                reached_a_verdict += 1;
            }
            everything_reached_a_verdict(
                "line",
                &relative,
                lines.len(),
                reached_a_verdict,
                &mut findings,
            );
            read += reached_a_verdict;
        })
        .is_err()
        {
            return Err(format!("{tree}/ could not be walked"));
        }
    }
    if tally.hatches == 0 {
        return Err(
            "no lint is put down and no test door exists anywhere in this tree, \
                    which is a walk that stopped matching rather than a tree with \
                    nothing to say"
                .to_owned(),
        );
    }
    Ok((findings, read, tally.doors))
}

/// Whether this file is one the hatch walk asks its rule of.
///
/// **The canaries are the inputs, not the tree.**
/// `xtask/canaries/blanket_allow.rs` exists to hold a `#![allow(clippy::all)]`
/// that the blanket-allow gate must find; it is not compiled and reporting it
/// would be reporting a fixture for being one. `xtask/canaries/README.md`
/// argues each of them, which is the written reason this exclusion stands on —
/// and `the_exclusion_list_does_not_cover_what_the_gates_are_for` is the shape
/// that keeps such a list from turning a gate off.
///
/// Split out of the walk so the fixture that plants in every real directory can
/// ask the walk's own scope instead of restating it. The rule is the one the
/// walk already applied, spelled the same way: many of the real directories are
/// outside `crates/`, `plugins/` and `xtask/`, and an expectation that ignored
/// that refused a tree with nothing wrong with it.
pub(crate) fn in_the_hatch_scope(relative: &str) -> bool {
    (relative.starts_with("crates/")
        || relative.starts_with("plugins/")
        || relative.starts_with("xtask/"))
        // The extension is asked of the path rather than of the text, which is
        // how the walk asked it before this was split out — and `ends_with`
        // here is a lint in this workspace, because it decides `.RS` the other
        // way on a case-insensitive filesystem.
        && Path::new(relative).extension().is_some_and(|kind| kind == "rs")
        && !relative.starts_with("xtask/canaries/")
}

/// No source file turns a whole lint group off for itself.
///
/// # Errors
///
/// Returns why the sources could not be read.
pub(crate) fn no_source_puts_the_lints_down(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let mut findings = Vec::new();
    let mut read = 0_usize;
    // **The whole tree, not `src/`.** This walked `xtask/src` and each crate's
    // `src/`, which is 56 of the 83 Rust files here. The 27 it could not reach
    // are every `crates/*/tests/`, every `xtask/tests/` and the eleven
    // `crates/nc-log/fixtures/` — all compiled by cargo, all able to carry an
    // inner attribute. Measured: `#![allow(clippy::all, clippy::pedantic,
    // unused)]` plus four real violations in `crates/nc-journal/tests/
    // tampering.rs` — acceptance criterion 3's own suite — and the count did not
    // move: `ok 56 source file(s) leave the workspace lints on`, `ok lints`,
    // `check passed`. The violations were real; removing the allow line alone
    // turned `cargo clippy --tests` red four times.
    //
    // `crates/nc-log/fixtures/` is deliberately included: those files are
    // compiled by the wall gate, and one of them carrying a blanket allow is a
    // fixture that stops proving what it says it proves.
    {
        walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            let mut judged_here = 0_usize;
            // The second counter, for the reason the §15.1 walk carries: the
            // one above says a line became this gate's to judge, and a
            // `continue` written below it leaves that number whole while the
            // rule is never asked.
            let mut reached_a_verdict = 0_usize;
            // **An attribute is not a line.** This read one line and asked it to
            // both open and close, so rustfmt's own wrapping hid the defect:
            //
            // ```rust
            // #![allow(
            //     clippy::all,
            //     clippy::pedantic
            // )]
            // ```
            //
            // over `crates/nc-journal/tests/tampering.rs` — acceptance criterion
            // 3's suite — gave `ok 83 source file(s) leave the workspace lints
            // on` and `check passed`, with four real clippy violations
            // underneath. And rustfmt *produces* that shape once the list is
            // long enough, so the hole opens by itself.
            // The line the accumulator opened at, and how many lines it has taken
            // in. **The count is what makes the identity honest**: the loop
            // charges `judged_here` once per line, so an attribute that spans
            // four lines must return four verdicts when it finally balances, not
            // one. Written the other way the two counters could never be equal
            // whenever the accumulator was used at all — so ordinary rustfmt
            // output, `#![allow(\n    clippy::match_same_arms\n)]`, made this gate
            // fail with a message blaming a filter that does not exist. Measured:
            // `6 line(s) of this file became this gate's to judge and 4 reached a
            // verdict`, on a file whose only unusual property was being wrapped.
            let mut held: Option<(usize, String, usize, bool)> = None;
            // **An outer attribute on a `mod` covers a whole file.** This gate
            // read `#![` and nothing else, and the corpus case arguing for that
            // — "an outer allow is scoped to the item under it and is the
            // mechanism working" — chose `fn f() {}`, where it is true. On a
            // module declaration it is false: measured, one real
            // `clippy::uninlined_format_args` planted in
            // `crates/nc-journal/src/entry.rs` failed a cold clippy, and then
            // `#[allow(clippy::all, clippy::pedantic)]` written above
            // `pub mod entry;` in that crate's root left clippy silent and
            // `cargo xtask check` at `check passed` — with the run differing
            // from clean only by four counts going up by one, one of them the
            // count of source lines this gate says it judged.
            //
            // So an outer attribute is accumulated the same way, and judged only
            // once the item below it is known to be a module. They are held in a
            // list because attributes stack: `#[cfg(test)]` above
            // `#[allow(…)]` above `mod tests` is ordinary Rust.
            let mut awaiting: Vec<(usize, String, usize)> = Vec::new();
            // **The whole text through the stripper, not each line on its own.**
            // The sibling walk fifty lines up does exactly this, and for exactly
            // this reason: quote state has to survive a literal that opens on
            // one line and closes on another. Per line, a literal inside a
            // wrapped attribute never balances, the accumulator runs to its
            // bound, and the gate says "an inner attribute here never closes its
            // brackets" over an **outer** attribute that closes one line down —
            // a cause and a remedy that are both false, on a line whose author
            // did nothing wrong, while everything below it goes unjudged. One
            // rule written twice with the second copy the narrower, in one file,
            // fifty lines apart. Found by a reviewer in wave 21.
            let spent = without_string_contents(&text);
            for (index, line) in spent.lines().enumerate() {
                // At the top of the loop, for the reason written on the §15.1
                // walk above.
                judged_here += 1;
                // **`split("//")` is not a comment stripper, and this file says
                // so 1300 lines up.** An inner attribute whose string holds a
                // `//` — a URL in a `#![doc]`, which is ordinary Rust — was
                // truncated to an unbalanced fragment, and `held` then opened an
                // accumulator that can never close: from that line on, every
                // line in the file was *appended* rather than judged. Measured,
                // in `crates/nc-journal/tests/tampering.rs` — criterion 3's own
                // suite — one line of `#![doc = "… <https://example.invalid/…>"]`
                // above a real blanket allow:
                //
                // ```text
                // ok    71155 source line(s) leave the workspace lints on   <- up
                // ok    lints
                // check passed
                // ```
                //
                // and with only that `#![doc]` line removed, the same file gave
                // three findings and `check failed`. One line of ordinary Rust
                // turned the gate off for everything under it.
                //
                // The contents go before the comment: a `//` inside a literal
                // must not open a comment, and a bracket inside one must not
                // count toward `balanced`.
                let code = line_without_a_comment(line).trim().to_owned();
                let code = code.as_str();
                let (at, attribute, lines, outer) = match held.take() {
                    Some((began, mut so_far, lines, outer)) => {
                        so_far.push(' ');
                        so_far.push_str(code);
                        (began, so_far, lines + 1, outer)
                    }
                    None if code.starts_with("#![") => (index, code.to_owned(), 1, false),
                    None if code.starts_with("#[") => (index, code.to_owned(), 1, true),
                    // An ordinary line, and that is a verdict. Counted here,
                    // like every other way out of the body, so that a `continue`
                    // added later is the one exit that does not count.
                    None => {
                        // **And it is the item any outer attribute above was
                        // written on**, which is the only thing that decides
                        // whether that attribute reaches one item or a file. A
                        // blank line does not decide it, so the wait continues
                        // past one.
                        if !code.is_empty() {
                            for (began, attribute, lines) in awaiting.drain(..) {
                                reached_a_verdict += lines;
                                if a_module_declaration(code) {
                                    put_the_lints_down(
                                        root,
                                        path,
                                        began,
                                        &attribute,
                                        "everything in the module below it",
                                        &mut findings,
                                    );
                                }
                            }
                        }
                        reached_a_verdict += 1;
                        continue;
                    }
                };
                if !balanced(&attribute) {
                    // **A held line has not reached a verdict.** This counted one
                    // here, which is what made the two-counter identity vacuous
                    // for this gate: `judged_here` and `reached_a_verdict` rose
                    // together while every line went into a buffer nobody asked
                    // the rule. The identity now means what its name says.
                    //
                    // Bounded, because an accumulator that never closes would
                    // swallow the rest of the file — and the bound is a
                    // **finding**, not a silent drop. Four thousand characters of
                    // source went unjudged and the run stayed green, which is the
                    // one outcome a gate must not have.
                    if attribute.len() < 4_000 {
                        held = Some((at, attribute, lines, outer));
                    } else {
                        reached_a_verdict += lines;
                        findings.push(Finding {
                            file: relative_to(root, path),
                            line: at + 1,
                            what: "an inner attribute here never closes its brackets, so \
                                   this gate stopped reading the file"
                                .to_owned(),
                            fix: "the walk gives up after four thousand characters rather \
                                  than swallowing the rest of the file — so everything below \
                                  this line was never asked whether it puts the workspace \
                                  lints down. Close the attribute, or find the `[` or `(` \
                                  that is unmatched"
                                .to_owned(),
                        });
                    }
                    continue;
                }
                // An outer attribute is judged by the item below it, which has
                // not been read yet. Its lines are charged when that happens, so
                // the identity still counts every line exactly once.
                if outer {
                    awaiting.push((at, attribute, lines));
                    continue;
                }
                // Every line the attribute took in, because every one of them was
                // charged to `judged_here` on the way past.
                reached_a_verdict += lines;
                put_the_lints_down(
                    root,
                    path,
                    at,
                    &attribute,
                    "everything below it",
                    &mut findings,
                );
            }
            // An attribute still waiting at the end of the file is on no item at
            // all, so it puts nothing down — but its lines were charged to
            // `judged_here` and have to be answered for.
            for (_, _, lines) in awaiting.drain(..) {
                reached_a_verdict += lines;
            }
            let here = relative_to(root, path);
            everything_reached_a_verdict(
                "line",
                &here,
                judged_here,
                reached_a_verdict,
                &mut findings,
            );
            // Counted per line, for the reason written on the §15.1 walk above:
            // a filter inside the line loop left a per-file count at full
            // strength. Measured, `if index > 0 { continue; }` — which reads
            // *more* correct than the real rule, since an inner attribute is
            // near the top of a file — with a blanket allow over acceptance
            // criterion 3's own suite and `ok 83 source file(s)`.
            read += judged_here;
        })
        .map_err(|error| format!("{} could not be walked: {error}", root.display()))?;
    }
    Ok((findings, read))
}

/// A compose file beside the stack's own, which `.gitignore` says is refused.
///
/// **`.gitignore` said `cargo xtask check` refuses one and nothing did.**
/// `COMPOSE_FILES` had exactly two readers, both of which only *read* the files
/// for `image:` lines; no rule judged their existence. Measured, with an
/// override replacing both healthchecks with `test: ["CMD-SHELL", "true"]` —
/// the shape `dev` waits on, and the incident `dev.rs` records:
/// `docker compose --profile '*' config` showed both gutted, the walk saw the
/// file (`169 path(s)`, up from 168), and `check passed` byte-identical.
///
/// Every name after the first, because `docker compose` merges any of them
/// silently and the stack a developer runs is then not the stack this
/// repository's gates read.
pub(crate) fn a_second_compose_file(root: &Path) -> (Vec<Finding>, usize) {
    let mut findings = Vec::new();
    let mut looked = 0_usize;
    for named in COMPOSE_FILES.iter().skip(1) {
        looked += 1;
        if !root.join(named).exists() {
            continue;
        }
        let what = format!("`{named}` is beside `{}`", COMPOSE_FILES[0]);
        findings.push(Finding {
            file: (*named).to_owned(),
            line: 1,
            what,
            fix: "`docker compose` merges this into the stack without saying so, so what \
                  `cargo xtask dev` brings up stops being what the pinned digests, the \
                  loopback rule and the healthcheck gate read. `.gitignore` says this \
                  file is refused. Fold what it does into docker-compose.yml, or keep it \
                  outside the repository"
                .to_owned(),
        });
    }
    (findings, looked)
}

/// A committed `.env` carries real credentials into every clone.
///
/// Walked at any depth rather than checked at the root, because `NOCTUA.md
/// §10.2` puts service directories in this tree and each service will want one.
pub(crate) fn committed_env_files(root: &Path) -> Result<(Vec<Finding>, usize), String> {
    let mut found = Vec::new();
    // How many directories were walked, so a walk that reached nothing is visible
    // rather than green. `check.rs`'s own header says every step reports what it
    // looked at, and this one printed only its verdict.
    // **Counted where it is judged, not where it is visited.** The increment was
    // the first line of the visitor, so a filter placed one line after it left
    // this number at full strength while the gate judged nothing: measured, a
    // `crates/`-shaped early return took the walk from judging 153 paths to
    // judging none, and the printed count went *up* to 154 — because the secret
    // file itself was one of the paths visited. This is the one number a reader
    // uses to conclude the walk reached everything.
    let mut walked = 0_usize;
    walk(root, &mut |path| {
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };
        // **Reported in any case, agreed with `.gitignore` in one.**
        // `holds_an_environment` is case-sensitive because git is, and the test
        // that binds the two exists to keep them saying the same thing — so the
        // case fold lives here, where nothing is compared with git. Measured,
        // an upper-cased env file under `policy/core/` holding a password:
        // `check passed`, and the whole run differed from a clean one by one
        // number, going *up* — `no .env is committed, in 169 path(s)` instead of
        // 168. The two rules this gate's own comment credits as the fail-safe
        // both miss it: `policy` is the one tree of the eight with allowed kinds
        // and no row in `ONLY_UNDER_THIS_NAME`, so such a name wears a kind
        // that tree may hold.
        if holds_an_environment(name) || holds_an_environment(&name.to_lowercase()) {
            found.push(Finding {
                file: relative_to(root, path),
                line: 1,
                what: format!("`{name}` is committed here"),
                fix: "it holds real credentials, and NOCTUA.md §15.5 puts secrets in \
                      ephemeral environment variables at the process boundary rather than \
                      in a file a clone could carry. Move it outside the tree, or export \
                      the variables, and delete this file"
                    .to_owned(),
            });
        }
        // **Last, so nothing can return between the verdict and the count.**
        // Moving it below the `file_name` guard was not enough: a filter placed
        // one line further down kept the number at full strength while the gate
        // judged nothing, and the count went *up* — 153 to 154 — because the
        // committed secret was itself a visited path. A counter anywhere but the
        // end of a visitor is a counter something can be inserted above.
        walked += 1;
    })?;
    Ok((found, walked))
}

/// The formats an environment is written in.
///
/// **The list this replaced was every file kind the repository holds**, so an
/// `env` file wearing toml, json, sh or yaml was read as "a module named `env`"
/// — the exemption the paragraph below writes for a Rust module. Measured, one
/// such file under `policy/` holding `DATABASE_URL` and
/// `AWS_SECRET_ACCESS_KEY`: `check passed`, exit 0, online and `--offline`
/// both, and the whole run differing from a clean one by a single number, the
/// path count going up by one. The gate enumerated the credential file and
/// reported only that it had counted one more.
///
/// These five are how an environment is actually written; `md`, `sql`, `rs`,
/// `lock`, `cedar` and the rest stay modules of that kind, which is what the
/// exemption is for.
const AN_ENVIRONMENT_IS_WRITTEN_IN: [&str; 5] = ["toml", "json", "yaml", "yml", "sh"];

/// Every file kind this repository holds.
///
/// The exemption below needs both lists: a suffix that is not a kind at all —
/// `env.production` — was never a module, and a suffix that is a kind but is
/// how an environment is written is not one either.
///
/// `env.rs` is a Rust module and `env.toml` a manifest. Both have `env` as a <!-- allow-reference: names `env.rs` `env.toml` — names a shape the rule judges, not a file here -->
/// dot-separated part and neither holds a shell's environment; an environment
/// file's qualifiers — `local`, `production`, `private` — name no format.
const KNOWN_FORMATS: [&str; 12] = [
    "rs",
    "toml",
    "md",
    "yml",
    "yaml",
    "json",
    "sql",
    "sh",
    "lock",
    "cedar",
    "cedarschema",
    "tsv",
];

/// Whether a file name is one that carries an environment.
///
/// **`.env` was the whole rule**, and `.env.example` says of it: *"The gate is
/// absolute so that nobody has to remember which `.env` was the safe one."* It
/// was exactly a gate that required remembering. `.env.local`, `.env.production`
/// and `services/auth/.env.local` are all conventional names for a file holding <!-- allow-reference: names `services/auth/.env.local` — the shapes the gate missed, none of which is in this tree -->
/// real credentials, none of them is in `.gitignore`, and none was caught — so
/// three of them would go into a clone with `check passed`.
///
///
/// The rule is now the shape rather than the spelling: anything beginning `.env`
/// is one, except the committed examples, which are the point of having them.
fn holds_an_environment(name: &str) -> bool {
    // **`production.env` is an environment file and `.env` is a spelling.**
    // This matched a `.env` prefix, so `production.env` at the root and `env`
    // beside a crate's source both went through holding credentials — and the
    // path count went *up*, because the walk read them and the gate did not
    // judge them. `env_file:` in a compose file names exactly that shape, and
    // `.gitignore` has the same blind spot, so neither this nor the ignore file
    // stops one reaching a clone.
    //
    // The shape is: the whole name is `env`, or it begins `.env`, or it ends
    // `.env`. A Rust source file named after the word is none of those,
    // because the
    // extension is what it ends with.
    // **Any component of the name, not its first or last.** The rule was
    // "starts with `.env`, or ends with `.env`, or is `env`", minus five exact
    // spellings — and it was wrong in both directions. `secrets.env.local` and
    // `env.production` hold credentials and fell between the clauses;
    // `.env.local.example` is a template and fell inside the wrong one: allow-reference: names `.env.local.example` — a name the rule judges, not a file here
    //
    // ```text
    // .env.local.example   -> error: it holds real credentials
    // env.production       -> ok  no .env is committed
    // secrets.env.local    -> ok  no .env is committed
    // ```
    //
    // A name is a run of dot-separated parts. It is an environment file when one
    // of them is `env`, and a template when one of them says so — which is the
    // shape, and holds for every arrangement of the two.
    // **No empty-part filter, because the three guards below dominate it.**
    // `split('.')` yields an empty part only from a leading dot, a trailing dot
    // or a doubled one, and the format exemption — the one clause `parts.len()`
    // reaches — rejects all three before it counts. Nothing else reads a part
    // that could be empty: `""` is neither `env` nor a template word. The filter
    // was one rule written twice, and `mutants.py` reported deleting it as a
    // weakening no input distinguishes. Checked before removing it: both forms
    // driven over 54,240 generated names — every arrangement of up to four parts
    // drawn from an alphabet holding the empty string, `env`, `envrc`, four
    // formats, three template words and three qualifiers — with no disagreement.
    // **Case-sensitive, because git is.** Lower-casing the name here was tried
    // and reverted: it made this function report a `.ENV` that `.gitignore` does
    // not hide, and `the_ignore_file_hides_exactly_what_the_gate_reports` exists
    // to say the two agree. Teaching the ignore file character classes does not
    // work either — this repository's own matcher cannot read them, so the two
    // halves would disagree in the other direction.
    //
    // A `.ENV` is reported by `committed_env_files`, which folds the case
    // itself — see the paragraph there. Those two rules catch a bare `.ENV` and
    // most spellings one directory down, but not one wearing a kind its tree
    // allows, which is why the fold is in the reporter rather than here.
    let parts: Vec<&str> = name.split('.').collect();
    // **A part equal to `env`, or a name that begins `.env`.** The docstring
    // above says "anything beginning `.env` is one" and the rule asked only the
    // first question — so `.envrc`, which splits to `["envrc"]`, was not an
    // environment file to this gate. That is direnv's standard file and the
    // commonest non-`.env` spelling of exactly what `§15.5` forbids; `.gitignore`
    // had the same blind spot, and the test that holds the two together listed
    // nineteen names and not that one, so both defences were off at once.
    // Measured: `.envrc` at the repository root holding a database URL with a
    // password and an `AWS_SECRET_ACCESS_KEY` gave
    // `ok no .env is committed, in 165 path(s)` — the count *up* — and
    // `check passed`, and `git check-ignore` would not have hidden it either.
    if !parts.contains(&"env") && !name.starts_with(".env") {
        return false;
    }
    // **A file with a type is that type.** `env.rs` is a Rust module and allow-reference: names `env.rs` — a name the rule judges, not a file here
    // `env.toml` a manifest; neither holds a shell's environment, and both have allow-reference: names `env.toml` — a name the rule judges, not a file here
    // `env` as a part. What separates them from `env.production` is that the
    // last part names a format — an environment file's qualifiers do not.
    //
    // **And the whole name is `env` and a format, not merely ending in one.**
    // `sh`, `json`, `yaml` and `toml` are exactly the formats an environment is
    // *written* in, and `source .env.sh` is a conventional way to load one — so
    // "a file ending in a known format is that format" exempted the very files
    // this gate exists to catch. Measured, two of them at the repository root:
    //
    // ```text
    // .env.sh    export DATABASE_URL=postgres://noctua:hunter2@db.internal/noctua
    // .env.json  {"DATABASE_PASSWORD": "hunter2"}
    //
    // ok    no .env is committed, in 165 path(s)     <- up from 163
    // check passed
    // ```
    //
    // A Rust module named `env` is a module because it *is* `env`, of that type.
    // A dotfile called `.env` with `.sh` after it is an environment with a
    // format, and so is `secrets.env.sh`: what tells them apart is whether
    // anything precedes the `env`. <!-- allow-reference: names `secrets.env.sh` —
    // a shape the rule judges, not a file here -->
    //
    // **The leading dot is what tells them apart, and it was filtered away one
    // line up.** A dotfile called `.env` with a format after it and a module
    // called `env` of that format both became `["env", "sh"]`, so the exemption
    // written for the module covered the environment. A name beginning with a
    // dot is a dotfile, and `.env` with any suffix is an environment.
    // **And no empty part anywhere**, which the filter above hides. A doubled dot
    // and a trailing dot both collapse to two parts, so the module exemption
    // fired and this said "not an environment file" — while the ignore file's
    // wildcard matches them and its negation does not, so git ignores them.
    // Driven over 115,454 generated names against real `git check-ignore`: 48
    // disagreements, every one of that shape, and the twenty-one-name agreement
    // test covered none of them. A doubled or trailing dot is not a file type.
    //
    // **Written as the shape, not as four questions about a length.**
    // `parts.len() == 2 && parts.first() == Some(&"env") && parts.last()
    // .is_some_and(…)` asked whether an `Option` was `None` behind a guard that
    // makes it `Some` — `mutants.py` reported `is_some_and -> is_none_or` as a
    // weakening no input distinguishes, which is what a dead branch looks like
    // from the outside.
    // **And the three dot guards are gone with it.** `!starts_with('.')`,
    // `!contains("..")` and `!ends_with('.')` were written to stop a name with an
    // empty part from reaching a length test that the filter had shortened. With
    // the parts unfiltered, every such name has three or more of them and the
    // pattern below does not match — so the guards decided nothing, and
    // `mutants.py` reported deleting each of them as a weakening no input
    // distinguishes. Both forms driven over the same 54,240 names: no
    // disagreement.
    // **A module, not every kind this repository holds.** The list this asked
    // was `KNOWN_FORMATS` — twelve file kinds — so an `env` file wearing any of
    // toml, json, sh or yaml was "a module named `env`". Measured, one such
    // file under `policy/` holding `DATABASE_URL` and `AWS_SECRET_ACCESS_KEY`:
    // `check passed`, exit 0, online and `--offline` both, and the entire run
    // differed from a clean one by a single number —
    // `no .env is committed, in 169 path(s)` instead of 168. The gate
    // enumerated the credential file and reported only that it had counted one
    // more.
    //
    // The paragraph above says what the exemption is for: *"A Rust module named
    // `env` is a module because it **is** `env`, of that type."* A module is
    // written in `rs`. Nothing else named `env.<kind>` is more likely a module
    // than an environment, and the list had eleven other kinds in it. Removing
    // the exemption altogether was measured too: `ok no .env is committed, in
    // 168 path(s)`, unchanged — nothing in this tree relies on it.
    if matches!(parts.as_slice(), ["env", last]
        if KNOWN_FORMATS.contains(last) && !AN_ENVIRONMENT_IS_WRITTEN_IN.contains(last))
    {
        return false;
    }
    !parts
        .iter()
        .any(|part| matches!(*part, "example" | "sample" | "template" | "dist"))
}

/// How many entries a `*`-glob matches under `directory`, at the depth it names.
///
/// For the counting gate's own floor: the noun's directory and glob reached
/// nothing but a `fix` string, so a directory that does not exist and a glob
/// that matches no file both read as a rule.
///
/// **The summary above this one named a function that is gone.** It said *"how
/// many files under `directory` end in `suffix`"*, which this has not done
/// since it began reading a glob segment by segment — and a block whose item
/// has been deleted fuses onto whatever is written next, where rustdoc renders
/// it as that item's summary.
///
/// `*.cedar` is the files directly in it; `*/Cargo.toml` is one directory down.
/// Segment by segment, because taking the text after the last dot and looking
/// for it anywhere beneath made two of the six `COUNTABLE` rows pass while the
/// command they hand a reader matched nothing.
#[cfg(test)]
pub(crate) fn glob_count(directory: &Path, glob: &str) -> usize {
    let mut here = vec![directory.to_path_buf()];
    let mut segments = glob.split('/').peekable();
    let mut found = 0_usize;
    while let Some(segment) = segments.next() {
        let last = segments.peek().is_none();
        let mut next = Vec::new();
        for path in &here {
            let Ok(entries) = std::fs::read_dir(path) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let matches = match segment.strip_prefix('*') {
                    Some(tail) => name.ends_with(tail),
                    None => name == segment,
                };
                if !matches {
                    continue;
                }
                if last {
                    found += 1;
                } else {
                    next.push(entry.path());
                }
            }
        }
        here = next;
    }
    found
}

/// The glob's depth, which every caller of it depends on.
#[cfg(test)]
mod a_glob_counts_at_the_depth_it_names {
    /// A segment before the last one steps down; the last one counts.
    ///
    /// **The depth was decided by nothing.** `let last = …` written `true`
    /// counts every match at every level, and `false` counts nothing at all —
    /// and every caller of this function is a `COUNTABLE` row whose directory
    /// happens to answer the same number either way, so the whole tree stayed
    /// green under both. The rows this serves are the commands a reader is
    /// handed: `ls crates/*/Cargo.toml` is one directory down and
    /// `ls catalog/*.toml` is not.
    #[test]
    fn a_segment_before_the_last_steps_down() {
        let scratch = std::env::temp_dir().join(format!("nc-glob-depth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        for at in ["one", "two"] {
            std::fs::create_dir_all(scratch.join(at)).expect("writable");
            std::fs::write(scratch.join(at).join("Cargo.toml"), "").expect("writable");
            std::fs::write(scratch.join(at).join("other.txt"), "").expect("writable");
        }
        std::fs::write(scratch.join("Cargo.toml"), "").expect("writable");

        assert_eq!(
            super::glob_count(&scratch, "*/Cargo.toml"),
            2,
            "one directory down is the two inside, and not the one at the top"
        );
        assert_eq!(
            super::glob_count(&scratch, "*.toml"),
            1,
            "directly in it is the one at the top, and not the two inside"
        );
        assert_eq!(
            super::glob_count(&scratch, "*/other.txt"),
            2,
            "a second name one directory down"
        );
        assert_eq!(
            super::glob_count(&scratch, "*/nothing.here"),
            0,
            "a glob that matches nothing answers nothing"
        );
        // And a `*` is a *suffix* match: `*.toml` is what a name ends in, not
        // what it begins with. Widened to accept either, `other.txt` answers
        // `*other` and the count is about a different set of files.
        assert_eq!(
            super::glob_count(&scratch, "*/*oth"),
            0,
            "`*oth` is a name *ending* in `oth`, and `other.txt` only begins with it"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }
}

/// Every `Cargo.toml` in the tree, in path order.
pub(crate) fn manifests(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut found = Vec::new();
    walk(root, &mut |path| {
        if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            found.push(path.to_path_buf());
        }
    })?;
    found.sort();
    Ok(found)
}

/// Whether a `target` directory in `parent` is the one cargo writes.
///
/// **Beside the *workspace* manifest, not beside any manifest.** The first form
/// of this rule asked whether a `Cargo.toml` sat in the parent, and one sits
/// beside nine of the directories in this tree — so a `target` under any member
/// became a place nothing looks. Measured, the same file in two directories:
///
/// ```text
/// a committed `.env` under migrations/  -> error: it is committed here
/// the same file under a member's build  -> ok  no .env is committed, in 151 paths
/// ```
///
/// Byte-identical files, opposite verdicts, and the path count did not move.
/// Every walk-based gate in the tool reads through here, so that was a hiding
/// place for a committed secret, an unread citation, a compile-time query and a
/// logging call site at once.
///
/// Cargo puts a workspace's build output in **one** directory, beside the
/// manifest that declares `[workspace]`; a member package inside a workspace
/// gets none of its own. Measured on this tree: `ls -d crates/*/target
/// xtask/target` finds zero. So the question is not "is there a manifest here"
/// but "is this the workspace's own directory", and a nested workspace answers
/// it for itself.
fn is_a_build_directory(parent: &Path) -> bool {
    // **A `[workspace]` line, not a `[workspace]` table.** This read the
    // manifest as lines and asked whether any trimmed to `[workspace]` — and a
    // manifest is text an author controls. The line inside a TOML *string*
    // satisfies it while leaving a perfectly valid member manifest:
    //
    // ```toml
    // description = """
    // The lint stanza below is the one declared by the root manifest's
    // [workspace]
    // table, so this crate takes it rather than declaring its own.
    // """
    // ```
    //
    // `cargo metadata` accepts it, and a `target/` inside `nc-log` then became a
    // hiding place both this predicate and `census` skip together — a `.env`
    // holding a production password and a `String::leak` inside the crate whose
    // whole purpose is that no run-time string reaches the log, with the
    // transcript byte-identical. The floor cannot see it precisely *because*
    // both sides share the predicate.
    //
    // So the question is asked of the parser, not of the text. `[workspace]` at
    // the top level is a table; the same characters inside a string are a
    // string, and `toml` knows the difference.
    std::fs::read_to_string(parent.join("Cargo.toml")).is_ok_and(|manifest| {
        toml::from_str::<toml::Table>(&manifest)
            .is_ok_and(|parsed| parsed.contains_key("workspace"))
    })
}

/// Whether a directory on [`SKIP`] is, here, a place this repository's own files
/// could sit.
///
/// **All three names were matched at any depth, and only `target` was anchored.**
/// The same measurement that closed `target` reopens for the other two: a
/// committed `.env` under a `node_modules` or a `.git` inside a crate's own
/// sources is invisible to every walk-based gate, with the path count unmoved —
///
/// ```text
/// walked=151 findings=[]                        both files present
/// walked=152 findings=[".../.env"]              the same file one directory up
/// ```
///
/// `.git` is anchored the way `target` is: this repository's own git directory
/// sits beside the manifest that declares `[workspace]`, and that one is git's
/// object store, not source. A `.git` anywhere else — inside a crate's sources,
/// under a fixture — is not this repository's history; it is a directory with a
/// familiar name, and a committed `.env` under it would be invisible to every
/// walk-based gate. `node_modules` is somebody else's tree in a repository that
/// has one, and this has none — so here it is a directory with a familiar name.
///
/// `COVERED`'s two tests cannot see any of this: they prove that at least one
/// file under each covered *path* is reached, and a hiding place is a path they
/// do not list.
fn is_somewhere_source_can_hide(parent: &Path, name: &str) -> bool {
    match name {
        // Cargo writes one build directory per workspace, beside the manifest
        // that declares `[workspace]`; a member inside a workspace gets none.
        // Git keeps one directory per checkout, beside the same manifest at the
        // repository's root; any other `.git` in the tree is not this
        // repository's and is walked like any directory.
        "target" | ".git" => !is_a_build_directory(parent),
        // A node package tree sits beside the node manifest that declares it.
        // **Nothing here is a node package.** This asked whether a node manifest
        // sat beside the directory — and this is a Rust monorepo with no such
        // manifest anywhere and no gate on one, so *adding* one made
        // `node_modules` a hiding place again: two lines took the `.env` gate,
        // the type-wall source gate, the compile-time-query gate, the
        // ignored-test gate, the citation gates and the excuse census off
        // everything inside it, with the path count going *up* because it
        // counted the manifest.
        //
        // `§10.2` maps what this repository holds and no part of it is a node
        // package. The day one arrives, the map changes and so does this line.
        // A clone would carry one, and it can hold a committed secret the gates
        // would then never read.
        "node_modules" => true,
        _ => false,
    }
}

/// How many Rust lines there are, counted by code that is not a gate.
///
/// **`> 0` is a floor with everything in it.** Two gates print a number of source
/// files read and both were guarded by nothing else, so a one-line early return
/// inside either left `ok 1 source file(s)` with a real defect underneath — a
/// blanket `#![allow(clippy::all)]` over acceptance criterion 3's own tamper
/// suite, and a hand-written `NC-KERN-4242` in no registry. This is the second
/// count the repository already uses for the path walk, restricted to the files
/// those two gates are about.
pub(crate) fn rust_lines(root: &Path, except: &Path) -> Result<usize, String> {
    lines_under(root, &["rs"], except)
}

/// The same, over whichever file kinds the walk being floored actually reads.
///
/// **A ratio is not a floor when a whole crate fits inside it.** The `§15.1`
/// floor allowed half, and one line — `if relative.starts_with("crates/
/// nc-journal/") { return; }` — removed 31 % of the walk with a real
/// `NC-KERN-4242` in the crate it skipped, printing `ok 17464 source line(s)`
/// and `check passed`. The journal is the audit log. What made the ratio
/// necessary was that the expected count was over the wrong ground: `.rs` only,
/// while the walk also reads every `.sql` in `migrations/`, and the whole of
/// `crates/` while the walk names two exclusions. Counted over the same ground,
/// the two agree exactly and the floor needs no slack.
///
/// # Errors
///
/// Returns why a directory could not be read.
pub(crate) fn lines_under(root: &Path, kinds: &[&str], except: &Path) -> Result<usize, String> {
    let mut found = 0_usize;
    // **A file is a scope too.** `read_dir` errors on one, and every caller
    // drops the error with `.ok()` — so naming a file in a floor's scope list
    // silently contributed zero. The emitter's floor named
    // `xtask/src/build.rs`, the stage that writes the bytes, and counted none
    // of its 268 lines: `read = 7148` against `expected = 6880`, which is
    // exactly that file, free.
    if root.is_file() {
        return Ok(std::fs::read_to_string(root)
            .map(|text| text.lines().count())
            .unwrap_or_default());
    }
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("could not read {}: {error}", root.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("could not read an entry: {error}"))?
            .path();
        // **The same rule `walk` uses, not a list of names.** `walk` descends
        // into a `.git`, a `node_modules`, or a `target` that is not beside a
        // `[workspace]` manifest — its own doc calls them places source can
        // hide — and this skipped all three unconditionally. Every `.rs` line
        // inside one therefore counted in the walk and not in the floor, which
        // is free slack in the one comparison called exact: measured, 2000
        // lines in a `.git` directory under a crate's `src/` paid for a filter
        // that removed `crates/nc-canon/src/` from the §15.1 walk with a real
        // `NC-KERN-4242` in it, and `check passed`.
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path == except {
            continue;
        }
        if path.is_dir() && SKIP.contains(&name) && !is_somewhere_source_can_hide(root, name) {
            continue;
        }
        if path.is_dir() {
            found += lines_under(&path, kinds, except)?;
        } else if path
            .extension()
            .is_some_and(|kind| kinds.iter().any(|wanted| kind == *wanted))
        {
            // **Lines, not files.** A per-file count is at full strength while
            // a filter inside a line loop judges nothing, which is how two gates
            // reported the whole tree and read one line of it.
            found += std::fs::read_to_string(&path)
                .map(|text| text.lines().count())
                .unwrap_or_default();
        }
    }
    Ok(found)
}

/// How many files are under `root`, counted by an enumerator that excludes
/// nothing of its own.
///
/// **A floor on the walk, not on a slice of it.** `every_member_was_walked`
/// compares 33 paths — every cargo `src_path` plus four by name — against a walk
/// that visits 153, so 120 were bounded by nothing. Two lines inside [`walk`],
/// one skipping `fixtures` and one marker excusing the two citations that named
/// it, took twelve files out of every walk-based gate with the floor green.
///
/// **And it shared its exclusions with the walk, which made it a mirror.** The
/// first version called `SKIP` and `is_somewhere_source_can_hide`, so a filter
/// expressed through either moved both counts together. A reviewer added
/// `if name == "vendor" { return true; }` to a helper both consulted, put a
/// `.env` holding a production password and a signing key in
/// a `vendor` directory inside a crate's sources, and the whole transcript came back
/// **byte-identical** — `ok no .env is committed, in 153 path(s)`, with that
/// directory simultaneously invisible to the citation gates, the type wall, the
/// compile-time-query gate and the hatch census.
///
/// So this shares nothing. It excludes the path cargo names —
/// `target_directory` from `cargo metadata` — and what sits beside the
/// workspace manifest as a checkout's own: cargo's build directory and git's
/// `.git`, judged by the directory's property below, not by `walk`'s list.
/// Every other name `walk` skips — a `node_modules`, a `.git` inside a crate's
/// sources — is counted here. What cannot happen is a *new* exclusion
/// appearing on one side only.
///
/// # Errors
///
/// Returns why a directory could not be read.
pub(crate) fn census(root: &Path, except: &Path) -> Result<usize, String> {
    let mut found = 0_usize;
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("could not read {}: {error}", root.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("could not read an entry: {error}"))?
            .path();
        // **Every cargo build directory, by the same rule `walk` uses.** This
        // excluded one hard-coded path, so a *nested* workspace's `target/` was
        // skipped by `walk` and counted here — two files of ordinary build
        // output made the floor accuse its own walk of filtering. And the one
        // excluded path was a blind spot on both sides: a `.env` holding a
        // password and a live `Box::leak` inside `target/` left the whole
        // transcript byte-identical, which is the shape this design exists to
        // prevent, reached through the hole the design concedes.
        //
        // Sharing `is_a_build_directory` is not the mirror the paragraph above
        // warns against: what must not be shared is a *list of names*, which is
        // where a new exclusion can be added on both sides at once. This is a
        // property of the directory — a `target` beside a `[workspace]`
        // manifest — and cargo, not this tool, decides it. Git's `.git` beside
        // the same manifest is the checkout's own metadata by the same
        // property, whether it is a directory or a linked worktree's file.
        let named = path.file_name().unwrap_or_default().to_string_lossy();
        if path == except
            || (named == "target" && path.is_dir() && is_a_build_directory(root))
            || (named == ".git" && is_a_build_directory(root))
        {
            continue;
        }
        if path.is_dir() {
            found += census(&path, except)?;
        } else {
            found += 1;
        }
    }
    Ok(found)
}

/// Everything this gate took in reached a verdict.
///
/// **A counter says a line became the gate's to judge; a filter one line below it
/// leaves that number whole while the rule is never asked.** Measured on the
/// citation walks: three one-line filters naming real directories, three real
/// defects in the tree, every printed count *up*, and `check passed`. A second
/// counter on every path out of the judgement cannot be true when something was
/// skipped, which is what no position for a single counter can do.
///
/// **One function, because it was five.** `gates.rs` held two inline copies,
/// `determinism.rs` a third, and `sections.rs` and `references.rs` a
/// byte-identical `every_citation_reached_a_verdict` each. Five copies of one
/// rule, and no test anywhere had ever handed any of them two numbers that
/// disagree — `!=` widened to `> judged + 100` left every one of them silent.
/// `the_identity_is_an_identity` in `corpus.rs` is the negative case.
///
/// `subject` is the word the count is in — `line`, `citation` — because the
/// sentence is read by somebody who has to go and look.
pub(crate) fn everything_reached_a_verdict(
    subject: &str,
    file: &str,
    judged: usize,
    reached: usize,
    findings: &mut Vec<Finding>,
) {
    if reached == judged {
        return;
    }
    findings.push(Finding {
        file: file.to_owned(),
        line: 1,
        what: format!(
            "{judged} {subject}(s) became this gate's to judge and {reached} reached a verdict"
        ),
        fix: "a filter between the counter at the top of the loop and the judgement at \
              the bottom leaves every printed number whole while the rule is never asked \
              — measured, with a hand-written code in the crate it skipped and every \
              count *up*. The two counters are here so that cannot be true; if something \
              legitimately needs skipping, skip it before the first one"
            .to_owned(),
    });
}

/// Whether the walk steps over a directory of this name under `root`.
///
/// **One rule, because a second copy of it disagreed.** `counts.rs` skipped
/// `target`, `.git` and `node_modules` by *name at any depth* while this skips
/// `target` only where cargo builds — the distinction the paragraph below
/// records, and a real one, since a Rust module directory may be called
/// anything. So a `README.md` under a module directory of that name inside this
/// crate's sources, which every other gate accepts, was inside the citation walk
/// and outside the floor that is supposed to be a second reading of it:
/// measured on 2026-09-12,
/// `error paragraphs: 2198 were judged and a second count of the same ground
/// says 2195`, and `check failed` on a tree with no defect in it.
pub(crate) fn is_skipped(root: &Path, name: &str) -> bool {
    SKIP.contains(&name) && !is_somewhere_source_can_hide(root, name)
}

/// Every file under `root`, at any depth, with build output skipped.
///
/// The one walk every gate in this crate uses, so "the tree" means the same set
/// of files to all of them — and so a directory excluded once is excluded
/// everywhere rather than in whichever gate remembered.
///
/// Every file under `root` that is not build output, in no particular order.
///
/// **Its doc was taken by the function inserted above it.** `everything_reached_
/// a_verdict` was written in the same edit that consolidated five copies of the
/// two-counter identity, and a doc comment attaches to the next item — so the
/// paragraph describing this walk ended up describing that one, and this was
/// left with none. The sixth time in this repository, which is why
/// `every_item_in_this_crate_is_documented` now exists — and the seventh was
/// this same block, taken by `is_skipped` when that was inserted above this
/// one. The gate named it in one line; nothing else did.
pub(crate) fn walk(root: &Path, visit: &mut dyn FnMut(&Path)) -> Result<(), String> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("could not read {}: {error}", root.display()))?;
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("could not read an entry: {error}"))?;
        children.push(entry.path());
    }
    children.sort();
    for path in children {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if path.is_dir() {
            // **`target` is build output only where cargo builds.** A skip
            // matched by name at any depth, and a Rust module directory may be
            // called anything — `nc-prune` already has a `Target` trait, so a
            // module directory of that name is not hypothetical. Measured with
            // an ordinary logging call site in a module directory of that name
            // inside a crate's sources: the
            // type-wall gate read 18 files instead of 19, `no_compile_time_query`
            // never saw it either, and `check passed`; the byte-identical file
            // one directory up was a finding. `SKIP`'s own doc says every entry is
            // "build output or somebody else's tree", and a module of this
            // repository's own source is neither.
            //
            // Cargo's build directory sits beside a manifest, which is the fact
            // that separates the two, and git's sits beside the same one.
            // `node_modules` stays matched at any depth: it is a whole tree
            // wherever it appears.
            if is_skipped(root, &name) {
                continue;
            }
            walk(&path, visit)?;
        } else if !(name == ".git" && is_a_build_directory(root)) {
            // A linked worktree's `.git` is a one-line file naming where its
            // git directory is, and it sits where the directory would: it is
            // the checkout's own metadata, not one of its files.
            visit(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    /// A wrapped attribute is one attribute, and a stray `]` is not one at all.
    ///
    /// **The repair had no case, in three copies.** It was written after a plant
    /// in `nc-journal` on 2026-09-13 and neutered in all three a day later with
    /// every suite green — `nc-journal` has no multi-line attribute in it, and
    /// neither does the tree this copy reads, so the branch decided nothing
    /// anywhere. The bound is what stops a stray `]` above an item walking the
    /// reader out of the block it is reading, and it is written down here too.
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
            super::past_the_attributes(&wrapped, wrapped.len() - 1),
            3,
            "the walk stopped at the continuation of a wrapped attribute rather than at \
             its first line"
        );
        let one = format!(
            "/// The summary.\n{}\nfn item() {{}}",
            ["#", "[inline]"].join("")
        );
        let plain = lines(&one);
        assert_eq!(super::past_the_attributes(&plain, plain.len() - 1), 1);
        // A closing bracket that belongs to an array, not to an attribute: the
        // walk stops rather than climbing out of the item it is reading.
        let array = lines("const A: [u8; 1] = [\n    1,\n];\nfn item() {}");
        assert_eq!(
            super::past_the_attributes(&array, array.len() - 1),
            array.len() - 1
        );
        // **A doc line ending in an intra-doc link is not an attribute's last
        // line.** `/// See [`other`]` ends in `]`, and the fallback walked out
        // of the item, across a blank line and a whole other item, to land on
        // an unrelated `#[inline]`. Found in wave 24.
        let linked = lines("#[inline]\nfn other() {}\n\n/// See [`other`]\nfn item() {}");
        assert_eq!(
            super::past_the_attributes(&linked, linked.len() - 1),
            linked.len() - 1,
            "a doc line ending in an intra-doc link walked the reader out of the block \
             it is reading"
        );
        // **The first repair to that was too strong, and this is the case that
        // measured it.** A wrapped attribute list may hold a blank line, and
        // stopping the inner walk at one made the item lose its doc block
        // entirely. Found in wave 25.
        let spaced =
            lines("/// Doc.\n#[doc(\n    alias = \"a\",\n\n    alias = \"b\"\n)]\nfn item() {}");
        assert_eq!(
            super::past_the_attributes(&spaced, spaced.len() - 1),
            1,
            "a blank line inside a wrapped attribute list cost the item its doc block"
        );
        // And the two spellings that were still too weak: a `//` comment ending
        // in `]`, and a macro call that does.
        for (what, text) in [
            (
                "a plain comment ending in a bracket",
                "#[inline]\n// see [the rfc]\nfn item() {}",
            ),
            (
                "a macro call ending in a bracket",
                "#[inline]\nthing![a, b]\nfn item() {}",
            ),
        ] {
            let held = lines(text);
            assert_eq!(
                super::past_the_attributes(&held, held.len() - 1),
                held.len() - 1,
                "{what} was read as a wrapped attribute's last line"
            );
        }
    }

    /// What a fixture crate's manifest declares about the doors feature.
    enum Manifest {
        /// A `[features]` block naming it, which is what the rule is about.
        Feature,
        /// The same name under `[dependencies]`, which is not that.
        Dependency,
        /// Neither.
        Bare,
    }

    /// A door behind a wrapped attribute is marked and said, and read as both.
    ///
    /// **The instruction told its reader to add what was already there.** The
    /// prologue walk climbed while each line opened `///` or `#[` or was blank,
    /// and a wrapped attribute ends on a line that is none of those — so a door
    /// carrying both its `#[doc(hidden)]` and its reason came back with an
    /// empty prologue and was refused with "a `_for_tests` door is not marked
    /// as one. Mark it, and say above it what it is for". The correct reader
    /// was in this file behind `#[cfg(test)]`; see [`past_the_attributes`].
    ///
    /// The second door is the floor: the walk must still refuse a door that is
    /// genuinely unmarked, wrapped attribute or not, or this fixture would pass
    /// for having stopped reading.
    #[test]
    fn a_door_marked_behind_a_wrapped_attribute_is_read_as_marked() {
        let held = crate::check::LeavesNothingBehind::new("wrapped-door");
        let root = held.root.clone();
        let directory = root.join("crates/nc-wrapped");
        std::fs::create_dir_all(directory.join("src")).expect("writable");
        std::fs::write(
            directory.join("Cargo.toml"),
            format!(
                "[package]\nname = \"nc-wrapped\"\n\n[features]\n{} = []\n",
                super::DOORS_ARE_BEHIND
            ),
        )
        .expect("writable");
        std::fs::write(
            directory.join("src/lib.rs"),
            concat!(
                "/// A door a test reaches a private thing through, and why.\n",
                "#[doc(hidden)]\n",
                "#[cfg_attr(\n",
                "    docsrs,\n",
                "    doc(cfg(feature = \"test-support\"))\n",
                ")]\n",
                // **The `cfg` itself wrapped, which is the third question.**
                // `marked` and `said` need only some line of the prologue;
                // this one needs `#[cfg(` and the feature name on one, and
                // rustfmt puts them on different ones — so the spelling this
                // rule's own remedy asks for was judged as being in the
                // default build.
                "#[cfg(any(\n",
                "    test,\n",
                "    feature = \"test-support\"\n",
                "))]\n",
                "pub fn marked_for_tests() {}\n",
                "\n",
                "#[cfg(feature = \"test-support\")]\n",
                "pub fn bare_for_tests() {}\n",
                "\n",
                // **The opener from one attribute and the name from another.**
                // The docs.rs idiom puts this item behind nothing and supplies
                // the feature name; the `#[cfg(unix)]` above it supplies the
                // `#[cfg(`. Joined across the prologue, a door genuinely in the
                // default build reads as behind the feature — invariant 13's
                // default-build half switched off without a word.
                "#[doc(hidden)]\n",
                "/// A door, and the reason it is here.\n",
                "#[cfg(unix)]\n",
                "#[cfg_attr(docsrs, doc(cfg(feature = \"test-support\")))]\n",
                "pub fn adrift_for_tests() {}\n",
            ),
        )
        .expect("writable");

        let (findings, _, _) =
            super::every_hatch_carries_its_reason(&root).expect("the scratch is readable");
        let said: Vec<&str> = findings.iter().map(|it| it.what.as_str()).collect();
        assert_eq!(
            said.iter()
                .filter(|what| what.contains("is not marked as one"))
                .count(),
            1,
            "the door above the wrapped attribute is marked and said so, and the one \
             below it is neither: {findings:#?}"
        );
        // **And the third question the prologue answers.** `behind` needs
        // `#[cfg(` and the feature name, and rustfmt puts them on different
        // lines — so the wrapped spelling this rule's own remedy asks for was
        // read as a door in the default build. Neither door here is.
        assert_eq!(
            said.iter()
                .filter(|what| what.contains("is in the default"))
                .count(),
            1,
            "the wrapped `#[cfg(any(…))]` door is behind the feature and the one whose \
             opener and feature name come from two different attributes is not — and \
             exactly one of these three doors is in the default build: {findings:#?}"
        );
    }

    /// A sibling crate is not the crate whose name its own begins with.
    ///
    /// **The rule that holds invariant 13's default-build half compared a path
    /// against a directory *name* with no separator**, so a crate whose
    /// directory opens with another's was claimed by it: a crate declaring no
    /// `test-support` feature would be told its door contradicts one, and the
    /// remedy would send its author to a manifest key that is not there. This is
    /// the too-strong half of the prefix family, and the type wall in
    /// `xtask/src/wall.rs` already writes its own sibling case out.
    ///
    /// Driven through the shipped walk over a scratch workspace rather than
    /// through the predicate, because what was wrong was the *application* of
    /// the set and the set itself was right.
    #[test]
    fn a_sibling_crate_is_not_the_one_whose_name_it_begins_with() {
        let held = crate::check::LeavesNothingBehind::new("sibling-door");
        let root = held.root.clone();
        let door = concat!(
            "/// A door a test reaches a private thing through, and why it exists.\n",
            "#[doc(hidden)]\n",
            "pub fn peek_for_tests() -> u8 {\n",
            "    1\n",
            "}\n",
        );
        // **A dependency of that name is not a feature of that name.** The rule
        // asks for `\n[features]` *and* a line opening with the feature's name;
        // dropping the header half leaves a crate that merely *depends* on
        // something called `test-support` counted as one that declares the
        // feature, so its doors are judged by a rule its manifest never opted
        // into. Found in round sixty-four by running a plant a reviewer prepared
        // and could not reach.
        for (named, declares) in [
            ("nc-shut", Manifest::Feature),
            // The sibling: its directory name opens with the other's and it
            // declares no feature at all.
            ("nc-shut-notes", Manifest::Bare),
            // The control: the same door in a crate whose name shares no prefix.
            ("nc-apart", Manifest::Bare),
            // The one the header half is for: the name appears, under
            // `[dependencies]`, and declares no feature at all.
            ("nc-depends", Manifest::Dependency),
        ] {
            let directory = root.join("crates").join(named);
            std::fs::create_dir_all(directory.join("src")).expect("writable");
            let manifest = match declares {
                Manifest::Feature => format!(
                    "[package]\nname = \"{named}\"\n\n[features]\n{} = []\n",
                    super::DOORS_ARE_BEHIND
                ),
                Manifest::Dependency => format!(
                    "[package]\nname = \"{named}\"\n\n[dependencies]\n{} = \"1\"\n",
                    super::DOORS_ARE_BEHIND
                ),
                Manifest::Bare => format!("[package]\nname = \"{named}\"\n"),
            };
            std::fs::write(directory.join("Cargo.toml"), manifest).expect("writable");
            std::fs::write(directory.join("src/lib.rs"), door).expect("writable");
        }

        let (findings, _, _) =
            super::every_hatch_carries_its_reason(&root).expect("the scratch is readable");
        let named: Vec<&str> = findings
            .iter()
            .map(|finding| finding.file.as_str())
            .collect();
        assert!(
            named.contains(&"crates/nc-shut/src/lib.rs"),
            "the crate that does declare the feature was not judged, so this fixture \
             proves nothing about the ones that do not: {findings:#?}"
        );
        for outside in [
            "crates/nc-shut-notes/src/lib.rs",
            "crates/nc-apart/src/lib.rs",
            "crates/nc-depends/src/lib.rs",
        ] {
            assert!(
                !named.contains(&outside),
                "`{outside}` is in a crate that declares no `{}` feature and it was \
                 judged as one that does: {findings:#?}",
                super::DOORS_ARE_BEHIND
            );
        }
    }

    /// The crates that shut their doors are the ones whose manifest says so.
    ///
    /// **Invariant 13's default-build half could be switched off in one
    /// character.** The rule fires only for a crate in this set, and nothing
    /// drove the set: measured on 2026-09-10, one condition changed so it comes
    /// back empty for every crate, and *"a `_for_tests` door is in the default
    /// build of a crate whose `test-support` feature says its doors are not"*
    /// can never fire again — with the whole suite and `check` green. The defect
    /// it closes is dated in its own doc: a journal door in the shipped surface.
    ///
    /// Asked of the tree first, because a scratch alone would agree with a
    /// predicate that answers nothing about this repository.
    #[test]
    fn the_crates_that_shut_their_doors_are_the_ones_whose_manifest_says_so() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let shut = super::the_crates_that_shut_their_doors(root);
        assert!(
            !shut.is_empty(),
            "no crate in this tree was read as shutting its doors, so the rule that \
             holds invariant 13's default-build half fires for none of them"
        );
        for whose in &shut {
            let manifest = std::fs::read_to_string(root.join(whose).join("Cargo.toml"))
                .expect("a crate in this set has a manifest");
            assert!(
                manifest.contains(super::DOORS_ARE_BEHIND),
                "`{whose}` was read as shutting its doors and its manifest does not \
                 declare `{}`",
                super::DOORS_ARE_BEHIND
            );
        }

        // And both directions, over manifests this test writes: a crate that
        // declares the feature is in, one that declares none is out, and one
        // that has a `[features]` table without it is out too.
        // **Owned by a guard rather than removed by hand.** This removed the
        // directory before writing it and never after, so it left one behind on
        // *every* run rather than only on a failing one — 3,073 of the 9,250 in
        // `/tmp` on 2026-09-27, a third of the total, from this test alone.
        // `LeavesNothingBehind` removes on `Drop`, which is the path a panicking
        // test takes too.
        let held = crate::check::LeavesNothingBehind::new("shutdoors");
        let scratch = held.root.clone();
        for (name, manifest) in [
            (
                "shuts",
                format!(
                    "[package]\nname = \"shuts\"\n\n[features]\n{} = []\n",
                    super::DOORS_ARE_BEHIND
                ),
            ),
            ("opens", "[package]\nname = \"opens\"\n".to_owned()),
            (
                "other",
                "[package]\nname = \"other\"\n\n[features]\nsomething-else = []\n".to_owned(),
            ),
        ] {
            let crate_root = scratch.join("crates").join(name);
            std::fs::create_dir_all(&crate_root).expect("writable");
            std::fs::write(crate_root.join("Cargo.toml"), manifest).expect("writable");
        }
        let read = super::the_crates_that_shut_their_doors(&scratch);
        assert!(
            read.iter().any(|whose| whose.ends_with("shuts")),
            "a crate whose manifest declares the feature was read as leaving its doors \
             open: {read:?}"
        );
        assert!(
            !read.iter().any(|whose| whose.ends_with("opens"))
                && !read.iter().any(|whose| whose.ends_with("other")),
            "a crate that declares no such feature was read as shutting its doors, which \
             reports every door in it: {read:?}"
        );
    }

    /// `[workspace.lints]` declares the lints written down here.
    ///
    /// **Nothing pinned the contents.** The gate beside this one checks that
    /// every member *opts in*; nothing checked there was anything to opt into.
    /// Measured: `unsafe_code = "forbid"` deleted from the root manifest and a
    /// raw-pointer dereference planted in `xtask/src/store.rs` — it compiles,
    /// which is the proof the crate is outside the lints, and every printed line
    /// was byte-identical to a clean run down to `check passed`.
    ///
    /// Written out rather than read from the manifest: a test that asked the
    /// file what the file says would agree with any file. `unsafe_code` is the
    /// one that cannot be overridden at crate level, so its absence is the
    /// invisible one.
    #[test]
    fn the_workspace_declares_the_lints_written_down_here() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let declared = super::the_workspace_lints(root).expect("the root manifest reads");
        assert_eq!(
            declared,
            [
                ("clippy::all".to_owned(), "warn".to_owned()),
                ("clippy::pedantic".to_owned(), "warn".to_owned()),
                (
                    "rust::missing_debug_implementations".to_owned(),
                    "warn".to_owned()
                ),
                ("rust::missing_docs".to_owned(), "warn".to_owned()),
                ("rust::unreachable_pub".to_owned(), "warn".to_owned()),
                ("rust::unsafe_code".to_owned(), "forbid".to_owned()),
            ],
            "`[workspace.lints]` moved. Every gate in this crate that reasons about lints \
             rests on these six, and a member opting in to nothing is a member with no \
             lints at all — `unsafe_code = \"forbid\"` cannot be overridden at crate \
             level, so its absence is the one nothing else can see"
        );
    }

    /// The root manifest sets the profile keys written down here, and no others.
    ///
    /// **A profile key is the one way to disarm a check that no argument list
    /// and no source census can see.** Measured in wave 37: three lines in the
    /// root manifest,
    ///
    /// ```toml
    /// [profile.test]
    /// debug-assertions = false
    /// overflow-checks = false
    /// ```
    ///
    /// take twelve tests that fail on a falsified `debug_assert!` to passing,
    /// with `cargo xtask check` green over it and every count in the transcript
    /// unmoved but one, which moves for any manifest edit at all and says
    /// nothing about what the lines contain.
    ///
    /// **A whitelist, because the ways cargo can weaken a profile grow with
    /// cargo.** A list of keys to refuse was tried on paper and three spellings
    /// walked through it in one afternoon: `inherits = "release"`, which takes
    /// `debug-assertions` and `overflow-checks` together without naming either;
    /// the dotted form `profile.test.debug-assertions = false`, which no line
    /// scan sees; and a `CARGO_PROFILE_*` environment variable, which needs no
    /// manifest at all and is refused one file over. Equality against what is
    /// written here refuses all three, and whatever cargo adds next.
    ///
    /// This is `[workspace.lints]`'s shape, one screen up, for the same reason.
    #[test]
    fn the_root_manifest_sets_the_profile_keys_written_down_here() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let text = std::fs::read_to_string(root.join("Cargo.toml")).expect("the root manifest");
        let manifest = toml::from_str::<toml::Value>(&text).expect("the root manifest is TOML");
        let mut set = Vec::new();
        if let Some(profiles) = manifest.get("profile").and_then(toml::Value::as_table) {
            for (profile, table) in profiles {
                let Some(table) = table.as_table() else {
                    set.push((profile.clone(), String::new(), table.to_string()));
                    continue;
                };
                for (key, value) in table {
                    set.push((profile.clone(), key.clone(), value.to_string()));
                }
            }
        }
        set.sort();
        assert_eq!(
            set,
            [
                (
                    "release".to_owned(),
                    "codegen-units".to_owned(),
                    "1".to_owned()
                ),
                (
                    "release".to_owned(),
                    "lto".to_owned(),
                    "\"thin\"".to_owned()
                ),
                ("release".to_owned(), "opt-level".to_owned(), "3".to_owned()),
            ],
            "the root manifest's `[profile]` tables moved. Every key here reaches rustc \
             without appearing in any argument this repository's gates read, and \
             `debug-assertions` or `overflow-checks` turned off under `test` is a suite \
             that stops asserting with every count in the transcript unmoved"
        );
        // **And no member manifest carries one.** Cargo ignores `[profile]` in a
        // member and says so in a warning nothing here reads, so a table written
        // there sets nothing and reads to the next person as though it did.
        //
        // **Every member cargo resolves, not the directories under `crates/`.**
        // Measured by a reviewer: `[profile.release]` appended to
        // `plugins/reference/Cargo.toml` left this test and the whole gate green
        // while `cargo metadata` warned that it would be ignored, and the same
        // lines in a crate's manifest failed it. The root manifest is a virtual
        // one, so every member is a member manifest, and a member tree added
        // later is in this list the day cargo builds it.
        let members = crate::members::members(root).expect("cargo lists this workspace");
        for member in &members {
            let manifest = root.join(&member.path).join("Cargo.toml");
            let text = std::fs::read_to_string(&manifest)
                .unwrap_or_else(|error| panic!("`{}` is readable: {error}", member.path));
            assert!(
                !text.contains("[profile"),
                "{} carries a `[profile]` table. Cargo ignores one in a member manifest, \
                 so it changes nothing and tells a reader it does",
                super::relative_to(root, &manifest)
            );
        }
        assert!(
            members
                .iter()
                .any(|member| member.path.starts_with("plugins/")),
            "no member cargo lists is a plugin, so the manifests this read are not the \
             ones the check above was blind to"
        );
    }

    /// Every shape of unpinned image the file pass can see is a finding.
    ///
    /// **This gate had no test of any kind.** `grep` for `every_image_is_pinned`,
    /// `pinned_by_digest`, `in_the_compose_files` and `is_a_build_key` across
    /// every test file returned nothing, and its only guard was
    /// `inspected_something(read, "images")` — `> 0`. Two one-line positional
    /// filters, one per pass, plus the log store repointed at its own `latest`
    /// tag, left `ok 4 image(s) in the stack, each pinned by digest`
    /// byte-identical to baseline while `cargo xtask dev` would pull whatever
    /// that tag resolves to today.
    ///
    /// The file pass is what this drives: it reads files rather than asking
    /// docker, so it runs anywhere, and it is the half that catches a variable,
    /// a profile and a `build:` — the three the resolved projection cannot show.
    #[test]
    fn every_unpinned_shape_in_a_compose_file_is_a_finding() {
        let root = std::env::temp_dir().join(format!("nc-compose-unpinned-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a writable scratch");
        let pinned = "postgres:18-alpine@sha256:\
                      d3e1620b0e4b0e0c1e5a2b4c1d6e7f80a1b2c3d4e5f60718293a4b5c6d7e8f90";
        for (about, written) in [
            ("a bare tag", "    image: postgres:latest"),
            (
                "a digest of the wrong length",
                "    image: alpine:latest@sha256:deadbeef",
            ),
            (
                "an upper-case digest",
                "    image: alpine:latest@sha256:\
                 D3E1620B0E4B0E0C1E5A2B4C1D6E7F80A1B2C3D4E5F60718293A4B5C6D7E8F90",
            ),
            (
                "a digest that is not hex",
                "    image: alpine:latest@sha256:\
                 zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            ),
            (
                "a variable with a pinned default",
                "    image: ${IMG:-postgres:18-alpine}",
            ),
            ("a built service", "    build:"),
            (
                "a built service in flow style",
                "    x: {build: {context: .}}",
            ),
            ("a built service with a quoted key", "    \"build\":"),
        ] {
            // **Two services, the unpinned one second.** Written alone, a
            // positional filter — `if read > 1 { continue; }`, the shape this
            // gate had no defence against — never fires, and the test passes
            // for having asked nothing.
            std::fs::write(
                root.join("docker-compose.yml"),
                format!(
                    "services:\n  first:\n    image: {}\n  probe:\n{written}\n",
                    pinned.replace(' ', "")
                ),
            )
            .expect("a writable scratch");
            let mut findings = Vec::new();
            let read = super::in_the_compose_files(&root, &mut findings);
            assert!(
                !findings.is_empty(),
                "{about} was accepted as pinned, having read {read} image(s)"
            );
        }

        // **The floor: a pinned image is not a finding**, or the rule above
        // would be a ban on compose files rather than a rule about digests.
        std::fs::write(
            root.join("docker-compose.yml"),
            format!(
                "services:\n  probe:\n    image: {}\n",
                pinned.replace(' ', "")
            ),
        )
        .expect("a writable scratch");
        let mut findings = Vec::new();
        let read = super::in_the_compose_files(&root, &mut findings);
        assert!(
            findings.is_empty(),
            "a digest-pinned image was reported: {findings:#?}"
        );
        assert_eq!(read, 1, "the pass read {read} image(s) of one");

        // And every one of the eight names it claims to read is read: a file
        // this pass skips is a file an unpinned image can live in.
        for named in [
            "docker-compose.yaml",
            "docker-compose.override.yml",
            "docker-compose.override.yaml",
            "compose.yml",
            "compose.yaml",
            "compose.override.yml",
            "compose.override.yaml",
        ] {
            std::fs::write(
                root.join(named),
                "services:\n  probe:\n    image: postgres:latest\n",
            )
            .expect("a writable scratch");
            let mut findings = Vec::new();
            let _ = super::in_the_compose_files(&root, &mut findings);
            assert!(
                findings.iter().any(|finding| finding.file == named),
                "`{named}` is named as a file this pass reads and was not read"
            );
            std::fs::remove_file(root.join(named)).expect("a writable scratch");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A code is read as raised however the file imported the enum.
    ///
    /// **The floor `codes_something_raises` had none of, on the half a fixture
    /// cannot reach.** Every mechanism beside this plants
    /// `nc_catalog::ErrorCode::Planted*` — the one spelling the scan was written
    /// for — so all of them agree with the needle rather than with rustc.
    /// Measured on the real tree, `use nc_catalog::ErrorCode as Code;` and
    /// `Code::PolicyUnreadable` in `crates/nc-journal/src/memory.rs` left the
    /// step at its baseline `ok 29 code(s) …`, while the unaliased control
    /// reported `NC-POL-0001` as produced. `docs/errors.md` is generated from
    /// `raising`, so the published page would say the policy subsystem raises
    /// nothing while the crate returns its code.
    ///
    /// Both directions: a file that names the enum under no name at all raises
    /// nothing, or the rule would answer "produced" about every file it read.
    #[test]
    fn a_raised_code_is_found_under_every_spelling_of_its_import() {
        // Assembled, or this file writes the paths it is looking for.
        let path = ["nc_catalog::Error", "Code"].join("");
        let held = ["Error", "Code"].join("");
        for (why, source) in [
            (
                "the path written out",
                format!("pub fn raises() -> {path} {{ let it = {path}::Planted; it }}\n"),
            ),
            (
                "the type imported under its own name",
                format!(
                    "use {path};\npub fn raises() -> {held} {{ let it = {held}::Planted; it }}\n"
                ),
            ),
            (
                "the type imported under another name",
                format!(
                    "use {path} as Code;\npub fn raises() -> Code {{ let it = Code::Planted; it }}\n"
                ),
            ),
            (
                "the variant imported bare, beside the type",
                format!(
                    "use {path}::{{self, Planted}};\npub fn raises() -> {held} {{ let it = Planted; it }}\n"
                ),
            ),
            (
                "the variant imported bare and renamed",
                format!(
                    "use {path}::Planted as Refused;\npub fn raises() {{ let it = Refused; }}\n"
                ),
            ),
            // **The two spellings that are not a `use` statement at all.** A
            // type alias is the third way to give the enum a second name, and
            // the qualified path is a fourth way to write the same path — both
            // measured leaving the verdict line unmoved with a reserved number
            // produced by a public function.
            (
                "the type given a second name by a `type` alias",
                format!(
                    "type Held = {path};\npub fn raises() -> Held {{ let it = Held::Planted; it }}\n"
                ),
            ),
            (
                "the alias declared with a visibility, and wrapped",
                format!(
                    "pub(crate) type Held =\n    {path};\npub fn raises() -> Held {{ let it = Held::Planted; it }}\n"
                ),
            ),
            (
                "the path written inside angle brackets",
                format!("pub fn raises() -> {path} {{ let it = <{path}>::Planted; it }}\n"),
            ),
        ] {
            let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &source, "");
            let (raised, read) = super::codes_something_raises(&root).expect("the scratch walks");
            assert!(read > 0, "the scratch was not read at all ({why})");
            assert!(
                raised.contains(&"Planted".to_owned()),
                "`{source}` constructs a declared code ({why}) and the raising walk read \
                 {raised:?}. A needle list cannot resolve a name: the code is published \
                 as raised by nothing, and `docs/errors.md` is generated from that"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        for (why, source) in [
            (
                "a crate that names no code at all",
                "pub fn nothing() -> usize { 1 }\n".to_owned(),
            ),
            (
                "a name that merely ends in those letters",
                format!(
                    "use crate::My{held};\npub fn raises() -> My{held} {{ My{held}::Planted }}\n"
                ),
            ),
            (
                "an import of a variant nothing then uses",
                format!("use {path}::{{self, Planted}};\npub fn nothing() -> usize {{ 1 }}\n"),
            ),
            (
                "a type alias to something that is not the enum",
                "type Held = usize;\npub fn raises() { let it = Held::Planted; }\n".to_owned(),
            ),
            (
                "an alias whose right-hand side merely ends in those letters",
                format!(
                    "type Held = crate::My{held};\npub fn raises() {{ let it = Held::Planted; }}\n"
                ),
            ),
        ] {
            let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &source, "");
            let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
            assert!(
                !raised.contains(&"Planted".to_owned()),
                "`{source}` raises nothing ({why}) and the walk answered {raised:?}, so \
                 a `reserved` code would be reported as produced by a file that only \
                 mentions it"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// A function whose whole body is one code is a name for it under every
    /// spelling of the import, not only under two literals.
    ///
    /// **The exclusion was narrower than the finder five lines above it, and
    /// that is the direction that costs a reader the gate.**
    /// [`super::error_code_names`] resolves four import spellings;
    /// [`super::without_bare_returns`] decided *mapping table* from *raise* by
    /// matching `-> ErrorCode {` and a body opening `ErrorCode::`. Under a
    /// rename or a bare import the exclusion stopped applying while the finder
    /// kept working, so the walk reported a producer the registry's own comment
    /// says is not one — `NC-IDS-0003`, whose only mention outside its tests is
    /// `MachineFlake::exhausted`, three lines that name the variant and are
    /// called by nothing.
    ///
    /// Both directions, because an exclusion that answered "not a producer"
    /// about everything would empty the walk: the last two rows here are uses
    /// of a code written under the same spellings, and each must still be found.
    #[test]
    fn a_body_that_is_one_code_is_a_name_for_it_under_every_spelling() {
        // Assembled, or this file writes the paths it is looking for.
        let path = ["nc_catalog::Error", "Code"].join("");
        let held = ["Error", "Code"].join("");
        for (why, source) in [
            (
                "the type imported under its own name",
                format!(
                    "use {path};\npub const fn names() -> {held} {{\n    {held}::Planted\n}}\n"
                ),
            ),
            (
                "the type imported under another name",
                format!(
                    "use {path} as Code;\npub const fn names() -> Code {{\n    Code::Planted\n}}\n"
                ),
            ),
            (
                "the variant imported bare, beside the type",
                format!(
                    "use {path}::{{self, Planted}};\npub const fn names() -> {held} {{\n    Planted\n}}\n"
                ),
            ),
            (
                // The braced form, because the unbraced `use X::Planted as R;`
                // spells `ErrorCode::Planted` on the import line itself and the
                // type scan reads that — a property of the finder above, not of
                // this exclusion, and one the test beside this already rests on.
                "the variant imported bare and renamed",
                format!(
                    "use {path}::{{self, Planted as Refused}};\npub const fn names() -> {held} {{\n    Refused\n}}\n"
                ),
            ),
        ] {
            let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &source, "");
            let (raised, read) = super::codes_something_raises(&root).expect("the scratch walks");
            assert!(read > 0, "the scratch was not read at all ({why})");
            assert!(
                !raised.contains(&"Planted".to_owned()),
                "`{source}` is a function whose whole body is one code ({why}) — a name \
                 for that code and not a use of it — and the walk answered {raised:?}. A \
                 `reserved` number is then published as produced, against the registry's \
                 own comment saying it is not"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        // The other direction: the same spellings around a body that is not one
        // code are still producers, or the exclusion has emptied the walk.
        for (why, source) in [
            (
                "a renamed type used inside a larger body",
                format!(
                    "use {path} as Code;\npub fn raises() -> Code {{\n    let it = Code::Planted;\n    it\n}}\n"
                ),
            ),
            (
                "a bare variant used inside a larger body",
                format!(
                    "use {path}::{{self, Planted}};\npub fn raises() -> {held} {{\n    let it = Planted;\n    it\n}}\n"
                ),
            ),
        ] {
            let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &source, "");
            let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
            assert!(
                raised.contains(&"Planted".to_owned()),
                "`{source}` constructs a declared code ({why}) and the walk read \
                 {raised:?}, so the exclusion above has swallowed a real producer"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// An import is not a producer, in either half of the scan.
    ///
    /// **One rule written twice, and the second copy read different text.** The
    /// bare-variant half already dropped the `use` lines — its own paragraph
    /// says why — while the qualified-path loop above it ran over the whole
    /// file, so `use nc_catalog::ErrorCode::Planted;` matched the `ErrorCode::`
    /// needle and made the import the producer of the variant it imports.
    ///
    /// Measured on the real tree, by a refactor of `nc-ids`'s `exhausted()`
    /// that is semantics-preserving, clippy clean and leaves no unused import —
    /// importing the variant and returning it bare instead of qualifying it:
    /// `error `NC-IDS-0003` is declared `reserved` and is produced by a crate
    /// outside the catalog`, where the same function one token different reads
    /// `ok 29 code(s) are declared as what the tree does with them`. The loud
    /// direction costs a developer a false error on a `reserved` number. The
    /// quiet one is the same line: a `raised` code whose last real producer is
    /// deleted keeps reading as produced for as long as one import survives,
    /// and `docs/errors.md` goes on publishing a subsystem that is gone.
    ///
    /// All three spellings, because a repair that empties the finder passes the
    /// first assertion alone.
    #[test]
    fn an_import_is_not_a_producer() {
        let path = ["nc_catalog::Error", "Code"].join("");
        // Qualified, in a `use` line, and written nowhere else.
        let imported = format!("use {path}::Planted;\npub fn nothing() -> usize {{ 1 }}\n");
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &imported, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            !raised.contains(&"Planted".to_owned()),
            "an import that names a variant was read as producing it ({raised:?}), so              every `use nc_catalog::ErrorCode::X;` in the tree declares itself the              producer of `X` and the two halves of this scan judge different text"
        );
        let _ = std::fs::remove_dir_all(&root);

        // And the qualified form in a body is still a producer.
        let qualified = format!(
            "use {path}::Planted;\npub fn raises() -> {path} {{ let it = {path}::Planted; it }}\n"
        );
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &qualified, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            raised.contains(&"Planted".to_owned()),
            "a code constructed by a qualified path in a function body was read as              produced by nothing ({raised:?}), so the repair above has emptied the finder"
        );
        let _ = std::fs::remove_dir_all(&root);

        // And so is the bare form the import makes legal.
        let bare =
            format!("use {path}::Planted;\npub fn raises() -> {path} {{ let it = Planted; it }}\n");
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &bare, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            raised.contains(&"Planted".to_owned()),
            "a code constructed bare under an import was read as produced by nothing              ({raised:?}): the import is the only thing that spells the type, and              dropping the `use` lines must not drop the variant with them"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A brace inside a literal in a test module does not blank the rest of the
    /// file, and a source directory called `tests` is still source.
    ///
    /// **Two holes in the walk's scope, both silent, both measured on the real
    /// tree.** [`super::outside_the_tests`] counted braces before anything
    /// stripped the literals, so `format!("{{")` in a `#[cfg(test)]` module took
    /// `depth` away for ever and every line below it was blanked — five
    /// producers in `crates/nc-canon/src/parse.rs` went dark at once, and the
    /// quiet reading of the same hole is a `reserved` number produced below such
    /// a test and reported by nothing. [`super::in_the_raising_scope`] excluded
    /// any path holding `/tests/`, which is `crates/*/src/**/tests/` as well as
    /// the integration tests it was written for — and the floor beside it asks
    /// the same predicate, so both sides agreed about a directory neither read:
    /// a public producer of the reserved `NC-DB-0002` in
    /// a module `nc-ids` called `tests` under its own `src` left
    /// `ok 29 code(s) …` and every number unmoved.
    ///
    /// Both directions on each: a test module still ends, and a real integration
    /// test is still out.
    #[test]
    fn the_walks_scope_is_not_moved_by_a_brace_in_a_literal_or_a_directory_name() {
        let path = ["nc_catalog::Error", "Code"].join("");
        // A test module whose literals hold braces, and a producer below it.
        let source = format!(
            "#[cfg(test)]\nmod probe {{\n    #[test]\n    fn braces() {{\n        \
             assert_eq!(format!(\"{{{{\"), \"{{\");\n    }}\n}}\n\
             pub fn raises() -> {path} {{ let it = {path}::Planted; it }}\n"
        );
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &source, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            raised.contains(&"Planted".to_owned()),
            "a `#[cfg(test)]` module holding an unbalanced brace *inside a string* blanked \
             the code below it, so the walk read {raised:?}: every producer after such a \
             test is invisible, and a `reserved` number produced there is reported by \
             nothing"
        );
        let _ = std::fs::remove_dir_all(&root);

        // And the module is still cut: a code named only inside it is not a
        // producer, or the repair above has emptied the exclusion.
        let only_in_a_test = format!(
            "#[cfg(test)]\nmod probe {{\n    #[test]\n    fn braces() {{\n        \
             let _ = format!(\"{{{{\");\n        let _ = {path}::Planted;\n    }}\n}}\n\
             pub fn nothing() -> usize {{ 1 }}\n"
        );
        let (_held, root, _, _) =
            somewhere_unpredictable("crates/nc-planted/src", &only_in_a_test, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            !raised.contains(&"Planted".to_owned()),
            "a code named only inside a `#[cfg(test)]` module was read as produced \
             ({raised:?}), so the module is no longer cut at all"
        );
        let _ = std::fs::remove_dir_all(&root);

        // **A `#[cfg(test)]` item that ends in a semicolon ends there.** The
        // region closes on `}` *or* `;` at `depth <= opened`; the `;` half was
        // held by neither a test nor an instance under `crates/`. Measured, the
        // clause removed: `cargo test -p xtask` gave 401 passed and
        // `cargo run -p xtask -- check` printed `check passed`, console
        // identical to a clean run. The tree has ten such items today and all
        // ten are in `xtask/src`, which this walk never reads — so the clause is
        // correct, load-bearing the moment one appears under `crates/*/src`,
        // and was decided by nothing.
        let semicolon = format!(
            "#[cfg(test)]\npub const PROBE: u32 = 3;\n\
             pub fn raises() -> {path} {{ let it = {path}::Planted; it }}\n"
        );
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &semicolon, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            raised.contains(&"Planted".to_owned()),
            "a `#[cfg(test)]` item ending in `;` swallowed the rest of the file, so the \
             walk read {raised:?}: every producer below such a line is invisible, and a \
             `reserved` number produced there is reported by nothing"
        );
        let _ = std::fs::remove_dir_all(&root);

        // **A `#[cfg(test)]` arm or field ends at its comma.** A reviewer hid a
        // producer of a reserved code behind one, the region running on to the
        // enclosing brace; and a test-only function whose parameters span
        // lines, each ending in a comma, is still cut whole.
        for shape in [
            format!(
                "pub fn raises(step: u8) -> {path} {{\n    match step {{\n        \
                 #[cfg(test)]\n        0 => {path}::Other,\n        _ => {path}::Planted,\n    \
                 }}\n}}\n"
            ),
            format!(
                "pub fn raises() -> Holder {{\n    Holder {{\n        #[cfg(test)]\n        \
                 probe: {path}::Other,\n        code: {path}::Planted,\n    }}\n}}\n"
            ),
        ] {
            let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &shape, "");
            let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
            assert!(
                raised.contains(&"Planted".to_owned()),
                "a producer after a `#[cfg(test)]` arm or field was blanked with it, so \
                 the walk read {raised:?}: {shape}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
        let signature = format!(
            "#[cfg(test)]\nfn only(\n    step: u8,\n) -> {path} {{\n    {path}::Planted\n}}\n\
             pub fn nothing() -> usize {{ 1 }}\n"
        );
        let (_held, root, _, _) = somewhere_unpredictable("crates/nc-planted/src", &signature, "");
        let (raised, _) = super::codes_something_raises(&root).expect("the scratch walks");
        assert!(
            !raised.contains(&"Planted".to_owned()),
            "a test-only function whose parameters end in commas was read as shipped from \
             its first parameter on ({raised:?})"
        );
        let _ = std::fs::remove_dir_all(&root);

        // The scope, both ways round: `src/**/tests/` is a member's own source
        // and `crates/<name>/tests/` is the directory cargo builds integration
        // tests from.
        for inside in [
            "crates/nc-ids/src/tests/mod.rs",
            "crates/nc-ids/src/store/tests/fixtures.rs",
            // **The canary half of the same repair, which no case reached.**
            // `directly_under_a_member` and `inside_a_canary_directory`
            // replaced a bare `contains("/canaries/")` for the reason the
            // `/tests/` half was replaced — and only the `/tests/` half got a
            // case here. Measured: the canary predicate put back to
            // `relative.starts_with("canaries/") || relative.contains("/canaries/")`
            // and `cargo test -p xtask` gave 401 passed. One rule written twice
            // and one copy tested.
            "crates/nc-ids/src/canaries/mod.rs",
            "crates/nc-ids/src/lib.rs",
        ] {
            assert!(
                super::in_the_raising_scope(inside),
                "`{inside}` is a member's own source and the walk steps over it, so a \
                 code produced there is published as raised by nothing"
            );
        }
        for outside in [
            "crates/nc-ids/tests/properties.rs",
            "crates/nc-journal/tests/tampering.rs",
            "crates/nc-catalog/src/lib.rs",
            "crates/nc-log/canaries/one.rs",
        ] {
            assert!(
                !super::in_the_raising_scope(outside),
                "`{outside}` is a whole file of tests, the registry itself, or a canary, \
                 and the walk read it as a producer"
            );
        }
    }

    /// Every tree-wide walk reaches a defect it could not have been told about.
    ///
    /// **The canaries and the line floors are defeated by the same one-line
    /// filter, and it goes between them.** Both counters sit above the
    /// judgement, so `read += 1;` then `continue;` leaves the number where it
    /// was, and the canary lives at a path a filter can name. Measured, four
    /// filters of one line each — `!relative.contains("/canaries/")` twice and
    /// `index > 40` twice — with four real defects in the tree:
    ///
    /// ```text
    /// ok    64653 source line(s) leave the workspace lints on      (baseline 64631)
    /// ok    25329 source line(s) name no error code as text        (baseline 25319)
    /// ok    1453 section citation(s) name a real section           (baseline 1452)
    /// ok    786 citation(s) resolve                                (baseline 785)
    /// check passed
    /// ```
    ///
    /// Every number *up*, every canary still reported, 248 xtask tests green.
    /// A count cannot close this — the counter runs before the judgement, so a
    /// filter after it is invisible to any floor derived from the counter. What
    /// closes it is a fixture whose location the filter's author cannot know:
    /// the directory name, its depth, the file name and the line the defect
    /// sits on all come from the process id, so `contains("/canaries/")`,
    /// `index > 40` and every other constant miss it. The static canaries stay
    /// — they read well in a diff and they survive in a tree with no `git` —
    /// and this is the third mechanism their README says is needed.
    #[test]
    fn a_walk_reaches_a_defect_planted_where_no_filter_can_name_it() {
        let (_held, root, file, line) =
            somewhere_unpredictable("planted", "#![allow(clippy::all)]", "// nothing here");
        let (findings, read) =
            super::no_source_puts_the_lints_down(&root).expect("the scratch walks");
        reached(&findings, &file, line, read, "the blanket-allow walk");
        let _ = std::fs::remove_dir_all(&root);

        // And the same defect in every directory this repository really has, so
        // a filter naming one of them by its real path is caught too.
        let (_held, root, mut planted) =
            in_every_real_directory("#![allow(clippy::all)]", "// nothing here");
        // **And under the names this tree wears.** Wave 39 gave this to the
        // include walk alone; wave 40 measured what the other three were left
        // with. This walk reads every `.rs` in the tree and charges its counter
        // per file, and the floor beside it counts the same files a second way,
        // so a clause keyed on a name lowers both and the equality holds across
        // — with every fixture green, because every `.rs` they planted was
        // called `f{n}.rs`, a name nothing in this repository wears.
        and_a_file_under_every_name_it_opens(
            &root,
            &mut planted,
            "#![allow(clippy::all)]",
            "// nothing here",
        );
        let (findings, _) = super::no_source_puts_the_lints_down(&root).expect("the scratch walks");
        reached_every_one(&findings, &planted, "the blanket-allow walk");

        the_hatch_walk_goes_everywhere_inside_its_scope();

        the_two_walks_that_had_no_mechanism();

        // **The walk that decides what a code's `raising` says**, which had no
        // mechanism of any kind: no canary, no fixture, no corpus entry, no
        // test, and a floor counting *variants found* rather than files read.
        // One line naming a real crate left two `reserved` codes being produced
        // by a public function, with `check passed`.
        //
        // **A distinct code per directory**, because one code repeated cannot
        // see a directory going missing: the walk finds the identical variant
        // in the twenty others and the assertion holds while the walk is short.
        // Measured — a crate added to the skip chain left this green until each
        // place got a code of its own.
        let named = |index: usize| format!("Planted{index}");
        let (_held, root, all) = in_every_real_directory_each(
            &|index| {
                format!(
                    "pub const fn produces() -> nc_catalog::ErrorCode {{ \
                     nc_catalog::ErrorCode::{} }}",
                    named(index)
                )
            },
            "// nothing here",
        );
        let planted: Vec<(String, usize)> = all
            .iter()
            .filter(|(file, _)| {
                (file.starts_with("crates/") || file.starts_with("plugins/"))
                    && super::in_the_raising_scope(file)
            })
            .cloned()
            .collect();
        assert!(
            planted.len() > 5,
            "only {} directory/directories are inside the raising scope",
            planted.len()
        );
        let (raised, read) = super::codes_something_raises(&root).expect("the scratch walks");
        let missed: Vec<&str> = planted
            .iter()
            .filter(|(file, _)| {
                let index = all
                    .iter()
                    .position(|(other, _)| other == file)
                    .expect("planted is a subset of all");
                !raised.contains(&named(index))
            })
            .map(|(file, _)| file.as_str())
            .collect();
        assert!(
            missed.is_empty(),
            "the raising walk read {} of {} directories inside its own scope. A code \
             produced in one it skipped is published as raised by nothing, and \
             `docs/errors.md` is generated from that. Missed: {missed:#?}",
            planted.len() - missed.len(),
            planted.len()
        );
        // **And the crates the scope must hold, by hand.** The floor beside this
        // asks `in_the_raising_scope` for the expectation, so a crate added to
        // the skip chain leaves both sides agreeing — the derived-expectation
        // trap this file has now walked into twice. The list is written out.
        for member in JUDGED_BY_THE_RAISING_RULE {
            assert!(
                super::in_the_raising_scope(&format!("{member}src/lib.rs")),
                "`{member}` is written here as a crate the raising walk reads, and its \
                 own predicate says otherwise"
            );
        }
        every_source_of_those_crates_is_in_the_raising_scope();
        assert_eq!(
            read,
            super::rust_files_under(&root, "crates", super::in_the_raising_scope)
                + super::rust_files_under(&root, "plugins", super::in_the_raising_scope),
            "the walk read a different number of files than a second count of the same \
             ground, so something inside it is filtering"
        );
        let _ = std::fs::remove_dir_all(&root);

        the_env_walk_reaches_a_secret_wherever_it_sits();
    }

    /// The `.env` walk reaches a secret at an unpredictable path and in every
    /// real directory.
    ///
    /// Split from the caller, which crossed clippy's line limit with it inline
    /// once wave 40 gave the three walks above it their name plantings.
    fn the_env_walk_reaches_a_secret_wherever_it_sits() {
        // **The one gate here that is about a secret.** Its exact floor
        // compares the paths walked against a second enumeration, which says
        // nothing about whether `holds_an_environment` was ever asked — and a
        // committed `.env` is the finding whose absence looks exactly like a
        // clean tree. There is no fixed canary for it to be filtered around
        // either: a real `.env` in this repository is the defect.
        let (_held, root, file, _) = somewhere_unpredictable("planted", "", "");
        let secret = root.join(&file).with_file_name(".env.local");
        std::fs::write(&secret, "POSTGRES_PASSWORD=hunter2\n").expect("a writable scratch");
        let (findings, walked) = super::committed_env_files(&root).expect("the scratch walks");
        let named = super::relative_to(&root, &secret);
        assert!(
            findings.iter().any(|finding| finding.file == named),
            "the .env walk did not reach a secret planted at {named}, having walked \
             {walked} path(s): {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&root);

        // **And a secret in every real directory, which is the worst of the
        // five.** `if relative_to(root, path).starts_with("crates/") { walked +=
        // 1; return; }` left a `crates/nc-journal/.env.local` holding a password // allow-reference: `crates/nc-journal/.env.local` is the secret that measurement planted, and it was removed
        // and an AWS key in the tree, and the printed path count went *up* —
        // the secret is itself one of the paths counted — with `check passed`.
        let (_held, root, planted) = in_every_real_directory("", "");
        let secrets: Vec<String> = planted
            .iter()
            .map(|(file, _)| {
                let secret = root.join(file).with_file_name(".env.local");
                std::fs::write(&secret, "POSTGRES_PASSWORD=hunter2\n").expect("a writable scratch");
                super::relative_to(&root, &secret)
            })
            .collect();
        let (findings, walked) = super::committed_env_files(&root).expect("the scratch walks");
        let missed: Vec<&String> = secrets
            .iter()
            .filter(|named| !findings.iter().any(|finding| finding.file == **named))
            .collect();
        assert!(
            missed.is_empty(),
            "the .env walk reached {} of {} secrets, having walked {walked} path(s). \
             Missed: {missed:#?}",
            secrets.len() - missed.len(),
            secrets.len()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The hatch walk reaches a defect in every real directory of its scope.
    ///
    /// **The third walk with no mechanism**: no identity, no canary —
    /// `xtask/canaries/` is returned from outright, so it can never hold one —
    /// and not on this list. Its `read` charged `lines.len()` per file before
    /// the loop that judges those lines, so a filter between the two left the
    /// printed number and the exact floor under it both whole while invariant
    /// 15's only rule went unasked.
    ///
    /// **The expectation is filtered through the walk's own predicate**, and the
    /// filter is floored. Sixteen of the forty-six real directories are outside
    /// `crates/` and `xtask/`, so an entry that planted everywhere and expected
    /// everything refused a clean tree — measured. And the filler is blank
    /// rather than `// nothing here`, because a plain `//` line directly above
    /// an allow *is* a written reason to this rule: with the usual filler the
    /// plant would be excused by its own padding and this fixture would prove
    /// the opposite of what it says.
    ///
    /// Split from the caller, which crossed clippy's line limit with it inline.
    fn the_hatch_walk_goes_everywhere_inside_its_scope() {
        let (_held, root, mut all) =
            in_every_real_directory("#[allow(dead_code)]\npub const PROBE: usize = 1;", "");
        // **And under the names this tree wears**, for the reason the
        // blanket-allow fixture carries: this walk's `read` is charged per file
        // and the floor under it counts the same ground again, so a clause
        // keyed on a *name* moves both and neither notices. The filler stays
        // blank here, which is why the helper takes one.
        and_a_file_under_every_name_it_opens(
            &root,
            &mut all,
            "#[allow(dead_code)]\npub const PROBE: usize = 1;",
            "",
        );
        let planted: Vec<(String, usize)> = all
            .iter()
            .filter(|(file, _)| super::in_the_hatch_scope(file))
            .cloned()
            .collect();
        assert!(
            planted.len() > 8,
            "only {} of {} directories are inside the hatch walk's scope, so this \
             fixture measures the filter rather than the walk",
            planted.len(),
            all.len()
        );
        let (findings, _, _) =
            super::every_hatch_carries_its_reason(&root).expect("the scratch walks");
        reached_every_one(&findings, &planted, "the hatch walk");
        // **And the places the scope must hold, by hand**, because everything
        // above this line asks `in_the_hatch_scope` for its own expectation.
        for place in READ_BY_THE_HATCH_WALK {
            assert!(
                super::in_the_hatch_scope(&format!("{place}/src/lib.rs"))
                    && std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
                        .join(place)
                        .is_dir(),
                "`{place}` is written here as a place the hatch walk reads, and its own \
                 predicate says otherwise"
            );
        }
    }

    /// How many `.rs` files `directory` holds at any depth, but under a
    /// `canaries` directory: read with `std::fs` alone, apart from every walk
    /// of this module.
    fn sources_under(directory: &std::path::Path, count: &mut usize) {
        let entries = std::fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("`{}` is readable: {error}", directory.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                if path.file_name().is_none_or(|name| name != "canaries") {
                    sources_under(&path, count);
                }
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                *count += 1;
            }
        }
    }

    /// Every source file of the listed crates, not only each crate's root.
    ///
    /// **The hand list asked about `{member}src/lib.rs` and nothing else**, so a
    /// clause keyed on a *file* passed it untouched — and everything else around
    /// the raising walk asks `in_the_raising_scope` for its own expectation: the
    /// fixture filters `planted` through it, and the equality beside that takes
    /// its second count through it as well. Three readers, one predicate, and a
    /// clause added inside it lowers all three together. Measured in wave 40,
    /// with a public documented producer of the reserved `NC-DB-0002` written
    /// into `crates/nc-journal/src/postgres.rs`: `&& !relative.ends_with(
    /// "/postgres.rs")` left the whole 78-line transcript byte-identical, the
    /// declared-code count unmoved, and `check passed`.
    ///
    /// **The two classes those crates hold that the rule really does step over
    /// are spelled out here rather than asked of the predicate**, which is what
    /// makes this a second reader instead of a fourth mirror: a member's own
    /// `tests/` directory, which cargo builds whole as integration tests, and a
    /// `canaries` directory. A third exclusion added to the predicate fails
    /// here, and that failure is the edit paying for itself.
    fn every_source_of_those_crates_is_in_the_raising_scope() {
        let tree = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut judged = 0_usize;
        let _ = super::walk(tree, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            let relative = super::relative_to(tree, path);
            let Some(member) = JUDGED_BY_THE_RAISING_RULE
                .iter()
                .find(|member| relative.starts_with(**member))
            else {
                return;
            };
            let inside = relative.strip_prefix(*member).unwrap_or_default();
            if inside.starts_with("tests/") || relative.contains("/canaries/") {
                return;
            }
            judged += 1;
            assert!(
                super::in_the_raising_scope(&relative),
                "`{relative}` is a source file of `{member}`, written down here as a \
                 crate whose sources decide what a code's `raising` says, and the \
                 walk's own predicate steps over it. A code produced in a file the \
                 walk skips is published by `docs/errors.md` as raised by nothing"
            );
        });
        // **Counted again by a reader of its own**, because a walk that reached
        // nothing asserts nothing: every `.rs` file of each crate but those in
        // its `tests/` and its canaries, read with `std::fs` alone. The floor
        // was a number written here — "the six crates hold thirty-four" — which
        // stayed as the list doubled to twelve and their files to more than
        // eighty, so a walk skipping all of `nc-kernel` still cleared it.
        let mut on_disk = 0_usize;
        for member in JUDGED_BY_THE_RAISING_RULE {
            let entries = std::fs::read_dir(tree.join(member)).expect("the crate is readable");
            for entry in entries {
                let path = entry.expect("a directory entry").path();
                let skipped = path
                    .file_name()
                    .is_some_and(|name| name == "tests" || name == "canaries" || name == "target");
                if path.is_dir() && !skipped {
                    sources_under(&path, &mut on_disk);
                } else if path.extension().is_some_and(|kind| kind == "rs") {
                    on_disk += 1;
                }
            }
        }
        assert!(on_disk > 0, "the crates of the raising rule hold no source");
        assert_eq!(
            judged, on_disk,
            "{judged} source file(s) of the crates the raising rule names were asked, and \
             they hold {on_disk}: the walk this reads through is not reaching all of them, \
             so the assertion inside it is deciding less than it reads"
        );
    }

    /// The local SQL walk reaches a refused statement planted in every
    /// directory its writers really have, under every name it opens there, and
    /// in the one file it reads alone.
    ///
    /// **The walk counted a file before judging it, and nothing planted
    /// anywhere but where its own tests looked**, which a reviewer found: a
    /// filter keyed on one subtree, placed past the count, left the count and
    /// every test green. It now counts both and is held to them being equal,
    /// and this is what holds the filter placed past both — measured on
    /// 2026-10-26, `crates/nc-kernel/src/host` skipped that way: 76 of 99
    /// planted statements reported, and this test red.
    #[test]
    fn the_local_sql_walk_goes_everywhere_its_writers_have() {
        let defect = |index: usize| format!("pub const S: &str = \"DROP TRIGGER main.t{index}\";");
        let (_held, root, mut planted) = in_every_real_directory_each(&defect, "// nothing here");
        and_a_file_under_every_name_it_opens(
            &root,
            &mut planted,
            "pub const S: &str = \"DROP TRIGGER main.t\";",
            "// nothing here",
        );
        let at = planted.first().map_or(1, |(_, line)| *line);
        let alone = "crates/nc-prune/src/sqlite.rs";
        std::fs::write(
            root.join(alone),
            format!(
                "{}pub const S: &str = \"DROP TRIGGER main.t\";\n",
                "// nothing here\n".repeat(at - 1)
            ),
        )
        .expect("a writable scratch");
        planted.retain(|(file, _)| file != alone);
        planted.push((alone.to_owned(), at));
        let judged: Vec<(String, usize)> = planted
            .iter()
            .filter(|(file, _)| {
                crate::local::WRITERS
                    .iter()
                    .any(|writer| file == writer || file.starts_with(&format!("{writer}/")))
            })
            .cloned()
            .collect();
        for writer in ["crates/nc-local/src/", "crates/nc-kernel/src/"] {
            assert!(
                judged.iter().any(|(file, _)| file.starts_with(writer)),
                "nothing was planted under `{writer}`"
            );
        }
        let (findings, _) =
            crate::local::no_local_sql_outside_the_migrations(&root).expect("the scratch walks");
        reached_every_one(&findings, &judged, "the local SQL walk");
    }

    /// The unsafe-token walk reaches `unsafe` planted in every directory this
    /// tree really has, and under every name it opens.
    ///
    /// **It had no mechanism but three files named by hand**, which a
    /// reviewer found: a filter keyed on a subtree inside its closure leaves
    /// those three read and every other file of that subtree unasked.
    /// Measured on 2026-10-26, `crates/nc-kernel/tests` skipped that way:
    /// 1384 of 1412 planted defects reported, and this test red.
    #[test]
    fn the_unsafe_walk_goes_everywhere() {
        let defect = "pub fn f() { unsafe {} }";
        // Padded with empty lines rather than comments: the walk's cost is the
        // characters it reads, and the line the defect sits on is the same.
        let (_held, root, mut planted) = in_every_real_directory(defect, "");
        and_a_file_under_every_name_it_opens(&root, &mut planted, defect, "");
        let (findings, _) = super::the_unsafe_rule::unsafe_written_under(&root);
        reached_every_one(&findings, &planted, "the unsafe-token walk");
    }

    /// The vouched-statement walk reaches a statement built from a value,
    /// planted in every directory this tree really has and under every name it
    /// opens, and reports each.
    ///
    /// **It sits inside a walk whose two counters a filter pays**, which a
    /// reviewer found: the files the walk read and the files it judged stay
    /// equal across a condition placed around the call, so nothing but a
    /// planted statement in every place says the call was made for each.
    /// Measured on 2026-10-26, the call skipped for `crates/nc-kernel/tests`:
    /// 1384 of 1412 planted statements reported, and this test red.
    #[test]
    fn the_vouched_statement_walk_goes_everywhere() {
        let defect = "pub fn f(t: &str) { let _ = sqlx::query(sqlx::AssertSqlSafe(format!(\"SELECT \
                      {t}\"))); }";
        // **Padded with empty lines, because the walk's cost is the characters
        // it reads.** With the comment filler the other fixtures use, 1412
        // files of thirteen thousand lines each took 463 s in a debug build;
        // the line the defect sits on is the same either way.
        let (_held, root, mut planted) = in_every_real_directory(defect, "");
        and_a_file_under_every_name_it_opens(&root, &mut planted, defect, "");
        let mut files = Vec::new();
        super::walk(&root, &mut |path| {
            if path.extension().is_some_and(|kind| kind == "rs") {
                files.push(path.to_path_buf());
            }
        })
        .expect("the scratch walks");
        let mut findings = Vec::new();
        crate::check::the_query_and_ignored_test_walk(&root, &files, &mut findings);
        reached_every_one(&findings, &planted, "the vouched-statement walk");
    }

    /// The canary walk and the include walk reach a defect in every real directory.
    ///
    /// Split from the caller, which crossed clippy's line limit with it inline.
    fn the_two_walks_that_had_no_mechanism() {
        // **The two walks that had no mechanism at all.** Neither
        // `no_crate_compiles_a_canary` nor
        // `nothing_includes_what_the_gates_do_not_read` was named by any test,
        // any canary, any corpus entry or any fixture — only a line floor, and
        // that floor is paid at the top of the loop, so a `continue` below it
        // leaves the number whole. Measured, one line in each:
        //
        // ```text
        // if relative.starts_with("crates/nc-canon/") { continue; }
        //
        // ok    79295 source line(s) leave the workspace lints on   (baseline 79288)
        // ok    79658 source line(s) compile no canary              (baseline 79651)
        // ok    79295 source line(s) pull in only what the gates read
        // check passed
        // ```
        //
        // — every number *up*, and `libnc_canon.rlib` holding `NC-KERN-9999`
        // twice: a canary is a file that carries a real registry code and a
        // blanket allow *by design*, so compiling one in is shipping both. The
        // include walk is the same shape and worse, because `.in` is a file kind
        // no gate reads, so §15.1 never sees the code either.
        // **Assembled, not written.** This gate reads its own source, and a line
        // holding the whole `#[path = "../canaries/…"]` form *is* the thing it
        // refuses — written out, the fixture made `check` report itself. The
        // citation fixture beside it is assembled for the same reason.
        // **A different spelling per directory, because one repeated decides
        // one.** The list this gate matches on holds five forms and the fixture
        // planted the first of them everywhere — so, measured on 2026-09-27,
        // deleting `include!`, `include_str!`, `include_bytes!` or the unspaced
        // `path=` from that list left the whole suite green: four of the five
        // rows were decided by nothing, in the gate whose subject is that cargo
        // must not compile a canary. `_each` exists for exactly this — its doc
        // says one defect repeated cannot see a directory going missing, and it
        // cannot see a *spelling* going missing either.
        let spellings = the_spellings_that_reach_a_canary();
        let (_held, root, mut planted) = in_every_real_directory_each(
            &|index| spellings[index % spellings.len()].clone(),
            "// nothing here",
        );
        // **And a manifest in each, because that is the half cargo acts on.**
        // Assembled like the `.rs` reach above and for the same reason. It is
        // planted at the same unpredictable line: at line 1 a clause keyed on
        // the name *and* the line escapes, which is exactly the shape measured
        // one gate over in §15.1's fixture in this same wave.
        and_a_manifest_beside_each_source(&root, &mut planted);
        // Except the `canaries/` directories themselves, which this walk skips by
        // design and says so through its own predicate — a canary naming another
        // canary is not a defect.
        let judged: Vec<(String, usize)> = planted
            .iter()
            .filter(|(file, _)| !super::inside_a_canary_directory(file))
            .cloned()
            .collect();
        the_canary_directories_are_this_trees_two(&planted);
        assert!(
            judged.len() < planted.len(),
            "{} of {} plantings are in canary directories, and none being excused means \
             the walk's own skip is not being exercised at all",
            planted.len() - judged.len(),
            planted.len()
        );
        let (findings, _) = super::no_crate_compiles_a_canary(&root).expect("the scratch walks");
        reached_every_one(&findings, &judged, "the canary walk");

        let (_held, root, mut planted) =
            in_every_real_directory("include!(concat!(\"hid\", \"den.in\"));", "// nothing here");
        // **And under every name this walk opens, for the reason the canary
        // fixture carries one screen up.** This walk pays its counter at the
        // top of the line loop and judges below it, and the floor beside it
        // compares that counter against a raw line count of the same ground —
        // so a clause keyed on a file name, written between the two, lowers
        // both and leaves the equality exact. Every `.rs` this fixture planted
        // was called `f{n}.rs`, a name nothing in the tree wears: measured in
        // wave 39, `relative.starts_with("crates/nc-") && relative.ends_with(
        // "/lib.rs")` took 14,294 lines — the eight library roots, which is
        // where this gate's own doc records the attack living — out of the walk
        // with the whole suite green at 652.
        and_a_file_under_every_name_it_opens(
            &root,
            &mut planted,
            "include!(concat!(\"hid\", \"den.in\"));",
            "// nothing here",
        );
        let (findings, _) =
            super::nothing_includes_what_the_gates_do_not_read(&root).expect("the scratch walks");
        reached_every_one(&findings, &planted, "the include walk");

        // **A code the registry *declares*, not only one it does not.** Both
        // committed fixtures used area `KERN`, which has no declared code, so
        // the branch that says "a declared code counts too" was never exercised
        // from the declared side — and narrowing the rule to
        // `if declared.contains(&code) { continue; }` left every fixture green
        // while `crates/nc-prune` shipped a hand-written `NC-DB-0004` and the
        // printed count went *up*. The rule is that the identity is the
        // `ErrorCode` variant and not its spelling; a fixture on each side of
        // that branch is what says so.
        let (_held, root, file, line) = somewhere_unpredictable(
            "planted",
            "pub const PROBE: &str = \"NC-DB-0004\";",
            "// nothing here",
        );
        std::fs::create_dir_all(root.join("crates")).expect("a writable scratch");
        std::fs::rename(root.join("planted"), root.join("crates/planted")).expect("movable");
        let file = format!("crates/{file}");
        let declared = vec!["NC-DB-0004".to_owned()];
        let (findings, read, _) =
            super::no_code_is_written_at_a_call_site(&root, &declared).expect("the scratch walks");
        reached(&findings, &file, line, read, "the §15.1 walk");
        // And the one the registry does not declare, which is the other side of
        // the same branch.
        let _ = std::fs::remove_dir_all(&root);
        let (_held, root, file, line) = somewhere_unpredictable(
            "planted",
            "pub const PROBE: &str = \"NC-KERN-4242\";",
            "// nothing here",
        );
        std::fs::create_dir_all(root.join("crates")).expect("a writable scratch");
        std::fs::rename(root.join("planted"), root.join("crates/planted")).expect("movable");
        let file = format!("crates/{file}");
        let (findings, read, _) =
            super::no_code_is_written_at_a_call_site(&root, &declared).expect("the scratch walks");
        reached(&findings, &file, line, read, "the §15.1 walk");
        let _ = std::fs::remove_dir_all(&root);

        // **The same, in every real directory this gate's scope holds.** The
        // scope is asked of the gate's own predicate rather than restated, which
        // is what makes this a test of the exclusion list too: a crate added to
        // `OUTSIDE_THE_ERROR_CODE_SCOPE` disappears from the expectation here in
        // the same edit, and a crate *not* in it must be reached.
        let (_held, root, mut all) = in_every_real_directory(
            "pub const PROBE: &str = \"NC-KERN-4242\";",
            "// nothing here",
        );
        // **And under the names this tree wears.** Measured in wave 40, with
        // wave 39's repair in the tree: `!relative.ends_with("parse.rs")`
        // inside this walk hid a hand-written `NC-KERN-4242` in
        // `crates/nc-canon/src/parse.rs` and left the whole transcript
        // byte-identical, because the counter it pays is paid per file and the
        // floor beside it recounts the same ground. Wave 39 wrote the answer
        // and gave it to one walk of the four.
        and_a_file_under_every_name_it_opens(
            &root,
            &mut all,
            "pub const PROBE: &str = \"NC-KERN-4242\";",
            "// nothing here",
        );
        // **And a `.sql`, because this gate reads one and no fixture ever
        // planted one.** §15.1's scope is `crates/` plus `migrations/`, and
        // `migrations/` holds nothing but `.sql`; every fixture wrote
        // `/f{n}.rs`, and the corpus's single `.sql` case puts its code on line
        // 2. So a clause keyed on the comment marker *and* a line — measured in
        // wave 38 — cleared both at once, and a registry code written out as
        // text inside a live `RAISE EXCEPTION` in the journal's append-only
        // trigger left the whole transcript byte-identical. The depth is not
        // enough on its own: this plants at the same unpredictable line the
        // `.rs` plant uses.
        and_a_sql_file_beside_each_source(&root, &mut all);
        let planted: Vec<(String, usize)> = all
            .iter()
            .filter(|(file, _)| super::in_the_error_code_scope(file))
            .cloned()
            .collect();
        // **The floor on the filter, and it is a count of what is left rather
        // than a fraction.** It was `planted * 2 > all`, which held while the
        // fixture only enumerated `crates/*/src|tests`; walking the whole tree
        // put `catalog/`, `docs/`, `.github/` and twenty others into `all`, and
        // §15.1's scope is `crates/` plus `migrations/` by design — so the
        // fraction failed for the gate being correctly scoped. What matters is
        // that enough directories remain to be filtered around.
        assert!(
            planted.len() > 8,
            "only {} of {} directories are inside §15.1's scope, so this fixture is \
             asserting about too little of the tree",
            planted.len(),
            all.len()
        );
        let (findings, _, _) =
            super::no_code_is_written_at_a_call_site(&root, &declared).expect("the scratch walks");
        // And the other side of the exclusion list: every directory it names is
        // one the gate really does skip, so a name that stopped matching is a
        // failure rather than dead text.
        for (file, _) in all
            .iter()
            .filter(|(file, _)| !planted.iter().any(|(kept, _)| kept == file))
        {
            assert!(
                !findings.iter().any(|finding| finding.file == *file),
                "`{file}` is named as outside §15.1's scope and the walk judged it anyway"
            );
        }
        reached_every_one(&findings, &planted, "the §15.1 walk");
    }

    /// The same, for the two citation gates and the determinism walk.
    ///
    /// Separate from the walk above because each of these needs a shape around
    /// the file: the citation gates read a workspace, and the determinism walk
    /// reads three named directories. What is being asserted is the same thing —
    /// the gate arrived at a line nobody writing a filter could have named.
    #[test]
    fn the_citation_gates_and_the_emitter_walk_reach_one_too() {
        // The determinism walk reads the places `EMITS` names, so the tree has
        // to have them. The defect is a clock, which is what it exists to refuse.
        let (_held, root, file, line) = somewhere_unpredictable(
            "crates/nc-catalog-compiler/src",
            "let _ = std::time::SystemTime::now();",
            "// nothing here",
        );
        std::fs::create_dir_all(root.join("crates/nc-canon/src")).expect("a writable scratch");
        std::fs::create_dir_all(root.join("crates/nc-schema/src")).expect("a writable scratch");
        std::fs::create_dir_all(root.join("xtask/src")).expect("a writable scratch");
        // `EMITS` names a directory that must exist for the walk to run at all;
        // the plant is nested inside it, and `somewhere_unpredictable` creates
        // only the leaf.
        std::fs::create_dir_all(root.join("crates/nc-catalog-compiler/src"))
            .expect("a writable scratch");
        std::fs::write(root.join("xtask/src/build.rs"), "// nothing here\n")
            .expect("a writable scratch");
        let (findings, read) = crate::determinism::nothing_the_emitter_reads_changes(&root)
            .expect("the scratch walks");
        reached(
            &findings,
            &file,
            line,
            read,
            "the emitter's determinism walk",
        );
        let _ = std::fs::remove_dir_all(&root);

        the_emitter_walk_reads_every_place_it_is_written_to_read();

        // The citation gates read the workspace above the repository, so the
        // scratch is one: a marker, a document with one section in it, and the
        // repository under `repos/`.
        // **Assembled rather than written.** A source line holding the whole
        // citation is a citation, and the two gates this test drives read this
        // file: written out, the fixture made `check` report itself.
        let dangling = format!(
            "//! `crates/nc-{}/src/lib.rs`, and `NOCTUA.md §{}.98`.",
            "nowhere", 98
        );
        let (_held, above, file, line) =
            somewhere_unpredictable("repos/noctua-central/crates", &dangling, "// nothing here");
        std::fs::write(above.join(".noctua-workspace"), "").expect("a writable scratch");
        std::fs::write(
            above.join("NOCTUA.md"),
            "# A document\n\n## §1. One section\n",
        )
        .expect("a writable scratch");
        // Every document the section gate opens, so a missing one is not what
        // this test measures. Each holds one section, which the citation below
        // deliberately does not name.
        std::fs::create_dir_all(above.join("reference")).expect("a writable scratch");
        for named in [
            "NOCTUA-PLAN.md",
            "AGENTS.md",
            "README.md",
            "CLAUDE.md",
            "reference/identity-and-authorization.md",
        ] {
            std::fs::write(above.join(named), "# A document\n\n## §1. One section\n")
                .expect("a writable scratch");
        }
        let here = above.join("repos/noctua-central");
        let inside = file
            .strip_prefix("repos/noctua-central/")
            .expect("planted inside the repository")
            .to_owned();

        // The middle value is the citations read; the third is what could not be
        // judged, which is a different question.
        let (findings, cited, _) = crate::references::run(&here).expect("the scratch walks");
        assert!(
            findings
                .iter()
                .any(|finding| finding.file == inside && finding.line == line),
            "the reference gate did not reach a defect planted at {inside}:{line}, having \
             read {cited} citation(s): {findings:#?}"
        );

        let (findings, read) =
            crate::sections::every_section_cited_exists(&here).expect("the scratch walks");
        // **`==`, not `ends_with`.** The reference half beside this one compares
        // exactly, and this one did not — which is what let the section gate give
        // a repository file a second spelling: it tries the workspace prefix
        // first with no `inside` guard, and the repository is *inside* the
        // workspace, so one file came out as `<workspace>/repos/noctua-central/…`
        // here and as `crates/…` from the gate next to it. `counts.rs` documents
        // fixing exactly that.
        assert!(
            findings
                .iter()
                .any(|finding| finding.file == inside && finding.line == line),
            "the section gate did not reach a defect planted at {inside}:{line}, having \
             read {read} citation(s): {findings:#?}"
        );

        // **And one above the repository**, which is where eight of the documents
        // this gate reads actually live. Every plant sat under `repos/`, so a
        // branch keyed on which *tree* a file is in — `if !path.starts_with(root)
        // { reached += 1; continue; }` — skipped all eight with the citation
        // count going *up* and `check passed`. Widening the plant past the
        // longest file closed the index-shaped filter and says nothing about this
        // one.
        let deep = "The rule is written out in NOCTUA.md \u{a7}98.98, which nobody wrote.\n";
        std::fs::write(
            above.join("AGENTS.md"),
            format!("# A document\n\n## \u{a7}1. One section\n\n{deep}"),
        )
        .expect("a writable scratch");
        let (findings, _) =
            crate::sections::every_section_cited_exists(&here).expect("the scratch walks");
        assert!(
            findings
                .iter()
                .any(|finding| finding.file.contains("<workspace>/AGENTS.md")),
            "the section gate did not reach a citation in a document above the \
             repository, which is where five of the ones it reads live: {findings:#?}"
        );
        let _ = std::fs::remove_dir_all(&above);

        the_citation_gates_go_everywhere(&dangling);
    }

    /// The two citation gates reach a defect in every directory the tree has.
    ///
    /// **Split from the single plant beside it because clippy's line limit says
    /// so**, and because it is a different question: that one asks whether the
    /// walk arrives at an unpredictable *line*, this one whether it arrives at
    /// every real *path*.
    fn the_citation_gates_go_everywhere(dangling: &str) {
        // **And the same defect in every directory the repository really has.**
        // Both plants above sit at one path, and the part inside the repository
        // is a literal — so `if relative.starts_with("crates/nc-")` skipped every
        // crate's source, 44 % of the section gate's corpus, with the static
        // canaries still reported, both fixtures green and `check passed`. The
        // counters cannot say it: the filter is inside the loop they are in.
        let (_held, root, mut planted) = in_every_real_directory(dangling, "// nothing here");
        // **The workspace is a guard of its own, because this is the one place
        // a scratch leaves the guard that owns it.** The tree is *renamed* into
        // a workspace shape, and a rename to an arbitrary destination is exactly
        // how a path escapes containment — the guard is then holding an empty
        // directory and refusing nothing, while 2.2 MB sits wherever the rename
        // put it until the last line of the function, which a failing assertion
        // never reaches. Measured by a reviewer on 2026-09-27.
        //
        // Naming it was the first repair and it was not enough: the guard calls
        // its own root `scratch` in every instance, so a name built from
        // `root.file_name()` was one constant path for every caller and every
        // process — two test binaries under `cargo test --workspace` removed it
        // from under each other. A second guard fixes both: the name carries a
        // pid and a counter again, and the directory is removed on the path a
        // panicking test takes.
        let workspace = crate::check::LeavesNothingBehind::new("mirrored-workspace");
        let above = workspace.root.clone();
        let here = above.join("repos/noctua-central");
        std::fs::create_dir_all(above.join("reference")).expect("a writable scratch");
        std::fs::create_dir_all(here.parent().expect("a parent")).expect("a writable scratch");
        std::fs::rename(&root, &here).expect("movable");

        // **And one file of every kind both gates open, in every one of those
        // directories.** Every plant these two fixtures have ever made is a
        // `.rs` file — `somewhere_unpredictable` and `in_every_real_directory_
        // each` both write `/f{n}.rs` — so a rule keeping `rs` and dropping
        // another kind under a named subtree was invisible to both, while
        // `references.rs` wrote down the opposite: that these fixtures are what
        // refuses a clause added to `reads`. Measured in wave 37: a clause
        // dropping `cedarschema` under `policy/` took the eighteen live
        // citations in `policy/schema.cedarschema` out of the gate with every
        // fixture green and the floor beside them silent, because that floor
        // takes its second count through `reads` as well.
        //
        // The kinds are `READ` and `READ_BY_NAME` themselves, which the
        // partition test holds against the tree in both directions, so a kind
        // the repository gains is planted the day the list names it. Line 1
        // rather than past the longest file: the `.rs` plant beside this one
        // carries the unpredictable depth already, and this asks the other
        // question — whether the walk opens the kind at all. A filter would
        // have to key on kind *and* line together to escape both, which is the
        // bound this pair leaves, written here rather than left to a reader.
        let real = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        for (index, directory) in the_directories_that_really_exist(real).iter().enumerate() {
            // `the_directories_that_really_exist` collects entries, so the
            // repository root is never among them and this is a path with a
            // directory in it, always. The branch that handled an empty one was
            // unreachable and read as though root-level files were planted;
            // what holds the root is elsewhere and was measured in wave 38 —
            // `WHAT_MAY_SIT_AT_THE_ROOT` is a closed list checked both ways,
            // and a `.env` holding a password there fails by name.
            let at = format!("{directory}/");
            // **And under the names the tree really wears, which is the half
            // wave 37 wrote a false bound about.** That bound said "a filter
            // would have to key on kind *and* line together to escape both".
            // It does not: `reads` is handed a `&Path`, so a clause keyed on a
            // file *name* escapes with one clause and no line condition at all,
            // and every file this fixture planted was called `k{n}.{kind}`, a
            // name nothing in the tree wears. Measured in wave 38: one clause
            // refusing `Cargo.toml` takes 26 live citations out of both gates
            // with every fixture green, and one refusing `lib.rs` would take
            // 101.
            //
            // **And where the tree wears each one, which wave 38 left to
            // chance.** Every read name is planted somewhere, so a clause
            // naming one loses a finding — but a name planted in an arbitrary
            // directory answers nothing keyed on a *path*, and 127 of this
            // tree's 138 read names sit in exactly one directory. Measured in
            // wave 40: `schema.cedarschema` was planted under
            // `crates/nc-log/src`, so a clause in `reads` refusing that file
            // by its real relative path took its eighteen live citations out
            // of the gate with the transcript one count line short of a clean
            // run. So each name is planted in every directory this tree really
            // wears it in, the eleven worn in several in all of them, and the
            // spread by index stays on top of that so no real directory goes
            // unplanted. The names come from the tree, so a name it gains is
            // planted the day it appears.
            let named = crate::sections::READ
                .iter()
                .map(|kind| format!("{at}k{index}.{kind}"))
                .chain(
                    crate::sections::READ_BY_NAME
                        .iter()
                        .map(|name| format!("{at}{name}")),
                )
                .chain(
                    the_names_this_tree_wears(real)
                        .into_iter()
                        .filter(|(_, wears_it)| wears_it.len() > 1)
                        .map(|(name, _)| format!("{at}{name}")),
                )
                .chain(
                    the_names_this_tree_wears(real)
                        .into_iter()
                        .enumerate()
                        .filter(|(nth, (_, wears_it))| {
                            wears_it.len() == 1
                                && (wears_it.iter().any(|sits| sits == directory)
                                    || nth % the_directories_that_really_exist(real).len() == index)
                        })
                        .map(|(_, (name, _))| format!("{at}{name}")),
                );
            for relative in named {
                let path = here.join(&relative);
                std::fs::create_dir_all(path.parent().expect("a parent"))
                    .expect("a writable scratch");
                std::fs::write(&path, format!("{dangling}\n")).expect("a writable scratch");
                planted.push((relative, 1));
            }
        }
        std::fs::write(above.join(".noctua-workspace"), "").expect("a writable scratch");
        for named in [
            "NOCTUA.md",
            "NOCTUA-PLAN.md",
            "AGENTS.md",
            "README.md",
            "CLAUDE.md",
            "reference/identity-and-authorization.md",
        ] {
            std::fs::write(
                above.join(named),
                "# A document\n\n## \u{a7}1. One section\n",
            )
            .expect("a writable scratch");
        }
        let (findings, _, _) = crate::references::run(&here).expect("the scratch walks");
        reached_every_one(&findings, &planted, "the reference gate");
        let (findings, _) =
            crate::sections::every_section_cited_exists(&here).expect("the scratch walks");
        reached_every_one(&findings, &planted, "the section gate");
    }

    /// Every workspace member is either judged by `§15.1` or named as not, by
    /// hand.
    ///
    /// **The exclusion list had no case per member and could only grow.** See
    /// `JUDGED_BY_THE_ERROR_CODE_RULE`: every consumer of
    /// `OUTSIDE_THE_ERROR_CODE_SCOPE` derives its expectation from it, so one
    /// added line switched the gate off over the audit crate with every count
    /// consistent and `check passed`.
    ///
    /// Both directions, and the list of members is cargo's, so a member added is
    /// a case added: one that is in neither written list fails here rather than
    /// being quietly judged or quietly not.
    ///
    /// **Cargo's list, not `crates/`.** This read the members with
    /// `read_dir("crates")`, so a member anywhere else was in no list and
    /// nothing disagreed — the one-directional list the message below warns
    /// about. Measured when `plugins/` became a member tree: `nc-plugin-reference`
    /// was in neither list, and a hand-written `NC-JRNL-0003` in its source left
    /// §15.1's line identical to a clean run.
    #[test]
    fn every_member_crate_is_judged_or_named_as_not() {
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let mut members: Vec<String> = crate::members::members(root)
            .expect("cargo lists this workspace")
            .into_iter()
            .map(|member| format!("{}/", member.path))
            .collect();
        members.sort();
        assert!(
            members.len() > 5,
            "only {} member(s) were found, so this proves little",
            members.len()
        );
        // **And from more than one directory**, which is the whole point of
        // asking cargo: a census that still reached only `crates/` would pass
        // every assertion below over the members it happened to see.
        let trees: std::collections::BTreeSet<&str> = members
            .iter()
            .filter_map(|member| member.split('/').next())
            .collect();
        assert!(
            trees.len() > 2,
            "the members sit under {trees:?} alone, and this workspace globs `crates/*`, \
             `plugins/*` and `xtask`: the census is not reading what cargo builds"
        );

        for member in &members {
            let judged = super::JUDGED_BY_THE_ERROR_CODE_RULE.contains(&member.as_str());
            let excluded = super::OUTSIDE_THE_ERROR_CODE_SCOPE.contains(&member.as_str());
            assert!(
                judged != excluded,
                "`{member}` is {} — every member crate is either judged by §15.1 or \
                 written into the exclusion list, and saying both or neither is how a \
                 crate stops being judged with nothing disagreeing",
                if judged {
                    "in both lists"
                } else {
                    "in neither list"
                }
            );
            // And the predicate agrees with the hand list, which is the half
            // that catches a line added to the exclusion list alone.
            assert_eq!(
                super::in_the_error_code_scope(&format!("{member}src/lib.rs")),
                judged,
                "`{member}` is written as {} and the gate's own predicate disagrees",
                if judged { "judged" } else { "excluded" }
            );
        }

        // Nothing in either written list has stopped being what it names: a
        // judged entry is a member, and an excluded one is a member or a
        // directory that holds none — `catalog/` and `docs/`, which are
        // generated from the registry rather than built by cargo.
        for named in super::JUDGED_BY_THE_ERROR_CODE_RULE {
            assert!(
                members.iter().any(|member| member == named),
                "`{named}` is written as judged by §15.1 and is not a member here"
            );
        }
        for named in super::OUTSIDE_THE_ERROR_CODE_SCOPE {
            let a_member = members.iter().any(|member| member == named);
            let holds_none = root.join(named).is_dir()
                && !members.iter().any(|member| member.starts_with(named));
            assert!(
                a_member || holds_none,
                "`{named}` is written into the exclusion list and is neither a member \
                 nor a directory of this tree that holds none"
            );
        }
    }

    /// The places the hatch walk must read, written out.
    ///
    /// **A fixture that filters with the predicate it tests agrees with it.**
    /// The expectation beside this is built by filtering the planted set through
    /// `in_the_hatch_scope`, so a directory leaving the scope leaves the
    /// expectation in the same edit and nothing disagrees — the same
    /// derived-expectation trap `JUDGED_BY_THE_RAISING_RULE` below exists for,
    /// one gate over. The floor on the filter does not close it either: thirty
    /// real directories are in scope and the floor asks for more than eight, so
    /// twenty-one could leave with the fixture green.
    ///
    /// `xtask/canaries/` is absent on purpose — it holds the inputs the other
    /// gates are measured against, with the reason written in
    /// `xtask/canaries/README.md`.
    ///
    /// **The place has to be there**, which the assertion asks separately:
    /// `in_the_hatch_scope` is pure text and answers `true` for a name that is
    /// no longer a directory, so for a wave the list could name a crate this
    /// tree does not have and say nothing. The paragraph here sent a reader to
    /// `cargo xtask check` for that half, and the exact count there cannot close
    /// it either — its expectation is computed from the same directories on
    /// disk, so a place removed lowers both sides of an equality. Found in
    /// wave 32.
    ///
    /// **What this still does not catch**: a place that exists and that the walk
    /// reads, but whose `.rs` files it never reaches for some reason inside the
    /// walk. That is the planted fixture above's question, not this list's.
    //
    // The entries carry no trailing slash, and that is not a style choice: one
    // of the meta-gate's operators rewrites a line of `OUTSIDE_THE_ERROR_CODE_
    // SCOPE` naming the compiler's crate, and a second copy of that text
    // anywhere in this file leaves the operator with nothing unique to apply to
    // — reported as `not applicable`, which is a clause with no coverage. Found
    // in wave 30.
    const READ_BY_THE_HATCH_WALK: [&str; 15] = [
        "crates/nc-authz",
        "crates/nc-canon",
        "crates/nc-catalog",
        "crates/nc-catalog-compiler",
        "crates/nc-guest",
        "crates/nc-ids",
        "crates/nc-journal",
        "crates/nc-kernel",
        "crates/nc-local",
        "crates/nc-log",
        "crates/nc-policy",
        "crates/nc-prune",
        "crates/nc-schema",
        "plugins/reference",
        "xtask",
    ];

    /// The crates whose sources decide what a code's `raising` says.
    ///
    /// Written out, because the floor beside it asks `in_the_raising_scope` for
    /// its expectation — so a crate added to the skip chain moves both sides
    /// together and nothing disagrees. That is the same trap the §15.1
    /// exclusion list fell into, one gate over.
    ///
    /// The registry's own crate and its compiler are absent on purpose: they are
    /// the declaration rather than a producer.
    const JUDGED_BY_THE_RAISING_RULE: [&str; 12] = [
        "crates/nc-authz/",
        "crates/nc-canon/",
        "crates/nc-guest/",
        "crates/nc-ids/",
        "crates/nc-journal/",
        "crates/nc-kernel/",
        "crates/nc-local/",
        "crates/nc-log/",
        "crates/nc-policy/",
        "crates/nc-prune/",
        "crates/nc-schema/",
        "plugins/reference/",
    ];

    /// The determinism walk reaches a clock in each of `EMITS`, not one of three.
    ///
    /// Split from its caller, which crossed clippy's line limit with it inline.
    fn the_emitter_walk_reads_every_place_it_is_written_to_read() {
        // **And one in every place the walk reads, not one of three.** The
        // fixture above plants in a single entry of `EMITS`, and the walk's
        // two-counter identity is payable: `read += 1; entered += 1;` are the
        // first two statements of the loop and the comment branch four lines
        // below does `judged += 1; continue;` — the shape this repository's own
        // canary README calls the one that defeats every counting mechanism,
        // shipped in production code as the precedent. Measured, a clock in the
        // stage that writes the generated bytes plus one line after `entered`:
        //
        // ```text
        // ok    two generations are byte-identical
        // ok    7489 line(s) of the emitter read nothing that changes  (baseline 7486)
        // check passed
        // ```
        //
        // Every one of `EMITS`, each with a defect of its own, so a walk that
        // stops reading one of them loses a distinct finding.
        let (_held, root, _, _) = somewhere_unpredictable("planted", "", "");
        let _ = std::fs::remove_dir_all(&root);
        let mut expected = Vec::new();
        // **A directory or a file**, because `EMITS` holds both: two crate source
        // directories and `xtask/src/build.rs`, the one file that pipes every
        // generated byte through rustfmt. A fixture assuming all three were
        // directories made a directory *named* `build.rs`, and the walk found
        // nothing to read in it.
        for (index, under) in crate::determinism::EMITS.iter().enumerate() {
            let clock =
                format!("// nothing here\nlet _ = std::time::SystemTime::now(); // {index}\n");
            let at = root.join(under);
            if std::path::Path::new(under)
                .extension()
                .is_some_and(|kind| kind == "rs")
            {
                std::fs::create_dir_all(at.parent().expect("a parent"))
                    .expect("a writable scratch");
                std::fs::write(&at, &clock).expect("a writable scratch");
                expected.push((*under).to_owned());
            } else {
                std::fs::create_dir_all(&at).expect("a writable scratch");
                let leaf = format!("clock{index}.rs");
                std::fs::write(at.join(&leaf), &clock).expect("a writable scratch");
                expected.push(format!("{under}/{leaf}"));
            }
        }
        let (findings, _) = crate::determinism::nothing_the_emitter_reads_changes(&root)
            .expect("the scratch walks");
        let missed: Vec<&String> = expected
            .iter()
            .filter(|named| !findings.iter().any(|finding| finding.file == **named))
            .collect();
        assert!(
            missed.is_empty(),
            "the emitter's determinism walk read {} of {} places it is written to read. \
             A clock in one it skipped makes the generated output differ between two \
             machines, which is criterion 4. Missed: {missed:#?}",
            expected.len() - missed.len(),
            expected.len()
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// How many times `needle` appears in `text`.
    fn count_of(text: &[char], needle: &str) -> usize {
        let needle: Vec<char> = needle.chars().collect();
        text.windows(needle.len())
            .filter(|here| *here == needle.as_slice())
            .count()
    }

    /// Plantings that are not one of the six, and why.
    ///
    /// Three of the helper's mentions are its own machinery — the definition of
    /// `in_every_real_directory`, the `in_every_real_directory_each` it
    /// delegates to, and that function's own definition — and one is the test
    /// that proves the fixture reaches a directory no filter can name, which
    /// plants without judging any named walk.
    const PLANTED_WITHOUT_A_LABEL: usize = 4;

    /// Every walk that must be reached by the planted fixture, and the proof it is.
    ///
    /// **The mechanisms are held by nothing.** `xtask/canaries/README.md` argues
    /// that five mechanisms cover the walks — and the *set* of walks each one
    /// covers is itself a list, which nothing enumerated. Two walks turned out to
    /// have none of them at all; and deleting the four lines of a fixture test
    /// that plant for a given walk moves **no test count**, because removing
    /// statements from a body removes no `#[test]` attribute:
    ///
    /// ```text
    /// ok    80312 source line(s) leave the workspace lints on
    /// ok    lints
    /// ok    844 test(s)          <- unchanged
    /// check passed
    /// ```
    ///
    /// — with a blanket `#![allow(clippy::all)]` over acceptance criterion 3's
    /// own suite. So the list is written here and checked against the source: a
    /// walk named here whose fixture call has gone is a failure, and a fixture
    /// deleted is exactly that.
    ///
    /// It is a hand list on purpose. Deriving it from the source would make it
    /// agree with whatever the source does, which is the trap three gates in
    /// this file have already fallen into.
    const REACHED_BY_THE_PLANTED_FIXTURE: [(&str, &str); 10] = [
        (
            "the blanket-allow walk",
            "a_walk_reaches_a_defect_planted_where_no_filter_can_name_it",
        ),
        (
            "the hatch walk",
            "the_hatch_walk_goes_everywhere_inside_its_scope",
        ),
        ("the §15.1 walk", "the_two_walks_that_had_no_mechanism"),
        ("the canary walk", "the_two_walks_that_had_no_mechanism"),
        ("the include walk", "the_two_walks_that_had_no_mechanism"),
        ("the reference gate", "the_citation_gates_go_everywhere"),
        ("the section gate", "the_citation_gates_go_everywhere"),
        (
            "the local SQL walk",
            "the_local_sql_walk_goes_everywhere_its_writers_have",
        ),
        ("the unsafe-token walk", "the_unsafe_walk_goes_everywhere"),
        (
            "the vouched-statement walk",
            "the_vouched_statement_walk_goes_everywhere",
        ),
    ];

    /// Every clause of the `include!` gate, decided.
    ///
    /// **Two of its branches were dead to this suite.** A reviewer removed the
    /// `is_rust` filter and the empty-argument filter and the whole gate suite
    /// stayed green at `849 test(s)`, so what this gate lets through was
    /// decided by reading it. The first filter is the entire *allow* side of a
    /// deliberately positive rule — with it gone every ordinary
    /// `include!("generated.rs")` in the tree becomes a finding — and the
    /// second is what keeps the gate from reporting its own source.
    ///
    /// The calls are assembled at run time rather than written out. A literal
    /// `include!` of a non-Rust name in this file is one the gate would read
    /// off its own source and report against `gates.rs`, which is a fixture
    /// that breaks the tree it is testing.
    #[test]
    fn the_include_gate_decides_every_clause() {
        let scratch = std::env::temp_dir().join(format!("nc-include-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("crates/probe/src")).expect("writable");

        let findings = |argument: &str| {
            let source = format!("const PROBE: () = include{}({argument});\n", "!");
            std::fs::write(scratch.join("crates/probe/src/lib.rs"), source).expect("writable");
            super::nothing_includes_what_the_gates_do_not_read(&scratch)
                .expect("the tree is walkable")
                .0
        };

        // The allow side: one string literal ending `.rs` is the form a walk can
        // follow, and it is the only form that passes.
        let left_over = findings("\"generated.rs\"");
        assert!(
            left_over.is_empty(),
            "an ordinary include of a Rust file is what this gate exists to permit: {left_over:?}"
        );
        let left_over = findings("\"Generated.RS\"");
        assert!(
            left_over.is_empty(),
            "the extension is compared without case, and the test that says so is here: {left_over:?}"
        );
        // A name the compiler assembles is a name no walk can follow.
        let built = findings("concat!(env!(\"OUT_DIR\"), \"/x.rs\")");
        assert_eq!(built.len(), 1, "an assembled name is not one file on disk");
        assert!(
            built[0].what.contains("assembles what it includes"),
            "reported as the wrong thing: {}",
            built[0].what
        );
        // A file of another extension is outside every walk in this suite.
        let other = findings("\"notes.txt\"");
        assert_eq!(
            other.len(),
            1,
            "a non-Rust include is what the gate refuses"
        );
        assert!(
            other[0].what.contains("no gate reads it"),
            "reported as the wrong thing: {}",
            other[0].what
        );
        // Nothing between the parentheses is this gate reading a mention of
        // itself, not an include of anything.
        let left_over = findings("");
        assert!(
            left_over.is_empty(),
            "an empty argument includes nothing, so there is nothing to report: {left_over:?}"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// Every clause of the `include:` block reader, decided.
    ///
    /// **Three of its branches were dead to this suite.** A reviewer deleted the
    /// block-closing clause, the `is_rust` filter above it and the
    /// empty/`:`-terminated/absolute entry filter, and `cargo xtask check`
    /// printed the same lines and `849 test(s)`. The function has one call site
    /// and had no test at all, so what it returns was decided by reading it.
    ///
    /// The last case is what the closing clause is *for*: without it the reader
    /// runs to the end of the file and every later list in the workflow becomes
    /// an included path.
    #[test]
    fn the_include_reader_decides_every_clause() {
        for (label, text, expected) in [
            (
                "no block at all",
                "jobs:\n  build:\n    runs-on: ubuntu\n",
                vec![],
            ),
            ("a plain entry", "include:\n  - a.md\n", vec!["a.md"]),
            (
                "a `path:` entry, quoted",
                "include:\n  - path: \"a.md\"\n",
                vec!["a.md"],
            ),
            (
                "a blank line does not end the block",
                "include:\n  - a.md\n\n  - b.md\n",
                vec!["a.md", "b.md"],
            ),
            (
                "an entry at column zero does not end it either",
                "include:\n- a.md\n",
                vec!["a.md"],
            ),
            ("an empty entry", "include:\n  -\n", vec![]),
            (
                "a nested key, not a path",
                "include:\n  - nested:\n",
                vec![],
            ),
            (
                "an absolute path is not this tree's",
                "include:\n  - /etc/hosts\n",
                vec![],
            ),
            (
                "a key at column zero ends the block",
                "include:\n  - a.md\nother:\n  - b.md\n",
                vec!["a.md"],
            ),
        ] {
            assert_eq!(
                super::includes(text),
                expected,
                "the `include:` reader disagrees about {label}"
            );
        }
    }

    /// Each of those walks still has a fixture that plants in every real directory.
    ///
    /// Reads this file rather than running the walks: what is being checked is
    /// that the *call* is there, and a call that has been deleted cannot be
    /// observed by running anything.
    #[test]
    fn every_walk_that_needs_the_fixture_still_calls_it() {
        // **The code, not the text.** `source.contains` is satisfied by the
        // call's own text inside a comment — measured, the §15.1 walk's
        // assertion commented out byte-for-byte, this test green and the whole
        // suite unchanged, which is precisely the invisibility this list exists
        // to end.
        //
        // The first repair asked `!line.trim_start().starts_with("//")`, and a
        // block comment is not a line that starts with `//`: the same call
        // wrapped in `/* … */` passed again, after which one filter naming
        // `crates/nc-journal` and a hand-written `NC-KERN-4242` inside it left
        // `ok 32432 source line(s) name no error code as text (§15.1)` — the
        // baseline number, because a filter that pays both counters moves
        // neither. `without_comments` is the stripper the walks themselves use:
        // it follows `/* … */` across lines and knows a `//` inside a string
        // literal is not a comment.
        let raw = include_str!("gates.rs");
        // **What is code, decided per character.** The needle carries the walk's
        // label, which is a string literal — so it cannot be looked for in a copy
        // with literals blanked, and looking for it in the source finds any text
        // that spells it. `code_without_text` blanks comments and literals while
        // keeping the character count, so a position in it answers "is this
        // code?" about the same position in the source. The needle's *first*
        // character is the call's name, which is code in a real call and blank
        // inside any literal or comment holding a copy of it.
        //
        // The three defeats this closes, each measured with the §15.1 walk's
        // assertion deleted: the call commented out with `//`, the same wrapped
        // in `/* … */`, and
        // `const _: &str = r#"reached_every_one(&findings, &planted, "…")"#;`
        // written anywhere in the file.
        let bare: Vec<char> = super::code_without_text(raw).chars().collect();
        let source: Vec<char> = raw.chars().collect();
        assert_eq!(
            bare.len(),
            source.len(),
            "the scanner changed the character count, so a position in one no longer \
             names the same place in the other"
        );
        // The floor, on both ends: something was blanked, and the code was not.
        assert!(
            bare.iter().filter(|character| **character == ' ').count()
                > source.iter().filter(|character| **character == ' ').count(),
            "the scanner blanked nothing"
        );
        assert!(
            super::code_without_text(raw).contains("fn the_directories_that_really_exist"),
            "the scanner blanked the code this test judges"
        );
        let is_code = |needle: &str| {
            let needle: Vec<char> = needle.chars().collect();
            source.windows(needle.len()).enumerate().any(|(at, here)| {
                here == needle.as_slice() && bare.get(at).is_some_and(|first| *first != ' ')
            })
        };
        // **And the list itself has a floor.** Everything below asks whether
        // each *named* walk still calls the helper, and nothing asked whether
        // the names are still all of them — so deleting an entry and its call
        // site together left all 295 unit tests green with the §15.1 walk's
        // fixture assertion gone. The list is the only memory of the six; the
        // count of real calls is the second reader it did not have.
        // Counted as *plantings*, not as assertions. Counting the assertions
        // compares the list against the thing the list is about, so deleting an
        // entry and its call together leaves both smaller and this silent —
        // which is the edit a reviewer used, with all 295 unit tests green and
        // the §15.1 walk's coverage gone. A planting is a test saying "this walk
        // is under test"; the assertion is what it then proves. The first is the
        // population and the second is the claim, and only the first is
        // independent of the list.
        let plantings = count_of(&bare, "in_every_real_directory");
        assert_eq!(
            plantings,
            REACHED_BY_THE_PLANTED_FIXTURE.len() + PLANTED_WITHOUT_A_LABEL,
            "{plantings} mention(s) of the planting helper and {} walk(s) written down as \
             covered by it: a walk that plants and no longer asserts is one nothing \
             remembers was covered",
            REACHED_BY_THE_PLANTED_FIXTURE.len()
        );
        let mut missing = Vec::new();
        for (walk, test) in REACHED_BY_THE_PLANTED_FIXTURE {
            // The assertion helper, with this walk's label: the fixture plants
            // in every real directory and this is what says the walk arrived at
            // all of them — and **the whole call**, not its argument tail. The
            // needle was `&planted, "…")`, which
            // `let _ = (&findings, &planted, "…");` satisfies without calling
            // anything.
            //
            // **And once, because two copies are not two proofs.** The rule
            // asked whether the call is written *anywhere* in this file, so the
            // call could be deleted from the walk's own test and written a
            // second time inside another walk's — where it runs against that
            // walk's findings and proves nothing about this one. Measured: the
            // §15.1 walk's call moved into the section gate's test, with
            // `plantings` and the fixture list untouched, left all 48 tests and
            // the whole gate green.
            let calls: Vec<String> = ["planted", "judged"]
                .into_iter()
                .map(|over| format!("reached_every_one(&findings, &{over}, \"{walk}\")"))
                .filter(|call| is_code(call))
                .collect();
            // `is_code` answers whether it is code; this answers how many there
            // are and whether the one there is sits in a test that plants.
            // Comments are spent — a doc comment in this file quotes the call
            // verbatim, which is the string-literal twin `is_code` was hardened
            // against, seen from the other side — and literals are kept, because
            // the label is one.
            // **And in the test that plants for it.** The rule asked whether the
            // call is written *anywhere* in this file, so it could be deleted
            // from its own test and written a second time inside another's —
            // where it runs against that walk's findings and proves nothing
            // about this one. Measured: the §15.1 walk's call moved into the
            // citation gates' test, with the fixture list untouched, left all 48
            // tests and the whole gate green.
            //
            // Comments are spent first, because a doc comment in this file
            // quotes one of these calls verbatim — the string-literal twin
            // `is_code` was hardened against, seen from the other side.
            let spent = super::without_comments(raw, "//");
            let holds = spent
                .split("\n    fn ")
                .find(|body| {
                    body.contains(&format!(
                        "reached_every_one(&findings, &planted, \"{walk}\")"
                    )) || body.contains(&format!(
                        "reached_every_one(&findings, &judged, \"{walk}\")"
                    ))
                })
                .map(|body| body.split_once('(').map_or("", |(name, _)| name).to_owned());
            if calls.is_empty() || holds.as_deref() != Some(test) {
                missing.push(walk);
            }
        }
        assert!(
            missing.is_empty(),
            "these walks are written down as covered by the planted fixture and do not \
             call it exactly once, so a one-line filter naming a real subtree is \
             invisible to them again — and a second copy of the call, in another walk's \
             test, runs against that walk's findings: {missing:#?}"
        );

        the_proof_helper_is_still_the_proof(raw);
    }

    /// `reached_every_one` is still the proof, read whole.
    ///
    /// Split from the caller, which crossed clippy's line limit with it inline
    /// once wave 40 gave the helper its second reader.
    fn the_proof_helper_is_still_the_proof(raw: &str) {
        // And the helper itself is still an assertion. A `reached_every_one`
        // that stopped asserting would satisfy every line above.
        // **The definition, not this line.** `split_once("fn reached_every_one(")`
        // finds *this test's own source* first — the needle is written here —
        // so the "body" it read was the rest of this function, which of course
        // contains an `assert!`. The helper could be gutted entirely and this
        // stayed green. The definition is the one at column four.
        //
        // Read from the comment-stripped `code` for the same reason as above: an
        // `assert!` inside the helper that had been commented out would satisfy
        // a search of the raw text.
        let judged = super::code_without_text(raw);
        let helper = judged
            .split_once("\n    fn reached_every_one(")
            .expect("the helper is here")
            .1;
        let body = helper.split_once("\n    }\n").expect("it closes").0;
        // **Read whole, because neither pin tied the value asserted to the
        // value computed.** Two assertions stood here — that the body still
        // holds `assert!(` and `missed.is_empty()` — and one statement between
        // the computation and the assertion satisfies both: `let mut missed`,
        // then `missed.clear();`, and all six walks above prove
        // `assert!(empty.is_empty())` over an emptied vector. Measured on
        // 2026-09-11 together with a one-line filter hiding `crates/nc-canon/`
        // from the canary walk — the attack this fixture exists for, and the one
        // its own doc quotes — at `gates::` `48 passed; 0 failed` and
        // `check passed`.
        //
        // `code_without_text` has already blanked every string, so what is left
        // after dropping empty lines is the helper's statements and nothing
        // else. There is no line to add between them.
        // A blanked string leaves its line empty, and the punctuation that
        // closed the message leaves a lone comma: neither is a statement, and
        // dropping them keeps this list about the code rather than about how
        // long the failure message happens to be.
        let statements: Vec<&str> = body
            .lines()
            .filter(|line| !matches!(line.trim(), "" | ","))
            .collect();
        let held = [
            "findings: &[super::Finding], planted: &[(String, usize)], what: &str) {",
            "        let missed: Vec<&str> = planted",
            "            .iter()",
            "            .filter(|(file, line)| {",
            "                !findings",
            "                    .iter()",
            "                    .any(|finding| finding.file == *file && finding.line == *line)",
            "            })",
            "            .map(|(file, _)| file.as_str())",
            "            .collect();",
            "        assert!(",
            "            missed.is_empty(),",
            "            planted.len() - missed.len(),",
            "            planted.len()",
            "        );",
            "        let worn = the_paths_this_tree_wears();",
            "        let as_worn = planted",
            "            .iter()",
            "            .filter(|(file, _)| worn.contains(file))",
            "            .count();",
            "        assert!(",
            "            as_worn > 20,",
            "            planted.len()",
            "        );",
        ];
        assert_eq!(
            statements, held,
            "`reached_every_one` is not the proof written down. Read it whole: a \
             statement between the vector and the assertion — `missed.clear()` is one \
             line — turns all six walks above into `assert!(empty.is_empty())`, and a \
             pin that asks only whether the words are still there cannot see it"
        );
    }

    /// The repository's real directories, as a walk would name them.
    ///
    /// Read from the tree rather than listed, so a crate added later is covered
    /// without anyone remembering to add it — the same reason `SCOPED_OUT` is
    /// wrong to keep by hand.
    /// A manifest beside each planted source, because cargo acts on manifests.
    ///
    /// Its own function because the fixture it serves is at clippy's line limit,
    /// which is how this repository has split three others. The name
    /// deliberately does not contain `in_every_real_directory`: the planting
    /// census counts that string in this file's own source, and a helper named
    /// after it would move a number without a planting having been added.
    /// The plantings this walk skips are in this tree's two canary directories.
    ///
    /// Its own function for clippy's line limit. **It counts directories, not
    /// plantings**: the earlier form compared excused *plantings* against a
    /// literal, which held while the fixture planted two files per directory
    /// and broke the moment it planted one per name the tree wears.
    fn the_canary_directories_are_this_trees_two(planted: &[(String, usize)]) {
        let named = "canaries";
        let excused: std::collections::BTreeSet<&str> = planted
            .iter()
            .filter(|(file, _)| super::inside_a_canary_directory(file))
            .filter_map(|(file, _)| file.find(named).map(|at| &file[..at + named.len()]))
            .collect();
        assert!(
            excused.len() == 2,
            "{} canary director(ies) were excused and this tree has two: {excused:?}",
            excused.len()
        );
    }

    /// A canary directory at a member's root is one, under every member tree
    /// cargo resolves — a plugin's included — and the word deeper in a path
    /// is not.
    ///
    /// **A reviewer found a real bug with it**: the predicate's member helper
    /// knew `crates/*` and `xtask`, so a plugin's `canaries/` was read by the
    /// raising walk as shipped code while a crate's was excused.
    #[test]
    fn a_canary_directory_is_one_at_the_root_of_every_member() {
        for canary in [
            "crates/nc-canon/canaries/x.rs",
            "plugins/reference/canaries/x.rs",
            "xtask/canaries/x.rs",
        ] {
            assert!(super::inside_a_canary_directory(canary), "{canary}");
        }
        for shipped in [
            "plugins/reference/src/canaries/x.rs",
            "crates/nc-canon/src/canaries.rs",
        ] {
            assert!(!super::inside_a_canary_directory(shipped), "{shipped}");
        }
    }

    /// A file under every `.rs` name this tree wears, carrying one defect.
    ///
    /// The four walks that read `.rs` across the whole tree — the include walk,
    /// the blanket-allow walk, the hatch walk and §15.1's — have no kind
    /// dimension to vary, so the name is the only one their fixture can be
    /// blind in, and `f{n}.rs` is a name nothing wears. Every name planted
    /// where this tree wears it, and every name it wears in more than one
    /// directory planted in all of them.
    ///
    /// **The filler comes from the caller.** It was `// nothing here`, written
    /// here, and the hatch walk's own fixture says why that is not a detail: a
    /// plain `//` line directly above an allow *is* a written reason to that
    /// rule, so padding of that shape excuses the plant and the fixture proves
    /// the opposite of what it says. Wave 39 wrote this for one caller and
    /// wave 40 gave it to the other three.
    fn and_a_file_under_every_name_it_opens(
        root: &Path,
        planted: &mut Vec<(String, usize)>,
        defect: &str,
        filler: &str,
    ) {
        let tree = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let at = planted.first().map_or(1, |(_, line)| *line);
        let mut padding = String::new();
        for _ in 1..at {
            padding.push_str(filler);
            padding.push('\n');
        }
        let opened: Vec<(String, Vec<String>)> = the_names_this_tree_wears(tree)
            .into_iter()
            .filter(|(name, _)| {
                std::path::Path::new(name)
                    .extension()
                    .and_then(|kind| kind.to_str())
                    .is_some_and(|kind| kind == "rs")
            })
            .collect();
        assert!(
            opened.len() > 20,
            "{} `.rs` name(s) were found in this tree, which is not it",
            opened.len()
        );
        let directories = the_directories_that_really_exist(tree);
        for (index, directory) in directories.iter().enumerate() {
            for (nth, (name, wears_it)) in opened.iter().enumerate() {
                // **Where the tree really wears it, and everywhere if it wears
                // it in several.** A name planted in an arbitrary directory
                // answers a clause keyed on the *name* and nothing keyed on the
                // **path**: measured in wave 40, 127 of this tree's 138 read
                // names sit in exactly one directory and were planted somewhere
                // else entirely — `policy/schema.cedarschema` in
                // `crates/nc-log/src` — so `!path.ends_with(<that path>)` took
                // its eighteen live citations out of the gate with the whole
                // transcript one count line short of the clean run.
                let everywhere = wears_it.len() > 1;
                if !everywhere
                    && !wears_it.iter().any(|at| at == directory)
                    && nth % directories.len() != index
                {
                    continue;
                }
                let relative = format!("{directory}/{name}");
                let path = root.join(&relative);
                std::fs::create_dir_all(path.parent().expect("a parent"))
                    .expect("a writable scratch");
                std::fs::write(&path, format!("{padding}{defect}\n")).expect("a writable scratch");
                planted.push((relative, at));
            }
        }
    }

    fn the_spellings_that_reach_a_canary() -> [String; 5] {
        let directory = "canaries";
        [
            format!("#[path = \"../{directory}/a_code_written_by_hand.rs\"]\npub mod canary;"),
            format!("#[path=\"../{directory}/a_code_written_by_hand.rs\"]\npub mod canary;"),
            format!("include!(\"../{directory}/a_code_written_by_hand.rs\");"),
            format!("include_str!(\"../{directory}/a_code_written_by_hand.rs\");"),
            format!("include_bytes!(\"../{directory}/a_code_written_by_hand.rs\");"),
        ]
    }

    /// A file under every name the canary walk opens, beside each planted source.
    fn and_a_manifest_beside_each_source(root: &Path, planted: &mut Vec<(String, usize)>) {
        let tree = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let at = planted.first().map_or(1, |(_, line)| *line);
        // Assembled so that no line here holds both halves at once: this
        // file is read by the gate the fixture drives, and a line carrying
        // `path =` beside the directory name *is* the thing it refuses. The
        // reach above says so; written as one line, this made `check` report
        // itself, which is how wave 38 learned the lesson a second time.
        let directory = "canaries";
        let names = format!("path = \"{directory}/a_code_written_by_hand.rs\"");
        let mut filler = String::new();
        for _ in 1..at {
            filler.push_str("# nothing here\n");
        }
        // **Names, not only the manifest, and the reason is measured.** The walk
        // and the floor beside it both ask `is_read_by_the_canary_walk`, so a
        // clause added inside that predicate lowers what is read and what is
        // expected by the same lines and the equality holds straight across —
        // this campaign's own sentence, applied to a predicate. The only thing
        // that can refuse it is a fixture planting what the clause would
        // exclude, and this planted `f{n}.rs` plus a manifest: `f{n}.rs` is a
        // name nothing in this tree wears. Measured on 2026-09-27,
        // `&& !relative.ends_with("/main.rs")` inside the predicate left the
        // **whole suite green at 652** while the gate read nothing of `xtask`'s
        // own binary root, one directory from the canaries. `/lib.rs` was
        // caught by a single test, and only because a scratch in `check.rs`
        // happens to wear that name by house convention.
        //
        // The names come from the tree, filtered by the walk's own two lists:
        // every one planted somewhere, and every one the tree wears in more
        // than one directory planted in all of them — the shape wave 38 gave
        // the citation fixture, for the same reason.
        let opened: Vec<(String, Vec<String>)> = the_names_this_tree_wears(tree)
            .into_iter()
            .filter(|(name, _)| {
                std::path::Path::new(name)
                    .extension()
                    .and_then(|kind| kind.to_str())
                    .is_some_and(|kind| super::CANARY_READS.contains(&kind))
                    || super::CANARY_READS_BY_NAME.contains(&name.as_str())
            })
            .collect();
        assert!(
            opened.len() > 20,
            "{} name(s) this walk opens were found in the tree, which is not it",
            opened.len()
        );
        let directories = the_directories_that_really_exist(tree);
        for (index, directory) in directories.iter().enumerate() {
            for (nth, (name, wears_it)) in opened.iter().enumerate() {
                // **Where the tree really wears it, and everywhere if it wears
                // it in several.** A name planted in an arbitrary directory
                // answers a clause keyed on the *name* and nothing keyed on the
                // **path**: measured in wave 40, 127 of this tree's 138 read
                // names sit in exactly one directory and were planted somewhere
                // else entirely — `policy/schema.cedarschema` in
                // `crates/nc-log/src` — so `!path.ends_with(<that path>)` took
                // its eighteen live citations out of the gate with the whole
                // transcript one count line short of the clean run.
                let everywhere = wears_it.len() > 1;
                if !everywhere
                    && !wears_it.iter().any(|at| at == directory)
                    && nth % directories.len() != index
                {
                    continue;
                }
                let relative = format!("{directory}/{name}");
                let path = root.join(&relative);
                std::fs::create_dir_all(path.parent().expect("a parent"))
                    .expect("a writable scratch");
                std::fs::write(&path, format!("{filler}{names}\n")).expect("a writable scratch");
                planted.push((relative, at));
            }
        }
    }

    /// Every file name the citation gates open, and every directory this tree
    /// wears it in.
    ///
    /// Read from the tree for the same reason the directories are: a name it
    /// gains is planted on the day it gains it, not on the day somebody
    /// remembers. The second half of the pair is what decides *where*, and it
    /// was a `bool` — "in more than one directory" — until wave 40 measured
    /// what that costs. A name-keyed clause loses a finding wherever the name
    /// is planted; a *path*-keyed one only loses it where the tree really
    /// wears that name, and 127 of this tree's 138 read names sit in exactly
    /// one directory each. So the directories are returned rather than their
    /// count, and a caller plants each name where this tree wears it as well
    /// as wherever its spread by index lands. Found in wave 38, bounded in
    /// wave 40.
    ///
    /// **What it cannot return is a kind `sections::READ` does not name.** The
    /// walk below filters on that list, so a caller narrowing the result to a
    /// second list — `and_a_manifest_beside_each_source` narrows it to
    /// `super::CANARY_READS` — receives the intersection and can never see the
    /// difference. The two lists agree today, `rs` and `toml` being on both
    /// sides; a kind added to the canary lists alone would be planted under no
    /// name at all, and silently.
    fn the_names_this_tree_wears(root: &Path) -> Vec<(String, Vec<String>)> {
        let mut where_each_sits: std::collections::BTreeMap<
            String,
            std::collections::BTreeSet<String>,
        > = std::collections::BTreeMap::new();
        let _ = super::walk(root, &mut |path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return;
            };
            let read = path
                .extension()
                .and_then(|kind| kind.to_str())
                .is_some_and(|kind| crate::sections::READ.contains(&kind))
                || crate::sections::READ_BY_NAME.contains(&name);
            if !read {
                return;
            }
            let relative = super::relative_to(root, path);
            let directory = relative
                .rsplit_once('/')
                .map_or(String::new(), |(at, _)| at.to_owned());
            where_each_sits
                .entry(name.to_owned())
                .or_default()
                .insert(directory);
        });
        assert!(
            where_each_sits.len() > 100,
            "{} read file name(s) were found in this tree, which is not it",
            where_each_sits.len()
        );
        where_each_sits
            .into_iter()
            .map(|(name, directories)| (name, directories.into_iter().collect()))
            .collect()
    }

    fn the_directories_that_really_exist(root: &Path) -> Vec<String> {
        // **Walked, not shaped.** This was `crates/*/src`, `crates/*/tests` and
        // `xtask/`'s two — a fraction of the directories this
        // repository has. It named none of `migrations/`, `catalog/`, `policy/`,
        // `docs/`, `.github/`, `.cargo/`, any crate's `fixtures/` or
        // `canaries/`, or the root itself, and four of those are in `COVERED`
        // as "the directories the gates exist for". So the fixture that exists
        // because a filter can name a real subtree could not see a filter naming
        // one of those: measured, `if relative.starts_with("policy/") { walked
        // += 1; return; }` in the `.env` walk left a committed `.env.local`
        // holding a password and an AWS key on disk, with the printed path count
        // going *up* to 167 and `check passed`.
        //
        // A shape is a list with extra steps, and this file has removed several.
        // The tree is the list.
        let mut found = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(at) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&at) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                // What `walk` itself skips, asked of `walk`'s own predicate —
                // a directory this list leaves out is a directory no fixture
                // ever plants in. The three names spelled out here said the
                // same thing and were not the same thing: `super::is_skipped`
                // unskips a `target` with no workspace manifest beside it, and
                // unskips `.git` and `node_modules` outright, because a module
                // directory may be called anything and this repository has
                // neither a git directory (`§5.3`) nor a node package. The
                // walk descends into all three; this list left them out, so
                // the fixture that exists because a filter can name a real
                // subtree could not name one of them.
                if super::is_skipped(&at, &name) {
                    continue;
                }
                found.push(super::relative_to(root, &path));
                stack.push(path);
            }
        }
        found.sort();
        // The floor: a tree with a handful of directories would satisfy every
        // assertion below by having nothing to be filtered around. Forty-five
        // today, and the number is a floor rather than a pin because a crate
        // added is a directory added.
        assert!(
            found.len() > 30,
            "only {} real directory/directories were found under {}",
            found.len(),
            root.display()
        );
        found
    }

    /// A defect in **every** directory the real repository has, at a line and a
    /// name no filter can predict.
    ///
    /// **The path was the one thing a fixture never varied.** `somewhere_
    /// unpredictable` randomises the depth, the directory name, the file name
    /// and the line, and takes the part *inside* the repository — `"crates"`,
    /// `"crates/nc-catalog-compiler/src"` — as a literal from its caller. So a
    /// filter naming a real subtree is invisible to it: the scratch tree has no
    /// `crates/nc-journal/` in it to be skipped. Paying the counter on the way
    /// past defeats every floor and every identity, because both counters are
    /// inside the loop the filter is in. Measured, five one-line filters, five
    /// real defects, every count *up*, `check passed`:
    ///
    /// ```text
    /// ok    76133 source line(s) leave the workspace lints on   (baseline 76096)
    /// ok    29929 source line(s) name no error code as text     (baseline 29911)
    /// ok    965 citation(s) resolve                             (baseline 964)
    /// ok    166 path(s) — with crates/nc-journal/.env.local in one of them
    /// check passed
    /// ```
    ///
    /// A pid-derived path does not close it either: `crates/nc-{pid}/` is still
    /// not `crates/nc-journal/`. What closes it is a scratch tree whose
    /// directories **are** the repository's, with a defect in each — then a
    /// filter naming any one of them loses exactly that finding, and every
    /// caller of this asserts on all of them.
    fn in_every_real_directory(
        defect: &str,
        filler: &str,
    ) -> (
        crate::check::LeavesNothingBehind,
        std::path::PathBuf,
        Vec<(String, usize)>,
    ) {
        in_every_real_directory_each(&|_| defect.to_owned(), filler)
    }

    /// The same, with a defect of its own per directory.
    ///
    /// **One defect repeated cannot see a directory going missing.** A walk that
    /// stops reading one place still finds the identical code in the twenty
    /// others, so the assertion holds while the walk is short — measured on the
    /// raising walk, where a crate added to the skip chain left the fixture
    /// green. A distinct defect per place is what makes each place its own
    /// assertion.
    ///
    /// **The guard comes back with the tree, because a helper cannot outlive its
    /// caller's failure.** This removed the directory before writing it and
    /// never after, so every caller that asserted its way out left the whole
    /// mirrored tree behind — and eight callers share it. The guard is returned
    /// so the caller holds it, and `Drop` removes the tree on the path a
    /// panicking test takes. Found in the machine census of 2026-09-27.
    fn in_every_real_directory_each(
        defect: &dyn Fn(usize) -> String,
        filler: &str,
    ) -> (
        crate::check::LeavesNothingBehind,
        std::path::PathBuf,
        Vec<(String, usize)>,
    ) {
        use core::fmt::Write as _;

        // The counter that kept two trees in one process apart is the guard's
        // now, and it names the test as well as the run.
        let seed = std::process::id() as usize;
        let held = crate::check::LeavesNothingBehind::new("mirrored-tree");
        let root = held.root.clone();

        let real = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let longest = super::longest_file_read(real);
        let at = longest + 1 + seed % 100;
        assert!(at > longest && longest > 100, "the fixture is at line {at}");
        let mut padding = String::new();
        for _ in 1..at {
            padding.push_str(filler);
            padding.push('\n');
        }

        let mut planted = Vec::new();
        for (index, directory) in the_directories_that_really_exist(real).iter().enumerate() {
            // The unpredictable part, on top of the real one: depth, name and
            // line still come from the pid, so a filter keyed on any of those
            // misses this as well as on the path.
            let mut relative = directory.clone();
            for step in 0..=((seed + index) % 3) {
                let _ = write!(relative, "/d{}{step}", (seed + index) % 977);
            }
            let _ = write!(relative, "/f{}.rs", (seed + index) % 613);
            let path = root.join(&relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a writable scratch");
            std::fs::write(&path, format!("{padding}{}\n", defect(index)))
                .expect("a writable scratch");
            planted.push((relative, at));
        }
        (held, root, planted)
    }

    /// A `.sql` carrying the same code, in every real directory.
    ///
    /// Its own function for clippy's line limit, which the caller crossed once
    /// its `.rs` plantings learned the names this tree wears. §15.1's scope is
    /// `crates/` plus `migrations/`, and `migrations/` holds nothing but
    /// `.sql`; every fixture wrote `/f{n}.rs`, and the corpus's single `.sql`
    /// case puts its code on line 2. So a clause keyed on the comment marker
    /// *and* a line — measured in wave 38 — cleared both at once, and a
    /// registry code written out as text inside a live `RAISE EXCEPTION` in the
    /// journal's append-only trigger left the whole transcript byte-identical.
    /// The depth is not enough on its own: this plants at the same
    /// unpredictable line the `.rs` plant uses.
    fn and_a_sql_file_beside_each_source(root: &Path, planted: &mut Vec<(String, usize)>) {
        let tree = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        let at = planted.first().map_or(1, |(_, line)| *line);
        let mut filler = String::new();
        for _ in 1..at {
            filler.push_str("-- nothing here\n");
        }
        for directory in the_directories_that_really_exist(tree) {
            let relative = format!("{directory}/m{}.sql", at % 613);
            let path = root.join(&relative);
            std::fs::create_dir_all(path.parent().expect("a parent")).expect("a writable scratch");
            std::fs::write(&path, format!("{filler}SELECT 'NC-KERN-4242';\n"))
                .expect("a writable scratch");
            planted.push((relative, at));
        }
    }

    /// Every planted defect was reported, and the walk says which was not.
    ///
    /// **And the fixture planted under paths this tree really wears**, which is
    /// the half no list held. Wave 39 wrote the answer — plant every name the
    /// tree opens, not `f{n}.rs`, which is a name it wears nowhere — and gave
    /// it to one walk of the six; wave 40 measured the other five and found the
    /// same hole in all of them. A hand list of which fixtures plant names
    /// would have the shape this file has been burned by twice: three of the
    /// walks share one test body, so a list keyed on the body is satisfied by
    /// one of the three. This asks the fixture instead, at run time, and it
    /// asks it inside the helper every one of them already calls.
    fn reached_every_one(findings: &[super::Finding], planted: &[(String, usize)], what: &str) {
        let missed: Vec<&str> = planted
            .iter()
            .filter(|(file, line)| {
                !findings
                    .iter()
                    .any(|finding| finding.file == *file && finding.line == *line)
            })
            .map(|(file, _)| file.as_str())
            .collect();
        assert!(
            missed.is_empty(),
            "{what} reached {} of {} defects, each planted in a directory the real \
             repository has. A filter naming one subtree pays every counter on its way \
             past and leaves both the floor and the canary where they were, so this is \
             what says the walk went everywhere. Missed: {missed:#?}",
            planted.len() - missed.len(),
            planted.len()
        );
        let worn = the_paths_this_tree_wears();
        let as_worn = planted
            .iter()
            .filter(|(file, _)| worn.contains(file))
            .count();
        assert!(
            as_worn > 20,
            "{what}'s fixture planted {as_worn} of its {} file(s) at a path this tree \
             really wears. A name nothing wears answers a clause keyed on a name and \
             nothing at all keyed on a path, and every walk here pays its counter per \
             file against a floor that recounts the same ground — so both move together \
             and the equality holds straight across a clause that turned the walk off",
            planted.len()
        );
    }

    /// Every path this tree really wears a read name at.
    ///
    /// The second reader beside `reached_every_one`'s own. That one asks whether
    /// the walk arrived at what the fixture planted, which a fixture planting
    /// nothing but `f{n}.rs` satisfies completely — and `f{n}.rs` is a name this
    /// repository wears nowhere, so a clause keyed on a real name, or on a real
    /// relative path, walks past every one of them and the fixture says the walk
    /// went everywhere. Measured in wave 40, each with wave 39's repair already
    /// in the tree: `!relative.ends_with("parse.rs")` hid a hand-written
    /// `NC-KERN-4242` in `crates/nc-canon` from §15.1's walk with the whole
    /// transcript byte-identical, and one clause naming the policy schema by its
    /// real relative path took eighteen live citations out of the section gate
    /// with the transcript one count line short of a clean run.
    ///
    /// Read from the tree, so a name it gains is required of every fixture on
    /// the day it appears — and gutting this to return nothing makes the
    /// assertion it feeds fail rather than pass, which is the direction a
    /// derived expectation has to fail in to be worth having.
    fn the_paths_this_tree_wears() -> std::collections::BTreeSet<String> {
        let real = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        the_names_this_tree_wears(real)
            .into_iter()
            .flat_map(|(name, directories)| {
                directories
                    .into_iter()
                    .map(move |at| format!("{at}/{name}"))
            })
            .collect()
    }

    /// A scratch tree with one defect at a depth, a name and a line the caller
    /// cannot predict, and the path and line it was planted at.
    ///
    /// The process id is the source of all three. It is not secret and it is not
    /// random, and neither is the point: what matters is that it is not a
    /// constant a filter can be written against, and that it changes between the
    /// run that writes the filter and the run that judges it.
    fn somewhere_unpredictable(
        under: &str,
        defect: &str,
        filler: &str,
    ) -> (
        crate::check::LeavesNothingBehind,
        std::path::PathBuf,
        String,
        usize,
    ) {
        use core::fmt::Write as _;

        // **A number of its own per call.** The name was the pid and the
        // defect's length, so two plants of the same length in one process
        // shared a root — and each begins by removing it. Alone the test
        // passed; under the whole suite it failed on
        // `a writable scratch: Os { code: 2, kind: NotFound }`, which is the
        // same collision `nc-lints-{pid}` produced between two other tests.
        let seed = std::process::id() as usize;
        let held = crate::check::LeavesNothingBehind::new("planted");
        let root = held.root.clone();
        let mut relative = under.to_owned();
        for step in 0..=(seed % 4) {
            let _ = write!(relative, "/d{}{step}", seed % 977);
        }
        let _ = write!(relative, "/f{}.rs", seed % 613);
        let path = root.join(&relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("a writable scratch");
        // Past every line bound a filter is likely to carry, and not on a round
        // number. The defect is the last line, so `at` is the file's length.
        // **Past the longest file this suite reads, not between 200 and 296.**
        // The window was ninety-seven lines wide and written on this line, so a
        // filter carrying a constant above it — `if index > 300 { … }`, the same
        // shape with a different digit — was invisible to every fixture while
        // turning the walk off for the rest of each file. Measured: a blanket
        // allow inside a `mod` block near the end of criterion 3's own suite,
        // with both counters kept honest, gave `ok 73702 source line(s) leave the
        // workspace lints on` — up from baseline — and `check passed`.
        //
        // A defect past the longest real file has no window: a filter that lets
        // any real file's tail through lets this through, and the test fails.
        let longest = super::longest_file_read(std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.."
        )));
        let at = longest + 1 + seed % 100;
        assert!(
            at > longest && longest > 100,
            "the fixture is at line {at} and the longest file this suite reads is \
             {longest}: a filter tuned above the fixture would be invisible to it"
        );
        let mut text = String::new();
        for _ in 1..at {
            text.push_str(filler);
            text.push('\n');
        }
        text.push_str(defect);
        text.push('\n');
        std::fs::write(&path, text).expect("a writable scratch");
        (held, root, relative, at)
    }

    /// The walk reported the planted defect, at the line it is on.
    fn reached(findings: &[super::Finding], file: &str, line: usize, read: usize, what: &str) {
        assert!(
            findings
                .iter()
                .any(|finding| finding.file == file && finding.line == line),
            "{what} did not reach a defect planted at {file}:{line}. A filter between the \
             counter and the judgement leaves every count where it was and the canary \
             where it is, so this is what says the walk arrived: {findings:#?}"
        );
        assert!(
            read >= line,
            "{what} read {read} line(s) and the defect is on {line}"
        );
    }

    /// A blanket allow is one however the attribute is written.
    ///
    /// **The gate read one line and asked it to open and close.** rustfmt wraps
    /// an attribute once its list is long enough, so the hole opens by itself:
    ///
    /// ```rust
    /// #![allow(
    ///     clippy::all,
    ///     clippy::pedantic
    /// )]
    /// ```
    ///
    /// at the top of `crates/nc-journal/tests/tampering.rs` — acceptance
    /// criterion 3's own suite, with four real clippy violations underneath —
    /// gave `ok 64872 source line(s) leave the workspace lints on` and
    /// `check passed`. `cfg_attr` and `expect` were holes of their own: the
    /// first is the same wrapper that defeated the test census and the
    /// ignored-test gate in earlier rounds, and the second suppresses exactly
    /// as `allow` does.
    ///
    /// Driven through the walk rather than through the parser, because what was
    /// wrong was the line loop and not the name matching.
    #[test]
    fn a_blanket_allow_is_found_however_the_attribute_is_written() {
        // **A name of its own.** Two tests in this module built
        // `nc-lints-{pid}` — same process, same path, run concurrently by
        // libtest, each removing the directory the other was using. Measured, 12
        // runs at `--test-threads=4`: six red on nothing but that, with
        // `a writable scratch: Os { code: 2, kind: NotFound }`. It also failed
        // in the safe direction: this test's floor half asserts
        // `findings.is_empty()`, which passes for having inspected nothing when
        // the other has just wiped the tree.
        let root = std::env::temp_dir().join(format!("nc-blanket-allow-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("a writable scratch");
        for (about, text) in [
            ("on one line", "#![allow(clippy::all)]\nfn main() {}\n"),
            (
                "wrapped by rustfmt",
                "#![allow(\n    clippy::all,\n    clippy::pedantic\n)]\nfn main() {}\n",
            ),
            (
                "behind a condition that is always true",
                "#![cfg_attr(all(), allow(clippy::all))]\nfn main() {}\n",
            ),
            (
                "as an expectation",
                "#![expect(clippy::pedantic)]\nfn main() {}\n",
            ),
            (
                "with room around the path",
                "#![allow( clippy :: all )]\nfn main() {}\n",
            ),
        ] {
            let file = root.join(format!("{}.rs", about.replace(' ', "_")));
            std::fs::write(&file, text).expect("a writable scratch");
            let (findings, read) =
                super::no_source_puts_the_lints_down(&root).expect("the scratch walks");
            assert!(
                findings.iter().any(|finding| finding.file.ends_with(
                    file.file_name()
                        .expect("a named file")
                        .to_str()
                        .expect("utf-8")
                )),
                "a blanket allow written {about} was not reported: {findings:#?}"
            );
            assert!(read > 0, "the walk read nothing, so it judged nothing");
            std::fs::remove_file(&file).expect("a writable scratch");
        }

        // **The floor: one named lint is not a group.** A gate that reported
        // every `allow` would satisfy the loop above and make this repository
        // impossible to write — `§19.5`'s vacuous assertion arriving as an
        // over-strict rule instead of an absent one.
        for allowed in [
            "#![allow(clippy::needless_pass_by_value)]\nfn main() {}\n",
            "#![doc = \"a page with // in it\"]\n#[allow(clippy::all)]\nfn main() {}\n",
            // **A literal that spans a wrapped attribute.** Stripped per line
            // the quote state never balanced, the accumulator ran to its bound
            // and the gate said "an inner attribute here never closes its
            // brackets" — of an *outer* attribute, closed one line down, whose
            // author had written nothing wrong, while everything below it went
            // unjudged. The whole text goes through the stripper now, which is
            // what the sibling walk fifty lines up in this file already did.
            // **A literal continued with a backslash, whose closing bracket sits
            // on the continuation line.** Read per line, that line's trailing
            // quote is an *opening* one, so the `]` is spent as string content,
            // the `[` never closes, and the accumulator runs to its bound — the
            // gate then says "an inner attribute here never closes its brackets"
            // of an outer attribute that closes correctly, and everything below
            // it goes unjudged. Measured on `crates/nc-prune/src/postgres.rs`
            // by a reviewer in wave 21; the whole text goes through the
            // stripper now, which is what the sibling walk in this file already
            // did.
            "#[must_use = \"a message that \\\n              asks nothing\"]\npub fn f() {}\n",
            // **A shebang is not an inner attribute.** `#!` opens one only with
            // the `[`, and a script's first line carries neither brackets to
            // balance nor a lint to find — but read as an attribute it opens an
            // accumulator that swallows the file, and the counter below then
            // reports that nothing reached a verdict. Wave 21's mutation pass
            // found that clause surviving: every fixture the tree had was
            // decided the same way under both spellings, because a shebang
            // without a bracket closes on the line below it either way.
            "#!shebang(\nfn main() {}\n",
        ] {
            let file = root.join("allowed.rs");
            std::fs::write(&file, allowed).expect("a writable scratch");
            let (findings, _) =
                super::no_source_puts_the_lints_down(&root).expect("the scratch walks");
            assert!(
                findings.is_empty(),
                "this is not a blanket allow on the file: {allowed}\n{findings:#?}"
            );
            std::fs::remove_file(&file).expect("a writable scratch");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A code written by hand is one however its pieces are put together.
    ///
    /// **Four spellings assembled the same code past a scan of the literal
    /// text**, all four measured green in a crate the gate walks. Three of them
    /// are string glue and are read here; the fourth builds the digits at run
    /// time and cannot be, which is written down rather than pretended about.
    #[test]
    fn a_code_assembled_from_pieces_is_still_written_by_hand() {
        for (way, line) in [
            ("plainly", "    const HAND: &str = \"NC-KERN-4242\";"),
            (
                "through `concat!`",
                "    const HAND: &str = concat!(\"NC-\", \"KERN-4242\");",
            ),
            (
                "with a hex escape",
                "    const HAND: &str = \"NC-KERN-424\\x32\";",
            ),
            (
                "with the pieces spaced out",
                "    const HAND: &str = concat!( \"NC-\" , \"KERN-4242\" );",
            ),
        ] {
            let found = super::codes_in(line)
                .into_iter()
                .chain(super::codes_in(&super::glued(line)))
                .count();
            assert!(found > 0, "a code written {way} was not read: {line}");
        }
        // **And the prose that is not a code**, measured as a false `error`:
        // removing every space made an English sentence with spaced hyphens
        // read as `NC-CANON-0001`, and the gate reported a doc attribute in
        // `nc-canon` as a hand-written code — in one of two byte-equivalent
        // spellings of the same comment.
        for prose in [
            "    #[doc = \"Historically raised as NC - CANON - 0001 before the registry.\"]",
            "    /// Historically raised as NC - CANON - 0001 before the registry.",
            "    // the KERN area holds no codes yet, so NC - KERN - 0001 is free",
        ] {
            assert!(
                super::codes_in(prose).is_empty()
                    && super::codes_in(&super::glued(prose)).is_empty(),
                "prose was read as a code: {prose}"
            );
        }
        // The floor: a rule that answered "a code" to everything would satisfy
        // every line above.
        for ordinary in [
            "    let total = counts.get(\"NC\").unwrap_or(&0) + 4242;",
            "    // the KERN area holds no codes yet",
            "    const VERSION: &str = \"1.2.3\";",
        ] {
            assert!(
                super::codes_in(ordinary).is_empty()
                    && super::codes_in(&super::glued(ordinary)).is_empty(),
                "ordinary code was read as an error code: {ordinary}"
            );
        }
    }

    /// A condition above a test is a hidden test, however it is spelled.
    ///
    /// Every entry below was free before this gate stopped being a list of
    /// spellings: each one hides a test and each was invisible both to the gate
    /// and — for the last two — to the census in `check.rs` that is supposed to
    /// floor it. `#[cfg(target_os = "vxworks")]` is the one a reviewer reached
    /// for, and it needed no feature declaration and no `any()` trick.
    #[test]
    fn a_condition_above_a_test_is_a_hidden_test_however_it_is_spelled() {
        for hiding in [
            "#[ignore]",
            "#[cfg(feature = \"audit\")]",
            "#[cfg(any())]",
            "#[cfg(target_os = \"vxworks\")]",
            "#[cfg(all(unix, not(unix)))]",
            "#[cfg(nothing_defines_this)]",
            "#[cfg_attr(all(), ignore = \"…\")]",
        ] {
            let text = format!("{hiding}\n#[test]\nfn a_test() {{}}\n");
            let findings = super::no_ignored_test("a.rs", &text);
            assert_eq!(
                findings.len(),
                1,
                "`{hiding}` hides a test and this gate did not say so"
            );
            // And the other way round: the attribute below the test's own is
            // reached too, which is where rustfmt puts a second one.
            let below = format!("#[test]\n{hiding}\nfn a_test() {{}}\n");
            assert_eq!(
                super::no_ignored_test("a.rs", &below).len(),
                1,
                "`{hiding}` under `#[test]` hides the same test"
            );
        }
    }

    /// The spellings a formatter or an ordinary author produces, and the ones a
    /// reviewer reached for.
    ///
    /// Every entry was measured **passing** against the version of this gate
    /// that read the attribute run as "lines beginning with `#[`". None needed
    /// ingenuity: a blank line between two attributes, a doc comment between the
    /// attribute and the item, and a `cfg_attr` wrapped across lines are all
    /// things `cargo fmt` and ordinary editing produce. The last two are the
    /// expensive ones — a condition on a `mod x;` statement deletes every test
    /// in another file without touching it.
    #[test]
    fn the_attribute_run_is_read_however_it_is_laid_out() {
        for (way, text) in [
            (
                "a blank line between the attributes",
                "#[test]\n\n#[ignore]\nfn a_test() {}\n",
            ),
            (
                "a blank line, the other way round",
                "#[ignore]\n\n#[test]\nfn a_test() {}\n",
            ),
            (
                "a doc comment between the attribute and the item",
                "#[cfg(any())]\n#[test]\n/// what it does\nfn a_test() {}\n",
            ),
            (
                "a `cfg_attr` wrapped across lines",
                "#[cfg_attr(\n    all(),\n    ignore = \"…\"\n)]\n#[test]\nfn a_test() {}\n",
            ),
            (
                "a `cfg` wrapped across lines",
                "#[cfg(\n    any()\n)]\n#[test]\nfn a_test() {}\n",
            ),
            (
                "a configured tokio test",
                "#[cfg(all(unix, not(unix)))]\n#[tokio::test(flavor = \"multi_thread\")]\nasync fn a_test() {}\n",
            ),
            (
                "a condition on the `mod` that declares a test file",
                "#[cfg(unix)]\nmod tampering;\n",
            ),
            (
                "a false condition on the `mod` that declares a test file",
                "#[cfg(any())]\nmod tampering;\n",
            ),
        ] {
            assert_eq!(
                super::no_ignored_test("a.rs", text).len(),
                1,
                "{way} hides a test and this gate did not say so"
            );
        }
    }

    /// The census and the gate agree on what a test attribute is.
    ///
    /// They were two lists, and the gap between them was worth one hidden test
    /// per spelling: the census counted `#[test]` and `#[tokio::test]` exactly,
    /// the gate matched two other prefixes, and `#[tokio::test(flavor = …)]` was
    /// in neither. Now there is one predicate and this is it.
    #[test]
    fn every_way_of_writing_a_test_is_a_test_to_both() {
        for spelling in [
            "#[test]",
            "    #[test]",
            "#[tokio::test]",
            // Absolute paths, which are ordinary Rust and which `rustc --test`
            // runs. `::tokio::test` was invisible to this predicate and, because
            // the census shares it, to the count that is supposed to floor it.
            "#[::tokio::test]",
            "#[::std::prelude::v1::test]",
            "#[std::prelude::v1::test]",
            "#[tokio::test(flavor = \"multi_thread\")]",
            "#[tokio::test(start_paused = true)]",
            "#[cfg_attr(all(), test)]",
            "#[cfg_attr(all(), tokio::test)]",
            // Absolute paths inside a `cfg_attr`, which the bare form was made
            // to resolve and this branch was not.
            "#[cfg_attr(all(), ::tokio::test)]",
            "#[cfg_attr(unix, ::std::prelude::v1::test)]",
            // A condition holding its own commas.
            "#[cfg_attr(all(unix, not(windows)), test)]",
            "#[::core::prelude::v1::test]",
        ] {
            assert!(
                super::is_a_test_attribute(spelling),
                "`{spelling}` declares a test and was not counted as one"
            );
        }
        for not_a_test in [
            "#[cfg(test)]",
            "#[testing]",
            "#[test_case]",
            "fn test() {}",
            "// #[test]",
        ] {
            assert!(
                !super::is_a_test_attribute(not_a_test),
                "`{not_a_test}` is not a test attribute"
            );
        }
    }

    /// And the three conditions this tree really carries are not findings.
    ///
    /// The floor for the rule above: a gate that answered "hidden" to every
    /// `cfg` would satisfy that test and make this file impossible to write.
    #[test]
    fn a_condition_that_is_not_about_a_test_is_left_alone() {
        for allowed in [
            "#[cfg(test)]\nmod tests {\n}\n",
            "#[cfg(any(test, feature = \"test-support\"))]\npub fn helper() {}\n",
            "#[test]\nfn a_test() {\n    #[cfg(unix)]\n    {\n        let _ = 1;\n    }\n}\n",
        ] {
            assert!(
                super::no_ignored_test("a.rs", allowed).is_empty(),
                "this is not a hidden test: {allowed}"
            );
        }
    }

    /// A test module is one by what it holds, whatever it is called.
    ///
    /// **Thirty of this tree's test modules are not called `tests`.** The gate
    /// keyed `about_a_test` on the name, so `query_tests` (50 tests),
    /// `the_frozen_path_rule` (52), `hostile_store_tests` (18),
    /// `every_report_wire` (16) and `narrowing` (7) were none of them about a
    /// test, and a second condition on one was invisible.
    ///
    /// Measured, `#[cfg(test)]` and `#[cfg(not(test))]` together above
    /// `mod width_tests` in `crates/nc-ids/src/base32.rs`: `cargo test -p xtask`
    /// gave 401 passed and `cargo run -p xtask -- check` printed
    /// `check passed`, the only trace being `1072 test(s)` becoming `1070`. The
    /// census in `check.rs` is the only other reader, and it carries two tests
    /// of slack — doctests count towards `passed` and are not `#[test]`
    /// attributes — so hiding one or two moved no number anywhere.
    ///
    /// Both directions, because widening this without widening the exemption
    /// beside it turns every `#[cfg(test)]` module in the tree into a finding.
    #[test]
    fn a_test_module_is_one_by_what_it_holds() {
        let hidden = "#[cfg(test)]\n#[cfg(not(test))]\nmod width_tests {\n    \
                      #[test]\n    fn a_test() {}\n}\n";
        assert_eq!(
            super::no_ignored_test("a.rs", hidden).len(),
            1,
            "a second condition on a test module called anything but `tests` was not \
             reported, so every test inside it is deleted from the run and the only \
             other reader is a census with two tests of slack"
        );
        // The ordinary spelling of a test module, under any name, is not a
        // finding — or thirty modules in this tree become one each.
        for allowed in [
            "#[cfg(test)]\nmod width_tests {\n    #[test]\n    fn a_test() {}\n}\n",
            "#[cfg(test)]\npub mod query_tests {\n    #[test]\n    fn a_test() {}\n}\n",
        ] {
            assert!(
                super::no_ignored_test("a.rs", allowed).is_empty(),
                "this is how a test module is written: {allowed}"
            );
        }
        // And a module with no test in it is a module, not a test module: a
        // condition on one is somebody's build decision.
        let left_over = super::no_ignored_test(
            "a.rs",
            "#[cfg(test)]\n#[cfg(not(test))]\nmod width_tests {\n    \
                 pub fn helper() {}\n}\n",
        );
        assert!(
            left_over.is_empty(),
            "a module holding no test was read as a test module, so the rule now \
             reports every conditional module in the tree: {left_over:?}"
        );
    }

    /// A condition on the whole file hides every test in it.
    #[test]
    fn a_condition_on_the_file_hides_every_test_below_it() {
        let text = "#![cfg(feature = \"audit\")]\n#[test]\nfn a_test() {}\n";
        assert_eq!(super::no_ignored_test("a.rs", text).len(), 1);
        // A file with no test in it is a module gate, not a hidden test.
        assert_eq!(
            super::no_ignored_test("a.rs", "#![cfg(unix)]\npub fn f() {}\n"),
            [] as [crate::gates::Finding; 0]
        );
    }

    use super::{
        COVERED, NOTHING_ELSE_MAY_BE_SKIPPED, SKIP, colours_come_through_the_design_system,
        committed_env_files, every_member_takes_the_workspace_lints, manifests,
        no_compile_time_query, walk,
    };
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
    }

    #[test]
    fn a_manifest_naming_the_colour_crate_is_a_finding() {
        let findings = colours_come_through_the_design_system(
            "Cargo.toml",
            "[dependencies]\nnoctua-colors-tokens = \"0.2\"\n",
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn a_manifest_that_does_not_name_it_is_not_a_finding() {
        // The other half. Without this, inverting the condition would still pass
        // the test above.
        let findings = colours_come_through_the_design_system(
            "Cargo.toml",
            "[dependencies]\nnoctua-design = { path = \"../noctua-design\" }\n",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// Every shape that left the stripper reading a string as code, or code as prose.
    ///
    /// Each of these was measured through the gate's own functions, and each is
    /// the same failure in both directions: a `//` inside a string read as a
    /// comment cuts real code out of the scan, and a `--` outside one read as
    /// code leaves a comment in it. A gate that cannot tell them apart reports
    /// its own prose or misses a live call.
    #[test]
    fn a_string_is_not_a_comment_and_a_comment_is_not_a_string() {
        // **Spelled from two halves**, because this gate reads its own source:
        // written out, the fixtures below would each be a finding against the
        // file that defines the gate. `COMPILE_TIME_QUERIES` is built the same
        // way and for the same reason.
        let call = format!("sqlx::{}(\"S\")", concat!("query", "!"));
        // **The one the last fix broke.** `r#"…"#` was closed correctly and
        // `r"…"` — no hashes — closed at its own opening quote, because
        // `closes_raw(_, _, 0)` is `(0..0).all(…)`, which is true.
        for opener in ["r", "r#", "r##", "br", "br#", "cr", "cr#"] {
            let hashes = opener.trim_start_matches(['r', 'b', 'c']);
            let line = format!("let e = {opener}\"http://x\"{hashes}; let n = {call};\n");
            let findings = no_compile_time_query("src/x.rs", &line);
            assert_eq!(
                findings.len(),
                1,
                "`{opener}\"…\"{hashes}` swallowed the rest of its line: {findings:#?}"
            );
        }

        // **A char literal holding a quote**, which this file's own source
        // contains — it inverted the quote state for the rest of the line, so
        // the `//` of an ordinary URL after it was read as a comment and
        // everything past it was dropped. Measured: **0** findings.
        //
        // A string with a `//` in it has to follow the literal, or the mis-read
        // costs nothing: `without_comments` keeps strings verbatim and only
        // removes comments, so the damage needs a comment marker to land on.
        for literal in ["'\"'", "'\\''", "'\\n'", "'\\u{{1F600}}'"] {
            let shape = format!("let c = {literal}; let e = \"http://x\"; let n = {call};\n");
            assert_eq!(
                no_compile_time_query("src/x.rs", &shape).len(),
                1,
                "this line lost its call: {shape}"
            );
        }
        // A lifetime is not a literal, and reading one as a literal would
        // swallow the code after it just as surely.
        let lifetime = format!("fn f<'a>(x: &'a str) {{ let e = \"http://x\"; {call}; }}\n");
        assert_eq!(no_compile_time_query("src/x.rs", &lifetime).len(), 1);

        // **A `//` inside an ordinary string is not a comment**, which is the
        // case the raw-string work exists beside.
        let url = format!("let e = \"http://x\"; let n = {call};\n");
        assert_eq!(no_compile_time_query("src/x.rs", &url).len(), 1);

        // **SQL's string quote is `'`, and the stripper only knew `\"`.** Both
        // directions, measured: real content cut out of the first, and in the
        // second the exact hole the migration-comment fix closed, reopened by
        // one `\"`.
        let sql = super::without_comments("INSERT INTO t VALUES ('a -- b'); CODE\n", "--");
        assert!(
            sql.contains("CODE") && sql.contains("-- b"),
            "a `--` inside a SQL string was read as a comment: {sql:?}"
        );
        let identifier = super::without_comments("SELECT 'the \" char'; -- PROBE\n", "--");
        assert!(
            !identifier.contains("PROBE"),
            "a `--` comment survived a string holding a quote: {identifier:?}"
        );
        // A doubled quote is SQL's escape, and the line still ends closed.
        let doubled = super::without_comments("SELECT 'it''s'; -- PROBE\n", "--");
        assert!(!doubled.contains("PROBE"), "{doubled:?}");

        // **`/* … */` is a comment in Rust and not in YAML or shell.** Treating
        // it as one everywhere cuts real content out of those files: a workflow
        // step or a shell line holding `/*` — a glob, a path, a comment marker
        // of some other language — would swallow everything to the next `*/`,
        // which may be the end of the file.
        let globbed = "run: ls /* && cargo fmt --all\nrun: cargo xtask check\n";
        let shell = super::without_comments(globbed, "#");
        assert!(
            shell.contains("cargo xtask check"),
            "a `/*` in a shell line swallowed the rest of the file: {shell:?}"
        );
        // And in Rust it is a comment, including nested and across lines.
        let nested = super::without_comments(
            "let a = 1; /* outer /* inner */ still */ let b = 2;\n",
            "//",
        );
        assert!(
            nested.contains("let a = 1;") && nested.contains("let b = 2;"),
            "{nested:?}"
        );
        assert!(!nested.contains("inner"), "{nested:?}");

        // **And a fixture that needs the raw string to be raw.** The loop above
        // holds `r"…"` — no interior quote — and without an interior quote a
        // raw string and an ordinary string are parsed identically, so the
        // whole `br`/`cr` half of `raw_string_at` could be deleted with the loop
        // still passing. What a raw string is *for* is the interior quote:
        //
        // ```text
        //           shipped   with br/cr removed
        //   br#        1              0
        //   cr#        1              0
        // ```
        for opener in ["r#", "br#", "cr#", "r##", "br##"] {
            let hashes = opener.trim_start_matches(['r', 'b', 'c']);
            let line = format!("let e = {opener}\"a \"http://x\" b\"{hashes}; let n = {call};\n");
            let findings = no_compile_time_query("src/x.rs", &line);
            assert_eq!(
                findings.len(),
                1,
                "`{opener}` holding a quote was not read as a raw string: {findings:#?}"
            );
        }

        // **A char literal is Rust's too**, and the `rust &&` on that branch had
        // no test while five of its siblings did. Most inputs cannot tell the
        // two apart: consuming a literal and opening-then-closing a string end
        // at the same character, so the fixture has to be one where the quotes
        // do *not* pair up.
        //
        // Three quotes in a row is SQL's escaped quote at the start of a string
        // that has not closed, so what follows is string content and a comment
        // marker in it is not a comment. Read as a char literal instead, the
        // three vanish and the rest of the line is stripped as prose.
        let sql = super::without_comments("SELECT '''-- NC_AUDIT_PROBE\n", "--");
        assert!(
            sql.contains("NC_AUDIT_PROBE"),
            "an unterminated SQL string was read as a char literal, and its content \
             stripped as a comment: {sql:?}"
        );

        // **A raw-string opener is Rust's, and these files are not Rust.** A
        // raw string does not reset at a newline — that is what makes it a raw
        // string — so one `-r"` in a shell or YAML line put the scanner inside
        // a string for the rest of the file, and every `#` comment after it was
        // read as code. `no_unread_variable` then counts a variable named only
        // in a comment as read, which is that gate's own stated failure.
        let yaml = super::without_comments(
            "run: grep -r\"x\n# NC_AUDIT_PROBE is prose here.\nrun: cargo test\n",
            "#",
        );
        assert!(
            !yaml.contains("NC_AUDIT_PROBE"),
            "a stray `r\"` swallowed the rest of a YAML file: {yaml:?}"
        );
        assert!(yaml.contains("cargo test"), "{yaml:?}");
        // The same shape in SQL, whose comment marker is different and whose
        // quote is `'`.
        let sql = super::without_comments(
            "SELECT r\"x;\n-- NC_AUDIT_PROBE is prose here.\nSELECT 1;\n",
            "--",
        );
        assert!(!sql.contains("NC_AUDIT_PROBE"), "{sql:?}");

        // And `'` is a quote in shell and YAML too, where it was not one.
        let shell = super::without_comments("echo 'a # b'; CODE # gone\n", "#");
        assert!(
            shell.contains("a # b") && shell.contains("CODE") && !shell.contains("gone"),
            "{shell:?}"
        );
    }

    /// A raw string leaves [`super::without_comments`] still closed.
    ///
    /// The opener is copied whole, so a closer of one `"` writes `r#"…"` back —
    /// a raw string that never ends. Nothing reads this output as prose: five
    /// callers hand it straight to [`super::without_string_contents`], which is
    /// raw-string aware, finds no close, and spends the rest of the file as
    /// string content. The damage is silent and it is not local: one legitimate
    /// raw string near the top of a file takes every rule below it out of
    /// service, and the rules that go quiet are the ones that count things, so
    /// they report a smaller number rather than an error.
    #[test]
    fn a_raw_string_is_closed_as_wide_as_it_opened() {
        // Assembled rather than written out, so this fixture is not itself a
        // raw string in a file the raw-string walks read.
        let hash = "#";
        let source = format!("let a = r{hash}\"x\"{hash};\nlet b = 1;\nlet c = 2;\n");

        let stripped = super::without_comments(&source, "//");
        assert!(
            stripped.contains("let c = 2"),
            "the tail of the file survives the stripper itself: {stripped:?}"
        );

        // The measurement that matters is the composition, because that is what
        // every caller writes.
        let code = super::without_string_contents(&stripped);
        assert!(
            code.contains("let b = 1") && code.contains("let c = 2"),
            "a raw string closed one character short, so everything after it was \
             read as string content: {code:?}"
        );

        // And the same source with one more hash, because the bug was written as
        // a constant width and a fix can be too.
        let wider = format!("let a = r{hash}{hash}\"x\"{hash}{hash};\nlet b = 1;\nlet c = 2;\n");
        let code = super::without_string_contents(&super::without_comments(&wider, "//"));
        assert!(code.contains("let c = 2"), "{code:?}");
    }

    #[test]
    fn the_name_in_a_comment_is_prose_and_not_a_dependency() {
        // Every manifest in this repository explains in a comment why the colour
        // crate is absent. A gate that read its own justification as a violation
        // would be unusable on the first day.
        let findings = colours_come_through_the_design_system(
            "Cargo.toml",
            "# noctua-colors is deliberately absent.\n",
        );
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// The manifest is read the way cargo reads it, not scanned for a word.
    ///
    /// **Two spellings walked past a text scan in succession.** `split('#')` cut
    /// the line wherever the character stood, so a fragment in a git URL hid the
    /// rest of it; the shared comment stripper closed that and nothing else, and
    /// then `"noctua\u002Dcolors" = "0.4"` held no literal `noctua-colors`
    /// anywhere in the file. Measured, that key in the root
    /// `[workspace.dependencies]`: `ok 10 manifest(s) reach the colour system
    /// through the design system` and `check passed`, while `tomllib` and cargo
    /// both read the key as `noctua-colors`.
    ///
    /// Both ways round, because every manifest in this repository explains in a
    /// comment why the colour crate is absent, and a gate that read its own
    /// justification as a violation would be unusable on the first day.
    #[test]
    fn the_colour_crate_is_found_however_the_manifest_spells_it() {
        for (manifest, named) in [
            // The plain spelling.
            ("[dependencies]\nnoctua-colors = \"1\"\n", 1),
            // A `#` in a value is not a comment.
            (
                "[dependencies]\ncolors = { git = \"https://x.example/d.git#v2\", package = \
                 \"noctua-colors\" }\n",
                1,
            ),
            // An escape is the character it names.
            (
                "[workspace.dependencies]\n\"noctua\\u002Dcolors\" = \"0.4\"\n",
                1,
            ),
            // A rename is a dependency on what it renames.
            (
                "[dev-dependencies]\ncolours = { package = \"noctua-colors\", version = \"1\" }\n",
                1,
            ),
            // And every table cargo reads.
            ("[build-dependencies]\nnoctua-colors = \"1\"\n", 1),
            (
                "[target.'cfg(unix)'.dependencies]\nnoctua-colors = \"1\"\n",
                1,
            ),
            // The prose every manifest here carries is not a dependency.
            ("# noctua-colors is deliberately absent, see §8.6\n", 0),
            (
                "[dependencies]\nnoctua-design = \"1\" # not noctua-colors\n",
                0,
            ),
            // A crate whose name is the colour system's with something on the
            // end is the same second copy under another package.
            ("[dependencies]\nnoctua-colors-extra = \"1\"\n", 1),
        ] {
            let findings = colours_come_through_the_design_system("Cargo.toml", manifest);
            assert_eq!(
                findings.len(),
                named,
                "`{manifest}` names the colour crate {named} time(s) and the gate found \
                 {}. A manifest it misses declares a second copy of the colour system at \
                 a version nothing keeps in step",
                findings.len()
            );
        }
    }

    #[test]
    fn every_finding_carries_a_remedy_that_is_not_the_defect_restated() {
        // Length is not the test: a reviewer defeated the old version of this by
        // writing a hundred and forty characters saying only what was wrong.
        // What is checked now is that a remedy exists in its own field and says
        // something the defect did not.
        let findings = colours_come_through_the_design_system(
            "Cargo.toml",
            "[dependencies]\nnoctua-colors = \"1\"\n",
        );
        let finding = &findings[0];
        assert!(!finding.fix.is_empty(), "no remedy");
        assert_ne!(finding.fix, finding.what, "the remedy restates the defect");
        assert!(
            !finding.what.contains(&finding.fix) && !finding.fix.contains(&finding.what),
            "one field contains the other, so only one thing was said"
        );
    }

    #[test]
    fn a_committed_env_file_is_found_at_any_depth_and_a_missing_one_is_not() {
        // The gate had no test at all until a reviewer replaced its body with
        // `Ok(Vec::new())` and watched the whole suite stay green.
        let scratch = std::env::temp_dir().join(format!("nc-env-gate-{}", std::process::id()));
        let nested = scratch.join("services").join("auth");
        std::fs::create_dir_all(&nested).expect("the scratch directory is writable");
        // A file that is not a `.env`, so the clean case has something to walk.
        // Without it the count is legitimately zero and would say nothing.
        std::fs::write(nested.join(".env.example"), "SECRET=\n").expect("writable");

        let (clean, walked) = committed_env_files(&scratch).expect("readable");
        assert!(clean.is_empty(), "a tree with no .env produced {clean:?}");
        // The count, so a walk that reached nothing cannot read as a clean tree.
        assert!(walked > 0, "the walk visited nothing and reported clean");

        std::fs::write(nested.join(".env"), "SECRET=1\n").expect("writable");
        let (findings, _) = committed_env_files(&scratch).expect("readable");
        assert_eq!(
            findings.len(),
            1,
            "a nested .env was not found: {findings:?}"
        );
        assert!(
            findings[0].file.ends_with("services/auth/.env"),
            "{:?}",
            findings[0].file
        );
        assert!(
            !findings[0].file.starts_with('/'),
            "the path is absolute, which puts the developer's home directory in the message: {}",
            findings[0].file
        );

        // **Every shape, not the one spelling.** `.env.example` promises "the
        // gate is absolute so that nobody has to remember which `.env` was the
        // safe one" — and it matched `.env` exactly, so `.env.local` and
        // `.env.production` were neither ignored by git nor caught here. Three
        // credential-bearing files would have gone into a clone.
        std::fs::remove_file(nested.join(".env")).expect("writable");
        for carries_secrets in [".env.local", ".env.production", ".env.development.local"] {
            std::fs::write(nested.join(carries_secrets), "SECRET=1\n").expect("writable");
            let (findings, _) = committed_env_files(&scratch).expect("readable");
            assert!(
                findings.iter().any(|f| f.file.ends_with(carries_secrets)),
                "`{carries_secrets}` is a real environment and was not caught: {findings:?}"
            );
            std::fs::remove_file(nested.join(carries_secrets)).expect("writable");
        }
        // And the templates stay legal, since committing one is the point.
        for template in [".env.example", ".env.sample", ".env.template"] {
            std::fs::write(nested.join(template), "SECRET=\n").expect("writable");
        }
        let (findings, _) = committed_env_files(&scratch).expect("readable");
        assert!(
            findings.is_empty(),
            "a template was reported as a committed environment: {findings:?}"
        );

        std::fs::remove_dir_all(&scratch).expect("removable");
    }

    #[test]
    fn a_member_outside_the_workspace_lints_is_a_finding() {
        // Two deleted lines took `unsafe_code = "forbid"` and `missing_docs`
        // off a crate with `check passed`. A crate added without the stanza is
        // unlinted from birth, which is the more likely way it happens.
        let scratch = std::env::temp_dir().join(format!("nc-lints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(scratch.join("crates/free/src")).expect("writable");
        std::fs::write(scratch.join("Cargo.toml"), "[workspace]\n").expect("writable");
        std::fs::write(
            scratch.join("crates/free/Cargo.toml"),
            "[package]\nname = \"free\"\n",
        )
        .expect("writable");
        let (findings, checked) =
            every_member_takes_the_workspace_lints(&scratch).expect("readable");
        assert_eq!(checked, 1);
        assert!(
            findings
                .iter()
                .any(|finding| finding.file.contains("crates/free/Cargo.toml")),
            "a member outside the lints was not reported: {findings:#?}"
        );

        // **A `workspace = true` in some *other* table is not the stanza.** The
        // gate asked two unrelated questions — is there a line reading
        // `workspace = true`, and does the text contain `[lints]` — so moving
        // the key into `[dev-dependencies.proptest]` and mentioning `[lints]` in
        // a comment satisfied both, with `unsafe_code = "forbid"` off and
        // `check passed`.
        std::fs::write(
            scratch.join("crates/free/Cargo.toml"),
            "[package]\nname = \"free\"\n\n             # The workspace's [lints] table is declared at the root.\n             [dev-dependencies.proptest]\nworkspace = true\n",
        )
        .expect("writable");
        let (findings, _) = every_member_takes_the_workspace_lints(&scratch).expect("readable");
        assert!(
            findings
                .iter()
                .any(|finding| finding.file.contains("crates/free/Cargo.toml")),
            "a `workspace = true` in another table passed as the lints stanza: {findings:#?}"
        );

        // And with the stanza, nothing.
        std::fs::write(
            scratch.join("crates/free/Cargo.toml"),
            "[package]\nname = \"free\"\n\n[lints]\nworkspace = true\n",
        )
        .expect("writable");
        let (findings, _) = every_member_takes_the_workspace_lints(&scratch).expect("readable");
        assert!(findings.is_empty(), "{findings:#?}");
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn nothing_may_be_added_to_the_exclusion_list() {
        // `COVERED` matches by path and a skip matches by name at any depth, so
        // a name that is not a prefix of any covered path — `tests`, `fixtures`,
        // `examples`, `benches` — passes every guard `COVERED` can offer while
        // taking whole classes of file out of every walk.
        // Compared as slices so a *longer* list is a test failure with this
        // message rather than a type error about array lengths.
        assert_eq!(
            SKIP.as_slice(),
            NOTHING_ELSE_MAY_BE_SKIPPED.as_slice(),
            "SKIP grew a name. Every entry is build output or somebody else's \
             tree; a fourth kind is a gate being switched off for a directory, \
             which is what this list is closed against"
        );
    }

    /// Whether `.gitignore` hides a name, for the patterns this file uses.
    ///
    /// Enough of the format to answer the question honestly: comments and blank
    /// lines are skipped, a pattern with a `/` is anchored and cannot be judged
    /// from a bare name so it is skipped too, `*` matches any run of characters,
    /// a leading `!` un-ignores, and the **last** matching line wins.
    fn hidden_by_ignore(patterns: &str, name: &str) -> bool {
        let mut hidden = false;
        for line in patterns.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.contains('/') {
                continue;
            }
            let (negated, glob) = line
                .strip_prefix('!')
                .map_or((false, line), |rest| (true, rest));
            if glob_matches(glob, name) {
                hidden = !negated;
            }
        }
        hidden
    }

    /// `*` against a name, anchored at both ends. No other metacharacter is used
    /// by the file, so none is implemented — an unsupported one would be read as
    /// a literal and the test would fail rather than quietly pass.
    fn glob_matches(glob: &str, name: &str) -> bool {
        let mut ends = glob.split('*');
        let Some(first) = ends.next() else {
            return false;
        };
        if !name.starts_with(first) {
            return false;
        }
        let mut at = first.len();
        let parts: Vec<&str> = ends.collect();
        for (index, part) in parts.iter().enumerate() {
            if index + 1 == parts.len() {
                return name.len() >= at && name[at..].ends_with(part);
            }
            match name[at..].find(part) {
                Some(found) => at += found + part.len(),
                None => return false,
            }
        }
        name.len() == at
    }

    /// The gate and `.gitignore` decide the same names.
    ///
    /// They are two statements of one rule and they disagreed in both
    /// directions, each disagreement a real loss. `env.production` and
    /// `secrets.env.local` were neither ignored nor caught, so a real
    /// credentials file would have gone into a clone. `.env.local.example` and <!-- allow-reference: names `.env.local.example` — names the rule judges, not files here -->
    /// `example.env` are templates that were ignored, so a clone was missing the
    /// file it needs to start. `env.rs` was a Rust module git hid. <!-- allow-reference: names `env.rs` — a name the rule judges, not a file here -->
    ///
    /// The third column is `git check-ignore`'s own verdict, measured against
    /// this file in a throwaway repository — `hidden_by_ignore` is checked
    /// against git rather than trusted, and the gate is checked against both.
    #[test]
    fn the_ignore_file_hides_exactly_what_the_gate_reports() {
        let ignore = std::fs::read_to_string(root().join(".gitignore"))
            .expect("the repository has a .gitignore");
        // (name, what `git check-ignore` answered)
        let measured = [
            (".env", true),
            (".env.local", true),
            ("env.production", true),
            ("production.env", true),
            ("secrets.env.local", true),
            (".env.local.private", true),
            ("env", true),
            // **direnv's file**, which both halves missed together: it splits to
            // one part, so the gate's part rule never saw it, and this list of
            // nineteen names did not hold it either — so the test that exists to
            // keep the two honest agreed with both of them being wrong.
            (".envrc", true),
            (".envrc.local", true),
            (".env.example", false),
            (".env.local.example", false),
            ("env.dist", false),
            ("example.env", false),
            ("sample.env", false),
            ("secrets.example.env", false),
            ("template.env", false),
            ("env.sample", false),
            // **Case, which neither half looked at.** Every `.gitignore`
            // pattern was lower case and so was every name here, so the two
            // agreed about the wrong answer — measured, a `.ENV` at the root
            // holding a password and an AWS secret hidden by neither.
            (".ENV", false),
            (".Env.local", false),
            ("PRODUCTION.ENV", false),
            ("env.rs", false),
            // **A config format is an environment, and this said otherwise.**
            // The ignore file un-ignored all twelve kinds and the gate exempted
            // all twelve, so the two agreed — about the wrong answer. Measured,
            // one such file under `policy/` holding `DATABASE_URL` and
            // `AWS_SECRET_ACCESS_KEY`: `check passed`, and the whole run
            // differing from a clean one by the path count going up by one.
            ("env.toml", true),
            // A doubled or a trailing dot: git ignores these and the gate said
            // they were modules, which is 48 names' worth of disagreement.
            ("env..rs", true),
            ("env.rs.", true),
            ("environment.rs", false),
            ("envoy.toml", false),
        ];
        // **And the rules written below those negations still decide.** Git
        // takes the last matching pattern, so `!*.md` above would undo the
        // private-notes rule if that rule were ever moved up. Measured the same
        // way: `git check-ignore` hides this file.
        assert!(
            hidden_by_ignore(&ignore, "AGENTS.local.md"),
            "a broad negation reached the developer's private notes"
        );
        for (name, git_hides_it) in measured {
            assert_eq!(
                hidden_by_ignore(&ignore, name),
                git_hides_it,
                "`.gitignore` no longer hides `{name}` the way git measured it"
            );
            assert_eq!(
                super::holds_an_environment(name),
                git_hides_it,
                "the gate and `.gitignore` disagree about `{name}`"
            );
        }
    }

    /// What counts as an environment file, and what is a template.
    ///
    /// **The rule had no test at all**, so its whole shape could be reverted to
    /// the `.env` prefix it replaced with the suite green — and it was wrong in
    /// both directions anyway:
    ///
    /// ```text
    /// .env.local.example   -> error: it holds real credentials
    /// env.production       -> ok  no .env is committed
    /// secrets.env.local    -> ok  no .env is committed
    /// ```
    ///
    /// `env_file:` in a compose file names exactly the shapes it missed, and
    /// `.gitignore` has the same blind spot, so neither this nor the ignore file
    /// stopped one reaching a clone.
    #[test]
    fn an_environment_file_is_one_whose_name_says_so() {
        for holds in [
            ".env",
            ".env.local",
            ".env.production",
            "env",
            "env.production",
            "production.env",
            "secrets.env.local",
            ".env.local.private",
        ] {
            assert!(
                super::holds_an_environment(holds),
                "`{holds}` holds credentials and was not judged"
            );
        }
        for template in [
            ".env.example",
            ".env.sample",
            ".env.template",
            ".env.local.example",
            "example.env",
            "env.dist",
        ] {
            assert!(
                !super::holds_an_environment(template),
                "`{template}` is a template and was reported as a secret"
            );
        }
        // And names that merely contain the letters.
        for other in ["environment.rs", "env.rs", "prevent.md", "envoy.toml"] {
            assert!(
                !super::holds_an_environment(other),
                "`{other}` is not an environment file"
            );
        }
    }

    #[test]
    /// A module directory called `target` is source, and is walked.
    ///
    /// **The skip matched by name at any depth**, and this repository's own
    /// `nc-prune` has a `Target` trait, so the name is one a module would
    /// plausibly take. Measured with an ordinary logging call site in such a
    /// directory: the type-wall gate read one file fewer, `no_compile_time_query`
    /// never opened it either, and `check passed` — while the byte-identical
    /// file one directory up was a finding. `SKIP`'s own doc says every entry is
    /// "build output or somebody else's tree", and this was neither.
    fn a_module_named_target_is_source_and_a_build_directory_is_not() {
        let scratch = std::env::temp_dir().join(format!("nc-skip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        // A crate: its build directory is cargo's, and a `target` under its
        // sources is a module.
        std::fs::create_dir_all(scratch.join("crates/one/src/target")).expect("writable");
        std::fs::create_dir_all(scratch.join("crates/one/target/debug")).expect("writable");
        std::fs::create_dir_all(scratch.join("crates/one/src/target/inner")).expect("writable");
        std::fs::write(scratch.join("crates/one/Cargo.toml"), "[package]\n").expect("writable");
        std::fs::write(scratch.join("crates/one/src/target/mod.rs"), "//! m\n").expect("writable");
        std::fs::write(
            scratch.join("crates/one/src/target/inner/deep.rs"),
            "//! d\n",
        )
        .expect("writable");
        std::fs::write(scratch.join("crates/one/target/debug/build.rs"), "//! b\n")
            .expect("writable");
        std::fs::write(scratch.join("crates/one/target/debug/.env"), "SECRET=1\n")
            .expect("writable");
        // And the workspace's own build directory, beside the root manifest.
        std::fs::create_dir_all(scratch.join("target/debug")).expect("writable");
        std::fs::write(scratch.join("Cargo.toml"), "[workspace]\n").expect("writable");
        std::fs::write(scratch.join("target/debug/artifact.rs"), "//! a\n").expect("writable");
        // And the repository's own git directory, beside the same manifest.
        std::fs::create_dir_all(scratch.join(".git/objects/ab")).expect("writable");
        std::fs::write(scratch.join(".git/HEAD"), "ref: refs/heads/dev\n").expect("writable");
        std::fs::write(scratch.join(".git/objects/ab/cdef"), "x").expect("writable");

        let mut seen: Vec<String> = Vec::new();
        walk(&scratch, &mut |path| {
            seen.push(crate::relative_to(&scratch, path));
        })
        .expect("the scratch tree is readable");

        // **The repository's own `.git`, and only that one.** It is git's object
        // store: walked, its packed objects are files no gate can read, and the
        // marker census reports itself short over them.
        for metadata in [".git/HEAD", ".git/objects/ab/cdef"] {
            assert!(
                !seen.contains(&metadata.to_owned()),
                "the repository's own git directory was walked: {seen:?}"
            );
        }
        assert_eq!(
            crate::gates::census(&scratch, &scratch.join("target")).expect("readable"),
            seen.len(),
            "the census and the walk disagree about the repository's own `.git`"
        );
        for source in [
            "crates/one/src/target/mod.rs",
            "crates/one/src/target/inner/deep.rs",
        ] {
            assert!(
                seen.contains(&source.to_owned()),
                "a module directory named `target` was skipped: {seen:?}"
            );
        }
        // **The workspace's own build directory, and only that one.** The first
        // form of this rule asked whether a `Cargo.toml` sat beside the
        // directory, and one sits beside every member — so every crate got a
        // directory nothing looks in. Measured on the real tree, the same file
        // in two places:
        //
        // ```text
        // migrations/.env.production   -> error: `.env.production` is committed here
        // crates/nc-log/target/.env    -> ok  no .env is committed, in 151 path(s)
        // ```
        //
        // Cargo writes one build directory per workspace, beside the manifest
        // that declares `[workspace]`; a member inside a workspace gets none.
        assert!(
            !seen.contains(&"target/debug/artifact.rs".to_owned()),
            "the workspace's build directory was walked: {seen:?}"
        );
        for member_output in [
            "crates/one/target/debug/build.rs",
            "crates/one/target/debug/.env",
        ] {
            assert!(
                seen.contains(&member_output.to_owned()),
                "a member directory named `target` was skipped, so anything in it is \
                 invisible to every walk-based gate: {seen:?}"
            );
        }

        // **And the other two names on the list.** A committed `.env` under
        // either, anywhere but where each belongs, is invisible with the path
        // count unmoved. A `.git` inside a crate's sources is not this
        // repository's history, which sits beside the workspace manifest.
        for hidden in ["crates/one/src/node_modules", "crates/one/src/.git"] {
            std::fs::create_dir_all(scratch.join(hidden)).expect("writable");
            std::fs::write(scratch.join(hidden).join(".env"), "SECRET=1\n").expect("writable");
        }
        // A node package tree, which is somebody else's and stays skipped.
        std::fs::create_dir_all(scratch.join("web/node_modules/left-pad")).expect("writable");
        std::fs::write(scratch.join("web/package.json"), "{}\n").expect("writable");
        std::fs::write(scratch.join("web/node_modules/left-pad/index.js"), "//\n")
            .expect("writable");

        let mut seen: Vec<String> = Vec::new();
        walk(&scratch, &mut |path| {
            seen.push(crate::relative_to(&scratch, path));
        })
        .expect("the scratch tree is readable");
        for hidden in [
            "crates/one/src/node_modules/.env",
            "crates/one/src/.git/.env",
        ] {
            assert!(
                seen.contains(&hidden.to_owned()),
                "a directory with a familiar name hid a committed secret: {seen:?}"
            );
        }
        // **A node manifest does not buy a hiding place**, which is what made
        // this a two-line switch: adding one beside a `node_modules` took every
        // walk-based gate off the files inside it.
        assert!(
            seen.contains(&"web/node_modules/left-pad/index.js".to_owned()),
            "a node manifest bought a directory nothing looks in: {seen:?}"
        );
        let _ = std::fs::remove_dir_all(&scratch);
    }

    /// A linked worktree's `.git` file at the repository's root is its metadata,
    /// and a file of that name anywhere else is a file like any other.
    ///
    /// `git worktree add` leaves a one-line `.git` *file* where the directory
    /// would be. Walked, it is a file with no extension, which the kind gates
    /// refuse — so a correct checkout would fail the gate for being one.
    #[test]
    fn a_worktrees_git_file_at_the_root_is_metadata_and_elsewhere_is_a_file() {
        let held = crate::check::LeavesNothingBehind::new("worktree-git-file");
        let scratch = held.root.clone();
        std::fs::create_dir_all(scratch.join("crates/one/src")).expect("writable");
        std::fs::write(scratch.join("Cargo.toml"), "[workspace]\n").expect("writable");
        std::fs::write(scratch.join(".git"), "gitdir: /elsewhere/worktrees/one\n")
            .expect("writable");
        std::fs::write(scratch.join("crates/one/src/.git"), "gitdir: /elsewhere\n")
            .expect("writable");

        let mut seen: Vec<String> = Vec::new();
        walk(&scratch, &mut |path| {
            seen.push(crate::relative_to(&scratch, path));
        })
        .expect("the scratch tree is readable");

        assert!(
            !seen.contains(&".git".to_owned()),
            "the worktree's own `.git` file was walked as one of its files: {seen:?}"
        );
        assert!(
            seen.contains(&"crates/one/src/.git".to_owned()),
            "a `.git` file inside a crate's sources was skipped: {seen:?}"
        );
        // And the second count agrees with the walk over the same tree: it
        // judges the root's `.git` by the same property, written separately.
        assert_eq!(
            crate::gates::census(&scratch, &scratch.join("target")).expect("readable"),
            seen.len(),
            "the census and the walk disagree about a worktree's `.git` file"
        );
    }

    #[test]
    fn the_exclusion_list_does_not_cover_what_the_gates_are_for() {
        // **And the list itself, because both tests here iterate it.** An entry
        // deleted deletes its own case: measured in round sixty-five, dropping
        // `"policy"` — seven Cedar invariants and the schema — left
        // `check passed`, so the claim that the gates must reach that directory
        // could be withdrawn with nothing said. `SKIP` is pinned exactly this
        // way against `NOTHING_ELSE_MAY_BE_SKIPPED`; the list it is checked
        // against was not.
        assert_eq!(
            COVERED,
            [
                "crates/nc-authz/src",
                "crates/nc-canon/src",
                "crates/nc-catalog/src",
                "crates/nc-catalog-compiler/src",
                "crates/nc-guest/src",
                "crates/nc-ids/src",
                "crates/nc-journal/src",
                "crates/nc-kernel/src",
                "crates/nc-local/src",
                "crates/nc-log/src",
                "crates/nc-policy/src",
                "crates/nc-prune/src",
                "crates/nc-schema/src",
                "xtask/src",
                "catalog",
                "policy",
                "plugins",
                "migrations",
                "local-migrations",
                "docs",
            ],
            "the directories the gates are for moved, and both tests below read this list"
        );
        // At the string level: adding "crates" to SKIP would turn this
        // repository's headline invariant off in silence.
        for covered in COVERED {
            for skip in SKIP {
                assert!(
                    covered != skip && !covered.starts_with(&format!("{skip}/")),
                    "SKIP entry `{skip}` swallows `{covered}`, which the gates exist for"
                );
            }
        }
    }

    #[test]
    fn the_walk_actually_reaches_the_directories_the_gates_are_for() {
        // And at the walk level, because the string test above would still pass
        // if `walk` stopped descending for some other reason.
        let mut seen: Vec<String> = Vec::new();
        walk(root(), &mut |path| {
            seen.push(crate::relative_to(root(), path));
        })
        .expect("the tree is readable");
        assert!(seen.len() > 10, "only {} files were walked", seen.len());
        for covered in COVERED {
            assert!(
                seen.iter()
                    .any(|path| path.starts_with(&format!("{covered}/"))),
                "nothing under `{covered}/` was walked, so the gates cover none of it"
            );
        }
    }

    /// A char literal is spent, and a lifetime is not.
    ///
    /// **The clause this holds had no test at all.** Round 54 taught
    /// `without_string_contents` to blank the *contents* of a char literal,
    /// because `'{'` and `'}'` were braces to every brace-counting scan
    /// downstream — and reverting that one clause left `cargo test -p xtask`
    /// at `327 passed; 0 failed`. Nothing in the crate names this function;
    /// its four call sites are floored only by counts that the tree happens to
    /// leave balanced, because a body holding `'{' => depth += 1` and
    /// `'}' => …` cancels out.
    ///
    /// What the clause buys, measured in `ui.rs`'s counter-reader rule: a test
    /// reading a process-wide counter without the hold, with
    /// `let at = "x}y".find('}');` above the read, is **invisible** with the
    /// clause reverted and **named** with it in place. A repair is a claim, and
    /// this is the claim's case.
    #[test]
    fn a_char_literal_is_spent_and_a_lifetime_is_kept() {
        // The two that were braces to every scan downstream.
        assert_eq!(
            super::without_string_contents("let a = '}';"),
            "let a = ' ';"
        );
        assert_eq!(
            super::without_string_contents("let a = '{';"),
            "let a = ' ';"
        );
        // A quote inside one must not open a string, which is the older reason
        // this branch exists at all.
        assert_eq!(
            super::without_string_contents("let a = '\"'; let b = \"x\";"),
            "let a = ' '; let b = \" \";"
        );
        // An escape is one literal, however many characters it is written with.
        assert_eq!(
            super::without_string_contents("let a = '\\n';"),
            "let a = '  ';"
        );
        // A lifetime is not a literal: `'a` has no closing quote, and spending
        // it would eat the code after it.
        assert_eq!(
            super::without_string_contents("fn f<'a>(x: &'a str) {}"),
            "fn f<'a>(x: &'a str) {}"
        );
        // Length is preserved in *characters*, which every caller's offsets
        // depend on — the coordinate rule `AGENTS.md` states.
        for source in [
            "let a = '}';",
            "let a = '\"'; let b = \"x\";",
            "fn f<'a>(x: &'a str) {}",
        ] {
            assert_eq!(
                super::without_string_contents(source).chars().count(),
                source.chars().count(),
                "`{source}` changed length, so every offset taken from it moves"
            );
        }
    }

    /// Everything a scanner must not read as code is spent, and nothing moves.
    ///
    /// **The property the whole of `blanked` exists for is the length**, and it
    /// is the one its neighbour does not have. `without_string_contents` writes
    /// the closing `"#` of a raw string back as a single `"`, so every offset
    /// after the first raw string in a file is short by the number of hashes —
    /// which is the third of the four failed attempts recorded on
    /// [`super::the_test_modules`]. That difference is measured here rather
    /// than asserted in prose, because the day it stops being true is the day
    /// this function could be deleted in favour of the other one.
    ///
    /// The rest is what a brace-counting caller trips over: a `{` in a comment,
    /// a `{` in a string, a `{` in a raw string, and `'{'` — a char literal,
    /// which is the fourth failed attempt.
    #[test]
    fn a_blanked_file_holds_no_comment_no_literal_and_the_same_offsets() {
        let same_length = |source: &str| {
            assert_eq!(
                super::blanked(source).chars().count(),
                source.chars().count(),
                "`{source}` changed length, so every offset taken from it moves"
            );
        };
        // A line comment, and the newline that ends it.
        assert_eq!(
            super::blanked("let a = 1; // {\nlet b = 2;"),
            "let a = 1;     \nlet b = 2;"
        );
        // A block comment, which nests.
        assert_eq!(
            super::blanked("a /* { /* } */ { */ b"),
            "a                   b"
        );
        // A string, a raw string with hashes, and a char literal.
        assert_eq!(super::blanked("let a = \"{\";"), "let a = \" \";");
        // **Both sides of a raw string's hashes, not one.** This pinned
        // `r#" " ;` — the opening hashes kept and the closing ones spent —
        // which is a raw string that opens and never closes, so any reader
        // scanning the result runs one delimiter out of phase for the rest of
        // the file. The expectation was the defect: measured by a reviewer on
        // 2026-09-12, one `let _ = r#"x"#;` in `check.rs` turned the floor
        // census red with a list of garbage naming no floor.
        assert_eq!(super::blanked("let a = r#\"{\"#;"), "let a = r \" \" ;");
        assert_eq!(super::blanked("let a = '{';"), "let a = ' ';");
        // **A string may span lines.** A raw newline inside quotes is part of
        // the literal, and the braces around it are not braces — the case that
        // `without_string_contents`' own comment gets wrong, and the one that
        // made this walk find an unbalanced file the first time it ran.
        assert_eq!(super::blanked("let a = \"{\n}\";"), "let a = \" \n \";");
        // A lifetime is not a literal, and spending it would eat the code after.
        assert_eq!(
            super::blanked("fn f<'a>(x: &'a str) {}"),
            "fn f<'a>(x: &'a str) {}"
        );
        for source in [
            "let a = 1; // {\nlet b = 2;",
            "a /* { /* } */ { */ b",
            "let a = \"{\";",
            "let a = r#\"{\"#;",
            "let a = r###\"}\"###;",
            "let a = '{';",
            "let a = '\\n';",
            "let a = \"{\n}\";",
            "let a = \"x\\\nstill the same string\";",
            "fn f<'a>(x: &'a str) {}",
        ] {
            same_length(source);
        }
        // **And the neighbour it exists beside does not do this**, which is why
        // there are two of them. If this ever stops failing, one of them is
        // redundant and the doc above is wrong.
        let raw = "let a = r#\"x\"#;";
        assert_ne!(
            super::without_string_contents(raw).chars().count(),
            raw.chars().count(),
            "`without_string_contents` preserves length on a raw string now, so \
             `blanked` is a second copy of it and one of the two should go"
        );

        // Every `.rs` file in the tree, which is the corpus the release gate's
        // boundary is taken from: the same number of characters, and braces
        // that balance — a scanner that reads a comment or a literal as code
        // finds one that does not.
        let root = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/.."));
        // **Two counters, and the one that matters is incremented last.**
        // `read` stood at the top of this callback and the floor beneath the
        // walk was `read > 50`, so it counted files *opened* rather than files
        // *judged* — measured, `if read > 0 { return; }` on the line under it:
        // 88 files opened, none of them checked for length preservation or for
        // brace balance, and this test `ok`. It is the shape
        // `everything_reached_a_verdict` exists for, and the one repaired
        // twenty lines away in `check.rs`: a filter between the counter at the
        // top of a loop and the judgement at the bottom leaves every printed
        // number whole while the rule is never asked.
        //
        // `opened` is the walk's own count of `.rs` files and `judged` is what
        // came out the far end of the assertions, so anything that leaves early
        // makes the two disagree. The floor stays, because `0 == 0` is an
        // identity too.
        let mut opened = 0_usize;
        let mut judged = 0_usize;
        let walked = super::walk(root, &mut |path| {
            if path.extension().is_none_or(|kind| kind != "rs") {
                return;
            }
            opened += 1;
            let Ok(text) = std::fs::read_to_string(path) else {
                return;
            };
            let neutral = super::blanked(&text);
            assert_eq!(
                neutral.chars().count(),
                text.chars().count(),
                "{} changed length, so every offset taken from it moves",
                path.display()
            );
            let mut depth = 0_isize;
            for character in neutral.chars() {
                match character {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                assert!(
                    depth >= 0,
                    "{} closes a brace it never opened once blanked, so something \
                     inside a comment or a literal was read as code",
                    path.display()
                );
            }
            assert_eq!(
                depth,
                0,
                "{} does not balance its braces once blanked",
                path.display()
            );
            judged += 1;
        });
        walked.expect("the tree is walkable");
        assert_eq!(
            judged, opened,
            "the walk opened {opened} source file(s) and {judged} reached both \
             assertions: something between the two is filtering, and a file that was \
             never blanked reads exactly like a file that blanked cleanly"
        );
        assert!(
            judged > 50,
            "the walk judged {judged} source file(s), so this measured almost nothing"
        );
    }

    /// The modules of tests are cut out, and only those.
    ///
    /// **The boundary `xtask/src/release.rs` needs, and the four ways of
    /// finding it that did not bind** are on [`super::the_test_modules`]. What
    /// is watched here is the three ways it can be wrong: a `#[cfg(test)]` that
    /// is not a module, a second module of tests, and the words `#[cfg(test)]`
    /// written where they are not code — which is what a scanner reading its
    /// own source meets, and the reason the anchor is looked for in the blanked
    /// text.
    #[test]
    fn every_module_of_tests_is_cut_out_and_nothing_else_is() {
        let cut = |source: &str| -> Vec<String> {
            super::the_test_modules(source)
                .expect("the braces close")
                .into_iter()
                .map(|region| source[region].to_owned())
                .collect()
        };
        // A `#[cfg(test)]` const is not a module: matching braces from one
        // finds the *next* item's brace and swallows everything between.
        let two = "#[cfg(test)]\nconst A: [&str; 1] = [\"x\"];\n\
                   #[cfg(test)]\nmod tests {\n    fn a() {}\n}\n\
                   pub fn shipped() {}\n\
                   #[cfg(test)]\nmod more {\n    fn b() {}\n}\n";
        let found = cut(two);
        assert_eq!(
            found.len(),
            2,
            "a `#[cfg(test)]` item that is not a module was measured, or a module \
             was missed: {found:#?}"
        );
        assert!(found[0].contains(&["mod ", "tests {"].join("")) && found[1].contains("mod more"));
        assert!(
            !found.iter().any(|region| region.contains("shipped")),
            "the code between two modules of tests was cut out with them: {found:#?}"
        );
        // Indented is not column zero, and a module of tests is a top-level item.
        let left_over =
            cut("fn a() {\n    #[cfg(test)]\n    mod inner {\n        fn b() {}\n    }\n}\n");
        assert!(
            left_over.is_empty(),
            "an indented `#[cfg(test)]` was taken for a file's module of tests: {left_over:?}"
        );
        // The words, written where they are not code. This is the case the
        // anchor is looked for in the blanked text for: a scanner that reads
        // its own source finds them in itself.
        let quoted = format!(
            "const A: &str = \"{}\\nmod tests {{\";\n// {}\n// mod tests {{\n\
             #[cfg(test)]\nmod tests {{\n    fn a() {{}}\n}}\n",
            ["#[cfg(", "test)]"].join(""),
            ["#[cfg(", "test)]"].join("")
        );
        let found = cut(&quoted);
        assert_eq!(
            found.len(),
            1,
            "the words `#[cfg(test)]` inside a string or a comment were taken for \
             an item: {found:#?}"
        );
        // Braces that never close are not answered for.
        assert!(
            super::the_test_modules("#[cfg(test)]\nmod tests {\n    fn a() {}\n").is_none(),
            "a module whose braces never close was given an end anyway"
        );
    }

    /// A compose file beside the stack's own is a finding, and only those are.
    ///
    /// **The rule was a repair with no test.** `grep` found `a_second_compose_file`
    /// at its definition and its one call site and nowhere else; `corpus.rs`'s
    /// compose driver exercises `every_image_is_pinned`, not this, and the only
    /// floor was `looked >= 7`. Measured: `if true { continue; }` before the push,
    /// with a real override file in the tree, gave
    /// `ok the stack is one compose file` and `check passed`.
    ///
    /// Every name after the first, because `docker compose` merges any of them
    /// silently — and `docker-compose.yml` alone, because a rule that fires on
    /// the correct tree is not a rule.
    #[test]
    fn a_compose_file_beside_the_first_is_a_finding() {
        let scratch =
            std::env::temp_dir().join(format!("nc-compose-beside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).expect("writable");
        std::fs::write(scratch.join(super::COMPOSE_FILES[0]), "services:\n").expect("writable");

        let (none, looked) = super::a_second_compose_file(&scratch);
        assert!(
            none.is_empty(),
            "the stack's own file is the stack, and this refused it: {none:#?}"
        );
        assert!(
            looked >= 7,
            "{looked} name(s) were looked for, and docker merges eight"
        );

        for named in super::COMPOSE_FILES.iter().skip(1) {
            std::fs::write(scratch.join(named), "services:\n").expect("writable");
            let (found, _) = super::a_second_compose_file(&scratch);
            assert!(
                found.iter().any(|finding| finding.file == *named),
                "`{named}` is beside the stack's own file and nothing said so: {found:#?}"
            );
            std::fs::remove_file(scratch.join(named)).expect("this test wrote it");
        }
        let _ = std::fs::remove_dir_all(&scratch);
    }

    #[test]
    fn build_output_is_not_walked() {
        // The other half: a gate that walked build output would report on files
        // this repository does not own, at a volume nobody reads.
        let mut seen: Vec<String> = Vec::new();
        walk(root(), &mut |path| {
            seen.push(crate::relative_to(root(), path));
        })
        .expect("the tree is readable");
        assert!(
            !seen.iter().any(|path| path.starts_with("target/")),
            "build output was walked"
        );
    }

    #[test]
    fn the_manifest_walk_finds_every_member_of_the_workspace() {
        // Named rather than counted. `>= 2` passed while two of the four
        // manifests were invisible, which is the shape of assertion §19.5 calls
        // vacuous: true, and true of a broken walk too.
        let found = manifests(root()).expect("the tree is readable");
        let paths: Vec<String> = found
            .iter()
            .map(|path| crate::relative_to(root(), path))
            .collect();
        for member in ["Cargo.toml", "xtask/Cargo.toml"] {
            assert!(
                paths.iter().any(|path| path == member),
                "`{member}` was not walked: {paths:?}"
            );
        }
        assert!(
            paths.iter().any(|path| path.starts_with("crates/")),
            "no crate manifest was walked: {paths:?}"
        );
    }

    /// Every lint group `rustc` names is on the list this gate refuses.
    ///
    /// **The list named two of the thirteen.** `warnings` and `unused` were on
    /// it and the other eleven were not, so `#![allow(nonstandard_style)]` at a
    /// crate root — or `future_incompatible`, or `rust_2024_compatibility` —
    /// turned a whole group off for everything below it and the verdict printed
    /// `ok lints` over it. The gate's own header says what it is for: an allow
    /// is a judgement about one lint, and a group is all of them at once.
    ///
    /// **Asked of the compiler rather than of a reader**, because a
    /// hand-written copy of somebody else's list is right on the day it is
    /// written and on no other. `rust-toolchain.toml` pins the toolchain, so
    /// this is a fixed table for a given tree, and the day the pin moves and
    /// brings a group with it this fails instead of going quiet.
    ///
    /// The other direction is deliberately not asserted. `missing_docs`,
    /// `unreachable_pub` and `missing_debug_implementations` are single lints
    /// rather than groups, and they are on the list because `[workspace.lints]`
    /// sets all three to `warn` — a crate root putting one of them down is the
    /// same defeat of the same default.
    #[test]
    fn every_lint_group_rustc_names_is_on_this_list() {
        // The table, by its own shape rather than by a line count: a heading, a
        // column header, a rule of dashes, then one row per group until the
        // blank line. Reading it by offset is a scan that breaks silently the
        // first time a blank line moves.
        const HEADING: &str = "Lint groups provided by rustc:";
        let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned());
        #[expect(
            clippy::disallowed_methods,
            reason = "the compiler is asked what its lint groups are; its output is read, not printed"
        )]
        let help = std::process::Command::new(&rustc)
            .args(["-W", "help"])
            .output()
            .unwrap_or_else(|error| panic!("`{rustc} -W help`, which compiled this test: {error}"));
        let printed = String::from_utf8_lossy(&help.stdout).into_owned();
        let at = printed.find(HEADING).unwrap_or_else(|| {
            panic!("`{rustc} -W help` does not print `{HEADING}`, so this read nothing")
        });
        let mut named: Vec<String> = Vec::new();
        let mut past_the_rule = false;
        for line in printed[at + HEADING.len()..].lines() {
            let Some(word) = line.split_whitespace().next() else {
                if past_the_rule {
                    break;
                }
                continue;
            };
            if word.starts_with("----") {
                past_the_rule = true;
                continue;
            }
            if past_the_rule {
                // A lint name is written with hyphens here and with underscores
                // in an attribute, and it is the attribute this gate reads.
                named.push(word.replace('-', "_"));
            }
        }
        for anchor in ["warnings", "unused"] {
            assert!(
                named.iter().any(|group| group == anchor),
                "`{anchor}` is a lint group of every rustc there has been and this scan of \
                 `{rustc} -W help` did not find it, so it read the wrong part of the output \
                 and the loop below decides nothing: {named:?}"
            );
        }
        assert!(
            named.len() >= 13,
            "`{rustc} -W help` names {} lint group(s), and it named thirteen when this was \
             written: the scan is short, so a group could be missing from the gate and this \
             test would not say so",
            named.len()
        );
        for group in &named {
            assert!(
                super::LINT_GROUPS.contains(&group.as_str()),
                "`{rustc}` names `{group}` a lint group and this gate does not, so a crate \
                 root can put it down for its whole tree — every lint in it, at once — and \
                 the run says `ok lints` over the top"
            );
        }
    }
}

/// How many lines the longest file any gate reads has.
///
/// **The number a planted fixture has to sit past.** A fixture placed by
/// `pid % k + c` lands in a window of `k` lines, and `k` is written in the test —
/// so a filter carrying a constant *above* that window is invisible to it while
/// turning the gate off for every real file. Measured: the four fixtures added
/// for exactly this purpose sat in windows of 97, 40, 60 and 30 lines, and
/// `if index > 300 { … }` planted in the blanket-allow walk passed all of them
/// with a real blanket allow underneath and every count *up*.
///
/// A fixture longer than the longest real file has no such window: any filter
/// that would let a real file's tail through lets the fixture's defect through
/// too, and the test fails. That is the relationship, and it is asserted rather
/// than assumed — the longest file grows, and a fixture that stopped being
/// longer would go quiet without saying so.
#[cfg(test)]
pub(crate) fn longest_file_read(root: &Path) -> usize {
    // **The workspace as well as the repository, and every kind a gate reads.**
    // Two of the gates this serves — the citation walk and the counts walk — read
    // the workspace documents *above* the repository, and `<workspace>/NOCTUA.md`
    // is 308 lines longer than the longest file here. So the fixtures planted
    // inside that gap and a filter written in it was invisible again: measured,
    // `if index > 5200 { reached += 1; continue; }` with a dangling citation at
    // line 5352 of the Master Technical Reference gave `ok 1520 section
    // citation(s) name a real section` — up from baseline — and `check passed`.
    //
    // The extension list is the union of what the gates read, not a private
    // three: nothing is live today at the other kinds, and a measurement that
    // does not cover the corpus it stands for says nothing when that changes.
    let mut longest = 0_usize;
    let mut measure = |at: &Path| {
        let _ = walk(at, &mut |path| {
            if path.extension().is_some_and(|kind| {
                matches!(
                    kind.to_string_lossy().as_ref(),
                    "rs" | "sql" | "md" | "toml" | "yml" | "yaml" | "cedar" | "cedarschema"
                )
            }) && let Ok(text) = std::fs::read_to_string(path)
            {
                longest = longest.max(text.lines().count());
            }
        });
    };
    measure(root);
    if let Some(workspace) = crate::sections::workspace_of(root) {
        for name in crate::counts::ABOVE {
            if let Ok(text) = std::fs::read_to_string(workspace.join(name)) {
                longest = longest.max(text.lines().count());
            }
        }
    }
    longest
}
