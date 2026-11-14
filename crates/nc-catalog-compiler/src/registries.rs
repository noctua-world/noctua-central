//! The registries other entries draw from, and those of things outside the
//! platform: the categories and the external identifiers (`NOCTUA.md §20.4`).
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
//! **An external identifier is a thing outside the platform** — an
//! application, a binary, a configuration file — and what identifies it on
//! each system it is identified on: on Linux the commands that start it, its
//! desktop files, its Flatpak and Snap names and its paths; on macOS its
//! bundle identifiers, its application bundles and its executables; on Windows
//! its executables, its keys under `App Paths` and its paths. Each field holds
//! values of one closed shape ([`identification`]); an entry identified on no
//! system, and a system's table naming nothing, are refused.
//!
//! **What a person is shown of a set, a category and an external identifier**
//! — a display name and a line description in every locale, and for an
//! external identifier a paragraph and tags as well — is the entry judge's,
//! through `crate::metadata`, as every entry's texts are.
//!
//! # What this answers (`§2.4`)
//!
//! A category's name, its standard's name and its role are each a closed
//! shape: a name is lower-case letters and digits in words joined by single
//! hyphens, which a list and a command line can carry, a standard name is
//! printable ASCII with nothing a reader sees nothing of, and a role is one of
//! the words its standard uses. An external identifier's name is the same
//! shape, and every value it is identified by is bounded, holds nothing a
//! reader sees nothing of and no `..`, and is anchored where it is a path: at
//! the root on Linux and macOS, and on Windows at one of the directories the
//! system's environment names. **Identification is data**: nothing in this
//! build runs, expands or resolves a value, and a reader that one day matches
//! one against the system holds a match as a hint, never as proof of what it
//! found. Nothing here is read at run time from anywhere but the generated
//! crate.

use std::collections::{BTreeMap, BTreeSet};

use crate::load::{floor, push, says_nothing};
use crate::model::{
    CategoryEntry, CategoryFile, CategorySetEntry, ExternalIdentifierEntry, ExternalIdentifierFile,
};
use crate::problem::{Problem, line_of};

/// Where the categories registry is written.
pub(crate) const CATEGORIES: &str = "catalog/categories.toml";

/// The text each registry this module reads was read from.
pub(crate) struct Texts {
    /// `catalog/categories.toml`.
    pub(crate) categories: String,
    /// `catalog/external-identifiers.toml`.
    pub(crate) identifiers: String,
}

/// Every registry this module reads, read, parsed and held to its rules,
/// with the text each was read from.
///
/// `None` when one could not be read or parsed, which is reported; each is
/// read whatever became of the other, so a build reports both.
pub(crate) fn all(
    root: &std::path::Path,
    problems: &mut Vec<Problem>,
) -> Option<(CategoryFile, ExternalIdentifierFile, Texts)> {
    let categories = categories(root, problems);
    let identifiers = external_identifiers(root, problems);
    let ((categories_text, categories), (identifiers_text, identifiers)) =
        (categories?, identifiers?);
    let texts = Texts {
        categories: categories_text,
        identifiers: identifiers_text,
    };
    Some((categories, identifiers, texts))
}

/// The categories registry, read, parsed and held to the rules above, with
/// its text for the line a later problem is filed at; `None` when it could
/// not be read or parsed, which is reported.
fn categories(
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

/// Where the external-identifiers registry is written.
pub(crate) const EXTERNAL_IDENTIFIERS: &str = "catalog/external-identifiers.toml";

/// The longest value an entry is identified by, in characters.
///
/// The longest path Windows opens without its long-path prefix, which no
/// other field's values come near.
const IDENTIFICATION_MAX: usize = 260;

/// The directories a Windows path may begin at.
///
/// As the system's environment names them: the programs installed for every
/// user, in both of their directories, and those installed for one.
const WINDOWS_ANCHORS: [&str; 3] = [
    "%ProgramFiles%\\",
    "%ProgramFiles(x86)%\\",
    "%LOCALAPPDATA%\\",
];

/// One field of what identifies an external identifier on a system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Field {
    /// A command `PATH` finds on Linux.
    LinuxExecutable,
    /// A desktop file's name.
    DesktopId,
    /// A Flatpak application identifier.
    FlatpakId,
    /// A Snap package's name.
    SnapName,
    /// An absolute path on Linux.
    LinuxPath,
    /// A macOS bundle identifier.
    BundleId,
    /// An application bundle's absolute path.
    AppPath,
    /// The executable inside a macOS bundle.
    MacExecutable,
    /// A Windows executable's name.
    WindowsExecutable,
    /// A key under the registry's `App Paths`.
    AppPathsKey,
    /// A path under a directory the Windows environment names.
    WindowsPath,
}

impl Field {
    /// The key the registry writes it under, in its system's table.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::LinuxExecutable | Self::MacExecutable | Self::WindowsExecutable => "executables",
            Self::DesktopId => "desktop_ids",
            Self::FlatpakId => "flatpak_ids",
            Self::SnapName => "snap_names",
            Self::LinuxPath | Self::WindowsPath => "paths",
            Self::BundleId => "bundle_ids",
            Self::AppPath => "app_paths",
            Self::AppPathsKey => "app_paths_keys",
        }
    }
}

/// A system an external identifier is identified on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum System {
    /// Linux, in any distribution.
    Linux,
    /// Apple's macOS.
    MacOs,
    /// Windows.
    Windows,
}

impl System {
    /// The table the registry writes it as, under `[identifier.<table>]`.
    pub(crate) const fn table(self) -> &'static str {
        match self {
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Windows => "windows",
        }
    }

    /// What a page calls it.
    pub(crate) const fn word(self) -> &'static str {
        match self {
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
        }
    }
}

/// What identifies one entry on one system: the system, and each of its
/// fields with its values.
pub(crate) type OnASystem<'a> = (System, Option<Vec<(Field, &'a [String])>>);

/// What identifies an entry on each system, in the order the registry and
/// every page write them; `None` for a system it is not identified on.
pub(crate) fn on_each_system(entry: &ExternalIdentifierEntry) -> [OnASystem<'_>; 3] {
    [
        (
            System::Linux,
            entry.linux.as_ref().map(|on| {
                vec![
                    (Field::LinuxExecutable, on.executables.as_slice()),
                    (Field::DesktopId, on.desktop_ids.as_slice()),
                    (Field::FlatpakId, on.flatpak_ids.as_slice()),
                    (Field::SnapName, on.snap_names.as_slice()),
                    (Field::LinuxPath, on.paths.as_slice()),
                ]
            }),
        ),
        (
            System::MacOs,
            entry.macos.as_ref().map(|on| {
                vec![
                    (Field::BundleId, on.bundle_ids.as_slice()),
                    (Field::AppPath, on.app_paths.as_slice()),
                    (Field::MacExecutable, on.executables.as_slice()),
                ]
            }),
        ),
        (
            System::Windows,
            entry.windows.as_ref().map(|on| {
                vec![
                    (Field::WindowsExecutable, on.executables.as_slice()),
                    (Field::AppPathsKey, on.app_paths_keys.as_slice()),
                    (Field::WindowsPath, on.paths.as_slice()),
                ]
            }),
        ),
    ]
}

/// The external-identifiers registry, read, parsed and held to the rules
/// above.
///
/// With its text for the line a later problem is filed at; `None` when it
/// could not be read or parsed, which is reported.
fn external_identifiers(
    root: &std::path::Path,
    problems: &mut Vec<Problem>,
) -> Option<(String, ExternalIdentifierFile)> {
    let text = crate::load::read(root, EXTERNAL_IDENTIFIERS, problems)?;
    let file: ExternalIdentifierFile = crate::load::parse(EXTERNAL_IDENTIFIERS, &text, problems)?;
    check_external_identifiers(&file, &text, problems);
    Some((text, file))
}

/// Every external identifier: its name once and of its shape, and what
/// identifies it.
fn check_external_identifiers(
    file: &ExternalIdentifierFile,
    text: &str,
    problems: &mut Vec<Problem>,
) {
    floor(
        file.identifiers.len(),
        EXTERNAL_IDENTIFIERS,
        "external identifier",
        problems,
    );
    let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
    for entry in &file.identifiers {
        let times = seen.entry(entry.name.as_str()).or_default();
        *times += 1;
        let line = line_of(text, &format!("name = \"{}\"", entry.name), *times);
        if *times > 1 {
            push(
                problems,
                EXTERNAL_IDENTIFIERS,
                line,
                format!("the external identifier `{}` is declared twice", entry.name),
                "a reader finds an external identifier by its name, and two of one name \
                 would leave which one it finds to whichever the compiler read last. Keep \
                 one",
                "duplicate-external-identifier",
            );
        }
        if !a_slug(&entry.name) {
            push(
                problems,
                EXTERNAL_IDENTIFIERS,
                line,
                format!("`{}` cannot name an external identifier", entry.name),
                "an external identifier's name is a word a command line, a page and a search \
                 all carry: lower-case letters and digits, in words joined by single hyphens \
                 — `google-chrome`",
                "external-identifier-name-malformed",
            );
        }
        check_the_identification(entry, line, problems);
    }
}

/// What identifies one entry: at least one system, every system's table
/// naming something, and every value of its field's shape, once.
fn check_the_identification(
    entry: &ExternalIdentifierEntry,
    line: Option<usize>,
    problems: &mut Vec<Problem>,
) {
    let systems = on_each_system(entry);
    if systems.iter().all(|(_, fields)| fields.is_none()) {
        push(
            problems,
            EXTERNAL_IDENTIFIERS,
            line,
            format!("`{}` is identified on no system", entry.name),
            "an external identifier is what identifies a thing on the systems it runs on, \
             and one identified on none identifies nothing. Write a `[identifier.linux]`, \
             `[identifier.macos]` or `[identifier.windows]` table with what was measured \
             there",
            "identified-on-no-system",
        );
        return;
    }
    for (system, fields) in systems {
        let Some(fields) = fields else {
            continue;
        };
        if fields.iter().all(|(_, values)| values.is_empty()) {
            push(
                problems,
                EXTERNAL_IDENTIFIERS,
                line,
                format!(
                    "`{}`'s `{}` table names nothing",
                    entry.name,
                    system.table()
                ),
                "a system's table says the thing is identified there, and one with no value \
                 says so with nothing behind it. Write what was measured there, or remove the \
                 table",
                "identification-empty",
            );
        }
        for (field, values) in fields {
            for (at, value) in values.iter().enumerate() {
                let written = format!("{}.{}", system.table(), field.key());
                if let Err(rule) = identification(field, value) {
                    push(
                        problems,
                        EXTERNAL_IDENTIFIERS,
                        line,
                        format!(
                            "`{}`'s `{written}` holds \"{}\", which is not one",
                            entry.name,
                            crate::printable(value)
                        ),
                        rule.fix(),
                        "identification-malformed",
                    );
                }
                if values[..at].contains(value) {
                    push(
                        problems,
                        EXTERNAL_IDENTIFIERS,
                        line,
                        format!(
                            "`{}`'s `{written}` holds \"{}\" twice",
                            entry.name,
                            crate::printable(value)
                        ),
                        "a value identifies its thing once, and a second copy is one a reader \
                         matching it counts twice. Keep one",
                        "identification-twice",
                    );
                }
            }
        }
    }
}

/// Whether a value can be what its field holds, and the rule it breaks if not.
///
/// **Every field** holds a value a reader sees all of, of at most
/// [`IDENTIFICATION_MAX`] characters, with nothing before or after it and no
/// `..` anywhere — a path that climbs out of where it says it is. **Each
/// field** then has its own shape: a name `PATH` or a bundle finds holds no
/// `/`; a desktop file's name ends `.desktop`; a Flatpak identifier is three or
/// more dotted words and a bundle identifier two or more, of ASCII letters,
/// digits and hyphens (and underscores, for Flatpak); a Snap name is lower-case
/// letters, digits and hyphens, at most 40; a path on Linux and an application
/// bundle on macOS begin at the root, the second ending `.app`; and on Windows
/// an executable and an `App Paths` key are a file name ending `.exe`, and a
/// path begins at one of [`WINDOWS_ANCHORS`] and is written with backslashes.
///
/// # Errors
///
/// [`Refused`] says which rule a value breaks.
pub(crate) fn identification(field: Field, value: &str) -> Result<(), Refused> {
    if says_nothing(value) || value.trim() != value {
        return Err(Refused::NotAsWritten);
    }
    if value.chars().any(nc_schema::unseen) {
        return Err(Refused::Unseen);
    }
    if value.chars().count() > IDENTIFICATION_MAX {
        return Err(Refused::TooLong);
    }
    // Two dots in a row, wherever they are: a path climbing out of where it
    // says it is, and nothing any other field's value holds.
    if value
        .chars()
        .zip(value.chars().skip(1))
        .any(|pair| pair == ('.', '.'))
    {
        return Err(Refused::Climbs);
    }
    let well_formed = match field {
        Field::LinuxExecutable | Field::MacExecutable => !value.contains('/'),
        Field::DesktopId => {
            !value.contains('/')
                && value
                    .strip_suffix(".desktop")
                    .is_some_and(|stem| !stem.is_empty())
        }
        Field::FlatpakId => dotted(value, 3, |character| {
            character.is_ascii_alphanumeric() || character == '_' || character == '-'
        }),
        Field::SnapName => {
            value.chars().count() <= 40
                && value.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                })
        }
        Field::LinuxPath => value.starts_with('/'),
        Field::BundleId => dotted(value, 2, |character| {
            character.is_ascii_alphanumeric() || character == '-'
        }),
        Field::AppPath => {
            value.starts_with('/')
                && value
                    .strip_suffix(".app")
                    .is_some_and(|bundle| !bundle.ends_with('/'))
        }
        Field::WindowsExecutable | Field::AppPathsKey => {
            !value.contains(['\\', '/'])
                && value
                    .strip_suffix(".exe")
                    .is_some_and(|stem| !stem.is_empty())
        }
        Field::WindowsPath => {
            !value.contains('/')
                && WINDOWS_ANCHORS.iter().any(|anchor| {
                    value
                        .strip_prefix(anchor)
                        .is_some_and(|rest| !rest.is_empty())
                })
        }
    };
    if well_formed {
        Ok(())
    } else {
        Err(Refused::Shape(field))
    }
}

/// Which rule an identification value breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// It says nothing, or something stands before or after it.
    NotAsWritten,
    /// It holds a character a reader sees nothing of.
    Unseen,
    /// It is longer than [`IDENTIFICATION_MAX`].
    TooLong,
    /// It holds `..`.
    Climbs,
    /// It is not the shape its field holds.
    Shape(Field),
}

impl Refused {
    /// The rule, as the fix a diagnostic gives.
    const fn fix(self) -> &'static str {
        match self {
            Self::NotAsWritten => {
                "a value is written as the system writes it, with nothing before or after \
                 it. Copy it again from where it was measured"
            }
            Self::Unseen => {
                "a value a reader sees part of is one a reader cannot check against the \
                 system. Write it with nothing a reader sees nothing of"
            }
            Self::TooLong => {
                "a value is at most 260 characters, the longest path Windows opens without \
                 its long-path prefix. Shorten it, or leave it out"
            }
            Self::Climbs => {
                "a value holds no `..`: in a path it climbs out of the directory the path \
                 says it is in, and no name or identifier holds one. Write the value as it \
                 resolves"
            }
            Self::Shape(field) => the_shape_of(field),
        }
    }
}

/// What a value of a field looks like, as the fix a diagnostic gives.
const fn the_shape_of(field: Field) -> &'static str {
    match field {
        Field::LinuxExecutable | Field::MacExecutable => {
            "an executable is the name a system starts it by, with no `/`: its path, where \
             one was measured, is a `paths` or `app_paths` value"
        }
        Field::DesktopId => {
            "a desktop file is named as its directory lists it, ending `.desktop`, with no \
             `/` — `google-chrome.desktop`"
        }
        Field::FlatpakId => {
            "a Flatpak identifier is three or more words joined by dots, each of ASCII \
             letters, digits, `_` and `-` — `org.chromium.Chromium`"
        }
        Field::SnapName => {
            "a Snap name is lower-case letters, digits and hyphens, at most 40 — `chromium`"
        }
        Field::LinuxPath => "a path on Linux is absolute, beginning at `/`",
        Field::BundleId => {
            "a bundle identifier is two or more words joined by dots, each of ASCII \
             letters, digits and `-` — `com.google.Chrome`"
        }
        Field::AppPath => {
            "an application bundle's path is absolute and ends `.app` — \
             `/Applications/Google Chrome.app`"
        }
        Field::WindowsExecutable | Field::AppPathsKey => {
            "a Windows executable and an `App Paths` key are a file name ending `.exe`, with \
             no separator — `chrome.exe`"
        }
        Field::WindowsPath => {
            "a Windows path begins at `%ProgramFiles%\\`, `%ProgramFiles(x86)%\\` or \
             `%LOCALAPPDATA%\\` — the directories its environment names, so one path holds \
             on every machine — and is written with backslashes"
        }
    }
}

/// At least `least` words joined by single dots, each of characters `allowed`
/// admits.
fn dotted(value: &str, least: usize, allowed: impl Fn(char) -> bool) -> bool {
    value.split('.').count() >= least
        && value
            .split('.')
            .all(|word| !word.is_empty() && word.chars().all(&allowed))
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
    use super::{Field, System, a_slug, a_standard_name, identification};

    /// Every system is written as its table in the registry writes it, and
    /// called on a page what its makers call it.
    #[test]
    fn every_system_is_written_as_the_registry_and_a_page_write_it() {
        for (system, table, word) in [
            (System::Linux, "linux", "Linux"),
            (System::MacOs, "macos", "macOS"),
            (System::Windows, "windows", "Windows"),
        ] {
            assert_eq!(system.table(), table, "{system:?}");
            assert_eq!(system.word(), word, "{system:?}");
        }
    }

    /// Each identification field, with values its system writes and values
    /// that each break exactly one of its rules.
    ///
    /// The rules every field shares are driven through one field each: a
    /// value that says nothing through a Snap name, whose own shape takes an
    /// empty one, `..` through a Windows path, and the rest through a Linux
    /// path. **An absolute path is written with its root apart**,
    /// `concat!("/", …)`, because the determinism gate reads a literal opening
    /// at `/` in a crate the emitter reaches as a file on this machine.
    const WHAT_EACH_FIELD_TAKES: [(Field, &[&str], &[&str]); 11] = [
        (
            Field::LinuxExecutable,
            &["google-chrome-stable", "chromium"],
            &[concat!("/", "usr/bin/chromium")],
        ),
        (
            Field::DesktopId,
            &["google-chrome.desktop", "com.google.Chrome.desktop"],
            &[
                "google-chrome",
                ".desktop",
                "applications/google-chrome.desktop",
            ],
        ),
        (
            Field::FlatpakId,
            &[
                "org.chromium.Chromium",
                "com.google.Chrome_dev",
                "org.example.my-app",
            ],
            &["org.chromium", "org.chromium.", "org.chromium.Chro mium"],
        ),
        (
            Field::SnapName,
            &["chromium", "chromium-ffmpeg-2"],
            &["", "Chromium", "chromium_beta"],
        ),
        (
            Field::LinuxPath,
            &[
                concat!("/", "opt/google/chrome/google-chrome"),
                concat!("/", "usr/bin/chromium"),
            ],
            &[
                "usr/bin/chromium",
                concat!("/", "usr/bin/chromium "),
                concat!("/", "usr/bin/chro\u{202e}mium"),
            ],
        ),
        (
            Field::BundleId,
            &["com.google.Chrome", "org.chromium.Chromium"],
            &["Chrome", "com.google.", "com.google.Chrome_beta"],
        ),
        (
            Field::AppPath,
            &[concat!("/", "Applications/Google Chrome.app")],
            &[
                "Applications/Chromium.app",
                concat!("/", "Applications/Chromium"),
                concat!("/", "Applications/.app"),
            ],
        ),
        (
            Field::MacExecutable,
            &["Google Chrome", "Chromium"],
            &["Contents/MacOS/Chromium"],
        ),
        (
            Field::WindowsExecutable,
            &["chrome.exe"],
            &[
                "chrome",
                ".exe",
                "Application\\chrome.exe",
                "bin/chrome.exe",
            ],
        ),
        (Field::AppPathsKey, &["chrome.exe"], &["chrome.com"]),
        (
            Field::WindowsPath,
            &[
                "%ProgramFiles%\\Google\\Chrome\\Application\\chrome.exe",
                "%ProgramFiles(x86)%\\Google\\Chrome\\Application\\chrome.exe",
                "%LOCALAPPDATA%\\Google\\Chrome\\Application\\chrome.exe",
            ],
            &[
                "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
                "%APPDATA%\\Google\\Chrome\\chrome.exe",
                "%PROGRAMFILES%\\Google\\chrome.exe",
                "%ProgramFiles%\\",
                "%ProgramFiles%\\Google/Chrome/chrome.exe",
                "%ProgramFiles%\\Google\\..\\..\\Windows\\explorer.exe",
            ],
        ),
    ];

    /// Every identification field refuses what it cannot be.
    ///
    /// **Both ways, field by field**: each field accepts what its system
    /// writes, and refuses a value that breaks exactly one of its rules — and
    /// each bound is held at the bound and one past it.
    #[test]
    fn every_identification_field_refuses_what_it_cannot_be() {
        for (field, accepted, refused) in WHAT_EACH_FIELD_TAKES {
            for value in accepted {
                assert_eq!(
                    identification(field, value),
                    Ok(()),
                    "`{value}` was refused as {field:?}"
                );
            }
            for value in refused {
                assert!(
                    identification(field, value).is_err(),
                    "`{value}` was taken as {field:?}"
                );
            }
        }
        let long = |length: usize| format!("{}{}", '/', "a".repeat(length - 1));
        assert_eq!(identification(Field::LinuxPath, &long(260)), Ok(()));
        assert!(identification(Field::LinuxPath, &long(261)).is_err());
        assert_eq!(identification(Field::SnapName, &"a".repeat(40)), Ok(()));
        assert!(identification(Field::SnapName, &"a".repeat(41)).is_err());
    }

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
