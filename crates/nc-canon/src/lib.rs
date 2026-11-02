//! Canonical JSON: ordered keys, stable formatting, one implementation.
//!
//! `NOCTUA.md §16.4` makes this a decision rather than a detail, for two
//! reasons that pull in the same direction:
//!
//! * **A hash over non-canonical JSON is a hash over formatting.** The journal's
//!   chain (`§14.9`) is only verifiable if the bytes it hashed can be produced
//!   again, byte for byte, by something that was not there at the time.
//! * **Agent-friendliness is a serialization decision.** Deterministic output
//!   means an edit produces a minimal, readable diff instead of a reordered file.
//!
//! It is the WIT `payload` type (`reference/noctua-kernel.wit`, `types.payload`)
//! and it governs the IPC channel, command arguments, event payloads and the
//! journal.
//!
//! # What this deliberately cannot represent
//!
//! **There is no floating-point value.** Not an oversight: a canonical form for
//! a binary float requires pinning a decimal shortest-representation algorithm,
//! and two implementations that disagree by one digit produce two hashes for one
//! entry — which, in a chain, is indistinguishable from tampering. A later stage
//! that needs a real number carries it as a scaled integer or as text, and says
//! which in the field's own name. That is a smaller cost than a chain that
//! sometimes fails to verify.
//!
//! # Where this follows RFC 8785 and where it does not
//!
//! Key ordering and string escaping are RFC 8785's, because they are the two
//! places every hand-rolled canonicaliser gets wrong and because interoperating
//! with one that exists is worth more than inventing one. Numbers are not: RFC
//! 8785 defines them through ECMAScript's `Number`, and the whole point of the
//! paragraph above is not to depend on that.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
pub mod parse;

pub use parse::{ParseError, parse};

use core::cmp::Ordering;
use core::fmt::Write as _;
use std::collections::BTreeMap;

/// One canonical value.
pub enum Value {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A whole number. See the module docs for why there is no float.
    Integer(i64),
    /// Text.
    Text(String),
    /// An ordered list.
    Array(Vec<Value>),
    /// A map. Written with its keys sorted, whatever order they were inserted.
    Object(BTreeMap<String, Value>),
}

/// **Every one of these is iterative**, and the derives they replace were not.
///
/// `Value::write` and [`Value::drop`] were made iterative because a deeply
/// nested value aborted the process — `fatal runtime error: stack overflow` in
/// the function every journal entry and every log line goes through. The three
/// derived traits recurse for exactly the same reason and were left alone, so
/// the hazard survived the fix in a quieter place: measured, `Value::clone`,
/// `==` and `{:?}` all abort at about two thousand levels, and
/// `Entry::chain` reaches `clone` through `hashed_form` with no bound anywhere
/// between a caller and the crash.
///
/// The variants are public, so any caller can build one. That is the whole
/// argument, and it does not stop at the two functions that were noticed first.
impl Clone for Value {
    fn clone(&self) -> Self {
        /// What the walk still has to do.
        enum Step<'a> {
            Visit(&'a Value),
            Array(usize),
            Object(Vec<String>),
        }

        let mut work = vec![Step::Visit(self)];
        // Finished children, in the order their parent will take them.
        let mut done: Vec<Self> = Vec::new();
        while let Some(step) = work.pop() {
            match step {
                Step::Visit(Self::Null) => done.push(Self::Null),
                Step::Visit(Self::Bool(flag)) => done.push(Self::Bool(*flag)),
                Step::Visit(Self::Integer(number)) => done.push(Self::Integer(*number)),
                Step::Visit(Self::Text(text)) => done.push(Self::Text(text.clone())),
                Step::Visit(Self::Array(items)) => {
                    work.push(Step::Array(items.len()));
                    // Pushed in reverse so they pop in order.
                    for item in items.iter().rev() {
                        work.push(Step::Visit(item));
                    }
                }
                Step::Visit(Self::Object(fields)) => {
                    work.push(Step::Object(fields.keys().cloned().collect()));
                    for field in fields.values().rev() {
                        work.push(Step::Visit(field));
                    }
                }
                Step::Array(count) => {
                    let items = done.split_off(done.len() - count);
                    done.push(Self::Array(items));
                }
                Step::Object(keys) => {
                    let values = done.split_off(done.len() - keys.len());
                    done.push(Self::Object(keys.into_iter().zip(values).collect()));
                }
            }
        }
        done.pop().unwrap_or(Self::Null)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        let mut pairs = vec![(self, other)];
        while let Some((left, right)) = pairs.pop() {
            match (left, right) {
                (Self::Null, Self::Null) => {}
                (Self::Bool(left), Self::Bool(right)) if left == right => {}
                (Self::Integer(left), Self::Integer(right)) if left == right => {}
                (Self::Text(left), Self::Text(right)) if left == right => {}
                (Self::Array(left), Self::Array(right)) if left.len() == right.len() => {
                    pairs.extend(left.iter().zip(right.iter()));
                }
                (Self::Object(left), Self::Object(right)) if left.len() == right.len() => {
                    for (key, value) in left {
                        match right.get(key) {
                            Some(other) => pairs.push((value, other)),
                            None => return false,
                        }
                    }
                }
                _ => return false,
            }
        }
        true
    }
}

impl Eq for Value {}

impl core::fmt::Debug for Value {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        /// What the walk still has to write.
        enum Step<'a> {
            Visit(&'a Value),
            Literal(&'static str),
            Key(&'a str),
        }

        let mut work = vec![Step::Visit(self)];
        while let Some(step) = work.pop() {
            match step {
                Step::Literal(text) => formatter.write_str(text)?,
                Step::Key(key) => write!(formatter, "{key:?}: ")?,
                Step::Visit(Self::Null) => formatter.write_str("Null")?,
                Step::Visit(Self::Bool(flag)) => write!(formatter, "Bool({flag})")?,
                Step::Visit(Self::Integer(number)) => write!(formatter, "Integer({number})")?,
                Step::Visit(Self::Text(text)) => write!(formatter, "Text({text:?})")?,
                Step::Visit(Self::Array(items)) => {
                    formatter.write_str("Array([")?;
                    work.push(Step::Literal("])"));
                    for (index, item) in items.iter().enumerate().rev() {
                        work.push(Step::Visit(item));
                        if index > 0 {
                            work.push(Step::Literal(", "));
                        }
                    }
                }
                Step::Visit(Self::Object(fields)) => {
                    // **In the order `to_canonical` writes them**, which is by
                    // UTF-16 code unit and not `BTreeMap`'s UTF-8 order. The two
                    // disagree for any key above the basic multilingual plane —
                    // measured, `Debug` and `to_canonical` printing the same
                    // object's two keys in *opposite* orders — and the one place
                    // a person reads `Debug` for an object like this is while
                    // chasing a hash mismatch, where the order is the thing they
                    // are looking at.
                    formatter.write_str("Object({")?;
                    work.push(Step::Literal("})"));
                    let mut ordered: Vec<(&String, &Self)> = fields.iter().collect();
                    ordered.sort_by(|(left, _), (right, _)| utf16_cmp(left, right));
                    for (index, (key, value)) in ordered.into_iter().enumerate().rev() {
                        work.push(Step::Visit(value));
                        work.push(Step::Key(key));
                        if index > 0 {
                            work.push(Step::Literal(", "));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl Value {
    /// An object, from pairs.
    #[must_use]
    pub fn object<K: Into<String>>(pairs: impl IntoIterator<Item = (K, Self)>) -> Self {
        Self::Object(
            pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    /// Text, from anything that can become a `String`.
    #[must_use]
    pub fn text(value: impl Into<String>) -> Self {
        Self::Text(value.into())
    }

    /// The text this value holds, taken out of it.
    ///
    /// **`Value` implements [`Drop`], so a consumer cannot move a field out of
    /// one.** `match value { Value::Text(held) => held, … }` is
    /// `error[E0509]: cannot move out of type Value, which implements the Drop
    /// trait` — and the `Drop` is there on purpose, to dismantle a deep value
    /// without recursing. Every caller was therefore obliged to `clone()`, with
    /// nothing saying so. These three take the contents and leave the value
    /// behind.
    #[must_use]
    pub fn into_text(mut self) -> Option<String> {
        match &mut self {
            Self::Text(held) => Some(core::mem::take(held)),
            _ => None,
        }
    }

    /// The elements this value holds, taken out of it.
    #[must_use]
    pub fn into_array(mut self) -> Option<Vec<Self>> {
        match &mut self {
            Self::Array(held) => Some(core::mem::take(held)),
            _ => None,
        }
    }

    /// The fields this value holds, taken out of it.
    #[must_use]
    pub fn into_object(mut self) -> Option<BTreeMap<String, Self>> {
        match &mut self {
            Self::Object(held) => Some(core::mem::take(held)),
            _ => None,
        }
    }

    /// Renders the value in canonical form: sorted keys, no whitespace.
    #[must_use]
    pub fn to_canonical(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }

    /// How deeply the value nests. A scalar is 1.
    ///
    /// Iterative, like `Self::write` and for the same reason: this exists to
    /// answer a question about deep values, and it must not be the thing that
    /// falls over on one.
    #[must_use]
    pub fn depth(&self) -> usize {
        let mut deepest = 0_usize;
        let mut stack = vec![(self, 1_usize)];
        while let Some((value, depth)) = stack.pop() {
            deepest = deepest.max(depth);
            match value {
                Self::Array(items) => stack.extend(items.iter().map(|item| (item, depth + 1))),
                Self::Object(fields) => {
                    stack.extend(fields.values().map(|field| (field, depth + 1)));
                }
                _ => {}
            }
        }
        deepest
    }

    /// Writes the value, without recursion.
    ///
    /// **An explicit stack, and that is not a style choice.** [`parse()`] bounds
    /// its depth at [`parse::MAX_DEPTH`] because a recursive-descent parser on
    /// adversarial input is a stack overflow — and this, on the other side of
    /// the same crate, had no bound at all. A `Value` nested a few hundred deep
    /// (which any caller may build; the variants are public) aborted the process
    /// with `fatal runtime error: stack overflow`, in the function every journal
    /// entry and every log line goes through.
    fn write(&self, out: &mut String) {
        // Popped in order, so each is pushed after what must follow it.
        enum Step<'a> {
            Value(&'a Value),
            Key(&'a str),
            Literal(&'static str),
        }

        let mut stack = vec![Step::Value(self)];
        while let Some(step) = stack.pop() {
            match step {
                Step::Literal(text) => out.push_str(text),
                Step::Key(key) => {
                    write_string(key, out);
                    out.push(':');
                }
                Step::Value(value) => match value {
                    Self::Null => out.push_str("null"),
                    Self::Bool(true) => out.push_str("true"),
                    Self::Bool(false) => out.push_str("false"),
                    Self::Integer(number) => {
                        let _ = write!(out, "{number}");
                    }
                    Self::Text(text) => write_string(text, out),
                    Self::Array(items) => {
                        out.push('[');
                        stack.push(Step::Literal("]"));
                        for (index, item) in items.iter().enumerate().rev() {
                            stack.push(Step::Value(item));
                            if index > 0 {
                                stack.push(Step::Literal(","));
                            }
                        }
                    }
                    Self::Object(fields) => {
                        // Sorted by UTF-16 code unit, which is RFC 8785's rule
                        // and is NOT what `BTreeMap` gives: Rust orders strings
                        // by UTF-8 bytes, and the two disagree for any key
                        // holding a character above the basic multilingual
                        // plane. It has never come up, and it would produce two
                        // hashes for one entry on the day it did.
                        let mut keys: Vec<&String> = fields.keys().collect();
                        keys.sort_by(|left, right| utf16_cmp(left, right));
                        out.push('{');
                        stack.push(Step::Literal("}"));
                        for (index, key) in keys.into_iter().enumerate().rev() {
                            stack.push(Step::Value(&fields[key]));
                            stack.push(Step::Key(key));
                            if index > 0 {
                                stack.push(Step::Literal(","));
                            }
                        }
                    }
                },
            }
        }
    }
}

impl Drop for Value {
    /// Dismantles the value without recursing.
    ///
    /// **The derived drop is recursive, and a drop cannot return an error.** So
    /// a deeply nested value aborted the process on the way *out* even after the
    /// writer stopped recursing on the way in — same signal, same stack, one
    /// scope later. It is the quieter half of the same hazard: nothing in the
    /// source says "recurse here", which is why it survived the fix to
    /// `Value::write`.
    ///
    /// Children are moved into a flat stack and each value then drops holding
    /// none, so the recursion is one level deep whatever the value is.
    fn drop(&mut self) {
        let mut stack = Vec::new();
        take_children(self, &mut stack);
        while let Some(mut child) = stack.pop() {
            take_children(&mut child, &mut stack);
        }
    }
}

/// Moves a value's children onto `stack`, leaving it childless.
fn take_children(value: &mut Value, stack: &mut Vec<Value>) {
    match value {
        Value::Array(items) => stack.append(items),
        Value::Object(fields) => stack.extend(core::mem::take(fields).into_values()),
        _ => {}
    }
}

/// RFC 8785's ordering: by UTF-16 code unit.
fn utf16_cmp(left: &str, right: &str) -> Ordering {
    left.encode_utf16().cmp(right.encode_utf16())
}

/// RFC 8785's escaping: the two mandatory escapes, the five short forms, and a
/// `u`-escape for every other control character. Everything else is literal
/// UTF-8, because escaping what does not need it would still be canonical and
/// would make every log line and every diff unreadable.
fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // **Lower case, and the case is the specification's.** RFC 8785
            // defers string escaping to ECMAScript, whose `UnicodeEscape` is
            // "a four-digit **lowercase** hexadecimal number" — and this crate's
            // whole premise is that interoperating with a specification that
            // exists beats inventing one. `{:04X}` here was invisible: measured
            // on 2026-09-08, one character changed, `cargo run --quiet -p xtask
            // -- check --offline` answered `check passed` at `ok 1219 test(s)`
            // with the report `diff`-identical to a clean run, and `U+000B`
            // became `\u000B`. Nothing saw it because the only escape any test
            // named was `U+0001`, whose four digits hold no hex letter, and
            // `parse::hex4` accepts either case — so the round trip and
            // `canonical_form_is_a_fixed_point` both survive. `to_hex` beside
            // this has a whole test about lower case being load-bearing;
            // this is the same argument in the escape that goes into every
            // digest. `a_control_character_is_escaped_in_the_case_the_specification_writes`
            // is what holds it now.
            control if control < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// The domain-separated hash of a value.
///
/// **Domain separation is length-prefixed, not concatenated.** Hashing
/// `domain` followed by the bytes lets a crafted domain and a crafted payload
/// swap the boundary between them, and the fix costs sixteen bytes.
///
/// **The domain's length is the one that makes the encoding injective**, and the
/// value's is belt and braces. Once `len(domain) || domain` has been written the
/// boundary is fixed and everything after it is the value, so no second pair can
/// produce the same stream. Measured, rather than assumed: with *both* prefixes
/// removed, `hash("x", -1)` and `hash("x-", 1)` collide — both are `x-1` — and
/// with either one present they do not. The second prefix stays because it costs
/// eight bytes and removes the need for anyone to redo that reasoning, not
/// because the encoding would otherwise be forgeable.
/// `blake3::derive_key` would also separate domains, but it is a key derivation
/// function, and using one as a hash is the kind of choice that is fine until
/// somebody reads the result as a key.
#[must_use]
pub fn hash(domain: &str, value: &Value) -> [u8; 32] {
    let canonical = value.to_canonical();
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(domain.len() as u64).to_le_bytes());
    hasher.update(domain.as_bytes());
    hasher.update(&(canonical.len() as u64).to_le_bytes());
    hasher.update(canonical.as_bytes());
    *hasher.finalize().as_bytes()
}

/// A character as it can safely appear inside a diagnostic.
///
/// **A message that names a control character must not contain one.** Several
/// refusals in this fleet exist *because* a newline or a quote would forge a
/// line or a field — and each of them wrote the offending character out raw, so
/// the one place that names it was the one place it appeared. `§15.1` asks a
/// diagnostic to point at an exact location, and a message with a line break in
/// it does not.
///
/// **An ordinary character comes back as itself; one that could forge a line
/// comes back as `U+XXXX`** — which is what a person would have to look up
/// anyway.
///
/// This said "printable characters come back as themselves", and that is false
/// for the two characters the function exists for. `"` and `\` are printable
/// and both are escaped, because both are what would end a field or a line in
/// the formats this crate writes:
///
/// ```text
/// shown('a')  = "a"        shown('"')  = "U+0022"
/// shown('/')  = "/"        shown('\\') = "U+005C"
/// shown('\'') = "'"        shown('\n') = "U+000A"
/// ```
///
/// The test that pinned it hand-picked `'a'`, `'Z'`, `'9'`, `'é'`, `'—'`, `' '`,
/// `'/'`, `'\''` and `'中'` — nine ordinary characters, and neither of the two
/// this is written for.
///
/// # Which characters
///
/// `char::is_control` is the Unicode `Cc` category, and **that is not the set of
/// characters that end a line**. `U+2028 LINE SEPARATOR` and `U+2029 PARAGRAPH
/// SEPARATOR` are `Zl` and `Zp`, so `is_control` says no to both — and this
/// repository has already named all three together, in `OpaqueId`'s refusal and
/// in the naming rules. A function whose whole purpose is that a diagnostic
/// cannot be broken across lines had two of the three ways to break one still
/// passing straight through it.
#[must_use]
pub fn shown(character: char) -> String {
    if is_unsafe_in_a_message(character) {
        format!("U+{:04X}", character as u32)
    } else {
        character.to_string()
    }
}

/// Whether [`shown`] escapes this character, as a rule rather than as a literal.
///
/// Named so the two call sites and the tests can agree about *why* a character
/// is escaped: it either ends a line or forges a field boundary.
#[must_use]
pub fn is_unsafe_in_a_message(character: char) -> bool {
    character.is_control()
        // Zl and Zp. Not Cc, and they end a line everywhere it matters:
        // JavaScript source, JSON consumed as JavaScript, and every terminal
        // and editor that renders text by Unicode's line-breaking rules.
        || matches!(character, '\u{2028}' | '\u{2029}')
        || character == '"'
        || character == '\\'
}

/// Lowercase hex, which is how a hash appears anywhere a person reads it.
#[must_use]
pub fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Why some text is not lowercase hex.
///
/// An enum rather than a `char`, because the three failures are different and a
/// `char` could only express one of them: an odd length was encoded as the null
/// character — a magic value — and a non-ASCII character was reported as its
/// first UTF-8 *byte* reinterpreted as Latin-1, which is a character the input
/// does not contain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexError {
    /// Hex is two characters per byte, and this had an odd number.
    OddLength {
        /// How many characters there were.
        length: usize,
    },
    /// A character the alphabet does not hold.
    NotHex {
        /// Which one, and where.
        character: char,
        /// Its character offset.
        at: usize,
    },
}

impl HexError {
    /// The catalog code this maps to.
    ///
    /// It had none, and the manifest said every error in this crate carries one.
    /// `from_hex` is how a hash is read back out of a database row, so a failure
    /// here is a failure the central log has to be queryable by (`§15.1`).
    #[must_use]
    pub const fn code(self) -> nc_catalog::ErrorCode {
        nc_catalog::ErrorCode::CanonUnparsable
    }
}

#[cfg(test)]
mod hex_error_tests {
    use super::HexError;

    #[test]
    fn a_failure_reading_a_hash_back_is_reported_under_this_crates_area() {
        // The method had no caller at all, so the mapping it declares was never
        // exercised by anything. What matters about it is not that it compiles —
        // `ErrorCode` is an enum, so that much is free — but that a hash which
        // will not read back is reported under *this* crate's area, where a
        // responder chasing it will look (`NOCTUA.md §15.1`).
        // **The code, not only the area.** `CANON` has two codes and the
        // assertion below read only the first half of one: measured on
        // 2026-09-08, `CanonUnparsable` replaced by `CanonNumber`,
        // `cargo run --quiet -p xtask -- check --offline` answered
        // `check passed` at `ok 1219 test(s)` with the report `diff`-identical
        // to a clean run — an unreadable digest reported as *"A payload holds a
        // number this model cannot represent"*. The sibling mapping,
        // `ParseError::code`, is pinned code by code in `properties.rs`; two
        // mappings in one crate, one held to the code and one only to the area.
        for error in [
            HexError::OddLength { length: 3 },
            HexError::NotHex {
                character: 'z',
                at: 1,
            },
        ] {
            assert_eq!(error.code().area(), "CANON", "{error}");
            assert_eq!(
                error.code(),
                nc_catalog::ErrorCode::CanonUnparsable,
                "a hash that will not read back is reported under `{}`, which is a \
                 different failure with a different remedy: {error}",
                error.code().code()
            );
        }
    }
}

#[cfg(test)]
mod messages_are_one_line {
    use super::{HexError, from_hex, is_unsafe_in_a_message, parse, shown};

    /// A character as JSON writes it inside a string.
    fn escaped(character: char) -> String {
        match character {
            '"' => "\\\"".to_owned(),
            '\\' => "\\\\".to_owned(),
            '\n' => "\\n".to_owned(),
            '\r' => "\\r".to_owned(),
            control if control < '\u{20}' => format!("\\u{:04x}", control as u32),
            other => other.to_string(),
        }
    }

    /// Everything this repository has ever called "a character that would forge
    /// a line or a field", in one place, so a gap is visible.
    const WOULD_FORGE: [char; 10] = [
        '\n', '\r', '\u{0}', '\u{1b}', '\u{b}', '\u{85}', '\u{2028}', '\u{2029}', '"', '\\',
    ];

    #[test]
    fn every_character_that_could_end_a_line_is_named_rather_than_written() {
        for bad in WOULD_FORGE {
            let rendered = shown(bad);
            assert_eq!(
                rendered,
                format!("U+{:04X}", bad as u32),
                "U+{:04X} is written into a message raw",
                bad as u32
            );
            assert!(is_unsafe_in_a_message(bad));
        }
    }

    #[test]
    fn a_printable_character_is_still_itself() {
        // Without this, `shown` could escape everything and pass the test above
        // while making every diagnostic unreadable.
        for fine in ['a', 'Z', '9', 'é', '—', ' ', '/', '\'', '中'] {
            assert_eq!(shown(fine), fine.to_string());
            assert!(!is_unsafe_in_a_message(fine));
        }
    }

    #[test]
    fn the_two_places_that_reach_a_reader_carry_the_rule() {
        // **The rule is only worth having where it is used.** `shown` was correct
        // and its two call sites had no test in this crate at all, so a refusal
        // that stopped calling it would have gone out with a raw newline in it.
        for bad in WOULD_FORGE {
            // An even number of characters, so the length arm does not answer
            // first and leave the arm under test unreached.
            let text = format!("a{bad}");
            let message = from_hex(&text).expect_err("not hex").to_string();
            assert!(
                !message.contains(bad),
                "from_hex's refusal of U+{:04X} contains one: {message:?}",
                bad as u32
            );
            assert!(
                message.contains(&format!("U+{:04X}", bad as u32)),
                "from_hex's refusal does not name what it refused: {message}"
            );
        }
        // **The other call site**, which had none: `parse`'s duplicate-key
        // refusal is the one message a caller-supplied key reaches, and
        // replacing its `shown` with the key itself left the whole workspace
        // green. A key is where a control character arrives from outside.
        for bad in WOULD_FORGE {
            let document = format!("{{\"a{}b\":1,\"a{}b\":2}}", escaped(bad), escaped(bad));
            let message = parse(&document)
                .expect_err("a duplicate key is a refusal")
                .to_string();
            assert!(
                !message.contains(bad),
                "parse's duplicate-key refusal of U+{:04X} contains one: {message:?}",
                bad as u32
            );
            assert!(
                message.contains(&format!("U+{:04X}", bad as u32)),
                "the refusal does not name the character in the key: {message}"
            );
        }

        // And the arm that does not go through `shown`, so the loop above is not
        // the only thing keeping `HexError`'s whole `Display` honest.
        let odd = HexError::OddLength { length: 3 }.to_string();
        assert!(odd.contains('3'), "{odd}");
        assert!(!odd.contains('\n'), "{odd}");
    }
}

#[cfg(test)]
mod deep_values {
    use super::{Value, parse};
    use std::collections::BTreeMap;

    /// A value nested `depth` levels, as `{"a":{"a":{…}}}`.
    fn nested(depth: usize) -> Value {
        let mut value = Value::Integer(1);
        for _ in 1..depth {
            let mut fields = BTreeMap::new();
            fields.insert("a".to_owned(), value);
            value = Value::Object(fields);
        }
        value
    }

    #[test]
    fn depth_counts_what_it_says_it_counts() {
        assert_eq!(Value::Null.depth(), 1);
        assert_eq!(Value::Array(vec![]).depth(), 1);
        assert_eq!(nested(1).depth(), 1);
        assert_eq!(nested(10).depth(), 10);
        // The deepest branch, not the first or the last.
        assert_eq!(
            Value::Array(vec![Value::Null, nested(5), Value::Null]).depth(),
            6
        );
        // **And the same of an object, which nothing asked.** Every object this
        // sentence was written against had one key per level, so a walk that
        // followed only the first value of each — `fields.values().take(1)` —
        // answered the same for all of them, and the whole workspace stayed
        // green. Measured: a resource whose deep branch sits under its *second*
        // key reported `depth() = 2` against a real 67, and
        // `nc_journal::readable_record` — the one thing between a caller and a
        // row that, once written, makes the journal unreadable for ever, because
        // the trigger refuses `DELETE` — answered `Ok`.
        //
        // Both directions, so a `.skip(1)` walk is caught too.
        assert_eq!(
            Value::object([("a", Value::Null), ("b", nested(5))]).depth(),
            6
        );
        assert_eq!(
            Value::object([("a", nested(5)), ("b", Value::Null)]).depth(),
            6
        );
    }

    #[test]
    fn nothing_a_caller_can_do_with_a_deep_value_aborts_the_process() {
        // **Four operations, not two.** `write` and `drop` were made iterative
        // after a deep value aborted with `fatal runtime error: stack overflow`;
        // `Clone`, `PartialEq` and `Debug` were still derived, so the same
        // hazard survived in three quieter places. Measured at the time: all
        // three aborted at about two thousand levels, and `Entry::chain` reaches
        // `clone` through `hashed_form` with nothing bounding it.
        //
        // Ten thousand, not sixty-five: the point is that no depth reaches any
        // of them.
        let deep = nested(10_000);
        let copy = deep.clone();
        assert_eq!(copy, deep, "a deep value does not equal its own clone");
        assert_ne!(format!("{copy:?}"), "");
        // And inequality, which takes the other exit from the same walk.
        assert_ne!(copy, nested(9_999));
        // The clone is a real one: changing the original does not change it.
        drop(deep);
        assert_eq!(copy.depth(), 10_000);
    }

    #[test]
    fn writing_a_value_far_deeper_than_the_parser_reads_does_not_abort_the_process() {
        // **`to_canonical` had no depth bound and recursed.** A `Value` nested a
        // few hundred deep — which any caller can build, since the variants are
        // public — aborted the process with `fatal runtime error: stack
        // overflow`, in the function every journal entry and every log line goes
        // through. `parse::MAX_DEPTH`'s own doc names that exact hazard, on the
        // other side of the same crate.
        //
        // Ten thousand, not sixty-five: the point is that no depth reaches it.
        let canonical = nested(10_000).to_canonical();
        assert_eq!(canonical.matches("{\"a\":").count(), 9_999);
        assert!(canonical.ends_with(&"}".repeat(9_999)));
    }

    #[test]
    fn the_writer_and_the_parser_agree_about_what_they_can_carry() {
        // At the parser's limit, a round trip works.
        let at_the_limit = nested(parse::MAX_DEPTH);
        let written = at_the_limit.to_canonical();
        assert_eq!(parse(&written).expect("at the limit"), at_the_limit);

        // Past it, the writer still writes and the parser still refuses — which
        // is the asymmetry that bricked the journal. Nothing here can close it;
        // what closes it is the caller refusing to *store* such a value, which
        // `nc_journal` now does. This test exists so the asymmetry is a stated
        // fact rather than a surprise.
        let past_it = nested(parse::MAX_DEPTH + 2);
        let written = past_it.to_canonical();
        let refusal = parse(&written).expect_err("past the limit");
        assert!(refusal.to_string().contains("nests deeper"), "{refusal}");
    }
}

impl core::fmt::Display for HexError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OddLength { length } => write!(
                formatter,
                "hex is two characters per byte and this has {length}"
            ),
            Self::NotHex { character, at } => write!(
                formatter,
                "`{}` at {at} is not a lowercase hex digit",
                shown(*character)
            ),
        }
    }
}

impl core::error::Error for HexError {}

/// Reads lowercase hex back.
///
/// Lowercase only, deliberately: [`to_hex`] emits lowercase, and accepting both
/// would mean two spellings of one hash — which, for a value that identifies an
/// entry in a chain, is two names for one thing.
///
/// # Errors
///
/// Returns [`HexError`] naming the offending character and where it is, or the
/// length when it is odd.
pub fn from_hex(text: &str) -> Result<Vec<u8>, HexError> {
    let characters: Vec<char> = text.chars().collect();
    if !characters.len().is_multiple_of(2) {
        return Err(HexError::OddLength {
            length: characters.len(),
        });
    }
    let mut out = Vec::with_capacity(characters.len() / 2);
    // `as_chunks` rather than `chunks_exact`, because the length is already
    // known even and a fixed-size chunk gives the pair as an array — the
    // remainder is provably empty and is bound to `_` to say so.
    let (pairs, _) = characters.as_chunks::<2>();
    for (index, [high, low]) in pairs.iter().enumerate() {
        let at = index * 2;
        out.push((digit(*high, at)? << 4) | digit(*low, at + 1)?);
    }
    Ok(out)
}

fn digit(character: char, at: usize) -> Result<u8, HexError> {
    match character {
        '0'..='9' => Ok(character as u8 - b'0'),
        'a'..='f' => Ok(character as u8 - b'a' + 10),
        _ => Err(HexError::NotHex { character, at }),
    }
}

#[cfg(test)]
mod tests {
    use super::{HexError, Value, from_hex, hash, parse, to_hex, utf16_cmp};

    /// Every arm and both separators of the hand-written `Debug`.
    ///
    /// **It was written as a work stack and driven by nothing.** The impl is an
    /// explicit `Vec<Step>` rather than the derive, and the only reason to
    /// write one that way is to print a value too deeply nested to recurse
    /// through — the same hazard `Value::write`, `Value::clone` and
    /// `Value::drop` are iterative for. Measured, with two variant names
    /// changed in the output and both `if index > 0` separators deleted, so
    /// that `Array([1, 2])` printed as `Array([12])`: `cargo xtask check`
    /// printed `check passed`.
    ///
    /// What that costs is every diagnostic that prints a `Value`: a panic
    /// message, an assertion's left-hand side, a test failure. A separator is
    /// the part of a stack-based printer that is easiest to get wrong, and
    /// `Array([12])` is a value that does not exist.
    #[test]
    fn the_debug_output_names_every_shape() {
        for (value, expected) in [
            (Value::Null, "Null".to_owned()),
            (Value::Bool(true), "Bool(true)".to_owned()),
            (Value::Bool(false), "Bool(false)".to_owned()),
            (Value::Integer(-7), "Integer(-7)".to_owned()),
            (Value::text("a\"b"), "Text(\"a\\\"b\")".to_owned()),
            (Value::Array(vec![]), "Array([])".to_owned()),
            (
                Value::Array(vec![Value::Integer(1)]),
                "Array([Integer(1)])".to_owned(),
            ),
            (
                // Two items, which is the smallest value a missing separator
                // changes: one item has no separator to lose.
                Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
                "Array([Integer(1), Integer(2)])".to_owned(),
            ),
            (
                Value::Array(vec![Value::Null, Value::Bool(false), Value::Integer(3)]),
                "Array([Null, Bool(false), Integer(3)])".to_owned(),
            ),
            (Value::object([("a", Value::Null)]), {
                "Object({\"a\": Null})".to_owned()
            }),
            (
                Value::object([("b", Value::Integer(2)), ("a", Value::Integer(1))]),
                // Sorted, because the fields are a `BTreeMap` — which is the
                // property the whole crate exists for, said once more here.
                "Object({\"a\": Integer(1), \"b\": Integer(2)})".to_owned(),
            ),
            (
                Value::Array(vec![
                    Value::object([("k", Value::Array(vec![Value::Null, Value::Null]))]),
                    Value::Null,
                ]),
                "Array([Object({\"k\": Array([Null, Null])}), Null])".to_owned(),
            ),
        ] {
            assert_eq!(format!("{value:?}"), expected);
        }
    }
    /// The printer holds no recursion, so a value nothing can recurse through
    /// still prints.
    ///
    /// Ten thousand levels, the depth `clone`, `to_canonical` and `drop` are
    /// each already driven at above — the impl is a work stack for the same
    /// reason they are loops, and it was the one of the four with nothing
    /// behind the claim.
    ///
    /// Compared whole rather than by counting the opening brackets: a printer
    /// that wrote every `Array([` and then stopped would satisfy a count, and
    /// what is being checked is that the value came out the other end intact.
    #[test]
    fn a_value_too_deep_to_recurse_through_still_prints() {
        const DEEP: usize = 10_000;
        let mut value = Value::Null;
        for _ in 0..DEEP {
            value = Value::Array(vec![value]);
        }
        let expected = format!("{}Null{}", "Array([".repeat(DEEP), "])".repeat(DEEP));
        assert_eq!(format!("{value:?}"), expected);
    }

    #[test]
    fn an_object_is_written_with_its_keys_sorted_whatever_order_it_was_built_in() {
        let one = Value::object([("b", Value::Integer(2)), ("a", Value::Integer(1))]);
        let other = Value::object([("a", Value::Integer(1)), ("b", Value::Integer(2))]);
        assert_eq!(one.to_canonical(), r#"{"a":1,"b":2}"#);
        assert_eq!(one.to_canonical(), other.to_canonical());
    }

    #[test]
    fn there_is_no_whitespace_anywhere() {
        let value = Value::object([
            ("list", Value::Array(vec![Value::Integer(1), Value::Null])),
            ("nested", Value::object([("k", Value::Bool(true))])),
        ]);
        assert_eq!(
            value.to_canonical(),
            r#"{"list":[1,null],"nested":{"k":true}}"#
        );
    }

    /// Nothing above U+001F is escaped, which is the other half of the rule.
    ///
    /// **The complement was asserted for three characters.** U+0085, U+2028 and
    /// U+2029 were pinned as written literally, and every other code point was
    /// free: adding `'/' => out.push_str("\\/")` — the single most common
    /// divergence between JSON writers, and what `json_encode` does without
    /// `JSON_UNESCAPED_SLASHES` — left `check passed`, rustfmt clean and clippy
    /// clean, while producing a digest no conforming implementation agrees with.
    /// A `/` is in nearly every resource selector this journal stores.
    ///
    /// The likelier accident is U+007F: `is_unsafe_in_a_message` eighty lines
    /// below uses `char::is_control`, which is U+0000–U+001F **and**
    /// U+007F–U+009F, so anyone unifying the two predicates rebases every digest
    /// ever written. That plant was green too.
    ///
    /// This crate's premise is that interoperating with a specification that
    /// exists is worth more than inventing one, so the rule is the
    /// specification's: escape the two that must be escaped, the C0 range, and
    /// nothing else.
    #[test]
    fn nothing_a_specification_writes_literally_is_escaped() {
        // **Every scalar value, because a list is a spelling.** The loop ran
        // `0x20..=0xffff` — one plane of seventeen — and wave 13 widened it with
        // seven code points past the BMP, which held the complement for seven
        // and for none of the million beside them: a reviewer in wave 14 added
        // an escape for U+1F601, one code point from the one the list held, and
        // the crate stayed green with a digest no conforming implementation
        // agrees with. The whole range costs 0.23s against 0.02s and needs no
        // generator, so the list bought nothing. `char::from_u32` refuses the
        // surrogate range, which is what the `else { continue }` is for.
        for code in 0x20_u32..=0x10_ffff {
            let Some(character) = char::from_u32(code) else {
                continue;
            };
            if character == '"' || character == '\\' {
                continue;
            }
            assert_eq!(
                Value::text(character.to_string()).to_canonical(),
                format!("\"{character}\""),
                "U+{code:04X} was escaped, and a digest written that way is one no \
                 conforming implementation agrees with"
            );
        }
    }

    /// A hex failure names where it is, and not only the first character.
    ///
    /// **Pinned at zero, which is where every wrong formula agrees.** The two
    /// tests that read `HexError::NotHex`'s offset both offend at the first
    /// character of the first pair, so `index * 2` could be `index`, and
    /// `at + 1` could be `at`, with the whole crate green — and the reader is
    /// told *"`z` at 3 is not a lowercase hex digit"*, which is an offset into
    /// the input a person counts along. Named by a reviewer in wave 13.
    #[test]
    fn a_hex_failure_names_where_it_is_and_not_only_the_first_pair() {
        let Err(HexError::NotHex { character, at }) = from_hex("00zz") else {
            panic!("`00zz` is not hex");
        };
        assert_eq!(
            (character, at),
            ('z', 2),
            "the high nibble of the second pair"
        );
        let Err(HexError::NotHex { character, at }) = from_hex("0z") else {
            panic!("`0z` is not hex");
        };
        assert_eq!(
            (character, at),
            ('z', 1),
            "the low nibble of the first pair"
        );
    }

    #[test]
    fn a_control_character_is_escaped_so_a_line_cannot_be_forged() {
        assert_eq!(Value::text("a\nb").to_canonical(), r#""a\nb""#);
        // Escaped, not dropped: a control character in a value must not be
        // able to end a line in a log or a record in a file.
        assert_eq!(Value::text("a\u{1}b").to_canonical(), "\"a\\u0001b\"");
        assert_eq!(Value::text("a\u{8}b").to_canonical(), "\"a\\bb\"");
        assert_eq!(Value::text("a\"b\\c").to_canonical(), r#""a\"b\\c""#);
        // **All five short escapes, not two of them.** RFC 8785 names
        // `\b \t \n \f \r` and this pinned `\n` and `\b`; the other three
        // could fall through to the generic `\u00XX` form with the whole suite
        // green, because the round trip still holds — `parse("a\u0009b")`
        // renders as `"a\u0009b"` again — and nothing else looks at the bytes.
        // The crate's premise is interoperating with a standard that exists, and
        // a digest over a payload holding a tab would then be one only this code
        // can compute:
        //
        // ```text
        // weakened  "a\u0009b"  a4bd5497d6c6a8b7…      restored  "a\tb"  b23c9936f473b20e…
        // ```
        for (held, written) in [
            ("a\u{8}b", r#""a\bb""#),
            ("a\tb", r#""a\tb""#),
            ("a\nb", r#""a\nb""#),
            ("a\u{c}b", r#""a\fb""#),
            ("a\rb", r#""a\rb""#),
        ] {
            assert_eq!(
                Value::text(held).to_canonical(),
                written,
                "RFC 8785 gives this character a two-character escape, and a digest \
                 written with the generic form is one no conforming implementation \
                 agrees with"
            );
        }
    }

    #[test]
    fn text_outside_ascii_is_written_literally() {
        assert_eq!(
            Value::text("praia à noite").to_canonical(),
            "\"praia à noite\""
        );
    }

    #[test]
    fn keys_sort_by_utf16_code_unit_and_not_by_utf8_byte() {
        // The one place Rust's own ordering is wrong for this format. A key
        // above the basic multilingual plane sorts before a private-use one in
        // UTF-16 and after it in UTF-8.
        let above = "\u{10000}";
        let private = "\u{e000}";
        assert_eq!(utf16_cmp(above, private), core::cmp::Ordering::Less);
        assert!(
            above > private,
            "UTF-8 ordering disagrees, which is the point"
        );

        let value = Value::object([(private, Value::Null), (above, Value::Null)]);
        let canonical = value.to_canonical();
        assert!(
            canonical.find(above) < canonical.find(private),
            "keys were ordered by UTF-8: {canonical}"
        );
    }

    #[test]
    fn the_same_value_always_hashes_the_same_and_a_changed_one_does_not() {
        let value = Value::object([("a", Value::Integer(1))]);
        let changed = Value::object([("a", Value::Integer(2))]);
        assert_eq!(hash("test", &value), hash("test", &value));
        assert_ne!(hash("test", &value), hash("test", &changed));
    }

    #[test]
    fn two_domains_hash_one_value_differently() {
        let value = Value::Integer(1);
        assert_ne!(hash("journal.entry", &value), hash("journal.unit", &value));
        // **And a domain this function may not normalise**, which nothing here
        // could see. Every domain string in the workspace is lower-case,
        // trimmed, unpadded ASCII — the journal entry's own versioned domain,
        // `test`, `nc-canon.test`, `journal.entry`, `journal.unit`, `x`, `x-`,
        // `ab`, `a` — so `to_lowercase`, `str::trim` and an ASCII filter were each the
        // identity on all of them. Measured on 2026-09-08, one line added to
        // `hash` (`let domain = domain.to_lowercase();`):
        // `cargo run --quiet -p xtask -- check --offline` at `check passed`,
        // `ok 1219 test(s)`, the report moving only in its raw source-line
        // counters, with
        //
        // ```text
        // hash("Journal.Entry", Integer(1)) = 0fff8a45ac115421…
        // hash("journal.entry", Integer(1)) = 0fff8a45ac115421…
        // ```
        //
        // `a_domain_boundary_cannot_be_moved` proves the *boundary* cannot move
        // and never that the domain reaches the hasher verbatim. This is the
        // repair `tampering.rs` applied to the record's fields, applied to the
        // domain of the hash function itself — `hash` is public API and later
        // stages add domains.
        for (one, other) in [
            ("Journal.Entry", "journal.entry"),
            ("journal.entry ", "journal.entry"),
            (" journal.entry", "journal.entry"),
            ("journal.entry\u{301}", "journal.entry"),
            ("journal.entry", "entry"),
            ("journal.entry", "journal"),
        ] {
            assert_ne!(
                hash(one, &value),
                hash(other, &value),
                "`{one}` and `{other}` are one domain to this function, so a rendering \
                 of the domain — the most ordinary thing anybody adds to a text \
                 argument — puts two separated domains back together"
            );
        }
    }

    #[test]
    fn every_control_character_is_escaped_deterministically() {
        // The property test covers this, but under-powered: with `U+0002`
        // emitted raw it caught the defect in 7 of 12 default-sized runs, because
        // the generator has to happen to draw that one character. A loop over the
        // range costs nothing and catches it every time.
        for code in 0x00_u32..=0x1f {
            let character = char::from_u32(code).expect("a C0 control is a character");
            let canonical = Value::text(character.to_string()).to_canonical();
            assert!(
                !canonical.contains(character),
                "U+{code:04X} reached the output raw: {canonical:?}"
            );
            assert_eq!(
                parse(&canonical).expect("what to_canonical wrote must parse"),
                Value::text(character.to_string()),
                "U+{code:04X} did not survive its own escape"
            );
        }
    }

    #[test]
    fn a_control_character_is_escaped_in_the_case_the_specification_writes() {
        // Both hex letters and both nibbles: `U+000B` puts one in the last
        // position, `U+001F` puts one there over a different high nibble, and
        // `U+000E` is the plain case beside them. An assertion over `U+0001`
        // alone — which is what stood here — cannot tell the two cases apart at
        // all, because none of its four digits is a letter.
        for (character, written) in [
            ('\u{b}', "\"\\u000b\""),
            ('\u{e}', "\"\\u000e\""),
            ('\u{1f}', "\"\\u001f\""),
            ('\u{1}', "\"\\u0001\""),
        ] {
            assert_eq!(
                Value::text(character.to_string()).to_canonical(),
                written,
                "U+{:04X} is not escaped the way RFC 8785 writes it. It defers to \
                 ECMAScript's `UnicodeEscape`, which is a four-digit *lowercase* \
                 hexadecimal number — and a second spelling of one escape is a second \
                 digest over one payload, which no conforming implementation agrees with",
                character as u32
            );
        }
    }

    /// Two hashes this crate has measured, byte for byte.
    ///
    /// **The first two survivors `xtask/mutants.py` reported when `nc-canon` was
    /// carried into it**, on 2026-09-08, and they are the same clause seen
    /// twice: `hasher.update(&(domain.len() as u64).to_le_bytes())` replaced by
    /// `&(0_u64).to_le_bytes()`, and the same for the payload's prefix, both
    /// left `cargo test -q -p nc-canon` green at 46 and 11. A prefix whose value
    /// is constant is not a length prefix — it is a separator, and what the
    /// length is for is that a *caller-supplied* domain cannot forge one.
    ///
    /// Every other property this crate asserts about `hash` is relational: two
    /// digests differ, or two agree. A relation cannot see a change that moves
    /// both sides together, which is what a constant prefix, a swapped pair of
    /// prefixes and a flipped endianness all are. `nc-journal` pins the digest
    /// of a whole entry for exactly that reason and its own doc says so; this is
    /// the same instrument one crate down, over the function `nc-journal` calls.
    ///
    /// The two vectors are measured rather than derived. Changing one means
    /// changing what this crate writes into every hash chain in the system, and
    /// that is a decision rather than an edit.
    #[test]
    fn the_hash_is_the_one_this_crate_has_measured() {
        for (domain, value, digest) in [
            (
                "journal.entry",
                Value::object([("id", Value::Integer(1))]),
                "14dce37a3c08b4cdab57a4a463d0707422ca59f340b36d246f8ebff01399871d",
            ),
            (
                "x",
                Value::text("y"),
                "385add4100f2872175b515ea375feac98e7c7afc2d9df9fd472942bd12e9f2ee",
            ),
            // **One vector where a byte count and a character count differ.**
            // Every digest pinned anywhere in this tree was pure ASCII, so the
            // two length prefixes were held only where the two numbers agree:
            // measured on 2026-09-10, both `len()` calls written
            // `chars().count()` left `cargo test --workspace --locked` green
            // across 21 targets and `cargo xtask check` at `check passed` with
            // every verdict line identical. What that changes is the digest of
            // every entry whose payload holds one non-ASCII character — and
            // `action` and `resource` are the two fields `nc-journal`'s own
            // tamper corpus fills with `Cafe\u{301}`, `U+200B` and `U+2028`.
            //
            // The domain carries one too, because it is prefixed by the same
            // rule and was pinned by nothing.
            (
                "journal.entr\u{e9}e",
                Value::object([("caf\u{e9}", Value::text("\u{2028}na\u{ef}ve"))]),
                "1aae92fb61bec561d0551d19c91b42cd3d7a61959a4a963839a0e3353ce40141",
            ),
        ] {
            assert_eq!(
                to_hex(&hash(domain, &value)),
                digest,
                "`hash({domain:?}, …)` is not the digest this crate measured. Every entry \
                 in every journal is chained through this function, so a digest that \
                 moves is a chain that no longer verifies against what was stored"
            );
        }
    }

    #[test]
    fn a_domain_boundary_cannot_be_moved() {
        // **A pair that genuinely collides without the prefixes.** The pair here
        // used to be `("ab", "c")` against `("a", "bc")`, under a comment saying
        // those two would hash the same bytes without them — they would not.
        // Their canonical forms are `"c"` and `"bc"` *with the quotes*, so the
        // concatenations are `ab"c"` and `a"bc"`, which differ. Deleting both
        // prefixes left the test green: it asserted a consequence rather than the
        // property, in the one place guarding hash-domain confusion (`§19.5`).
        //
        // These two do collide. A canonical integer carries no quotes, so
        // `"x" + "-1"` and `"x-" + "1"` are the same eight bytes.
        assert_eq!(
            format!("x{}", Value::Integer(-1).to_canonical()),
            format!("x-{}", Value::Integer(1).to_canonical()),
            "the premise of this test: without prefixes these are one byte string"
        );
        assert_ne!(
            hash("x", &Value::Integer(-1)),
            hash("x-", &Value::Integer(1)),
            "the domain boundary moved, so a crafted domain and a crafted payload \
             can be swapped for one another"
        );
        assert_ne!(hash("ab", &Value::text("c")), hash("a", &Value::text("bc")));
    }

    #[test]
    fn hex_round_trips_and_rejects_what_is_not_hex() {
        let bytes = hash("test", &Value::Null);
        assert_eq!(from_hex(&to_hex(&bytes)), Ok(bytes.to_vec()));
    }

    #[test]
    fn a_hex_failure_names_the_character_the_input_actually_holds() {
        // It used to report the first UTF-8 byte of a multi-byte character,
        // reinterpreted as Latin-1 — a character the input does not contain.
        assert_eq!(
            from_hex("çç"),
            Err(super::HexError::NotHex {
                character: 'ç',
                at: 0
            })
        );
        assert_eq!(
            from_hex("abc"),
            Err(super::HexError::OddLength { length: 3 })
        );
        assert_eq!(
            from_hex("AB"),
            Err(super::HexError::NotHex {
                character: 'A',
                at: 0
            }),
            "uppercase is a second spelling of one hash"
        );
    }
}
