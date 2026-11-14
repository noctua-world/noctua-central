//! One character folded, the characters a match never claims, and where a word opens.
//!
//! Each is a function of one character, or of a character and the one before it,
//! so a text keeps its length through all three and an index into the folded
//! text is an index into the original. A string-level lower case would not: `İ`
//! lowercases to two characters, and every position after it would move.

use unicode_normalization::char::decompose_canonical;

/// The first precomposed Hangul syllable.
const HANGUL_FIRST: char = '\u{AC00}';

/// The last precomposed Hangul syllable.
const HANGUL_LAST: char = '\u{D7A3}';

/// The character a match compares in place of `c`: lower case first, then its base letter.
///
/// The base letter is the first scalar of the canonical decomposition, so `é`
/// is `e`, `ç` is `c`, `ã` is `a` and `İ` is `i`; a letter with no
/// decomposition stays itself, so `ß`, `ø`, `æ` and `ł` are not folded to
/// anything they merely resemble. There is no smart case: matching never
/// depends on how the query was capitalised.
///
/// **A Hangul syllable is not folded.** Its decomposition begins with its
/// leading consonant, so folding it would let one syllable typed find every
/// syllable that opens with the same consonant.
#[must_use]
pub fn fold(c: char) -> char {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if (HANGUL_FIRST..=HANGUL_LAST).contains(&lower) {
        return lower;
    }
    let mut base = None;
    decompose_canonical(lower, |part| {
        base.get_or_insert(part);
    });
    base.unwrap_or(lower)
}

/// Whether `c` splits a query into words.
///
/// What JavaScript's `\s` matches, so the interface splits as this crate does:
/// Unicode's white space, which includes the no-break space, and the
/// byte-order mark, which is not white space to Rust.
#[must_use]
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{FEFF}'
}

/// Whether a match skips `c` in a field, and drops it inside a query word.
///
/// A space, or a character a name is joined with: `buttongroup` finds
/// `button-group`, and `verify` finds `journal.verify`, because each of
/// them is skipped.
#[must_use]
pub fn is_separator(c: char) -> bool {
    is_space(c) || matches!(c, '-' | '_' | '.' | '/' | ':')
}

/// Whether `c`, read after `previous`, opens a word of a field's text.
///
/// The first character does, and so does one after a separator, and a capital
/// after anything that is not one — so `ButtonGroup` opens two words and
/// `HTTPServer` one. Read on the original characters, since folding has
/// already forgotten which were capitals.
pub(crate) fn opens_a_word(previous: Option<char>, c: char) -> bool {
    match previous {
        None => true,
        Some(before) => is_separator(before) || (is_upper(c) && !is_upper(before)),
    }
}

/// Whether `c` is a capital, as the browser's matcher reads one.
///
/// A character its lower case changes and its upper case keeps — so a
/// title-case letter such as `ǅ` is not one, and neither is a letter of a
/// script without case.
fn is_upper(c: char) -> bool {
    !c.to_lowercase().eq([c]) && c.to_uppercase().eq([c])
}

#[cfg(test)]
mod tests {
    use super::{fold, is_separator, is_space, opens_a_word};

    /// The indices of a text whose characters open a word.
    fn starts(text: &str) -> Vec<usize> {
        let mut previous = None;
        let mut found = Vec::new();
        for (index, c) in text.chars().enumerate() {
            if !is_separator(c) && opens_a_word(previous, c) {
                found.push(index);
            }
            previous = Some(c);
        }
        found
    }

    #[test]
    fn a_letter_folds_to_its_lower_case_base_letter() {
        for (letter, base) in [
            ('É', 'e'),
            ('é', 'e'),
            ('Ç', 'c'),
            ('ç', 'c'),
            ('Ã', 'a'),
            ('õ', 'o'),
            ('ú', 'u'),
            ('İ', 'i'),
            ('Ω', 'ω'),
            ('Ά', 'α'),
            ('A', 'a'),
        ] {
            assert_eq!(fold(letter), base, "{letter}");
        }
    }

    #[test]
    fn a_letter_with_no_base_letter_stays_itself() {
        // A decomposition table that maps these to the letters they resemble is
        // the defect `nc-ids` records in its own naming rule.
        for letter in ['ß', 'ø', 'æ', 'ł', 'đ', 'ħ', 'a', '1', '中'] {
            assert_eq!(fold(letter), letter, "{letter}");
        }
        assert_eq!(fold('Ø'), 'ø');
        assert_eq!(fold('Ł'), 'ł');
    }

    #[test]
    fn a_hangul_syllable_is_not_folded_to_its_leading_consonant() {
        // `한` decomposes to the consonant `ᄒ` and two more jamo; folded, it
        // would match every syllable that opens with `ᄒ`, `하` among them.
        for syllable in ['가', '한', '하', '힣'] {
            assert_eq!(fold(syllable), syllable, "{syllable}");
        }
        assert_ne!(fold('한'), fold('하'));
    }

    #[test]
    fn folding_a_text_keeps_its_length() {
        // `İ` lowercases to two characters as a string, and to one here.
        let text = "İstanbul São Paulo ǅ";
        let folded: String = text.chars().map(fold).collect();
        assert_eq!(folded.chars().count(), text.chars().count());
        assert_eq!(folded, "istanbul sao paulo ǆ");
        assert_ne!(text.to_lowercase().chars().count(), text.chars().count());
    }

    #[test]
    fn every_separator_is_one_the_javascript_matcher_also_skips() {
        for space in [
            ' ', '\t', '\n', '\u{A0}', '\u{2003}', '\u{3000}', '\u{FEFF}', '\u{85}',
        ] {
            assert!(is_space(space), "{space:?} splits a query");
            assert!(is_separator(space), "{space:?} is skipped");
        }
        for joiner in ['-', '_', '.', '/', ':'] {
            assert!(!is_space(joiner), "{joiner} does not split a query");
            assert!(is_separator(joiner), "{joiner} is skipped");
        }
        // Text a name holds, and the invisible characters the registries refuse
        // already: none is a separator, so none is silently dropped here.
        for kept in [
            '\'', '’', ',', '(', '#', '+', 'a', 'é', '\u{200B}', '\u{200D}',
        ] {
            assert!(!is_separator(kept), "{kept:?} is matched like any letter");
        }
    }

    #[test]
    fn a_word_starts_after_a_separator_and_at_a_capital_after_a_small_letter() {
        assert_eq!(starts("ButtonGroup"), [0, 6]);
        assert_eq!(starts("journal.verify"), [0, 8]);
        assert_eq!(starts("HTTPServer"), [0]);
        assert_eq!(starts("button-group_list/item:end"), [0, 7, 13, 18, 23]);
        assert_eq!(starts("São Paulo"), [0, 4]);
        assert_eq!(starts("-lead"), [1]);
        assert_eq!(starts("x2D"), [0, 2]);
        // No case, so no capital: only the separator opens a word.
        assert_eq!(starts("한국 어"), [0, 3]);
    }
}
