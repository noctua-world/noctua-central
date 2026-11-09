//! Reading JSON back into a [`crate::Value`].
//!
//! **Why a parser exists at all**, when the journal only ever writes: the WIT
//! `payload` type is canonical JSON, so everything a plugin hands the kernel —
//! command arguments, event payloads — arrives as text this has to turn into a
//! value (`NOCTUA.md §16.4`). It is also what makes the round-trip a law that can
//! be property-tested rather than a claim.
//!
//! **It accepts more than it emits, on purpose.** Input comes from somewhere
//! else: a plugin, a request, a file somebody edited. Insisting on canonical
//! input would reject a perfectly good payload for having a space in it, and
//! would make "canonical" a thing callers have to know about rather than
//! something the system does for them. What comes *out* of
//! [`Value::to_canonical`](crate::Value::to_canonical) is canonical, always.
//!
//! **A fractional number is refused rather than rounded.** There is no float in
//! this model (see the crate docs), and silently truncating `1.5` to `1` would
//! put a value into a hash chain that the sender never sent.

use std::collections::BTreeMap;

use nc_catalog::ErrorCode;

use crate::Value;

/// Why some text is not a value this model can hold.
///
/// **Sealed, for the reason the sibling type carries.** The only constructor,
/// `ParseError::at`, is private, and with the three fields `pub` a caller
/// could build `ParseError { at: 0, what: "chain broken".into(), code:
/// ErrorCode::JournalChainBroken }` and hand back, from the canonical parser, an
/// error naming a `critical` journal-tampering code. The parser's own codes are
/// the ones it raises; `stopped_at()`, `what()` and `code()` read them.
///
/// **`at()` is not one of them**: `at` is the private constructor, and the
/// reader is `stopped_at()`. A doc that names a method a caller cannot call
/// sends them to the field instead.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseError {
    /// Byte offset into the input.
    at: usize,
    /// What was wrong there, and — where there is one — what to do about it.
    what: String,
    /// The catalog code this maps to.
    ///
    /// Every error type in `nc-canon`, `nc-catalog` and `nc-ids` — the three
    /// crates `nc-journal` and `nc-log` are each built on — answers `code()`:
    /// `HexError`, `Base32Error`, `OpaqueIdError`, `NameError` and `MintError`
    /// as well as this one. This is the most likely runtime failure of the layer
    /// that carries payloads between processes, and therefore the one the
    /// central log most needs to be queried by (`NOCTUA.md §15.1`).
    code: ErrorCode,
}

impl ParseError {
    /// Where in the input the parser stopped, in bytes.
    #[must_use]
    pub const fn stopped_at(&self) -> usize {
        self.at
    }

    /// What was wrong there.
    #[must_use]
    pub fn what(&self) -> &str {
        &self.what
    }

    /// The catalog code this is reported under.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        self.code
    }

    fn at(reader: &Reader<'_>, what: &str, code: ErrorCode) -> Self {
        Self {
            at: reader.at,
            what: what.to_owned(),
            code,
        }
    }
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "at byte {}: {}", self.at, self.what)
    }
}

impl core::error::Error for ParseError {}

/// Reads one value, and nothing after it.
///
/// # Errors
///
/// Returns the byte offset, what was wrong, and the catalog code it maps to.
/// Trailing content is an error rather than ignored: two values in one payload
/// means the sender and the receiver disagree about where it ends.
///
/// It accepts a few things canonical form never emits, because input comes from
/// somewhere else: insignificant whitespace, `/` escaped, and a number written
/// with leading zeros. What it refuses is anything it could not write back —
/// a fraction, a number outside `i64`, a duplicate key, a lone surrogate.
pub fn parse(text: &str) -> Result<Value, ParseError> {
    let mut reader = Reader {
        bytes: text.as_bytes(),
        at: 0,
    };
    reader.skip_whitespace();
    let value = reader.value()?;
    reader.skip_whitespace();
    if reader.at != reader.bytes.len() {
        return Err(reader.error("there is more text after the value"));
    }
    Ok(value)
}

/// How deep a payload may nest.
///
/// A bound, not a preference: a recursive-descent parser on adversarial input is
/// a stack overflow, which aborts the process rather than returning an error.
/// Sixty-four is far past anything a command's arguments have ever needed.
pub const MAX_DEPTH: usize = 64;

/// The deepest [`Value::depth`] this parser will read back.
///
/// **The two numbers are on different scales, and comparing them is a
/// measurable off-by-one.** [`Value::depth`] counts a scalar as 1;
/// `value_at_depth` starts the outermost value at 0. So the deepest value this
/// parser accepts — `[` sixty-four times, a scalar, `]` sixty-four times, which
/// this module's own test asserts parses — reports a `depth()` of 65, and a
/// caller comparing `depth() > MAX_DEPTH` refuses a value the parser accepts
/// while saying "nests 65 deep and the journal reads 64": two numbers that are
/// not measurements of the same thing.
///
/// One name for each scale, so a caller holding a [`Value`] compares like with
/// like. `the_two_depth_scales_agree_at_the_boundary` is what keeps them from
/// drifting apart: it builds the value at exactly this depth and parses it.
///
/// [`Value`]: crate::Value
/// [`Value::depth`]: crate::Value::depth
pub const MAX_NESTING: usize = MAX_DEPTH + 1;

/// The most characters of a key written twice that its refusal shows.
///
/// **A key is as long as the text holding it, and the refusal is not**: it is
/// logged, and quoted by whoever reads the text, so the key is cut past this
/// many of its own characters and `…` says so.
const KEY_SHOWN_AT_MOST: usize = 64;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn error(&self, what: &str) -> ParseError {
        ParseError::at(self, what, ErrorCode::CanonUnparsable)
    }

    /// A number this model cannot hold, which is a different failure from text
    /// that is not a value at all.
    fn number_error(&self, what: &str) -> ParseError {
        ParseError::at(self, what, ErrorCode::CanonNumber)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// The four bytes JSON calls whitespace, and only those.
    ///
    /// RFC 8259 §2 names space, tab, line feed and carriage return. A vertical
    /// tab or a form feed is **not** whitespace in JSON and must stay an error;
    /// widening this to `is_ascii_whitespace` would accept `\u{b}` and `\u{c}`
    /// between tokens and make this parser read documents no other one does.
    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), ParseError> {
        if self.peek() == Some(byte) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", char::from(byte))))
        }
    }

    fn value(&mut self) -> Result<Value, ParseError> {
        self.value_at_depth(0)
    }

    fn value_at_depth(&mut self, depth: usize) -> Result<Value, ParseError> {
        if depth > MAX_DEPTH {
            return Err(self.error("the value nests deeper than this parser will follow"));
        }
        match self.peek() {
            Some(b'n') => self.literal("null", Value::Null),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'"') => self.string().map(Value::Text),
            Some(b'[') => self.array(depth),
            Some(b'{') => self.object(depth),
            Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
            Some(_) => Err(self.error("this is not the start of a value")),
            None => Err(self.error("the text ended where a value was expected")),
        }
    }

    fn literal(&mut self, word: &str, value: Value) -> Result<Value, ParseError> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(self.error(&format!("expected `{word}`")))
        }
    }

    fn array(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.at += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.skip_whitespace();
            items.push(self.value_at_depth(depth + 1)?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b']') => {
                    self.at += 1;
                    return Ok(Value::Array(items));
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, ParseError> {
        self.expect(b'{')?;
        let mut fields = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(Value::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let at = self.at;
            let key = self.string()?;
            if fields.contains_key(&key) {
                // Refused rather than last-wins. A duplicate key is how two
                // readers of one payload end up holding different values, and a
                // hash over the survivor would not match a hash over the other.
                self.at = at;
                // The key escaped: a duplicate key holding a newline would put
                // one in the message that refuses it. And cut, counted in its
                // own characters before any is escaped.
                let mut shown: String = key
                    .chars()
                    .take(KEY_SHOWN_AT_MOST)
                    .map(crate::shown)
                    .collect();
                if key.chars().nth(KEY_SHOWN_AT_MOST).is_some() {
                    shown.push('…');
                }
                return Err(self.error(&format!("`{shown}` appears twice in one object")));
            }
            self.skip_whitespace();
            self.expect(b':')?;
            self.skip_whitespace();
            let value = self.value_at_depth(depth + 1)?;
            fields.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(b'}') => {
                    self.at += 1;
                    return Ok(Value::Object(fields));
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    fn number(&mut self) -> Result<Value, ParseError> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        // **The digit count first, because a form with no digits is not a
        // fractional number.** Asked about `.`/`e`/`E` before counting, `-.`,
        // `-e` and `-e5` — none of which holds a single digit — would be told
        // *"this model holds whole numbers only … carry it as a scaled integer
        // or as text"* under `CanonNumber`. That is advice about a value the
        // sender does not have, and the wrong `ErrorCode` for the field `§15.1`
        // says the log is queried by. `parse("-")` takes the other branch.
        let digits = self.at - start - usize::from(self.bytes[start] == b'-');
        if digits == 0 {
            self.at = start;
            return Err(self.error("a number needs at least one digit"));
        }
        if matches!(self.peek(), Some(b'.' | b'e' | b'E')) {
            self.at = start;
            return Err(self.number_error(
                "this model holds whole numbers only, so a fractional or exponent form is \
                 refused rather than rounded — carry it as a scaled integer or as text, and \
                 say which in the field's name",
            ));
        }
        // A `-` with nothing after it is refused above rather than reported as
        // an `i64` overflow, which would send a plugin author to the wrong
        // problem entirely.
        // The bytes came from a `&str` and are all ASCII digits or a leading
        // `-`, so this slice is text by construction — no fallible conversion,
        // and therefore no error arm nobody can ever reach.
        let text = String::from_utf8_lossy(&self.bytes[start..self.at]);
        text.parse::<i64>().map(Value::Integer).map_err(|_| {
            self.at = start;
            self.number_error("this number is outside the range a 64-bit signed integer holds")
        })
    }

    fn string(&mut self) -> Result<String, ParseError> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let Some(byte) = self.peek() else {
                return Err(self.error("the text ended inside a string"));
            };
            match byte {
                b'"' => {
                    self.at += 1;
                    return Ok(out);
                }
                b'\\' => {
                    self.at += 1;
                    out.push(self.escape()?);
                }
                control if control < 0x20 => {
                    return Err(self.error("a raw control character cannot appear in a string"));
                }
                _ => {
                    // Copy one whole character. The input is a `&str`, so the
                    // bytes are valid UTF-8 and `self.at` is at a boundary —
                    // stepping a byte at a time would split a multi-byte
                    // character in half. Sliced with `from_utf8_lossy` rather
                    // than with a fallible conversion, because a failure arm
                    // here is an arm no reader can ever see and no test can
                    // ever reach.
                    // The byte at `self.at` is not a quote, a backslash or a
                    // control character, and the input is a `&str` — so it
                    // starts a character, and taking one cannot fail. Written as
                    // an infallible step rather than as a fallible one with an
                    // arm no reader can ever see.
                    let width = Self::width_of(byte);
                    let end = (self.at + width).min(self.bytes.len());
                    out.push_str(&String::from_utf8_lossy(&self.bytes[self.at..end]));
                    self.at = end;
                }
            }
        }
    }

    fn escape(&mut self) -> Result<char, ParseError> {
        let Some(byte) = self.peek() else {
            return Err(self.error("the text ended inside an escape"));
        };
        // **The offset before the step, because a step is one byte and a
        // character is not.** With `self.at += 1` first, the rejecting arm
        // below reports `self.at` — so for `"\é"` the error carries byte 3,
        // which is the middle of a two-byte character. `ParseError.at` is
        // documented as a byte offset and rendered as `at byte N`; a caller
        // writing `&text[..error.at]` — the ordinary thing to do with one —
        // panics, in the crate whose job is to absorb hostile input. Measured
        // over 400,000 malformed inputs, that order would be the only rule
        // producing a non-boundary offset, 186 times.
        //
        // It is also the byte *after* the offender, where every other rule
        // reports the offender itself.
        let offender = self.at;
        self.at += 1;
        Ok(match byte {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode_escape(),
            _ => {
                self.at = offender;
                return Err(self.error("this is not an escape JSON defines"));
            }
        })
    }

    fn unicode_escape(&mut self) -> Result<char, ParseError> {
        let high = self.hex4()?;
        // A surrogate pair is two escapes, and a lone half is not a character.
        // Rejecting it is what keeps every `Value::Text` valid UTF-8, which is
        // what lets `to_canonical` write text through without checking.
        if (0xd800..0xdc00).contains(&high) {
            if !self.bytes[self.at..].starts_with(b"\\u") {
                return Err(self.error("a high surrogate must be followed by its low half"));
            }
            self.at += 2;
            let low = self.hex4()?;
            if !(0xdc00..0xe000).contains(&low) {
                return Err(self.error("a high surrogate must be followed by its low half"));
            }
            let combined = 0x1_0000 + ((high - 0xd800) << 10) + (low - 0xdc00);
            // **A refusal, not a substitution.** A well-formed pair is always a
            // character and the two range checks above are what make that true —
            // but `unwrap_or(REPLACEMENT_CHARACTER)` says *"if they ever stop
            // making it true, put a character the sender never sent into a hash
            // chain"*. Measured with the high range narrowed by one digit,
            // `0xd800..0xdbff`, and nothing exercising the endpoints of either
            // range: `"\ud800"` and `"\udc00"` are still refused, `"\udbff"` is
            // **accepted** as `U+FFFD`, `cargo test -p nc-canon` stays green and
            // `check --offline` is byte-identical to a clean run. A silent
            // fall-through is what this file's own paragraph beside `width_of`
            // calls the shape to avoid.
            let Some(character) = char::from_u32(combined) else {
                return Err(self.error("a surrogate pair that is not a character"));
            };
            return Ok(character);
        }
        if (0xdc00..0xe000).contains(&high) {
            return Err(self.error("a low surrogate cannot stand on its own"));
        }
        // Four hex digits below 0xd800 or at 0xe000 and above are all scalar
        // values, for the same reason — and for the same reason as above, if
        // that ever stops being true this refuses rather than inventing a
        // character.
        char::from_u32(high).ok_or_else(|| self.error("an escape that is not a character"))
    }

    /// How many bytes a UTF-8 character beginning with `byte` occupies.
    ///
    /// The input is a `&str`, so a leading byte is well formed by construction.
    fn width_of(byte: u8) -> usize {
        match byte {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            _ => 4,
        }
    }

    fn hex4(&mut self) -> Result<u32, ParseError> {
        if self.at + 4 > self.bytes.len() {
            return Err(self.error("an escape needs four hex digits"));
        }
        let text = core::str::from_utf8(&self.bytes[self.at..self.at + 4])
            .map_err(|_| self.error("an escape needs four hex digits"))?;
        // **Every one of the four is a hex digit.** `from_str_radix` accepts a
        // leading `+` or `-`, so without this `\u+abc` parses as U+0ABC — a
        // leniency this parser's documentation does not offer, arriving through
        // a call whose own error message says four hex digits.
        if !text.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(self.error("an escape needs four hex digits"));
        }
        let value = u32::from_str_radix(text, 16)
            .map_err(|_| self.error("an escape needs four hex digits"))?;
        self.at += 4;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_DEPTH, parse};

    use crate::Value;

    /// The two leniencies this parser's own header promises, asked both ways.
    ///
    /// **A rule that is too strong is green until something legitimate tries.**
    /// `parse`'s `# Errors` and this module's header both promise a document
    /// written somewhere else is accepted: insignificant whitespace, `/`
    /// escaped, and a number with leading zeros. Whitespace has two tests, and
    /// without this nothing asks the other two in either direction: both
    /// deletions — the `\/` arm removed, and a leading zero refused — are
    /// measured green.
    ///
    /// The `payload` this parses arrives from a plugin. `\/` and `01` are both
    /// well-formed JSON that ordinary producers emit, and refusing either sends
    /// a plugin author a diagnostic about a document that is valid.
    #[test]
    fn a_document_written_somewhere_else_is_accepted() {
        // `\/` is an escape JSON defines and this writer never emits.
        assert_eq!(
            parse(r#""a\/b""#).expect("an escaped solidus is JSON"),
            Value::text("a/b")
        );
        assert_eq!(
            parse(r#"{"a\/b":1}"#).expect("an escaped solidus in a key is JSON"),
            Value::object([("a/b", Value::Integer(1))])
        );
        // A leading zero is a number JSON's grammar allows a producer to write.
        for (written, meant) in [("01", 1), ("00", 0), ("-01", -1), ("000012", 12)] {
            assert_eq!(
                parse(written).unwrap_or_else(|error| panic!("{written}: {error}")),
                Value::Integer(meant),
                "`{written}` is a number written somewhere else, and this refused it"
            );
        }
    }

    #[test]
    fn all_four_of_json_s_whitespace_bytes_are_skipped_and_no_others() {
        // All four: with only a space in any fixture, `matches!(…, Some(b' '))`
        // alone passes the rest of the suite while refusing every
        // pretty-printed document this parser is handed.
        for gap in [" ", "\t", "\n", "\r", " \t\r\n "] {
            let text = format!("{gap}{{{gap}\"a\"{gap}:{gap}[{gap}1{gap},{gap}2{gap}]{gap}}}{gap}");
            let parsed = super::parse(&text).unwrap_or_else(|error| panic!("{gap:?}: {error}"));
            assert_eq!(
                parsed,
                crate::Value::object([(
                    "a",
                    crate::Value::Array(vec![crate::Value::Integer(1), crate::Value::Integer(2)])
                )]),
                "{gap:?}"
            );
        }
        // **And no others.** A vertical tab and a form feed are ASCII whitespace
        // and are *not* JSON whitespace, so widening this to
        // `is_ascii_whitespace` would make this parser read documents no other
        // conforming one does — and the journal hashes what this parses.
        for not_whitespace in ["\u{b}", "\u{c}", "\u{a0}", "\u{2028}"] {
            let text = format!("{not_whitespace}1");
            assert!(
                super::parse(&text).is_err(),
                "{not_whitespace:?} was accepted between tokens"
            );
        }
    }

    #[test]
    fn every_shape_the_model_holds_reads_back() {
        let value = Value::object([
            ("null", Value::Null),
            ("yes", Value::Bool(true)),
            ("no", Value::Bool(false)),
            ("number", Value::Integer(-42)),
            ("text", Value::text("praia à noite")),
            (
                "list",
                Value::Array(vec![Value::Integer(1), Value::text("two")]),
            ),
            ("nested", Value::object([("k", Value::Null)])),
        ]);
        assert_eq!(parse(&value.to_canonical()), Ok(value));
    }

    /// Both ends of every width class this reader steps by.
    ///
    /// **The generator draws the middle of each class and never an end.**
    /// `width_of` decides how many bytes a character occupies, and the generator
    /// alone pins its arms only where a mistake changes nothing: without these
    /// rows, `0xe0..=0xef` narrowed to `0xe0..=0xee` leaves `cargo xtask check`
    /// byte-identical to a clean run, and under it a rendering **this crate has
    /// just written** cannot be read back —
    ///
    /// ```text
    /// in="！"  canonical="\"！\""  parse=Err("at byte 5: the text ended inside a string")
    /// ```
    ///
    /// U+FF01 is one of the fullwidth forms; `0xef` also leads U+FE0F and the
    /// byte-order mark. Narrowed at the other end, `0xe1..=0xef` costs Thai,
    /// Devanagari, Bengali and Tibetan, and `0xc0..=0xde` costs U+07C0–U+07FF.
    ///
    /// So the ends are written out. Each row is the first or the last scalar
    /// value whose UTF-8 encoding begins with the lead byte its arm names, and
    /// each goes out through the writer and back in through the reader — which
    /// is the property that matters: what this crate emits, it reads.
    #[test]
    fn both_ends_of_every_width_class_read_back() {
        for (lead, character) in [
            (0x7f_u8, '\u{7f}'),
            (0xc2, '\u{80}'),
            (0xdf, '\u{7ff}'),
            (0xe0, '\u{800}'),
            (0xef, '\u{ffff}'),
            (0xf0, '\u{10000}'),
            (0xf4, '\u{10ffff}'),
        ] {
            let mut encoded = [0_u8; 4];
            assert_eq!(
                character.encode_utf8(&mut encoded).as_bytes()[0],
                lead,
                "U+{:04X} does not begin the arm this row is written for",
                character as u32
            );
            // Beside a second character, so a width read one byte short or one
            // byte long lands inside the next one rather than at the quote,
            // where the parser's own bounds clamp would hide it.
            let value = Value::object([("k", Value::text(format!("a{character}b{character}")))]);
            let canonical = value.to_canonical();
            assert_eq!(
                parse(&canonical),
                Ok(value),
                "U+{:04X} leads {lead:#04x}, and what this crate wrote as {canonical} \
                 is not what it read back",
                character as u32
            );
        }
    }

    #[test]
    fn whitespace_is_accepted_on_the_way_in_and_never_written_on_the_way_out() {
        let value = parse("{ \"a\" : [ 1 , 2 ] }").expect("parses");
        assert_eq!(value.to_canonical(), r#"{"a":[1,2]}"#);
    }

    #[test]
    fn a_number_with_no_digits_says_so_rather_than_blaming_the_range() {
        // `-` is not an i64 overflow: reporting it as one sends the author of a
        // plugin to the wrong problem entirely.
        let error = parse("-").expect_err("must refuse");
        assert!(error.what.contains("at least one digit"), "{error}");
        assert_eq!(error.code, nc_catalog::ErrorCode::CanonUnparsable);
    }

    #[test]
    fn a_fractional_number_is_refused_rather_than_rounded() {
        // Rounding would put a value into a hash chain that the sender never
        // sent, and nothing downstream could tell.
        let error = parse("1.5").expect_err("must refuse");
        assert!(error.what.contains("whole numbers"), "{error}");
        assert_eq!(error.code, nc_catalog::ErrorCode::CanonNumber);
        assert!(parse("1e3").is_err());
    }

    /// The depth bound is the number its own paragraph argues for.
    ///
    /// **This crate's other tests and `nc-journal`'s read the constant under
    /// test**, so they move with it. `nc-kernel`'s
    /// `a_tree_is_held_to_its_count_of_nodes_and_its_depth` builds a tree of
    /// `ui_tree_depth` nodes and so notices only a bound lowered below that
    /// tree's depth, not one moved anywhere above it. `nc-journal` compares
    /// against it, so a lower bound would make the journal refuse any command
    /// whose arguments nest one level past it. The doc says "far past anything
    /// a command's arguments have ever needed"; that is an argument about a
    /// number, and the number belongs where a change to it has to be deliberate.
    #[test]
    fn the_depth_bound_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            super::MAX_DEPTH,
            64,
            "far past anything a command's arguments have ever needed, and far below \
             what would let a hostile payload exhaust the stack"
        );
    }

    /// The bound is tested where it is, not twenty digits past it.
    ///
    /// **`99999999999999999999` alone is not a range test** — twenty nines,
    /// about `10^20`, five times `u64::MAX`. The proptest generator is a uniform
    /// `any::<i64>()`, which reaches an endpoint with probability zero, so with
    /// nothing naming `i64::MAX`, `i64::MIN`, `u64::MAX` or `2^53`, a one-line
    /// widening — a `u64` fallback with an `as i64` cast — puts values the
    /// sender never wrote into the chain, and `cargo xtask check` passes:
    ///
    /// ```text
    /// parse("9223372036854775808")  = Ok(Integer(-9223372036854775808))
    /// parse("18446744073709551615") = Ok(Integer(-1))
    /// parse("99999999999999999999") = Err(…)     <- twenty nines, still refused
    /// ```
    ///
    /// A digest over a number that is not the number that was sent is a chain
    /// that verifies on one side and not the other, which is what this crate
    /// exists to make impossible. This file says the same thing about the depth
    /// bound eight screens down: *a bound tested only at `MAX - 1` and `MAX + 5`
    /// is a bound nobody has checked.*
    #[test]
    fn a_number_outside_the_range_is_refused_rather_than_wrapped() {
        // Both ends, exactly.
        assert_eq!(parse("9223372036854775807"), Ok(Value::Integer(i64::MAX)));
        assert_eq!(parse("-9223372036854775808"), Ok(Value::Integer(i64::MIN)));
        // And one past each, which is where a wrap shows up as a valid answer.
        assert!(
            parse("9223372036854775808").is_err(),
            "one past `i64::MAX` parsed, so a value wrapped rather than being refused"
        );
        assert!(
            parse("-9223372036854775809").is_err(),
            "one past `i64::MIN` parsed, so a value wrapped rather than being refused"
        );
        // `u64::MAX`, which a `u64` fallback would read as `-1`.
        assert!(parse("18446744073709551615").is_err());
        assert!(parse("99999999999999999999").is_err());
    }

    #[test]
    fn a_duplicate_key_is_refused() {
        let error = parse(r#"{"a":1,"a":2}"#).expect_err("must refuse");
        assert!(error.what.contains('a'), "{error}");
    }

    /// A key written twice is shown escaped and cut, because it is the text's.
    ///
    /// **Whoever quotes the refusal quotes the key**: `nc-channel` carries it
    /// into a manifest's refusal, which writes no character that ends a line
    /// or a field and no more than sixty-four of the manifest's characters.
    #[test]
    fn a_key_written_twice_is_shown_escaped_and_cut() {
        // A line feed and a line separator, a quote and a backslash, each
        // written out as its code point.
        let error = parse(r#"{"k\n\u2028\"\\":1,"k\n\u2028\"\\":2}"#).expect_err("a key twice");
        assert!(
            error
                .what
                .contains("`kU+000AU+2028U+0022U+005C` appears twice"),
            "{error}"
        );
        // Counted in characters, not bytes: a key of a hundred two-byte
        // characters shows its first sixty-four and `…`, and one of exactly
        // sixty-four shows whole.
        let twice = |key: &str| {
            parse(&format!(r#"{{"{key}":1,"{key}":2}}"#))
                .expect_err("a key twice")
                .what
        };
        assert_eq!(
            twice(&"é".repeat(100)),
            format!("`{}…` appears twice in one object", "é".repeat(64))
        );
        assert_eq!(
            twice(&"é".repeat(64)),
            format!("`{}` appears twice in one object", "é".repeat(64))
        );
    }

    #[test]
    fn text_after_the_value_is_an_error() {
        assert!(parse("1 2").is_err());
        assert!(parse("{} {}").is_err());
    }

    #[test]
    fn a_surrogate_pair_becomes_one_character_and_a_lone_half_is_refused() {
        assert_eq!(parse(r#""😀""#), Ok(Value::text("\u{1f600}")));
        // Asserting the *message*, not `is_err()`: a lone high surrogate is
        // refused by `hex4`'s length check as well, so `is_err()` stays true
        // with the guard this is named for deleted.
        let lone = parse(r#""\ud83d""#).expect_err("must refuse");
        assert!(lone.what.contains("low half"), "{lone}");
        assert!(parse(r#""\ude00""#).is_err());

        // **Both ranges at both ends.** A one-digit narrowing moves only an
        // endpoint, which lone halves strictly interior to each range, such as
        // `\ud83d` and `\ude00`, never reach — and at the endpoint it produces
        // no error at all but `U+FFFD`, silently, in the crate whose job is to
        // absorb hostile input. Measured with
        // `0xd800..0xdbff` and only those two halves named: `"\udbff"` accepted
        // as `Text("\u{fffd}")`, `cargo test -p nc-canon` green, and
        // `check --offline` byte-identical to a clean run. This file's own
        // paragraph says it: *"a bound tested only at `MAX - 1` and `MAX + 5`
        // is a bound nobody has checked"*.
        for high in ["\\ud800", "\\udbff"] {
            let refused = parse(&format!("\"{high}\"")).expect_err("a lone high half");
            assert!(
                refused.what.contains("low half"),
                "`{high}` was refused for another reason, or accepted: {refused}"
            );
        }
        for low in ["\\udc00", "\\udfff"] {
            let refused = parse(&format!("\"{low}\"")).expect_err("a lone low half");
            assert!(
                refused.what.contains("stand on its own"),
                "`{low}` was refused for another reason, or accepted: {refused}"
            );
        }
        // And the top of the pair arithmetic, which is the other end of the same
        // two ranges: the highest pair there is.
        assert_eq!(
            parse(r#""\udbff\udfff""#),
            Ok(Value::text("\u{10ffff}")),
            "the widest surrogate pair did not come back as the character it names"
        );
        // A high half followed by a low one that is out of range is refused
        // rather than composed.
        assert!(parse(r#""\ud800\ue000""#).is_err());
    }

    #[test]
    fn an_escape_with_too_few_digits_is_refused() {
        // The one arm that *is* reachable: four hex digits, and a string that
        // ends before them.
        assert!(parse("\"\\uabc\"").is_err());
        assert!(parse("\"\\u\"").is_err());
        let error = parse("\"\\uab").expect_err("must refuse");
        assert!(error.what.contains("four hex digits"), "{error}");
    }

    #[test]
    fn a_high_surrogate_followed_by_something_that_is_not_an_escape_is_refused() {
        // The **`\u` guard**, with something other than an escape after the
        // high half. Without it, `self.at += 2`
        // swallows whatever two bytes follow and reads the next four as the low
        // half — so `"\ud83dzzdc00"` becomes `"🐀"`: two characters vanish and a
        // character the sender never sent enters a hash chain.
        //
        // A test over this shape asserting only `is_err()` stays true under that
        // mutation for a different reason — `hex4`'s *length* check catches it
        // instead. A test asserting a consequence rather than the property.
        for attempt in [
            "\"\\ud83dzzdc00\"",
            "\"\\ud83dABdc00\"",
            "\"\\ud83d??dc00\"",
        ] {
            let error = parse(attempt).expect_err("must refuse");
            assert!(
                error.what.contains("low half"),
                "{attempt} was refused for the wrong reason: {error}"
            );
        }
        // And the pair that is well formed still works, so this is about what
        // follows the high half and not about surrogates in general.
        assert_eq!(
            parse("\"\\ud83d\\ude00\"").expect("parses").to_canonical(),
            "\"😀\""
        );
    }

    #[test]
    fn a_high_surrogate_followed_by_an_ordinary_escape_is_refused() {
        // The **low-half range** check. A high surrogate with nothing after it,
        // and a lone low one, both take the other arm; this one needs a
        // well-formed `\u` escape whose value is outside `DC00..E000`.
        //
        // Removing the check underflows `low - 0xDC00`: a debug panic, and in
        // release a wrapped value that becomes `U+FFFD` — a character the sender
        // never sent, inside a hash chain.
        let error = parse("\"\\ud83d\\u0041\"").expect_err("must refuse");
        assert!(error.what.contains("low half"), "{error}");
        assert!(
            parse("\"\\ud83d\\ue000\"").is_err(),
            "just past the low range"
        );
        assert!(
            parse("\"\\ud83d\\udbff\"").is_err(),
            "just below the low range"
        );
        // And the pair that is well formed still works, so this is about the
        // range and not about surrogates in general.
        assert_eq!(
            parse("\"\\ud83d\\ude00\"").expect("parses").to_canonical(),
            "\"😀\""
        );
    }

    #[test]
    fn an_escape_may_not_carry_a_sign() {
        // `u32::from_str_radix` accepts a leading `+`, so `\u+abc` parsed as
        // U+0ABC — a leniency `parse`'s documentation does not list, on a path
        // whose error says "an escape needs four hex digits".
        assert!(parse("\"\\u+abc\"").is_err());
        assert!(parse("\"\\u-abc\"").is_err());
    }

    #[test]
    fn an_escape_whose_four_bytes_are_not_four_characters_is_refused() {
        // `hex4` slices four *bytes* and reads them as text, and its third
        // failure path is the one where those bytes are not a character
        // boundary. It is reachable — a multi-byte character straight after
        // `\u` puts the slice through the middle of one — and turning the arm
        // into `unreachable!()` panics here rather than anywhere else.
        let error = parse("\"\\uab€\"").expect_err("must refuse");
        assert!(error.what.contains("four hex digits"), "{error}");
        // And the same shape with the character at the front, so the slice
        // begins rather than ends inside it.
        assert!(parse("\"\\u€ab\"").is_err());
    }

    #[test]
    fn a_multi_byte_character_is_copied_whole() {
        // Stepping a byte at a time would split one in half, and the input is a
        // `&str` so the width is known from the leading byte.
        for text in ["\"é\"", "\"中\"", "\"𐀀\"", "\"aébé\""] {
            let value = parse(text).expect("parses");
            assert_eq!(value.to_canonical(), text, "{text}");
        }
    }

    #[test]
    fn a_raw_control_character_in_a_string_is_refused() {
        assert!(parse("\"a\nb\"").is_err());

        // **Every byte of the range, and the one above it**, which is how the
        // sibling rule twenty lines up is written and why widening *it* is
        // caught. With a single character, `0x0a`, in the middle of the range,
        // narrowing the bound to `0x0b` accepts a raw `CR`, a raw `ESC` and
        // every other C0 above it, with `cargo test -p nc-canon` green and
        // `check --offline` byte-identical to a clean run.
        // A document RFC 8259 forbids and every other conforming reader rejects
        // is a payload two readers disagree about the existence of.
        for code in 0x00_u8..=0x1f {
            let raw = format!("\"a{}b\"", char::from(code));
            let refused = parse(&raw).expect_err("a raw control character");
            assert!(
                refused.what.contains("control character"),
                "U+{code:04X} was refused for another reason, or accepted: {refused}"
            );
        }
        // And the complement at the boundary: a raw space is ordinary text, and
        // a rule that refused it would refuse most documents there are.
        assert_eq!(parse("\"a b\""), Ok(Value::text("a b")));
        // The escaped forms of the same characters are how they are written, and
        // stay legitimate.
        assert_eq!(parse(r#""a\nb""#), Ok(Value::text("a\nb")));
        assert_eq!(parse(r#""a\u001bb""#), Ok(Value::text("a\u{1b}b")));
    }

    #[test]
    fn the_depth_bound_is_exactly_where_it_says_it_is() {
        // The boundary, not a value either side of it. A bound tested only at
        // `MAX - 1` and `MAX + 5` is a bound nobody has checked.
        let at = format!("{}1{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert!(parse(&at).is_ok(), "{MAX_DEPTH} levels should parse");
        let past = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert!(parse(&past).is_err(), "{} levels should not", MAX_DEPTH + 1);
    }

    /// The parser's bound and `Value::depth`'s scale meet exactly at one number.
    ///
    /// **They are two scales, not one.** `depth()` counts a scalar as 1 and
    /// `value_at_depth` starts at 0, so the deepest value this parser accepts
    /// reports 65 — and a journal asking `depth() > MAX_DEPTH` refuses a
    /// resource its own reader takes, with a message naming "65" and "64" as
    /// though they measured the same thing.
    ///
    /// This is what keeps [`MAX_NESTING`] from drifting: it is derived from
    /// `MAX_DEPTH`, and the derivation is checked against the parser rather than
    /// asserted in a comment.
    #[test]
    fn the_two_depth_scales_agree_at_the_boundary() {
        let at = format!("{}1{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        let value = parse(&at).expect("the deepest value the parser accepts");
        assert_eq!(
            value.depth(),
            super::MAX_NESTING,
            "the deepest parseable value does not report MAX_NESTING"
        );
        // And one deeper is refused, so `MAX_NESTING` is the boundary and not
        // merely a number the boundary is somewhere below.
        let past = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert!(parse(&past).is_err());
        // A scalar is the other end of the same scale.
        assert_eq!(parse("1").expect("a scalar parses").depth(), 1);

        // **And the object side, not arrays alone.** Both shapes go through
        // the same depth counter, so the two agree today — and if one stopped
        // counting, a test over arrays alone would not notice. An object is
        // the shape a real payload nests through.
        let nested = format!("{}1{}", "{\"a\":".repeat(MAX_DEPTH), "}".repeat(MAX_DEPTH));
        let value = parse(&nested).expect("the deepest object the parser accepts");
        assert_eq!(
            value.depth(),
            super::MAX_NESTING,
            "the deepest parseable object does not report MAX_NESTING"
        );
        let past = format!(
            "{}1{}",
            "{\"a\":".repeat(MAX_DEPTH + 1),
            "}".repeat(MAX_DEPTH + 1)
        );
        assert!(
            parse(&past).is_err(),
            "an object one past the bound was accepted while an array was not"
        );
    }

    #[test]
    fn nesting_past_the_bound_is_an_error_rather_than_a_stack_overflow() {
        // An overflow aborts the process. On a kernel that parses payloads from
        // plugins, that is a denial of service written by whoever sent it.
        let deep = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH + 5),
            "]".repeat(MAX_DEPTH + 5)
        );
        assert!(parse(&deep).is_err());
        let fine = format!(
            "{}1{}",
            "[".repeat(MAX_DEPTH - 1),
            "]".repeat(MAX_DEPTH - 1)
        );
        assert!(parse(&fine).is_ok());
    }

    #[test]
    fn an_error_says_where_it_was() {
        let error = parse(r#"{"a":}"#).expect_err("must refuse");
        assert!(error.at > 0, "{error}");
        assert!(error.to_string().starts_with("at byte "), "{error}");
    }
}

#[cfg(test)]
mod the_readers_are_the_surface {
    /// A refusal says what was wrong, through the accessor that is its surface.
    ///
    /// **A public accessor nothing reads is a guarantee nothing holds.**
    /// `ParseError`'s fields are private and its doc says the readers are the
    /// safe surface — but this crate's other tests reach the data through the
    /// private field, so without this `what()` could answer the empty string
    /// with all of them green. The offset's reader is `stopped_at()`; `at` is
    /// the private constructor.
    #[test]
    fn a_refusal_says_what_was_wrong_through_its_own_accessors() {
        let refused = super::parse("{,}").expect_err("`{,}` is not canonical");
        assert!(
            !refused.what().is_empty(),
            "`what()` does not answer what was wrong: {:?}",
            refused.what()
        );
        assert!(
            refused.to_string().contains(refused.what()),
            "the sentence a reader sees does not hold what `what()` answers: {} / {}",
            refused,
            refused.what()
        );
        assert!(
            refused
                .to_string()
                .contains(&refused.stopped_at().to_string()),
            "the sentence a reader sees does not hold where `stopped_at()` says it stopped"
        );
    }
}
