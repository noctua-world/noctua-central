//! How a match ranks, and the integer it packs into.
//!
//! A word's score is six components, compared in the order [`Score`] declares
//! them, and an item's is the sum of its words', component by component. Each
//! component is packed into bits of its own, as wide as its largest sum needs,
//! so the packed integer orders exactly as the components do: no number of word
//! starts lifts a match past one whose tier is higher, by construction rather
//! than by a constant chosen large enough.

use crate::{FIELD_CHARS_MAX, QUERY_CHARS_MAX, WORDS_MAX};

/// The tier of a word that is the whole field.
pub(crate) const EXACT: u16 = 3;

/// The tier of a run of consecutive characters that opens a word of the field.
pub(crate) const AT_A_START: u16 = 2;

/// The tier of a run of consecutive characters anywhere else.
pub(crate) const INSIDE: u16 = 1;

/// The tier of a word whose characters are scattered.
pub(crate) const SCATTERED: u16 = 0;

/// How many bits a value up to `largest` needs.
const fn bits(largest: usize) -> u32 {
    usize::BITS - largest.leading_zeros()
}

/// The largest tier sum: every word the whole of a field.
const TIERS_LARGEST: usize = EXACT as usize * WORDS_MAX;

/// The largest sum of names: every word on a name.
const NAMES_LARGEST: usize = WORDS_MAX;

/// The largest sum of word starts: every character of every word on one.
const STARTS_LARGEST: usize = QUERY_CHARS_MAX;

/// The largest sum of consecutive pairs: one word of the bound, all of it a run.
const CONSECUTIVE_LARGEST: usize = QUERY_CHARS_MAX - 1;

/// The largest earliness one word can have: its first character the field's first.
const EARLINESS_LARGEST: usize = FIELD_CHARS_MAX - 1;

/// The largest sum of earliness, and of shortness, whose largest per word is the same.
const PER_FIELD_LARGEST: usize = WORDS_MAX * EARLINESS_LARGEST;

/// The width of the tier sum.
const TIERS_BITS: u32 = bits(TIERS_LARGEST);

/// The width of the sum of names.
const NAMES_BITS: u32 = bits(NAMES_LARGEST);

/// The width of the sum of word starts.
const STARTS_BITS: u32 = bits(STARTS_LARGEST);

/// The width of the sum of consecutive pairs.
const CONSECUTIVE_BITS: u32 = bits(CONSECUTIVE_LARGEST);

/// The width of the sum of earliness.
const EARLINESS_BITS: u32 = bits(PER_FIELD_LARGEST);

/// The width of the sum of shortness.
const SHORTNESS_BITS: u32 = bits(PER_FIELD_LARGEST);

/// Where shortness sits: the lowest bits, since it is compared last.
const SHORTNESS_AT: u32 = 0;

/// Where earliness sits.
const EARLINESS_AT: u32 = SHORTNESS_AT + SHORTNESS_BITS;

/// Where the consecutive pairs sit.
const CONSECUTIVE_AT: u32 = EARLINESS_AT + EARLINESS_BITS;

/// Where the word starts sit.
const STARTS_AT: u32 = CONSECUTIVE_AT + CONSECUTIVE_BITS;

/// Where the names sit.
const NAMES_AT: u32 = STARTS_AT + STARTS_BITS;

/// Where the tiers sit: the highest bits, since they are compared first.
const TIERS_AT: u32 = NAMES_AT + NAMES_BITS;

/// Every bit the packed score can use.
pub(crate) const TOTAL_BITS: u32 = TIERS_AT + TIERS_BITS;

/// A score is below `2^53`, so a JavaScript number, which JSON is read into, holds it exactly.
const _: () = assert!(TOTAL_BITS <= 53);

/// Each component's largest sum fits its `u16`, so adding two words' scores cannot overflow.
const _: () = assert!(
    TIERS_BITS <= 16
        && NAMES_BITS <= 16
        && STARTS_BITS <= 16
        && CONSECUTIVE_BITS <= 16
        && EARLINESS_BITS <= 16
        && SHORTNESS_BITS <= 16
);

/// How well a query matched an item, compared component by component in the order written.
///
/// Every component is a sum over the query's words: the tiers, the words that
/// matched a name rather than a tag, the characters on word starts, the
/// adjacent pairs of characters, how early each word begins — the field's last
/// index less its first position — and how short each field is — the field
/// bound less the field's characters, separators aside.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Score {
    /// The sum of the words' tiers.
    tiers: u16,
    /// How many words matched a name.
    names: u16,
    /// How many matched characters open a word of their field.
    starts: u16,
    /// How many matched characters follow the one before them directly.
    consecutive: u16,
    /// How early the words begin in their fields.
    earliness: u16,
    /// How short the words' fields are.
    shortness: u16,
}

impl Score {
    /// The score of the empty query, which matches every item equally.
    pub const ZERO: Self = Self {
        tiers: 0,
        names: 0,
        starts: 0,
        consecutive: 0,
        earliness: 0,
        shortness: 0,
    };

    /// One word's score, from its parts.
    pub(crate) fn of_word(
        tier: u16,
        name: bool,
        (starts, consecutive, earliness): (u16, u16, u16),
        shortness: u16,
    ) -> Self {
        Self {
            tiers: tier,
            names: u16::from(name),
            starts,
            consecutive,
            earliness,
            shortness,
        }
    }

    /// This score and another's, component by component.
    pub(crate) fn plus(self, other: Self) -> Self {
        Self {
            tiers: self.tiers + other.tiers,
            names: self.names + other.names,
            starts: self.starts + other.starts,
            consecutive: self.consecutive + other.consecutive,
            earliness: self.earliness + other.earliness,
            shortness: self.shortness + other.shortness,
        }
    }

    /// The score as one integer below `2^53`, ordered exactly as the score is.
    ///
    /// What a surface outside Rust ranks by: a JSON number a JavaScript client
    /// reads without losing a unit.
    #[must_use]
    pub fn packed(self) -> u64 {
        u64::from(self.tiers) << TIERS_AT
            | u64::from(self.names) << NAMES_AT
            | u64::from(self.starts) << STARTS_AT
            | u64::from(self.consecutive) << CONSECUTIVE_AT
            | u64::from(self.earliness) << EARLINESS_AT
            | u64::from(self.shortness) << SHORTNESS_AT
    }

    /// The sum of the words' tiers.
    ///
    /// Three for a whole field, two for a run opening a word, one for a run
    /// anywhere else, and none for scattered characters.
    #[must_use]
    pub fn tiers(self) -> u16 {
        self.tiers
    }

    /// How many words matched a name rather than a tag.
    #[must_use]
    pub fn names(self) -> u16 {
        self.names
    }

    /// How many matched characters open a word of their field.
    #[must_use]
    pub fn starts(self) -> u16 {
        self.starts
    }

    /// How many matched characters follow the one before them directly, separators aside.
    #[must_use]
    pub fn consecutive(self) -> u16 {
        self.consecutive
    }

    /// How early the words begin: each adds its field's last index less its first position.
    #[must_use]
    pub fn earliness(self) -> u16 {
        self.earliness
    }

    /// How short the words' fields are: each adds the bound less its field's characters.
    #[must_use]
    pub fn shortness(self) -> u16 {
        self.shortness
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CONSECUTIVE_LARGEST, NAMES_LARGEST, PER_FIELD_LARGEST, STARTS_LARGEST, Score,
        TIERS_LARGEST, TOTAL_BITS,
    };

    /// A component's largest sum, as the `u16` it is kept in.
    fn largest(sum: usize) -> u16 {
        u16::try_from(sum).expect("every largest sum fits the component")
    }

    /// A score with one value in every component, read from an array in their order.
    fn score([tiers, names, starts, consecutive, earliness, shortness]: [u16; 6]) -> Score {
        Score {
            tiers,
            names,
            starts,
            consecutive,
            earliness,
            shortness,
        }
    }

    #[test]
    fn the_packed_score_orders_as_its_components_do() {
        let widest = [
            largest(TIERS_LARGEST),
            largest(NAMES_LARGEST),
            largest(STARTS_LARGEST),
            largest(CONSECUTIVE_LARGEST),
            largest(PER_FIELD_LARGEST),
            largest(PER_FIELD_LARGEST),
        ];
        // For each component: every component after it at its largest sum is
        // still below one unit of it, both as a score and packed. A width one
        // bit short spills the largest sum into its neighbour and turns this
        // around.
        for at in 0..widest.len() {
            let mut below = [0; 6];
            below[at + 1..].copy_from_slice(&widest[at + 1..]);
            let mut above = [0; 6];
            above[at] = 1;
            let (below, above) = (score(below), score(above));
            assert!(below < above, "component {at}");
            assert!(below.packed() < above.packed(), "component {at}, packed");
        }
        // And each component reaches the packed integer: two scores that differ
        // in one component alone differ there too.
        for at in 0..widest.len() {
            let mut one = [0; 6];
            one[at] = widest[at];
            assert_ne!(score(one).packed(), 0, "component {at} is packed");
            assert_eq!(
                score(one).packed().count_ones(),
                widest[at].count_ones(),
                "component {at} is packed whole, in bits of its own"
            );
        }
        assert_eq!(Score::ZERO.packed(), 0);
        assert_eq!(Score::ZERO, Score::default());
    }

    #[test]
    fn the_widest_score_a_query_can_reach_is_a_safe_json_integer() {
        let widest = score([
            largest(TIERS_LARGEST),
            largest(NAMES_LARGEST),
            largest(STARTS_LARGEST),
            largest(CONSECUTIVE_LARGEST),
            largest(PER_FIELD_LARGEST),
            largest(PER_FIELD_LARGEST),
        ]);
        assert!(widest.packed() < 1 << 53);
        assert!(widest.packed() < 1 << TOTAL_BITS);
        assert_eq!(TOTAL_BITS, 50);
    }

    #[test]
    fn a_score_sums_its_words_component_by_component() {
        let first = score([3, 1, 2, 4, 120, 100]);
        let second = score([1, 0, 5, 0, 7, 9]);
        assert_eq!(first.plus(second), score([4, 1, 7, 4, 127, 109]));
        assert_eq!(first.plus(Score::ZERO), first);
        let word = Score::of_word(2, true, (3, 4, 5), 6);
        assert_eq!(word, score([2, 1, 3, 4, 5, 6]));
        assert_eq!(
            [
                word.tiers(),
                word.names(),
                word.starts(),
                word.consecutive(),
                word.earliness(),
                word.shortness()
            ],
            [2, 1, 3, 4, 5, 6]
        );
        assert_eq!(Score::of_word(0, false, (0, 0, 0), 0).names(), 0);
    }
}
