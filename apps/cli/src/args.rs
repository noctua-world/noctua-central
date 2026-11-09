//! What a command line asks (`NOCTUA.md §20.2`), read in two steps.
//!
//! ```text
//! noctua [--json] [--help] <verb | command.name> [arguments]
//! noctua help [<command.name> | <verb> | exit-codes]
//! noctua <command.name> --<argument> <value> | --<argument>=<value> | --<boolean> …
//! noctua <command.name> -- '<one JSON object>'
//! ```
//!
//! 1. [`read`] reads the options before the name, and the name: `--json`
//!    renders the answer as one canonical document, `--help` asks for help,
//!    and the first other word is a verb — one word, [`VERBS`] — or a
//!    command's registered name, which is always dotted, so the two never
//!    meet. Nothing here asks the daemon anything.
//! 2. [`arguments`] reads what follows a command's name against that
//!    command's schema, as the daemon's registry hands it: **a flag's name is
//!    the property's, verbatim**, a value is read as the property's type —
//!    text as written, a whole number, `--flag` or `--flag=false` for a
//!    boolean, JSON for an object — an array's flag is repeated once per item
//!    in order, and every other flag given twice is refused. `--` instead of
//!    flags takes the arguments as one JSON object. `--help` alone after a
//!    name is help for that command, unless its schema declares a `help`
//!    property, whose flag it then is.
//!
//! **The arguments read are held to the command's own schema before anything
//! is sent**, with `nc_schema::Schema::validate`, the call the kernel makes —
//! so a mistake is said here, under the code the kernel would refuse it with
//! (`NC-VAL-0002`), and the kernel, which validates again, stays the
//! authority. Every word repeated in a refusal is shown escaped: a command
//! line is whatever was typed, and a refusal is printed to a terminal.

use std::collections::BTreeMap;

use nc_canon::Value;
use nc_catalog::ErrorCode;
use nc_schema::{Argument, Schema, Shape};

/// The verbs the CLI answers itself, each one word.
///
/// **Undotted, and a registered command's name is always dotted**
/// (`nc_schema::command_name_is_well_formed`), so a verb never shadows a
/// command and a command never takes a verb's place. `start`, `stop`,
/// `restart` and `status` join `help` once the CLI controls its daemon's
/// process.
pub const VERBS: [&str; 1] = ["help"];

/// The help topic that is the exit-code table, which needs no daemon.
pub const EXIT_CODES: &str = "exit-codes";

/// What a command line asks, before any daemon is asked anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Whether the answer is rendered as one canonical JSON document.
    pub json: bool,
    /// What is asked.
    pub asked: Asked,
}

/// What a command line asks of the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Asked {
    /// Help, on a topic.
    Help(Topic),
    /// A registered command, and the words after its name, which are read
    /// against its schema once the registry has handed it.
    Command {
        /// The command's registered name.
        name: String,
        /// Every word after it.
        words: Vec<String>,
    },
}

/// What help is asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Topic {
    /// The CLI and every command the registry holds.
    Everything,
    /// The verbs.
    Verbs,
    /// The exit codes.
    ExitCodes,
    /// One command, by its registered name.
    Command(String),
}

/// What the words after a command's name are, read against its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Read {
    /// The command's arguments, as one object the schema accepts.
    Arguments(Value),
    /// Help for the command.
    Help,
}

/// A command line refused before anything was sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unread {
    /// A command line the CLI does not read; the text says what, and how to
    /// fix it.
    Line(String),
    /// Arguments read, which the command's schema refuses; the text says
    /// which.
    Arguments(String),
}

impl Unread {
    /// The catalog code this maps to: the CLI's own for a command line it does
    /// not read, and the kernel's for arguments the schema refuses.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        match self {
            Self::Line(_) => ErrorCode::CliCommandLineUnreadable,
            Self::Arguments(_) => ErrorCode::ValidationArgumentsInvalid,
        }
    }

    /// What was refused, and how to fix it.
    #[must_use]
    pub fn detail(&self) -> &str {
        let (Self::Line(detail) | Self::Arguments(detail)) = self;
        detail
    }
}

/// A command line the CLI does not read, said as `detail`.
fn unread(detail: impl Into<String>) -> Unread {
    Unread::Line(detail.into())
}

/// A word from a command line, fit to repeat in a refusal: every character a
/// reader sees nothing of, and every one that ends a line, escaped.
#[must_use]
pub fn shown(word: &str) -> String {
    crate::render::escaped(word)
}

/// What `words` — a command line without the program's name — ask.
///
/// # Errors
///
/// [`Unread::Line`] for an option this build does not know or one given
/// twice, a word that is neither a verb nor a command's name, and help asked
/// about more than one thing.
pub fn read(words: &[String]) -> Result<Line, Unread> {
    let mut json = false;
    let mut help = false;
    let mut at = 0;
    while let Some(word) = words.get(at).filter(|word| word.starts_with('-')) {
        let given = match word.as_str() {
            "--json" => &mut json,
            "--help" => &mut help,
            other => {
                return Err(unread(format!(
                    "`{}` is not an option this CLI reads before a command; the options are \
                     `--json` and `--help`",
                    shown(other)
                )));
            }
        };
        if *given {
            return Err(unread(format!("`{word}` is given twice")));
        }
        *given = true;
        at += 1;
    }
    let rest = &words[at..];
    let asked = if help {
        Asked::Help(topic(rest)?)
    } else {
        match rest.split_first() {
            None => Asked::Help(Topic::Everything),
            Some((word, after)) if word == "help" => Asked::Help(topic(after)?),
            Some((word, after)) => Asked::Command {
                name: named(word)?,
                words: after.to_vec(),
            },
        }
    };
    Ok(Line { json, asked })
}

/// The topic help is asked about in `words`: nothing, one verb, one command's
/// name, or the exit codes.
fn topic(words: &[String]) -> Result<Topic, Unread> {
    match words {
        [] => Ok(Topic::Everything),
        [word] if word == EXIT_CODES => Ok(Topic::ExitCodes),
        [word] if VERBS.contains(&word.as_str()) => Ok(Topic::Verbs),
        [word] => named(word).map(Topic::Command),
        [_, extra, ..] => Err(unread(format!(
            "help is asked about one thing at a time, and `{}` is a second",
            shown(extra)
        ))),
    }
}

/// `word` as a command's registered name: dotted, as every one is.
fn named(word: &str) -> Result<String, Unread> {
    if nc_schema::command_name_is_well_formed(word) {
        Ok(word.to_owned())
    } else {
        Err(unread(format!(
            "`{}` is neither a verb this CLI answers ({}) nor a command's name, which is dotted \
             lower-case words such as `journal.verify`",
            shown(word),
            VERBS.join(", ")
        )))
    }
}

/// The arguments `words` give the command `name`, read against `schema` and
/// held to it.
///
/// # Errors
///
/// [`Unread::Line`] for words this grammar does not read as the schema's
/// flags, or for text after `--` that is not one JSON object; and
/// [`Unread::Arguments`] for arguments read that the schema refuses.
pub fn arguments(schema: &Schema, name: &str, words: &[String]) -> Result<Read, Unread> {
    let declared = schema.arguments();
    if matches!(words, [only] if only == "--help")
        && !declared.iter().any(|argument| argument.name == "help")
    {
        return Ok(Read::Help);
    }
    let value = match words {
        [first, rest @ ..] if first == "--" => match rest {
            [text] => object(text)?,
            _ => {
                return Err(unread(
                    "`--` takes exactly one JSON object after it, with no flag beside it",
                ));
            }
        },
        _ => flags(&declared, words)?,
    };
    held_to(schema, name, value).map(Read::Arguments)
}

/// The object `words` give, flag by flag, against the arguments `declared`.
fn flags(declared: &[Argument<'_>], words: &[String]) -> Result<Value, Unread> {
    let mut fields: BTreeMap<String, Value> = BTreeMap::new();
    let mut words = words.iter();
    while let Some(word) = words.next() {
        let Some(flag) = word.strip_prefix("--").filter(|flag| !flag.is_empty()) else {
            return Err(unread(format!(
                "`{}` is not a flag: after a command's name each argument is `--<name> <value>`, \
                 `--<name>=<value>` or, for a boolean, `--<name>`; or `--` and one JSON object",
                shown(word)
            )));
        };
        let (name, inline) = match flag.split_once('=') {
            Some((name, value)) => (name, Some(value)),
            None => (flag, None),
        };
        let Some(argument) = declared.iter().find(|argument| argument.name == name) else {
            return Err(unread(format!(
                "`--{}` is not an argument of this command; it takes {}",
                shown(name),
                the_flags(declared)
            )));
        };
        match &argument.shape {
            Shape::Boolean => {
                let flag = match inline {
                    None | Some("true") => true,
                    Some("false") => false,
                    Some(other) => {
                        return Err(unread(format!(
                            "`--{name}` is a boolean: given alone it is true, and `=true` or \
                             `=false` say which; `{}` is neither",
                            shown(other)
                        )));
                    }
                };
                once(&mut fields, name, Value::Bool(flag))?;
            }
            Shape::Array { items, .. } => {
                let text = value_of(name, inline, &mut words)?;
                let item = scalar(name, items, text)?;
                match fields
                    .entry(name.to_owned())
                    .or_insert_with(|| Value::Array(Vec::new()))
                {
                    Value::Array(held) => held.push(item),
                    _ => return Err(unread(format!("`--{name}` is given twice"))),
                }
            }
            shape => {
                let text = value_of(name, inline, &mut words)?;
                once(&mut fields, name, scalar(name, shape, text)?)?;
            }
        }
    }
    Ok(Value::Object(fields))
}

/// The arguments `declared`, as a refusal lists them.
fn the_flags(declared: &[Argument<'_>]) -> String {
    if declared.is_empty() {
        return "no argument".to_owned();
    }
    declared
        .iter()
        .map(|argument| format!("`--{}`", shown(argument.name)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The value of the flag `name`: written after its `=`, or the next word.
fn value_of<'a>(
    name: &str,
    inline: Option<&'a str>,
    words: &mut impl Iterator<Item = &'a String>,
) -> Result<&'a str, Unread> {
    match inline {
        Some(value) => Ok(value),
        None => words.next().map(String::as_str).ok_or_else(|| {
            unread(format!(
                "`--{name}` takes a value, as the next word or after `=`"
            ))
        }),
    }
}

/// `value` set as the flag `name`, which only an array's flag may be twice.
fn once(fields: &mut BTreeMap<String, Value>, name: &str, value: Value) -> Result<(), Unread> {
    if fields.insert(name.to_owned(), value).is_some() {
        return Err(unread(format!(
            "`--{name}` is given twice, and only a list's flag is repeated, once for each item"
        )));
    }
    Ok(())
}

/// `text`, the value of the flag `name`, read as `shape` says.
fn scalar(name: &str, shape: &Shape<'_>, text: &str) -> Result<Value, Unread> {
    match shape {
        Shape::String { .. } => Ok(Value::text(text)),
        Shape::Integer { .. } => whole(text).map(Value::Integer).ok_or_else(|| {
            unread(format!(
                "`--{name}` takes a whole number, and `{}` is not one",
                shown(text)
            ))
        }),
        Shape::Boolean => match text {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(unread(format!(
                "`--{name}` takes `true` or `false`, and `{}` is neither",
                shown(other)
            ))),
        },
        Shape::Object(_) => object(text).map_err(|refused| {
            unread(format!(
                "`--{name}` takes one JSON object: {}",
                refused.detail()
            ))
        }),
        Shape::Array { .. } => match parsed(text) {
            Ok(list @ Value::Array(_)) => Ok(list),
            Ok(_) => Err(unread(format!("`--{name}` takes one JSON list"))),
            Err(said) => Err(unread(format!("`--{name}` takes one JSON list: {said}"))),
        },
    }
}

/// `text` as a whole number: an optional `-` and decimal digits, and nothing
/// else, within 64 signed bits.
fn whole(text: &str) -> Option<i64> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

/// `text` read as JSON by the one reader of it, or what it said, escaped.
fn parsed(text: &str) -> Result<Value, String> {
    nc_canon::parse(text).map_err(|refused| shown(&refused.to_string()))
}

/// `text` as one JSON object.
fn object(text: &str) -> Result<Value, Unread> {
    match parsed(text) {
        Ok(object @ Value::Object(_)) => Ok(object),
        Ok(_) => Err(unread(
            "the arguments are one JSON object, and this is not an object",
        )),
        Err(said) => Err(unread(format!("the arguments are not JSON: {said}"))),
    }
}

/// `value`, once `schema` accepts it as the arguments of `name`.
///
/// **The call the kernel makes, on the text the kernel reads**: the canonical
/// form, parsed as `serde_json` reads it, so what is refused here is what the
/// kernel would refuse.
fn held_to(schema: &Schema, name: &str, value: Value) -> Result<Value, Unread> {
    let text = value.to_canonical();
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|refused| {
        unread(format!(
            "the arguments of `{name}` did not read back as JSON: {}",
            shown(&refused.to_string())
        ))
    })?;
    match schema.validate(&json) {
        Ok(()) => Ok(value),
        Err(violations) => {
            let more = violations.len().saturating_sub(1);
            let first = violations
                .first()
                .map_or_else(String::new, ToString::to_string);
            Err(Unread::Arguments(format!(
                "the arguments of `{name}` do not fit its schema: {}{}",
                shown(&first),
                if more == 0 {
                    String::new()
                } else {
                    format!(", and {more} more")
                }
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use nc_canon::Value;
    use nc_catalog::ErrorCode;
    use nc_schema::Schema;

    use super::{Asked, Line, Read, Topic, Unread, VERBS};

    fn words(line: &[&str]) -> Vec<String> {
        line.iter().map(|word| (*word).to_owned()).collect()
    }

    fn line(line: &[&str]) -> Line {
        super::read(&words(line)).expect("the line is read")
    }

    fn refused(line: &[&str]) -> Unread {
        super::read(&words(line)).expect_err("the line is refused")
    }

    fn schema(text: &str) -> Schema {
        Schema::compile(text).expect("inside the subset")
    }

    /// Every type, a repeated list, an object and a property named `help`.
    const EVERY_TYPE: &str = r#"{"type":"object","additionalProperties":false,"properties":{
        "text":{"type":"string","maxLength":8},
        "count":{"type":"integer","minimum":-5,"maximum":500},
        "flag":{"type":"boolean"},
        "tags":{"type":"array","items":{"type":"string"},"maxItems":3},
        "numbers":{"type":"array","items":{"type":"integer"}},
        "inner":{"type":"object","additionalProperties":false,"properties":{"x":{"type":"integer"}}}},
        "required":["text"]}"#;

    fn arguments(line: &[&str]) -> Result<Read, Unread> {
        super::arguments(&schema(EVERY_TYPE), "test.command", &words(line))
    }

    fn read_as(line: &[&str]) -> Value {
        match arguments(line) {
            Ok(Read::Arguments(value)) => value,
            other => panic!("{line:?} was read as {other:?}"),
        }
    }

    fn refusal(line: &[&str]) -> Unread {
        arguments(line).expect_err("the arguments are refused")
    }

    /// The options before a command, the command's name and the words after
    /// it are read apart, and the name is taken whole.
    #[test]
    fn a_command_line_is_its_options_its_name_and_the_words_after_it() {
        assert_eq!(
            line(&["--json", "journal.verify", "--from", "3"]),
            Line {
                json: true,
                asked: Asked::Command {
                    name: "journal.verify".to_owned(),
                    words: words(&["--from", "3"])
                }
            }
        );
        assert_eq!(
            line(&["journal.verify", "--json"]),
            Line {
                json: false,
                asked: Asked::Command {
                    name: "journal.verify".to_owned(),
                    words: words(&["--json"])
                }
            },
            "an option after the name belongs to the command's schema"
        );
    }

    /// Help is asked by its verb or its option, about everything, the verbs,
    /// the exit codes or one command, and nothing alone asks for everything.
    #[test]
    fn help_is_asked_about_one_thing_at_a_time() {
        for (asked, topic) in [
            (&[][..], Topic::Everything),
            (&["help"], Topic::Everything),
            (&["--help"], Topic::Everything),
            (&["help", "help"], Topic::Verbs),
            (&["help", "exit-codes"], Topic::ExitCodes),
            (&["--help", "exit-codes"], Topic::ExitCodes),
            (
                &["help", "kernel.undo"],
                Topic::Command("kernel.undo".to_owned()),
            ),
            (
                &["--json", "--help", "kernel.undo"],
                Topic::Command("kernel.undo".to_owned()),
            ),
        ] {
            assert_eq!(line(asked).asked, Asked::Help(topic), "{asked:?}");
        }
        assert!(line(&["--json", "help"]).json);
        for refusal in [
            &["help", "kernel.undo", "kernel.redo"][..],
            &["help", "Kernel.Undo"],
            &["help", "undo"],
        ] {
            assert_eq!(
                refused(refusal).code(),
                ErrorCode::CliCommandLineUnreadable,
                "{refusal:?}"
            );
        }
    }

    /// An option this build does not read, one given twice, and a word that is
    /// neither a verb nor a dotted name are refused, each with the CLI's code.
    #[test]
    fn an_unknown_option_a_repeated_one_and_an_unknown_word_are_refused() {
        for line in [
            &["--verbose", "journal.verify"][..],
            &["-j", "journal.verify"],
            &["--json", "--json", "journal.verify"],
            &["--help", "--help"],
            &["verify"],
            &["start"],
            &["journal"],
            &["journal.Verify"],
        ] {
            let refusal = refused(line);
            assert_eq!(
                refusal.code(),
                ErrorCode::CliCommandLineUnreadable,
                "{line:?}"
            );
            assert!(matches!(refusal, Unread::Line(_)), "{line:?}");
        }
        let said = refused(&["verify"]);
        assert!(said.detail().contains("help"), "{said:?}");
    }

    /// Every verb is one undotted word, so no registered name is one: a name is
    /// always dotted, and every catalog command's name is read as a command.
    #[test]
    fn the_verbs_and_the_registered_names_never_meet() {
        for verb in VERBS {
            assert!(!verb.contains('.'), "{verb}");
            assert!(!nc_schema::command_name_is_well_formed(verb), "{verb}");
            assert_ne!(line(&[verb]).asked, Asked::Help(Topic::ExitCodes));
        }
        assert!(!nc_schema::command_name_is_well_formed(super::EXIT_CODES));
        for command in nc_catalog::COMMANDS {
            assert!(!VERBS.contains(&command.name), "{}", command.name);
            assert_eq!(
                line(&[command.name]).asked,
                Asked::Command {
                    name: command.name.to_owned(),
                    words: Vec::new()
                }
            );
        }
        for command in nc_plugin_reference::behaviour::commands() {
            assert!(!VERBS.contains(&command.name.as_str()), "{}", command.name);
            assert!(matches!(
                line(&[&command.name]).asked,
                Asked::Command { .. }
            ));
        }
    }

    /// Each type is read as its type: text as written, a whole number, a
    /// boolean alone or said, a list one flag per item in order, and an
    /// object from JSON — and a value after `=` is the same value.
    #[test]
    fn each_type_is_read_as_its_type() {
        let read = read_as(&[
            "--text",
            "a b",
            "--count=-4",
            "--flag",
            "--tags",
            "x",
            "--tags=y",
            "--numbers",
            "7",
            "--inner",
            r#"{"x":2}"#,
        ]);
        assert_eq!(
            read,
            Value::object([
                ("text", Value::text("a b")),
                ("count", Value::Integer(-4)),
                ("flag", Value::Bool(true)),
                (
                    "tags",
                    Value::Array(vec![Value::text("x"), Value::text("y")])
                ),
                ("numbers", Value::Array(vec![Value::Integer(7)])),
                ("inner", Value::object([("x", Value::Integer(2))])),
            ])
        );
        assert_eq!(
            read_as(&["--text=--flag", "--flag=false"]),
            Value::object([
                ("text", Value::text("--flag")),
                ("flag", Value::Bool(false))
            ]),
            "a value after `=` is taken whole, and a boolean said false is false"
        );
        assert_eq!(
            read_as(&["--text", "--count"]),
            Value::object([("text", Value::text("--count"))]),
            "the word after a flag that takes a value is its value"
        );
        assert_eq!(
            read_as(&["--text="]),
            Value::object([("text", Value::text(""))])
        );
    }

    /// `--` takes the arguments as one JSON object, and nothing beside it.
    #[test]
    fn two_dashes_take_one_json_object_and_nothing_beside_it() {
        assert_eq!(
            read_as(&["--", r#"{"text":"t","tags":["a","b"]}"#]),
            Value::object([
                ("text", Value::text("t")),
                (
                    "tags",
                    Value::Array(vec![Value::text("a"), Value::text("b")])
                ),
            ])
        );
        for line in [
            &["--"][..],
            &["--", "{}", "{}"],
            &["--", "[]"],
            &["--", "{"],
            &["--text", "t", "--", "{}"],
        ] {
            assert_eq!(
                refusal(line).code(),
                ErrorCode::CliCommandLineUnreadable,
                "{line:?}"
            );
        }
    }

    /// What the schema does not read is refused before anything is sent.
    ///
    /// A flag it does not declare, a value of another type, a flag with no
    /// value, a scalar's flag given twice and a word that is no flag.
    #[test]
    fn what_the_schema_does_not_read_is_refused_by_the_cli() {
        for line in [
            &["--text", "t", "--colour", "red"][..],
            &["--text", "t", "--count", "four"],
            &["--text", "t", "--count", "+4"],
            &["--text", "t", "--count", "4.0"],
            &["--text", "t", "--count", "99999999999999999999"],
            &["--text", "t", "--count"],
            &["--text", "t", "--flag=yes"],
            &["--text", "t", "--text", "u"],
            &["--text", "t", "--flag", "--flag"],
            &["--text", "t", "loose"],
            &["--text", "t", "--"],
            &["--text", "t", "--inner", "[1]"],
            &["--text", "t", "--numbers", "x"],
        ] {
            let refusal = refusal(line);
            assert_eq!(
                refusal.code(),
                ErrorCode::CliCommandLineUnreadable,
                "{line:?}"
            );
        }
        let said = refusal(&["--colour", "red"]);
        assert!(
            said.detail().contains("`--text`") && said.detail().contains("`--inner`"),
            "a flag the command does not take is refused naming those it does: {said:?}"
        );
    }

    /// Arguments read and then refused by the schema carry the code the kernel
    /// refuses them with, saying which argument.
    #[test]
    fn arguments_the_schema_refuses_are_refused_with_the_kernels_code() {
        for line in [
            &[][..],
            &["--count", "4"],
            &["--text", "far too long"],
            &["--text", "t", "--count", "501"],
            &[
                "--text", "t", "--tags", "a", "--tags", "b", "--tags", "c", "--tags", "d",
            ],
        ] {
            let refusal = refusal(line);
            assert_eq!(
                refusal.code(),
                ErrorCode::ValidationArgumentsInvalid,
                "{line:?}"
            );
            assert!(matches!(refusal, Unread::Arguments(_)), "{line:?}");
        }
        assert!(refusal(&[]).detail().contains("text"), "{:?}", refusal(&[]));
    }

    /// `--help` alone after a name is help for the command, unless the schema
    /// declares a `help` property, whose flag it then is.
    #[test]
    fn help_after_a_name_is_help_unless_the_schema_takes_it() {
        assert_eq!(arguments(&["--help"]), Ok(Read::Help));
        let takes_help = schema(
            r#"{"type":"object","additionalProperties":false,"properties":{"help":{"type":"boolean"}}}"#,
        );
        assert_eq!(
            super::arguments(&takes_help, "test.command", &words(&["--help"])),
            Ok(Read::Arguments(Value::object([(
                "help",
                Value::Bool(true)
            )])))
        );
        assert!(
            arguments(&["--text", "t", "--help"]).is_err(),
            "`--help` beside other flags is a flag like any other"
        );
    }

    /// A word repeated in a refusal is escaped: a command line is whatever was
    /// typed, and the refusal is printed to a terminal.
    #[test]
    fn a_word_repeated_in_a_refusal_is_escaped() {
        let said = refused(&["\u{1b}[2J"]);
        assert!(!said.detail().contains('\u{1b}'), "{said:?}");
        let said = refusal(&["--\u{202e}x", "1"]);
        assert!(!said.detail().contains('\u{202e}'), "{said:?}");
        let said = refusal(&["--text", "t", "--count", "\u{7}"]);
        assert!(!said.detail().contains('\u{7}'), "{said:?}");
    }
}
