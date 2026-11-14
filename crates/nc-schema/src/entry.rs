//! A registry entry's texts, judged whole: the one judge for the catalog and
//! for every plugin.
//!
//! `NOCTUA.md §20.4` gives every entry of a registry a display name, a line
//! description, a paragraph and tags, in every locale, and optionally
//! categories, a glyph and a wiki text. The catalog compiler holds the
//! catalog's entries to the rules below at build time; the kernel holds a
//! plugin's to the same rules at `activate`. [`judge`] is those rules, written
//! once, and it reports **every** problem it finds rather than the first, as
//! [`crate::descriptor::judge`] does.
//!
//! **A registry chooses which texts it carries**, through [`Rules::requires`]:
//! a command carries all four, a category its display name and line alone. A
//! text a registry does not carry is refused when it is written, because a
//! text nothing shows is a text nobody keeps true.
//!
//! # What this answers (`§2.4`)
//!
//! **A text shows a reader everything it holds.** Every text and every tag is
//! held to [`crate::unseen`]: a direction override in a display name shows one
//! command under another's words, and a run of tag characters in a paragraph is
//! an instruction an agent reads and a person does not.
//!
//! **Every text is bounded**, in characters, by the constants below: a line is
//! one terminal line, and a display name, a paragraph and a tag each have a
//! bound a list row and a matcher can hold. A wiki text is bounded in bytes by
//! [`WIKI_BYTES_MAX`].
//!
//! **A translation is not the source sentence again.** A text identical to the
//! source locale's, by [`the_same_sentence`], is refused unless the entry
//! exempts it with a reason that reads as one — an exemption with no reason is
//! an untranslated text with a second column drawn around it.
//!
//! **A glyph is a code point the Symbols-Only font draws**, written one way,
//! and held to [`SYMBOLS_ONLY`].
//!
//! **What this does not answer**: whether a category names one the platform
//! holds is the caller's, which hands the names in through [`Rules`]; whether
//! two entries share a display name is the registry's, which sees them all;
//! and whether a text is true, or a glyph depicts what its comment says.

use core::fmt;
use std::collections::BTreeSet;

use crate::{excerpt, shows_nothing, unseen};

/// The longest display name, in characters.
pub const DISPLAY_NAME_MAX: usize = 64;

/// The longest line description, in characters: one terminal line.
pub const LINE_MAX: usize = 80;

/// The longest paragraph, in characters.
pub const PARAGRAPH_MAX: usize = 1_024;

/// The longest tag, in characters.
pub const TAG_MAX: usize = 40;

/// The most tags one entry carries in one locale.
pub const TAGS_MAX: usize = 32;

/// The most categories one entry names.
pub const CATEGORIES_MAX: usize = 4;

/// The largest wiki text, in bytes.
pub const WIKI_BYTES_MAX: usize = 65_536;

/// One text an entry carries in each locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Text {
    /// The name a person reads first.
    DisplayName,
    /// One line saying what it is.
    Line,
    /// A paragraph saying more.
    Paragraph,
    /// The words a search finds it by.
    Tags,
}

impl Text {
    /// Every text, in the order an entry shows them.
    pub const ALL: [Self; 4] = [Self::DisplayName, Self::Line, Self::Paragraph, Self::Tags];

    /// The most characters this text may hold, or one tag of it.
    #[must_use]
    pub const fn bound(self) -> usize {
        match self {
            Self::DisplayName => DISPLAY_NAME_MAX,
            Self::Line => LINE_MAX,
            Self::Paragraph => PARAGRAPH_MAX,
            Self::Tags => TAG_MAX,
        }
    }
}

impl fmt::Display for Text {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::DisplayName => "display name",
            Self::Line => "line description",
            Self::Paragraph => "paragraph",
            Self::Tags => "tags",
        })
    }
}

/// What an entry says in one locale, each text `None` where it was not written.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Texts<'a> {
    /// The display name.
    pub display_name: Option<&'a str>,
    /// The line description.
    pub line: Option<&'a str>,
    /// The paragraph.
    pub paragraph: Option<&'a str>,
    /// The tags, in the order written.
    pub tags: Option<&'a [String]>,
}

impl Texts<'_> {
    /// The text of one of the three sentences; `None` for the tags.
    const fn sentence(&self, text: Text) -> Option<&str> {
        match text {
            Text::DisplayName => self.display_name,
            Text::Line => self.line,
            Text::Paragraph => self.paragraph,
            Text::Tags => None,
        }
    }

    /// Whether this text was written at all.
    const fn holds(&self, text: Text) -> bool {
        match text {
            Text::Tags => self.tags.is_some(),
            sentence => self.sentence(sentence).is_some(),
        }
    }
}

/// One locale's texts, under its BCP 47 tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InLocale<'a> {
    /// The BCP 47 tag.
    pub locale: &'a str,
    /// What the entry says in it.
    pub texts: Texts<'a>,
}

/// An entry as its author wrote it.
#[derive(Debug, Clone, Copy)]
pub struct Entry<'a> {
    /// Every locale, the source locale first.
    pub locales: &'a [InLocale<'a>],
    /// The categories it names.
    pub categories: &'a [String],
    /// Its glyph, written `U+XXXX`.
    pub glyph: Option<&'a str>,
    /// Each text exempt from the untranslated rule, with the reason.
    pub same_in_both: &'a [(Text, &'a str)],
}

/// What a registry holds its entries to.
#[derive(Debug, Clone, Copy)]
pub struct Rules<'a> {
    /// The texts an entry of this registry carries, each required.
    pub requires: &'a [Text],
    /// The categories an entry may name.
    pub categories: &'a [&'a str],
}

/// Why a glyph was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GlyphProblem {
    /// Not `U+` and upper-case hex, four digits at least and no leading zero past them.
    Malformed(String),
    /// A code point the Symbols-Only font does not draw.
    NotInTheFont(u32),
}

impl fmt::Display for GlyphProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(written) => write!(
                formatter,
                "has a glyph written `{written}`, and a glyph is written `U+` and its code \
                 point in upper-case hex, four digits at least and no leading zero past them \
                 — `U+F0E2`, never `U+0F0E2`"
            ),
            Self::NotInTheFont(point) => write!(
                formatter,
                "has the glyph `U+{point:04X}`, which the Symbols-Only font does not draw"
            ),
        }
    }
}

impl std::error::Error for GlyphProblem {}

/// One thing wrong with an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryProblem {
    /// A locale in which the entry wrote nothing at all.
    LocaleMissing {
        /// The locale.
        locale: String,
    },
    /// A text the registry requires, absent or saying nothing in one locale.
    Missing {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
    },
    /// A text the registry does not carry, written.
    NotCarried {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
    },
    /// A text past its bound, in characters.
    TooLong {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
        /// How many characters it holds.
        held: usize,
    },
    /// A text or a tag holding a character a reader sees nothing of.
    Unseen {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
        /// The character.
        character: char,
    },
    /// A placeholder in a text no caller fills.
    Placeholder {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
        /// The placeholder's name.
        name: String,
    },
    /// A text that is the source locale's sentence again.
    Untranslated {
        /// The locale.
        locale: String,
        /// The text.
        text: Text,
    },
    /// An exemption from the untranslated rule that exempts nothing.
    ExemptionUnused(Text),
    /// An exemption whose reason does not say why.
    ExemptionUnreasoned(Text),
    /// A paragraph that says what the line says.
    ParagraphRepeatsLine {
        /// The locale.
        locale: String,
    },
    /// An empty tag list.
    NoTag {
        /// The locale.
        locale: String,
    },
    /// More tags than [`TAGS_MAX`].
    TooManyTags {
        /// The locale.
        locale: String,
        /// How many there are.
        held: usize,
    },
    /// A tag that says nothing.
    TagBlank {
        /// The locale.
        locale: String,
    },
    /// A tag holding a comma, which a list joined by commas reads as two.
    TagMalformed {
        /// The locale.
        locale: String,
        /// The tag, cut and escaped.
        tag: String,
    },
    /// Two tags of one locale that lower-case to one.
    TagTwice {
        /// The locale.
        locale: String,
        /// The tag, cut and escaped.
        tag: String,
    },
    /// More categories than [`CATEGORIES_MAX`].
    TooManyCategories(usize),
    /// A category the registry does not hold, cut and escaped.
    CategoryUnknown(String),
    /// A category named twice, cut and escaped.
    CategoryTwice(String),
    /// A glyph refused.
    Glyph(GlyphProblem),
}

/// The predicate a caller writes its own subject in front of.
///
/// As [`crate::DescriptorProblem`]'s is: `` `kernel.undo` has no `pt-BR`
/// display name `` names the command because the caller wrote it, and the
/// kernel writes the plugin beside it.
impl fmt::Display for EntryProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocaleMissing { locale } => {
                write!(formatter, "says nothing at all in `{locale}`")
            }
            Self::Missing { locale, text } => write!(formatter, "has no `{locale}` {text}"),
            Self::NotCarried { locale, text } => write!(
                formatter,
                "has a `{locale}` {text}, which no entry of its registry carries"
            ),
            Self::TooLong { locale, text, held } => write!(
                formatter,
                "has a `{locale}` {text} of {held} characters, past the {} it may hold",
                text.bound()
            ),
            Self::Unseen {
                locale,
                text,
                character,
            } => write!(
                formatter,
                "has a `{locale}` {text} holding `U+{:04X}`, which a reader sees nothing of",
                u32::from(*character)
            ),
            Self::Placeholder { locale, text, name } => write!(
                formatter,
                "has a `{locale}` {text} holding the placeholder `{{{}}}`, which nothing fills",
                excerpt(name)
            ),
            Self::Untranslated { locale, text } => write!(
                formatter,
                "has a `{locale}` {text} that is the source sentence, word for word"
            ),
            Self::ExemptionUnused(text) => write!(
                formatter,
                "exempts its {text} from the untranslated rule, and no locale repeats it"
            ),
            Self::ExemptionUnreasoned(text) => write!(
                formatter,
                "exempts its {text} from the untranslated rule with a reason that does not \
                 say why"
            ),
            Self::ParagraphRepeatsLine { locale } => write!(
                formatter,
                "has a `{locale}` paragraph that says what its line description says"
            ),
            Self::NoTag { locale } => write!(formatter, "has no `{locale}` tag"),
            Self::TooManyTags { locale, held } => write!(
                formatter,
                "has {held} `{locale}` tags, past the {TAGS_MAX} an entry may hold"
            ),
            Self::TagBlank { locale } => {
                write!(formatter, "has a `{locale}` tag that says nothing")
            }
            Self::TagMalformed { locale, tag } => write!(
                formatter,
                "has the `{locale}` tag `{tag}`, holding a comma a list of tags reads as two"
            ),
            Self::TagTwice { locale, tag } => {
                write!(formatter, "has the `{locale}` tag `{tag}` twice")
            }
            Self::TooManyCategories(held) => write!(
                formatter,
                "names {held} categories, past the {CATEGORIES_MAX} an entry may name"
            ),
            Self::CategoryUnknown(name) => write!(
                formatter,
                "names the category `{name}`, which its registry does not hold"
            ),
            Self::CategoryTwice(name) => {
                write!(formatter, "names the category `{name}` twice")
            }
            Self::Glyph(problem) => problem.fmt(formatter),
        }
    }
}

impl std::error::Error for EntryProblem {}

/// Judges an entry, reporting every problem found.
///
/// # Errors
///
/// Every [`EntryProblem`] found, never an empty list: per locale in the order
/// handed, the texts in [`Text::ALL`]'s order — [`EntryProblem::LocaleMissing`]
/// standing for every [`EntryProblem::Missing`] of a locale that wrote nothing
/// — then the exemptions, the categories and the glyph.
pub fn judge(entry: &Entry<'_>, rules: &Rules<'_>) -> Result<(), Vec<EntryProblem>> {
    let mut problems = Vec::new();
    let source = entry.locales.first();
    for (at, in_locale) in entry.locales.iter().enumerate() {
        let locale = in_locale.locale;
        let texts = &in_locale.texts;
        if Text::ALL.iter().all(|text| !texts.holds(*text)) {
            problems.push(EntryProblem::LocaleMissing {
                locale: locale.to_owned(),
            });
            continue;
        }
        for text in Text::ALL {
            let carried = rules.requires.contains(&text);
            if !carried {
                if texts.holds(text) {
                    problems.push(EntryProblem::NotCarried {
                        locale: locale.to_owned(),
                        text,
                    });
                }
                continue;
            }
            if text == Text::Tags {
                judge_tags(locale, texts.tags, &mut problems);
                continue;
            }
            let Some(written) = texts.sentence(text).filter(|held| !says_nothing(held)) else {
                problems.push(EntryProblem::Missing {
                    locale: locale.to_owned(),
                    text,
                });
                continue;
            };
            judge_sentence(locale, text, written, &mut problems);
            let theirs = source
                .filter(|_| at > 0)
                .and_then(|source| source.texts.sentence(text));
            if theirs.is_some_and(|theirs| the_same_sentence(written, theirs))
                && !entry.same_in_both.iter().any(|(exempt, _)| *exempt == text)
            {
                problems.push(EntryProblem::Untranslated {
                    locale: locale.to_owned(),
                    text,
                });
            }
        }
        if let (Some(line), Some(paragraph)) = (texts.line, texts.paragraph)
            && rules.requires.contains(&Text::Paragraph)
            && !says_nothing(paragraph)
            && the_same_sentence(line, paragraph)
        {
            problems.push(EntryProblem::ParagraphRepeatsLine {
                locale: locale.to_owned(),
            });
        }
    }
    judge_exemptions(entry, &mut problems);
    judge_categories(entry.categories, rules.categories, &mut problems);
    if let Some(written) = entry.glyph
        && let Err(problem) = glyph(written)
    {
        problems.push(EntryProblem::Glyph(problem));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// One sentence's bound, characters and placeholders.
fn judge_sentence(locale: &str, text: Text, written: &str, problems: &mut Vec<EntryProblem>) {
    let held = written.chars().count();
    if held > text.bound() {
        problems.push(EntryProblem::TooLong {
            locale: locale.to_owned(),
            text,
            held,
        });
    }
    if let Some(character) = written.chars().find(|shown| unseen(*shown)) {
        problems.push(EntryProblem::Unseen {
            locale: locale.to_owned(),
            text,
            character,
        });
    }
    // A command's texts take no parameters: they are shown in a list, on a
    // page and to an agent, where there is no caller to fill one.
    if let Some(name) = placeholders(written).into_iter().next() {
        problems.push(EntryProblem::Placeholder {
            locale: locale.to_owned(),
            text,
            name,
        });
    }
}

/// One locale's tags: at least one, at most [`TAGS_MAX`], each judged, none twice.
fn judge_tags(locale: &str, tags: Option<&[String]>, problems: &mut Vec<EntryProblem>) {
    let Some(tags) = tags else {
        problems.push(EntryProblem::Missing {
            locale: locale.to_owned(),
            text: Text::Tags,
        });
        return;
    };
    if tags.is_empty() {
        problems.push(EntryProblem::NoTag {
            locale: locale.to_owned(),
        });
    }
    if tags.len() > TAGS_MAX {
        problems.push(EntryProblem::TooManyTags {
            locale: locale.to_owned(),
            held: tags.len(),
        });
    }
    let mut seen = BTreeSet::new();
    for tag in tags {
        if says_nothing(tag) {
            problems.push(EntryProblem::TagBlank {
                locale: locale.to_owned(),
            });
            continue;
        }
        let held = tag.chars().count();
        if held > TAG_MAX {
            problems.push(EntryProblem::TooLong {
                locale: locale.to_owned(),
                text: Text::Tags,
                held,
            });
        }
        if let Some(character) = tag.chars().find(|shown| unseen(*shown)) {
            problems.push(EntryProblem::Unseen {
                locale: locale.to_owned(),
                text: Text::Tags,
                character,
            });
        }
        if tag.contains(',') {
            problems.push(EntryProblem::TagMalformed {
                locale: locale.to_owned(),
                tag: excerpt(tag),
            });
        }
        // **Per character, lower-cased**: `Desfazer` and `desfazer` are one
        // word to a search, which folds case before it compares.
        let folded: String = tag.chars().flat_map(char::to_lowercase).collect();
        if !seen.insert(folded) {
            problems.push(EntryProblem::TagTwice {
                locale: locale.to_owned(),
                tag: excerpt(tag),
            });
        }
    }
}

/// Every exemption exempts a text some locale repeats, and says why.
fn judge_exemptions(entry: &Entry<'_>, problems: &mut Vec<EntryProblem>) {
    let Some((source, others)) = entry.locales.split_first() else {
        return;
    };
    for (text, reason) in entry.same_in_both {
        let repeated = source.texts.sentence(*text).is_some_and(|theirs| {
            others.iter().any(|other| {
                other
                    .texts
                    .sentence(*text)
                    .is_some_and(|ours| the_same_sentence(ours, theirs))
            })
        });
        if !repeated {
            problems.push(EntryProblem::ExemptionUnused(*text));
        }
        if !a_reason_is_prose(reason) {
            problems.push(EntryProblem::ExemptionUnreasoned(*text));
        }
    }
}

/// At most [`CATEGORIES_MAX`] categories, each one the registry holds, none twice.
fn judge_categories(named: &[String], held: &[&str], problems: &mut Vec<EntryProblem>) {
    if named.len() > CATEGORIES_MAX {
        problems.push(EntryProblem::TooManyCategories(named.len()));
    }
    let mut seen = BTreeSet::new();
    for name in named {
        if !held.contains(&name.as_str()) {
            problems.push(EntryProblem::CategoryUnknown(excerpt(name)));
        }
        if !seen.insert(name.as_str()) {
            problems.push(EntryProblem::CategoryTwice(excerpt(name)));
        }
    }
}

/// Whether a text says nothing a reader would see.
///
/// **What a reader sees, not what `char::is_whitespace` answers**: a text of
/// one `U+3164 HANGUL FILLER` is a letter to `trim` and nothing on a screen,
/// so it is missing rather than written.
fn says_nothing(text: &str) -> bool {
    text.chars()
        .all(|shown| shown.is_whitespace() || unseen(shown))
}

/// Whether an exemption's reason says why, rather than standing in for one.
///
/// Twenty characters and four words is the floor: a reason that is a word or a
/// dash is the unexplained hole the reason exists to close.
#[must_use]
pub fn a_reason_is_prose(reason: &str) -> bool {
    reason.trim().chars().count() >= 20 && reason.split_whitespace().count() >= 4
}

/// The code points the Symbols-Only font draws, as inclusive ranges.
///
/// **Measured, not transcribed**: the font's own charset, which `NOCTUA.md
/// §20.4` names as the glyphs an entry may carry, read with
///
/// ```text
/// fc-query -f '%{charset}\n' noctua-design/assets/fonts/SymbolsNerdFont-Regular.woff2
/// ```
///
/// which prints 28 ranges holding 10,410 code points, the Material Design
/// icons among them in plane 15 — so a point is a `u32`, and a table of
/// `u16` would lose two thirds of the font. When Nerd Fonts is upgraded, run
/// that command again and write what it prints here; the test beside this
/// counts the points.
pub const SYMBOLS_ONLY: [(u32, u32); 28] = [
    (0x23FB, 0x23FE),
    (0x2630, 0x2630),
    (0x2665, 0x2665),
    (0x26A1, 0x26A1),
    (0x276C, 0x2771),
    (0x2B58, 0x2B58),
    (0xE000, 0xE00A),
    (0xE0A0, 0xE0A3),
    (0xE0B0, 0xE0C8),
    (0xE0CA, 0xE0CA),
    (0xE0CC, 0xE0D2),
    (0xE0D4, 0xE0D4),
    (0xE0D6, 0xE0D7),
    (0xE200, 0xE2A9),
    (0xE300, 0xE3E3),
    (0xE5FA, 0xE6B8),
    (0xE700, 0xE8EF),
    (0xEA60, 0xEA88),
    (0xEA8A, 0xEA8C),
    (0xEA8F, 0xEAC7),
    (0xEAC9, 0xEAC9),
    (0xEACC, 0xEB09),
    (0xEB0B, 0xEB4E),
    (0xEB50, 0xEC1E),
    (0xED00, 0xEFCE),
    (0xF000, 0xF381),
    (0xF400, 0xF533),
    (0xF0001, 0xF1AF0),
];

/// The glyph a text writes, if it is written the one way and the font draws it.
///
/// **One spelling per glyph**: `U+` and the code point in upper-case hex, four
/// digits at least and no leading zero past them — so `U+F0E2` and never
/// `U+f0e2`, `U+0F0E2` or the character itself, and a search for a glyph finds
/// every entry that carries it.
///
/// # Errors
///
/// [`GlyphProblem::Malformed`] for any other spelling, and
/// [`GlyphProblem::NotInTheFont`] for a point outside [`SYMBOLS_ONLY`].
pub fn glyph(written: &str) -> Result<char, GlyphProblem> {
    let malformed = || GlyphProblem::Malformed(excerpt(written));
    let digits = written.strip_prefix("U+").ok_or_else(malformed)?;
    if !(4..=6).contains(&digits.len())
        || !digits
            .chars()
            .all(|digit| digit.is_ascii_digit() || matches!(digit, 'A'..='F'))
    {
        return Err(malformed());
    }
    let point = u32::from_str_radix(digits, 16).map_err(|_| malformed())?;
    if format!("U+{point:04X}") != written {
        return Err(malformed());
    }
    if !SYMBOLS_ONLY
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&point))
    {
        return Err(GlyphProblem::NotInTheFont(point));
    }
    char::from_u32(point).ok_or(GlyphProblem::NotInTheFont(point))
}

/// The format a wiki text is written in, named by its file's extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WikiFormat {
    /// `txt`: plain text.
    PlainText,
    /// `md`: Markdown.
    Markdown,
    /// `typ`: Typst.
    Typst,
}

impl WikiFormat {
    /// Every format, in the order the extensions are listed.
    pub const ALL: [Self; 3] = [Self::PlainText, Self::Markdown, Self::Typst];

    /// The extension this format is written under, lower case.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::PlainText => "txt",
            Self::Markdown => "md",
            Self::Typst => "typ",
        }
    }

    /// The format an extension names, compared exactly.
    ///
    /// **`MD` names nothing.** One spelling per format, so a file's format is
    /// read off its name one way on every file system, and a case-folding one
    /// cannot hold two texts that differ only in it.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|format| format.extension() == extension)
    }
}

/// Why a wiki text was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WikiProblem {
    /// A text that says nothing.
    Blank,
    /// A text past [`WIKI_BYTES_MAX`], in bytes.
    TooLarge(usize),
    /// A character a reader sees nothing of, a line break aside.
    Unseen(char),
}

impl fmt::Display for WikiProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => formatter.write_str("says nothing"),
            Self::TooLarge(bytes) => write!(
                formatter,
                "holds {bytes} bytes, past the {WIKI_BYTES_MAX} a wiki text may hold"
            ),
            Self::Unseen(character) => write!(
                formatter,
                "holds `U+{:04X}`, which a reader sees nothing of",
                u32::from(*character)
            ),
        }
    }
}

impl std::error::Error for WikiProblem {}

/// Judges one wiki text.
///
/// A line break is the one character [`crate::unseen`] answers for that a
/// wiki text may hold: it is text over several lines, and every other control
/// moves a cursor or turns a line around.
///
/// # Errors
///
/// [`WikiProblem::Blank`], [`WikiProblem::TooLarge`] or [`WikiProblem::Unseen`],
/// the first found in that order.
pub fn judge_wiki(text: &str) -> Result<(), WikiProblem> {
    if says_nothing(text) {
        return Err(WikiProblem::Blank);
    }
    if text.len() > WIKI_BYTES_MAX {
        return Err(WikiProblem::TooLarge(text.len()));
    }
    if let Some(character) = text.chars().find(|held| *held != '\n' && unseen(*held)) {
        return Err(WikiProblem::Unseen(character));
    }
    Ok(())
}

/// Whether two sentences are the same one, for the untranslated rule.
///
/// **Byte equality alone makes the rule one keystroke wide**: a trailing
/// period, a capital, or two spaces between words make an English sentence
/// pasted into a translation pass as one. Measured, `error.db.migration-failed`
/// in `catalog/i18n/pt-BR.toml` set to the English sentence with a full stop
/// added, under byte equality:
///
/// ```text
/// cargo xtask check   ->  check passed
/// docs/errors.md      ->  both language columns, one sentence
/// ```
///
/// Normalised on the three things a translator never changes on purpose and a
/// copy-paste changes by accident: case, run-together whitespace, and the
/// punctuation a sentence ends on. Anything past that is a judgement about
/// meaning, which an exemption and its written reason are for.
///
/// **What a reader sees, not what the bytes are.** What [`shows_nothing`]
/// answers for is replaced by a space, as the catalog's printer replaces it
/// on every page — so the English sentence with one of its spaces written
/// `U+200D` normalises to the sentence the page publishes, and not to
/// `oldrecords`. What [`crate::BLANK_RANGES`] holds is dropped: those sit
/// inside words, and a filler appended to the English sentence is the English
/// sentence on the page. Measured on `NC-JRNL-0001`, the journal-tampering
/// message, filed in pt-BR as the English sentence plus `U+2063` or `U+3164`:
/// with whitespace alone dropped, the build accepts both, and `docs/errors.md`
/// publishes English in the pt-BR column.
#[must_use]
pub fn the_same_sentence(ours: &str, theirs: &str) -> bool {
    fn plainly(text: &str) -> String {
        let folded: String = text
            .chars()
            .filter_map(|character| {
                if shows_nothing(character) {
                    Some(' ')
                } else if unseen(character) {
                    None
                } else {
                    Some(character)
                }
            })
            .collect::<String>()
            .to_lowercase();
        let words: Vec<&str> = folded.split_whitespace().collect();
        words
            .join(" ")
            .trim_matches(|character: char| {
                matches!(character, '.' | '!' | '?' | ':' | ';' | ',' | '…')
            })
            .to_owned()
    }
    plainly(ours) == plainly(theirs)
}

/// The `{name}` placeholders in a sentence.
///
/// A set rather than a list, because a message may legitimately repeat one and
/// the caller supplies it once either way. A brace pair holding anything but a
/// bare name of ASCII letters, digits and `_` is prose — a JSON fragment in an
/// example, or a set written out — and not a placeholder.
#[must_use]
pub fn placeholders(sentence: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = sentence;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else { break };
        let name = &rest[..close];
        if !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            found.insert(name.to_owned());
        }
        rest = &rest[close + 1..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::{
        CATEGORIES_MAX, Entry, EntryProblem, GlyphProblem, InLocale, Rules, SYMBOLS_ONLY, TAGS_MAX,
        Text, Texts, WIKI_BYTES_MAX, WikiFormat, WikiProblem, a_reason_is_prose, glyph, judge,
        judge_wiki, placeholders, the_same_sentence,
    };

    /// Every text a command carries.
    const EVERY_TEXT: [Text; 4] = Text::ALL;

    /// The texts of one locale, as owned strings a test can break one of.
    #[derive(Clone)]
    struct Written {
        locale: &'static str,
        display_name: Option<String>,
        line: Option<String>,
        paragraph: Option<String>,
        tags: Option<Vec<String>>,
    }

    impl Written {
        fn texts(&self) -> InLocale<'_> {
            InLocale {
                locale: self.locale,
                texts: Texts {
                    display_name: self.display_name.as_deref(),
                    line: self.line.as_deref(),
                    paragraph: self.paragraph.as_deref(),
                    tags: self.tags.as_deref(),
                },
            }
        }

        fn set(&mut self, text: Text, to: Option<&str>) {
            let to = to.map(str::to_owned);
            match text {
                Text::DisplayName => self.display_name = to,
                Text::Line => self.line = to,
                Text::Paragraph => self.paragraph = to,
                Text::Tags => self.tags = to.map(|tag| vec![tag]),
            }
        }
    }

    /// A sound entry, in English and in Brazilian Portuguese.
    fn sound() -> [Written; 2] {
        [
            Written {
                locale: "en",
                display_name: Some("Undo".to_owned()),
                line: Some("Take back your last step.".to_owned()),
                paragraph: Some("Everything the step changed is put back as it was.".to_owned()),
                tags: Some(vec!["undo".to_owned(), "take back".to_owned()]),
            },
            Written {
                locale: "pt-BR",
                display_name: Some("Desfazer".to_owned()),
                line: Some("Desfazer o seu último passo.".to_owned()),
                paragraph: Some("Tudo o que o passo mudou volta a ser como era.".to_owned()),
                tags: Some(vec!["desfazer".to_owned(), "voltar atrás".to_owned()]),
            },
        ]
    }

    /// Judges written locales with no category, no glyph and no exemption.
    fn judged(locales: &[Written]) -> Result<(), Vec<EntryProblem>> {
        judged_with(locales, &[], None, &[])
    }

    fn judged_with(
        locales: &[Written],
        categories: &[String],
        glyph: Option<&str>,
        same_in_both: &[(Text, &str)],
    ) -> Result<(), Vec<EntryProblem>> {
        let in_locales: Vec<InLocale<'_>> = locales.iter().map(Written::texts).collect();
        judge(
            &Entry {
                locales: &in_locales,
                categories,
                glyph,
                same_in_both,
            },
            &Rules {
                requires: &EVERY_TEXT,
                categories: &["noctua.history", "noctua.maintenance"],
            },
        )
    }

    #[test]
    fn a_sound_entry_is_accepted() {
        assert_eq!(judged(&sound()), Ok(()));
    }

    #[test]
    fn an_entry_missing_a_locale_is_refused_naming_the_locale() {
        let mut locales = sound();
        for text in Text::ALL {
            locales[1].set(text, None);
        }
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::LocaleMissing {
                locale: "pt-BR".to_owned()
            }])
        );
    }

    #[test]
    fn each_text_left_empty_is_missing_in_its_locale_and_field() {
        for at in 0..2 {
            for text in [Text::DisplayName, Text::Line, Text::Paragraph] {
                for blank in [
                    None,
                    Some(""),
                    Some("  \t"),
                    Some("\u{3164}"),
                    Some("\u{2063}"),
                ] {
                    let mut locales = sound();
                    locales[at].set(text, blank);
                    let locale = locales[at].locale.to_owned();
                    let problems = judged(&locales).expect_err("a text is empty");
                    assert!(
                        problems.contains(&EntryProblem::Missing { locale, text }),
                        "{blank:?} as the {text} of locale {at} was not missing: {problems:?}"
                    );
                }
            }
            let mut locales = sound();
            locales[at].tags = None;
            let locale = locales[at].locale.to_owned();
            assert_eq!(
                judged(&locales),
                Err(vec![EntryProblem::Missing {
                    locale: locale.clone(),
                    text: Text::Tags
                }])
            );
            locales[at].tags = Some(Vec::new());
            assert_eq!(judged(&locales), Err(vec![EntryProblem::NoTag { locale }]));
        }
    }

    #[test]
    fn each_text_past_its_bound_is_refused_and_one_at_it_is_not() {
        for text in [Text::DisplayName, Text::Line, Text::Paragraph, Text::Tags] {
            for at in 0..2 {
                // Counted in characters: `é` is two bytes and one character.
                let at_the_bound = "é".repeat(text.bound());
                let mut locales = sound();
                locales[at].set(text, Some(&at_the_bound));
                assert_eq!(
                    judged(&locales),
                    Ok(()),
                    "a {text} of exactly {} characters was refused",
                    text.bound()
                );
                let past = "é".repeat(text.bound() + 1);
                locales[at].set(text, Some(&past));
                assert_eq!(
                    judged(&locales),
                    Err(vec![EntryProblem::TooLong {
                        locale: locales[at].locale.to_owned(),
                        text,
                        held: text.bound() + 1
                    }])
                );
            }
        }
    }

    #[test]
    fn a_character_a_reader_sees_nothing_of_is_refused_in_every_text_and_tag() {
        for character in ['\u{202e}', '\u{fe0f}', '\u{e0041}', '\u{200b}', '\u{1b}'] {
            for text in Text::ALL {
                for at in 0..2 {
                    let mut locales = sound();
                    let held = format!("Look{character} here");
                    locales[at].set(text, Some(&held));
                    let problems = judged(&locales).expect_err("a text hides a character");
                    assert!(
                        problems.contains(&EntryProblem::Unseen {
                            locale: locales[at].locale.to_owned(),
                            text,
                            character
                        }),
                        "`U+{:04X}` in the {text} was accepted: {problems:?}",
                        u32::from(character)
                    );
                }
            }
        }
    }

    #[test]
    fn a_portuguese_text_identical_to_the_english_is_refused_unless_exempt_with_a_reason() {
        let mut locales = sound();
        locales[1].display_name = Some("undo.".to_owned());
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::Untranslated {
                locale: "pt-BR".to_owned(),
                text: Text::DisplayName
            }])
        );
        let reason = "the product name, which is the same word in both";
        assert_eq!(
            judged_with(&locales, &[], None, &[(Text::DisplayName, reason)]),
            Ok(())
        );
        assert_eq!(
            judged_with(&locales, &[], None, &[(Text::DisplayName, "same")]),
            Err(vec![EntryProblem::ExemptionUnreasoned(Text::DisplayName)])
        );
        assert_eq!(
            judged_with(&sound(), &[], None, &[(Text::Line, reason)]),
            Err(vec![EntryProblem::ExemptionUnused(Text::Line)])
        );
        // The source locale is the sentence every other one is compared with,
        // and never with itself.
        let mut lone = sound();
        lone[1] = lone[0].clone();
        lone[1].locale = "pt-BR";
        let problems = judged(&lone).expect_err("every pt-BR text is English");
        assert_eq!(problems.len(), 3, "{problems:?}");
    }

    #[test]
    fn a_placeholder_in_a_text_is_refused() {
        for text in [Text::DisplayName, Text::Line, Text::Paragraph] {
            let mut locales = sound();
            locales[1].set(text, Some("Desfazer {passo} agora"));
            assert_eq!(
                judged(&locales),
                Err(vec![EntryProblem::Placeholder {
                    locale: "pt-BR".to_owned(),
                    text,
                    name: "passo".to_owned()
                }])
            );
        }
    }

    #[test]
    fn a_paragraph_that_says_what_the_line_says_is_refused() {
        let mut locales = sound();
        locales[0].paragraph = Some("take back your last step".to_owned());
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::ParagraphRepeatsLine {
                locale: "en".to_owned()
            }])
        );
    }

    #[test]
    fn a_tag_twice_in_one_locale_is_refused_and_one_tag_in_both_locales_is_not() {
        let mut locales = sound();
        locales[1].tags = Some(vec!["Desfazer".to_owned(), "desfazer".to_owned()]);
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::TagTwice {
                locale: "pt-BR".to_owned(),
                tag: "desfazer".to_owned()
            }])
        );
        // One word in both languages is two tags, one in each list.
        let mut shared = sound();
        shared[1].tags = Some(vec!["undo".to_owned()]);
        assert_eq!(judged(&shared), Ok(()));
    }

    #[test]
    fn a_tag_list_is_held_to_its_count_and_each_tag_to_its_shape() {
        let mut locales = sound();
        let many: Vec<String> = (0..=TAGS_MAX).map(|at| format!("tag {at}")).collect();
        locales[0].tags = Some(many[..TAGS_MAX].to_vec());
        assert_eq!(judged(&locales), Ok(()), "{TAGS_MAX} tags were refused");
        locales[0].tags = Some(many);
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::TooManyTags {
                locale: "en".to_owned(),
                held: TAGS_MAX + 1
            }])
        );
        locales[0].tags = Some(vec!["undo".to_owned(), " ".to_owned()]);
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::TagBlank {
                locale: "en".to_owned()
            }])
        );
        locales[0].tags = Some(vec!["undo, redo".to_owned()]);
        assert_eq!(
            judged(&locales),
            Err(vec![EntryProblem::TagMalformed {
                locale: "en".to_owned(),
                tag: "undo, redo".to_owned()
            }])
        );
    }

    #[test]
    fn a_text_its_registry_does_not_carry_is_refused_when_written() {
        let locales = sound();
        let in_locales: Vec<InLocale<'_>> = locales.iter().map(Written::texts).collect();
        let problems = judge(
            &Entry {
                locales: &in_locales,
                categories: &[],
                glyph: None,
                same_in_both: &[],
            },
            &Rules {
                requires: &[Text::DisplayName, Text::Line],
                categories: &[],
            },
        )
        .expect_err("a paragraph and tags are written");
        assert_eq!(
            problems,
            [
                EntryProblem::NotCarried {
                    locale: "en".to_owned(),
                    text: Text::Paragraph
                },
                EntryProblem::NotCarried {
                    locale: "en".to_owned(),
                    text: Text::Tags
                },
                EntryProblem::NotCarried {
                    locale: "pt-BR".to_owned(),
                    text: Text::Paragraph
                },
                EntryProblem::NotCarried {
                    locale: "pt-BR".to_owned(),
                    text: Text::Tags
                },
            ]
        );
    }

    #[test]
    fn a_category_the_registry_does_not_hold_is_refused() {
        let named = |names: &[&str]| -> Vec<String> {
            names.iter().map(|name| (*name).to_owned()).collect()
        };
        assert_eq!(
            judged_with(&sound(), &named(&["noctua.history"]), None, &[]),
            Ok(())
        );
        assert_eq!(
            judged_with(&sound(), &named(&["noctua.everything"]), None, &[]),
            Err(vec![EntryProblem::CategoryUnknown(
                "noctua.everything".to_owned()
            )])
        );
        assert_eq!(
            judged_with(
                &sound(),
                &named(&["noctua.history", "noctua.history"]),
                None,
                &[]
            ),
            Err(vec![EntryProblem::CategoryTwice(
                "noctua.history".to_owned()
            )])
        );
        let past: Vec<String> = (0..=CATEGORIES_MAX)
            .map(|_| "noctua.maintenance".to_owned())
            .collect();
        let problems = judged_with(&sound(), &past, None, &[]).expect_err("too many");
        assert_eq!(
            problems[0],
            EntryProblem::TooManyCategories(CATEGORIES_MAX + 1)
        );
    }

    #[test]
    fn every_problem_is_reported_not_the_first() {
        let mut locales = sound();
        locales[0].display_name = Some("Undo\u{202e}".to_owned());
        locales[1].line = None;
        locales[1].tags = Some(Vec::new());
        let problems = judged_with(
            &locales,
            &["kernel.nothing".to_owned()],
            Some("U+f0e2"),
            &[],
        )
        .expect_err("everything is wrong");
        assert_eq!(
            problems,
            [
                EntryProblem::Unseen {
                    locale: "en".to_owned(),
                    text: Text::DisplayName,
                    character: '\u{202e}'
                },
                EntryProblem::Missing {
                    locale: "pt-BR".to_owned(),
                    text: Text::Line
                },
                EntryProblem::NoTag {
                    locale: "pt-BR".to_owned()
                },
                EntryProblem::CategoryUnknown("kernel.nothing".to_owned()),
                EntryProblem::Glyph(GlyphProblem::Malformed("U+f0e2".to_owned())),
            ]
        );
    }

    #[test]
    fn what_a_refusal_repeats_of_a_tag_is_cut_and_escaped() {
        let long = format!("{}\u{1b}[2J{}", "a".repeat(10), "b".repeat(200));
        let mut locales = sound();
        locales[0].tags = Some(vec![long.clone(), long]);
        let problems = judged(&locales).expect_err("a tag twice");
        let said: String = problems.iter().map(ToString::to_string).collect();
        assert!(
            !said.contains('\u{1b}'),
            "an escape reached the refusal: {said}"
        );
        assert!(said.contains('…'), "a long tag was repeated whole: {said}");
    }

    #[test]
    fn every_problem_reads_as_a_sentence_naming_its_subject() {
        for (problem, names) in [
            (
                EntryProblem::Missing {
                    locale: "pt-BR".to_owned(),
                    text: Text::DisplayName,
                },
                "`pt-BR` display name",
            ),
            (
                EntryProblem::TooLong {
                    locale: "pt-BR".to_owned(),
                    text: Text::Line,
                    held: 88,
                },
                "`pt-BR` line description of 88 characters, past the 80",
            ),
            (
                EntryProblem::Glyph(GlyphProblem::NotInTheFont(0xF382)),
                "`U+F382`",
            ),
            (
                EntryProblem::TagTwice {
                    locale: "en".to_owned(),
                    tag: "undo".to_owned(),
                },
                "`undo` twice",
            ),
        ] {
            let said = problem.to_string();
            assert!(said.contains(names), "`{said}` does not say `{names}`");
        }
    }

    /// The table holds the font's 10,410 points, sorted and disjoint.
    ///
    /// **The number is the font's, measured**: a range dropped, widened or
    /// written twice moves it, and the regeneration command in the table's
    /// documentation is what gives it back.
    #[test]
    fn the_symbols_only_table_holds_the_fonts_ten_thousand_four_hundred_and_ten_points() {
        let points: u32 = SYMBOLS_ONLY
            .iter()
            .map(|(first, last)| last - first + 1)
            .sum();
        assert_eq!(points, 10_410);
        assert_eq!(SYMBOLS_ONLY.len(), 28);
        for pair in SYMBOLS_ONLY.windows(2) {
            assert!(
                pair[0].1 + 1 < pair[1].0,
                "{pair:x?} are out of order, overlap or touch, which a measured charset never \
                 prints"
            );
        }
        for (first, last) in SYMBOLS_ONLY {
            assert!(first <= last, "({first:x}, {last:x}) is empty");
        }
    }

    #[test]
    fn a_glyph_inside_the_font_is_accepted_in_every_plane_it_draws() {
        assert_eq!(glyph("U+F0E2"), Ok('\u{f0e2}'));
        assert_eq!(glyph("U+23FB"), Ok('\u{23fb}'));
        assert_eq!(glyph("U+F0001"), Ok('\u{f0001}'));
        assert_eq!(glyph("U+F1AF0"), Ok('\u{f1af0}'));
    }

    #[test]
    fn a_glyph_outside_the_font_is_refused() {
        for point in [0xF382_u32, 0xF1AF1, 0x23FA, 0xE00B, 0x0041, 0x10_FFFF] {
            assert_eq!(
                glyph(&format!("U+{point:04X}")),
                Err(GlyphProblem::NotInTheFont(point)),
                "`U+{point:04X}` was accepted"
            );
        }
    }

    #[test]
    fn a_glyph_written_any_other_way_is_refused() {
        for written in [
            "f0e2",
            "U+f0e2",
            "u+F0E2",
            "U+0F0E2",
            "U+F0E",
            "U+1000000",
            "U+F0E2 ",
            "\u{f0e2}",
            "U+",
            "",
            "U+-F0E",
            "U++F0E2",
        ] {
            assert!(
                matches!(glyph(written), Err(GlyphProblem::Malformed(_))),
                "`{written}` was not refused as malformed"
            );
        }
        // A refusal states the rule it applied: `U+0F0E2` is upper-case hex of
        // five digits, so a message that said only that would contradict the
        // input it refuses.
        let refused = glyph("U+0F0E2").expect_err("a padded spelling is refused");
        assert!(
            refused.to_string().contains("no leading zero"),
            "the refusal of `U+0F0E2` does not say a leading zero is what it refused: \
             {refused}"
        );
    }

    #[test]
    fn a_wiki_format_is_named_by_its_extension_in_lower_case_only() {
        assert_eq!(
            WikiFormat::from_extension("txt"),
            Some(WikiFormat::PlainText)
        );
        assert_eq!(WikiFormat::from_extension("md"), Some(WikiFormat::Markdown));
        assert_eq!(WikiFormat::from_extension("typ"), Some(WikiFormat::Typst));
        for unknown in ["MD", "Md", "TXT", "rst", "markdown", "", "md "] {
            assert_eq!(
                WikiFormat::from_extension(unknown),
                None,
                "`{unknown}` named a format"
            );
        }
        for format in WikiFormat::ALL {
            assert_eq!(WikiFormat::from_extension(format.extension()), Some(format));
        }
    }

    #[test]
    fn a_wiki_text_is_held_to_its_bytes_and_to_what_a_reader_sees() {
        assert_eq!(judge_wiki("# Undo\n\nIt takes a step back.\n"), Ok(()));
        assert_eq!(judge_wiki(" \n\u{3164}\n"), Err(WikiProblem::Blank));
        let at_the_bound = "a".repeat(WIKI_BYTES_MAX);
        assert_eq!(judge_wiki(&at_the_bound), Ok(()));
        let past = format!("{at_the_bound}a");
        assert_eq!(
            judge_wiki(&past),
            Err(WikiProblem::TooLarge(WIKI_BYTES_MAX + 1))
        );
        for character in ['\r', '\t', '\u{202e}', '\u{1b}', '\u{fe0f}'] {
            assert_eq!(
                judge_wiki(&format!("Undo{character}\n")),
                Err(WikiProblem::Unseen(character)),
                "`U+{:04X}` in a wiki text was accepted",
                u32::from(character)
            );
        }
    }

    #[test]
    fn a_reason_is_prose_from_twenty_characters_and_four_words() {
        assert!(a_reason_is_prose("aaaa bbbb cccc ddddd"));
        assert!(!a_reason_is_prose("a b c ddddddddddddd"));
        assert!(!a_reason_is_prose("aaaaaa bbbbbb cccccc"));
        assert!(!a_reason_is_prose(""));
    }

    #[test]
    fn two_sentences_differing_in_case_spacing_or_ending_are_the_same_one() {
        assert!(the_same_sentence("Undo.", "undo"));
        assert!(the_same_sentence("Take  back", "take back!"));
        assert!(the_same_sentence("Take\u{200d}back", "take back"));
        assert!(the_same_sentence("Undo\u{3164}", "Undo"));
        assert!(!the_same_sentence("Undo", "Desfazer"));
    }

    #[test]
    fn a_sentence_yields_the_names_it_interpolates() {
        let found = placeholders("{field} must be between {min} and {max}.");
        assert_eq!(found.len(), 3);
        assert!(found.contains("field") && found.contains("min") && found.contains("max"));
        assert!(placeholders("Noctua is working.").is_empty());
        assert_eq!(placeholders("{name}, {name}").len(), 1);
        // A brace pair around anything but a bare name is prose.
        assert!(placeholders(r#"such as {"type": "object"}"#).is_empty());
        assert!(placeholders("{}").is_empty());
        assert!(placeholders("{ spaced }").is_empty());
    }
}
