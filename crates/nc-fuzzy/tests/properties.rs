//! The laws the fuzzy core holds, over items and queries it was never shown.
//!
//! `NOCTUA.md §12.4` makes three promises a list relies on while a person types:
//! a word adds a condition and never a result, every character the list colours
//! is one the word really matched, and the order does not jump about. The corpus
//! holds the cases somebody wrote down; these hold the ones nobody did, with
//! separators, capitals, accents, scripts without case and fields past the bound
//! among them — and, last, the chosen alignment against every alignment there is.

use nc_fuzzy::{FIELD_CHARS_MAX, Item, Match, Matcher, Query, Role, Score, fold, is_separator};
use proptest::prelude::*;
use proptest::sample::Index;

/// Each word's field and positions, as a match reports them.
type Hits = Vec<(usize, Vec<usize>)>;

/// A word that no generated text holds, so a query carrying it finds nothing.
const NOWHERE: &str = "\u{A66E}";

/// A field's role.
fn role() -> impl Strategy<Value = Role> {
    prop_oneof![Just(Role::Name), Just(Role::Tag)]
}

/// Text a field may hold.
///
/// Every separator, capitals after small letters and after capitals, accents,
/// Greek, Hangul and Han, and a branch past the field bound, so that each rule
/// of the core is reached by some generated field.
fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z]{0,12}",
        "[a-zA-Z \\-_./:]{0,24}",
        "[a-zA-ZÀ-ÿ ]{0,24}",
        "[α-ωΑ-Ωά-ώ ]{0,12}",
        "[가-힣 ]{0,8}",
        "[一-龥 ]{0,8}",
        "[a-zé\u{a0}\u{feff}\u{85}\t]{0,16}",
        "([A-Z][a-z]{0,6}){1,4}",
        "[A-Z]{2,5}[a-z]{0,6}",
        "[a-z ]{120,200}",
        "[aAbBé \\-]{0,12}",
    ]
}

/// An item's fields.
fn fields() -> impl Strategy<Value = Vec<(Role, String)>> {
    prop::collection::vec((role(), text()), 1..5)
}

/// How one word of a query is drawn: its field, its characters, and how it is written.
type Plan = (Index, Vec<Index>, u8, String);

/// A word's plan: characters sampled in order from a field, re-cased, de-accented or random.
fn plan() -> impl Strategy<Value = Plan> {
    (
        any::<Index>(),
        prop::collection::vec(any::<Index>(), 1..6),
        0_u8..4,
        "[a-zé]{1,4}",
    )
}

/// A list of items, and the words of a query drawn mostly from one of them.
///
/// Drawn from one item, so that most queries find it and the laws below assert
/// something; the random words are what make the rest find nothing.
fn case() -> impl Strategy<Value = (Vec<Vec<(Role, String)>>, Vec<String>)> {
    (
        prop::collection::vec(fields(), 1..5),
        any::<Index>(),
        prop::collection::vec(plan(), 1..4),
    )
        .prop_map(|(items, target, plans)| {
            let fields = &items[target.index(items.len())];
            let words = plans.iter().map(|plan| drawn(fields, plan)).collect();
            (items, words)
        })
}

/// One word, drawn as its plan says.
fn drawn(fields: &[(Role, String)], (field, picks, how, random): &Plan) -> String {
    let usable: Vec<char> = fields[field.index(fields.len())]
        .1
        .chars()
        .take(FIELD_CHARS_MAX)
        .filter(|c| !is_separator(*c))
        .collect();
    if usable.is_empty() || *how == 3 {
        return random.clone();
    }
    let mut chosen: Vec<usize> = picks.iter().map(|pick| pick.index(usable.len())).collect();
    chosen.sort_unstable();
    chosen.dedup();
    chosen
        .iter()
        .map(|&at| match how {
            1 => recased(usable[at]),
            2 => fold(usable[at]),
            _ => usable[at],
        })
        .collect()
}

/// `c` in upper case, where that is one character that folds as `c` does.
fn recased(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(capital), None) if fold(capital) == fold(c) => capital,
        _ => c,
    }
}

/// The items, built.
fn built(items: &[Vec<(Role, String)>]) -> Vec<Item> {
    items
        .iter()
        .map(|fields| Item::new(fields.iter().map(|(role, text)| (*role, text.as_str()))))
        .collect()
}

/// The query its words make, joined by spaces.
fn query(words: &[String]) -> Query {
    Query::parse(&words.join(" ")).expect("every generated query is within its bound")
}

/// What `query` finds of each item, in the items' order.
fn each(query: &Query, items: &[Item]) -> Vec<Option<Match>> {
    let mut matcher = Matcher::new();
    items
        .iter()
        .map(|item| matcher.matched(query, item))
        .collect()
}

proptest! {
    /// Every position a hit returns holds, folded, the character of its word at the same place.
    #[test]
    fn every_returned_position_holds_its_words_character((items, words) in case()) {
        let query = query(&words);
        for (fields, found) in items.iter().zip(each(&query, &built(&items))) {
            let Some(found) = found else { continue };
            prop_assert_eq!(found.hits().len(), query.words().len());
            for (word, hit) in query.words().zip(found.hits()) {
                let text: Vec<char> = fields[hit.field()].1.chars().collect();
                prop_assert_eq!(hit.positions().len(), word.len());
                for (&at, &wanted) in hit.positions().iter().zip(word) {
                    prop_assert_eq!(fold(text[at]), wanted, "position {} of {:?}", at, fields[hit.field()].1);
                }
            }
        }
    }

    /// Positions rise strictly, stay inside what is read of their field, and miss every separator.
    #[test]
    fn positions_rise_strictly_and_stay_inside_their_field((items, words) in case()) {
        let query = query(&words);
        for (fields, found) in items.iter().zip(each(&query, &built(&items))) {
            let Some(found) = found else { continue };
            for hit in found.hits() {
                prop_assert!(hit.field() < fields.len());
                let text: Vec<char> = fields[hit.field()].1.chars().collect();
                let read = text.len().min(FIELD_CHARS_MAX);
                prop_assert!(hit.positions().windows(2).all(|pair| pair[0] < pair[1]));
                for &at in hit.positions() {
                    prop_assert!(at < read, "{} is past the {} characters read", at, read);
                    prop_assert!(!is_separator(text[at]), "{:?} is a separator", text[at]);
                }
            }
        }
    }

    /// An item a query finds is still found once any one of its words is taken away.
    #[test]
    fn removing_a_word_never_removes_a_result((items, words) in case()) {
        let items = built(&items);
        let found = each(&query(&words), &items);
        for removed in 0..words.len() {
            let mut fewer = words.clone();
            fewer.remove(removed);
            for (before, after) in found.iter().zip(each(&query(&fewer), &items)) {
                prop_assert!(before.is_none() || after.is_some(), "removing {:?} lost an item", words[removed]);
            }
        }
    }

    /// The words left after one is taken away keep the field and the positions each had.
    #[test]
    fn the_words_left_keep_the_hits_they_had((items, words) in case()) {
        let items = built(&items);
        let found = each(&query(&words), &items);
        for removed in 0..words.len() {
            let mut fewer = words.clone();
            fewer.remove(removed);
            for (before, after) in found.iter().zip(each(&query(&fewer), &items)) {
                let (Some(before), Some(after)) = (before, after) else { continue };
                let mut kept = before.hits().to_vec();
                kept.remove(removed);
                prop_assert_eq!(after.hits(), kept.as_slice());
            }
        }
    }

    /// A word no field can hold removes every item, whatever the other words found.
    #[test]
    fn a_word_that_cannot_match_removes_the_item((items, words) in case()) {
        let mut more = words;
        more.push(NOWHERE.to_owned());
        prop_assert!(each(&query(&more), &built(&items)).iter().all(Option::is_none));
    }

    /// The words typed in another order find the same items and scores, each word its own hit.
    #[test]
    fn ranking_is_the_same_whatever_order_the_words_are_typed_in((items, words) in case(), turn in any::<Index>()) {
        let items = built(&items);
        let mut turned = words.clone();
        turned.rotate_left(turn.index(words.len()));
        turned.reverse();
        let mut matcher = Matcher::new();
        let ranked = matcher.ranked(&query(&words), &items);
        let again = matcher.ranked(&query(&turned), &items);
        prop_assert_eq!(ranked.len(), again.len());
        for (one, other) in ranked.iter().zip(&again) {
            prop_assert_eq!(one.index(), other.index());
            prop_assert_eq!(one.found().score(), other.found().score());
            let mut hits = one.found().hits().to_vec();
            hits.rotate_left(turn.index(words.len()));
            hits.reverse();
            prop_assert_eq!(other.found().hits(), hits.as_slice());
        }
    }

    /// An item nothing matches, put anywhere in the list, moves no other item's place in it.
    #[test]
    fn ranking_stays_stable_when_an_unmatched_item_joins_the_list((items, words) in case(), at in any::<Index>()) {
        let items = built(&items);
        let query = query(&words);
        let mut matcher = Matcher::new();
        let before = matcher.ranked(&query, &items);
        let place = at.index(items.len() + 1);
        let mut joined = items;
        joined.insert(place, Item::new([(Role::Name, NOWHERE)]));
        let after = matcher.ranked(&query, &joined);
        prop_assert_eq!(before.len(), after.len());
        for (one, other) in before.iter().zip(&after) {
            let moved = if one.index() >= place { one.index() + 1 } else { one.index() };
            prop_assert_eq!(moved, other.index());
            prop_assert_eq!(one.found(), other.found());
        }
    }

    /// The ranked list is every item found, by descending score and then by place in the list.
    #[test]
    fn the_ranked_list_is_ordered_by_score_then_by_place_in_the_list((items, words) in case()) {
        let items = built(&items);
        let query = query(&words);
        let ranked = Matcher::new().ranked(&query, &items);
        for pair in ranked.windows(2) {
            let (one, other) = (&pair[0], &pair[1]);
            prop_assert!(
                one.found().score() > other.found().score()
                    || (one.found().score() == other.found().score() && one.index() < other.index())
            );
            prop_assert!(
                one.found().score().packed() > other.found().score().packed()
                    || one.found().score() == other.found().score()
            );
        }
        let found = each(&query, &items);
        let mut listed: Vec<usize> = ranked.iter().map(nc_fuzzy::Ranked::index).collect();
        listed.sort_unstable();
        let matched: Vec<usize> = (0..items.len()).filter(|&index| found[index].is_some()).collect();
        prop_assert_eq!(listed, matched);
        for one in &ranked {
            prop_assert_eq!(Some(one.found()), found[one.index()].as_ref());
        }
    }

    /// No text and no query, whatever characters they hold, makes the matcher panic.
    #[test]
    fn no_text_and_no_query_makes_the_matcher_panic(
        texts in prop::collection::vec((role(), prop::collection::vec(any::<char>(), 0..300)), 0..5),
        typed in prop::collection::vec(any::<char>(), 0..80),
        plans in prop::collection::vec(plan(), 1..4),
    ) {
        let fields: Vec<(Role, String)> = texts
            .into_iter()
            .map(|(role, chars)| (role, chars.into_iter().collect()))
            .collect();
        let item = Item::new(fields.iter().map(|(role, text)| (*role, text.as_str())));
        // Whatever was typed, and words drawn from the fields themselves, so
        // that arbitrary characters reach the tiers and the table too.
        let mut queries: Vec<Query> = Query::parse(&typed.into_iter().collect::<String>()).into_iter().collect();
        if !fields.is_empty() {
            let words: Vec<String> = plans.iter().map(|plan| drawn(&fields, plan)).collect();
            queries.extend(Query::parse(&words.join(" ")));
        }
        let mut matcher = Matcher::new();
        for query in &queries {
            for ranked in matcher.ranked(query, std::slice::from_ref(&item)) {
                prop_assert_eq!(ranked.found().hits().len(), query.words().len());
                for hit in ranked.found().hits() {
                    let read = fields[hit.field()].1.chars().count().min(FIELD_CHARS_MAX);
                    prop_assert!(hit.positions().iter().all(|&at| at < read));
                }
            }
        }
    }

    /// Each word's field, positions and score are the best an exhaustive search finds.
    #[test]
    fn the_chosen_alignment_is_the_best_an_exhaustive_search_finds(
        fields in prop::collection::vec((role(), "[aAbBé \\-]{0,10}"), 1..4),
        words in prop::collection::vec("[abé]{1,4}", 1..3),
    ) {
        let item = Item::new(fields.iter().map(|(role, text)| (*role, text.as_str())));
        let query = query(&words);
        let found = Matcher::new().matched(&query, &item);
        let mut expected: Option<(Hits, [u16; 6])> = Some((Vec::new(), [0; 6]));
        for word in query.words() {
            let best = exhaustive(&fields, word);
            expected = match (expected, best) {
                (Some((mut hits, mut sum)), Some((key, field, positions))) => {
                    hits.push((field, positions));
                    for (total, part) in sum.iter_mut().zip(key) {
                        *total += part;
                    }
                    Some((hits, sum))
                }
                _ => None,
            };
        }
        let found = found.map(|found| {
            let hits: Hits = found
                .hits()
                .iter()
                .map(|hit| (hit.field(), hit.positions().to_vec()))
                .collect();
            (hits, components(found.score()))
        });
        prop_assert_eq!(found, expected);
    }
}

/// A score's components, in the order it compares them.
fn components(score: Score) -> [u16; 6] {
    [
        score.tiers(),
        score.names(),
        score.starts(),
        score.consecutive(),
        score.earliness(),
        score.shortness(),
    ]
}

/// The best way `word` lies in `fields`, found by trying every alignment in every field.
///
/// Its own reading of the rule, from the original text: the separators left
/// out, a word start where the text opens, after a separator, or at a capital
/// after a character that is not one, and every increasing choice of positions
/// whose folded characters spell the word. The best is the largest key, the
/// first field and then the leftmost positions among equals — the order the
/// loops below try them in.
fn exhaustive(fields: &[(Role, String)], word: &[char]) -> Option<([u16; 6], usize, Vec<usize>)> {
    let capital = |c: char| c.to_lowercase().ne([c]) && c.to_uppercase().eq([c]);
    let mut best: Option<([u16; 6], usize, Vec<usize>)> = None;
    for (index, (role, text)) in fields.iter().enumerate() {
        let chars: Vec<char> = text.chars().collect();
        let mut kept: Vec<usize> = Vec::new();
        let mut opens: Vec<bool> = Vec::new();
        for (at, &c) in chars.iter().enumerate() {
            if is_separator(c) {
                continue;
            }
            let before = at.checked_sub(1).map(|before| chars[before]);
            opens.push(
                before
                    .is_none_or(|before| is_separator(before) || (capital(c) && !capital(before))),
            );
            kept.push(at);
        }
        let folded: Vec<char> = kept.iter().map(|&at| fold(chars[at])).collect();
        for choice in alignments(&folded, word) {
            let run = choice.windows(2).all(|pair| pair[1] == pair[0] + 1);
            let tier = if folded == word {
                3
            } else if run {
                if opens[choice[0]] { 2 } else { 1 }
            } else {
                0
            };
            let count = |n: usize| u16::try_from(n).expect("a small count");
            let key = [
                tier,
                u16::from(*role == Role::Name),
                count(choice.iter().filter(|&&at| opens[at]).count()),
                count(
                    choice
                        .windows(2)
                        .filter(|pair| pair[1] == pair[0] + 1)
                        .count(),
                ),
                count(FIELD_CHARS_MAX - 1 - kept[choice[0]]),
                count(FIELD_CHARS_MAX - folded.len()),
            ];
            if best.as_ref().is_none_or(|(held, ..)| key > *held) {
                best = Some((key, index, choice.iter().map(|&at| kept[at]).collect()));
            }
        }
    }
    best
}

/// Every increasing choice of indices into `folded` that spells `word`, leftmost first.
fn alignments(folded: &[char], word: &[char]) -> Vec<Vec<usize>> {
    let Some((first, rest)) = word.split_first() else {
        return vec![Vec::new()];
    };
    let mut found = Vec::new();
    for (at, c) in folded.iter().enumerate() {
        if c != first {
            continue;
        }
        for mut tail in alignments(&folded[at + 1..], rest) {
            for later in &mut tail {
                *later += at + 1;
            }
            tail.insert(0, at);
            found.push(tail);
        }
    }
    found
}

/// Enough generated cases find an item for the laws above to assert something.
///
/// A floor, not the number: what it refuses is a change to a generator, or to
/// the rule, that quietly leaves most cases finding nothing while every law
/// above stays green by asserting nothing — the pattern `nc-ids`' own floor
/// set. At least a quarter of the cases find an item, and at least a quarter of
/// the exhaustive search's words lie somewhere.
///
/// Measured over these 512 samples of each: 330 queries find an item, and 187
/// words lie in their fields.
#[test]
fn enough_generated_cases_reach_the_assertion() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    const SAMPLES: usize = 512;
    let mut runner = TestRunner::deterministic();
    let mut finding = 0_usize;
    for _ in 0..SAMPLES {
        let (items, words) = case()
            .new_tree(&mut runner)
            .expect("the generator produces a value")
            .current();
        if each(&query(&words), &built(&items))
            .iter()
            .any(Option::is_some)
        {
            finding += 1;
        }
    }
    assert!(
        finding * 4 >= SAMPLES,
        "only {finding} of {SAMPLES} generated queries find an item, so the laws above \
         assert nothing in most of their cases"
    );

    let mut lying = 0_usize;
    let generator = (
        prop::collection::vec((role(), "[aAbBé \\-]{0,10}"), 1..4),
        "[abé]{1,4}",
    );
    for _ in 0..SAMPLES {
        let (fields, word) = generator
            .new_tree(&mut runner)
            .expect("the generator produces a value")
            .current();
        let folded: Vec<char> = word.chars().map(fold).collect();
        if exhaustive(&fields, &folded).is_some() {
            lying += 1;
        }
    }
    assert!(
        lying * 4 >= SAMPLES,
        "only {lying} of {SAMPLES} words lie in their fields, so the exhaustive search \
         compares nothing in most of its cases"
    );
}
