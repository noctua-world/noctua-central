//! What help says (`NOCTUA.md §20.2`, `§20.5`): the CLI's own grammar and
//! verbs, fixed and in English, shown whether a daemon answers or not, and
//! every command the daemon's registry holds, with the arguments its schema
//! declares.
//!
//! **The registry is the one list**: help names every command the daemon
//! would run and none it would not, and a command's arguments are read from
//! the same schema its command line is read against
//! (`nc_schema::Schema::arguments`). **Every command is shown by the texts
//! the daemon sent** — its entry's display name, line, paragraph and tags, in
//! the language the registry was asked in, the environment's, and its
//! categories by name — a command the catalog writes as much as a plugin's,
//! and never by a text this binary compiled in. **Every text from the daemon
//! is escaped** ([`crate::render::escaped`]) — a name, a display name, a line,
//! a paragraph, a tag, a category, an argument's title and description, an
//! option, an entity type — so nothing the daemon relays prints anything a
//! terminal acts on. No glyph is printed: a terminal is not promised the font
//! that draws one.

use core::fmt::Write as _;

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
        b"start" => "start this channel's daemon, unless it runs, and wait for it to answer",
        b"stop" => {
            "stop this channel's daemon, and keep anything that restores it from starting it \
             again until `start`"
        }
        b"restart" => "stop this channel's daemon, then start it",
        b"status" => "say how this channel's daemon is, or why it does not answer",
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

/// `texts`, each escaped, joined by a comma, or `none` when there are none:
/// a tag holds no comma, which the entry judge refuses.
fn listed_as_words(texts: &[String]) -> String {
    if texts.is_empty() {
        return "none".to_owned();
    }
    texts
        .iter()
        .map(|text| escaped(text))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The grammar and the verbs, which help shows whether a daemon answers or not.
fn grammar(cli: &str) -> String {
    let mut text = usage(cli);
    text.push('\n');
    text.push_str(&verbs());
    text
}

/// Help on everything: the grammar, the verbs, and every command the registry
/// holds, by its name, its display name and its line.
#[must_use]
pub fn everything(cli: &str, commands: &[Listed]) -> String {
    let mut text = grammar(cli);
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
        let _ = writeln!(
            text,
            "  {name:<width$}   {} — {}",
            escaped(&listed.display_name),
            escaped(&listed.line)
        );
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

/// Help on everything when the daemon's registry could not be read: the
/// grammar and the verbs, and where the commands would have been.
#[must_use]
pub fn unlisted(cli: &str) -> String {
    let mut text = grammar(cli);
    let _ = write!(
        text,
        "\nCommands:\n  none shown: they are the daemon's registry, which could not be read; the \
         error says why, and `{cli} help` lists them once the daemon answers\n"
    );
    text
}

/// Help on one command: its name and display name, its line, its paragraph,
/// its categories and its tags, and each argument its schema declares.
#[must_use]
pub fn command(cli: &str, listed: &Listed, schema: &Schema) -> String {
    let name = escaped(&listed.name);
    let mut text = format!(
        "{name} — {}\n{}\n\n{}\n\nCategories: {}\nTags: {}\n",
        escaped(&listed.display_name),
        escaped(&listed.line),
        escaped(&listed.paragraph),
        listed_as_words(&listed.categories),
        listed_as_words(&listed.tags)
    );
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
    use nc_schema::Schema;

    use crate::args::VERBS;
    use crate::client::Listed;

    /// An entry the daemon sent, its texts written from `display_name`.
    fn listed(name: &str, display_name: &str, args_schema: &str) -> Listed {
        Listed {
            name: name.to_owned(),
            display_name: display_name.to_owned(),
            line: format!("{display_name}, in a line"),
            paragraph: format!("{display_name}, in a paragraph."),
            tags: vec!["one".to_owned(), "two".to_owned()],
            categories: Vec::new(),
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
        assert_eq!(super::what_it_does("verify"), "");
    }

    /// Help on everything lists every command the registry holds, one line
    /// each — its name, then its display name and its line as the daemon sent
    /// them — with the grammar and the verbs.
    #[test]
    fn help_on_everything_lists_every_command_by_its_name_display_name_and_line() {
        let commands = [
            listed("journal.verify", "Verificar histórico de atividades", EMPTY),
            listed("reference.note.get", "Ler uma nota", EMPTY),
        ];
        let text = super::everything("noctua-staging", &commands);
        for line in [
            "  journal.verify       Verificar histórico de atividades — Verificar histórico de \
             atividades, in a line\n",
            "  reference.note.get   Ler uma nota — Ler uma nota, in a line\n",
        ] {
            assert!(text.contains(line), "`{line}` is not in:\n{text}");
        }
        assert!(text.contains("noctua-staging [--json] <command>"), "{text}");
        assert!(text.contains(&super::verbs()), "{text}");
        assert!(super::everything("noctua", &[]).contains("holds no command"));
    }

    /// Every text the daemon supplied is escaped, the catalog's commands' as
    /// much as a plugin's: a name, a display name, a line, a paragraph, a tag
    /// and a category, in the list and in help on the command.
    ///
    /// **A catalog command's included**, which help once took from this
    /// binary's own catalog and printed raw: the texts now cross the endpoint,
    /// and a daemon of another build, or one answering for another, is what
    /// sent them.
    #[test]
    fn every_text_the_daemon_supplied_is_escaped_the_catalogs_as_much_as_a_plugins() {
        const MARKED: &str = "\u{1b}[2J\u{202e}";
        for name in ["kernel.undo", "reference.note.get"] {
            let sent = Listed {
                name: format!("{name}{MARKED}"),
                display_name: format!("Undo{MARKED}"),
                line: format!("Take it back{MARKED}"),
                paragraph: format!("All of it{MARKED}\nreally"),
                tags: vec![format!("undo{MARKED}")],
                categories: vec![format!("noctua.history{MARKED}")],
                args_schema: EMPTY.to_owned(),
            };
            let schema = Schema::compile(EMPTY).expect("inside the subset");
            for text in [
                super::everything("noctua", std::slice::from_ref(&sent)),
                super::command("noctua", &sent, &schema),
            ] {
                assert!(
                    !text.contains('\u{1b}') && !text.contains('\u{202e}'),
                    "{name}: {text}"
                );
                for escaped in [
                    format!("{name}\\u{{1b}}[2J\\u{{202e}}"),
                    "Undo\\u{1b}[2J\\u{202e}".to_owned(),
                    "Take it back\\u{1b}[2J\\u{202e}".to_owned(),
                ] {
                    assert!(text.contains(&escaped), "`{escaped}` is not in:\n{text}");
                }
            }
            let text = super::command("noctua", &sent, &schema);
            for escaped in [
                "All of it\\u{1b}[2J\\u{202e}\\u{a}really",
                "Tags: undo\\u{1b}[2J\\u{202e}\n",
                "Categories: noctua.history\\u{1b}[2J\\u{202e}\n",
            ] {
                assert!(text.contains(escaped), "`{escaped}` is not in:\n{text}");
            }
        }
    }

    /// Help on a command shows its display name and line, its paragraph, its
    /// categories and its tags, in that order, before its usage.
    #[test]
    fn help_on_a_command_shows_its_paragraph_tags_and_categories() {
        let sent = Listed {
            categories: vec!["noctua.history".to_owned(), "noctua.files".to_owned()],
            ..listed("kernel.undo", "Desfazer", EMPTY)
        };
        let text = super::command(
            "noctua",
            &sent,
            &Schema::compile(EMPTY).expect("inside the subset"),
        );
        assert!(
            text.starts_with(
                "kernel.undo — Desfazer\nDesfazer, in a line\n\nDesfazer, in a paragraph.\n\n\
                 Categories: noctua.history, noctua.files\nTags: one, two\n\nUsage:\n"
            ),
            "{text}"
        );
        let unfiled = super::command(
            "noctua",
            &listed("kernel.undo", "Desfazer", EMPTY),
            &Schema::compile(EMPTY).expect("inside the subset"),
        );
        assert!(unfiled.contains("\nCategories: none\n"), "{unfiled}");
    }

    /// Help with no registry to read still shows the grammar and the verbs,
    /// and says where the commands would have been.
    #[test]
    fn help_without_a_registry_is_the_grammar_and_the_verbs() {
        let text = super::unlisted("noctua-staging");
        assert!(text.contains(&super::usage("noctua-staging")), "{text}");
        assert!(text.contains(&super::verbs()), "{text}");
        assert!(text.contains("none shown"), "{text}");
        assert!(text.contains("`noctua-staging help` lists them"), "{text}");
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
        let turned = super::command(
            "noctua",
            &listed("reference.\u{1b}[2J\u{202e}eton.put", "Put", EMPTY),
            &Schema::compile(EMPTY).expect("inside the subset"),
        );
        for expected in [
            "reference.\\u{1b}[2J\\u{202e}eton.put — Put\n",
            "  noctua reference.\\u{1b}[2J\\u{202e}eton.put [arguments]\n",
        ] {
            assert!(
                turned.contains(expected),
                "`{expected}` is not in:\n{turned}"
            );
        }
        assert!(
            !turned.contains('\u{1b}') && !turned.contains('\u{202e}'),
            "{turned}"
        );
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

    /// A command the catalog writes is shown by the texts the daemon sent,
    /// and by none this binary compiled in: the registry was asked in the
    /// environment's language, and what it answered is what help says.
    #[test]
    fn a_catalog_command_is_shown_by_the_texts_the_daemon_sent() {
        let schema = Schema::compile(EMPTY).expect("inside the subset");
        let text = super::command(
            "noctua",
            &listed("kernel.undo", "Undone, as the daemon says", EMPTY),
            &schema,
        );
        assert!(
            text.starts_with("kernel.undo — Undone, as the daemon says\n"),
            "{text}"
        );
        for locale in nc_catalog::Locale::ALL {
            let compiled = |key| nc_catalog::message(key, *locale).expect("a message");
            assert!(
                !text.contains(&format!("— {}\n", compiled("command.kernel.undo.title")))
                    && !text.contains(compiled("command.kernel.undo.description")),
                "{locale:?}: {text}"
            );
        }
        assert!(text.contains("Arguments:\n  none\n"), "{text}");
    }
}
