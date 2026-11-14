//! What every entry shows a person: its texts, its tags, its glyph and its
//! wiki text (`NOCTUA.md §20.4`).
//!
//! **The rules are not written here.** `nc_schema::entry` holds them, once,
//! for the catalog and for every plugin the kernel admits; this module hands
//! that judge each command's texts as the locales resolve them, and reports
//! what it finds against the line a reader has to edit.
//!
//! **Nothing is reported twice.** A key absent from a locale, a sentence that
//! is the English one again and a placeholder in a command's sentence are
//! already the catalog's own message rules — `missing-key`,
//! `dangling-message-key`, `blank-message`, `untranslated` and
//! `param-not-declared`, over every key of every registry — so the judge is
//! handed `None` for a key that does not resolve and what it says of those is
//! dropped here, by name, in [`the_check_for`].
//!
//! # Wiki texts
//!
//! A wiki text is found, never declared: it sits at
//! `catalog/wiki/<locale>/<registry>/<name>.<extension>`, its format is its
//! extension — `txt` plain text, `md` Markdown, `typ` Typst, in lower case and
//! nothing else — and the extension is what follows the last dot, so a dotted
//! command name keeps its dots. **The tree is closed**: every file in it is
//! placed under a locale and a registry this catalog has, named for an entry
//! that registry holds, written in a format, in every locale and in one format
//! across them, or the build fails naming the file. A wiki text is held to
//! `nc_schema::entry::judge_wiki`, compiled into the generated crate, and
//! never read at run time from a directory an install does not have.
//!
//! # What this answers (`§2.4`)
//!
//! What a list, a page or an agent is shown of a command is a text the judge
//! read whole: bounded, holding nothing a reader cannot see, and translated.
//! A file under `catalog/wiki/` that is not a regular file — a directory where
//! a text belongs, or a link — is refused rather than followed, so the walk
//! reads nothing outside the tree it was pointed at.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use nc_schema::entry::{self, EntryProblem, InLocale, Rules, Text, Texts, WikiFormat};

use crate::load::SOURCE_LOCALE;
use crate::model::{Catalog, CommandEntry, WikiText};
use crate::problem::{Problem, line_of};

/// Where the wiki texts live.
const WIKI: &str = "catalog/wiki";

/// The registries a wiki text may belong to, as their directories are named.
const REGISTRIES: [&str; 1] = ["commands"];

/// The categories a command may name.
///
/// **None yet.** The categories registry is what a name is held to, and until
/// it is in this catalog every category a command names is one no list can
/// show it under, which the judge refuses.
const CATEGORIES: [&str; 0] = [];

/// Every command's texts, held to the one judge, and every display name to
/// being one command's.
pub(crate) fn check(
    root: &Path,
    catalog: &Catalog,
    commands_text: &str,
    problems: &mut Vec<Problem>,
) {
    let locale_texts: Vec<(String, String)> = catalog
        .locales
        .iter()
        .map(|locale| {
            let file = format!("catalog/i18n/{}.toml", locale.meta.bcp47);
            let text = std::fs::read_to_string(root.join(&file)).unwrap_or_default();
            (file, text)
        })
        .collect();
    for command in &catalog.commands {
        let in_locales: Vec<InLocale<'_>> = catalog
            .locales
            .iter()
            .map(|locale| InLocale {
                locale: &locale.meta.bcp47,
                texts: Texts {
                    display_name: locale.message.get(&command.title).map(String::as_str),
                    line: locale.message.get(&command.description).map(String::as_str),
                    paragraph: locale.message.get(&command.paragraph).map(String::as_str),
                    tags: locale.tags.get(&command.tags).map(Vec::as_slice),
                },
            })
            .collect();
        let judged = entry::judge(
            &entry::Entry {
                locales: &in_locales,
                categories: &command.categories,
                glyph: command.glyph.as_deref(),
                same_in_both: &[],
            },
            &Rules {
                requires: &Text::ALL,
                categories: &CATEGORIES,
            },
        );
        let Err(found) = judged else {
            continue;
        };
        for problem in found {
            let Some((check, fix)) = the_check_for(&problem) else {
                continue;
            };
            let (file, line) = located(command, &problem, catalog, &locale_texts, commands_text);
            problems.push(Problem {
                file,
                line,
                what: format!("`{}` {problem}", command.name),
                fix: fix.to_owned(),
                check,
            });
        }
    }
    one_command_per_display_name(catalog, &locale_texts, problems);
}

/// Where a reader edits what a problem is about: the locale file's key for a
/// text, the command's own entry for the rest.
fn located(
    command: &CommandEntry,
    problem: &EntryProblem,
    catalog: &Catalog,
    locale_texts: &[(String, String)],
    commands_text: &str,
) -> (String, Option<usize>) {
    let in_locale = |locale: &str, key: &str| {
        catalog
            .locales
            .iter()
            .position(|held| held.meta.bcp47 == locale)
            .and_then(|at| locale_texts.get(at))
            .map(|(file, text)| (file.clone(), line_of(text, &format!("\"{key}\""), 1)))
    };
    let key_of = |text: Text| match text {
        Text::DisplayName => &command.title,
        Text::Line => &command.description,
        Text::Paragraph => &command.paragraph,
        Text::Tags => &command.tags,
    };
    let at_the_text = match problem {
        EntryProblem::TooLong { locale, text, .. }
        | EntryProblem::Unseen { locale, text, .. }
        | EntryProblem::NotCarried { locale, text } => in_locale(locale, key_of(*text)),
        EntryProblem::ParagraphRepeatsLine { locale } => in_locale(locale, &command.paragraph),
        EntryProblem::NoTag { locale }
        | EntryProblem::TooManyTags { locale, .. }
        | EntryProblem::TagBlank { locale }
        | EntryProblem::TagMalformed { locale, .. }
        | EntryProblem::TagTwice { locale, .. } => in_locale(locale, &command.tags),
        _ => None,
    };
    at_the_text.unwrap_or_else(|| {
        (
            "catalog/commands.toml".to_owned(),
            line_of(commands_text, &format!("name = \"{}\"", command.name), 1),
        )
    })
}

/// The check a problem of the judge is reported under and what fixes it, or
/// nothing for a problem the catalog's message rules already report.
///
/// Written out per problem, so a problem the judge gains is a decision made
/// here rather than one inherited from a wildcard.
const fn the_check_for(problem: &EntryProblem) -> Option<(&'static str, &'static str)> {
    match problem {
        // `missing-key`, `dangling-message-key` and `blank-message` report a
        // key absent or blank, under the key and the line it belongs on;
        // `untranslated` reports a sentence that is the English one, under the
        // exemptions `THE_SAME_IN_BOTH` holds with their reasons; and
        // `param-not-declared` a placeholder, since `declared_params` gives a
        // command's sentences the empty set. The catalog hands the judge no
        // exemption, so it can find none unused or unreasoned.
        EntryProblem::LocaleMissing { .. }
        | EntryProblem::Missing { .. }
        | EntryProblem::Untranslated { .. }
        | EntryProblem::Placeholder { .. }
        | EntryProblem::ExemptionUnused(_)
        | EntryProblem::ExemptionUnreasoned(_) => None,
        EntryProblem::NotCarried { .. } => Some((
            "entry-text-not-carried",
            "a registry says which texts its entries carry, and a text none of its \
             surfaces shows is one nobody keeps true. Remove it from every locale",
        )),
        EntryProblem::TooLong { .. } => Some((
            "entry-text-too-long",
            "every text is bounded where it is shown: a line description is one terminal \
             line, a display name a list row, a tag a word a search matches. Say it in \
             fewer words, and move the detail to the paragraph",
        )),
        EntryProblem::Unseen { .. } => Some((
            "entry-invisible-character",
            "a character a reader sees nothing of makes a text say one thing to a person \
             and another to an agent or a terminal. Remove it; a line break in a sentence \
             is a second sentence, or the paragraph",
        )),
        EntryProblem::ParagraphRepeatsLine { .. } => Some((
            "paragraph-repeats-line",
            "the paragraph is shown below the line description, and the same sentence \
             twice tells a reader nothing the first did not. Say more in the paragraph: \
             what it reads, what it changes, when it stops",
        )),
        EntryProblem::NoTag { .. } => Some((
            "no-tag",
            "a command is found by its display name and its tags, in the reader's own \
             language, so every locale lists at least one word a person would type",
        )),
        EntryProblem::TooManyTags { .. } => Some((
            "too-many-tags",
            "tags are the words a search finds a command by, and past the bound they \
             find it for everything. Keep the words a person would type for it",
        )),
        EntryProblem::TagBlank { .. } => Some((
            "tag-blank",
            "a tag that says nothing is matched by nothing and shown as a gap in the list \
             of tags. Remove it, or write the word it was meant to be",
        )),
        EntryProblem::TagMalformed { .. } => Some((
            "tag-malformed",
            "tags are shown joined by commas, so a tag holding one reads as two. Split it \
             into two tags, or write it without the comma",
        )),
        EntryProblem::TagTwice { .. } => Some((
            "tag-twice",
            "a search folds case before it compares, so two tags differing only in case \
             are one word listed twice. Keep one of them",
        )),
        EntryProblem::TooManyCategories(_) => Some((
            "too-many-categories",
            "a category is a place a list shows the command under, and one listed \
             everywhere is found nowhere in particular. Keep the categories it belongs to",
        )),
        EntryProblem::CategoryUnknown(_) => Some((
            "unknown-category",
            "a command's categories are names the platform's categories registry holds, \
             and a name it does not hold is a category no list can show the command under. \
             Name one the registry holds, or leave `categories` out",
        )),
        EntryProblem::CategoryTwice(_) => Some((
            "category-twice",
            "a category named twice lists the command twice under one heading. Name each \
             category once",
        )),
        EntryProblem::Glyph(entry::GlyphProblem::Malformed(_)) => Some((
            "glyph-malformed",
            "a glyph is written `U+` and its code point in upper-case hex, four digits at \
             least and no leading zero past them — `U+F0E2`, never `U+0F0E2` or `U+f0e2` — \
             one spelling, so a search for a glyph finds every entry that carries it",
        )),
        EntryProblem::Glyph(entry::GlyphProblem::NotInTheFont(_)) => Some((
            "glyph-not-in-the-font",
            "a glyph is drawn by the Symbols-Only font, and a code point it does not hold \
             is a blank box. Choose one inside `nc_schema::entry::SYMBOLS_ONLY`, which is \
             that font's measured charset",
        )),
    }
}

/// No two commands are shown by one display name in one locale.
///
/// **Compared as the untranslated rule compares**, by
/// `nc_schema::entry::the_same_sentence`: `Undo` and `undo.` are one name to a
/// person reading a list, and a list holding both shows two rows a reader
/// cannot tell apart.
fn one_command_per_display_name(
    catalog: &Catalog,
    locale_texts: &[(String, String)],
    problems: &mut Vec<Problem>,
) {
    for (locale, (file, text)) in catalog.locales.iter().zip(locale_texts) {
        let shown: Vec<(&str, &str)> = catalog
            .commands
            .iter()
            .filter_map(|command| {
                locale
                    .message
                    .get(&command.title)
                    .map(|name| (command.name.as_str(), name.as_str()))
            })
            .collect();
        for (at, (name, display_name)) in shown.iter().enumerate() {
            let Some((first, _)) = shown[..at]
                .iter()
                .find(|(_, earlier)| entry::the_same_sentence(earlier, display_name))
            else {
                continue;
            };
            let key = catalog
                .commands
                .iter()
                .find(|command| command.name == *name)
                .map_or_else(String::new, |command| command.title.clone());
            problems.push(Problem {
                file: file.clone(),
                line: line_of(text, &format!("\"{key}\""), 1),
                what: format!(
                    "`{first}` and `{name}` are both shown as \"{}\" in {}",
                    crate::printable(display_name),
                    locale.meta.bcp47
                ),
                fix: "a display name is how a person tells one command from another in a \
                      list, and two commands under one name are two rows nobody can choose \
                      between. Name each for what it does"
                    .to_owned(),
                check: "display-name-twice",
            });
        }
    }
}

/// One wiki file, as the walk found it.
struct Found {
    /// The format its extension names.
    format: WikiFormat,
    /// Its text.
    text: String,
    /// Where it is, for a diagnostic.
    path: String,
}

/// Every wiki text under `catalog/wiki/`, held to the tree's shape and to the
/// judge `nc_schema::entry::judge_wiki`, by registry and then by name.
///
/// An absent `catalog/wiki/` is a catalog with no wiki text, which is a
/// catalog a wiki text is optional in.
pub(crate) fn wiki(root: &Path, catalog: &Catalog, problems: &mut Vec<Problem>) -> Vec<WikiText> {
    let tags: Vec<&str> = catalog
        .locales
        .iter()
        .map(|locale| locale.meta.bcp47.as_str())
        .collect();
    let commands: Vec<&str> = catalog
        .commands
        .iter()
        .map(|command| command.name.as_str())
        .collect();
    let mut found: BTreeMap<(String, String), BTreeMap<String, Found>> = BTreeMap::new();
    let top = root.join(WIKI);
    // Asked of the path itself, as every entry below is: a link standing where
    // the tree belongs would carry the walk out of the catalog.
    match std::fs::symlink_metadata(&top) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Ok(held) if held.file_type().is_dir() => {}
        _ => {
            misplaced(WIKI, "a directory holding one tree per locale", problems);
            return Vec::new();
        }
    }
    for (locale, kind, _) in listed(&top, WIKI, problems) {
        let under = format!("{WIKI}/{locale}");
        // A locale whose own file failed to load is reported there, and its
        // wiki directory is not a second defect.
        if kind.is_dir()
            && !tags.contains(&locale.as_str())
            && root.join(format!("catalog/i18n/{locale}.toml")).is_file()
        {
            continue;
        }
        if !kind.is_dir() || !tags.contains(&locale.as_str()) {
            misplaced(
                &under,
                "a directory named for a locale this catalog has",
                problems,
            );
            continue;
        }
        for (registry, kind, _) in listed(&top.join(&locale), &under, problems) {
            let under = format!("{under}/{registry}");
            if !kind.is_dir() || !REGISTRIES.contains(&registry.as_str()) {
                misplaced(&under, "a directory named for a registry", problems);
                continue;
            }
            for (file, kind, path) in listed(&top.join(&locale).join(&registry), &under, problems) {
                let relative = format!("{under}/{file}");
                if !kind.is_file() {
                    misplaced(
                        &relative,
                        "a regular file holding one entry's text",
                        problems,
                    );
                    continue;
                }
                let Some((name, format)) = one_text(&relative, &file, &commands, problems) else {
                    continue;
                };
                let Some(read) = judged_text(&relative, &path, problems) else {
                    continue;
                };
                let held = found.entry((registry.clone(), name.clone())).or_default();
                if let Some(first) = held.insert(
                    locale.clone(),
                    Found {
                        format,
                        text: read,
                        path: relative.clone(),
                    },
                ) {
                    problems.push(Problem::in_file(
                        &relative,
                        format!(
                            "is a second wiki text for `{name}` in {locale}, beside {}",
                            first.path
                        ),
                        "an entry has one wiki text per locale, in one format; two would leave \
                         which one a reader is shown to whichever the walk read last. Keep one",
                        "wiki-twice",
                    ));
                }
            }
        }
    }
    found
        .into_iter()
        .filter_map(|((registry, name), held)| complete(&registry, &name, held, &tags, problems))
        .collect()
}

/// One wiki file's text, read as UTF-8 and held to the judge.
fn judged_text(relative: &str, path: &Path, problems: &mut Vec<Problem>) -> Option<String> {
    let Ok(read) = std::fs::read_to_string(path) else {
        problems.push(Problem::in_file(
            relative,
            "could not be read as UTF-8 text",
            "a wiki text is compiled into the generated crate as a string, so it \
             is UTF-8 text a reader can open. Save it as UTF-8, or take it out",
            "wiki-unreadable",
        ));
        return None;
    };
    if let Err(why) = entry::judge_wiki(&read) {
        problems.push(Problem::in_file(
            relative,
            format!("{why}"),
            "a wiki text is shown to a person and handed to an agent, so it is \
             bounded and holds nothing a reader cannot see but its line breaks. \
             Write what the entry is, in the locale's own words",
            "wiki-text-refused",
        ));
        return None;
    }
    Some(read)
}

/// Every entry a directory holds, by name, with its kind and its path.
///
/// **Sorted**, so the walk reads one order whatever order the file system
/// lists in, and a diagnostic or a generated table never moves with it.
fn listed(
    directory: &Path,
    relative: &str,
    problems: &mut Vec<Problem>,
) -> Vec<(String, std::fs::FileType, PathBuf)> {
    // The marker sits on its own short line because rustfmt moves a trailing
    // comment off a long one, and the gate reads the line the call is on.
    let listing = std::fs::read_dir(directory); // allow-nondeterminism: this `read_dir` is sorted by name below, which `wiki_texts_are_read_in_one_order_whatever_the_directory_order` proves
    let entries = match listing {
        Ok(entries) => entries,
        Err(error) => {
            problems.push(Problem::in_file(
                relative,
                format!("could not be read: {error}"),
                "every wiki text is found by walking this tree, so a directory in it that \
                 cannot be opened hides every text below it. Fix its permissions",
                "wiki-unreadable",
            ));
            return Vec::new();
        }
    };
    let mut held: Vec<(String, std::fs::FileType, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let kind = entry.file_type().ok()?;
            Some((
                entry.file_name().to_string_lossy().into_owned(),
                kind,
                entry.path(),
            ))
        })
        .collect();
    held.sort_by(|a, b| a.0.cmp(&b.0));
    held
}

/// Reports a path in the wiki tree that is not where a text belongs.
fn misplaced(relative: &str, belongs: &str, problems: &mut Vec<Problem>) {
    problems.push(Problem::in_file(
        relative,
        format!("is not {belongs}"),
        "a wiki text sits at catalog/wiki/<locale>/<registry>/<name>.<extension>, and \
         nothing else sits in that tree — a file the walk does not place is one no entry \
         shows. Move it there, or take it out",
        "wiki-misplaced",
    ));
}

/// The entry a wiki file names and the format its extension does, or the
/// problem with either.
fn one_text(
    relative: &str,
    file: &str,
    commands: &[&str],
    problems: &mut Vec<Problem>,
) -> Option<(String, WikiFormat)> {
    let Some((name, extension)) = file.rsplit_once('.') else {
        problems.push(Problem::in_file(
            relative,
            "has no extension, so it names no format",
            "a wiki text's format is its extension: `txt` for plain text, `md` for \
             Markdown, `typ` for Typst, in lower case",
            "wiki-format-unknown",
        ));
        return None;
    };
    let Some(format) = WikiFormat::from_extension(extension) else {
        problems.push(Problem::in_file(
            relative,
            format!("is written `.{extension}`, which names no format"),
            "a wiki text's format is its extension, spelled one way: `txt` for plain text, \
             `md` for Markdown, `typ` for Typst, in lower case — `MD` is not `md`, so one \
             tree reads the same on every file system",
            "wiki-format-unknown",
        ));
        return None;
    };
    if !commands.contains(&name) {
        problems.push(Problem::in_file(
            relative,
            format!("is the wiki text of `{name}`, which the registry does not hold"),
            "a wiki text is named for the entry it describes, and one naming no entry is \
             shown by nothing. Rename it for the entry, or take it out",
            "wiki-orphan",
        ));
        return None;
    }
    Some((name.to_owned(), format))
}

/// One entry's wiki texts, held to being in every locale, in one format, and
/// translated.
fn complete(
    registry: &str,
    name: &str,
    mut held: BTreeMap<String, Found>,
    tags: &[&str],
    problems: &mut Vec<Problem>,
) -> Option<WikiText> {
    let mut texts: Vec<Found> = Vec::new();
    for tag in tags {
        match held.remove(*tag) {
            Some(text) => texts.push(text),
            None => problems.push(Problem::in_file(
                format!("{WIKI}/{tag}/{registry}"),
                format!("has no wiki text for `{name}`, which another locale has"),
                "every text a person is shown exists in every locale, the wiki text \
                 included — a reader of this locale would find the entry's page empty. \
                 Translate it into this locale, in the same format",
                "wiki-locale-missing",
            )),
        }
    }
    if texts.len() != tags.len() {
        return None;
    }
    let format = texts.first()?.format;
    if let Some(other) = texts.iter().find(|text| text.format != format) {
        problems.push(Problem::in_file(
            &other.path,
            format!(
                "is written in `.{}` and the source locale's text of `{name}` in `.{}`",
                other.format.extension(),
                format.extension()
            ),
            "one entry's wiki text is one document in several languages, and a reader \
             switching language should not switch renderer. Write every locale's in one \
             format",
            "wiki-formats-differ",
        ));
        return None;
    }
    let source = &texts.first()?.text;
    let mut untranslated = false;
    for text in &texts[1..] {
        if entry::the_same_sentence(&text.text, source) {
            untranslated = true;
            problems.push(Problem::in_file(
                &text.path,
                format!("is the {SOURCE_LOCALE} wiki text of `{name}`, word for word"),
                "a wiki text present in every locale with the same words reads as \
                 translated and is not. Translate it",
                "untranslated",
            ));
        }
    }
    (!untranslated).then(|| WikiText {
        registry: registry.to_owned(),
        name: name.to_owned(),
        format,
        texts: texts.into_iter().map(|text| text.text).collect(),
    })
}

#[cfg(test)]
mod tests {
    use nc_schema::entry::{EntryProblem, GlyphProblem, Text};

    use super::the_check_for;

    /// What the catalog's message rules already report is dropped, and every
    /// other problem of the judge is reported under a check of its own with a
    /// fix.
    ///
    /// **Both halves**: a problem dropped that no message rule reports is a
    /// refusal the catalog never makes — a paragraph past its bound would ship
    /// — and one reported here as well as there is a reader told the same
    /// thing twice, under two names.
    #[test]
    fn the_message_rules_problems_are_dropped_and_every_other_is_reported() {
        let locale = || "pt-BR".to_owned();
        for dropped in [
            EntryProblem::LocaleMissing { locale: locale() },
            EntryProblem::Missing {
                locale: locale(),
                text: Text::DisplayName,
            },
            EntryProblem::Untranslated {
                locale: locale(),
                text: Text::Line,
            },
            EntryProblem::Placeholder {
                locale: locale(),
                text: Text::Paragraph,
                name: "x".to_owned(),
            },
            EntryProblem::ExemptionUnused(Text::DisplayName),
            EntryProblem::ExemptionUnreasoned(Text::DisplayName),
        ] {
            assert_eq!(the_check_for(&dropped), None, "{dropped:?} was reported");
        }
        let mut checks = std::collections::BTreeSet::new();
        for reported in [
            EntryProblem::NotCarried {
                locale: locale(),
                text: Text::Tags,
            },
            EntryProblem::TooLong {
                locale: locale(),
                text: Text::Line,
                held: 81,
            },
            EntryProblem::Unseen {
                locale: locale(),
                text: Text::Line,
                character: '\u{202e}',
            },
            EntryProblem::ParagraphRepeatsLine { locale: locale() },
            EntryProblem::NoTag { locale: locale() },
            EntryProblem::TooManyTags {
                locale: locale(),
                held: 33,
            },
            EntryProblem::TagBlank { locale: locale() },
            EntryProblem::TagMalformed {
                locale: locale(),
                tag: "a, b".to_owned(),
            },
            EntryProblem::TagTwice {
                locale: locale(),
                tag: "a".to_owned(),
            },
            EntryProblem::TooManyCategories(5),
            EntryProblem::CategoryUnknown("x".to_owned()),
            EntryProblem::CategoryTwice("x".to_owned()),
            EntryProblem::Glyph(GlyphProblem::Malformed("f0e2".to_owned())),
            EntryProblem::Glyph(GlyphProblem::NotInTheFont(0xF382)),
        ] {
            let (check, fix) =
                the_check_for(&reported).unwrap_or_else(|| panic!("{reported:?} was dropped"));
            assert!(fix.len() > 40, "{check}'s fix says too little: {fix}");
            assert!(checks.insert(check), "`{check}` names two problems");
        }
    }
}
