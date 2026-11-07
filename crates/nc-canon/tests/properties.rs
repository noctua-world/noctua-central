//! The laws canonical serialization has to obey, under property test.
//!
//! `NOCTUA.md §19.2` puts "canonical serialization round-trips" here rather than
//! among the unit tests, and the reason is the same one §16.3 gives for file
//! names: the failure mode is not a crash. A value that serialises one way and
//! reads back another produces a hash that will not verify **later**, against an
//! entry nobody can reconstruct, and the example-based tests only cover the
//! shapes somebody thought of.

use nc_canon::{Value, hash, parse};
use proptest::prelude::*;

/// Arbitrary values, including the characters that break naive canonicalisers:
/// quotes, backslashes, control characters, text above the basic multilingual
/// plane, and keys that differ only in case.
fn value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::Integer),
        "[a-z0-9 ]{0,12}".prop_map(Value::text),
        "[\"\\\\\\u0000-\\u001f]{0,4}".prop_map(Value::text),
        "[\\u{10000}-\\u{10010}\\u{e000}-\\u{e010}]{0,4}".prop_map(Value::text),
        // The characters some readers treat as ending a line while JSON does
        // not. Generated on purpose: the property below says what happens to
        // them, and an assertion about characters the generator cannot produce
        // is the vacuous kind §19.5 is about.
        "[\\u{7f}\\u{85}\\u{2028}\\u{2029}]{1,3}".prop_map(Value::text),
    ];
    leaf.prop_recursive(4, 24, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            prop::collection::vec((key(), inner), 0..4).prop_map(Value::object),
        ]
    })
}

/// Object keys, including the ones that must be escaped.
///
/// **This generator draws what must be escaped.** One such as
/// `[a-zA-Z\u{e000}\u{10000}]{1,6}` cannot draw a quote, a backslash or a
/// control character — so nothing would test that a key is escaped at all.
/// Under one, replacing `write_string(key, out)` with raw quoting would leave
/// the whole suite green, and under that mutation the key `a\nb` puts a **raw
/// newline inside a canonical rendering**, defeating the one property that
/// exists so a value cannot forge a log line.
///
/// Such keys are reachable: `parse(r#"{"a\"b":1}"#)` produces one, and any
/// caller may hand one to `Value::object`.
fn key() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z\u{e000}\u{10000}]{1,6}",
        "[\"\\\\]{1,3}",
        "[\u{0}-\u{1f}]{1,3}",
        "[a-z]{1,3}[\"\\\\\u{0}-\u{1f}]{1,2}[a-z]{1,3}",
    ]
}

proptest! {
    /// The law: what canonical form produces, the parser reads back unchanged.

#[test]
    fn a_canonical_value_reads_back_as_itself(value in value()) {
        let canonical = value.to_canonical();
        let parsed = parse(&canonical).expect("what to_canonical wrote must parse");
        prop_assert_eq!(&parsed, &value);
    }

    /// And the other direction, which is the half that catches a writer whose
    /// output the parser merely *tolerates*: serialising what was parsed gives
    /// the same bytes.
    ///
    /// **This is a consequence of its neighbour and cannot fail while that one
    /// passes.** `once = w(v)` and `twice = w(p(w(v)))`; the test ten lines
    /// above asserts `p(w(v)) == v` over the same strategy, so `twice = w(v) =
    /// once` identically — a function compared with itself, the shape `§19.5`
    /// names. It is kept because it states the law a reader looks for, and the
    /// law is held by `canonical_bytes_come_back_as_themselves` below, which
    /// starts from *bytes* rather than from a value this crate built.
    #[test]
    fn canonical_form_is_a_fixed_point(value in value()) {
        let once = value.to_canonical();
        let twice = parse(&once).expect("parses").to_canonical();
        prop_assert_eq!(once, twice);
    }

    /// A value that made the round trip hashes the same as the one that went in.
    ///
    /// **Not "whatever order they were built in", which this cannot show.**
    /// `parse` returns a `BTreeMap`, so the rebuilt object's insertion order is
    /// whatever `parse` chose — the generator has no way to vary it, and the
    /// claim is untestable through it. The order property is the test below.
    #[test]
    fn a_value_hashes_the_same_after_a_round_trip(value in value()) {
        let rebuilt = parse(&value.to_canonical()).expect("parses");
        prop_assert_eq!(hash("test", &value), hash("test", &rebuilt));
    }

    /// **The order property, asserted against the rule rather than against
    /// itself.**
    ///
    /// `Value::object` collects into a `BTreeMap`, so an object built forwards
    /// and one built backwards are *the same value* — and comparing them is
    /// `f(x) == f(x)`, true of any `f`. Measured: reversing the writer's key
    /// sort turns three other tests red and such a comparison passes.
    ///
    /// What the journal actually rests on is the written order, so that is what
    /// this reads: the keys as they appear in the canonical bytes must be the
    /// keys sorted by UTF-16 code unit, which is RFC 8785's rule and **not** the
    /// UTF-8 order `BTreeMap` gives.
    #[test]
    fn an_object_is_written_with_its_keys_in_rfc_8785_order(
        mut pairs in prop::collection::vec((key(), value()), 1..6)
    ) {
        // Duplicate keys would make the two objects genuinely different — the
        // last insert wins, and reversing changes which that is.
        pairs.sort_by(|left, right| left.0.cmp(&right.0));
        pairs.dedup_by(|left, right| left.0 == right.0);
        let object = Value::object(pairs.clone());
        let written = object.to_canonical();

        // **The whole object, rendered here in the order the rule names.**
        // Locating each key in the output does not work: a key can be a
        // substring of another key's escaped form, and `find` then reports the
        // wrong position. Building the expected text is the reimplementation
        // that has no such ambiguity.
        let mut ordered = pairs.clone();
        ordered.sort_by(|(left, _), (right, _)| left.encode_utf16().cmp(right.encode_utf16()));
        let expected = format!(
            "{{{}}}",
            ordered
                .iter()
                .map(|(key, value)| format!(
                    "{}:{}",
                    Value::text(key.clone()).to_canonical(),
                    value.to_canonical()
                ))
                .collect::<Vec<_>>()
                .join(",")
        );
        prop_assert_eq!(&written, &expected);

        // And building it the other way round produces the same bytes, which is
        // the property a caller depends on even though `Value::object` makes it
        // hard to falsify from here.
        let backwards = Value::object(pairs.into_iter().rev().collect::<Vec<_>>());
        prop_assert_eq!(written, backwards.to_canonical());
        prop_assert_eq!(hash("test", &object), hash("test", &backwards));
    }

    /// A canonical rendering occupies exactly one line of a file.
    ///
    /// The journal's chain and the central log both store one JSON document per
    /// line, so a value that could emit a raw `\n` could forge a record. It
    /// cannot: RFC 8785 escapes the whole C0 range, which is every character a
    /// byte-oriented reader treats as ending a line.
    #[test]
    fn a_canonical_rendering_occupies_one_line(value in value()) {
        let canonical = value.to_canonical();
        let raw = canonical.chars().find(|character| *character < '\u{20}');
        prop_assert!(raw.is_none(), "{:?} reached the output raw: {:?}", raw, canonical);
    }

    /// And the three characters that are **not** escaped, asserted as such.
    ///
    /// `U+0085`, `U+2028` and `U+2029` end a line for Emacs, for a JavaScript
    /// parser and for anything else reading Unicode line breaks — and RFC 8785
    /// emits all three raw. Escaping them would be safer for those readers and
    /// is exactly what this must not do: the canonical bytes are what the
    /// journal hashes, so a rendering that differs from every other conforming
    /// implementation produces a chain only this code can verify (`§16.4`).
    #[test]
    fn the_unicode_line_separators_are_emitted_raw_because_the_format_says_so(
        text in "[\\u{85}\\u{2028}\\u{2029}]{1,3}"
    ) {
        let canonical = Value::text(&text).to_canonical();
        for character in text.chars() {
            prop_assert!(
                canonical.contains(character),
                "{:?} was escaped, which diverges from RFC 8785: {:?}",
                character,
                canonical
            );
        }
        // A consumer that needs one line per record therefore writes the JSON
        // and the newline itself, and never splits on a Unicode line break.
        prop_assert!(!canonical.contains('\n'));
    }
}

/// Two documents are equal only when they hold the same keys, not the same count.
///
/// **Without this, the arm that says so is reached by no test.** Deleting
/// `None => return false` from the object case of `PartialEq` — keeping the walk,
/// so `{"a":1}` and `{"b":1}` simply share no pairs to compare — leaves
/// `cargo xtask check` green:
///
/// ```text
/// a.to_canonical() = {"a":1}
/// b.to_canonical() = {"b":1}
/// hash(a) = a33b09c4…   hash(b) = e2f83b7c…
/// a == b  -> true
/// ```
///
/// `Value` is the `payload` type this platform carries and the thing the journal
/// chain hashes (`§14.9`, `§16.4`). Two readers agreeing on one document is the
/// whole reason it exists, and `Eq` is derived from this — so `BTreeSet`,
/// `assert_eq!` and every `contains` inherit it. This compares two objects of
/// equal size with different keys.
///
/// The hash is asserted beside the equality, because the two must not be able to
/// disagree: equality that is coarser than the hash is what lets a chain verify
/// on one side and not the other.
#[test]
fn objects_of_the_same_size_with_different_keys_are_not_equal() {
    use nc_canon::Value;

    let one = Value::object([("a", Value::Integer(1))]);
    let other = Value::object([("b", Value::Integer(1))]);
    assert_ne!(
        one, other,
        "two documents with no key in common compared equal"
    );
    assert_ne!(
        nc_canon::hash("nc-canon.test", &one),
        nc_canon::hash("nc-canon.test", &other),
        "the two hash the same, so the difference is not observable at all"
    );

    // Nested, because the walk is iterative and the arm is inside its loop: a
    // difference one level down must reach the same answer.
    let outer = |inner: Value| Value::object([("held", inner)]);
    assert_ne!(outer(one.clone()), outer(other.clone()));

    // The same size and one key in common, which is the case a `len()` check and
    // a partial walk both let through.
    let two = Value::object([("a", Value::Integer(1)), ("b", Value::Integer(2))]);
    let two_other = Value::object([("a", Value::Integer(1)), ("c", Value::Integer(2))]);
    assert_ne!(two, two_other);

    // **And containers of *different* size, in both directions.** Everything
    // above is written at equal size — deliberately — so all of it still
    // passes with the two `left.len() == right.len()` guards in `PartialEq`
    // removed, after which:
    //
    // ```text
    // {"a":1} == {"a":1,"b":2}   ->  true
    // {"a":1,"b":2} == {"a":1}   ->  false     <- and `Eq` is derived from this
    // [] == [1]                  ->  true      <- the empty array equals every array
    // vec![{"a":1}].contains(&{"a":1,"b":2})   ->  true
    // ```
    //
    // which is what this test's own opening forbids: equality coarser than the
    // hash is a chain that verifies on one side and not the other. Both
    // directions, because the object case is *asymmetric* — a comparison that
    // walks the shorter side answers differently depending on which side that
    // is, so a one-way assertion passes with the guard gone.
    let bigger = Value::object([("a", Value::Integer(1)), ("b", Value::Integer(2))]);
    assert_ne!(one, bigger, "an object equals one that holds strictly more");
    assert_ne!(
        bigger, one,
        "and the comparison is not symmetric without a length"
    );
    for (left, right) in [
        (Value::Array(vec![]), Value::Array(vec![Value::Integer(1)])),
        (
            Value::Array(vec![Value::Integer(1)]),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
        ),
    ] {
        assert_ne!(left, right, "an array equals one that holds strictly more");
        assert_ne!(right, left, "and the same the other way round");
        assert_ne!(
            nc_canon::hash("nc-canon.test", &left),
            nc_canon::hash("nc-canon.test", &right),
            "the hash tells them apart, so equality must too"
        );
    }
    assert_ne!(
        nc_canon::hash("nc-canon.test", &one),
        nc_canon::hash("nc-canon.test", &bigger)
    );

    // The floor: equality still holds where it should, so the assertions above
    // are about which documents differ and not about a comparison that always
    // says no.
    assert_eq!(one.clone(), Value::object([("a", Value::Integer(1))]));
    assert_eq!(two.clone(), two.clone());
    assert_eq!(
        nc_canon::hash("nc-canon.test", &one),
        nc_canon::hash("nc-canon.test", &one)
    );
}

/// Every shape of malformed JSON this parser has a sentence for is refused.
///
/// **Without it, nine of its rejection arms are reached by no test.** Turning
/// eight of them into acceptances would leave `cargo test --workspace` green,
/// and the parser would then take five documents it exists to refuse:
///
/// ```text
///          {"a" 1} -> Ok("{\"a\":1}")          no `:` between key and value
///           {"a":1 -> Ok("{\"a\":1}")          unterminated object
///    "unterminated -> Ok("\"unterminated\"")   unterminated string
///             "\q" -> Ok("\"q\"")              an escape JSON does not define
///                  -> Ok("null")               empty input is a value
/// ```
///
/// Turned into acceptances, the other three are still refused, by the
/// trailing-garbage check rather than by the arm written for them — so a test
/// of the refusal alone would leave `expected ',' or ']'` and `expected 'null'`
/// unreachable prose. Each of these is a document the platform would otherwise
/// accept, canonicalise and hash as if it had been well-formed, which is the
/// one thing a canonical form must not do.
///
/// Asserted on the message as well as the refusal, because an arm that stops
/// being reachable still refuses — from somewhere else, saying something else —
/// and a sentence nobody can reach is prose rather than a diagnostic.
#[test]
fn every_malformed_shape_is_refused_by_the_rule_written_for_it() {
    for (text, names) in [
        // A key and a value with nothing between them.
        (r#"{"a" 1}"#, "expected `:`"),
        // An object and an array that never close.
        (r#"{"a":1"#, "expected `,` or `}`"),
        ("[1, 2", "expected `,` or `]`"),
        // A string that never closes.
        (r#""unterminated"#, "ended inside a string"),
        // An escape JSON does not define.
        (r#""\q""#, "not an escape JSON defines"),
        // Literals cut short, the arm the trailing-garbage check would otherwise
        // cover for — leaving these three sentences unreachable prose.
        ("nul", "expected `null`"),
        ("tru", "expected `true`"),
        ("fals", "expected `false`"),
        // Two values with no comma, which is the other one.
        ("[1 2]", "expected `,` or `]`"),
        // Nothing at all is not a value.
        ("", "ended where a value was expected"),
        ("   ", "ended where a value was expected"),
        // **A `\u` escape that runs off the end of the text**, which is the
        // string ending and not the escape being malformed. The bound is
        // `self.at + 4 > len`, and written `>=` it answers *"an escape needs
        // four hex digits"* for an escape that has exactly four — measured,
        // `"\u0041` (unterminated) comes back `at byte 3: an escape needs four
        // hex digits` where the text has run out at byte 7. A refusal that
        // names the wrong rule sends a reader to look at bytes that are
        // correct, which is the direction this whole list exists to hold.
        (r#""\u0041"#, "ended inside a string"),
        // And the escape genuinely short, which is the other side of that bound.
        (r#""\u04""#, "an escape needs four hex digits"),
    ] {
        let refused = parse(text)
            .map(|value| value.to_canonical())
            .expect_err(&format!("`{text}` is not JSON and was accepted"));
        let said = refused.to_string();
        assert!(
            said.contains(names),
            "`{text}` was refused, and not by the rule written for it — the message \
             does not say `{names}`: {said}"
        );
    }

    // The floor: the parser still accepts JSON, so the loop above is about
    // which documents are refused and not about a parser that refuses all.
    for text in [
        r#"{"a":1}"#,
        r#"{"a":[1,2],"b":{"c":null}}"#,
        r#""A\n\t\\\"""#,
        "[]",
        "{}",
        "null",
        "true",
        "-12",
    ] {
        parse(text).unwrap_or_else(|why| panic!("`{text}` is JSON and was refused: {why}"));
    }
}

/// Every refusal carries an offset a caller can slice at, and the rule that fits.
///
/// **Two places in the same function, both about which rule answers.**
///
/// `ParseError.stopped_at()` is documented as a byte offset and rendered as `at byte N`,
/// so `&text[..error.stopped_at()]` is the ordinary thing to do with one. An escape rule
/// that steps one *byte* forward before deciding reports, for a backslash
/// followed by a two-byte character, the middle of that character — and that
/// slice panics, in the crate whose job is to absorb hostile input. Measured
/// over 400,000 malformed inputs, that step would be the only rule producing a
/// non-boundary offset, 186 times.
///
/// And the number rules have an order: a sign followed by `.` or `e` holds no
/// digit at all, and asked about the fraction first it is told *"this model
/// holds whole numbers only … carry it as a scaled integer"* — advice about a
/// value the sender does not have, under `CanonNumber` rather than
/// `CanonUnparsable`. The digit rule stops that misdirection by running
/// first.
#[test]
fn every_refusal_can_be_sliced_at_and_names_the_rule_that_fits() {
    use nc_catalog::ErrorCode;

    // Every offset lands on a character boundary, over a corpus that mixes
    // multi-byte characters into every position a rule can stop at.
    let mut checked = 0_usize;
    for text in [
        "\"\\é\"",
        "\"\\中\"",
        "[\"a\\é\"]",
        "{\"é\":\\q}",
        "\"é",
        "[é]",
        "{é}",
        "{\"é\" 1}",
        "é",
        "\"a\\u00\"",
        "\"\\uD800\"",
    ] {
        let Err(refused) = parse(text) else {
            continue;
        };
        checked += 1;
        assert!(
            text.is_char_boundary(refused.stopped_at()),
            "`{text}` was refused at byte {}, which is not a character boundary — a \
             caller slicing there panics: {refused}",
            refused.stopped_at()
        );
    }
    assert!(checked > 8, "only {checked} case(s) were refused");

    // **And the offset points at the offender, not merely at a boundary.** The
    // corpus above holds no duplicate key and no number, and the assertion over
    // it asks only that a slice there would not panic — so over that corpus
    // alone, three refusals could be given any offset at all, and each could
    // be rewound to the wrong place with it still green.
    //
    // Each case names where the text goes wrong, so an offset that is merely
    // *legal* is not enough to pass.
    for (text, offender) in [
        // The duplicate key: the second `"a"`, not the object's opening brace.
        ("{\"a\": 1, \"a\": 2}", "\"a\": 2}"),
        // A number with no digit at all, at the sign.
        ("[1, -]", "-]"),
        // A number past what a 64-bit signed integer holds, at the number.
        ("[1, 99999999999999999999]", "99999999999999999999]"),
    ] {
        let refused = parse(text).expect_err("this text is refused");
        let at = refused.stopped_at();
        assert!(
            text.is_char_boundary(at),
            "`{text}` stopped at {at}, which is not a character boundary"
        );
        assert!(
            text[at..].starts_with(offender),
            "`{text}` stopped at {at}, which is `{}` — the rule that refused it is about \
             `{offender}`, and an offset a caller underlines points at the thing that is \
             wrong: {refused}",
            &text[at..]
        );
    }

    // And the rule that answers is the one that fits: a form with no digits is
    // not a fractional number, whatever punctuation follows the sign.
    for text in ["-", "-.", "-e", "-e5", "-E", "-x"] {
        let refused = parse(text).expect_err("no digits is not a number");
        assert!(
            refused.to_string().contains("at least one digit"),
            "`{text}` holds no digit and was refused by the fractional rule: {refused}"
        );
        assert_eq!(
            refused.code(),
            ErrorCode::CanonUnparsable,
            "`{text}` is unparsable, not a number this model cannot hold"
        );
    }

    // The floor in the other direction: a real fractional number still gets the
    // rule written for it, so asking for a digit first does not simply move
    // every case.
    for text in ["1.5", "-2.5", "1e5", "-3E2", "0.0"] {
        let refused = parse(text).expect_err("this model holds whole numbers only");
        assert!(
            refused.to_string().contains("whole numbers only"),
            "`{text}` is a fractional form and was refused by another rule: {refused}"
        );
        assert_eq!(refused.code(), ErrorCode::CanonNumber);
    }
}

/// `Debug` shows an object's keys in the order the canonical form writes them.
///
/// **Two hand-written walks of one type, disagreeing above the BMP.** `Debug`
/// used `BTreeMap`'s UTF-8 order and `to_canonical` re-sorts by UTF-16 code
/// unit, which is RFC 8785's rule and the thing the canonical writer exists for:
///
/// ```text
/// Debug        = Object({"\u{ffff}": Integer(2), "𐀀": Integer(1)})
/// to_canonical = {"𐀀":1,"￿":2}
/// ```
///
/// Opposite orders, and the one moment a person reads `Debug` on an object like
/// this is while chasing a hash mismatch — where the order is exactly what they
/// are looking at.
#[test]
fn debug_orders_an_object_the_way_the_canonical_form_does() {
    let value = nc_canon::Value::object([
        ("\u{ffff}", nc_canon::Value::Integer(2)),
        ("𐀀", nc_canon::Value::Integer(1)),
    ]);
    let shown = format!("{value:?}");
    let canonical = value.to_canonical();
    // `Debug` escapes a noncharacter and the canonical writer does not, so each
    // is looked for in the spelling that form uses.
    let (first_shown, first_written) = (
        shown.find("\\u{ffff}").expect("both keys are shown"),
        canonical.find('\u{ffff}').expect("both keys are written"),
    );
    let (second_shown, second_written) = (
        shown.find('𐀀').expect("both keys are shown"),
        canonical.find('𐀀').expect("both keys are written"),
    );
    assert_eq!(
        first_shown < second_shown,
        first_written < second_written,
        "`Debug` and `to_canonical` order these keys differently:\n  {shown}\n  {canonical}"
    );
}

/// A consumer can take a value's contents without cloning them.
///
/// **`Value` implements `Drop`, so nothing can be moved out of one.**
/// `match value { Value::Text(held) => held, … }` is `error[E0509]: cannot move
/// out of type Value, which implements the Drop trait`, and the `Drop` is
/// deliberate — it dismantles a deep value without recursing. Without the
/// methods that take the contents, every consumer is obliged to `clone()`, and
/// nothing says so or offers a way not to.
#[test]
fn a_consumer_can_take_what_a_value_holds() {
    assert_eq!(
        Value::text("held").into_text().as_deref(),
        Some("held"),
        "text could not be taken out of the value holding it"
    );
    assert_eq!(
        Value::Array(vec![Value::Integer(1)])
            .into_array()
            .map(|held| held.len()),
        Some(1)
    );
    assert_eq!(
        Value::object([("a", Value::Integer(1))])
            .into_object()
            .map(|held| held.len()),
        Some(1)
    );
    // And a value of another shape says so rather than panicking.
    assert_eq!(Value::Null.into_text(), None);
    assert_eq!(Value::Integer(1).into_array(), None);
    assert_eq!(Value::Bool(true).into_object(), None);
}

/// Canonical bytes come back as themselves.
///
/// **The second round-trip law, which the property beside it only looks like.**
/// `parse` then `to_canonical` is the identity on *values*; `to_canonical` then
/// `parse` is the identity on *canonical bytes*, and only the second catches a
/// writer whose output the parser merely tolerates.
/// `canonical_form_is_a_fixed_point` starts from a value this crate built, so it
/// is true whenever its neighbour is. This starts from bytes written by hand,
/// which is the only way to hand the parser something the writer would not have
/// produced.
///
/// Each of these is canonical: what the writer emits for that value, character
/// for character. A writer that changed its escaping, its key order or its
/// number spelling would answer something else for at least one of them.
#[test]
fn canonical_bytes_come_back_as_themselves() {
    for canonical in [
        "\"\"",
        "\"a\\nb\"",
        "0",
        "-1",
        "true",
        "null",
        "[]",
        "[1,2,3]",
        "{}",
        "{\"a\":1,\"b\":[true,null]}",
        "{\"\":0,\"a\":0,\"b\":0}",
    ] {
        let parsed = parse(canonical).expect("these are canonical");
        assert_eq!(
            parsed.to_canonical(),
            canonical,
            "the writer does not write the canonical bytes it was handed"
        );
    }
}
