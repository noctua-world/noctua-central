//! The fuzzy core held to its corpus, row by row.
//!
//! `fuzzy-corpus.tsv` beside this file is the rule written as data: what each
//! character folds to, how a query splits into words, how a query matches an
//! item and in what order a list comes back. `noctua-design`'s matcher is
//! brought to the same rows when the interface arrives, so the two
//! implementations are each held to one case file and never to each other.
//!
//! The reader is strict on purpose. A row it cannot read fails the suite naming
//! its line, rather than being skipped: a corpus whose malformed rows vanish is
//! a corpus that shrinks while every test stays green.

use nc_fuzzy::{Item, Matcher, Query, Role, fold};

/// The corpus, as the suite reads it.
const CORPUS: &str = include_str!("fuzzy-corpus.tsv");

/// One row: its line in the file, its kind, and its columns after the kind, unescaped.
struct Row {
    /// The row's line, counted from one, for the message that names it.
    line: usize,
    /// The first column: `fold`, `words`, `match` or `order`.
    kind: String,
    /// Every column after the kind.
    columns: Vec<String>,
}

/// Every row of the corpus, each column unescaped and its count held to its kind.
fn rows() -> Vec<Row> {
    let mut rows = Vec::new();
    for (index, text) in CORPUS.lines().enumerate() {
        let line = index + 1;
        if text.starts_with('#') || text.trim().is_empty() {
            continue;
        }
        let mut columns = text.split('\t').map(|column| unescaped(column, line));
        let kind = columns.next().unwrap_or_default();
        let columns: Vec<String> = columns.collect();
        let wanted = match kind.as_str() {
            "fold" | "words" => 2,
            "match" | "order" => 3,
            other => panic!("line {line}: `{other}` is not a kind of row this corpus has"),
        };
        assert_eq!(
            columns.len(),
            wanted,
            "line {line}: a `{kind}` row holds {wanted} columns after its kind"
        );
        rows.push(Row {
            line,
            kind,
            columns,
        });
    }
    rows
}

/// The rows of one kind.
fn of_kind(kind: &str) -> Vec<Row> {
    rows().into_iter().filter(|row| row.kind == kind).collect()
}

/// A column with every escape written out as its character.
///
/// A backslash is read only as a code point in braces; any other is a row the
/// reader cannot read, and fails rather than standing for itself.
fn unescaped(column: &str, line: usize) -> String {
    let mut out = String::new();
    let mut rest = column;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        let escape = &rest[at..];
        let close = escape
            .strip_prefix("\\u{")
            .and_then(|digits| digits.find('}'))
            .unwrap_or_else(|| {
                panic!("line {line}: a backslash opens nothing but a code point in braces")
            });
        let digits = &escape[3..3 + close];
        let character = u32::from_str_radix(digits, 16)
            .ok()
            .and_then(char::from_u32)
            .unwrap_or_else(|| panic!("line {line}: `{digits}` is not a character's code point"));
        out.push(character);
        rest = &escape[3 + close + 1..];
    }
    out.push_str(rest);
    out
}

/// An item from its column: fields joined by `|`, each a name unless it opens with `tag:`.
fn item(column: &str) -> Item {
    Item::new(
        column
            .split('|')
            .map(|field| match field.strip_prefix("tag:") {
                Some(tag) => (Role::Tag, tag),
                None => (Role::Name, field),
            }),
    )
}

/// The positions a hit column writes: indices and inclusive ranges joined by commas.
fn positions(written: &str, line: usize) -> Vec<usize> {
    let number = |text: &str| -> usize {
        text.parse()
            .unwrap_or_else(|_| panic!("line {line}: `{text}` is not a position"))
    };
    let mut out = Vec::new();
    for part in written.split(',') {
        match part.split_once('-') {
            Some((from, to)) => out.extend(number(from)..=number(to)),
            None => out.push(number(part)),
        }
    }
    out
}

/// What a match row expects: each word's field and positions, or `None` for `-`.
fn expected_hits(column: &str, line: usize) -> Option<Vec<(usize, Vec<usize>)>> {
    if column == "-" {
        return None;
    }
    Some(
        column
            .split('|')
            .map(|hit| match hit.split_once(':') {
                Some((field, written)) => (
                    field
                        .parse()
                        .unwrap_or_else(|_| panic!("line {line}: `{field}` is not a field")),
                    positions(written, line),
                ),
                None => (0, positions(hit, line)),
            })
            .collect(),
    )
}

/// A query from its column, which every row writes within the bound.
fn query(column: &str, line: usize) -> Query {
    Query::parse(column).unwrap_or_else(|_| panic!("line {line}: the query is past its bound"))
}

#[test]
fn every_fold_row_folds_as_it_says() {
    for row in of_kind("fold") {
        let (text, folded) = (&row.columns[0], &row.columns[1]);
        assert_eq!(
            text.chars().count(),
            folded.chars().count(),
            "line {}: a fold keeps the length of what it folds",
            row.line
        );
        for (from, to) in text.chars().zip(folded.chars()) {
            assert_eq!(
                fold(from),
                to,
                "line {}: {from:?} (U+{:04X}) folds to {to:?}",
                row.line,
                u32::from(from)
            );
        }
    }
}

#[test]
fn every_words_row_splits_as_it_says() {
    for row in of_kind("words") {
        let found: Vec<String> = query(&row.columns[0], row.line)
            .words()
            .map(|word| word.iter().collect())
            .collect();
        let expected: Vec<String> = match row.columns[1].as_str() {
            "-" => Vec::new(),
            written => written.split('|').map(str::to_owned).collect(),
        };
        assert_eq!(found, expected, "line {}", row.line);
    }
}

#[test]
fn every_match_row_matches_as_it_says() {
    let mut matcher = Matcher::new();
    for row in of_kind("match") {
        let found = matcher
            .matched(&query(&row.columns[0], row.line), &item(&row.columns[1]))
            .map(|found| {
                found
                    .hits()
                    .iter()
                    .map(|hit| (hit.field(), hit.positions().to_vec()))
                    .collect::<Vec<_>>()
            });
        assert_eq!(
            found,
            expected_hits(&row.columns[2], row.line),
            "line {}: `{}` on `{}`",
            row.line,
            row.columns[0],
            row.columns[1]
        );
    }
}

#[test]
fn every_order_row_ranks_as_it_says() {
    let mut matcher = Matcher::new();
    for row in of_kind("order") {
        let items: Vec<Item> = row.columns[1].split(';').map(item).collect();
        let found: Vec<usize> = matcher
            .ranked(&query(&row.columns[0], row.line), &items)
            .iter()
            .map(nc_fuzzy::Ranked::index)
            .collect();
        let expected: Vec<usize> = match row.columns[2].as_str() {
            "-" => Vec::new(),
            written => written
                .split(';')
                .map(|index| {
                    index
                        .parse()
                        .unwrap_or_else(|_| panic!("line {}: `{index}` is not an index", row.line))
                })
                .collect(),
        };
        assert_eq!(found, expected, "line {}: `{}`", row.line, row.columns[0]);
    }
}

#[test]
fn the_corpus_holds_rows_of_every_kind() {
    // Floors, not the numbers: a reader that stopped early, or a kind whose
    // rows all went, fails here rather than passing every test above.
    let rows = rows();
    for (kind, floor) in [("fold", 20), ("words", 10), ("match", 50), ("order", 20)] {
        let count = rows.iter().filter(|row| row.kind == kind).count();
        assert!(
            count >= floor,
            "{count} `{kind}` row(s), and the corpus holds at least {floor}"
        );
    }
    // And the rows the acceptance of the rule names: accents both ways, a
    // Portuguese synonym found through a tag, an English name, a word that
    // matches nothing removing the item, and two words sharing characters.
    let has = |kind: &str, query: &str, expected: &str| {
        rows.iter().any(|row| {
            row.kind == kind
                && row.columns[0] == query
                && row.columns.last().is_some_and(|last| last == expected)
        })
    };
    assert!(has("match", "configuracoes", "0-12"));
    assert!(has("match", "verificar historico", "0-8|12-20"));
    assert!(has("match", "procurar comando", "1:0-7|13-19"));
    assert!(has("order", "verify", "1"));
    assert!(has("match", "verificar zzz", "-"));
    assert!(has("match", "ver verificar", "0-2|0-8"));
}
