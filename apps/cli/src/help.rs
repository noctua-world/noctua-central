//! What help says (`NOCTUA.md §20.2`, `§20.5`): the CLI's own grammar and
//! verbs, fixed and in English, shown whether a daemon answers or not; the
//! registries the catalog holds, which this binary carries and no daemon is
//! asked for; and every command the daemon's registry holds, with the
//! arguments its schema declares.
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
//! terminal acts on. **And every text of a registry the catalog holds**,
//! which the compiler judged, is escaped the same way: one rule for every
//! text help prints that it did not write. No glyph is printed: a terminal is
//! not promised the font that draws one.

use core::fmt::Write as _;

use nc_catalog::Locale;
use nc_schema::{Argument, Schema, Shape};

use crate::args::{CATEGORIES, EXIT_CODES, EXTERNAL_IDENTIFIERS, VERBS};
use crate::client::Listed;
use crate::render::escaped;

/// What a verb does, as help says it.
#[must_use]
pub const fn what_it_does(verb: &str) -> &'static str {
    match verb.as_bytes() {
        b"help" => {
            "say what this CLI and each command take; `help exit-codes` lists the exit codes, \
             `help categories` the categories and `help external-identifiers` the external \
             identifiers"
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
         <verb> | {EXIT_CODES} | {CATEGORIES} | {EXTERNAL_IDENTIFIERS}]\n\nOptions, before the command:\n  --json   write the answer \
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
        "\n`{cli} help <command>` says what a command takes, `{cli} help {EXIT_CODES}` what each \
         exit code means, `{cli} help {CATEGORIES}` what a command is listed under, and \
         `{cli} help {EXTERNAL_IDENTIFIERS}` what the platform knows of the things outside it.\n"
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

/// Help on the categories: every set the catalog holds, what defines it, and
/// every category in it, each text in `locale` and escaped.
///
/// **From the catalog this binary was built with**, which is the registry
/// itself — a category is the catalog's alone, and no daemon holds another —
/// so it is shown with no daemon, as the exit codes are.
#[must_use]
pub fn categories(locale: Locale) -> String {
    let said = |key: &str| nc_catalog::message(key, locale).unwrap_or_default();
    let width = nc_catalog::CATEGORIES
        .iter()
        .map(|category| escaped(category.name).chars().count())
        .max()
        .unwrap_or(0);
    let mut text = String::from("Categories, by set:\n");
    for set in nc_catalog::CATEGORY_SETS {
        text.push_str(&set_heading(&Set {
            name: set.name,
            display_name: said(set.title_key),
            line: said(set.description_key),
            defined_by: set.defined_by,
            version: set.version,
        }));
        for category in nc_catalog::CATEGORIES
            .iter()
            .filter(|category| category.set == set.name)
        {
            text.push_str(&category_lines(
                &Category {
                    name: category.name,
                    display_name: said(category.title_key),
                    standard_name: category.standard_name,
                    role: category.role,
                    line: said(category.description_key),
                },
                width,
            ));
        }
    }
    text
}

/// One set of categories, as help heads it.
struct Set<'a> {
    /// Its name.
    name: &'a str,
    /// Its display name, in the reader's language.
    display_name: &'a str,
    /// Its line description, in the reader's language.
    line: &'a str,
    /// What defines it.
    defined_by: &'a str,
    /// The version of the standard it was read from, if it has one.
    version: Option<&'a str>,
}

/// One set's heading, every text escaped.
///
/// Its name and display name, its line description beneath, then what
/// defines it and the version that was read.
fn set_heading(set: &Set<'_>) -> String {
    format!(
        "\n{} — {}\n  {}\n  Defined by {}{}\n\n",
        escaped(set.name),
        escaped(set.display_name),
        escaped(set.line),
        escaped(set.defined_by),
        set.version
            .map_or_else(String::new, |version| format!(" ({})", escaped(version)))
    )
}

/// One category, as help lists it.
struct Category<'a> {
    /// Its name.
    name: &'a str,
    /// Its display name, in the reader's language.
    display_name: &'a str,
    /// What its standard calls it.
    standard_name: Option<&'a str>,
    /// Where its standard ranks it.
    role: Option<&'a str>,
    /// Its line description, in the reader's language.
    line: &'a str,
}

/// One category's two lines: its name, its display name and what its
/// standard calls it, then its line description beneath — every text escaped.
fn category_lines(category: &Category<'_>, width: usize) -> String {
    let name = escaped(category.name);
    let standard = match (category.standard_name, category.role) {
        (Some(named), Some(role)) => format!(" ({}, {})", escaped(named), escaped(role)),
        (Some(named), None) => format!(" ({})", escaped(named)),
        (None, Some(role)) => format!(" ({})", escaped(role)),
        (None, None) => String::new(),
    };
    format!(
        "  {name:<width$}   {}{standard}\n  {:width$}     {}\n",
        escaped(category.display_name),
        "",
        escaped(category.line)
    )
}

/// Help on the external identifiers: every one the catalog holds, its texts
/// in `locale`, and what identifies it on each system, every text escaped.
///
/// **From the catalog this binary was built with**, for the reason the
/// categories are: the registry is the catalog's alone.
#[must_use]
pub fn external_identifiers(locale: Locale) -> String {
    let said = |key: &str| nc_catalog::message(key, locale).unwrap_or_default();
    let mut text = String::from("External identifiers:\n");
    for identifier in nc_catalog::EXTERNAL_IDENTIFIERS {
        text.push_str(&identifier_heading(&Identifier {
            name: identifier.name,
            display_name: said(identifier.title_key),
            line: said(identifier.description_key),
            categories: identifier.categories,
        }));
        // Walked as the generated crate hands it, one system's fields after
        // another: help names no field, so it prints every one the registry has.
        let identification = identifier.identification();
        for on_one_system in identification.chunk_by(|one, next| one.0 == next.0) {
            let Some((system, _, _)) = on_one_system.first() else {
                continue;
            };
            let fields: Vec<(&str, &[&str])> = on_one_system
                .iter()
                .map(|(_, field, values)| (*field, *values))
                .collect();
            text.push_str(&identification_lines(system, &fields));
        }
    }
    text
}

/// One external identifier, as help heads it.
struct Identifier<'a> {
    /// Its name.
    name: &'a str,
    /// Its display name, in the reader's language.
    display_name: &'a str,
    /// Its line description, in the reader's language.
    line: &'a str,
    /// The categories it is listed under.
    categories: &'a [&'a str],
}

/// One external identifier's heading, every text escaped.
///
/// Its name and display name, its line description beneath, and the
/// categories it is listed under where it names any.
fn identifier_heading(identifier: &Identifier<'_>) -> String {
    let mut text = format!(
        "\n{} — {}\n  {}\n",
        escaped(identifier.name),
        escaped(identifier.display_name),
        escaped(identifier.line)
    );
    if !identifier.categories.is_empty() {
        let named: Vec<String> = identifier
            .categories
            .iter()
            .map(|name| escaped(name))
            .collect();
        let _ = writeln!(text, "  Categories: {}", named.join(", "));
    }
    text
}

/// What identifies an entry on one system: the system, then one line for each
/// value, under its field, every value escaped.
///
/// **One value to a line**, so a value holding a comma or a space is still
/// one value to a reader; a field with no value has no line.
fn identification_lines(system: &str, fields: &[(&str, &[&str])]) -> String {
    let width = fields
        .iter()
        .map(|(field, _)| field.len())
        .max()
        .unwrap_or(0);
    let mut text = format!("  {system}\n");
    for (field, values) in fields {
        for value in *values {
            let _ = writeln!(text, "    {field:<width$}   {}", escaped(value));
        }
    }
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
    use nc_catalog::Locale;
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

    /// Help on the categories lists every category the catalog holds under its
    /// set.
    ///
    /// By its name, and its display name and line in the environment's
    /// language, with what its standard calls it.
    #[test]
    fn help_on_the_categories_lists_every_category_under_its_set_in_the_environments_language() {
        for locale in [Locale::En, Locale::PtBR] {
            let text = super::categories(locale);
            let said = |key: &str| nc_catalog::message(key, locale).expect("a message");
            let mut at = 0;
            for set in nc_catalog::CATEGORY_SETS {
                let heading = format!("\n{} — {}\n", set.name, said(set.title_key));
                let opened = text[at..].find(&heading).unwrap_or_else(|| {
                    panic!("`{}` is not listed after the set before it", set.name)
                }) + at;
                let closed = nc_catalog::CATEGORY_SETS
                    .iter()
                    .filter_map(|other| text.find(&format!("\n{} — ", other.name)))
                    .filter(|other| *other > opened)
                    .min()
                    .unwrap_or(text.len());
                let under = &text[opened..closed];
                assert!(under.contains(said(set.description_key)), "{under}");
                let mut listed = 0;
                for category in nc_catalog::CATEGORIES
                    .iter()
                    .filter(|category| category.set == set.name)
                {
                    let row = format!("  {} ", category.name);
                    let found = under
                        .find(&row)
                        .unwrap_or_else(|| panic!("`{}` is not under its set", category.name));
                    let lines: Vec<&str> = under[found..].lines().take(2).collect();
                    assert!(lines[0].contains(said(category.title_key)), "{lines:?}");
                    assert!(
                        lines[1].contains(said(category.description_key)),
                        "{lines:?}"
                    );
                    if let Some(named) = category.standard_name {
                        assert!(lines[0].contains(named), "{lines:?}");
                    }
                    if let Some(role) = category.role {
                        assert!(lines[0].contains(role), "{lines:?}");
                    }
                    listed += 1;
                }
                assert!(listed > 0, "the set `{}` lists no category", set.name);
                at = opened;
            }
            assert_eq!(
                text.lines()
                    .filter(|line| nc_catalog::CATEGORIES
                        .iter()
                        .any(|category| line.starts_with(&format!("  {} ", category.name))))
                    .count(),
                nc_catalog::CATEGORIES.len(),
                "a category is listed twice or not at all"
            );
        }
        assert_ne!(
            super::categories(Locale::En),
            super::categories(Locale::PtBR)
        );
    }

    /// Every text a category is listed with is escaped: a name, a display
    /// name, a standard name, a role and a line.
    #[test]
    fn every_text_a_category_is_listed_with_is_escaped() {
        let shown = super::category_lines(
            &super::Category {
                name: "set.\u{1b}[2Jname",
                display_name: "Shown\u{202e}",
                standard_name: Some("Std\u{7}"),
                role: Some("main\u{1b}"),
                line: "A line\u{2028}broken",
            },
            12,
        );
        for raw in ['\u{1b}', '\u{202e}', '\u{7}', '\u{2028}'] {
            assert!(!shown.contains(raw), "{shown:?}");
        }
        for written in ["\\u{1b}[2J", "\\u{202e}", "\\u{7}", "\\u{2028}"] {
            assert!(shown.contains(written), "{written} is not in {shown:?}");
        }
        assert_eq!(shown.lines().count(), 2, "{shown:?}");
    }

    /// Every text a set is headed with is escaped as well.
    ///
    /// Its name, display name, line, what defines it and its version, each in
    /// turn holding what moves a terminal: none comes out raw, and the heading
    /// keeps its lines.
    #[test]
    fn every_text_a_set_is_headed_with_is_escaped() {
        let hostile: &'static str = "a\u{1b}[2Jb\u{202e}c\u{7}d\u{2028}e";
        let lines = super::set_heading(&super::Set {
            name: "set",
            display_name: "Shown",
            line: "A line",
            defined_by: "A standard",
            version: Some("1.1"),
        })
        .lines()
        .count();
        for field in 0..5 {
            let pick = |at: usize, plain: &'static str| if at == field { hostile } else { plain };
            let shown = super::set_heading(&super::Set {
                name: pick(0, "set"),
                display_name: pick(1, "Shown"),
                line: pick(2, "A line"),
                defined_by: pick(3, "A standard"),
                version: Some(pick(4, "1.1")),
            });
            for raw in ['\u{1b}', '\u{202e}', '\u{7}', '\u{2028}'] {
                assert!(!shown.contains(raw), "field {field}: {shown:?}");
            }
            for written in ["\\u{1b}[2J", "\\u{202e}", "\\u{7}", "\\u{2028}"] {
                assert!(
                    shown.contains(written),
                    "field {field}: {written} is not in {shown:?}"
                );
            }
            assert_eq!(shown.lines().count(), lines, "field {field}: {shown:?}");
        }
    }

    /// What identifies one external identifier on each system, value by value:
    /// the system, the field and the value, in the order the generated crate
    /// hands them out.
    ///
    /// **Read from `ExternalIdentifier::identification`, never named here**,
    /// which the compiler writes from the one list it judges every field by:
    /// a field this test named by hand would be a field help and its test
    /// could forget together.
    fn identified_by(identifier: &nc_catalog::ExternalIdentifier) -> Vec<(&str, &str, &str)> {
        identifier
            .identification()
            .into_iter()
            .flat_map(|(system, field, values)| {
                values.iter().map(move |value| (system, field, *value))
            })
            .collect()
    }

    /// Help on the external identifiers lists each one with what identifies it.
    ///
    /// Its name, its display name and line in the environment's language, its
    /// categories, and every value of every field under its system, in the
    /// order the catalog holds them, and nothing else.
    #[test]
    fn help_on_the_external_identifiers_lists_each_with_what_identifies_it_on_each_system() {
        for locale in [Locale::En, Locale::PtBR] {
            let text = super::external_identifiers(locale);
            let said = |key: &str| nc_catalog::message(key, locale).expect("a message");
            let mut at = 0;
            for identifier in nc_catalog::EXTERNAL_IDENTIFIERS {
                let heading = format!("\n{} — {}\n", identifier.name, said(identifier.title_key));
                let opened = text[at..]
                    .find(&heading)
                    .unwrap_or_else(|| panic!("`{}` is not listed in order", identifier.name))
                    + at;
                let closed = text[opened + heading.len()..]
                    .find("\n\n")
                    .map_or(text.len(), |end| opened + heading.len() + end);
                let under = &text[opened + heading.len()..closed];
                let mut lines = under.lines();
                assert_eq!(
                    lines.next(),
                    Some(format!("  {}", said(identifier.description_key)).as_str()),
                    "{under}"
                );
                if !identifier.categories.is_empty() {
                    assert_eq!(
                        lines.next(),
                        Some(
                            format!("  Categories: {}", identifier.categories.join(", ")).as_str()
                        ),
                        "{under}"
                    );
                }
                let mut system = "";
                let mut listed = Vec::new();
                for line in lines {
                    if let Some(value_line) = line.strip_prefix("    ") {
                        let (field, value) = value_line
                            .split_once(' ')
                            .unwrap_or_else(|| panic!("`{line}` names no value"));
                        listed.push((system, field, value.trim_start()));
                    } else {
                        system = line.trim_start();
                    }
                }
                assert_eq!(listed, identified_by(identifier), "{}", identifier.name);
                assert!(
                    listed.len() > 1,
                    "`{}` is identified by nothing",
                    identifier.name
                );
                at = opened + heading.len();
            }
            assert!(
                nc_catalog::EXTERNAL_IDENTIFIERS.len() > 1,
                "the catalog holds no external identifiers to list"
            );
        }
        assert_ne!(
            super::external_identifiers(Locale::En),
            super::external_identifiers(Locale::PtBR)
        );
    }

    /// Every text an external identifier is headed with is escaped.
    ///
    /// Its name, display name, line and each category it is listed under, each
    /// in turn holding what moves a terminal: none comes out raw, and the
    /// heading keeps its lines.
    #[test]
    fn every_text_an_external_identifier_is_headed_with_is_escaped() {
        let hostile: &'static str = "a\u{1b}[2Jb\u{202e}c\u{7}d\u{2028}e";
        let lines = super::identifier_heading(&super::Identifier {
            name: "name",
            display_name: "Shown",
            line: "A line",
            categories: &["set.one", "set.two"],
        })
        .lines()
        .count();
        for field in 0..5 {
            let pick = |at: usize, plain: &'static str| if at == field { hostile } else { plain };
            let categories = [pick(3, "set.one"), pick(4, "set.two")];
            let shown = super::identifier_heading(&super::Identifier {
                name: pick(0, "name"),
                display_name: pick(1, "Shown"),
                line: pick(2, "A line"),
                categories: &categories,
            });
            for raw in ['\u{1b}', '\u{202e}', '\u{7}', '\u{2028}'] {
                assert!(!shown.contains(raw), "field {field}: {shown:?}");
            }
            for written in ["\\u{1b}[2J", "\\u{202e}", "\\u{7}", "\\u{2028}"] {
                assert!(
                    shown.contains(written),
                    "field {field}: {written} is not in {shown:?}"
                );
            }
            assert_eq!(shown.lines().count(), lines, "field {field}: {shown:?}");
        }
    }

    /// Every value an external identifier is identified by is escaped.
    ///
    /// Each value of each field in turn holds what moves a terminal: none
    /// comes out raw, every value keeps its one line, and a field with no
    /// value has none.
    #[test]
    fn every_value_an_external_identifier_is_identified_by_is_escaped() {
        let hostile: &'static str = "a\u{1b}[2Jb\u{202e}c\u{7}d\u{2028}e";
        for at in 0..3 {
            let pick = |here: usize, plain: &'static str| if here == at { hostile } else { plain };
            let paths = [pick(1, "/usr/bin/one"), pick(2, "/usr/bin/two")];
            let shown = super::identification_lines(
                "Linux",
                &[
                    ("executables", &[pick(0, "one")]),
                    ("desktop_ids", &[]),
                    ("paths", &paths),
                ],
            );
            for raw in ['\u{1b}', '\u{202e}', '\u{7}', '\u{2028}'] {
                assert!(!shown.contains(raw), "value {at}: {shown:?}");
            }
            for written in ["\\u{1b}[2J", "\\u{202e}", "\\u{7}", "\\u{2028}"] {
                assert!(
                    shown.contains(written),
                    "value {at}: {written} is not in {shown:?}"
                );
            }
            assert_eq!(shown.lines().count(), 4, "value {at}: {shown:?}");
            assert!(!shown.contains("desktop_ids"), "{shown:?}");
        }
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
