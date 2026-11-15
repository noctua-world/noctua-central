//! The one fuzzy matcher every list filtered by typing goes through.
//!
//! `NOCTUA.md §12.4` asks for one behaviour and one implementation: the daemon
//! filters with it for the CLI and MCP, and the interface for every list. It
//! knows nothing of commands or of languages. A caller builds an [`Item`] from
//! its fields — a command's name, its display names and its tags in both
//! languages — each with its [`Role`], and a [`Matcher`] answers which field each
//! word of a [`Query`] matched, and where.
//!
//! # The rule
//!
//! - Characters are compared folded, one to one ([`fold()`]): lower case first,
//!   then the base letter, so `configuracoes` finds `Configurações`.
//! - A query is split into words at spaces ([`is_space`]). A separator inside a
//!   word ([`is_separator`]) is dropped, and no separator in a field is ever
//!   matched. An empty query matches every item, in the order given.
//! - Every word must match some field as a subsequence of its characters, or the
//!   item is not found. Words are independent: each picks its own field and its
//!   own positions, two words may claim the same character, and the order they
//!   are typed in changes neither what is found nor its score.
//! - A word lies in a field at one of four tiers, highest first: the whole
//!   field, a run of consecutive characters opening a word of the field, a run
//!   anywhere else, and scattered.
//! - Ranking compares, in order: the tier, the role ([`Role::Name`] before
//!   [`Role::Tag`]), the characters on word starts, the consecutive characters,
//!   how early the word begins and how short the field is — each summed over the
//!   words ([`Score`]). Equal scores keep the order the items were given in.
//!
//! # The coordinates
//!
//! A position is a `char` index into the original text of the field its [`Hit`]
//! names — what `chars()` counts in Rust and what spreading a string counts in
//! JavaScript. Folding keeps every index, and the copy without separators that
//! the matcher walks maps each of its own indices back before one is returned:
//! two strings of different lengths are two coordinate systems.
//!
//! # What holds it
//!
//! `crates/nc-fuzzy/tests/fuzzy-corpus.tsv` is the rule written as rows — every
//! fold, split, match and ranking it decides — and this crate's suite replays
//! each row. `noctua-design`'s matcher is brought to the same rows when the
//! interface arrives (`NOCTUA-PLAN.md`, Stage 16), so the two cannot drift
//! apart while each passes its own tests.
//!
//! # The security answer `NOCTUA.md §2.4` asks for
//!
//! A query is matched in bounded work, and an item's text is read up to its
//! bound. A query holds at most [`QUERY_CHARS_MAX`] characters and so at most
//! [`WORDS_MAX`] words, refused past that before anything is matched; a field is
//! read to [`FIELD_CHARS_MAX`] characters and no further, however long its text
//! is; and a scattered word costs one table of at most the product of the two,
//! kept in the [`Matcher`] and reused. There is no regular expression, no I/O,
//! no `unsafe` and no system `cfg`, and every width the score is packed into is
//! proven in `const` from those bounds, so a score is an integer JSON carries
//! exactly. What it does not answer is in `AGENTS.md`, under "What is
//! deliberately not claimed".

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
mod align;
mod fold;
mod query;
mod rank;
mod score;

pub use align::{Item, Role};
pub use fold::{fold, is_separator, is_space};
pub use query::{Query, QueryTooLong};
pub use rank::{Hit, Match, Matcher, Ranked};
pub use score::Score;

/// The most characters a query may hold, its spaces and separators included.
///
/// A bound on the work one query asks for. A command that reads a query
/// declares this as its `maxLength`, and a test beside it holds the two equal.
pub const QUERY_CHARS_MAX: usize = 64;

/// The most words a query can hold, which follows from its bound.
///
/// Implied and never refused on its own: two words need a space between them,
/// so a query of [`QUERY_CHARS_MAX`] characters holds at most half that many,
/// rounded up. The score's widths are summed over this many words.
pub const WORDS_MAX: usize = QUERY_CHARS_MAX.div_ceil(2);

/// How many characters of a field the matcher reads.
///
/// The rest of a longer text is never read: no match, position or score
/// depends on it, and a field cut there is never matched whole. A command that
/// matches a registry's texts holds each of their bounds within this one, with
/// a test beside it.
pub const FIELD_CHARS_MAX: usize = 128;

/// The last index a field's read characters can have.
///
/// One less than [`FIELD_CHARS_MAX`], held so in `const` below: a field's
/// positions are kept in a `u8`, and a bound past what that holds fails the
/// build rather than wrapping.
const LAST: u8 = 127;

/// The field bound is the one `LAST` counts to, so `0..=LAST` reads that many characters.
const _: () = assert!(LAST as usize + 1 == FIELD_CHARS_MAX);

#[cfg(test)]
mod tests {
    use super::{FIELD_CHARS_MAX, Item, Matcher, QUERY_CHARS_MAX, Query, Role, WORDS_MAX};

    #[test]
    fn the_word_bound_follows_from_the_query_bound() {
        // The densest query is one letter, a space, one letter: a query of the
        // bound holds half of it, rounded up, and one more character than an
        // even bound is one more word.
        let densest = vec!["a"; WORDS_MAX].join(" ");
        assert!(densest.chars().count() <= QUERY_CHARS_MAX);
        let query = Query::parse(&densest).expect("a query within its bound");
        assert_eq!(query.words().len(), WORDS_MAX);
        assert_eq!(WORDS_MAX, 32);
        // One word more needs two characters more than the bound allows.
        let past = vec!["a"; WORDS_MAX + 1].join(" ");
        assert!(past.chars().count() > QUERY_CHARS_MAX);
    }

    #[test]
    fn a_field_is_read_to_its_bound() {
        let mut matcher = Matcher::new();
        // The last character read, and the first one past it.
        let read = format!("{}z", "a".repeat(FIELD_CHARS_MAX - 1));
        let past = format!("{}z", "a".repeat(FIELD_CHARS_MAX));
        let z = Query::parse("z").expect("a query");
        let found = matcher
            .matched(&z, &Item::new([(Role::Name, read.as_str())]))
            .expect("the last character read is matched");
        assert_eq!(found.hits()[0].positions(), [FIELD_CHARS_MAX - 1]);
        assert_eq!(
            matcher.matched(&z, &Item::new([(Role::Name, past.as_str())])),
            None,
            "a character past the bound is never read"
        );
    }

    #[test]
    fn a_field_cut_at_its_bound_is_never_matched_whole() {
        // Its first word fills the bound exactly and the field goes on: the run
        // is the best the matcher can say, because the whole field was not read.
        let mut matcher = Matcher::new();
        let word = "a".repeat(QUERY_CHARS_MAX);
        let query = Query::parse(&word).expect("a query of the bound");
        let whole = Item::new([(Role::Name, word.as_str())]);
        let longer = format!("{word}{}", " ".repeat(FIELD_CHARS_MAX - QUERY_CHARS_MAX));
        let cut = format!("{longer}b");
        let exact = matcher.matched(&query, &whole).expect("a whole field");
        let read = matcher
            .matched(&query, &Item::new([(Role::Name, longer.as_str())]))
            .expect("a field read to its end");
        let cut = matcher
            .matched(&query, &Item::new([(Role::Name, cut.as_str())]))
            .expect("a field cut at its bound");
        assert_eq!(exact.score().tiers(), 3);
        assert_eq!(
            read.score().tiers(),
            3,
            "trailing spaces read whole are no part of a word"
        );
        assert_eq!(
            cut.score().tiers(),
            2,
            "a field cut at its bound is a run, never the whole"
        );
    }
}
