//! The laws the argument schema and the judge have to obey, under property test.
//!
//! `NOCTUA.md §19.2` puts laws here rather than among the examples beside the
//! code, as `nc-canon` and `nc-ids` do. The examples in `src/` pin each refusal
//! to the shape somebody thought of; these generate the shapes nobody did — an
//! instance built to fit any schema of the subset is accepted and any single
//! change to it is refused, and the judge never refuses with nothing to say.

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
