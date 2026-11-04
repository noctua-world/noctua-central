//! The completeness guarantee: **a key missing from either locale fails the
//! build**, and the error names the key (`NOCTUA.md §10.9`).
//!
//! Four checks, and the second is the one that gets forgotten. A locale missing
//! a key is obvious the moment somebody reads that page. A locale carrying a key
//! the source locale does not is invisible forever: nothing looks it up, so it
//! is a translation of a sentence that no longer exists, and it will be
//! faithfully maintained by the next translator who has no way to know.
//!
//! The third is placeholders. A placeholder the caller does not supply is
//! rendered literally, so the reader sees the braces — which is how a message
//! that reads perfectly in review reaches a person as
//! "{field} precisa estar entre {min} e {max}".
//!
//! The fourth is the declaration. A registry entry says what its message
//! interpolates, and the sender is generated from that declaration rather than
//! from the sentence — so a sentence asking for something the declaration omits
//! can never be supplied, whatever the translations agree on.
//!
//! **Every diagnostic here is English, and located where a line exists.** One
//! that interpolates the locale's *endonym* puts "Português do Brasil is
//! missing …" in a developer's build log, in a repository whose every artifact
//! is English (`§2.1`); and a diagnostic with no line is one `line_of` could
//! have located.
//!
//! One class carries none, and it cannot: a key **missing** from a locale has no
//! line in that locale to point at. It names the source file the sentence has to
//! be translated from instead, which is where the reader actually has to go.

use std::collections::BTreeSet;
use std::path::Path;

use crate::load::{SOURCE_LOCALE, declared_params, referenced_keys};
use crate::model::{Catalog, Locale};
use crate::problem::{Problem, line_of};

/// The 1-indexed line a key that is not in this file should be written on.
///
/// **The end of the file is a line, and it is under the wrong heading.** A key
/// absent from a locale has no line of its own, and the last one is accurate
/// as an insertion point and wrong as a place: these
/// files group their keys under `# --- <Group> ---` comments, and the last line
/// of `catalog/i18n/en.toml` sits inside the commands block. A newcomer told to
/// add `error.validation.…` at line 77 files it among the commands, and the
/// file is then sorted by nothing.
///
/// So it points after the last key sharing this one's prefix — the whole key
/// less its final segment, then progressively less of it — and falls back to the
/// end of the file when the group does not exist yet, which is the honest answer
/// for the first key of a new group.
fn where_to_write(text: &str, key: &str) -> usize {
    let mut prefix = key;
    while let Some((shorter, _)) = prefix.rsplit_once('.') {
        prefix = shorter;
        let needle = format!("\"{prefix}.");
        let last = text
            .lines()
            .enumerate()
            .filter(|(_, line)| line.trim_start().starts_with(&needle))
            .map(|(index, _)| index)
            .last();
        if let Some(index) = last {
            return index + 1;
        }
    }
    text.lines().count().max(1)
}

#[cfg(test)]
mod insertion_point_tests {
    /// A missing key is filed with its own group, not at the end of the file.
    ///
    /// **One key per group answers every question the same way.** With exactly
    /// one key under each prefix, *the last of the group* and *the first of it*
    /// are the same line and neither the `.last()` nor the walk that shortens
    /// the prefix is decided by anything: `.last()` → `.min()` and
    /// `while let` → `if let` both survive, and so does dropping the `.max(1)`
    /// that keeps an empty file off line zero. A multi-level assertion
    /// comparing the prefix walk against the end-of-file fallback compares the
    /// same number — a rule compared against itself. Every group below holds
    /// two keys, and the empty file is driven.
    #[test]
    fn a_key_is_written_where_its_neighbours_are() {
        let file = "# --- Errors ---\n\
                    \"error.db.gone\" = \"a\"\n\
                    \"error.db.late\" = \"b\"\n\
                    \"error.validation.out-of-range\" = \"c\"\n\
                    \"error.validation.too-long\" = \"d\"\n\
                    \n\
                    # --- Commands ---\n\
                    \"command.journal.verify\" = \"e\"\n\
                    \"command.journal.repair\" = \"f\"\n";
        // **After the last of the group, not the first.** A key filed before its
        // neighbours is still in the right block, so only a group of more than
        // one can tell the two apart.
        assert_eq!(super::where_to_write(file, "error.validation.probe"), 5);
        assert_eq!(super::where_to_write(file, "error.db.other"), 3);
        // **The walk shortens the prefix more than once.** `error.db.x.` matches
        // nothing, `error.db.` matches two lines, and a walk that takes one step
        // and stops falls through to the end of the file instead.
        assert_eq!(super::where_to_write(file, "error.db.x.deeper"), 3);
        assert_eq!(super::where_to_write(file, "command.limits.prune"), 9);
        // A group that does not exist yet goes at the end, which is the honest
        // answer.
        assert_eq!(super::where_to_write(file, "email.welcome.subject"), 9);
        // **And an empty file answers one, not zero.** There is no line zero to
        // send a reader to, and `.count()` without the floor gives exactly that.
        assert_eq!(super::where_to_write("", "error.db.first"), 1);
    }
}

/// What an entry declares it interpolates, against what its sentences do.
///
/// Split out of `check` for its line budget, and the split is where it belongs:
/// this is the only part of the file that reasons about one entry across several
/// messages rather than about one key at a time.
fn params(
    catalog: &Catalog,
    source_file: &str,
    source_text: &str,
    source: &Locale,
    problems: &mut Vec<Problem>,
) {
    //
    // Two checks, not one. Equality would be wrong now that an entry governs
    // more than one message: a subject that uses one of two declared parameters
    // is ordinary, and a subject that uses one nobody declared is not.
    for declaration in declared_params(catalog) {
        let mut used_anywhere = BTreeSet::new();
        for key in &declaration.keys {
            let Some(sentence) = source.message.get(key) else {
                continue;
            };
            let used = placeholders(sentence);
            let undeclared: Vec<&String> = used.difference(&declaration.declared).collect();
            if !undeclared.is_empty() {
                problems.push(Problem::at(
                    source_file,
                    line_of(source_text, &format!("\"{key}\""), 1).unwrap_or(1),
                    format!(
                        "\"{key}\" uses {undeclared:?}, which {} does not declare",
                        declaration.owner
                    ),
                    "the caller is compiled against the declaration, so a placeholder \
                     the sentence uses and the declaration omits can never be supplied \
                     — the reader sees the braces",
                    "param-not-declared",
                ));
            }
            used_anywhere.extend(used);
        }
        let unused: Vec<&String> = declaration.declared.difference(&used_anywhere).collect();
        if !unused.is_empty() {
            problems.push(Problem::at(
                source_file,
                declaration
                    .keys
                    .first()
                    .and_then(|key| line_of(source_text, &format!("\"{key}\""), 1))
                    .unwrap_or(1),
                format!(
                    "{} declares {unused:?}, which none of its sentences uses",
                    declaration.owner
                ),
                "every caller is made to supply it and no reader ever sees it, which is \
                 what a half-finished rename looks like. Remove it from the declaration, \
                 or put it back into the sentence it was taken out of",
                "param-never-used",
            ));
        }
    }
}

/// No locale's value is the source sentence, word for word.
///
/// **Every rule here is about a key's presence and its placeholders**, and none
/// about what the value says — so without this, seven pt-BR sentences replaced
/// verbatim with their English text compile, generate, and publish both
/// language columns of `docs/errors.md` identical, with `check passed`. Test
/// floors phrased as fractions ("more than half differ", "more than 20 of
/// 28") are satisfied comfortably by a handful of untranslated strings.
///
/// Nothing in this catalog legitimately coincides today, so the rule is absolute
/// and the exception list starts empty. A word that is the same in both
/// languages — a product name, an identifier — goes in it with the reason, which
/// is a two-place edit somebody reads.
fn no_value_is_the_source_sentence(
    file: &str,
    tag: &str,
    text: &str,
    locale: &crate::model::Locale,
    source: &crate::model::Locale,
    problems: &mut Vec<Problem>,
) {
    no_value_is_the_source_sentence_except(
        file,
        tag,
        text,
        locale,
        source,
        &exemptions::TheSameInBoth::shipped(),
        problems,
    );
}

/// Keys whose value is legitimately the same sentence in both locales, and why.
///
/// **The diagnostic names this, so it has to exist.** The refusal tells a
/// translator to "add it to `THE_SAME_IN_BOTH` with the reason", and with the
/// list only a parameter there would be nothing to add it to. Empty is the
/// honest state of this catalog; a product name or an identifier that coincides
/// goes here with the reason, which is the two-place edit somebody reads.
///
/// **And the reason is a field.** As `&[&str]` this would be a bare exclusion
/// list: the diagnostic promises "add it to `THE_SAME_IN_BOTH` with the reason
/// it is the same in both", and the only place a reason could go would be a
/// comment nothing reads. Any key could be exempted from the untranslated check
/// by adding six characters, with no floor, no count and nothing to say why —
/// and an exemption whose reason is unwritten is indistinguishable from a
/// translation somebody gave up on. `every_exemption_names_the_reason_it_is_one`
/// is what makes the second column load-bearing rather than decorative.
pub(crate) const THE_SAME_IN_BOTH: &[(&str, &str)] = &[];

/// The exemption list a caller may hand the untranslated rule.
///
/// **The list a test iterates has to be the list the shipped call hands over.**
/// `every_exemption_names_the_reason_it_is_one` walks the constant above — the
/// reason field, the character floor, the word floor, the duplicate check, all
/// of it — and nothing there reads what the live caller actually passes. With
/// a plain list, `&[THE_SAME_IN_BOTH, &[("error.ids.name-too-long", "x")]].concat()`
/// at the call site exempts a key from a rule this catalog calls absolute, with
/// no reason anybody reads: warning-free, `check passed`, transcript
/// byte-identical. That is audit question four exactly, which is whether an
/// exclusion list turns the gate off.
///
/// There is exactly one way to make one of these outside a test, and it hands
/// back the constant. A call site cannot add a row to it, concatenate onto it,
/// or pass a list of its own, because there is no such list to pass.
mod exemptions {
    /// Keys the untranslated rule exempts, and why each one is exempt.
    pub(super) struct TheSameInBoth(&'static [(&'static str, &'static str)]);

    impl TheSameInBoth {
        /// The list this catalog ships, which is the only one production has.
        pub(super) fn shipped() -> Self {
            Self(super::THE_SAME_IN_BOTH)
        }

        /// A list of a test's own, so the rule can be driven in both directions.
        #[cfg(test)]
        pub(super) fn of(rows: &'static [(&'static str, &'static str)]) -> Self {
            Self(rows)
        }

        /// The rows, for the one rule that reads them.
        ///
        /// `const`, because the log's type wall refuses a function that produces
        /// a `&'static str` at run time — such a function is a door into the
        /// wall whatever it calls, and the wall is right that a new one needs a
        /// reason. This one hands back the list it was built from and computes
        /// nothing, which `const fn` is how you say.
        pub(super) const fn rows(&self) -> &'static [(&'static str, &'static str)] {
            self.0
        }
    }
}

/// The same, with the exception list handed in.
///
/// **`THE_SAME_IN_BOTH` is empty, so with it alone the clause that reads it is
/// a branch no input can reach.** The whole mechanism — the constant, the
/// `.contains`, and the diagnostic telling a developer to "add it to
/// `THE_SAME_IN_BOTH` with the reason" — could then be deleted with every test
/// green. An exclusion list nobody has ever exercised is a list nobody knows
/// works, and the first entry would be added by someone trusting a hole that
/// had never been opened.
///
/// The list is a parameter so a test can pass one. The shipped caller passes
/// the empty constant, which is the honest state of this catalog: nothing in it
/// legitimately coincides.
fn no_value_is_the_source_sentence_except(
    file: &str,
    tag: &str,
    text: &str,
    locale: &crate::model::Locale,
    source: &crate::model::Locale,
    the_same_in_both: &exemptions::TheSameInBoth,
    problems: &mut Vec<Problem>,
) {
    for (key, ours) in &locale.message {
        let Some(theirs) = source.message.get(key) else {
            continue;
        };
        if the_same_sentence(ours, theirs)
            && !the_same_in_both
                .rows()
                .iter()
                .any(|(excused, _)| *excused == key.as_str())
        {
            problems.push(Problem {
                file: file.to_owned(),
                line: line_of(text, &format!("\"{key}\""), 1),
                what: format!("{tag}'s \"{key}\" is the {SOURCE_LOCALE} sentence, word for word"),
                fix: "a key present in both locales with the same text is an untranslated \
                      string that every gate here reads as translated — the published page \
                      then shows one sentence in two language columns. Translate it, or add \
                      it to `THE_SAME_IN_BOTH` with the reason it is the same in both"
                    .to_owned(),
                check: "untranslated",
            });
        }
    }
}

/// Whether two sentences are the same one, for the purpose above.
///
/// **Byte equality alone makes the rule one keystroke wide**: a trailing
/// period, a capital, or two spaces between words make an English sentence
/// pasted into `pt-BR.toml` pass as a translation, and every downstream gate —
/// presence, placeholders, both test floors — reads it as translated.
/// Measured, `error.db.migration-failed` in `catalog/i18n/pt-BR.toml` set to the
/// English sentence with a full stop added, under byte equality:
///
/// ```text
/// cargo xtask check   ->  check passed
/// docs/errors.md      ->  both language columns, one sentence
/// ```
///
/// Normalised on the three things a translator never changes on purpose and a
/// copy-paste changes by accident: case, run-together whitespace, and the
/// punctuation a sentence ends on. Anything past that is a judgement about
/// meaning, which is what `THE_SAME_IN_BOTH` and its written reason are for.
fn the_same_sentence(ours: &str, theirs: &str) -> bool {
    fn plainly(text: &str) -> String {
        // **What a reader cannot see is not what tells two messages apart.**
        // `split_whitespace` drops whitespace and nothing else, so with it alone
        // one `U+2063 INVISIBLE SEPARATOR` appended to the English sentence
        // would make it a different string here and the same sentence on the
        // page. Measured on `NC-JRNL-0001` — `critical`, non-retryable, the
        // journal-tampering message: with whitespace alone dropped,
        // `cargo xtask build` accepts it and `docs/errors.md` publishes English
        // in the pt-BR column.
        //
        // **And the escapers' set is a different question.** `shows_nothing`
        // answers `INVISIBLE_RANGES`, and what that array holds is replaced by a
        // *space* in every doc line and every published cell — so it can only
        // ever hold characters that stand between words, and
        // `U+3164 HANGUL FILLER` is a letter that stands inside them. Read
        // through that set, the English sentence plus `U+3164` filed as the
        // pt-BR `error.journal.chain-broken` gives `Ok, 0 problem(s)` and puts
        // that English sentence in the pt-BR column of `NC-JRNL-0001` in
        // `docs/errors.md`. `tells_two_sentences_apart` is this question's own
        // predicate, and the only reader of `BLANK_RANGES`.
        // **What a reader sees, not what the bytes are.** Deleting every
        // character `tells_two_sentences_apart` refuses is not enough, because
        // `printable` — which writes every `///` line and every published cell —
        // replaces the invisible half of that set with a space. So the English
        // sentence with one of its spaces written `U+200D` would normalise here
        // to `oldrecords` while publishing as `Old records`: the two columns of
        // `docs/errors.md` carry the identical English sentence and this rule,
        // whose whole job is to notice that, compares two different strings.
        // Measured, filed as the pt-BR `error.db.unreachable`:
        // `ACCEPTED, 0 problem(s)`.
        //
        // Substituting what `printable` substitutes makes this normalise to the
        // sentence the page actually publishes. What stays deleted is
        // `BLANK_RANGES`, which sits *inside* words and which nothing that
        // writes a file reads.
        let folded: String = text
            .chars()
            .filter_map(|character| {
                if crate::shows_nothing(character) {
                    Some(' ')
                } else if crate::tells_two_sentences_apart(character) {
                    Some(character)
                } else {
                    None
                }
            })
            .collect::<String>()
            .to_lowercase();
        let words: Vec<&str> = folded.split_whitespace().collect();
        words
            .join(" ")
            .trim_matches(|character: char| {
                matches!(character, '.' | '!' | '?' | ':' | ';' | ',' | '…')
            })
            .to_owned()
    }
    plainly(ours) == plainly(theirs)
}

/// A key that is present and says nothing.
///
/// **The completeness rules are about a key's *presence*; none of them looks at
/// the value.** Without this rule `""` and `"   "` both pass every rule in this
/// file — the first is caught downstream by a generated-crate assertion, and
/// the second only by a Markdown cell that happens to trim, so an ordinary
/// tidy-up to that comparison would ship a blank translation with `check passed`.
///
/// Asked of every locale including the source, because an empty English sentence
/// is a page with a gap in it.
fn no_key_is_blank(
    file: &str,
    tag: &str,
    text: &str,
    locale: &crate::model::Locale,
    problems: &mut Vec<Problem>,
) {
    for (key, value) in &locale.message {
        // **What a reader sees, not what `char::is_whitespace` answers.** The
        // rest of this file asks `shows_nothing`, and asked as
        // `value.trim().is_empty()` a value of three
        // `U+2063 INVISIBLE SEPARATOR` characters is accepted: the published
        // cell is empty, and `nc_catalog::message` hands a caller a string a
        // reader sees nothing of. Its own doc says this rule exists because `""`
        // and `"   "` would pass every other rule in this file; the third
        // spelling of the same thing would too.
        if value
            .chars()
            .any(|shown| !shown.is_whitespace() && crate::tells_two_sentences_apart(shown))
        {
            continue;
        }
        problems.push(Problem {
            file: file.to_owned(),
            // **The line the key is on, not the line it should be written on.**
            // Not `where_to_write`, whose own doc says it answers *"the
            // 1-indexed line a key that is **not** in this file should be
            // written on"* — and the key this diagnostic is about **is** in the
            // file. Measured through `where_to_write`, three blanked keys are
            // each reported seven lines late, each landing on a neighbouring key
            // with a full sentence in it: a reader following the location finds
            // healthy prose and no gap. `where_to_write`'s call sites are about
            // keys genuinely absent.
            line: crate::problem::line_of(text, &format!("\"{key}\""), 1)
                .or_else(|| Some(where_to_write(text, key))),
            what: format!("{tag}'s \"{key}\" is blank"),
            fix: "a key that is present and says nothing reads as translated and \
                  renders as a gap. Write the sentence, or remove the key from every \
                  locale — `missing-key` is the rule that keeps the two in step"
                .to_owned(),
            check: "blank-message",
        });
    }
}

/// Checks every locale against the source locale, and the registries against
/// both.
pub(crate) fn check(root: &Path, catalog: &Catalog, problems: &mut Vec<Problem>) {
    let Some(source) = catalog
        .locales
        .iter()
        .find(|locale| locale.meta.bcp47 == SOURCE_LOCALE)
    else {
        // `load` already reported the absence; without a source locale there is
        // nothing to compare against, so reporting every key of every other
        // locale as unexpected would bury it.
        return;
    };

    let source_file = format!("catalog/i18n/{SOURCE_LOCALE}.toml");
    let source_text = std::fs::read_to_string(root.join(&source_file)).unwrap_or_default();

    // Every key a registry points at has to resolve. This is the check that
    // makes "a notification that no longer exists is a compile error" true in
    // the other direction too: an entry pointing at a key nobody wrote.
    for (key, owner) in referenced_keys(catalog) {
        if !source.message.contains_key(&key) {
            // Located at the end of the file it is missing from, because that is
            // where the key has to be written. A diagnostic about an absent key
            // cannot point at the key, but it can point at the group the key
            // belongs to — see `where_to_write`.
            problems.push(Problem::at(
                &source_file,
                where_to_write(&source_text, &key),
                format!("{owner} points at \"{key}\", which no locale declares"),
                "the registry entry and the sentence are written together; add the key \
                 here first, because English is the source locale",
                "dangling-message-key",
            ));
        }
    }

    params(catalog, &source_file, &source_text, source, problems);

    // **And the other direction: every key is one a registry entry points at.**
    // Without it, a key present in every locale that no registry entry points
    // at would compile, land in `KEYS` and `MESSAGES`, and appear on no page —
    // a sentence translated into every language and shown to nobody,
    // maintained forever by people who cannot tell it is dead. That is the same
    // argument `orphan-key` is built on, one file over, and `referenced_keys`
    // already computes the set it needs.
    //
    // `nc_catalog::message` takes a `&str`, so a key no registry names is not
    // *provably* dead — nothing calls it with a literal today, and a caller that
    // one day does adds the key to a registry, which is where a sentence a user
    // can see belongs anyway.
    let referenced: BTreeSet<String> = referenced_keys(catalog).into_keys().collect();
    for key in source.message.keys() {
        if !referenced.contains(key) {
            problems.push(Problem::at(
                &source_file,
                line_of(&source_text, &format!("\"{key}\""), 1).unwrap_or(1),
                format!("\"{key}\" is a sentence no registry entry points at"),
                "every message key is reached through an error, notification, email or \
                 command, and one reached through none is shown nowhere — while every \
                 locale is still asked to carry it. Point an entry at it, or delete it \
                 from every locale",
                "unreferenced-key",
            ));
        }
    }

    // **Asked of the source before the loop that skips it.** `no_key_is_blank`'s
    // doc says it is asked of every locale "including the source, because an
    // empty English sentence is a page with a gap in it" — and were it called
    // only inside this loop, past the `continue` that skips the source,
    // `"error.db.prune-failed" = ""` in the source locale would compile clean,
    // while a test that edits only the translation would stay green.
    //
    // The generated surface does not catch it either: it asserts the sentence
    // is non-empty, which `""` fails and `"   "` passes — and whitespace is the
    // case this check exists for.
    no_key_is_blank(&source_file, SOURCE_LOCALE, &source_text, source, problems);
    for locale in &catalog.locales {
        if locale.meta.bcp47 == SOURCE_LOCALE {
            continue;
        }
        // (Both missing-key diagnostics point through `where_to_write`.)
        let file = format!("catalog/i18n/{}.toml", locale.meta.bcp47);
        let text = std::fs::read_to_string(root.join(&file)).unwrap_or_default();
        // The tag, never `meta.name`: the endonym is written in the locale's own
        // language, and build output is English (`§2.1`).
        let tag = &locale.meta.bcp47;

        no_value_is_the_source_sentence(&file, tag, &text, locale, source, problems);

        // Missing here. Named individually: "3 keys missing" sends a translator
        // hunting and "error.db.prune-failed is missing" does not.
        for key in source.message.keys() {
            if !locale.message.contains_key(key) {
                problems.push(Problem {
                    file: file.clone(),
                    line: Some(where_to_write(&text, key)),
                    what: format!("{tag} is missing \"{key}\""),
                    fix: format!(
                        "every locale carries every key. A lookup that quietly fell back \
                         to English would ship a page in two languages and report \
                         nothing — the sentence to translate is in {source_file}"
                    ),
                    check: "missing-key",
                });
            }
        }

        no_key_is_blank(&file, tag, &text, locale, problems);

        // And the other direction.
        for key in locale.message.keys() {
            if !source.message.contains_key(key) {
                problems.push(Problem {
                    file: file.clone(),
                    line: line_of(&text, &format!("\"{key}\""), 1),
                    what: format!("{tag} has \"{key}\", which {SOURCE_LOCALE}.toml does not"),
                    fix: "English is the source locale: add the key there, or delete it \
                          here. A translation of a sentence that no longer exists is \
                          maintained forever by people who cannot tell"
                        .to_owned(),
                    check: "orphan-key",
                });
            }
        }

        // Placeholders.
        for (key, sentence) in &locale.message {
            let Some(original) = source.message.get(key) else {
                continue;
            };
            let ours = placeholders(sentence);
            let theirs = placeholders(original);
            if ours != theirs {
                problems.push(Problem {
                    file: file.clone(),
                    line: line_of(&text, &format!("\"{key}\""), 1),
                    what: format!(
                        "{tag}'s \"{key}\" uses {ours:?} where {SOURCE_LOCALE}.toml uses {theirs:?}"
                    ),
                    fix: "a placeholder the caller does not supply is rendered literally, \
                          so the reader sees the braces. Translate the words around a \
                          placeholder, never the placeholder"
                        .to_owned(),
                    check: "placeholders-differ",
                });
            }
        }
    }
}

/// `placeholders`, for a test that has to drive the real thing.
///
/// **The gate comparing this with the emitted renderer runs this, not a
/// hand-written copy of it**, as the renderer half has a door for the same
/// reason. Measured against a copy: this predicate widened to accept `.` — so
/// the compiler declares `{user.name}` a placeholder while the renderer calls
/// it prose — leaves `cargo xtask check` passing, with the corpus holding a row
/// for that very sentence and a comment saying "neither side accepts it".
#[doc(hidden)]
#[must_use]
pub fn placeholders_for_tests(sentence: &str) -> Vec<String> {
    placeholders(sentence).into_iter().collect()
}

/// The `{name}` placeholders in a sentence.
///
/// A set rather than a list, because a message may legitimately repeat one and
/// the caller supplies it once either way.
pub(crate) fn placeholders(sentence: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut rest = sentence;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else { break };
        let name = &rest[..close];
        // A brace pair with anything but a bare name in it is prose — a JSON
        // fragment in an example, or a set written out — and not a placeholder.
        if !name.is_empty()
            && name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            found.insert(name.to_owned());
        }
        rest = &rest[close + 1..];
    }
    found
}

#[cfg(test)]
mod tests {
    use super::placeholders;

    /// The exemption silences the key it names, and only that key.
    ///
    /// **The hole, not just the rule** — the `SKIP`/`COVERED` pattern
    /// `NOCTUA.md §19.4` states and `nc-log` already follows. `THE_SAME_IN_BOTH`
    /// is empty, so `.contains(…)` over it is constant-false and the exemption
    /// arm a branch no input reaches: without this, the constant, the clause and
    /// the diagnostic naming it could all be deleted with every test green, and
    /// the first person to use the list would be trusting a hole nobody had
    /// opened.
    #[test]
    fn the_exemption_silences_the_key_it_names_and_no_other() {
        let locale = |pairs: &[(&str, &str)]| crate::model::Locale {
            meta: crate::model::LocaleMeta {
                name: "português".to_owned(),
                bcp47: "pt-BR".to_owned(),
                dir: "ltr".to_owned(),
            },
            message: pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        };
        let both = locale(&[("brand.name", "Noctua"), ("limit.reached", "Limit reached")]);

        let said = |excused: &'static [(&'static str, &'static str)]| {
            let mut problems = Vec::new();
            super::no_value_is_the_source_sentence_except(
                "catalog/i18n/pt-BR.toml",
                "pt-BR",
                "\"brand.name\"\n\"limit.reached\"\n",
                &both,
                &both,
                &super::exemptions::TheSameInBoth::of(excused),
                &mut problems,
            );
            problems
                .into_iter()
                .map(|problem| problem.what)
                .collect::<Vec<_>>()
        };

        let none = said(&[]);
        assert_eq!(
            none.len(),
            2,
            "both keys are the source sentence and the rule is absolute: {none:?}"
        );

        let one = said(&[("brand.name", "the product name, which is not translated")]);
        assert_eq!(
            one.len(),
            1,
            "the exemption silenced something other than the one key it names: {one:?}"
        );
        assert!(
            one[0].contains("limit.reached"),
            "the exemption silenced the wrong key: {one:?}"
        );
    }

    /// Whether an exemption's reason says why the sentence is the same in both.
    ///
    /// Prose, not a placeholder: a reason that is a word, a `-`, or the key
    /// written back is the same unexplained hole with a second column drawn
    /// around it. Twenty characters and four words is the floor, and the key
    /// restated is refused whatever its length.
    ///
    /// A function rather than three assertions inside the loop, so the rule can
    /// be driven over inputs — which, over an empty list, is the only way it can
    /// be driven at all.
    fn a_reason_is_prose(key: &str, reason: &str) -> bool {
        let words = reason.split_whitespace().count();
        reason.trim().len() >= 20 && words >= 4 && reason.trim() != key
    }

    /// Every exemption names the reason it is one, or there are none.
    ///
    /// **A list with no floor, no count and no field for a reason.** As
    /// `&[&str]` a key would be exempted from the untranslated check by adding
    /// six characters, and the diagnostic's own promise — "add it to
    /// `THE_SAME_IN_BOTH` with the reason it is the same in both" — would have
    /// nowhere to put the reason. What that buys is a published page showing one
    /// sentence in two language columns with nothing in the tree saying it was
    /// meant.
    ///
    /// So the reason is asserted to be prose rather than a placeholder. Empty is
    /// the honest state of this catalog and this test says so: it passes over an
    /// empty list, and the moment a first entry is added it has something to
    /// hold.
    #[test]
    fn every_exemption_names_the_reason_it_is_one() {
        // **The list the shipped rule is handed, not the constant beside it.**
        // The list sits behind a constructor whose doc says there is exactly one
        // way to make one outside a test and it hands back the constant — and a
        // test walking `THE_SAME_IN_BOTH` while the rule walks
        // `shipped().rows()` checks nothing of that: the seam moves one frame
        // inward and a key can be exempted, with an empty reason, from a rule
        // this catalog calls absolute. The naive form is caught only by
        // `-D dead-code` noticing an unreferenced constant, which is the
        // compiler objecting to tidiness rather than a gate objecting to a hole.
        // **A loop that cannot iterate.** `THE_SAME_IN_BOTH` is `&[]`, so
        // assertions inside a `for` over it alone would sit over an empty
        // slice: the reason predicate would run on no input, and the rule that
        // makes the second column load-bearing would itself be the unexercised
        // hole `the_exemption_silences_the_key_it_names_and_no_other` exists to
        // refuse one level down. Every clause of it could be deleted, or
        // weakened to `len() >= 1`, with the suite green — and the first person
        // to add an exemption would be trusting it.
        //
        // So it is driven over a written-out table, and the list is checked
        // with the same predicate. Empty is the honest state of this catalog,
        // and the rule works before there is anything for it to work on.
        for (key, reason) in [
            ("brand.name", "the product name, which is not translated"),
            (
                "error.db.class",
                "both locales quote the SQLSTATE class exactly as PostgreSQL prints it",
            ),
            // Twenty characters and four words, which is the floor from above.
            ("brand.name", "aaaa bbbb cccc ddddd"),
        ] {
            assert!(
                a_reason_is_prose(key, reason),
                "`{reason}` says why the sentence is the same in both and was refused, \
                 so a real exemption cannot be written"
            );
        }
        for (key, reason) in [
            // Nothing at all, which is what an unexplained hole looks like.
            ("brand.name", ""),
            ("brand.name", "-"),
            // A word.
            ("brand.name", "untranslated"),
            // Nineteen characters over four words: the floor from below.
            ("brand.name", "a b c ddddddddddddd"),
            // Twenty characters over three words: the other floor from below.
            ("brand.name", "aaaaaa bbbbbb cccccc"),
            // The key written back, long enough and wordy enough to pass
            // everything else.
            ("the product brand name", "the product brand name"),
        ] {
            assert!(
                !a_reason_is_prose(key, reason),
                "`{reason}` does not say why `{key}` is the same sentence in both \
                 languages and was accepted, so an exemption can be added with six \
                 characters and nothing anybody reads"
            );
        }

        let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        for (key, reason) in super::exemptions::TheSameInBoth::shipped().rows() {
            assert!(
                !key.trim().is_empty(),
                "an exemption with no key silences nothing and is a line nobody can read"
            );
            assert!(
                seen.insert(key),
                "`{key}` is exempted twice, and the second reason is one nobody applies"
            );
            assert!(
                a_reason_is_prose(key, reason),
                "`{key}` is exempted from the untranslated check with the reason \
                 `{reason}`, which does not say why the sentence is the same in both \
                 languages. A reader has to be able to judge the exemption without \
                 opening the locale files"
            );
        }
    }

    #[test]
    fn a_sentence_yields_the_names_it_interpolates() {
        let found = placeholders("{field} must be between {min} and {max}.");
        assert_eq!(found.len(), 3);
        assert!(found.contains("field") && found.contains("min") && found.contains("max"));
    }

    #[test]
    fn a_sentence_with_no_placeholders_yields_none() {
        let got = placeholders("Noctua is working.");
        assert!(got.is_empty(), "{got:?}");
    }

    #[test]
    fn a_repeated_placeholder_is_one_name() {
        assert_eq!(placeholders("{name}, {name}").len(), 1);
    }

    #[test]
    fn braces_around_something_that_is_not_a_name_are_prose() {
        // The alternative — treating every brace pair as a placeholder — would
        // make any message quoting a JSON fragment fail the cross-locale check
        // for a difference that is not one.
        let got = placeholders(r#"such as {"type": "object"}"#);
        assert!(got.is_empty(), "{got:?}");
        let got = placeholders("{}");
        assert!(got.is_empty(), "{got:?}");
        let got = placeholders("{ spaced }");
        assert!(got.is_empty(), "{got:?}");
    }
}

/// As [`the_same_sentence`], for a test that drives its punctuation set.
pub(crate) fn the_same_sentence_for_tests(ours: &str, theirs: &str) -> bool {
    the_same_sentence(ours, theirs)
}
