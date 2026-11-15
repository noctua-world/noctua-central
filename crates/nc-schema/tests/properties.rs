//! The laws the argument schema and the judge have to obey, under property test.
//!
//! `NOCTUA.md §19.2` puts laws here rather than among the examples beside the
//! code, as `nc-canon` and `nc-ids` do. The examples in `src/` pin each refusal
//! to the shape somebody thought of; these generate the shapes nobody did — an
//! instance built to fit any schema of the subset is accepted and any single
//! change to it is refused, and the judge never refuses with nothing to say;
//! a binding is read back only as it is written, and no Leader settled from a
//! configuration meets another channel's.

use nc_schema::entry::{Entry, EntryProblem, InLocale, Rules, Text, Texts};
use nc_schema::keys::{
    Binding, Chord, Claim, Key, Leader, LeaderPart, LeaderRule, Modifier, Modifiers, Settled,
    Table, Verdict, apart, intra, settle,
};
use nc_schema::{Declared, Reversibility, Schema, judge};
use proptest::prelude::*;
use proptest::strategy::ValueTree as _;
use proptest::test_runner::TestRunner;
use serde_json::{Value, json};

/// A sound declaration's schema: a note that identifies a `Content`, and text.
const NOTE_AND_TEXT: &str = r#"{"type":"object","additionalProperties":false,"properties":{"note":{"type":"string","x-entity":"Content","maxLength":128},"text":{"type":"string","maxLength":4096}},"required":["note","text"]}"#;

/// A schema inside the subset, built so an instance can be made to fit it.
#[derive(Debug, Clone)]
enum Shape {
    Object(Vec<(Shape, bool)>),
    Text { min: u8, max: u8, entity: bool },
    OneOf(Vec<String>),
    Integer { min: i64, max: i64 },
    Boolean,
    Array { item: Box<Shape>, min: u8, max: u8 },
}

impl Shape {
    /// The schema this shape describes, as JSON.
    fn schema(&self) -> Value {
        match self {
            Self::Object(fields) => {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();
                for (index, (field, needed)) in fields.iter().enumerate() {
                    properties.insert(format!("p{index}"), field.schema());
                    if *needed {
                        required.push(json!(format!("p{index}")));
                    }
                }
                json!({"type": "object", "additionalProperties": false,
                       "properties": properties, "required": required})
            }
            Self::Text { min, max, entity } => {
                let mut text = json!({"type": "string", "minLength": min, "maxLength": max});
                if *entity {
                    text["x-entity"] = json!("Content");
                }
                text
            }
            Self::OneOf(options) => json!({"type": "string", "enum": options}),
            Self::Integer { min, max } => {
                json!({"type": "integer", "minimum": min, "maximum": max})
            }
            Self::Boolean => json!({"type": "boolean"}),
            Self::Array { item, min, max } => {
                json!({"type": "array", "items": item.schema(), "minItems": min, "maxItems": max})
            }
        }
    }

    /// Instances this shape's schema accepts.
    fn fitting(&self) -> BoxedStrategy<Value> {
        match self {
            Self::Object(fields) => {
                let entries: Vec<BoxedStrategy<Option<(String, Value)>>> = fields
                    .iter()
                    .enumerate()
                    .map(|(index, (field, needed))| {
                        let name = format!("p{index}");
                        let held = field.fitting().prop_map(move |value| (name.clone(), value));
                        if *needed {
                            held.prop_map(Some).boxed()
                        } else {
                            proptest::option::of(held).boxed()
                        }
                    })
                    .collect();
                entries
                    .prop_map(|entries| Value::Object(entries.into_iter().flatten().collect()))
                    .boxed()
            }
            Self::Text { min, max, .. } => proptest::collection::vec(
                prop_oneof![proptest::char::range('a', 'z'), Just('é'), Just('ñ')],
                usize::from(*min)..=usize::from(*max),
            )
            .prop_map(|characters| Value::String(characters.into_iter().collect()))
            .boxed(),
            Self::OneOf(options) => proptest::sample::select(options.clone())
                .prop_map(Value::String)
                .boxed(),
            Self::Integer { min, max } => (*min..=*max).prop_map(|held| json!(held)).boxed(),
            Self::Boolean => any::<bool>().prop_map(Value::Bool).boxed(),
            Self::Array { item, min, max } => {
                proptest::collection::vec(item.fitting(), usize::from(*min)..=usize::from(*max))
                    .prop_map(Value::Array)
                    .boxed()
            }
        }
    }

    /// `fitting` with one thing changed so the schema no longer accepts it.
    ///
    /// `seed` picks what changes and where; every branch produces a value
    /// the schema refuses, whatever the seed.
    fn broken(&self, fitting: &Value, seed: usize) -> Value {
        let wrong_type = match self {
            Self::Object(_) => json!(1),
            Self::Text { .. } | Self::OneOf(_) => json!(false),
            Self::Integer { .. } => json!("0"),
            Self::Boolean => json!(0),
            Self::Array { .. } => json!({}),
        };
        match (self, fitting) {
            (Self::Object(fields), Value::Object(held)) => {
                let present: Vec<usize> = (0..fields.len())
                    .filter(|index| held.contains_key(&format!("p{index}")))
                    .collect();
                let needed = fields.iter().position(|(_, needed)| *needed);
                match seed % 4 {
                    0 => {
                        let mut more = held.clone();
                        more.insert("undeclared".to_owned(), json!(null));
                        Value::Object(more)
                    }
                    1 if needed.is_some() => {
                        let mut less = held.clone();
                        less.remove(&format!("p{}", needed.unwrap_or_default()));
                        Value::Object(less)
                    }
                    2 if !present.is_empty() => {
                        let index = present[(seed / 4) % present.len()];
                        let name = format!("p{index}");
                        let mut changed = held.clone();
                        changed
                            .insert(name.clone(), fields[index].0.broken(&held[&name], seed / 4));
                        Value::Object(changed)
                    }
                    _ => wrong_type,
                }
            }
            (Self::Text { max, .. }, _) if seed.is_multiple_of(2) => {
                Value::String("z".repeat(usize::from(*max) + 1))
            }
            (Self::OneOf(options), _) if seed.is_multiple_of(2) => {
                Value::String(format!("{} and more", options[0]))
            }
            (Self::Integer { max, .. }, _) if seed.is_multiple_of(2) => json!(max + 1),
            (Self::Array { item, max, .. }, Value::Array(items)) => match seed % 3 {
                0 => Value::Array(vec![json!(null); usize::from(*max) + 1]),
                1 if !items.is_empty() => {
                    let mut changed = items.clone();
                    let at = (seed / 3) % items.len();
                    changed[at] = item.broken(&items[at], seed / 3);
                    Value::Array(changed)
                }
                _ => wrong_type,
            },
            _ => wrong_type,
        }
    }
}

/// Any shape the law is stated over, three levels deep at most.
fn shapes() -> impl Strategy<Value = Shape> {
    let leaf = prop_oneof![
        (0_u8..4, 0_u8..6, any::<bool>()).prop_map(|(min, extra, entity)| Shape::Text {
            min,
            max: min + extra,
            entity
        }),
        proptest::collection::btree_set("[a-z]{1,6}", 1..4)
            .prop_map(|options| Shape::OneOf(options.into_iter().collect())),
        (-50_i64..50, 0_i64..100).prop_map(|(min, span)| Shape::Integer {
            min,
            max: min + span
        }),
        Just(Shape::Boolean),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            proptest::collection::vec((inner.clone(), any::<bool>()), 0..4).prop_map(Shape::Object),
            (inner, 0_u8..3, 0_u8..3).prop_map(|(item, min, extra)| Shape::Array {
                item: Box::new(item),
                min,
                max: min + extra
            }),
        ]
    })
}

/// A top-level shape, which is always an object, and an instance it fits.
fn cases() -> impl Strategy<Value = (Shape, Value, usize)> {
    proptest::collection::vec((shapes(), any::<bool>()), 0..5)
        .prop_map(Shape::Object)
        .prop_flat_map(|shape| {
            let fitting = shape.fitting();
            (Just(shape), fitting, any::<usize>())
        })
}

proptest! {
    /// A fitting instance is accepted and one change to it refused (`§19.2`).
    #[test]
    fn a_fitting_instance_is_accepted_and_one_change_to_it_is_refused(
        (shape, fitting, seed) in cases()
    ) {
        let text = shape.schema().to_string();
        let schema = Schema::compile(&text)
            .map_err(|error| TestCaseError::fail(format!("{error}: {text}")))?;
        prop_assert_eq!(schema.validate(&fitting), Ok(()), "{}", text);
        let broken = shape.broken(&fitting, seed);
        prop_assert!(
            schema.validate(&broken).is_err(),
            "{} was accepted by {}", broken, text
        );
    }
}

/// A declaration's fields as text a test owns, lent to the judge by
/// [`Owned::declared`].
#[derive(Debug, Clone)]
struct Owned {
    /// The command's name.
    name: String,
    /// What a person is shown for it.
    title: String,
    /// The argument schema's text.
    args_schema: String,
    /// The Cedar action.
    action: String,
    /// `realm`, `argument:<name>`, or anything.
    resource_selector: String,
    /// Whether it says it changes state.
    mutating: bool,
    /// Read, write or irreversible.
    reversibility: Reversibility,
}

/// What the judge is handed of a declaration a test owns.
impl Owned {
    /// The declaration these fields are.
    fn declared(&self) -> Declared<'_> {
        Declared {
            name: &self.name,
            title: &self.title,
            args_schema: &self.args_schema,
            action: &self.action,
            resource_selector: &self.resource_selector,
            mutating: self.mutating,
            reversibility: self.reversibility,
        }
    }
}

/// Declarations the judge admits often and refuses often: each field drawn
/// from what is sound most of the time, and from anything at all the rest, so
/// a declaration whose one defect is in a single field — or none, which is the
/// last arm of `judge` — is among what is made.
fn declarations() -> impl Strategy<Value = Owned> {
    let any_reversibility = || {
        prop_oneof![
            Just(Reversibility::Read),
            Just(Reversibility::Write),
            Just(Reversibility::Irreversible),
        ]
    };
    (
        prop_oneof![
            7 => "[a-z][a-z0-9-]{0,5}(\\.[a-z][a-z0-9-]{0,5}){1,2}",
            1 => "[a-zA-Z. -]{0,12}",
        ],
        prop_oneof![7 => "[a-z][a-z ]{0,3}", 1 => "[a-z ]{0,4}"],
        prop_oneof![
            1 => Just(String::new()),
            1 => Just("{".to_owned()),
            3 => Just(r#"{"type":"object","additionalProperties":false}"#.to_owned()),
            4 => Just(NOTE_AND_TEXT.to_owned()),
        ],
        prop_oneof![7 => "[A-Z][a-z]{2,5}", 1 => "[A-Za-z ]{0,6}"],
        prop_oneof![
            1 => Just(String::new()),
            4 => Just("realm".to_owned()),
            4 => Just("argument:note".to_owned()),
            1 => Just("argument:text".to_owned()),
            1 => "[a-z:]{0,10}",
        ],
        prop_oneof![
            7 => prop_oneof![
                Just((false, Reversibility::Read)),
                Just((true, Reversibility::Write)),
                Just((true, Reversibility::Irreversible)),
            ],
            1 => (any::<bool>(), any_reversibility()),
        ],
    )
        .prop_map(
            |(name, title, args_schema, action, resource_selector, (mutating, reversibility))| {
                Owned {
                    name,
                    title,
                    args_schema,
                    action,
                    resource_selector,
                    mutating,
                    reversibility,
                }
            },
        )
}

proptest! {
    /// The judge never refuses with nothing to say.
    ///
    /// `Err(vec![])` would be a refusal a caller cannot report, and the
    /// last `match` of `judge` is where one could come from — which only a
    /// declaration sound in every field reaches, and [`declarations`] makes
    /// one often (`the_declarations_judged_are_admitted_often_and_refused_often`).
    #[test]
    fn a_refusal_always_carries_a_problem(owned in declarations()) {
        let declared = owned.declared();
        if let Err(problems) = judge(&declared) {
            prop_assert!(!problems.is_empty(), "refused with no problem: {declared:?}");
        }
    }
}

/// The declarations the law above is judged over are admitted often and
/// refused often, so the law reaches the last arm of `judge` as well as every
/// refusal before it.
///
/// **A law can be vacuous**: with names drawn from any letters, almost none is
/// well formed — none of 256 generated declarations is admitted in each of
/// four runs, seven in 100,000 — and a last arm that answered a sound
/// declaration with `Err(vec![])` would pass the law every time.
#[test]
fn the_declarations_judged_are_admitted_often_and_refused_often() {
    let mut runner = TestRunner::deterministic();
    let strategy = declarations();
    let (mut admitted, mut refused) = (0_usize, 0_usize);
    for _ in 0..256 {
        let owned = strategy
            .new_tree(&mut runner)
            .expect("a declaration is made")
            .current();
        if judge(&owned.declared()).is_ok() {
            admitted += 1;
        } else {
            refused += 1;
        }
    }
    assert!(
        admitted >= 32 && refused >= 32,
        "{admitted} admitted and {refused} refused of 256"
    );
}

/// One locale's texts as a test owns them: a display name, a line, a
/// paragraph and tags, each built inside its bound.
#[derive(Debug, Clone)]
struct Said {
    /// The display name.
    display_name: String,
    /// The line description.
    line: String,
    /// The paragraph.
    paragraph: String,
    /// The tags.
    tags: Vec<String>,
}

/// Texts that satisfy every rule.
///
/// Words of letters, each within its bound, the paragraph longer than the
/// line so the two are never one sentence, and tags numbered so no two
/// lower-case to one.
fn said(prefix: &'static str) -> impl Strategy<Value = Said> {
    (
        "[A-Za-zÀ-ÿ]{1,12}( [a-zà-ÿ]{1,12}){0,3}",
        "[A-Za-zÀ-ÿ]{1,10}( [a-zà-ÿ]{1,10}){0,5}",
        "[A-Za-zÀ-ÿ]{1,10}( [a-zà-ÿ]{1,10}){6,40}",
        proptest::collection::vec("[a-zà-ÿ]{1,12}", 1..8),
    )
        .prop_map(move |(display_name, line, paragraph, words)| Said {
            display_name: format!("{prefix}{display_name}"),
            line: format!("{prefix}{line}"),
            paragraph: format!("{prefix}{paragraph}"),
            tags: words
                .into_iter()
                .enumerate()
                .map(|(at, word)| format!("{word} {at}"))
                .collect(),
        })
}

impl Said {
    /// What the judge is handed of this locale.
    fn lent<'a>(&'a self, locale: &'a str, emptied: Option<Text>) -> InLocale<'a> {
        let unless = |text: Text, held: &'a str| (emptied != Some(text)).then_some(held);
        InLocale {
            locale,
            texts: Texts {
                display_name: unless(Text::DisplayName, &self.display_name),
                line: unless(Text::Line, &self.line),
                paragraph: unless(Text::Paragraph, &self.paragraph),
                tags: (emptied != Some(Text::Tags)).then_some(self.tags.as_slice()),
            },
        }
    }
}

proptest! {
    /// An entry built to satisfy every rule is accepted, and emptying any one
    /// of its texts in either locale is refused naming that locale and text.
    ///
    /// **The law the shared judge owes both of its callers** (`§19.2`): the
    /// compiler hands it the catalog's texts and the kernel a plugin's, and a
    /// rule that refused a sound entry, or let one empty text through, would
    /// do so for both.
    #[test]
    fn an_entry_built_to_satisfy_every_rule_is_accepted_and_any_one_text_emptied_is_refused(
        english in said("En "),
        portuguese in said("Pt "),
        which in 0..Text::ALL.len(),
        in_portuguese in any::<bool>(),
    ) {
        let rules = Rules { requires: &Text::ALL, categories: &[] };
        let whole = [english.lent("en", None), portuguese.lent("pt-BR", None)];
        let entry = |locales| Entry { locales, categories: &[], glyph: None, same_in_both: &[] };
        prop_assert_eq!(nc_schema::entry::judge(&entry(&whole), &rules), Ok(()));

        let text = Text::ALL[which];
        let emptied = if in_portuguese {
            [english.lent("en", None), portuguese.lent("pt-BR", Some(text))]
        } else {
            [english.lent("en", Some(text)), portuguese.lent("pt-BR", None)]
        };
        let locale = if in_portuguese { "pt-BR" } else { "en" };
        let problems = nc_schema::entry::judge(&entry(&emptied), &rules)
            .expect_err("a text is emptied");
        prop_assert!(
            problems.contains(&EntryProblem::Missing { locale: locale.to_owned(), text }),
            "emptying the {} {} was not reported missing: {:?}", locale, text, problems
        );
    }
}

/// Every binding and every chord the grammar can write is read back as itself.
///
/// **Over the whole vocabulary, not a sample of it**: three tables and every
/// key, and every set of modifiers with every key, are few enough to walk.
#[test]
fn every_binding_written_is_read_back_as_itself() {
    let mut read = 0_usize;
    for table in Table::ALL {
        for key in Key::ALL {
            let binding = Binding { table, key };
            assert_eq!(Binding::parse(&binding.written()), Ok(binding));
            read += 1;
        }
    }
    for bits in 0..16_u8 {
        let held: Vec<Modifier> = Modifier::ALL
            .into_iter()
            .enumerate()
            .filter(|(at, _)| bits & (1 << at) != 0)
            .map(|(_, modifier)| modifier)
            .collect();
        for key in Key::ALL {
            let chord = Chord::new(Modifiers::of(&held), key);
            assert_eq!(Chord::parse(&chord.written()), Ok(chord));
            read += 1;
        }
    }
    assert_eq!(read, 3 * Key::ALL.len() + 16 * Key::ALL.len());
}

/// A text one of the three forms opens, a key and a tail: near enough to a
/// binding that a good share of them is one.
///
/// **Most of what a law over arbitrary text draws is nothing like a binding**,
/// and a law that only ever sees refusals cannot tell an exact reader from
/// one that normalises; so each part is drawn from spellings that are right,
/// and from the ones an author writes by mistake —
/// `the_texts_read_against_the_grammar_are_bindings_often_and_not_often`
/// holds the share.
fn binding_texts() -> impl Strategy<Value = String> {
    let opening = prop::sample::select(vec![
        "Leader ",
        "Leader+",
        "Leader+Chord+",
        "Leader ",
        "Leader+",
        "Leader+Chord+",
        "Leader ",
        "Leader+",
        "Leader+Chord+",
        "Leader ",
        "Leader+",
        "Leader+Chord+",
        "leader ",
        "Leader  ",
        "Leader+Shift+",
        "Leader+Chord ",
        "",
        "C-",
        "LEADER ",
        "Leader\u{200b} ",
    ]);
    let key = prop::sample::select(vec![
        "KeyQ",
        "Space",
        "Digit1",
        "ArrowLeft",
        "F12",
        "Equal",
        "Backquote",
        "keyq",
        "Escape",
        "q",
        "KeyQ KeyW",
        "",
    ]);
    let tail = prop::sample::select(vec!["", "", "", "", "", "", " ", "\u{2063}", "+KeyW"]);
    prop_oneof![
        6 => (opening, key, tail).prop_map(|(opening, key, tail)| format!("{opening}{key}{tail}")),
        1 => ".{0,24}",
    ]
}

/// Modifiers in some order and a key: near enough to a chord that a good
/// share of them is one.
fn chord_texts() -> impl Strategy<Value = String> {
    let word = prop::sample::select(vec![
        "Control", "Alt", "Shift", "Super", "Alt", "Super", "Ctrl", "alt", "", "Meta",
    ]);
    let key = prop::sample::select(vec!["Space", "KeyK", "Tab", "F2", "space", "Shift", ""]);
    prop_oneof![
        6 => (proptest::collection::vec(word, 0..4), key).prop_map(|(words, key)| {
            let mut parts: Vec<&str> = words;
            parts.push(key);
            parts.join("+")
        }),
        1 => ".{0,24}",
    ]
}

proptest! {
    /// No text but a binding written as the grammar writes it is read as one
    /// (acceptance 7 of Stage 14).
    ///
    /// **One spelling**: whatever `Binding::parse` accepts, `written` gives
    /// back character for character, so a reader that forgave a case, a space
    /// or an alias — reading two texts as one binding — fails here.
    #[test]
    fn no_text_but_a_written_binding_is_read_as_one(text in binding_texts()) {
        if let Ok(binding) = Binding::parse(&text) {
            prop_assert_eq!(binding.written(), text);
        }
    }

    /// No text but a chord written as the grammar writes it is read as one.
    #[test]
    fn no_text_but_a_written_chord_is_read_as_one(text in chord_texts()) {
        if let Ok(chord) = Chord::parse(&text) {
            prop_assert_eq!(chord.written(), text);
        }
    }
}

/// The texts the two laws above are judged over are read often and refused
/// often, so neither law is a law about refusals alone.
#[test]
fn the_texts_read_against_the_grammar_are_bindings_often_and_not_often() {
    let mut runner = TestRunner::deterministic();
    let (bindings, chords) = (binding_texts(), chord_texts());
    let (mut read, mut refused) = ([0_usize; 2], [0_usize; 2]);
    for _ in 0..256 {
        let text = bindings.new_tree(&mut runner).expect("a text").current();
        if Binding::parse(&text).is_ok() {
            read[0] += 1;
        } else {
            refused[0] += 1;
        }
        let text = chords.new_tree(&mut runner).expect("a text").current();
        if Chord::parse(&text).is_ok() {
            read[1] += 1;
        } else {
            refused[1] += 1;
        }
    }
    assert!(
        read.iter().chain(&refused).all(|count| *count >= 32),
        "bindings {} read and {} refused, chords {} read and {} refused, of 256 each",
        read[0],
        refused[0],
        read[1],
        refused[1]
    );
}

/// A channel's part, drawn from few enough Leaders and chord modifiers that
/// two claims meet often.
///
/// **A law over the whole vocabulary would almost never see a collision**, so
/// the pool is six sets of modifiers — Shift alone and one holding Shift
/// among them, for rule 1 — two keys and two chord modifiers.
fn parts() -> impl Strategy<Value = LeaderPart> {
    let held = prop::sample::select(vec![
        vec![Modifier::Alt],
        vec![Modifier::Control, Modifier::Alt],
        vec![Modifier::Alt, Modifier::Super],
        vec![Modifier::Alt, Modifier::Shift],
        vec![Modifier::Shift],
        vec![Modifier::Control],
    ]);
    let key = prop::sample::select(vec![Key::Space, Key::KeyK]);
    let chord = prop::sample::select(vec![Modifier::Shift, Modifier::Control]);
    (held, key, chord).prop_map(|(held, key, chord)| LeaderPart {
        leader: Leader::of(Chord::new(Modifiers::of(&held), key))
            .expect("every set in the pool holds a modifier"),
        chord,
    })
}

/// One to five claims, each a default and maybe a configured part, ranked in
/// an order that is not the order listed.
fn claims() -> impl Strategy<Value = Vec<Claim<u8>>> {
    proptest::collection::vec((parts(), proptest::option::of(parts())), 1..6)
        .prop_flat_map(|drawn| {
            let ranks: Vec<u8> = (0..).take(drawn.len()).collect();
            (Just(drawn), Just(ranks).prop_shuffle())
        })
        .prop_map(|(drawn, ranks)| {
            drawn
                .into_iter()
                .zip(ranks)
                .map(|((default, configured), rank)| Claim {
                    rank,
                    default,
                    configured,
                })
                .collect()
        })
}

proptest! {
    /// No Leader settled from a configuration meets another channel's: every
    /// accepted part keeps rule 1, and rules 2 and 3 against every other part
    /// in force.
    ///
    /// **Two parts both on their defaults are the one pair not held**, because
    /// two development checkouts share theirs until one is configured
    /// (`NOCTUA.md §17.1`); a judge that checked a part against the claims
    /// once, in the order listed, would let a later configured part meet an
    /// earlier one's.
    #[test]
    fn no_settled_leader_meets_another_channels(claims in claims()) {
        let settled = settle(&claims);
        prop_assert_eq!(settled.len(), claims.len());
        for (at, one) in settled.iter().enumerate() {
            if one.verdict == Verdict::Accepted {
                prop_assert_eq!(intra(one.part), Ok(()));
            }
            for (other_at, other) in settled.iter().enumerate() {
                let either = one.verdict == Verdict::Accepted || other.verdict == Verdict::Accepted;
                if at != other_at && either {
                    prop_assert_eq!(
                        apart(one.part, other.part), Ok(()),
                        "{:?} and {:?} met", claims[at], claims[other_at]
                    );
                }
            }
        }
    }

    /// A channel is never refused for one after it: what a claim settles to
    /// is the same whatever every claim ranked after it configures.
    #[test]
    fn a_channel_is_never_refused_for_one_after_it(
        claims in claims(),
        replaced in proptest::collection::vec(proptest::option::of(parts()), 5),
        cut in 0_u8..5,
    ) {
        let altered: Vec<Claim<u8>> = claims
            .iter()
            .zip(&replaced)
            .map(|(claim, configured)| if claim.rank > cut {
                Claim { configured: *configured, ..*claim }
            } else {
                *claim
            })
            .collect();
        let (before, after) = (settle(&claims), settle(&altered));
        for (at, claim) in claims.iter().enumerate() {
            if claim.rank <= cut {
                prop_assert_eq!(before[at], after[at], "{:?} moved", claim);
            }
        }
    }

    /// Settling does not depend on the order the claims are listed in.
    #[test]
    fn settling_does_not_depend_on_the_order_claims_are_listed(
        (claims, shuffled) in claims().prop_flat_map(|claims| {
            (Just(claims.clone()), Just(claims).prop_shuffle())
        })
    ) {
        let by_rank = |claims: &[Claim<u8>]| {
            let mut settled: Vec<(u8, Settled<u8>)> = claims
                .iter()
                .map(|claim| claim.rank)
                .zip(settle(claims))
                .collect();
            settled.sort_by_key(|(rank, _)| *rank);
            settled
        };
        prop_assert_eq!(by_rank(&claims), by_rank(&shuffled));
    }

    /// Refusing only ever returns to the default: a part in force is the
    /// claim's configured one when it was accepted, and its default otherwise.
    #[test]
    fn refusing_only_ever_returns_to_the_default(claims in claims()) {
        for (claim, settled) in claims.iter().zip(settle(&claims)) {
            match settled.verdict {
                Verdict::Accepted => prop_assert_eq!(Some(settled.part), claim.configured),
                Verdict::Default => {
                    prop_assert_eq!(claim.configured, None);
                    prop_assert_eq!(settled.part, claim.default);
                }
                Verdict::Refused { .. } => {
                    prop_assert!(claim.configured.is_some());
                    prop_assert_eq!(settled.part, claim.default);
                }
            }
        }
    }
}

/// The claims the laws above are judged over have configured parts accepted
/// often and refused often, by each rule between channels.
///
/// **A settlement law can be vacuous** the way the judge's can: drawn from the
/// whole vocabulary, two Leaders almost never meet, and a judge that accepted
/// everything would pass every law above.
#[test]
fn the_claims_settled_are_accepted_often_and_refused_often() {
    let mut runner = TestRunner::deterministic();
    let strategy = claims();
    let (mut accepted, mut shared, mut held_with, mut holding) =
        (0_usize, 0_usize, 0_usize, 0_usize);
    for _ in 0..256 {
        let claims = strategy.new_tree(&mut runner).expect("claims").current();
        for settled in settle(&claims) {
            match settled.verdict {
                Verdict::Accepted => accepted += 1,
                Verdict::Refused {
                    rule: LeaderRule::SharedWithAnother,
                    ..
                } => shared += 1,
                Verdict::Refused {
                    rule: LeaderRule::HeldWithTheChordIsAnothers,
                    ..
                } => {
                    held_with += 1;
                }
                Verdict::Refused {
                    rule: LeaderRule::HoldsTheChordModifier,
                    ..
                } => holding += 1,
                Verdict::Refused { .. } | Verdict::Default => {}
            }
        }
    }
    assert!(
        accepted >= 32 && shared >= 32 && held_with >= 16 && holding >= 16,
        "{accepted} accepted; refused {shared} for sharing a Leader, {held_with} for the \
         Leader held with the chord, {holding} for holding the chord modifier"
    );
}
