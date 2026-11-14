//! Which items a query finds, each word's field and positions, and their order.

use crate::align::{Item, aligned};
use crate::query::Query;
use crate::score::Score;

/// The matcher, holding the scratch it reuses from one item to the next.
///
/// Make one and keep it: the table a scattered word is laid with grows to the
/// largest it has needed and stays there, so matching a list allocates nothing
/// for an item that is not found.
#[derive(Clone, Debug, Default)]
pub struct Matcher {
    /// The dynamic program's table, filled per word and field.
    table: Vec<u32>,
    /// The tight positions of the alignment being weighed.
    trial: Vec<usize>,
    /// The tight positions of the best alignment so far, for the word at hand.
    best: Vec<usize>,
}

impl Matcher {
    /// A matcher with nothing allocated yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How `query` matches `item`, or `None` when one of its words matches none of its fields.
    ///
    /// Each word takes the field it matches best — by tier, then role, then the
    /// rest of [`Score`]'s order, the lowest field among equals — independently
    /// of every other word. The empty query matches every item with
    /// [`Score::ZERO`] and no hit.
    pub fn matched(&mut self, query: &Query, item: &Item) -> Option<Match> {
        let mut score = Score::ZERO;
        let mut hits = Vec::new();
        for word in query.words() {
            let mut chosen: Option<(Score, usize)> = None;
            for (index, field) in item.fields().iter().enumerate() {
                let Some(fit) = aligned(field, word, &mut self.table, &mut self.trial) else {
                    continue;
                };
                if chosen.is_none_or(|(held, _)| fit > held) {
                    chosen = Some((fit, index));
                    std::mem::swap(&mut self.trial, &mut self.best);
                }
            }
            let (fit, field) = chosen?;
            score = score.plus(fit);
            hits.push(Hit {
                field,
                positions: item.fields()[field].original(&self.best),
            });
        }
        Some(Match { score, hits })
    }

    /// Every item `query` matches, best first, with its place in `items`.
    ///
    /// Not cut to any length: a caller that shows a page takes it, and a count
    /// of everything found is the list's length. Equal scores keep the order
    /// the items were given in, so the list is stable while a person types.
    pub fn ranked(&mut self, query: &Query, items: &[Item]) -> Vec<Ranked> {
        let mut found: Vec<Ranked> = items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.matched(query, item)
                    .map(|found| Ranked { index, found })
            })
            .collect();
        found.sort_by(|left, right| {
            right
                .found
                .score
                .cmp(&left.found.score)
                .then(left.index.cmp(&right.index))
        });
        found
    }
}

/// How a query matched one item: its score, and each word's hit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// The sum of every word's score.
    score: Score,
    /// One hit per word of the query, in the order the words were typed.
    hits: Vec<Hit>,
}

impl Match {
    /// The sum of every word's score.
    #[must_use]
    pub fn score(&self) -> Score {
        self.score
    }

    /// One hit per word of the query, in the order the words were typed.
    ///
    /// Two hits may name the same field and share positions: words are matched
    /// independently, and a surface colours a character by the first word that
    /// claims it.
    #[must_use]
    pub fn hits(&self) -> &[Hit] {
        &self.hits
    }
}

/// Where one word of a query matched: a field of the item, and its characters there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The field's index, in the order the item was built with.
    field: usize,
    /// The matched characters' indices in the field's original text, rising.
    positions: Vec<usize>,
}

impl Hit {
    /// The field's index, in the order the item was built with.
    #[must_use]
    pub fn field(&self) -> usize {
        self.field
    }

    /// The matched characters' indices in the field's original text, rising.
    ///
    /// One per character of the word, each a `char` index — never a byte, and
    /// never an index into the folded text without separators.
    #[must_use]
    pub fn positions(&self) -> &[usize] {
        &self.positions
    }
}

/// One item a query found, with its place in the list it was found in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ranked {
    /// The item's index in the list handed to [`Matcher::ranked`].
    index: usize,
    /// How the query matched it.
    found: Match,
}

impl Ranked {
    /// The item's index in the list handed to [`Matcher::ranked`].
    #[must_use]
    pub fn index(&self) -> usize {
        self.index
    }

    /// How the query matched it.
    #[must_use]
    pub fn found(&self) -> &Match {
        &self.found
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, Matcher, Query};
    use crate::Role;

    /// An item of `(role, text)` fields.
    fn item(fields: &[(Role, &str)]) -> Item {
        Item::new(fields.iter().copied())
    }

    /// The indices `query` finds in `items`, best first.
    fn order(query: &str, items: &[Item]) -> Vec<usize> {
        let query = Query::parse(query).expect("a query");
        Matcher::new()
            .ranked(&query, items)
            .iter()
            .map(super::Ranked::index)
            .collect()
    }

    /// Each word's field and positions, for `query` on `item`.
    fn hits(query: &str, item: &Item) -> Option<Vec<(usize, Vec<usize>)>> {
        let query = Query::parse(query).expect("a query");
        Matcher::new().matched(&query, item).map(|found| {
            found
                .hits()
                .iter()
                .map(|hit| (hit.field(), hit.positions().to_vec()))
                .collect()
        })
    }

    #[test]
    fn every_word_must_match() {
        let verify = item(&[(Role::Name, "journal.verify"), (Role::Tag, "history")]);
        assert_eq!(hits("verify zzz", &verify), None);
        assert_eq!(hits("zzz verify", &verify), None);
        assert!(hits("verify history", &verify).is_some());
    }

    #[test]
    fn a_word_may_match_a_tag_while_another_matches_the_name() {
        let find = item(&[
            (Role::Name, "Encontrar um comando"),
            (Role::Tag, "procurar"),
        ]);
        assert_eq!(
            hits("procurar comando", &find),
            Some(vec![(1, (0..8).collect()), (0, (13..20).collect())])
        );
    }

    #[test]
    fn a_word_reports_the_field_it_matched() {
        let undo = item(&[
            (Role::Name, "kernel.undo"),
            (Role::Name, "Undo a step"),
            (Role::Name, "Desfazer um passo"),
            (Role::Tag, "revert"),
            (Role::Tag, "reverter"),
        ]);
        assert_eq!(hits("desfazer", &undo), Some(vec![(2, (0..8).collect())]));
        assert_eq!(hits("reverter", &undo), Some(vec![(4, (0..8).collect())]));
        assert_eq!(hits("revert", &undo), Some(vec![(3, (0..6).collect())]));
    }

    #[test]
    fn a_hit_on_a_name_outranks_the_same_hit_on_a_tag() {
        let tagged = item(&[(Role::Tag, "arquivo")]);
        let named = item(&[(Role::Name, "arquivo")]);
        assert_eq!(order("arquivo", &[tagged.clone(), named.clone()]), [1, 0]);
        assert_eq!(order("arq", &[tagged, named]), [1, 0]);
    }

    #[test]
    fn an_exact_tag_outranks_a_scattered_name() {
        let scattered = item(&[(Role::Name, "Procurar")]);
        let exact = item(&[(Role::Tag, "pcr")]);
        assert_eq!(order("pcr", &[scattered, exact]), [1, 0]);
    }

    #[test]
    fn among_equal_matches_the_shorter_field_ranks_first() {
        let longer = item(&[(Role::Name, "Verify everything")]);
        let shorter = item(&[(Role::Name, "Verify it")]);
        assert_eq!(order("ver", &[longer, shorter]), [1, 0]);
    }

    #[test]
    fn a_word_tied_across_fields_reports_the_first_of_them() {
        let twice = item(&[(Role::Tag, "undo"), (Role::Tag, "undo")]);
        assert_eq!(hits("undo", &twice), Some(vec![(0, (0..4).collect())]));
    }

    #[test]
    fn a_match_scores_the_sum_of_its_words() {
        let mut matcher = Matcher::new();
        let history = item(&[(Role::Name, "Verificar o histórico"), (Role::Tag, "audit")]);
        let score = |query: &str, matcher: &mut Matcher| {
            matcher
                .matched(&Query::parse(query).expect("a query"), &history)
                .expect("found")
                .score()
        };
        let both = score("ver hist aud", &mut matcher);
        let each = score("ver", &mut matcher)
            .plus(score("hist", &mut matcher))
            .plus(score("aud", &mut matcher));
        assert_eq!(both, each);
        assert_eq!(both.tiers(), 6);
        assert_eq!(both.names(), 2);
    }

    #[test]
    fn two_words_may_claim_the_same_characters() {
        let verify = item(&[(Role::Name, "Verificar")]);
        assert_eq!(
            hits("ver verificar", &verify),
            Some(vec![(0, (0..3).collect()), (0, (0..9).collect())])
        );
    }

    #[test]
    fn equal_scores_keep_the_order_the_items_were_given_in() {
        let items: Vec<Item> = ["ab x", "ab y", "ab z"]
            .iter()
            .map(|text| item(&[(Role::Name, text)]))
            .collect();
        assert_eq!(order("ab", &items), [0, 1, 2]);
        let mut reversed = items;
        reversed.reverse();
        assert_eq!(order("ab", &reversed), [0, 1, 2]);
    }

    #[test]
    fn an_empty_query_lists_every_item_in_order() {
        let items: Vec<Item> = ["b", "a", "", "c"]
            .iter()
            .map(|text| item(&[(Role::Name, text)]))
            .collect();
        assert_eq!(order("", &items), [0, 1, 2, 3]);
        assert_eq!(order(" - ", &items), [0, 1, 2, 3]);
        let found = Matcher::new()
            .matched(&Query::parse("").expect("a query"), &Item::new([]))
            .expect("the empty query finds an item with no field");
        assert_eq!(found.score(), crate::Score::ZERO);
        assert_eq!(found.hits(), []);
        assert_eq!(hits("a", &Item::new([])), None);
        assert!(Item::new([]).is_empty());
        let two = item(&[(Role::Name, "a"), (Role::Tag, "b")]);
        assert_eq!((two.len(), two.is_empty()), (2, false));
    }

    #[test]
    fn the_ranked_list_holds_every_match_and_nothing_else() {
        let items: Vec<Item> = ["Verify", "server", "Very Elaborate Report", "undo"]
            .iter()
            .map(|text| item(&[(Role::Name, text)]))
            .collect();
        assert_eq!(order("ver", &items), [0, 2, 1]);
        let ranked = Matcher::new().ranked(&Query::parse("ver").expect("a query"), &items);
        assert_eq!(ranked[0].found().hits()[0].positions(), [0, 1, 2]);
        assert_eq!(ranked[2].found().hits()[0].positions(), [3, 4, 5]);
    }

    #[test]
    fn an_item_is_matched_the_same_by_a_matcher_used_before() {
        // The scratch the matcher keeps carries nothing from one item to the next.
        let mut matcher = Matcher::new();
        let query = Query::parse("abc").expect("a query");
        let long = item(&[(Role::Name, "a b c a b c a b c a b c")]);
        let short = item(&[(Role::Name, "axbxc")]);
        let first = matcher.matched(&query, &long);
        let fresh = Matcher::new().matched(&query, &short);
        assert_eq!(matcher.matched(&query, &short), fresh);
        assert_eq!(matcher.matched(&query, &long), first);
    }
}
