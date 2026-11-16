//! The pages about what the catalog cannot see, from the tables they are
//! handed.
//!
//! `NOCTUA.md §20.5` publishes the CLI's reference and the keybindings beside
//! the registries, and part of each is not in `catalog/`: the CLI's grammar,
//! verbs and exit codes are `nc_cli`'s, and each channel's default Leader and
//! chord modifier are `nc_channel`'s. This crate links neither — both read
//! `nc-catalog`, which is what it generates — so `cargo xtask` builds a
//! [`Surfaces`] out of them and hands it to [`crate::compile`]. Every page
//! still comes out of the one emitter, and drift, staleness and the second
//! generation reach these as they reach every other page.
//!
//! **Nothing in a `Surfaces` is a command.** Every line about a command — its
//! texts and its arguments — is rendered from the catalog this compilation
//! reads, never from the one `cargo xtask` was built with, which is one build
//! behind whenever the catalog has just changed. What crosses is the CLI's
//! own tables, each channel's keys, and one function, [`Cli::described`],
//! which lays out what it is handed and reads no table of its own.
//!
//! **An incomplete surface is refused** (`surfaces-incomplete`): a CLI with no
//! name, no usage, no verb or no exit code, or no channel's Leader, would be
//! published as a page that says nothing, and `NOCTUA.md §19.3` refuses a gate
//! wired to nothing.

use core::fmt::Write as _;

use nc_schema::entry::Shown;
use nc_schema::keys::{Binding, Key, Keyboard, Leader, Modifier, SYSTEM_SHORTCUTS, Table};

use crate::Problem;
use crate::docs::{cell, fenced, header, shown_in, tags, words};
use crate::model::Catalog;

/// Where the surfaces are built, which a refusal of them names.
const BUILT_IN: &str = "xtask/src/build.rs";

/// What the catalog cannot see, as `cargo xtask` hands it to the compiler.
#[derive(Debug, Clone)]
pub struct Surfaces {
    /// The command line's own words.
    pub cli: Cli,
    /// Each channel's default keys, in the order their Leaders are settled.
    pub channels: Vec<ChannelKeys>,
}

/// The CLI's grammar, its verbs and its exit codes, and the layout its help
/// on a command is printed with.
#[derive(Debug, Clone)]
pub struct Cli {
    /// The name the CLI has in production, which the page writes it as.
    pub name: String,
    /// The grammar, as help prints it.
    pub usage: String,
    /// Each verb, with what it does.
    pub verbs: Vec<(String, String)>,
    /// Each exit code, with what a run that ends with it means.
    pub exit_codes: Vec<(u8, String)>,
    /// The one layout of help on a command, which help itself prints with.
    ///
    /// Handed the CLI's name, what a person is shown of the command in one
    /// locale and its compiled argument schema; it reads nothing else.
    pub described: fn(&str, &Shown<'_>, &nc_schema::Schema) -> String,
}

/// One channel's default keys.
#[derive(Debug, Clone, Copy)]
pub struct ChannelKeys {
    /// The channel, as its manifest writes it.
    pub channel: &'static str,
    /// Its default Leader.
    pub leader: Leader,
    /// Its chord modifier.
    pub chord: Modifier,
}

/// Every part of `surfaces` that holds nothing, each refused on its own.
pub(crate) fn incomplete(surfaces: &Surfaces) -> Vec<Problem> {
    let cli = &surfaces.cli;
    let mut found = Vec::new();
    for (empty, what) in [
        (cli.name.trim().is_empty(), "the CLI's name"),
        (cli.usage.trim().is_empty(), "the CLI's grammar"),
        (cli.verbs.is_empty(), "a verb"),
        (cli.exit_codes.is_empty(), "an exit code"),
        (surfaces.channels.is_empty(), "a channel's Leader"),
    ] {
        if empty {
            found.push(Problem::in_file(
                BUILT_IN,
                format!("the surfaces handed to the compiler hold no {what}"),
                "build them from the crates that hold them — `nc_cli` for the CLI's \
                 grammar, verbs and exit codes and `nc_channel` for each channel's keys — \
                 so the page they are published on says what the CLI and the channels do",
                "surfaces-incomplete",
            ));
        }
    }
    found
}

/// The CLI's reference: its grammar, its verbs, its exit codes, and what its
/// help says of each command the catalog declares, in every locale.
pub(crate) fn cli(catalog: &Catalog, surfaces: &Surfaces) -> String {
    let cli = &surfaces.cli;
    let mut out = header(
        "CLI",
        "apps/cli/src/args.rs, apps/cli/src/exit.rs, apps/cli/src/help.rs, \
         apps/cli/src/render.rs, catalog/commands.toml and catalog/i18n/",
        &format!(
            "What the command line reads and answers (`NOCTUA.md §20.2`): its grammar, its \
             verbs, its exit codes, and what its help says of every command the catalog \
             declares. The CLI is named for its channel (`§17.1`), and this page writes \
             production's name, `{}`.\n\n\
             **A verb is one word and every command's name is dotted**, so the two never \
             meet: a verb is what only the CLI does — help, and its own channel's daemon \
             started, stopped and asked how it is — and acts with no daemon, while a \
             command is dispatched by the daemon, decided and journaled. **Each command \
             below is what help prints of it**, in each language the environment can name, \
             laid out by the one function help lays it out with: the CLI's own words in \
             English, the command's texts from its entry. A plugin's commands are not on \
             this page: the daemon's registry holds them while the plugin is loaded, and \
             help lists them from there.",
            cell(&cli.name)
        ),
    );
    let _ = write!(out, "## Usage\n\n{}", fenced("text", &cli.usage));
    out.push_str("\n## Verbs\n\n| Verb | What it does |\n|---|---|\n");
    for (verb, what) in &cli.verbs {
        let _ = writeln!(out, "| `{}` | {} |", cell(verb), cell(what));
    }
    out.push_str("\n## Exit codes\n\n| Code | What a run ending with it means |\n|---|---|\n");
    for (code, meaning) in &cli.exit_codes {
        let _ = writeln!(out, "| {code} | {} |", cell(meaning));
    }
    out.push('\n');
    // **Sorted, like the generated crate**, as every page's sections are.
    let mut in_order: Vec<_> = catalog.commands.iter().collect();
    in_order.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in in_order {
        let _ = writeln!(out, "## `{}`\n", entry.name);
        // `load` compiled every schema it accepted, so one that does not
        // compile here is a catalog this function is never handed.
        let Ok(schema) = nc_schema::Schema::compile(&entry.args_schema) else {
            continue;
        };
        for tag in tags(catalog) {
            let Some(shown) = shown_in(catalog, entry, tag) else {
                continue;
            };
            let said = (cli.described)(&cli.name, &shown, &schema);
            let _ = writeln!(out, "In `{tag}`:\n\n{}", fenced("text", &said));
        }
    }
    out
}

/// The keys every channel is driven by, by default: the forms, the keys a
/// binding may name, each channel's Leader and chord modifier on each
/// keyboard, the catalog's bindings, and the chords each system keeps.
pub(crate) fn keybindings(catalog: &Catalog, surfaces: &Surfaces) -> String {
    let mut out = header(
        "Keybindings",
        "crates/nc-schema/src/keys.rs, crates/nc-channel/src/lib.rs, catalog/commands.toml \
         and catalog/i18n/",
        "The keys the interface is driven by (`NOCTUA.md §12.8`): the three forms a binding \
         takes, the keys one may name, each channel's default Leader and chord modifier on \
         each keyboard, every binding the catalog declares, and the chords each system \
         keeps for itself.\n\n\
         **These are the defaults, and no person's choice.** A person's own keys are a \
         file in their channel's configuration directory, `<config>/keybindings.json`, \
         judged against the other channels' Leaders; `noctua help keybindings` shows the \
         keys in force and why a choice was refused, with no daemon.",
    );
    out.push_str("## Forms\n\n| Form | Pressed as |\n|---|---|\n");
    for table in Table::ALL {
        let _ = writeln!(
            out,
            "| `{}<key>` | {} |",
            cell(table.opening()),
            pressed_as(table)
        );
    }
    out.push_str(
        "\nA key is a physical key, named as the W3C's `KeyboardEvent.code` names it, so \
         neither Shift nor a keyboard layout turns one key into another; `Chord` names a \
         role, so a binding holds whatever modifier its channel makes the chord modifier. \
         A binding has one spelling, and the build refuses any other.\n\n",
    );
    let _ = write!(
        out,
        "## Keys\n\n| Key | {} |\n|---|---|---|---|\n",
        on_each_keyboard(|keyboard| keyboard_named(keyboard).to_owned())
    );
    for key in Key::ALL {
        let _ = writeln!(
            out,
            "| `{}` | {} |",
            cell(key.code()),
            on_each_keyboard(|keyboard| key.written_for(keyboard).to_owned())
        );
    }
    let _ = write!(
        out,
        "\n## Leaders\n\n| Channel | {} |\n|---|---|---|---|\n",
        on_each_keyboard(|keyboard| keyboard_named(keyboard).to_owned())
    );
    for keys in &surfaces.channels {
        let _ = writeln!(
            out,
            "| `{}` | {} |",
            cell(keys.channel),
            on_each_keyboard(|keyboard| keys.leader.written_for(keyboard))
        );
    }
    let _ = write!(
        out,
        "\nEach Leader held with its channel's chord modifier, which a binding's `Chord` \
         names:\n\n| Channel | Chord modifier | {} |\n|---|---|---|---|---|\n",
        on_each_keyboard(|keyboard| keyboard_named(keyboard).to_owned())
    );
    for keys in &surfaces.channels {
        let _ = writeln!(
            out,
            "| `{}` | `{}` | {} |",
            cell(keys.channel),
            cell(keys.chord.word()),
            on_each_keyboard(|keyboard| keys.leader.with(keys.chord).written_for(keyboard))
        );
    }
    out.push('\n');
    bindings(catalog, &mut out);
    out.push_str(
        "## System shortcuts\n\nThe chords each system keeps for itself, each from the \
         source that lists it. A Leader or a binding pressing one is kept with a warning \
         that names the system, never refused.\n\n\
         | Keyboard | System | Chord | Source |\n|---|---|---|---|\n",
    );
    for kept in &SYSTEM_SHORTCUTS {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            keyboard_named(kept.keyboard),
            cell(kept.system),
            cell(&kept.chord.written_for(kept.keyboard)),
            cell(kept.source)
        );
    }
    out
}

/// The catalog's bindings, one row each in the order of their tables and
/// keys, with the display name of the command each runs in every locale.
///
/// **One sentence while there are none**, saying why: the catalog's bindings
/// arrive with the layer that binds them (`NOCTUA.md §15.2`).
fn bindings(catalog: &Catalog, out: &mut String) {
    out.push_str("## Bindings\n\n");
    let mut bound: Vec<(Binding, &crate::model::CommandEntry)> = Vec::new();
    for entry in &catalog.commands {
        for written in &entry.keybindings {
            // `load` refused every binding the grammar does not read, so one
            // that does not parse here is never handed to this function.
            if let Ok(binding) = Binding::parse(written) {
                bound.push((binding, entry));
            }
        }
    }
    if bound.is_empty() {
        out.push_str(
            "No command in the catalog declares a binding: they arrive with the layer that \
             binds them (`NOCTUA.md §15.2`).\n\n",
        );
        return;
    }
    bound.sort_by_key(|(binding, _)| *binding);
    let tags = tags(catalog);
    let mut heading = vec!["Binding".to_owned(), "Command".to_owned()];
    heading.extend(tags.iter().map(|tag| format!("Display name ({tag})")));
    let _ = writeln!(
        out,
        "| {} |\n|{}",
        heading.join(" | "),
        "---|".repeat(heading.len())
    );
    for (binding, entry) in bound {
        let mut cells = vec![
            format!("`{}`", cell(&binding.written())),
            format!("`{}`", cell(&entry.name)),
        ];
        for tag in &tags {
            cells.push(
                shown_in(catalog, entry, tag)
                    .map_or_else(|| "—".to_owned(), |shown| words(shown.display_name)),
            );
        }
        let _ = writeln!(out, "| {} |", cells.join(" | "));
    }
    out.push('\n');
}

/// What a binding of `table` is pressed as.
const fn pressed_as(table: Table) -> &'static str {
    match table {
        Table::Menu => "a key in the leader menu, after the Leader",
        Table::Held => "a key pressed with the Leader held",
        Table::HeldWithChord => "a key pressed with the Leader and the chord modifier held",
    }
}

/// How a page names `keyboard`.
const fn keyboard_named(keyboard: Keyboard) -> &'static str {
    match keyboard {
        Keyboard::Linux => "Linux",
        Keyboard::Windows => "Windows",
        Keyboard::Mac => "macOS",
    }
}

/// One cell for each keyboard, in the order of `Keyboard::ALL`, each
/// what `written` says for it.
fn on_each_keyboard(written: impl Fn(Keyboard) -> String) -> String {
    Keyboard::ALL
        .map(|keyboard| cell(&written(keyboard)))
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use nc_schema::Schema;
    use nc_schema::entry::Shown;
    use nc_schema::keys::{Chord, Key, Leader, Modifier, Modifiers};

    use super::{ChannelKeys, Cli, Surfaces, incomplete, on_each_keyboard};

    /// A layout that lays out nothing, which no test here reads.
    fn nothing(_: &str, _: &Shown<'_>, _: &Schema) -> String {
        String::new()
    }

    /// A surface holding every part.
    fn whole() -> Surfaces {
        Surfaces {
            cli: Cli {
                name: "noctua".to_owned(),
                usage: "Usage:\n".to_owned(),
                verbs: vec![("help".to_owned(), "say what a command takes".to_owned())],
                exit_codes: vec![(0, "the command ran".to_owned())],
                described: nothing,
            },
            channels: vec![ChannelKeys {
                channel: "production",
                leader: Leader::of(Chord::new(Modifiers::of(&[Modifier::Alt]), Key::Space))
                    .expect("a Leader holds a modifier"),
                chord: Modifier::Shift,
            }],
        }
    }

    /// A surface holding every part is complete, and each part left empty is
    /// refused alone, naming what it lacks.
    #[test]
    fn each_part_of_a_surface_left_empty_is_refused_by_name() {
        assert_eq!(incomplete(&whole()), Vec::<crate::Problem>::new());
        let mut all = whole();
        all.cli.name = "\t".to_owned();
        all.cli.usage.clear();
        all.cli.verbs.clear();
        all.cli.exit_codes.clear();
        all.channels.clear();
        let said: Vec<String> = incomplete(&all)
            .into_iter()
            .map(|problem| {
                assert_eq!(problem.check, "surfaces-incomplete");
                problem.what
            })
            .collect();
        assert_eq!(
            said,
            [
                "the CLI's name",
                "the CLI's grammar",
                "a verb",
                "an exit code",
                "a channel's Leader"
            ]
            .map(|what| format!("the surfaces handed to the compiler hold no {what}"))
        );
    }

    /// One cell for each keyboard, Linux, then Windows, then macOS, each
    /// escaped as a cell is.
    #[test]
    fn each_keyboard_has_its_cell_in_one_order_and_each_is_escaped() {
        assert_eq!(
            on_each_keyboard(|keyboard| format!("{keyboard:?}|")),
            "Linux\\| | Windows\\| | Mac\\|"
        );
    }
}
