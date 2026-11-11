//! A command line is read into the arguments of every command a schema
//! declares: every catalog command's and every reference-plugin command's
//! (`NOCTUA.md §20.2`).
//!
//! **Instance, then flags, then instance again.** For each schema an instance
//! using every argument it declares is built from its shape, written as the
//! command line a person types — each flag as its own word with its value, and
//! again with `=` — and read back by `nc_cli::args::arguments`, which must give
//! the instance itself; then as `--` and one JSON object, which must too. A
//! schema with one argument of every type, and a list repeated, is read the
//! same way, so no type is held only by a schema that happens not to use it;
//! and nothing here touches a disk or starts a process.

use nc_canon::Value;
use nc_cli::args::{self, Read};
use nc_schema::{Argument, Schema, Shape};

/// One argument of every type: text, a whole number, a boolean, a list of
/// text, a list of objects and an object.
const EVERY_TYPE: &str = r#"{"type":"object","additionalProperties":false,"properties":{
    "text":{"type":"string","minLength":2,"maxLength":40},
    "choice":{"type":"string","enum":["z","a"]},
    "count":{"type":"integer","minimum":-9,"maximum":9},
    "flag":{"type":"boolean"},
    "off":{"type":"boolean"},
    "tags":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":3},
    "numbers":{"type":"array","items":{"type":"integer","minimum":0}},
    "pairs":{"type":"array","items":{"type":"object","additionalProperties":false,
             "properties":{"k":{"type":"string"}},"required":["k"]}},
    "inner":{"type":"object","additionalProperties":false,
             "properties":{"x":{"type":"integer"},"y":{"type":"boolean"}},"required":["x"]}},
    "required":["text","count"]}"#;

/// Every schema the CLI has to read a command line into, by the command's
/// name.
fn every_schema() -> Vec<(String, String)> {
    let mut schemas: Vec<(String, String)> = nc_catalog::COMMANDS
        .iter()
        .map(|command| (command.name.to_owned(), command.args_schema.to_owned()))
        .collect();
    schemas.extend(
        nc_plugin_reference::behaviour::commands()
            .into_iter()
            .map(|command| (command.name, command.args_schema.to_owned())),
    );
    schemas.push(("test.every-type".to_owned(), EVERY_TYPE.to_owned()));
    schemas
}

/// A text of `length` characters, a space and a character no ASCII keyboard
/// types among them where it is long enough, so a value is read as one word
/// whatever it holds.
fn text_of(length: usize) -> String {
    "a ção-b".chars().cycle().take(length).collect()
}

/// A value `shape` accepts, at its bounds where it has them.
fn value_of(shape: &Shape<'_>, toggle: bool) -> Value {
    match shape {
        Shape::String {
            min_length,
            max_length,
            options,
            ..
        } => {
            if let Some(options) = options {
                return Value::text(*options.last().expect("an enum lists one"));
            }
            let least = usize::try_from(min_length.unwrap_or(3)).expect("a length");
            let most =
                max_length.map_or(usize::MAX, |most| usize::try_from(most).expect("a length"));
            Value::text(text_of(least.max(3).min(most)))
        }
        Shape::Integer { minimum, maximum } => Value::Integer(match (minimum, maximum) {
            (Some(least), _) => *least,
            (None, Some(most)) => (*most).min(-3),
            (None, None) => -3,
        }),
        Shape::Boolean => Value::Bool(toggle),
        Shape::Array {
            items,
            min_items,
            max_items,
        } => {
            let count = min_items.unwrap_or(2).max(2).min(max_items.unwrap_or(2));
            Value::Array((0..count).map(|at| value_of(items, at % 2 == 0)).collect())
        }
        Shape::Object(arguments) => instance_of(arguments),
    }
}

/// An instance giving every one of `arguments` a value.
fn instance_of(arguments: &[Argument<'_>]) -> Value {
    Value::object(
        arguments
            .iter()
            .enumerate()
            .map(|(at, argument)| (argument.name, value_of(&argument.shape, at % 2 == 0))),
    )
}

/// One value written as a flag's word: text as itself, a number in decimal,
/// a boolean as its word, and an object or a list as its JSON.
fn word_of(value: &Value) -> String {
    match value {
        Value::Text(text) => text.clone(),
        Value::Integer(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        other => other.to_canonical(),
    }
}

/// The command line a person types for `instance` against `arguments`.
///
/// Each flag and its value as two words, or as one with `=`, and a boolean as
/// it is written alone or with `=false`.
fn flags_of(arguments: &[Argument<'_>], instance: &Value, with_equals: bool) -> Vec<String> {
    let Value::Object(fields) = instance else {
        panic!("an instance is an object");
    };
    let mut words = Vec::new();
    for argument in arguments {
        let Some(value) = fields.get(argument.name) else {
            continue;
        };
        let values: Vec<String> = match (&argument.shape, value) {
            // A boolean takes no word after it: alone it is true, and
            // `=false` says false.
            (Shape::Boolean, Value::Bool(true)) if !with_equals => {
                words.push(format!("--{}", argument.name));
                continue;
            }
            (Shape::Boolean, Value::Bool(false)) => {
                words.push(format!("--{}=false", argument.name));
                continue;
            }
            (Shape::Array { .. }, Value::Array(items)) => items.iter().map(word_of).collect(),
            (_, value) => vec![word_of(value)],
        };
        for value in values {
            if with_equals {
                words.push(format!("--{}={value}", argument.name));
            } else {
                words.push(format!("--{}", argument.name));
                words.push(value);
            }
        }
    }
    words
}

/// What the CLI reads `words` as, against `schema`.
fn read(schema: &Schema, name: &str, words: &[String]) -> Value {
    match args::arguments(schema, name, words) {
        Ok(Read::Arguments(value)) => value,
        other => panic!("`{name}` read {words:?} as {other:?}"),
    }
}

/// Every catalog and reference-plugin schema, and one of every type, read
/// from flags back into the instance they were written from — each flag and
/// its value as two words, with `=`, and as `--` and one JSON object.
#[test]
fn every_schema_reads_its_flags_back_into_the_instance() {
    let mut types = [false; 5];
    for (name, text) in every_schema() {
        let schema = Schema::compile(&text).unwrap_or_else(|refused| panic!("{name}: {refused}"));
        let arguments = schema.arguments();
        for argument in &arguments {
            let at = match argument.shape {
                Shape::String { .. } => 0,
                Shape::Integer { .. } => 1,
                Shape::Boolean => 2,
                Shape::Array { .. } => 3,
                Shape::Object(_) => 4,
            };
            types[at] = true;
        }
        let instance = instance_of(&arguments);
        let json: serde_json::Value =
            serde_json::from_str(&instance.to_canonical()).expect("canonical text is JSON");
        assert!(
            schema.validate(&json).is_ok(),
            "{name}: the instance built for it does not fit it: {instance:?}"
        );
        for with_equals in [false, true] {
            let words = flags_of(&arguments, &instance, with_equals);
            assert_eq!(
                read(&schema, &name, &words),
                instance,
                "{name}: {words:?} is not the instance it was written from"
            );
        }
        let whole = vec!["--".to_owned(), instance.to_canonical()];
        assert_eq!(read(&schema, &name, &whole), instance, "{name}: `--`");
        let help = nc_cli::help::command(
            "noctua",
            &nc_cli::client::Listed {
                name: name.clone(),
                title: String::new(),
                args_schema: text.clone(),
            },
            &schema,
            nc_catalog::Locale::En,
        );
        for argument in &arguments {
            assert!(
                help.contains(&format!("  --{}", argument.name)),
                "{name}: help does not show `--{}`:\n{help}",
                argument.name
            );
        }
    }
    assert_eq!(types, [true; 5], "a type no schema here reads: {types:?}");
}

/// A list's flag is repeated once for each item, in the order given, and the
/// list it builds is held to the schema's count of items.
#[test]
fn a_lists_flag_is_repeated_once_for_each_item_in_order() {
    let schema = Schema::compile(EVERY_TYPE).expect("inside the subset");
    let words: Vec<String> = [
        "--text",
        "tt",
        "--count",
        "0",
        "--tags",
        "3",
        "--tags=1",
        "--tags",
        "2",
        "--pairs",
        r#"{"k":"b"}"#,
        "--pairs",
        r#"{"k":"a"}"#,
    ]
    .iter()
    .map(|word| (*word).to_owned())
    .collect();
    let value = read(&schema, "test.every-type", &words);
    let Value::Object(fields) = &value else {
        panic!("an object");
    };
    assert_eq!(
        fields.get("tags"),
        Some(&Value::Array(vec![
            Value::text("3"),
            Value::text("1"),
            Value::text("2")
        ]))
    );
    assert_eq!(
        fields.get("pairs"),
        Some(&Value::Array(vec![
            Value::object([("k", Value::text("b"))]),
            Value::object([("k", Value::text("a"))]),
        ]))
    );
    let mut too_many = words;
    too_many.extend(["--tags".to_owned(), "4".to_owned()]);
    let refused = args::arguments(&schema, "test.every-type", &too_many)
        .expect_err("a fourth item is past `maxItems`");
    assert_eq!(
        refused.code(),
        nc_catalog::ErrorCode::ValidationArgumentsInvalid
    );
}
