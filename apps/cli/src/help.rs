//! What help says (`NOCTUA.md §20.2`, `§20.5`): the CLI's own grammar and
//! verbs, fixed and in English, and every command the daemon's registry holds,
//! with the arguments its schema declares.
//!
//! **The registry is the one list**: help names every command the daemon
//! would run and none it would not, and a command's arguments are read from
//! the same schema its command line is read against
//! (`nc_schema::Schema::arguments`). A command the catalog declares is shown
//! by the catalog's title and description in the environment's language; a
//! plugin's, by the label it declared. **Every text from the daemon is
//! escaped** ([`crate::render::escaped`]) — a name, a label, a title, a
//! description, an option, an entity type — so a plugin's schema prints
//! nothing a terminal acts on.

use core::fmt::Write as _;

use nc_catalog::Locale;
use nc_schema::{Argument, Schema, Shape};

use crate::args::{EXIT_CODES, VERBS};
use crate::client::Listed;
use crate::render::escaped;

/// What a verb does, as help says it.
#[must_use]
pub const fn what_it_does(verb: &str) -> &'static str {
    match verb.as_bytes() {
        b"help" => {
            "say what this CLI and each command take; `help exit-codes` lists the exit codes"
        }
        _ => "",
    }
}

/// The grammar, for `cli`: the name this install's CLI has.
#[must_use]
pub fn usage(cli: &str) -> String {
    format!(
        "Usage:\n  {cli} [--json] <command> [arguments]\n  {cli} [--json] help [<command> | \
         <verb> | {EXIT_CODES}]\n\nOptions, before the command:\n  --json   write the answer \
         as one JSON document on standard output\n  --help   ask for help\n\nArguments follow \
         a command's name: `--<name> <value>`, `--<name>=<value>`, `--<boolean>` or \
         `--<boolean>=false`, a list's flag once for each item, or `--` and one JSON object.\n"
    )
}

/// The verbs, each with what it does.
#[must_use]
pub fn verbs() -> String {
    let width = VERBS.iter().map(|verb| verb.len()).max().unwrap_or(0);
    let mut text = String::from("Verbs:\n");
    for verb in VERBS {
        let _ = writeln!(text, "  {verb:<width$}   {}", what_it_does(verb));
    }
    text
}

/// What a person is shown for `listed`: the catalog's title in `locale` for
/// a command the catalog declares, and the plugin's label, escaped, for one it
/// does not.
#[must_use]
pub fn title(listed: &Listed, locale: Locale) -> String {
    nc_catalog::command(&listed.name)
        .and_then(|command| nc_catalog::message(command.title_key, locale))
        .map_or_else(|| escaped(&listed.title), str::to_owned)
}

/// The catalog's description of the command `name` in `locale`, for one the
/// catalog declares.
fn description(name: &str, locale: Locale) -> Option<String> {
    nc_catalog::command(name)
        .and_then(|command| nc_catalog::message(command.description_key, locale))
        .map(str::to_owned)
}

/// Help on everything: the grammar, the verbs, and every command the registry
/// holds, by its name and its title.
#[must_use]
pub fn everything(cli: &str, commands: &[Listed], locale: Locale) -> String {
    let mut text = usage(cli);
    text.push('\n');
    text.push_str(&verbs());
    text.push_str("\nCommands:\n");
    let names: Vec<String> = commands
        .iter()
        .map(|listed| escaped(&listed.name))
        .collect();
    let width = names
        .iter()
        .map(|name| name.chars().count())
        .max()
        .unwrap_or(0);
    for (name, listed) in names.iter().zip(commands) {
        let _ = writeln!(text, "  {name:<width$}   {}", title(listed, locale));
    }
    if commands.is_empty() {
        text.push_str("  none: the registry holds no command\n");
    }
    let _ = write!(
        text,
        "\n`{cli} help <command>` says what a command takes, and `{cli} help {EXIT_CODES}` what \
         each exit code means.\n"
    );
    text
}

/// Help on one command: its name, its title and description, and each
/// argument its schema declares.
#[must_use]
pub fn command(cli: &str, listed: &Listed, schema: &Schema, locale: Locale) -> String {
    let name = escaped(&listed.name);
    let mut text = format!("{name} — {}\n", title(listed, locale));
    if let Some(description) = description(&listed.name, locale) {
        let _ = writeln!(text, "{description}");
    }
    let _ = write!(
        text,
        "\nUsage:\n  {cli} {name} [arguments]\n  {cli} {name} -- '<one JSON object>'\n\nArguments:\n"
    );
    let arguments = schema.arguments();
    if arguments.is_empty() {
        text.push_str("  none\n");
    }
    for argument in &arguments {
        text.push_str(&argument_help(argument));
    }
    text
}

/// One argument, as help shows it: its flag, what it takes, whether it is
/// required, and its title and description beneath.
fn argument_help(argument: &Argument<'_>) -> String {
    let mut line = format!("  --{}", escaped(argument.name));
    match &argument.shape {
        Shape::Boolean => {
            line.push_str("  (a boolean: true when given alone, `=false` says false)");
        }
        Shape::Array {
            items,
            min_items,
            max_items,
        } => {
            let _ = write!(
                line,
                " <{}>  (repeated, once for each item{})",
                value(items),
                bounds(min_items.map(i128::from), max_items.map(i128::from), "item")
            );
        }
        shape => {
            let _ = write!(line, " <{}>", value(shape));
        }
    }
    if argument.required {
        line.push_str("  required");
    }
    line.push('\n');
    match (argument.title, argument.description) {
        (Some(title), Some(description)) => {
            let _ = writeln!(line, "      {}: {}", escaped(title), escaped(description));
        }
        (Some(said), None) | (None, Some(said)) => {
            let _ = writeln!(line, "      {}", escaped(said));
        }
        (None, None) => {}
    }
    line
}

/// What a value of `shape` is, as help names it.
fn value(shape: &Shape<'_>) -> String {
    match shape {
        Shape::String {
            min_length,
            max_length,
            options,
            entity,
        } => {
            if let Some(options) = options {
                let listed: Vec<String> = options.iter().map(|option| escaped(option)).collect();
                return format!("one of: {}", listed.join(", "));
            }
            let what = entity.map_or_else(
                || "text".to_owned(),
                |entity| format!("the id of a {}", escaped(entity)),
            );
            format!(
                "{what}{}",
                bounds(
                    min_length.map(i128::from),
                    max_length.map(i128::from),
                    "character"
                )
            )
        }
        Shape::Integer { minimum, maximum } => format!(
            "whole number{}",
            bounds(minimum.map(i128::from), maximum.map(i128::from), "")
        ),
        Shape::Boolean => "true or false".to_owned(),
        Shape::Array { .. } => "a JSON list".to_owned(),
        Shape::Object(_) => "a JSON object".to_owned(),
    }
}

/// The bounds `least` and `most` of a count of `unit`s, or of a number when
/// no unit is named, as help writes them after what they bound.
fn bounds(least: Option<i128>, most: Option<i128>, unit: &str) -> String {
    let plural = |count: i128| {
        if unit.is_empty() {
            String::new()
        } else if count == 1 {
            format!(" {unit}")
        } else {
            format!(" {unit}s")
        }
    };
    match (least, most) {
        (None, None) => String::new(),
        (Some(least), Some(most)) => format!(", from {least} to {most}{}", plural(most)),
        (Some(least), None) => format!(", at least {least}{}", plural(least)),
        (None, Some(most)) => format!(", at most {most}{}", plural(most)),
    }
}

#[cfg(test)]
mod tests {
    use nc_catalog::Locale;
    use nc_schema::Schema;

    use crate::args::VERBS;
    use crate::client::Listed;

    fn listed(name: &str, title: &str, args_schema: &str) -> Listed {
        Listed {
            name: name.to_owned(),
            title: title.to_owned(),
            args_schema: args_schema.to_owned(),
        }
    }

    const EMPTY: &str = r#"{"type":"object","additionalProperties":false}"#;

    /// Every verb has a line in help saying what it does.
    #[test]
    fn every_verb_says_what_it_does() {
        let verbs = super::verbs();
        for verb in VERBS {
            let said = super::what_it_does(verb);
            assert!(!said.is_empty(), "{verb}");
            assert!(verbs.contains(&format!("  {verb}")), "{verbs}");
            assert!(verbs.contains(said), "{verbs}");
        }
        assert_eq!(super::what_it_does("start"), "");
    }

    /// Help on everything lists every command the registry holds, by its name
    /// and its title — the catalog's in the environment's language, a
    /// plugin's label escaped — with the grammar and the verbs.
    #[test]
    fn help_on_everything_lists_every_command_by_its_name_and_title() {
        let commands = [
            listed("journal.verify", "command.journal.verify.title", EMPTY),
            listed("reference.note.get", "Read a note\u{1b}[2J", EMPTY),
        ];
        for locale in [Locale::En, Locale::PtBR] {
            let text = super::everything("noctua-staging", &commands, locale);
            let verify = nc_catalog::message("command.journal.verify.title", locale)
                .expect("the catalog's title");
            assert!(text.contains("journal.verify"), "{text}");
            assert!(text.contains(verify), "{text}");
            assert!(text.contains("reference.note.get"), "{text}");
            assert!(text.contains("Read a note\\u{1b}[2J"), "{text}");
            assert!(!text.contains('\u{1b}'), "{text}");
            assert!(text.contains("noctua-staging [--json] <command>"), "{text}");
            assert!(text.contains(&super::verbs()), "{text}");
        }
        assert!(super::everything("noctua", &[], Locale::En).contains("holds no command"));
    }

    /// Help on a command shows its title and description and every argument
    /// its schema declares, each with what it takes, its bounds, whether it is
    /// required and its annotations — every one of them escaped.
    #[test]
    fn help_on_a_command_shows_every_argument_from_its_schema() {
        let schema_text = r#"{"type":"object","additionalProperties":false,"properties":{
            "note":{"type":"string","x-entity":"Content","maxLength":128,"title":"Note","description":"which note"},
            "mode":{"type":"string","enum":["fast","slow"],"description":"how"},
            "count":{"type":"integer","minimum":1,"maximum":9},
            "from":{"type":"integer","minimum":0},
            "dry_run":{"type":"boolean"},
            "tags":{"type":"array","items":{"type":"string","minLength":1},"maxItems":3},
            "inner":{"type":"object","additionalProperties":false}},
            "required":["note"]}"#;
        let schema = Schema::compile(schema_text).expect("inside the subset");
        let text = super::command(
            "noctua",
            &listed("reference.note.put", "Put\u{202e} a note", schema_text),
            &schema,
            Locale::En,
        );
        for expected in [
            "reference.note.put — Put\\u{202e} a note",
            "  --note <the id of a Content, at most 128 characters>  required",
            "      Note: which note",
            "  --mode <one of: fast, slow>",
            "      how",
            "  --count <whole number, from 1 to 9>",
            "  --from <whole number, at least 0>",
            "  --dry_run  (a boolean",
            "  --tags <text, at least 1 character>  (repeated, once for each item, at most 3 items)",
            "  --inner <a JSON object>",
            "noctua reference.note.put -- '<one JSON object>'",
        ] {
            assert!(text.contains(expected), "`{expected}` is not in:\n{text}");
        }
        assert!(!text.contains('\u{202e}'), "{text}");
        let order: Vec<usize> = [
            "--note",
            "--mode",
            "--count",
            "--from",
            "--dry_run",
            "--tags",
            "--inner",
        ]
        .iter()
        .map(|flag| text.find(flag).expect("listed"))
        .collect();
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{text}");
    }

    /// A catalog command is shown by the catalog's title and description in
    /// the environment's language, whatever label the registry carries.
    #[test]
    fn a_catalog_command_is_shown_in_the_environments_language() {
        let schema = Schema::compile(EMPTY).expect("inside the subset");
        for locale in [Locale::En, Locale::PtBR] {
            let text = super::command(
                "noctua",
                &listed("kernel.undo", "command.kernel.undo.title", EMPTY),
                &schema,
                locale,
            );
            for key in [
                "command.kernel.undo.title",
                "command.kernel.undo.description",
            ] {
                let said = nc_catalog::message(key, locale).expect("a message");
                assert!(text.contains(said), "{locale:?}: {text}");
            }
            assert!(text.contains("Arguments:\n  none\n"), "{text}");
        }
    }
}
