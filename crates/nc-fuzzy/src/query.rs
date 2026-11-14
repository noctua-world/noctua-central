//! What a person typed, as the words the matcher looks for.

use core::fmt;

use crate::QUERY_CHARS_MAX;
use crate::fold::{fold, is_separator, is_space};

/// A query read into its words, each folded, with its separators dropped.
///
/// Built only by [`Query::parse`], so every query a [`crate::Matcher`] is
/// handed is within [`QUERY_CHARS_MAX`] — which is what the score's widths are
/// proven against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    /// Each word's folded characters, in the order they were typed.
    words: Vec<Vec<char>>,
}

impl Query {
    /// Reads `text` into its words.
    ///
    /// Words are split at [`is_space`]; a separator inside a word is dropped,
    /// and a word left empty is no word, so `"  - "` is the empty query, which
    /// matches every item.
    ///
    /// # Errors
    ///
    /// [`QueryTooLong`] when `text` holds more than [`QUERY_CHARS_MAX`]
    /// characters. Only that many are counted, so a longer text costs no more
    /// to refuse.
    pub fn parse(text: &str) -> Result<Self, QueryTooLong> {
        if text.chars().nth(QUERY_CHARS_MAX).is_some() {
            return Err(QueryTooLong);
        }
        let words = text
            .split(is_space)
            .map(|word| {
                word.chars()
                    .filter(|c| !is_separator(*c))
                    .map(fold)
                    .collect::<Vec<char>>()
            })
            .filter(|word| !word.is_empty())
            .collect();
        Ok(Self { words })
    }

    /// Each word's folded characters, in the order they were typed.
    pub fn words(&self) -> impl ExactSizeIterator<Item = &[char]> {
        self.words.iter().map(Vec::as_slice)
    }

    /// Whether the query holds no word, and so matches every item.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// A query holding more characters than [`QUERY_CHARS_MAX`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryTooLong;

impl fmt::Display for QueryTooLong {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "a query holds at most {QUERY_CHARS_MAX} characters; shorten it"
        )
    }
}

impl std::error::Error for QueryTooLong {}

#[cfg(test)]
mod tests {
    use super::{QUERY_CHARS_MAX, Query, QueryTooLong};

    /// The words of `text`, as strings.
    fn words(text: &str) -> Vec<String> {
        Query::parse(text)
            .expect("a query within its bound")
            .words()
            .map(|word| word.iter().collect())
            .collect()
    }

    #[test]
    fn a_query_is_split_into_words_at_spaces() {
        assert_eq!(words("verificar histórico"), ["verificar", "historico"]);
        assert_eq!(
            words("a\tb\u{A0}c\u{FEFF}d\u{85}e"),
            ["a", "b", "c", "d", "e"]
        );
        assert_eq!(words("  Configurações   do  "), ["configuracoes", "do"]);
    }

    #[test]
    fn separators_inside_a_word_are_dropped_and_a_word_of_separators_is_no_word() {
        assert_eq!(words("journal.verify"), ["journalverify"]);
        assert_eq!(
            words("button-group a/b c:d e_f"),
            ["buttongroup", "ab", "cd", "ef"]
        );
        assert_eq!(words("verify - ./ :"), ["verify"]);
        let empty = Query::parse(" -_ . ").expect("a query of separators");
        assert!(empty.is_empty());
        assert_eq!(empty.words().len(), 0);
        assert!(Query::parse("").expect("the empty query").is_empty());
    }

    #[test]
    fn a_query_longer_than_its_bound_is_refused() {
        let at = "a".repeat(QUERY_CHARS_MAX);
        let past = "a".repeat(QUERY_CHARS_MAX + 1);
        assert!(Query::parse(&at).is_ok());
        assert_eq!(Query::parse(&past), Err(QueryTooLong));
        // Counted in characters, not bytes: the bound of two-byte letters is
        // accepted.
        assert!(Query::parse(&"é".repeat(QUERY_CHARS_MAX)).is_ok());
        // And in characters, separators included.
        let spaced = format!("{} ", "a".repeat(QUERY_CHARS_MAX));
        assert_eq!(Query::parse(&spaced), Err(QueryTooLong));
        assert_eq!(
            QueryTooLong.to_string(),
            "a query holds at most 64 characters; shorten it"
        );
    }

    #[test]
    fn no_query_within_its_bound_holds_more_words_than_the_score_can_sum() {
        // The densest query of the bound, and one word longer than the bound
        // allows: the first is the widest the score is proven for.
        let densest = vec!["a"; crate::WORDS_MAX].join(" ");
        assert_eq!(words(&densest).len(), crate::WORDS_MAX);
        let one_more = vec!["a"; crate::WORDS_MAX + 1].join(" ");
        assert!(Query::parse(&one_more).is_err());
    }
}
