//! The one reader of Markdown: a closed subset, read into a typed tree.
//!
//! `NOCTUA.md §12.16` has the interface show help and manual pages, and
//! `§20.4` lets a registry entry carry a wiki text written in Markdown. Both
//! are read by this crate and by nothing else: the catalog compiler reads
//! every Markdown text it compiles through [`parse`], so a text outside the
//! subset fails the build naming its file, line and column, and the interface
//! draws the tree it answers, never a text.
//!
//! # The subset
//!
//! A document is blocks, **each ended by a blank line** or the end of its
//! container, so no line continues a block lazily and none interrupts one.
//!
//! | Block | Written |
//! |---|---|
//! | heading | one to six `#`, one space, its text; no closing `#` |
//! | paragraph | lines one after another, read as joined by one space |
//! | fenced code | three backticks or more, then a word of [`INFO`]; closed by a line of as many backticks or more; its lines kept as written |
//! | list | every item `- `, or every item a number, a full stop and a space, counted one by one from the first; an item's further lines begin under its text, and its first paragraph may be followed at once by a list under it |
//! | block quote | every line `>` alone, or `> ` and its text |
//! | table | a row of `\|`-bounded cells, a row of `---` cells, then rows of as many cells; `\\\|` writes a bar inside a cell |
//! | thematic break | exactly `---` |
//!
//! A list item and a block quote hold blocks in turn, at most [`DEPTH_MAX`]
//! deep. A comment — one whole line, `<!--` to `-->` — is read and dropped
//! before the first block, and nowhere else.
//!
//! | Inline | Written |
//! |---|---|
//! | code | a run of backticks, closed by a run as long; one space of padding at both ends is dropped |
//! | emphasis | `*text*` |
//! | strong | `**text**` |
//! | link | `[text](target)`, the target one of [`Target`]'s three forms |
//! | escape | a backslash before a character of ASCII punctuation |
//! | entity | `&lt;`, `&gt;` and `&amp;`, and no other |
//!
//! Emphasis may hold strong and strong emphasis, neither itself; a link holds
//! no link. `_` is text, and is refused where another reader would open
//! emphasis with it, so a page reads the same wherever it is rendered.
//!
//! # What is outside it
//!
//! Raw HTML, an autolink, an image, a reference link, a setext heading, an
//! indented code block, a tilde fence, a `*` or `+` bullet, a `1)` item, a
//! table's alignment colons, a hard line break, a tab or any other control
//! character, a line ending in a space, and an entity not listed above. Each
//! is refused with a [`Rule`] of its own, at the line and the column it was
//! read at ([`Refused`]) — never read as text, since the same source would
//! then be one page here and another wherever Markdown is read whole.
//!
//! # The coordinates
//!
//! A line is counted from one, and a column from one, in characters — what
//! `chars()` counts — so a position names the same place whatever a line
//! holds before it.
//!
//! # What holds it
//!
//! `crates/nc-markdown/tests/markdown-corpus.tsv` is the subset written as
//! rows: a source and the tree it is read as, or a source and where it is
//! refused and by which rule. This crate's suite replays every row, and holds
//! every [`Rule`] to a row that refuses by it.
//!
//! # The security answer `NOCTUA.md §2.4` asks for
//!
//! No markup of a text's own reaches a page: there is no raw HTML to pass
//! through, a link's target is one of three closed forms — `https` the only
//! scheme, so no `javascript:`, `data:` or `file:` target is ever read as one
//! — and what the tree holds is text for its reader to escape. The work is
//! bounded by the text: containers nest at most [`DEPTH_MAX`] deep, so the
//! reader's own depth is bounded, and no inline nests in itself. There is no
//! regular expression, no I/O, no `unsafe`, no dependency and no system
//! `cfg`. **What it does not answer:** how long a text may be, which is its
//! caller's bound — a wiki text's 64 KiB, a page's by the build that embeds it
//! — and what a `https` link leads to, which is its reader's to decide before
//! it opens anything.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]

use core::fmt;

/// How deep a list item or a block quote may sit inside others.
pub const DEPTH_MAX: usize = 8;

/// The most `#` a heading opens with.
pub const HEADING_MAX: usize = 6;

/// Every word a fenced block may name after its fence, the empty one first:
/// what the block holds, for a reader that sets it apart.
pub const INFO: [&str; 6] = ["", "json", "rust", "sh", "text", "toml"];

/// The only scheme a link's target may name.
pub const SCHEME: &str = "https://";

/// What a link to another text ends in, before its anchor: Markdown, plain
/// text or Typst — the formats a registry's wiki text is written in
/// (`NOCTUA.md §20.4`), which a generated page links.
pub const PAGE: [&str; 3] = [".md", ".txt", ".typ"];

/// A document: its blocks, in the order written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    /// The blocks.
    pub blocks: Vec<Block>,
}

/// One block of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A heading.
    Heading {
        /// From 1 to [`HEADING_MAX`].
        level: usize,
        /// What it says.
        text: Vec<Inline>,
    },
    /// A paragraph.
    Paragraph(Vec<Inline>),
    /// A fenced block of code.
    Code {
        /// The word after its fence: one of [`INFO`].
        info: &'static str,
        /// Its lines as written, each ended by a line feed.
        text: String,
    },
    /// A list.
    List {
        /// The number its first item is written with, for an ordered list.
        start: Option<u32>,
        /// Its items, each the blocks it holds.
        items: Vec<Vec<Self>>,
    },
    /// A block quote, and the blocks it holds.
    Quote(Vec<Self>),
    /// A table.
    Table {
        /// Its header row, cell by cell.
        head: Vec<Vec<Inline>>,
        /// Its rows, each as many cells as the header.
        rows: Vec<Vec<Vec<Inline>>>,
    },
    /// A thematic break.
    Break,
}

/// One inline of a block's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    /// Text, its escapes and entities read.
    Text(String),
    /// A code span, as written between its backticks.
    Code(String),
    /// Emphasis.
    Emphasis(Vec<Self>),
    /// Strong emphasis.
    Strong(Vec<Self>),
    /// A link.
    Link {
        /// What it shows.
        text: Vec<Self>,
        /// Where it leads.
        target: Target,
    },
}

/// Where a link leads: one of three closed forms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// An address of the web, opening with [`SCHEME`], whole.
    Web(String),
    /// Another text, by a relative path ending in one of [`PAGE`].
    Page {
        /// The path, as written.
        path: String,
        /// The heading of that page it leads to, if it names one.
        anchor: Option<String>,
    },
    /// A heading of this page.
    Anchor(String),
}

/// Why a source is not of the subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rule {
    /// A tab, a carriage return or another control character.
    Control,
    /// A line ending in a space.
    Trailing,
    /// A line beginning with a space where no list item continues.
    Indent,
    /// Raw HTML, an autolink, or a comment after the first block.
    Html,
    /// A fence naming a word outside [`INFO`], or written with tildes.
    Info,
    /// A fenced block never closed.
    Fence,
    /// A heading of another shape.
    Heading,
    /// A thematic break written any way but `---`.
    Break,
    /// A line of `=` or of `-` under a paragraph.
    Setext,
    /// A block quote's line of another shape, or a quote holding nothing.
    Quote,
    /// A list marker of another kind.
    Marker,
    /// A list item holding nothing.
    Item,
    /// An ordered item not numbered one past the item before it.
    Numbering,
    /// A table of another shape.
    Table,
    /// A table's second row holding anything but `---` cells.
    Delimiter,
    /// A table's row of another number of cells than its header.
    Cells,
    /// A block not ended by a blank line.
    Joined,
    /// A list item or a block quote too deep inside others.
    Deep,
    /// A backslash before anything but ASCII punctuation.
    Escape,
    /// A code span never closed.
    Code,
    /// A run of three `*` or more.
    Stars,
    /// A `*` or `**` that opens nothing, or is never closed.
    Star,
    /// Emphasis inside emphasis, or strong inside strong.
    Nested,
    /// A `_` another reader would open emphasis with.
    Underscore,
    /// A `[` or a `]` that is no link's.
    Bracket,
    /// A link's target of no form [`Target`] holds.
    Target,
    /// An image.
    Image,
    /// An entity other than `&lt;`, `&gt;` and `&amp;`.
    Entity,
}

impl Rule {
    /// Every rule, in the order declared.
    pub const ALL: [Self; 28] = [
        Self::Control,
        Self::Trailing,
        Self::Indent,
        Self::Html,
        Self::Info,
        Self::Fence,
        Self::Heading,
        Self::Break,
        Self::Setext,
        Self::Quote,
        Self::Marker,
        Self::Item,
        Self::Numbering,
        Self::Table,
        Self::Delimiter,
        Self::Cells,
        Self::Joined,
        Self::Deep,
        Self::Escape,
        Self::Code,
        Self::Stars,
        Self::Star,
        Self::Nested,
        Self::Underscore,
        Self::Bracket,
        Self::Target,
        Self::Image,
        Self::Entity,
    ];

    /// The one word the rule is named by, in the corpus and in a build's
    /// refusal.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Trailing => "trailing",
            Self::Indent => "indent",
            Self::Html => "html",
            Self::Info => "info",
            Self::Fence => "fence",
            Self::Heading => "heading",
            Self::Break => "break",
            Self::Setext => "setext",
            Self::Quote => "quote",
            Self::Marker => "marker",
            Self::Item => "item",
            Self::Numbering => "numbering",
            Self::Table => "table",
            Self::Delimiter => "delimiter",
            Self::Cells => "cells",
            Self::Joined => "joined",
            Self::Deep => "deep",
            Self::Escape => "escape",
            Self::Code => "code",
            Self::Stars => "stars",
            Self::Star => "star",
            Self::Nested => "nested",
            Self::Underscore => "underscore",
            Self::Bracket => "bracket",
            Self::Target => "target",
            Self::Image => "image",
            Self::Entity => "entity",
        }
    }

    /// What was read and what to write instead, as one sentence.
    #[must_use]
    pub const fn said(self) -> &'static str {
        match self {
            Self::Control => {
                "a tab, a carriage return or another control character: write spaces, and end \
                 each line with a line feed alone"
            }
            Self::Trailing => "the line ends in a space: take it away",
            Self::Indent => {
                "the line begins with a space where no list item continues: begin it at its \
                 block's edge, and write code in a fenced block"
            }
            Self::Html => {
                "raw HTML: write `&lt;` for the character, set a name in a code span, and keep \
                 a comment to the lines before the first block"
            }
            Self::Info => {
                "the fence names a kind of code this subset does not hold, or is written with \
                 tildes: open it with backticks and nothing, `json`, `rust`, `sh`, `text` or \
                 `toml`"
            }
            Self::Fence => {
                "the fenced block is never closed: end it with a line of as many backticks as \
                 opened it"
            }
            Self::Heading => {
                "a heading is one to six `#`, one space and its text, with no `#` closing it"
            }
            Self::Break => "a thematic break is exactly `---`",
            Self::Setext => {
                "a line of `=` or of `-` under a paragraph: write a heading with `#`, and put \
                 a blank line before a thematic break"
            }
            Self::Quote => {
                "a block quote's line is `>` alone or `> ` and its text, and a quote holds \
                 something"
            }
            Self::Marker => {
                "a list item opens with `- `, or with a number, a full stop and a space"
            }
            Self::Item => "the list item holds nothing: write its text after the marker",
            Self::Numbering => {
                "an ordered item is numbered one past the item before it: write the next number"
            }
            Self::Table => {
                "a table is a header row, a row of `---` cells and its rows, each opening and \
                 ending with `|`"
            }
            Self::Delimiter => {
                "a table's second row holds only cells of three `-` or more, with no colon"
            }
            Self::Cells => {
                "a table's row holds as many cells as its header: write the missing \
                            ones, or escape a bar inside a cell as `\\|`"
            }
            Self::Joined => {
                "a block ends at a blank line: put one before this line, or begin the line \
                 under its list item's text"
            }
            Self::Deep => "a list item or a block quote sits too deep inside others: flatten it",
            Self::Escape => {
                "a backslash escapes ASCII punctuation alone: write `\\\\` for a backslash"
            }
            Self::Code => {
                "the code span is never closed: close it with as many backticks as opened it"
            }
            Self::Stars => "three `*` or more in a row: write `*` for emphasis and `**` for strong",
            Self::Star => {
                "the `*` opens no emphasis that is closed: close it, or write `\\*` for the \
                 character"
            }
            Self::Nested => {
                "emphasis inside emphasis, or strong inside strong: write the inner one plain"
            }
            Self::Underscore => {
                "the `_` would open emphasis elsewhere: write emphasis with `*`, and `\\_` for \
                 the character"
            }
            Self::Bracket => {
                "the bracket is no link's: write a link as `[text](target)`, and `\\[` or \
                 `\\]` for the character"
            }
            Self::Target => {
                "the link leads nowhere this subset names: write an `https://` address, a \
                 relative path ending in `.md`, `.txt` or `.typ`, or `#` and a heading's name"
            }
            Self::Image => "an image: this subset holds none, so link to it or describe it",
            Self::Entity => {
                "an entity other than `&lt;`, `&gt;` and `&amp;`: write the character itself"
            }
        }
    }
}

/// A source refused: where, and by which rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused {
    /// The line, counted from one.
    pub line: usize,
    /// The column, counted from one, in characters.
    pub column: usize,
    /// The rule the source breaks there.
    pub rule: Rule,
}

impl fmt::Display for Refused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "line {}, column {}: {}",
            self.line,
            self.column,
            self.rule.said()
        )
    }
}

impl core::error::Error for Refused {}

/// One line of a container: what is left of it past the container's own
/// marks, and where that begins.
#[derive(Debug, Clone, Copy)]
struct Line<'a> {
    /// The line's number in the source, from one.
    number: usize,
    /// The column `text` begins at, from one.
    column: usize,
    /// What the container holds of the line.
    text: &'a str,
}

impl<'a> Line<'a> {
    /// The line past its first `marks` characters, every one of them ASCII.
    fn past(self, marks: usize) -> Self {
        Self {
            number: self.number,
            column: self.column + marks,
            text: self.text.get(marks..).unwrap_or(""),
        }
    }

    /// The refusal of this line by `rule`, `offset` characters into it.
    const fn refused(self, offset: usize, rule: Rule) -> Refused {
        Refused {
            line: self.number,
            column: self.column + offset,
            rule,
        }
    }

    /// Every character of the line, with where it is.
    fn characters(self) -> impl Iterator<Item = Character> + 'a {
        self.text
            .chars()
            .enumerate()
            .map(move |(offset, written)| Character {
                written,
                line: self.number,
                column: self.column + offset,
            })
    }

    /// Whether the line is nothing but `mark`, at least once.
    fn is_only(self, mark: char) -> bool {
        !self.text.is_empty() && self.text.chars().all(|character| character == mark)
    }
}

/// One character of a block's text, and where it was written.
#[derive(Debug, Clone, Copy)]
struct Character {
    /// The character.
    written: char,
    /// Its line.
    line: usize,
    /// Its column.
    column: usize,
}

impl Character {
    /// The refusal of this character by `rule`.
    const fn refused(self, rule: Rule) -> Refused {
        Refused {
            line: self.line,
            column: self.column,
            rule,
        }
    }
}

/// Reads `source` as a document of the subset.
///
/// # Errors
///
/// [`Refused`], naming the line, the column and the [`Rule`], for the first
/// thing in `source` that is outside the subset.
pub fn parse(source: &str) -> Result<Document, Refused> {
    let lines: Vec<Line<'_>> = source
        .split('\n')
        .enumerate()
        .map(|(index, text)| Line {
            number: index + 1,
            column: 1,
            text,
        })
        .collect();
    for line in &lines {
        if let Some(offset) = line.text.chars().position(char::is_control) {
            return Err(line.refused(offset, Rule::Control));
        }
        if line.text.ends_with(' ') {
            return Err(line.refused(line.text.chars().count() - 1, Rule::Trailing));
        }
    }
    Ok(Document {
        blocks: blocks(&lines, 0, false)?,
    })
}

/// A list item's marker: its kind, and how many characters it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Marker {
    /// `- `.
    Bullet,
    /// A number, a full stop and a space.
    Number {
        /// The number.
        number: u32,
        /// The characters of the whole marker.
        width: usize,
    },
}

impl Marker {
    /// How many characters the marker takes, the space after it included.
    const fn width(self) -> usize {
        match self {
            Self::Bullet => 2,
            Self::Number { width, .. } => width,
        }
    }

    /// Whether `other` is a marker of the same list.
    const fn continues(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Bullet, Self::Bullet) | (Self::Number { .. }, Self::Number { .. })
        )
    }
}

/// The marker `text` opens with, if it opens a list item.
///
/// # Errors
///
/// [`Rule::Marker`] for a marker of a kind the subset does not hold, and
/// [`Rule::Item`] for one with nothing after it.
fn marker(text: &str) -> Result<Option<Marker>, Rule> {
    if text.starts_with("- ") {
        return Ok(Some(Marker::Bullet));
    }
    if text == "-" {
        return Err(Rule::Item);
    }
    if ["* ", "+ "].iter().any(|other| text.starts_with(other)) || text == "*" || text == "+" {
        return Err(Rule::Marker);
    }
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    let rest = text.get(digits..).unwrap_or("");
    if digits == 0 {
        return Ok(None);
    }
    if rest.starts_with(") ") || rest == ")" {
        return Err(Rule::Marker);
    }
    if rest == "." {
        return Err(Rule::Item);
    }
    if !rest.starts_with(". ") {
        return Ok(None);
    }
    text.get(..digits)
        .and_then(|number| number.parse().ok())
        .filter(|_| digits <= 9)
        .map(|number| {
            Some(Marker::Number {
                number,
                width: digits + 2,
            })
        })
        .ok_or(Rule::Marker)
}

/// Whether `line` would begin a block of its own, which a paragraph's
/// further line may not.
fn begins_a_block(line: Line<'_>) -> bool {
    ["#", ">", "|", "```", "~~~", "<!--", " "]
        .iter()
        .any(|mark| line.text.starts_with(mark))
        || !matches!(marker(line.text), Ok(None))
}

/// The blocks `lines` hold, `depth` containers deep, in a list item where
/// `item`.
///
/// **A block ends at a blank line, but for one case**: in a list item, a
/// paragraph may be followed at once by the list under it, as an outline is
/// written.
///
/// # Errors
///
/// The first [`Refused`] of what the lines hold.
fn blocks(lines: &[Line<'_>], depth: usize, item: bool) -> Result<Vec<Block>, Refused> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(&line) = lines.get(at) {
        if line.text.is_empty() {
            at += 1;
            continue;
        }
        if depth > DEPTH_MAX {
            return Err(line.refused(0, Rule::Deep));
        }
        if line.text.starts_with("<!--") {
            if depth == 0 && out.is_empty() && is_a_comment(line.text) {
                at += 1;
                continue;
            }
            return Err(line.refused(0, Rule::Html));
        }
        let (block, next) = block(lines, at, line, depth, item)?;
        let outlined = item && matches!(block, Block::Paragraph(_));
        out.push(block);
        at = next;
        if let Some(joined) = lines.get(at).filter(|after| !after.text.is_empty())
            && !(outlined && opens_an_item(*joined))
        {
            return Err(joined.refused(0, Rule::Joined));
        }
    }
    Ok(out)
}

/// Whether `text`, which opens a comment, is one whole comment and nothing
/// more.
fn is_a_comment(text: &str) -> bool {
    text.strip_suffix("-->")
        .and_then(|open| open.strip_prefix("<!--"))
        .is_some_and(|inside| !inside.contains("--"))
}

/// The block beginning with `line`, which is `lines[at]` and not blank, and
/// the index of the first line after it.
///
/// # Errors
///
/// The [`Refused`] of the block, by the rule of its kind.
fn block(
    lines: &[Line<'_>],
    at: usize,
    line: Line<'_>,
    depth: usize,
    item: bool,
) -> Result<(Block, usize), Refused> {
    let text = line.text;
    if text.starts_with(' ') {
        return Err(line.refused(0, Rule::Indent));
    }
    if text.starts_with("```") {
        return fence(lines, at, line);
    }
    if text.starts_with("~~~") {
        return Err(line.refused(0, Rule::Info));
    }
    if text.starts_with('#') {
        return heading(line).map(|heading| (heading, at + 1));
    }
    if text == "---" {
        return Ok((Block::Break, at + 1));
    }
    if line.is_only('=') {
        return Err(line.refused(0, Rule::Setext));
    }
    if (line.is_only('-') && text.len() >= 2)
        || ((line.is_only('*') || line.is_only('_')) && text.len() >= 3)
    {
        return Err(line.refused(0, Rule::Break));
    }
    if text.starts_with('>') {
        return quote(lines, at, line, depth);
    }
    match marker(text) {
        Ok(Some(first)) => return list(lines, at, first, depth),
        Ok(None) => {}
        Err(rule) => return Err(line.refused(0, rule)),
    }
    if text.starts_with('|') {
        return table(lines, at, line);
    }
    paragraph(lines, at, item)
}

/// The fenced block `open`, which is `lines[at]`, opens.
fn fence(lines: &[Line<'_>], at: usize, open: Line<'_>) -> Result<(Block, usize), Refused> {
    let ticks = open.text.bytes().take_while(|byte| *byte == b'`').count();
    let named = open.text.get(ticks..).unwrap_or("");
    let Some(info) = INFO.iter().find(|info| **info == named) else {
        return Err(open.refused(ticks, Rule::Info));
    };
    let mut text = String::new();
    let mut next = at + 1;
    loop {
        let Some(line) = lines.get(next) else {
            return Err(open.refused(0, Rule::Fence));
        };
        next += 1;
        if line.text.len() >= ticks && line.is_only('`') {
            return Ok((Block::Code { info, text }, next));
        }
        text.push_str(line.text);
        text.push('\n');
    }
}

/// The heading `line` writes.
fn heading(line: Line<'_>) -> Result<Block, Refused> {
    let level = line.text.bytes().take_while(|byte| *byte == b'#').count();
    if level > HEADING_MAX {
        return Err(line.refused(0, Rule::Heading));
    }
    let rest = line.past(level);
    if !rest.text.starts_with(' ') {
        return Err(line.refused(level, Rule::Heading));
    }
    let said = rest.past(1);
    if said.text.starts_with(' ') || said.text.ends_with('#') {
        return Err(said.refused(0, Rule::Heading));
    }
    let characters: Vec<Character> = said.characters().collect();
    Ok(Block::Heading {
        level,
        text: inlines(&characters, Within::default())?,
    })
}

/// The block quote `open`, which is `lines[at]`, opens.
fn quote(
    lines: &[Line<'_>],
    at: usize,
    open: Line<'_>,
    depth: usize,
) -> Result<(Block, usize), Refused> {
    let mut held = Vec::new();
    let mut next = at;
    while let Some(&line) = lines.get(next).filter(|line| line.text.starts_with('>')) {
        let rest = line.past(1);
        if rest.text.is_empty() {
            held.push(rest);
        } else if rest.text.starts_with(' ') {
            held.push(rest.past(1));
        } else {
            return Err(rest.refused(0, Rule::Quote));
        }
        next += 1;
    }
    let inside = blocks(&held, depth + 1, false)?;
    if inside.is_empty() {
        return Err(open.refused(0, Rule::Quote));
    }
    Ok((Block::Quote(inside), next))
}

/// The index of the first line at or after `from` that is not blank.
fn next_written(lines: &[Line<'_>], from: usize) -> Option<usize> {
    lines
        .get(from..)?
        .iter()
        .position(|line| !line.text.is_empty())
        .map(|offset| from + offset)
}

/// The list opening at `lines[at]` with the marker `first`.
fn list(
    lines: &[Line<'_>],
    at: usize,
    first: Marker,
    depth: usize,
) -> Result<(Block, usize), Refused> {
    let under = " ".repeat(first.width());
    let mut items = Vec::new();
    let mut marked = first;
    let mut next = at;
    loop {
        let mut held: Vec<Line<'_>> = lines
            .get(next)
            .map(|line| line.past(marked.width()))
            .into_iter()
            .collect();
        next += 1;
        // The item's further lines: each under its text, a blank line among
        // them only where a line under its text follows.
        while let Some(written) = next_written(lines, next) {
            let Some(line) = lines
                .get(written)
                .filter(|line| line.text.starts_with(&under))
            else {
                break;
            };
            held.extend(lines.get(next..written).unwrap_or(&[]));
            held.push(line.past(under.len()));
            next = written + 1;
        }
        items.push(blocks(&held, depth + 1, true)?);
        // The next item: a marker of this list's kind, after any blank lines.
        let Some((written, line)) =
            next_written(lines, next).and_then(|written| Some((written, *lines.get(written)?)))
        else {
            break;
        };
        let Ok(Some(following)) = marker(line.text) else {
            break;
        };
        if !marked.continues(following) {
            break;
        }
        if let (Marker::Number { number, .. }, Marker::Number { number: after, .. }) =
            (marked, following)
            && number.checked_add(1) != Some(after)
        {
            return Err(line.refused(0, Rule::Numbering));
        }
        marked = following;
        next = written;
    }
    let start = match first {
        Marker::Bullet => None,
        Marker::Number { number, .. } => Some(number),
    };
    Ok((Block::List { start, items }, next))
}

/// The cells of the table row `line` writes, each trimmed of the spaces
/// around it, a bar escaped inside a cell read as the bar.
fn cells(line: Line<'_>) -> Result<Vec<Vec<Character>>, Refused> {
    let written: Vec<Character> = line.characters().collect();
    let mut row = Vec::new();
    let mut cell: Vec<Character> = Vec::new();
    let mut at = 1;
    while let Some(&here) = written.get(at) {
        let after = written.get(at + 1).copied();
        match (here.written, after) {
            ('\\', Some(bar)) if bar.written == '|' => {
                cell.push(bar);
                at += 2;
            }
            ('\\', Some(escaped)) if escaped.written.is_ascii_punctuation() => {
                cell.extend([here, escaped]);
                at += 2;
            }
            ('|', _) => {
                let from = cell
                    .iter()
                    .position(|held| held.written != ' ')
                    .unwrap_or(cell.len());
                let to = cell
                    .iter()
                    .rposition(|held| held.written != ' ')
                    .map_or(from, |last| last + 1);
                row.push(cell.get(from..to).unwrap_or(&[]).to_vec());
                cell.clear();
                at += 1;
            }
            _ => {
                cell.push(here);
                at += 1;
            }
        }
    }
    match cell.first() {
        Some(open) => Err(open.refused(Rule::Table)),
        None if row.is_empty() => Err(line.refused(0, Rule::Table)),
        None => Ok(row),
    }
}

/// The table `open`, which is `lines[at]`, opens.
fn table(lines: &[Line<'_>], at: usize, open: Line<'_>) -> Result<(Block, usize), Refused> {
    let mut written = Vec::new();
    let mut next = at;
    while let Some(&line) = lines.get(next).filter(|line| line.text.starts_with('|')) {
        written.push((line, cells(line)?));
        next += 1;
    }
    let (Some((_, head)), Some((under, delimiter))) = (written.first(), written.get(1)) else {
        return Err(open.refused(0, Rule::Table));
    };
    for cell in delimiter {
        if cell.len() < 3 || cell.iter().any(|held| held.written != '-') {
            return Err(cell.first().map_or_else(
                || under.refused(0, Rule::Delimiter),
                |open| open.refused(Rule::Delimiter),
            ));
        }
    }
    if let Some((line, _)) = written.iter().find(|(_, row)| row.len() != head.len()) {
        return Err(line.refused(0, Rule::Cells));
    }
    let read = |row: &Vec<Vec<Character>>| {
        row.iter()
            .map(|cell| inlines(cell, Within::default()))
            .collect::<Result<Vec<_>, _>>()
    };
    let head = read(head)?;
    let rows = written
        .iter()
        .skip(2)
        .map(|(_, row)| read(row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((Block::Table { head, rows }, next))
}

/// Whether `line` opens a list item of the subset.
fn opens_an_item(line: Line<'_>) -> bool {
    matches!(marker(line.text), Ok(Some(_)))
}

/// The paragraph opening at `lines[at]`: every line to the next blank one —
/// or, in a list item where `item`, to the list under it — read as joined by
/// one space.
fn paragraph(lines: &[Line<'_>], at: usize, item: bool) -> Result<(Block, usize), Refused> {
    let mut characters: Vec<Character> = Vec::new();
    let mut next = at;
    while let Some(&line) = lines.get(next).filter(|line| !line.text.is_empty()) {
        if next > at && item && opens_an_item(line) {
            break;
        }
        if next > at {
            if line.is_only('=') || line.is_only('-') {
                return Err(line.refused(0, Rule::Setext));
            }
            if begins_a_block(line) {
                return Err(line.refused(0, Rule::Joined));
            }
            if let Some(last) = characters.last().copied() {
                characters.push(Character {
                    written: ' ',
                    line: last.line,
                    column: last.column + 1,
                });
            }
        }
        characters.extend(line.characters());
        next += 1;
    }
    Ok((
        Block::Paragraph(inlines(&characters, Within::default())?),
        next,
    ))
}

/// Which inlines the text being read is already inside.
#[derive(Debug, Clone, Copy, Default)]
struct Within {
    /// Inside emphasis.
    emphasis: bool,
    /// Inside strong emphasis.
    strong: bool,
}

/// How many `mark` stand in a row at `from`.
fn run(characters: &[Character], from: usize, mark: char) -> usize {
    characters
        .get(from..)
        .unwrap_or(&[])
        .iter()
        .take_while(|held| held.written == mark)
        .count()
}

/// Where the run of exactly `ticks` backticks that closes a code span opened
/// before `from` begins.
fn closing_ticks(characters: &[Character], from: usize, ticks: usize) -> Option<usize> {
    let mut at = from;
    while let Some(held) = characters.get(at) {
        if held.written == '`' {
            let found = run(characters, at, '`');
            if found == ticks {
                return Some(at);
            }
            at += found;
        } else {
            at += 1;
        }
    }
    None
}

/// The index past whatever is written at `at` that a search for a closing
/// mark steps over whole: an escape, or a code span.
fn stepped_over(characters: &[Character], at: usize) -> usize {
    match characters.get(at).map(|held| held.written) {
        Some('\\') => at + 2,
        Some('`') => {
            let ticks = run(characters, at, '`');
            closing_ticks(characters, at + ticks, ticks).map_or(at + ticks, |close| close + ticks)
        }
        _ => at + 1,
    }
}

/// Where the run of exactly `stars` asterisks that closes emphasis opened
/// before `from` begins: one with something other than a space before it.
fn closing_stars(characters: &[Character], from: usize, stars: usize) -> Option<usize> {
    let mut at = from;
    while let Some(held) = characters.get(at) {
        if held.written == '*' {
            let found = run(characters, at, '*');
            let closes = at > from
                && at
                    .checked_sub(1)
                    .and_then(|before| characters.get(before))
                    .is_some_and(|before| before.written != ' ');
            if found == stars && closes {
                return Some(at);
            }
            at += found;
        } else {
            at = stepped_over(characters, at);
        }
    }
    None
}

/// Where the `]` that closes a link's text opened before `from` stands; none
/// where another `[` opens first.
fn closing_bracket(characters: &[Character], from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(held) = characters.get(at) {
        match held.written {
            ']' => return Some(at),
            '[' => return None,
            _ => at = stepped_over(characters, at),
        }
    }
    None
}

/// What a code span holds: what is written between its backticks, one space
/// of padding dropped from both ends where both have one and more than
/// spaces is held.
fn span(characters: &[Character]) -> String {
    let held: String = characters.iter().map(|held| held.written).collect();
    held.strip_prefix(' ')
        .and_then(|rest| rest.strip_suffix(' '))
        .filter(|_| held.chars().any(|character| character != ' '))
        .map_or_else(|| held.clone(), str::to_owned)
}

/// Whether `name` is a heading's name as an anchor writes it: lower-case
/// letters, digits and hyphens.
fn is_a_slug(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// Where a link written with the target `written` leads, if it is one of the
/// three forms.
fn target(written: &str) -> Option<Target> {
    if let Some(address) = written.strip_prefix(SCHEME) {
        let plain = |character: char| {
            character.is_ascii_graphic()
                && !matches!(character, '<' | '>' | '"' | '\'' | '\\' | '`' | '(' | ')')
        };
        return (address.starts_with(|first: char| first.is_ascii_alphanumeric())
            && address.chars().all(plain))
        .then(|| Target::Web(written.to_owned()));
    }
    let (path, anchor) = match written.split_once('#') {
        Some((path, anchor)) => (path, Some(anchor)),
        None => (written, None),
    };
    if anchor.is_some_and(|anchor| !is_a_slug(anchor)) {
        return None;
    }
    if path.is_empty() {
        return anchor.map(|anchor| Target::Anchor(anchor.to_owned()));
    }
    let named = |segment: &str| {
        // Two full stops, and nothing else: the directory above.
        (segment.len() == 2 && segment.bytes().all(|byte| byte == b'.'))
            || (segment.starts_with(|first: char| first.is_ascii_alphanumeric())
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-')))
    };
    let ends = PAGE.iter().any(|ending| path.ends_with(ending));
    (ends && path.split('/').all(named)).then(|| Target::Page {
        path: path.to_owned(),
        anchor: anchor.map(str::to_owned),
    })
}

/// The entity written at `at`, where an `&` stands: the character it is and
/// how many characters write it, none where no entity is written there.
///
/// # Errors
///
/// [`Rule::Entity`] for an entity the subset does not hold.
fn entity(characters: &[Character], at: usize) -> Result<Option<(char, usize)>, Rule> {
    let name: String = characters
        .get(at + 1..)
        .unwrap_or(&[])
        .iter()
        .map(|held| held.written)
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '#')
        .collect();
    let ended = characters
        .get(at + 1 + name.len())
        .is_some_and(|held| held.written == ';');
    if name.is_empty() || !ended {
        return Ok(None);
    }
    let character = match name.as_str() {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        _ => return Err(Rule::Entity),
    };
    Ok(Some((character, name.len() + 2)))
}

/// The emphasis or the strong emphasis the `*` at `at`, which is `here`,
/// opens, and the index past its closing run.
///
/// # Errors
///
/// [`Rule::Stars`] for a run of three or more, [`Rule::Nested`] for one
/// inside its own kind, [`Rule::Star`] for one that opens nothing or is never
/// closed, and what its text is refused by.
fn starred(
    characters: &[Character],
    at: usize,
    here: Character,
    within: Within,
) -> Result<(Inline, usize), Refused> {
    let stars = run(characters, at, '*');
    if stars > 2 {
        return Err(here.refused(Rule::Stars));
    }
    let strong = stars == 2;
    if (strong && within.strong) || (!strong && within.emphasis) {
        return Err(here.refused(Rule::Nested));
    }
    let opens = characters
        .get(at + stars)
        .is_some_and(|held| held.written != ' ');
    let Some(close) = closing_stars(characters, at + stars, stars).filter(|_| opens) else {
        return Err(here.refused(Rule::Star));
    };
    let held = characters.get(at + stars..close).unwrap_or(&[]);
    let inline = if strong {
        Inline::Strong(inlines(held, Within { strong, ..within })?)
    } else {
        Inline::Emphasis(inlines(
            held,
            Within {
                emphasis: true,
                ..within
            },
        )?)
    };
    Ok((inline, close + stars))
}

/// The link the `[` at `at`, which is `here`, opens, and the index past its
/// target's `)`.
///
/// # Errors
///
/// [`Rule::Bracket`] for a bracket that is no link's — never closed before
/// another opens, so no link holds a link, holding nothing, or with no `(`
/// after it — [`Rule::Target`]
/// for a target of no form the subset holds or never closed, and what its
/// text is refused by.
fn linked(
    characters: &[Character],
    at: usize,
    here: Character,
    within: Within,
) -> Result<(Inline, usize), Refused> {
    let close = closing_bracket(characters, at + 1)
        .filter(|close| *close > at + 1)
        .filter(|close| {
            characters
                .get(close + 1)
                .is_some_and(|held| held.written == '(')
        });
    let Some(close) = close else {
        return Err(here.refused(Rule::Bracket));
    };
    let opened = close + 1;
    let written: String = characters
        .get(opened + 1..)
        .unwrap_or(&[])
        .iter()
        .map(|held| held.written)
        .take_while(|character| *character != ')')
        .collect();
    let end = opened + 1 + written.chars().count();
    let Some(target) = target(&written).filter(|_| characters.get(end).is_some()) else {
        return Err(characters.get(opened).map_or_else(
            || here.refused(Rule::Target),
            |open| open.refused(Rule::Target),
        ));
    };
    let held = characters.get(at + 1..close).unwrap_or(&[]);
    Ok((
        Inline::Link {
            text: inlines(held, within)?,
            target,
        },
        end + 1,
    ))
}

/// The inlines `characters` write, read `within` what already holds them.
///
/// # Errors
///
/// The first [`Refused`] of what the characters write.
fn inlines(characters: &[Character], within: Within) -> Result<Vec<Inline>, Refused> {
    let mut out = Vec::new();
    let mut text = String::new();
    let mut at = 0;
    while let Some(&here) = characters.get(at) {
        let after = characters.get(at + 1).map(|held| held.written);
        let before = at
            .checked_sub(1)
            .and_then(|before| characters.get(before))
            .map(|held| held.written);
        let inline = match here.written {
            '\\' => {
                let Some(escaped) = after.filter(char::is_ascii_punctuation) else {
                    return Err(here.refused(Rule::Escape));
                };
                text.push(escaped);
                at += 2;
                continue;
            }
            '`' => {
                let ticks = run(characters, at, '`');
                let Some(close) = closing_ticks(characters, at + ticks, ticks) else {
                    return Err(here.refused(Rule::Code));
                };
                let held = characters.get(at + ticks..close).unwrap_or(&[]);
                at = close + ticks;
                Inline::Code(span(held))
            }
            '*' => {
                let (emphasis, next) = starred(characters, at, here, within)?;
                at = next;
                emphasis
            }
            '[' => {
                let (link, next) = linked(characters, at, here, within)?;
                at = next;
                link
            }
            ']' => return Err(here.refused(Rule::Bracket)),
            '!' if after == Some('[') => return Err(here.refused(Rule::Image)),
            '<' if after.is_some_and(|next| {
                next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?')
            }) =>
            {
                return Err(here.refused(Rule::Html));
            }
            '_' if before.is_none_or(|before| matches!(before, ' ' | '('))
                && after.is_some_and(|next| next != ' ') =>
            {
                return Err(here.refused(Rule::Underscore));
            }
            '&' => {
                let (character, written) = entity(characters, at)
                    .map_err(|rule| here.refused(rule))?
                    .unwrap_or(('&', 1));
                text.push(character);
                at += written;
                continue;
            }
            character => {
                text.push(character);
                at += 1;
                continue;
            }
        };
        if !text.is_empty() {
            out.push(Inline::Text(core::mem::take(&mut text)));
        }
        out.push(inline);
    }
    if !text.is_empty() {
        out.push(Inline::Text(text));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Block, DEPTH_MAX, INFO, Inline, Refused, Rule, parse};

    /// What `source` is refused by, and where.
    fn refused(source: &str) -> (usize, usize, Rule) {
        let Refused { line, column, rule } = parse(source).expect_err(source);
        (line, column, rule)
    }

    /// Every rule has a word of its own and a sentence of its own, and `ALL`
    /// holds each once.
    #[test]
    fn every_rule_is_named_once_and_says_what_to_write() {
        let mut words: Vec<&str> = Rule::ALL.iter().map(|rule| rule.word()).collect();
        let mut said: Vec<&str> = Rule::ALL.iter().map(|rule| rule.said()).collect();
        words.sort_unstable();
        words.dedup();
        said.sort_unstable();
        said.dedup();
        assert_eq!(words.len(), Rule::ALL.len());
        assert_eq!(said.len(), Rule::ALL.len());
        for rule in Rule::ALL {
            assert!(
                rule.word().bytes().all(|byte| byte.is_ascii_lowercase()),
                "{rule:?}"
            );
            assert!(rule.said().len() > rule.word().len(), "{rule:?}");
            assert!(!rule.said().ends_with('.'), "{rule:?}");
        }
    }

    /// A refusal is said with its line, its column and its rule's sentence.
    #[test]
    fn a_refusal_names_the_line_and_the_column() {
        let refusal = parse("# Title\n\ntext <b>").expect_err("raw HTML");
        assert_eq!(
            (refusal.line, refusal.column, refusal.rule),
            (3, 6, Rule::Html)
        );
        assert_eq!(
            refusal.to_string(),
            format!("line 3, column 6: {}", Rule::Html.said())
        );
    }

    /// A column is counted in characters, never in bytes.
    #[test]
    fn a_column_counts_characters() {
        assert_eq!(refused("ação <b>"), (1, 6, Rule::Html));
        assert_eq!(refused("é "), (1, 2, Rule::Trailing));
        assert_eq!(refused("- ação\n  é <b>"), (2, 5, Rule::Html));
    }

    /// Containers nest to the bound and are refused past it.
    #[test]
    fn containers_nest_to_the_bound_and_no_further() {
        let nested = |depth: usize| format!("{}text", "> ".repeat(depth));
        let mut held = parse(&nested(DEPTH_MAX)).expect("at the bound").blocks;
        for _ in 0..DEPTH_MAX {
            let [Block::Quote(inside)] = held.as_slice() else {
                panic!("{held:?}");
            };
            held = inside.clone();
        }
        assert_eq!(
            held,
            [Block::Paragraph(vec![Inline::Text("text".to_owned())])]
        );
        assert_eq!(
            refused(&nested(DEPTH_MAX + 1)),
            (1, 2 * (DEPTH_MAX + 1) + 1, Rule::Deep)
        );
    }

    /// Every word of the closed list opens a fence, and no other does.
    #[test]
    fn a_fence_names_a_word_of_the_list_and_no_other() {
        for info in INFO {
            let read = parse(&format!("```{info}\na\n```")).expect(info);
            assert_eq!(
                read.blocks,
                [Block::Code {
                    info,
                    text: "a\n".to_owned()
                }]
            );
        }
        for other in ["js", "JSON", "json ", "text x", "html"] {
            let source = format!("```{other}\na\n```");
            let rule = parse(&source).expect_err(other).rule;
            assert!(matches!(rule, Rule::Info | Rule::Trailing), "{other}");
        }
    }
}
