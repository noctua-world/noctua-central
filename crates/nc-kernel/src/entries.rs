//! A plugin's entries: what it says of each command it declares, in every
//! locale (`NOCTUA.md §20.4`), read from its own directory.
//!
//! **Beside `plugin.toml`, laid out as the catalog is.** A command table names
//! each command by the name it has *inside* the plugin's namespace — `note.get`
//! is registered as `<namespace>.note.get`, so one set of files serves every
//! namespace an install gives the plugin — with the categories and the glyph it
//! is shown with, and the texts it leaves the same in every locale with the
//! reason; one file per locale holds the texts:
//!
//! ```text
//! commands.toml      [[command]] name = "note.get"
//!                                categories = [...]          optional
//!                                glyph = "U+F0E2"            optional
//!                                same_in_both = { display_name = "<why>" }   optional
//! i18n/<tag>.toml    [command."note.get"]
//!                    display_name = "…"   line = "…"   paragraph = "…"   tags = ["…"]
//! ```
//!
//! **Read for their shape here, judged at `activate`.** [`PluginEntries::parse`]
//! refuses what is not the shape — TOML that does not parse, a field it does
//! not know, a value of another kind, a name no namespace can precede, a row
//! written twice, a locale's texts for a command the table does not list, a
//! locale handed twice or not at all, a file past [`PluginEntries::most_bytes`]
//! — each with `[NC-PLUG-0029]`, and every text left out is left for the judge.
//! Whether each command the plugin declares has its entry, and whether that
//! entry's texts are what every entry's must be, is `crate::registry`'s at
//! `activate`, through `nc_schema::entry::judge`: the rules the catalog
//! compiler holds every command of the catalog to, written once.
//!
//! # What this answers (`§2.4`)
//!
//! A plugin's texts are input from outside the platform, and nothing of them
//! is read past a bound: a file larger than every text its commands may hold,
//! with room for the TOML around them, is refused before it is parsed, and a
//! field the kernel does not know is a refusal rather than a text nothing
//! judges. A name no namespace can precede is refused here and never repeated.
//! **What it does not answer**: whether the texts are true, and everything the
//! judge holds them to, which `crate::registry` asks of it.

use core::fmt;
use std::collections::BTreeMap;

use nc_catalog::{ErrorCode, KERNEL, Locale};
use nc_schema::descriptor;
use nc_schema::entry::{self, EntryProblem, InLocale, Rules, Text, Texts};
use serde::Deserialize;

use crate::registry::{CommandEntry, EntryTexts, QUOTED, quoted_up_to};

/// What a plugin says of the commands it declares, by the names they have
/// inside its namespace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PluginEntries {
    rows: BTreeMap<String, Row>,
}

/// One command's entry as its plugin wrote it, before it is judged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Row {
    categories: Vec<String>,
    glyph: Option<String>,
    same_in_both: Vec<(Text, String)>,
    /// Its texts in each locale, in `Locale::ALL`'s order.
    texts: Vec<Said>,
}

/// One locale's texts of one entry, each `None` where it was not written.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    display_name: Option<String>,
    line: Option<String>,
    paragraph: Option<String>,
    tags: Option<Vec<String>>,
}

/// One way a plugin's entries are refused for their shape.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EntriesProblem {
    /// A file is not TOML of its shape; carries which file and what the parser
    /// said.
    NotEntries {
        /// The command table, or one locale's texts.
        file: String,
        /// What the parser said.
        said: String,
    },
    /// A file past [`PluginEntries::most_bytes`].
    TooLarge {
        /// The command table, or one locale's texts.
        file: String,
        /// How many bytes it holds.
        bytes: usize,
    },
    /// A name no namespace can precede to make a command's name; carries it,
    /// cut, escaped and between backticks.
    NameMalformed(String),
    /// A row of the command table written twice.
    RowTwice(String),
    /// One locale's texts for a command the table does not list.
    Unlisted {
        /// The locale.
        locale: String,
        /// The name, cut, escaped and between backticks.
        name: String,
    },
    /// A locale whose texts were not handed in.
    LocaleNotHanded(String),
    /// A locale whose texts were handed in twice.
    LocaleHandedTwice(String),
}

impl fmt::Display for EntriesProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEntries { file, said } => write!(formatter, "{file} is not entries: {said}"),
            Self::TooLarge { file, bytes } => write!(
                formatter,
                "{file} holds {bytes} bytes, past the {} a plugin's entries may hold",
                PluginEntries::most_bytes()
            ),
            Self::NameMalformed(name) => write!(
                formatter,
                "the command table names {name}, which is not dotted lower-case words that a \
                 namespace can precede"
            ),
            Self::RowTwice(name) => {
                write!(formatter, "the command table lists `{name}` twice")
            }
            Self::Unlisted { locale, name } => write!(
                formatter,
                "the `{locale}` texts describe {name}, which the command table does not list"
            ),
            Self::LocaleNotHanded(locale) => {
                write!(formatter, "there are no `{locale}` texts")
            }
            Self::LocaleHandedTwice(locale) => {
                write!(formatter, "the `{locale}` texts were handed twice")
            }
        }
    }
}

/// A plugin's entries the kernel refuses for their shape, with every problem
/// it found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntriesError {
    problems: Vec<EntriesProblem>,
}

impl EntriesError {
    /// Every problem found, the command table's first and then each locale's.
    #[must_use]
    pub fn problems(&self) -> &[EntriesProblem] {
        &self.problems
    }

    /// The code every refused entry carries.
    #[must_use]
    pub const fn code(&self) -> ErrorCode {
        ErrorCode::PluginEntriesInvalid
    }
}

impl fmt::Display for EntriesError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "[{}] the plugin's entries were refused:",
            ErrorCode::PluginEntriesInvalid
        )?;
        for problem in &self.problems {
            write!(formatter, " {problem};")?;
        }
        Ok(())
    }
}

impl std::error::Error for EntriesError {}

/// The command table as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenTable {
    #[serde(default)]
    command: Vec<WrittenRow>,
}

/// One row of the command table as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenRow {
    name: String,
    #[serde(default)]
    categories: Vec<String>,
    glyph: Option<String>,
    #[serde(default)]
    same_in_both: WrittenExemptions,
}

/// The texts a row leaves the same in every locale, each with its reason.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenExemptions {
    display_name: Option<String>,
    line: Option<String>,
    paragraph: Option<String>,
}

/// One locale's file as written.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct WrittenLocale {
    #[serde(default)]
    command: BTreeMap<String, Said>,
}

impl PluginEntries {
    /// Reads a plugin's entries: its command table, and each locale's texts.
    ///
    /// # Errors
    ///
    /// Every [`EntriesProblem`] of the shape found — the command table's, then
    /// each locale's in the order handed — as one [`EntriesError`] carrying
    /// `[NC-PLUG-0029]`; a file that does not parse is the one problem the
    /// parser stopped at in it.
    pub fn parse(commands: &str, texts: &[(Locale, &str)]) -> Result<Self, EntriesError> {
        let mut problems = Vec::new();
        let mut rows: BTreeMap<String, Row> = BTreeMap::new();
        let table = read::<WrittenTable>("the command table", commands, &mut problems);
        // **A table that was not read lists nothing a locale can be held to**,
        // so its texts are read for their own shape and no row of theirs is
        // called unlisted: that would be the table's one problem said again
        // for every command.
        let listed = table.is_some();
        if let Some(table) = table {
            for row in table.command {
                let name = row.name.clone();
                if !a_name_inside_a_namespace(&name) {
                    problems.push(EntriesProblem::NameMalformed(quoted(&name)));
                    continue;
                }
                if rows.contains_key(&name) {
                    problems.push(EntriesProblem::RowTwice(name));
                    continue;
                }
                rows.insert(name, Row::written(row));
            }
        }
        for locale in Locale::ALL {
            match texts.iter().filter(|(handed, _)| handed == locale).count() {
                0 => problems.push(EntriesProblem::LocaleNotHanded(locale.tag().to_owned())),
                1 => {}
                _ => problems.push(EntriesProblem::LocaleHandedTwice(locale.tag().to_owned())),
            }
        }
        for (locale, text) in texts {
            let file = format!("the `{}` texts", locale.tag());
            let Some(written) = read::<WrittenLocale>(&file, text, &mut problems) else {
                continue;
            };
            for (name, said) in written.command {
                match rows.get_mut(&name) {
                    Some(row) => row.texts[locale.index()] = said,
                    None if listed => problems.push(EntriesProblem::Unlisted {
                        locale: locale.tag().to_owned(),
                        name: quoted(&name),
                    }),
                    None => {}
                }
            }
        }
        if problems.is_empty() {
            Ok(Self { rows })
        } else {
            Err(EntriesError { problems })
        }
    }

    /// Every name the command table lists, inside the namespace, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.rows.keys().map(String::as_str)
    }

    /// The most bytes one file of a plugin's entries may hold.
    ///
    /// **Every text its commands may hold, twice**: `commands_per_plugin`
    /// entries of `entry_bytes` each, and as much again for the TOML written
    /// around them — keys, quotes, and a character escaped as two.
    #[must_use]
    pub fn most_bytes() -> usize {
        usize::try_from(KERNEL.commands_per_plugin)
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::try_from(KERNEL.entry_bytes).unwrap_or(usize::MAX))
            .saturating_mul(2)
    }

    /// The row the command table lists as `name`, inside the namespace.
    pub(crate) fn row(&self, name: &str) -> Option<&Row> {
        self.rows.get(name)
    }
}

impl Row {
    /// The row as written, its texts not yet handed in.
    fn written(row: WrittenRow) -> Self {
        let exemptions = [
            (Text::DisplayName, row.same_in_both.display_name),
            (Text::Line, row.same_in_both.line),
            (Text::Paragraph, row.same_in_both.paragraph),
        ];
        Self {
            categories: row.categories,
            glyph: row.glyph,
            same_in_both: exemptions
                .into_iter()
                .filter_map(|(text, reason)| reason.map(|reason| (text, reason)))
                .collect(),
            texts: vec![Said::default(); Locale::ALL.len()],
        }
    }

    /// The entry this row is, once the one judge holds it to `rules`.
    ///
    /// # Errors
    ///
    /// Every [`EntryProblem`] the judge found, in the order the judge
    /// found them.
    pub(crate) fn judged(&self, rules: &Rules<'_>) -> Result<CommandEntry, Vec<EntryProblem>> {
        let locales: Vec<InLocale<'_>> = Locale::ALL
            .iter()
            .zip(&self.texts)
            .map(|(locale, said)| InLocale {
                locale: locale.tag(),
                texts: Texts {
                    display_name: said.display_name.as_deref(),
                    line: said.line.as_deref(),
                    paragraph: said.paragraph.as_deref(),
                    tags: said.tags.as_deref(),
                },
            })
            .collect();
        let exemptions: Vec<(Text, &str)> = self
            .same_in_both
            .iter()
            .map(|(text, reason)| (*text, reason.as_str()))
            .collect();
        entry::judge(
            &entry::Entry {
                locales: &locales,
                categories: &self.categories,
                glyph: self.glyph.as_deref(),
                same_in_both: &exemptions,
            },
            rules,
        )?;
        Ok(CommandEntry::new(
            self.texts
                .iter()
                .map(|said| EntryTexts {
                    display_name: said.display_name.clone().unwrap_or_default(),
                    line: said.line.clone().unwrap_or_default(),
                    paragraph: said.paragraph.clone().unwrap_or_default(),
                    tags: said.tags.clone().unwrap_or_default(),
                })
                .collect(),
            self.categories.clone(),
            self.glyph
                .as_deref()
                .and_then(|written| entry::glyph(written).ok()),
        ))
    }
}

/// `text`, read as `T` if it is no larger than a plugin's entries may be,
/// with what refused it pushed to `problems` otherwise.
fn read<T: serde::de::DeserializeOwned>(
    file: &str,
    text: &str,
    problems: &mut Vec<EntriesProblem>,
) -> Option<T> {
    if text.len() > PluginEntries::most_bytes() {
        problems.push(EntriesProblem::TooLarge {
            file: file.to_owned(),
            bytes: text.len(),
        });
        return None;
    }
    toml::from_str(text)
        .map_err(|error| {
            problems.push(EntriesProblem::NotEntries {
                file: file.to_owned(),
                said: quoted_up_to(error.message(), QUOTED * 4),
            });
        })
        .ok()
}

/// Whether `name`, behind a namespace, is a command's name.
///
/// **Behind the shortest one**, a single letter: a name too long behind a
/// longer namespace is one the descriptor judge refuses in the command the
/// plugin declares, with its own sentence.
fn a_name_inside_a_namespace(name: &str) -> bool {
    descriptor::command_name_is_well_formed(&format!("a.{name}"))
}

/// A name the plugin wrote, as a refusal repeats it: cut, escaped and between
/// backticks, by the one rule every text a refusal of the kernel repeats is
/// held to.
fn quoted(text: &str) -> String {
    quoted_up_to(text, QUOTED)
}

#[cfg(test)]
mod tests {
    use nc_catalog::{ErrorCode, Locale};

    use super::{EntriesProblem, PluginEntries};

    /// A table of one command, `note.get`.
    const ONE: &str = "[[command]]\nname = \"note.get\"\n";

    /// Texts for `note.get` in one locale.
    fn texts(display_name: &str) -> String {
        format!(
            "[command.\"note.get\"]\ndisplay_name = \"{display_name}\"\nline = \"A line\"\n\
             paragraph = \"A paragraph that says more.\"\ntags = [\"note\"]\n"
        )
    }

    /// The problems `parse` found, or nothing when it read the entries.
    fn refused(commands: &str, handed: &[(Locale, &str)]) -> Vec<EntriesProblem> {
        match PluginEntries::parse(commands, handed) {
            Ok(_) => Vec::new(),
            Err(error) => {
                assert_eq!(error.code(), ErrorCode::PluginEntriesInvalid);
                error.problems().to_vec()
            }
        }
    }

    /// The reference plugin's own files parse, and list a row for its first
    /// command.
    #[test]
    fn the_reference_plugins_files_parse() {
        let entries = crate::entries_for_tests("reference");
        assert!(
            entries.names().any(|name| name == "note.get"),
            "{:?}",
            entries.names().collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_locale_table_for_a_command_commands_toml_does_not_list_is_refused() {
        let english = texts("Read a note");
        let elsewhere = english.replace("note.get", "note.\u{202e}put");
        assert_eq!(
            refused(ONE, &[(Locale::En, &english), (Locale::PtBR, &elsewhere)]),
            [EntriesProblem::Unlisted {
                locale: "pt-BR".to_owned(),
                name: "`note.\\u{202e}put`".to_owned()
            }]
        );
    }

    #[test]
    fn a_row_listed_twice_is_refused() {
        let english = texts("Read a note");
        assert_eq!(
            refused(
                &format!("{ONE}{ONE}"),
                &[(Locale::En, &english), (Locale::PtBR, &english)]
            ),
            [EntriesProblem::RowTwice("note.get".to_owned())]
        );
    }

    #[test]
    fn an_unknown_field_or_a_value_of_another_kind_is_refused() {
        let english = texts("Read a note");
        for (commands, locale) in [
            (format!("{ONE}colour = \"red\"\n"), english.clone()),
            (ONE.to_owned(), format!("{english}title = \"Read\"\n")),
            (ONE.to_owned(), english.replace("[\"note\"]", "\"note\"")),
            ("[[command]]\nname = 7\n".to_owned(), english.clone()),
        ] {
            let problems = refused(
                &commands,
                &[(Locale::En, &english), (Locale::PtBR, &locale)],
            );
            assert_eq!(problems.len(), 1, "{commands} / {locale}: {problems:?}");
            assert!(
                matches!(&problems[0], EntriesProblem::NotEntries { .. }),
                "{problems:?}"
            );
        }
    }

    #[test]
    fn a_locale_not_handed_or_handed_twice_is_refused() {
        let english = texts("Read a note");
        assert_eq!(
            refused(ONE, &[(Locale::En, &english)]),
            [EntriesProblem::LocaleNotHanded("pt-BR".to_owned())]
        );
        assert_eq!(
            refused(
                ONE,
                &[
                    (Locale::En, &english),
                    (Locale::PtBR, &english),
                    (Locale::PtBR, &english)
                ]
            ),
            [EntriesProblem::LocaleHandedTwice("pt-BR".to_owned())]
        );
    }

    /// A name no namespace can precede is refused, cut and escaped, and a name
    /// of more than one segment is not.
    #[test]
    fn a_relative_name_no_namespace_can_precede_is_refused() {
        let english = "";
        // Each name as TOML writes it, and as the refusal repeats it.
        for (written, shown) in [
            ("Note.get", "`Note.get`"),
            ("note..get", "`note..get`"),
            ("", "``"),
            ("note.\\u001Bget", "`note.\\u{1b}get`"),
        ] {
            assert_eq!(
                refused(
                    &format!("[[command]]\nname = \"{written}\"\n"),
                    &[(Locale::En, english), (Locale::PtBR, english)]
                ),
                [EntriesProblem::NameMalformed(shown.to_owned())],
                "{written}"
            );
        }
        let long = "a".repeat(200);
        let problems = refused(
            &format!("[[command]]\nname = \"{long}\"\n"),
            &[(Locale::En, english), (Locale::PtBR, english)],
        );
        assert_eq!(
            problems,
            [EntriesProblem::NameMalformed(format!(
                "`{}…`",
                "a".repeat(crate::registry::QUOTED)
            ))]
        );
        assert_eq!(
            refused(
                "[[command]]\nname = \"note.get\"\n[[command]]\nname = \"probe\"\n",
                &[(Locale::En, english), (Locale::PtBR, english)]
            ),
            []
        );
    }

    /// A file past the bound is refused unparsed, and one at it is parsed.
    #[test]
    fn a_text_past_the_read_bound_is_refused() {
        let most = PluginEntries::most_bytes();
        let english = texts("Read a note");
        let padded = |bytes: usize| format!("{ONE}{}", "#".repeat(bytes - ONE.len()));
        assert_eq!(
            refused(
                &padded(most),
                &[(Locale::En, &english), (Locale::PtBR, &english)]
            ),
            []
        );
        assert_eq!(
            refused(
                &padded(most + 1),
                &[(Locale::En, &english), (Locale::PtBR, &english)]
            ),
            [EntriesProblem::TooLarge {
                file: "the command table".to_owned(),
                bytes: most + 1
            }]
        );
        assert_eq!(
            most,
            usize::try_from(nc_catalog::KERNEL.commands_per_plugin).expect("fits")
                * usize::try_from(nc_catalog::KERNEL.entry_bytes).expect("fits")
                * 2
        );
    }

    /// Every problem of the shape is reported, the table's first and then
    /// each locale's, not the first one found.
    #[test]
    fn every_problem_of_the_shape_is_reported_not_the_first() {
        let english = texts("Read a note");
        let problems = refused(
            "[[command]]\nname = \"Note\"\n[[command]]\nname = \"note.get\"\n",
            &[(Locale::En, "not = [toml")],
        );
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(matches!(problems[0], EntriesProblem::NameMalformed(_)));
        assert_eq!(
            problems[1],
            EntriesProblem::LocaleNotHanded("pt-BR".to_owned())
        );
        assert!(
            matches!(&problems[2], EntriesProblem::NotEntries { file, .. } if file == "the `en` texts"),
            "{problems:?}"
        );
        assert_eq!(
            refused(ONE, &[(Locale::En, &english), (Locale::PtBR, &english)]),
            []
        );
    }
}
