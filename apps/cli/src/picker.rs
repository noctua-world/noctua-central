//! The picker `checkouts` opens at a terminal, as a pure model: keys in, the
//! lines to draw out (`NOCTUA.md §17.1`). [`crate::terminal`] alone reads the
//! keys and draws the lines.
//!
//! **Filtered through `nc_fuzzy`**, the one matcher: each checkout is an item
//! of two names, its digest and its directory, so each word of a query may
//! match a word of either, and every word's positions are drawn highlighted.
//! The empty query lists every checkout in the order given, and the best match
//! is selected each time the query changes.
//!
//! **Every word of a directory is reached, however long it is**: the matcher
//! reads a name to `nc_fuzzy::FIELD_CHARS_MAX` characters, so a longer
//! directory is read again from each word the name before does not hold whole,
//! and a position found there is drawn where it is in the whole directory.
//!
//! **Every text from disk is escaped for the terminal** before it is drawn
//! ([`crate::render::escaped`]): a directory's name is whatever the disk holds.
//! A character drawn as its escape is highlighted whole when it is matched.
//!
//! **Every line fits the width it is drawn at**, counted in characters: a line
//! past it is cut, its last column `…`. The list scrolls to keep the selected
//! checkout in view, and the selection is held to the list as it shrinks.
//!
//! | Key | What it does |
//! |---|---|
//! | a character | adds it to the query, up to `nc_fuzzy::QUERY_CHARS_MAX` |
//! | Backspace | takes the query's last character off |
//! | Up, or Ctrl-P | moves the selection up |
//! | Down, or Ctrl-N | moves the selection down |
//! | Enter | chooses the selected checkout |
//! | Esc, or Ctrl-C | leaves without choosing |
//!
//! # What this does not answer
//!
//! A character a terminal draws two columns wide, which a line counts as
//! one; and a word of a directory longer than `nc_fuzzy::FIELD_CHARS_MAX`
//! characters, which is matched within that many alone.

use nc_fuzzy::{Item, Matcher, Query, Role};

/// One checkout, as the picker shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Its digest.
    pub digest: String,
    /// Its directory, as it records it; nothing for one that records none.
    pub directory: Option<String>,
    /// What is said of it where its directory would be, when it has none.
    pub note: String,
    /// Whether `noctua-dev` reaches it now.
    pub chosen: bool,
}

/// A key, as the picker reads one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A character typed.
    Character(char),
    /// Backspace.
    Backspace,
    /// Up, or Ctrl-P.
    Up,
    /// Down, or Ctrl-N.
    Down,
    /// Enter.
    Enter,
    /// Esc.
    Escape,
    /// Ctrl-C, which raw mode hands over as a key rather than a signal.
    Interrupt,
    /// Anything else, which changes nothing but is drawn again for: a
    /// terminal resized is one.
    Other,
}

/// What a key comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// The picker stays open.
    Stay,
    /// The entry at this index, of those the picker was made with, is chosen.
    Chosen(usize),
    /// The picker is left without a choice.
    Left,
}

/// The field of an item that holds the digest.
const DIGEST: usize = 0;

/// The field of an item that holds the directory, and the first of those that
/// hold it from a later word on.
const DIRECTORY: usize = 1;

/// The line that says what the keys do, drawn above the query.
const KEYS: &str = "Type to filter, Up and Down to move, Enter to choose, Esc to leave";

/// What a highlighted run of a line opens with: bold and underlined.
const HIGHLIGHT: &str = "\u{1b}[1;4m";

/// What a highlighted run of a line closes with.
const PLAIN: &str = "\u{1b}[22;24m";

/// What the selected line opens with: its colours reversed.
const SELECTED: &str = "\u{1b}[7m";

/// What the selected line closes with.
const UNSELECTED: &str = "\u{1b}[27m";

/// One entry the query matches, with each field's positions.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Shown {
    /// The entry's index.
    index: usize,
    /// The positions in its digest, as `char` indices.
    digest: Vec<usize>,
    /// The positions in its directory, as `char` indices.
    directory: Vec<usize>,
}

/// The picker: the entries, the query, what it matches and which is selected.
#[derive(Debug, Clone)]
pub struct Picker {
    /// Every entry, in the order given.
    entries: Vec<Entry>,
    /// Each entry read once into the fields a query is matched against.
    items: Vec<Item>,
    /// Where each entry's directory fields open in its directory, as `char`
    /// indices.
    openings: Vec<Vec<usize>>,
    /// The one matcher, kept between keys.
    matcher: Matcher,
    /// What has been typed.
    query: String,
    /// The entries the query matches, best first.
    shown: Vec<Shown>,
    /// Which of `shown` is selected.
    selected: usize,
}

impl Picker {
    /// A picker over `entries`, with an empty query: every entry, in the order
    /// given, the first selected.
    #[must_use]
    pub fn new(entries: Vec<Entry>) -> Self {
        let mut items = Vec::with_capacity(entries.len());
        let mut openings = Vec::with_capacity(entries.len());
        for entry in &entries {
            let directory = entry.directory.as_deref().unwrap_or_default();
            let opened = opened(directory);
            items.push(Item::new(
                std::iter::once((Role::Name, entry.digest.as_str())).chain(
                    opened
                        .iter()
                        .map(|&(_, byte)| (Role::Name, directory.get(byte..).unwrap_or_default())),
                ),
            ));
            openings.push(opened.into_iter().map(|(at, _)| at).collect());
        }
        let mut picker = Self {
            entries,
            items,
            openings,
            matcher: Matcher::new(),
            query: String::new(),
            shown: Vec::new(),
            selected: 0,
        };
        picker.matched();
        picker
    }

    /// What has been typed.
    #[must_use]
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The index of the selected entry, of those the picker was made with, if
    /// the query matches any.
    #[must_use]
    pub fn selected(&self) -> Option<usize> {
        self.shown.get(self.selected).map(|shown| shown.index)
    }

    /// The indices of the entries the query matches, best first.
    #[must_use]
    pub fn matching(&self) -> Vec<usize> {
        self.shown.iter().map(|shown| shown.index).collect()
    }

    /// What `key` comes to.
    pub fn key(&mut self, key: Key) -> Step {
        match key {
            Key::Character(character) => {
                let mut longer = self.query.clone();
                longer.push(character);
                // **Within the query's bound, or not taken**: a query the
                // matcher refuses would match nothing.
                if Query::parse(&longer).is_ok() {
                    self.query = longer;
                    self.matched();
                }
                Step::Stay
            }
            Key::Backspace => {
                if self.query.pop().is_some() {
                    self.matched();
                }
                Step::Stay
            }
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                Step::Stay
            }
            Key::Down => {
                if self.selected + 1 < self.shown.len() {
                    self.selected += 1;
                }
                Step::Stay
            }
            Key::Enter => self.selected().map_or(Step::Stay, Step::Chosen),
            Key::Escape | Key::Interrupt => Step::Left,
            Key::Other => Step::Stay,
        }
    }

    /// The entries the query matches now, the best selected.
    fn matched(&mut self) {
        self.selected = 0;
        // The query is held to its bound as it is typed, so it always reads.
        let Ok(query) = Query::parse(&self.query) else {
            self.shown.clear();
            return;
        };
        self.shown = self
            .matcher
            .ranked(&query, &self.items)
            .into_iter()
            .map(|ranked| {
                let mut shown = Shown {
                    index: ranked.index(),
                    digest: Vec::new(),
                    directory: Vec::new(),
                };
                for hit in ranked.found().hits() {
                    if hit.field() == DIGEST {
                        shown.digest.extend_from_slice(hit.positions());
                    } else if let Some(&open) =
                        self.openings[ranked.index()].get(hit.field().saturating_sub(DIRECTORY))
                    {
                        shown
                            .directory
                            .extend(hit.positions().iter().map(|at| at + open));
                    }
                }
                shown
            })
            .collect();
    }

    /// The lines to draw at `width` characters by `height` lines: what the keys
    /// do, the query, and as many matching entries as fit, the selected one in
    /// view — or that none matches.
    #[must_use]
    pub fn lines(&self, width: usize, height: usize) -> Vec<String> {
        let mut lines = vec![
            fitted(&plain(KEYS), width),
            fitted(&plain(&format!("> {}", self.query)), width),
        ];
        if self.shown.is_empty() {
            lines.push(fitted(&plain("  no checkout matches the query"), width));
            return lines;
        }
        let rows = height.saturating_sub(lines.len()).max(1);
        let first = (self.selected + 1).saturating_sub(rows);
        for (at, shown) in self.shown.iter().enumerate().skip(first).take(rows) {
            let entry = &self.entries[shown.index];
            let mut cells = plain(if at == self.selected { "› " } else { "  " });
            cells.extend(plain(if entry.chosen { "* " } else { "  " }));
            cells.extend(marked(&entry.digest, &shown.digest));
            cells.extend(plain("  "));
            match &entry.directory {
                Some(directory) => cells.extend(marked(directory, &shown.directory)),
                None => cells.extend(plain(&entry.note)),
            }
            let line = fitted(&cells, width);
            lines.push(if at == self.selected {
                format!("{SELECTED}{line}{UNSELECTED}")
            } else {
                line
            });
        }
        lines
    }
}

/// Where each field `directory` is read as opens, as a `char` index and a byte
/// index: at its start, and at each word the field opened before does not
/// read whole.
///
/// So every word of it up to `nc_fuzzy::FIELD_CHARS_MAX` characters long is
/// read whole by one field. Its words are split where `nc_fuzzy` splits a
/// name's, and at a backslash, which joins a path's names on Windows.
fn opened(directory: &str) -> Vec<(usize, usize)> {
    let mut opened = vec![(0, 0)];
    let mut open = 0;
    let mut word = None;
    // A separator past the last character ends the last word.
    let read = directory.char_indices().chain([(directory.len(), '/')]);
    for (at, (byte, character)) in read.enumerate() {
        let splits = nc_fuzzy::is_separator(character) || character == '\\';
        match word {
            None if !splits => word = Some((at, byte)),
            Some(start) if splits => {
                // A word opening where the last field does is read as far as
                // it can be already.
                if at > open + nc_fuzzy::FIELD_CHARS_MAX && start.0 > open {
                    opened.push(start);
                    open = start.0;
                }
                word = None;
            }
            _ => {}
        }
    }
    opened
}

/// One column of a line: what is drawn there, and whether it is highlighted.
type Cell = (char, bool);

/// `text`, escaped for the terminal, none of it highlighted.
fn plain(text: &str) -> Vec<Cell> {
    crate::render::escaped(text)
        .chars()
        .map(|character| (character, false))
        .collect()
}

/// `text`, each character escaped for the terminal on its own, highlighted
/// where its index is one of `positions`.
fn marked(text: &str, positions: &[usize]) -> Vec<Cell> {
    let mut cells = Vec::new();
    for (at, character) in text.chars().enumerate() {
        let highlighted = positions.contains(&at);
        cells.extend(
            crate::render::escaped(character.encode_utf8(&mut [0; 4]))
                .chars()
                .map(|shown| (shown, highlighted)),
        );
    }
    cells
}

/// `cells` as a line of at most `width` columns: cut, its last column `…`,
/// when longer, and each highlighted run between its two codes.
fn fitted(cells: &[Cell], width: usize) -> String {
    let kept: Vec<Cell> = if cells.len() > width {
        cells
            .iter()
            .take(width.saturating_sub(1))
            .copied()
            .chain((width > 0).then_some(('…', false)))
            .collect()
    } else {
        cells.to_vec()
    };
    let mut line = String::new();
    let mut open = false;
    for (character, highlighted) in kept {
        if highlighted != open {
            line.push_str(if highlighted { HIGHLIGHT } else { PLAIN });
            open = highlighted;
        }
        line.push(character);
    }
    if open {
        line.push_str(PLAIN);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::{Entry, HIGHLIGHT, Key, PLAIN, Picker, SELECTED, Step, UNSELECTED};

    fn entry(digest: &str, directory: Option<&str>, chosen: bool) -> Entry {
        Entry {
            digest: digest.to_owned(),
            directory: directory.map(str::to_owned),
            note: "records no directory: install it again to record one".to_owned(),
            chosen,
        }
    }

    fn three() -> Picker {
        Picker::new(vec![
            entry("0123abcd", Some("/home/ana/noctua-central"), true),
            entry("89efcd45", Some("/home/ana/work/review-copy"), false),
            entry("5e24507c", None, false),
        ])
    }

    /// A line with every code taken out: what a reader sees.
    fn seen(line: &str) -> String {
        let mut out = String::new();
        let mut rest = line;
        while let Some(at) = rest.find('\u{1b}') {
            out.push_str(&rest[..at]);
            let after = &rest[at..];
            let end = after.find('m').expect("a code ends in `m`");
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }

    fn typed(picker: &mut Picker, text: &str) {
        for character in text.chars() {
            assert_eq!(picker.key(Key::Character(character)), Step::Stay);
        }
    }

    /// The empty query lists every checkout in the order given, the first
    /// selected.
    ///
    /// The one `noctua-dev` reaches is marked, and one that records no
    /// directory says so where its directory would be.
    #[test]
    fn the_empty_query_lists_every_checkout_in_the_order_given() {
        let picker = three();
        assert_eq!(picker.matching(), [0, 1, 2]);
        assert_eq!(picker.selected(), Some(0));
        let lines: Vec<String> = picker
            .lines(120, 10)
            .iter()
            .map(|line| seen(line))
            .collect();
        assert_eq!(
            lines,
            [
                "Type to filter, Up and Down to move, Enter to choose, Esc to leave",
                "> ",
                "› * 0123abcd  /home/ana/noctua-central",
                "    89efcd45  /home/ana/work/review-copy",
                "    5e24507c  records no directory: install it again to record one",
            ]
        );
        let drawn = picker.lines(120, 10);
        assert!(
            drawn[2].starts_with(SELECTED) && drawn[2].ends_with(UNSELECTED),
            "{drawn:?}"
        );
        assert!(!drawn[3].contains(SELECTED), "{drawn:?}");
    }

    /// A query filters by any word of a directory or of a digest, the best
    /// match selected, and each word's positions are drawn highlighted.
    #[test]
    fn a_query_filters_by_a_directory_or_a_digest_and_highlights_its_positions() {
        let mut picker = three();
        typed(&mut picker, "review");
        assert_eq!(picker.matching(), [1]);
        assert_eq!(picker.selected(), Some(1));
        let line = &picker.lines(120, 10)[2];
        assert!(
            line.contains(&format!("{HIGHLIGHT}review{PLAIN}")),
            "the word was not drawn highlighted: {line:?}"
        );
        assert_eq!(seen(line), "›   89efcd45  /home/ana/work/review-copy");
        let mut digest = three();
        typed(&mut digest, "5e24");
        assert_eq!(
            digest.matching(),
            [2],
            "a digest is a name the query matches"
        );
        let line = &digest.lines(120, 10)[2];
        assert!(
            line.contains(&format!("{HIGHLIGHT}5e24{PLAIN}")),
            "{line:?}"
        );
        let mut both = three();
        typed(&mut both, "ana central");
        assert_eq!(both.matching(), [0], "every word of the query must match");
        let mut none = three();
        typed(&mut none, "zzz");
        assert_eq!(none.matching(), Vec::<usize>::new());
        assert_eq!(none.selected(), None);
        assert_eq!(none.key(Key::Enter), Step::Stay, "nothing to choose");
        assert_eq!(
            seen(&none.lines(120, 10)[2]),
            "  no checkout matches the query"
        );
        for _ in 0..3 {
            assert_eq!(none.key(Key::Backspace), Step::Stay);
        }
        assert_eq!(none.query(), "");
        assert_eq!(
            none.matching(),
            [0, 1, 2],
            "an emptied query lists them all again"
        );
    }

    /// Up and Down, and Ctrl-P and Ctrl-N as they arrive, move the selection
    /// and stop at either end; Enter chooses it; Esc and Ctrl-C leave.
    #[test]
    fn the_arrows_move_within_the_list_enter_chooses_and_esc_leaves() {
        let mut picker = three();
        assert_eq!(picker.key(Key::Up), Step::Stay);
        assert_eq!(picker.selected(), Some(0), "moved past the top");
        for expected in [1, 2, 2] {
            assert_eq!(picker.key(Key::Down), Step::Stay);
            assert_eq!(picker.selected(), Some(expected));
        }
        assert_eq!(picker.key(Key::Up), Step::Stay);
        assert_eq!(picker.key(Key::Enter), Step::Chosen(1));
        assert_eq!(picker.key(Key::Other), Step::Stay);
        assert_eq!(picker.key(Key::Escape), Step::Left);
        assert_eq!(picker.key(Key::Interrupt), Step::Left);
        // **A query changed selects the best match again**, and the selection
        // is held to the list it shrank to.
        picker.key(Key::Down);
        typed(&mut picker, "8");
        assert_eq!(picker.selected(), Some(1));
        assert_eq!(picker.key(Key::Down), Step::Stay);
        assert_eq!(
            picker.selected(),
            Some(1),
            "moved past the end of a shorter list"
        );
    }

    /// A query is held to the fuzzy core's bound: a character past it is not
    /// taken.
    #[test]
    fn a_query_is_held_to_its_bound() {
        let mut picker = three();
        typed(&mut picker, &"a".repeat(nc_fuzzy::QUERY_CHARS_MAX));
        assert_eq!(picker.key(Key::Character('b')), Step::Stay);
        assert_eq!(picker.query().chars().count(), nc_fuzzy::QUERY_CHARS_MAX);
    }

    /// Every line fits its width, a line past it cut with `…`, highlights
    /// closed, at any width — narrower than a path, and nothing at all.
    #[test]
    fn every_line_fits_its_width_and_a_longer_one_ends_in_an_ellipsis() {
        let mut picker = three();
        typed(&mut picker, "central");
        for width in [0, 1, 2, 5, 12, 20, 40, 200] {
            for line in picker.lines(width, 10) {
                let seen = seen(&line);
                assert!(seen.chars().count() <= width, "{width}: {seen:?}");
                assert_eq!(
                    line.matches(HIGHLIGHT).count(),
                    line.matches(PLAIN).count(),
                    "a highlight left open: {line:?}"
                );
            }
        }
        let cut = seen(&picker.lines(20, 10)[2]);
        assert_eq!(cut, "› * 0123abcd  /home…");
        // **The cut falls inside the match**: the one position kept is still
        // highlighted, and its run closed before the `…`.
        let narrow = &picker.lines(33, 10)[2];
        assert!(
            narrow.contains(&format!("{HIGHLIGHT}c{PLAIN}…")),
            "a position kept within the width is still highlighted: {narrow:?}"
        );
    }

    /// The list scrolls to keep the selection in view, at any height.
    #[test]
    fn the_list_scrolls_to_keep_the_selection_in_view() {
        let mut picker = Picker::new(
            (0..10)
                .map(|at| entry(&format!("0000000{at}"), Some(&format!("/c{at}")), false))
                .collect(),
        );
        for _ in 0..7 {
            picker.key(Key::Down);
        }
        let lines: Vec<String> = picker.lines(80, 5).iter().map(|line| seen(line)).collect();
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[2], "    00000005  /c5");
        assert_eq!(lines[4], "›   00000007  /c7");
        let tiny: Vec<String> = picker.lines(80, 0).iter().map(|line| seen(line)).collect();
        assert_eq!(tiny.last().map(String::as_str), Some("›   00000007  /c7"));
    }

    /// A word of a directory far past what the matcher reads of a name is
    /// matched, and drawn highlighted where it is in the directory.
    ///
    /// **Split as a path's names are split** on Linux and macOS and on
    /// Windows, and wherever in the directory the word lies: past the first
    /// name read, and past the second.
    #[test]
    fn every_word_of_a_directory_longer_than_a_name_is_read_is_matched_in_place() {
        let (a, b) = ("a".repeat(120), "b".repeat(120));
        for directory in [
            format!("/home/ana/{a}/middle-word/{b}/noctua-tail"),
            format!("C:\\Users\\ana\\{a}\\middle-word\\{b}\\noctua-tail"),
        ] {
            assert!(directory.chars().count() > 2 * nc_fuzzy::FIELD_CHARS_MAX);
            for word in ["middle", "tail"] {
                let mut picker = Picker::new(vec![
                    entry("0123abcd", Some(&directory), false),
                    entry("89efcd45", Some("/home/ana/noctua-central"), false),
                ]);
                typed(&mut picker, word);
                assert_eq!(picker.matching(), [0], "{word} in {directory}");
                let line = &picker.lines(1_000, 10)[2];
                let before = directory.find(word).expect("the word is there");
                assert_eq!(
                    line.find(HIGHLIGHT).map(|at| seen(&line[..at])),
                    Some(format!(
                        "›   0123abcd  {}",
                        crate::render::escaped(&directory[..before])
                    )),
                    "{word} was not drawn where it is: {line:?}"
                );
                assert!(
                    line.contains(&format!("{HIGHLIGHT}{word}{PLAIN}")),
                    "{line:?}"
                );
            }
        }
    }

    /// Every text from disk, and every character typed, is escaped for the
    /// terminal, and a character drawn as its escape is highlighted whole.
    #[test]
    fn every_text_from_disk_and_every_key_typed_is_escaped() {
        let mut picker = Picker::new(vec![
            entry("0123abcd", Some("/home/ana/\u{1b}[2J\u{202e}x"), false),
            Entry {
                note: "its record was not read: `/home/\u{1b}[2J`".to_owned(),
                ..entry("89efcd45", None, false)
            },
        ]);
        let refused = picker.lines(200, 10)[3].clone();
        assert!(
            !refused.contains('\u{1b}'),
            "a note reached it raw: {refused:?}"
        );
        let line = picker.lines(200, 10)[2].clone();
        assert!(
            !line.contains("\u{1b}[2J"),
            "a terminal's code reached it: {line:?}"
        );
        assert!(line.contains("\\u{1b}[2J\\u{202e}x"), "{line:?}");
        typed(&mut picker, "\u{202e}");
        assert_eq!(
            picker.matching(),
            [0],
            "the escaped character is matched as typed"
        );
        let drawn = &picker.lines(200, 10);
        assert_eq!(seen(&drawn[1]), "> \\u{202e}");
        assert!(
            drawn[2].contains(&format!("{HIGHLIGHT}\\u{{202e}}{PLAIN}")),
            "{drawn:?}"
        );
    }
}
