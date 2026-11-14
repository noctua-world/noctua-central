//! What help says (`NOCTUA.md §20.2`, `§20.5`): the CLI's own grammar and
//! verbs, fixed and in English, shown whether a daemon answers or not; the
//! registries the catalog holds, which this binary carries and no daemon is
//! asked for; and every command the daemon's registry holds, with the
//! arguments its schema declares.
//!
//! **The registry is the one list**: help names every command the daemon
//! would run and none it would not, and a command's arguments are read from
//! the same schema its command line is read against
//! (`nc_schema::Schema::arguments`). A command the catalog declares is shown
//! by the catalog's title and description in the environment's language; a
//! plugin's, by the label it declared. **Every text from the daemon is
//! escaped** ([`crate::render::escaped`]) — a name, a label, a title, a
//! description, an option, an entity type — so a plugin's schema prints
//! nothing a terminal acts on. **And every text of a registry the catalog
//! holds**, which the compiler judged, is escaped the same way: one rule for
//! every text help prints that it did not write.

use core::fmt::Write as _;

use nc_catalog::Locale;
use nc_schema::{Argument, Schema, Shape};

use crate::args::{CATEGORIES, EXIT_CODES, VERBS};
use crate::client::Listed;
use crate::render::escaped;

/// What a verb does, as help says it.
#[must_use]
pub const fn what_it_does(verb: &str) -> &'static str {
    match verb.as_bytes() {
        b"help" => {
            "say what this CLI and each command take; `help exit-codes` lists the exit codes, \
             and `help categories` the categories"
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
         <verb> | {EXIT_CODES} | {CATEGORIES}]\n\nOptions, before the command:\n  --json   write the answer \
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

/// The grammar and the verbs, which help shows whether a daemon answers or not.
fn grammar(cli: &str) -> String {
    let mut text = usage(cli);
    text.push('\n');
    text.push_str(&verbs());
    text
}

/// Help on everything: the grammar, the verbs, and every command the registry
/// holds, by its name and its title.
#[must_use]
pub fn everything(cli: &str, commands: &[Listed], locale: Locale) -> String {
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
        let _ = writeln!(text, "  {name:<width$}   {}", title(listed, locale));
    }
    if commands.is_empty() {
        text.push_str("  none: the registry holds no command\n");
    }
    let _ = write!(
        text,
        "\n`{cli} help <command>` says what a command takes, `{cli} help {EXIT_CODES}` what each \
         exit code means, and `{cli} help {CATEGORIES}` what a command is listed under.\n"
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
        assert_eq!(super::what_it_does("verify"), "");
    }

    /// Help on everything lists every command the registry holds, by its name
    /// and its title — the catalog's in the environment's language, a
    /// plugin's name and label escaped — with the grammar and the verbs.
    #[test]
    fn help_on_everything_lists_every_command_by_its_name_and_title() {
        let commands = [
            listed("journal.verify", "command.journal.verify.title", EMPTY),
            listed("reference.note.get", "Read a note\u{1b}[2J", EMPTY),
            listed("reference.\u{1b}[2Jwipe.do", "Wipe", EMPTY),
            listed("reference.\u{202e}eton.get", "Turned", EMPTY),
        ];
        for locale in [Locale::En, Locale::PtBR] {
            let text = super::everything("noctua-staging", &commands, locale);
            let verify = nc_catalog::message("command.journal.verify.title", locale)
                .expect("the catalog's title");
            assert!(text.contains("journal.verify"), "{text}");
            assert!(text.contains(verify), "{text}");
            assert!(text.contains("reference.note.get"), "{text}");
            assert!(text.contains("Read a note\\u{1b}[2J"), "{text}");
            assert!(text.contains("  reference.\\u{1b}[2Jwipe.do "), "{text}");
            assert!(text.contains("  reference.\\u{202e}eton.get "), "{text}");
            assert!(!text.contains('\u{1b}'), "{text}");
            assert!(!text.contains('\u{202e}'), "{text}");
            assert!(text.contains("noctua-staging [--json] <command>"), "{text}");
            assert!(text.contains(&super::verbs()), "{text}");
        }
        assert!(super::everything("noctua", &[], Locale::En).contains("holds no command"));
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
        let turned = super::command(
            "noctua",
            &listed("reference.\u{1b}[2J\u{202e}eton.put", "Put", EMPTY),
            &Schema::compile(EMPTY).expect("inside the subset"),
            Locale::En,
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
