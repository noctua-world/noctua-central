//! Where one word lies in one field, and how well.
//!
//! A field is read once, into the folded copy without separators the matcher
//! walks — the *tight* copy — with each tight index's original index beside it
//! and whether it opens a word. A word then lies in it at the best of four
//! tiers: the whole field; the best run of consecutive characters, every
//! occurrence weighed; or, when there is no run, the best scattered alignment a
//! dynamic program finds, its leftmost one among equals.
//!
//! **Tight indices never leave this module.** Every position handed out is
//! mapped back to the field's original text first, so a run that crosses a
//! separator reports the characters a reader sees on either side of it.

use crate::fold::{fold, is_separator, opens_a_word};
use crate::score::{AT_A_START, EXACT, INSIDE, SCATTERED, Score};
use crate::{FIELD_CHARS_MAX, LAST, QUERY_CHARS_MAX};

/// What a field is to the ranking: a name, or a tag.
///
/// At an equal tier a word on a name ranks before the same word on a tag, so a
/// command found by what it is called comes before one found by a synonym.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// What the item is called: its name, or its display name in a language.
    Name,
    /// A word the item is also found by: a tag or a synonym, in any language.
    Tag,
}

/// One thing a list shows, read once into the fields a query is matched against.
///
/// Built once and matched on every keystroke: everything the matcher needs of a
/// field's text is prepared here, and only the first [`FIELD_CHARS_MAX`]
/// characters of each are read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// The fields, in the order they were given — the order a [`crate::Hit`] names them by.
    fields: Vec<Field>,
}

impl Item {
    /// Reads each field's text, with its role.
    #[must_use]
    pub fn new<'a>(fields: impl IntoIterator<Item = (Role, &'a str)>) -> Self {
        Self {
            fields: fields
                .into_iter()
                .map(|(role, text)| Field::read(role, text))
                .collect(),
        }
    }

    /// How many fields the item has.
    #[must_use]
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Whether the item has no field, and so is found only by the empty query.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// The fields, in the order they were given.
    pub(crate) fn fields(&self) -> &[Field] {
        &self.fields
    }
}

/// One field of an item, as the matcher walks it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    /// Whether it is a name or a tag.
    role: Role,
    /// The folded characters read, separators left out.
    tight: Vec<char>,
    /// Each tight index's index in the original text.
    raw: Vec<u8>,
    /// Whether each tight index opens a word of the original text.
    starts: Vec<bool>,
    /// Whether the text was read to its end, which a whole match needs.
    whole: bool,
    /// The field bound less the tight length, which ranks a shorter field first.
    shortness: u16,
}

impl Field {
    /// Reads `text` up to the field bound.
    fn read(role: Role, text: &str) -> Self {
        let mut tight = Vec::new();
        let mut raw = Vec::new();
        let mut starts = Vec::new();
        let mut shortness = u16::from(LAST) + 1;
        let mut previous = None;
        let mut chars = text.chars();
        // The range first, so the character past the bound is left in `chars`
        // for the test below rather than taken and dropped by the zip.
        for (index, c) in (0..=LAST).zip(chars.by_ref()) {
            if !is_separator(c) {
                tight.push(fold(c));
                raw.push(index);
                starts.push(opens_a_word(previous, c));
                shortness -= 1;
            }
            previous = Some(c);
        }
        Self {
            role,
            tight,
            raw,
            starts,
            whole: chars.next().is_none(),
            shortness,
        }
    }

    /// The original indices of tight `positions`.
    pub(crate) fn original(&self, positions: &[usize]) -> Vec<usize> {
        positions
            .iter()
            .map(|&at| usize::from(self.raw[at]))
            .collect()
    }

    /// Word starts, adjacent pairs and earliness of tight `positions`, in that order.
    ///
    /// The quality a word is ranked by within its tier, counted the one way for
    /// every tier, so a score always says what its positions show.
    fn quality(&self, positions: impl IntoIterator<Item = usize>) -> (u16, u16, u16) {
        let mut starts = 0_u16;
        let mut consecutive = 0_u16;
        let mut earliness = 0_u16;
        let mut before: Option<usize> = None;
        for at in positions {
            starts += u16::from(self.starts[at]);
            match before {
                Some(before) => consecutive += u16::from(at == before + 1),
                None => earliness = u16::from(LAST - self.raw[at]),
            }
            before = Some(at);
        }
        (starts, consecutive, earliness)
    }
}

/// A word's quality as one integer: word starts, then adjacent pairs, then earliness.
///
/// A word holds at most [`QUERY_CHARS_MAX`] characters and its first position
/// is at most [`LAST`], so each part has bits of its own and the integer orders
/// as the three do.
fn packed((starts, consecutive, earliness): (u16, u16, u16)) -> u32 {
    u32::from(starts) * START + u32::from(consecutive) * NEXT + u32::from(earliness)
}

/// One adjacent pair's weight in the table: past every earliness.
const NEXT: u32 = 1 << (usize::BITS - (FIELD_CHARS_MAX - 1).leading_zeros());

/// One word start's weight in the table: past every count of adjacent pairs.
const START: u32 = NEXT << (usize::BITS - (QUERY_CHARS_MAX - 1).leading_zeros());

/// The widest quality one word can have, and the one added to its cell, fit the table's integer.
const _: () = assert!(START as usize * (QUERY_CHARS_MAX + 1) <= u32::MAX as usize);

/// Lays `word` in `field` at its best, writing its tight positions into `positions`.
///
/// `None` when the word is not a subsequence of the field, which is found by
/// one pass before anything else is asked; `table` is the scratch the
/// scattered tier fills.
pub(crate) fn aligned(
    field: &Field,
    word: &[char],
    table: &mut Vec<u32>,
    positions: &mut Vec<usize>,
) -> Option<Score> {
    positions.clear();
    if word.is_empty() || !holds(&field.tight, word) {
        return None;
    }
    let tier = if field.whole && field.tight == word {
        positions.extend(0..word.len());
        EXACT
    } else if let Some(at) = best_run(field, word) {
        positions.extend(at..at + word.len());
        if field.starts[at] { AT_A_START } else { INSIDE }
    } else {
        scattered(field, word, table, positions);
        SCATTERED
    };
    Some(Score::of_word(
        tier,
        field.role == Role::Name,
        field.quality(positions.iter().copied()),
        field.shortness,
    ))
}

/// Whether `word` is a subsequence of `tight`, taking each character as early as it comes.
fn holds(tight: &[char], word: &[char]) -> bool {
    let mut wanted = word;
    for c in tight {
        if let Some((first, rest)) = wanted.split_first()
            && first == c
        {
            wanted = rest;
        }
    }
    wanted.is_empty()
}

/// The best occurrence of `word` as a run in `field`, if any.
///
/// Every occurrence is weighed, not the first: one that opens a word ranks
/// above one inside a word wherever it is, then more word starts, then the
/// earlier.
fn best_run(field: &Field, word: &[char]) -> Option<usize> {
    let mut best: Option<(u16, u32, usize)> = None;
    for at in 0..=field.tight.len().saturating_sub(word.len()) {
        if field.tight.get(at..at + word.len()) != Some(word) {
            continue;
        }
        let tier = if field.starts[at] { AT_A_START } else { INSIDE };
        let quality = packed(field.quality(at..at + word.len()));
        if best.is_none_or(|(held, worth, _)| (tier, quality) > (held, worth)) {
            best = Some((tier, quality, at));
        }
    }
    best.map(|(.., at)| at)
}

/// Writes into `positions` the scattered alignment of `word` in `field` with the best quality.
///
/// `table[k * n + i]` holds one more than the best quality the word's
/// characters from `k` on can add when character `k` sits at tight index `i`,
/// or zero when they cannot be laid from there — earliness aside, which only
/// the first character has. It is filled from the last character back, each
/// row reading the next at `i + 1`, which adds an adjacent pair, and at the
/// best cell past it. The first character's place is the best total with its
/// earliness, and every later one the first index that reaches what the cell
/// before it promised: of the alignments with the best quality, the leftmost.
/// The table holds at most [`QUERY_CHARS_MAX`] rows of [`FIELD_CHARS_MAX`]
/// cells, and is the [`crate::Matcher`]'s, kept from one word to the next.
fn scattered(field: &Field, word: &[char], table: &mut Vec<u32>, positions: &mut Vec<usize>) {
    let tight = &field.tight;
    let (n, m) = (tight.len(), word.len());
    table.clear();
    table.resize(n * m, 0);
    let start_at = |i: usize| START * u32::from(field.starts[i]);
    for i in 0..n {
        if tight[i] == word[m - 1] {
            table[(m - 1) * n + i] = start_at(i) + 1;
        }
    }
    for k in (0..m - 1).rev() {
        let next = (k + 1) * n;
        let mut further = 0;
        for i in (0..n).rev() {
            if i + 2 < n {
                further = further.max(table[next + i + 2]);
            }
            if tight[i] != word[k] {
                continue;
            }
            let adjacent = match table.get(next + i + 1) {
                Some(&cell) if i + 1 < n && cell > 0 => cell + NEXT,
                _ => 0,
            };
            let best = adjacent.max(further);
            if best > 0 {
                table[k * n + i] = best + start_at(i);
            }
        }
    }
    let mut first: Option<(u32, usize)> = None;
    for i in 0..n {
        let cell = table[i];
        if cell == 0 {
            continue;
        }
        let total = cell + u32::from(LAST - field.raw[i]);
        if first.is_none_or(|(held, _)| total > held) {
            first = Some((total, i));
        }
    }
    let Some((_, mut at)) = first else {
        return;
    };
    positions.push(at);
    for k in 1..m {
        let promised = table[(k - 1) * n + at] - start_at(at);
        let row = k * n;
        let Some(next) = (at + 1..n).find(|&i| {
            let cell = table[row + i];
            cell > 0 && cell + if i == at + 1 { NEXT } else { 0 } == promised
        }) else {
            return;
        };
        positions.push(next);
        at = next;
    }
}

#[cfg(test)]
mod tests {
    use super::{Field, Role, Score, aligned};
    use crate::FIELD_CHARS_MAX;

    /// The tier and the original positions of `word` laid in a name field holding `text`.
    fn laid(text: &str, word: &str) -> Option<(u16, Vec<usize>)> {
        let field = Field::read(Role::Name, text);
        let word: Vec<char> = word.chars().map(crate::fold).collect();
        let mut positions = Vec::new();
        aligned(&field, &word, &mut Vec::new(), &mut positions)
            .map(|score: Score| (score.tiers(), field.original(&positions)))
    }

    #[test]
    fn an_exact_field_is_the_first_tier() {
        assert_eq!(laid("ver", "ver"), Some((3, vec![0, 1, 2])));
        assert_eq!(
            laid("Configurações", "configuracoes"),
            Some((3, (0..13).collect()))
        );
        // Separators aside: the whole field, read across them.
        assert_eq!(
            laid("button-group", "buttongroup"),
            Some((3, vec![0, 1, 2, 3, 4, 5, 7, 8, 9, 10, 11]))
        );
        // A field holding more is a run, however it starts.
        assert_eq!(laid("verify", "ver").map(|(tier, _)| tier), Some(2));
    }

    #[test]
    fn a_run_at_a_word_start_outranks_a_run_inside_a_word() {
        // `ab` inside `lab` comes first, and the one opening `Ab` is the one kept.
        assert_eq!(laid("lab Ab", "ab"), Some((2, vec![4, 5])));
        assert_eq!(laid("lab", "ab"), Some((1, vec![1, 2])));
        // Among runs that open a word, the one with more word starts, then the earlier.
        assert_eq!(laid("ab ab", "ab"), Some((2, vec![0, 1])));
        assert_eq!(laid("xab aB", "ab"), Some((2, vec![4, 5])));
        // The tier first: the run inside `xaB` has as many word starts, and is
        // earlier, and the run opening `ab` is still the one kept.
        assert_eq!(laid("xaB ab", "ab"), Some((2, vec![4, 5])));
    }

    #[test]
    fn a_run_outranks_a_scattered_match() {
        // `ver` scattered over three word starts, and a run of it inside `sever`.
        assert_eq!(
            laid("Vast Error Report; sever", "ver"),
            Some((1, vec![21, 22, 23]))
        );
        assert_eq!(laid("Vast Error Report", "ver"), Some((0, vec![0, 5, 11])));
    }

    #[test]
    fn among_scattered_matches_more_word_starts_win() {
        assert_eq!(laid("ButtonGroup", "bg"), Some((0, vec![0, 6])));
        // The `g` that opens `Group` is taken over the earlier one inside `Bug`.
        assert_eq!(laid("Bug Group", "bgr"), Some((0, vec![0, 4, 5])));
        // And the first character too: not the earliest `g`, the one that opens a word.
        assert_eq!(laid("Bug Group", "gp"), Some((0, vec![4, 8])));
    }

    #[test]
    fn among_equal_word_starts_more_adjacent_characters_win() {
        // Every alignment has the one start at `a`; only the later `bc` is a pair,
        // and the earlier `b` and `c` would be the leftmost.
        assert_eq!(laid("axbxc xbc", "abc"), Some((0, vec![0, 7, 8])));
    }

    #[test]
    fn a_run_may_cross_a_separator() {
        assert_eq!(laid("button-group", "ongr"), Some((1, vec![4, 5, 7, 8])));
        assert_eq!(laid("journal.verify", "lver"), Some((1, vec![6, 8, 9, 10])));
    }

    #[test]
    fn the_leftmost_of_equal_alignments_is_kept() {
        // `c` at 5 or at 7: each one start, no pair, the same first position.
        assert_eq!(laid("ab x c c", "abc"), Some((0, vec![0, 1, 5])));
        assert_eq!(laid("a x b b", "ab"), Some((0, vec![0, 4])));
    }

    #[test]
    fn a_word_no_field_holds_in_order_is_not_laid() {
        assert_eq!(laid("verify", "yfirev"), None);
        assert_eq!(laid("verify", "verifyx"), None);
        assert_eq!(laid("", "a"), None);
        assert_eq!(laid("---", "a"), None);
        assert_eq!(laid("a", ""), None);
    }

    #[test]
    fn a_score_counts_what_its_positions_show() {
        let field = Field::read(Role::Tag, "Bug Group");
        let word: Vec<char> = "bgr".chars().collect();
        let mut positions = Vec::new();
        let score = aligned(&field, &word, &mut Vec::new(), &mut positions).expect("laid");
        // `B` and `G` open words, `gr` is a pair, the first is at 0, and the
        // field holds eight characters.
        assert_eq!(
            [
                score.tiers(),
                score.names(),
                score.starts(),
                score.consecutive(),
                score.earliness(),
                score.shortness()
            ],
            [0, 0, 2, 1, 127, 120]
        );
        assert_eq!(field.original(&positions), [0, 4, 5]);
        let named = Field::read(Role::Name, "Bug Group");
        let score = aligned(&named, &word, &mut Vec::new(), &mut positions).expect("laid");
        assert_eq!(score.names(), 1);
    }

    #[test]
    fn the_table_reaches_the_bound_in_both_directions() {
        // A word of the query bound scattered over a field of the field bound:
        // the largest table the matcher fills.
        let field: String = "ab".repeat(FIELD_CHARS_MAX / 2);
        let word: String = "a".repeat(crate::QUERY_CHARS_MAX);
        let (tier, positions) = laid(&field, &word).expect("every other character");
        assert_eq!(tier, 0);
        assert_eq!(
            positions,
            (0..FIELD_CHARS_MAX).step_by(2).collect::<Vec<_>>()
        );
    }
}
