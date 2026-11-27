//! The reader held to its corpus, row by row.
//!
//! `markdown-corpus.tsv` beside this file is the subset written as data: a
//! source and the tree it is read as, or a source, where it is refused and by
//! which rule. Every construct the subset holds has a row that reads it, and
//! every rule a row that refuses by it — the last is held here, so a rule
//! added without a case fails this suite.
//!
//! The reader of the corpus is strict on purpose. A row it cannot read fails
//! the suite naming its line, rather than being skipped: a corpus whose
//! malformed rows vanish is a corpus that shrinks while every test stays
//! green.

use std::collections::BTreeSet;

use nc_markdown::{Block, Inline, Rule, Target, parse};

/// The corpus, as the suite reads it.
const CORPUS: &str = include_str!("markdown-corpus.tsv");

/// One row: its line in the file, its kind, and its columns after the kind.
struct Row {
    /// The row's line, counted from one, for the message that names it.
    line: usize,
    /// The first column: `accept` or `refuse`.
    kind: &'static str,
    /// The source, unescaped.
    source: String,
    /// Every column after the source, as written.
    columns: Vec<&'static str>,
}

/// `column` with its escapes read: `\n`, `\t`, `\r`, `\\` and `\u{…}`, and no
/// other.
fn unescaped(column: &str, line: usize) -> String {
    let mut out = String::new();
    let mut characters = column.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('\\') => out.push('\\'),
            Some('u') => {
                assert_eq!(
                    characters.next(),
                    Some('{'),
                    "line {line}: `\\u` opens a brace"
                );
                let hex: String = characters
                    .by_ref()
                    .take_while(|digit| *digit != '}')
                    .collect();
                let point = u32::from_str_radix(&hex, 16)
                    .ok()
                    .and_then(char::from_u32)
                    .unwrap_or_else(|| panic!("line {line}: `{hex}` is no code point"));
                out.push(point);
            }
            other => panic!("line {line}: `\\{other:?}` is no escape this corpus has"),
        }
    }
    out
}

/// Every row of the corpus, its column count held to its kind.
fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, text) in CORPUS.lines().enumerate() {
        let line = index + 1;
        if text.starts_with('#') || text.is_empty() {
            continue;
        }
        let mut columns = text.split('\t');
        let kind = columns.next().unwrap_or_default();
        let source = unescaped(columns.next().unwrap_or_default(), line);
        let columns: Vec<&str> = columns.collect();
        let wanted = match kind {
            "accept" => 1,
            "refuse" => 2,
            other => panic!("line {line}: `{other}` is not a kind of row this corpus has"),
        };
        assert_eq!(
            columns.len(),
            wanted,
            "line {line}: a `{kind}` row holds {wanted} column(s) after its source"
        );
        rows.push(Row {
            line,
            kind,
            source,
            columns,
        });
    }
    rows
}

/// `text` in quotation marks, a backslash, a quotation mark and a line feed
/// escaped.
fn quoted(text: &str) -> String {
    format!(
        "\"{}\"",
        text.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

/// The inlines, as the corpus writes a tree.
fn shown_inlines(inlines: &[Inline]) -> String {
    inlines
        .iter()
        .map(|inline| match inline {
            Inline::Text(text) => quoted(text),
            Inline::Code(text) => format!("code{}", quoted(text)),
            Inline::Emphasis(held) => format!("em[{}]", shown_inlines(held)),
            Inline::Strong(held) => format!("strong[{}]", shown_inlines(held)),
            Inline::Link { text, target } => {
                let target = match target {
                    Target::Web(address) => format!("web{}", quoted(address)),
                    Target::Page { path, anchor } => format!(
                        "page{}{}",
                        quoted(path),
                        anchor
                            .as_ref()
                            .map_or_else(String::new, |anchor| format!("#{}", quoted(anchor)))
                    ),
                    Target::Anchor(anchor) => format!("anchor{}", quoted(anchor)),
                };
                format!("link:{target}[{}]", shown_inlines(text))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The blocks, as the corpus writes a tree.
fn shown(blocks: &[Block]) -> String {
    let row = |cells: &[Vec<Inline>]| {
        cells
            .iter()
            .map(|cell| format!("cell[{}]", shown_inlines(cell)))
            .collect::<Vec<_>>()
            .join(" ")
    };
    blocks
        .iter()
        .map(|block| match block {
            Block::Heading { level, text } => format!("h{level}[{}]", shown_inlines(text)),
            Block::Paragraph(text) => format!("p[{}]", shown_inlines(text)),
            Block::Code { info, text } => format!("code:{info}[{}]", quoted(text)),
            Block::List { start, items } => {
                let items = items
                    .iter()
                    .map(|item| format!("li[{}]", shown(item)))
                    .collect::<Vec<_>>()
                    .join(" ");
                start.map_or_else(
                    || format!("ul[{items}]"),
                    |start| format!("ol:{start}[{items}]"),
                )
            }
            Block::Quote(held) => format!("quote[{}]", shown(held)),
            Block::Table { head, rows } => {
                let rows = rows
                    .iter()
                    .map(|cells| format!("row[{}]", row(cells)))
                    .fold(String::new(), |all, one| all + " " + &one);
                format!("table[head[{}]{rows}]", row(head))
            }
            Block::Break => "hr".to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Every `accept` row is read as the tree it writes.
#[test]
fn every_accepted_source_is_read_as_its_tree() {
    let mut read = 0_usize;
    for row in rows().iter().filter(|row| row.kind == "accept") {
        let document = parse(&row.source)
            .unwrap_or_else(|refused| panic!("line {}: refused, {refused}", row.line));
        assert_eq!(
            shown(&document.blocks),
            row.columns.first().copied().unwrap_or_default(),
            "line {}",
            row.line
        );
        read += 1;
    }
    assert!(read >= 40, "only {read} row(s) were read");
}

/// Every `refuse` row is refused where it says, by the rule it names.
#[test]
fn every_refused_source_is_refused_where_and_why_it_says() {
    let mut refused = 0_usize;
    for row in rows().iter().filter(|row| row.kind == "refuse") {
        let Err(why) = parse(&row.source) else {
            panic!("line {}: read, and the row refuses it", row.line);
        };
        let said = format!("{}:{}", why.line, why.column);
        assert_eq!(
            (said.as_str(), why.rule.word()),
            (
                row.columns.first().copied().unwrap_or_default(),
                row.columns.get(1).copied().unwrap_or_default()
            ),
            "line {}",
            row.line
        );
        refused += 1;
    }
    assert!(
        refused >= Rule::ALL.len(),
        "only {refused} row(s) were read"
    );
}

/// Every rule has a row that refuses by it, and no row names a rule there is
/// not.
#[test]
fn every_rule_has_a_row_that_refuses_by_it() {
    let named: BTreeSet<&str> = rows()
        .iter()
        .filter(|row| row.kind == "refuse")
        .filter_map(|row| row.columns.get(1).copied())
        .collect();
    let rules: BTreeSet<&str> = Rule::ALL.iter().map(|rule| rule.word()).collect();
    assert_eq!(named, rules);
}

/// Whatever is read is read again as itself once its lines end in a line
/// feed more: a final line feed changes no tree.
#[test]
fn a_final_line_feed_changes_no_tree() {
    for row in rows().iter().filter(|row| row.kind == "accept") {
        let once = parse(&row.source).expect("held above");
        let ended = parse(&format!("{}\n", row.source)).expect("a line feed more");
        assert_eq!(once, ended, "line {}", row.line);
    }
}

/// Where the link `[a](written)` leads, and none where its target is refused
/// — by the target's own rule, at the parenthesis that opens it.
fn led(written: &str) -> Option<Target> {
    match parse(&format!("[a]({written})")) {
        Ok(document) => match document.blocks.as_slice() {
            [Block::Paragraph(inlines)] => match inlines.as_slice() {
                [Inline::Link { target, .. }] => Some(target.clone()),
                other => panic!("{written}: read as {other:?}"),
            },
            other => panic!("{written}: read as {other:?}"),
        },
        Err(refused) => {
            assert_eq!(
                (refused.line, refused.column, refused.rule),
                (1, 4, Rule::Target),
                "{written}"
            );
            None
        }
    }
}

/// A link leads to one of the three forms, and `https` is the only scheme.
///
/// **Each form, and every way out of each**: an address of another scheme,
/// with no host, or holding a character a target may not; a path that is
/// absolute, hidden, empty in a segment, of another ending, or holding a
/// space or a drive; and an anchor that is empty or not lower case.
#[test]
fn a_link_leads_to_one_of_three_forms_and_to_nothing_else() {
    assert_eq!(
        led("https://example.org/a?b=1"),
        Some(Target::Web("https://example.org/a?b=1".to_owned()))
    );
    assert_eq!(
        led("../catalog/wiki/pt-BR/commands/kernel.undo.md#redo"),
        Some(Target::Page {
            path: "../catalog/wiki/pt-BR/commands/kernel.undo.md".to_owned(),
            anchor: Some("redo".to_owned()),
        })
    );
    assert_eq!(
        led("#when-it-stops"),
        Some(Target::Anchor("when-it-stops".to_owned()))
    );
    for ending in nc_markdown::PAGE {
        let path = format!("wiki/a{ending}");
        assert_eq!(
            led(&path),
            Some(Target::Page {
                path: path.clone(),
                anchor: None
            })
        );
    }
    for refused in [
        "",
        "#",
        "#Redo",
        "http://example.org",
        "https://",
        "https:///x",
        "https://a b",
        "https://a\"b",
        "https://a<b",
        "javascript:alert",
        "data:text/html,x",
        "file:///etc/passwd",
        "mailto:a@b.c",
        "/etc/passwd.md",
        "a.html",
        "a.md.exe",
        "a/.hidden.md",
        "a//b.md",
        "a b.md",
        "c:/a.md",
        "a.md#",
        "a.md#A",
        "...md",
        "a/.../b.md",
    ] {
        assert_eq!(led(refused), None, "{refused}");
    }
}
