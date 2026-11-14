//! The registries other entries draw from: the categories (`NOCTUA.md §20.4`).
//!
//! **A category is a name a list shows an entry under**, and the categories
//! registry is the closed set of them: a set — a standard's, or the
//! platform's own — and every category in it, named `<set>.<slug>`. A
//! command names its categories from here, and the judge refuses one this
//! registry does not hold (`unknown-category`).
//!
//! **What a standard says is written as the standard says it.** A category of
//! a standard set carries the name its standard gives it and, for a standard
//! that ranks its categories, where it stands; a category of the platform's
//! own set carries neither, because no standard defines it. Both halves are
//! refused rather than ignored: a standard name on the platform's own category
//! is a claim about a standard that does not make it.
//!
//! **What a person is shown of a set and of a category** — a display name and
//! a line description in every locale — is the entry judge's, through
//! `crate::metadata`, as every entry's texts are.
//!
//! # What this answers (`§2.4`)
//!
//! A category's name, its standard's name and its role are each a closed
//! shape: a name is lower-case letters and digits in words joined by single
//! hyphens, which a list and a command line can carry, a standard name is
//! printable ASCII with nothing a reader sees nothing of, and a role is one of
//! the words its standard uses. Nothing here is read at run time from anywhere
//! but the generated crate.

use std::collections::{BTreeMap, BTreeSet};

use crate::load::{floor, push, says_nothing};
use crate::model::{CategoryEntry, CategoryFile, CategorySetEntry};
use crate::problem::{Problem, line_of};

/// Where the categories registry is written.
pub(crate) const CATEGORIES: &str = "catalog/categories.toml";

/// The categories registry, read, parsed and held to the rules above, with
/// its text for the line a later problem is filed at; `None` when it could
/// not be read or parsed, which is reported.
pub(crate) fn categories(
    root: &std::path::Path,
    problems: &mut Vec<Problem>,
) -> Option<(String, CategoryFile)> {
    let text = crate::load::read(root, CATEGORIES, problems)?;
    let file: CategoryFile = crate::load::parse(CATEGORIES, &text, problems)?;
    check_categories(&file, &text, problems);
    Some((text, file))
}

/// Every set and every category, held to the rules above.
fn check_categories(file: &CategoryFile, text: &str, problems: &mut Vec<Problem>) {
    floor(file.sets.len(), CATEGORIES, "set of categories", problems);
    floor(file.categories.len(), CATEGORIES, "category", problems);
    let mut sets: BTreeMap<&str, &CategorySetEntry> = BTreeMap::new();
    for set in &file.sets {
        let line = line_of(text, &format!("name = \"{}\"", set.name), 1);
        if sets.insert(set.name.as_str(), set).is_some() {
            push(
                problems,
                CATEGORIES,
                line_of(text, &format!("name = \"{}\"", set.name), 2),
                format!("the set `{}` is declared twice", set.name),
                "a category names its set, and two sets of one name would leave which one \
                 it is in to whichever the compiler read last. Merge them, or rename one",
                "duplicate-category-set",
            );
        }
        check_one_set(set, line, problems);
    }
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    let mut standard_names: BTreeSet<(&str, &str)> = BTreeSet::new();
    for category in &file.categories {
        let count = seen.entry(category.name.as_str()).or_default();
        *count += 1;
        let line = line_of(text, &format!("name = \"{}\"", category.name), *count);
        if *count > 1 {
            push(
                problems,
                CATEGORIES,
                line,
                format!("the category `{}` is declared twice", category.name),
                "an entry names its categories by name, and two declarations of one name \
                 would show the entry under whichever the compiler read last. Keep one",
                "duplicate-category",
            );
        }
        let Some(set) = sets.get(category.set.as_str()) else {
            push(
                problems,
                CATEGORIES,
                line,
                format!(
                    "`{}` is in the set `{}`, which this registry does not declare",
                    category.name, category.set
                ),
                "a category is listed under its set, and a set nothing declares has no name \
                 a person is shown and no source. Declare the set as a `[[set]]`, or name one \
                 that is declared",
                "unknown-category-set",
            );
            continue;
        };
        check_the_name(category, line, problems);
        check_what_the_standard_says(category, set, line, problems);
        if let Some(standard_name) = category.standard_name.as_deref()
            && !standard_names.insert((category.set.as_str(), standard_name))
        {
            push(
                problems,
                CATEGORIES,
                line,
                format!(
                    "`{}` is the set `{}`'s `{standard_name}` a second time",
                    category.name, category.set
                ),
                "a standard names each of its categories once, so two categories under one \
                 standard name are one category written twice, and a reader matching an \
                 application's declared category would find both. Keep one",
                "standard-name-twice",
            );
        }
    }
    every_set_holds_a_category_and_ranks_all_or_none(file, text, problems);
}

/// One set's own fields: its name, what defines it, and its version.
fn check_one_set(set: &CategorySetEntry, line: Option<usize>, problems: &mut Vec<Problem>) {
    if !a_slug(&set.name) {
        push(
            problems,
            CATEGORIES,
            line,
            format!("`{}` cannot name a set", set.name),
            "a set's name is the first part of every category's name in it — \
             `freedesktop.web-browser` — so it is lower-case letters and digits, in words \
             joined by single hyphens, and no dot",
            "category-set-name-malformed",
        );
    }
    if says_nothing(&set.defined_by) {
        push(
            problems,
            CATEGORIES,
            line,
            format!("the set `{}` says nothing about what defines it", set.name),
            "a set is a standard's, or this catalog's own, and a reader of a category needs \
             to know which before trusting it. Name the standard, or this catalog",
            "category-set-undefined",
        );
    }
    let version = set.version.as_deref();
    if set.standard && version.is_none_or(says_nothing) {
        push(
            problems,
            CATEGORIES,
            line,
            format!("the standard set `{}` names no version", set.name),
            "a standard changes between versions, and a set taken from one is only as \
             true as the version it was read from. Write the version, or the date it was \
             read if its standard has none",
            "category-set-version-missing",
        );
    }
    if !set.standard && version.is_some() {
        push(
            problems,
            CATEGORIES,
            line,
            format!(
                "the set `{}` is this catalog's own and names a version",
                set.name
            ),
            "the platform's own set changes with this file, which is its only version; a \
             version written here claims a standard that does not exist. Remove it",
            "category-set-version-unexpected",
        );
    }
}

/// A category is named `<set>.<slug>`, under the set it says it is in.
fn check_the_name(category: &CategoryEntry, line: Option<usize>, problems: &mut Vec<Problem>) {
    match category.name.split_once('.') {
        Some((set, slug)) if set == category.set && a_slug(slug) => {}
        Some((set, _)) if set != category.set => push(
            problems,
            CATEGORIES,
            line,
            format!(
                "`{}` is named under `{set}` and is in the set `{}`",
                category.name, category.set
            ),
            "a category's name begins with its set's, so a name read alone — in a command's \
             `categories`, on a page — says which set it is from. Name it \
             `<set>.<slug>` after the set it is in",
            "category-outside-its-set",
        ),
        _ => push(
            problems,
            CATEGORIES,
            line,
            format!("`{}` is not `<set>.<slug>`", category.name),
            "a category is named for its set, a dot, and lower-case letters and digits in \
             words joined by single hyphens — `freedesktop.web-browser` — one spelling a \
             command line, a page and a search all carry",
            "category-name-malformed",
        ),
    }
}

/// A standard set's category carries its standard name, and nothing else's
/// does; a role is refused outside a standard set.
fn check_what_the_standard_says(
    category: &CategoryEntry,
    set: &CategorySetEntry,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    match (set.standard, category.standard_name.as_deref()) {
        (true, None) => push(
            problems,
            CATEGORIES,
            line,
            format!(
                "`{}` is in the standard set `{}` and carries no standard name",
                category.name, category.set
            ),
            "a category of a standard is matched against what an application declares, \
             which is the standard's own name for it. Write it as the standard writes it, \
             in `standard_name`",
            "standard-name-missing",
        ),
        (true, Some(written)) if !a_standard_name(written) => push(
            problems,
            CATEGORIES,
            line,
            format!(
                "`{}`'s standard name `{}` is not one a standard writes",
                category.name,
                crate::printable(written)
            ),
            "a standard name is matched character for character against what an \
             application declares, so it is printable ASCII with no space and nothing a \
             reader sees nothing of. Copy it from the standard",
            "standard-name-malformed",
        ),
        (false, Some(_)) => push(
            problems,
            CATEGORIES,
            line,
            format!(
                "`{}` is in the set `{}`, this catalog's own, and carries a standard name",
                category.name, category.set
            ),
            "the platform's own categories are named by this catalog alone, and a standard \
             name on one claims a standard that does not define it. Remove it",
            "standard-name-unexpected",
        ),
        _ => {}
    }
    if !set.standard && category.role.is_some() {
        push(
            problems,
            CATEGORIES,
            line,
            format!(
                "`{}` is in the set `{}`, this catalog's own, and carries a role",
                category.name, category.set
            ),
            "a role is where a standard ranks its category, and no standard ranks the \
             platform's own. Remove it",
            "role-unexpected",
        );
    }
}

/// Every set holds at least one category, and a set whose standard ranks its
/// categories ranks every one of them.
///
/// **All or none, decided by the set's own categories**: a standard that ranks
/// its categories ranks each — the Desktop Menu Specification registers every
/// one as main, additional or reserved — so a category of such a set with no
/// role is one whose rank was left out, and one with a role in a set whose
/// others carry none is a rank its standard does not give.
fn every_set_holds_a_category_and_ranks_all_or_none(
    file: &CategoryFile,
    text: &str,
    problems: &mut Vec<Problem>,
) {
    for set in &file.sets {
        let held: Vec<&CategoryEntry> = file
            .categories
            .iter()
            .filter(|category| category.set == set.name)
            .collect();
        if held.is_empty() {
            push(
                problems,
                CATEGORIES,
                line_of(text, &format!("name = \"{}\"", set.name), 1),
                format!("the set `{}` holds no category", set.name),
                "a set is shown as a heading over its categories, and one with none is a \
                 heading over nothing. Add its categories, or remove the set",
                "category-set-empty",
            );
        }
        let ranked = held
            .iter()
            .filter(|category| category.role.is_some())
            .count();
        if !set.standard || ranked == 0 || ranked == held.len() {
            continue;
        }
        for category in held.iter().filter(|category| category.role.is_none()) {
            push(
                problems,
                CATEGORIES,
                line_of(text, &format!("name = \"{}\"", category.name), 1),
                format!(
                    "`{}` carries no role, and {ranked} of the set `{}`'s {} categories do",
                    category.name,
                    set.name,
                    held.len()
                ),
                "a standard that ranks its categories ranks every one, so a set's \
                 categories carry a role each or none does. Write the role the standard \
                 gives it — `main`, `additional` or `reserved` — or remove the others",
                "role-missing",
            );
        }
    }
}

/// Lower-case ASCII letters and digits, in words joined by single hyphens.
fn a_slug(text: &str) -> bool {
    !text.is_empty()
        && text.split('-').all(|word| {
            !word.is_empty()
                && word
                    .chars()
                    .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        })
}

/// Printable ASCII with no space: what a standard writes a category's name as.
fn a_standard_name(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|character| character.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::{a_slug, a_standard_name};

    /// A slug is lower-case letters and digits in words joined by single
    /// hyphens, and nothing else.
    #[test]
    fn a_slug_is_lower_case_words_joined_by_single_hyphens() {
        for slug in ["web-browser", "2d-graphics", "kde", "p-2p", "action-games"] {
            assert!(a_slug(slug), "`{slug}` was refused");
        }
        for not_a_slug in [
            "",
            "Web-browser",
            "web--browser",
            "-web",
            "web-",
            "web_browser",
            "web.browser",
            "web browser",
            "navegação",
        ] {
            assert!(!a_slug(not_a_slug), "`{not_a_slug}` was taken as a slug");
        }
    }

    /// A standard name is printable ASCII with no space.
    #[test]
    fn a_standard_name_is_printable_ascii_with_no_space() {
        for name in ["WebBrowser", "2DGraphics", "public.app-category.games"] {
            assert!(a_standard_name(name), "`{name}` was refused");
        }
        for not_one in [
            "",
            "Web Browser",
            "Web\u{200b}Browser",
            "Navegação",
            "Tab\t",
        ] {
            assert!(
                !a_standard_name(not_one),
                "`{not_one}` was taken as a standard name"
            );
        }
    }
}
