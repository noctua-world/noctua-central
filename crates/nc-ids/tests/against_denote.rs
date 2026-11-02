//! Every slug this crate produces, against what real Denote produced.
//!
//! `NOCTUA.md §16.3` makes Denote the authority for file naming, and a claim of
//! fidelity that nobody can re-run is prose. `denote-corpus.tsv` beside this file
//! holds what GNU Emacs 30.2 and `denote.el` 4.1.0 actually returned, row by
//! row, together with the command that produced it, so the claim can be
//! checked again the day Denote changes.
//!
//! **The deviations are listed here, by row, with their reason.** A test that
//! merely skipped the rows it failed would be a test that measured nothing; each
//! exception below names a character a file name cannot hold, which is the only
//! kind of addition `§16.3` permits.

use nc_ids::{FileName, Snowflake};

/// The corpus, as (typed, title, signature, keyword).
fn corpus() -> Vec<(String, String, String, String)> {
    let text = include_str!("denote-corpus.tsv");
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let mut next = || fields.next().unwrap_or_default().to_owned();
            (next(), next(), next(), next())
        })
        .collect()
}

fn id() -> Snowflake {
    Snowflake::from_crockford("01J8XK2M9P51").expect("a valid identifier")
}

/// The inputs whose slug this repository deliberately does not match, and why.
///
/// Each holds a character Windows rejects in a path, which Denote's own set does
/// not remove. Nothing else in the corpus may appear here: an entry is a
/// documented deviation, and an undocumented one is a defect.
///
/// The list said "Both" while holding three, because the third was added by this
/// test finding it rather than by somebody writing it down.
const DEVIATIONS: [(&str, &str); 3] = [
    (
        "with\\backslash",
        "Windows reads a backslash as a path separator, so a name holding one is two names",
    ),
    (
        "less < than",
        "`<` is one of the characters Windows rejects outright in a file name",
    ),
    (
        "greater > than",
        "`>` likewise, and it was found by this test rather than listed in advance",
    ),
];

#[test]
fn every_title_matches_denote() {
    let mut checked = 0_usize;
    let mut deviated = Vec::new();
    for (typed, expected, _, _) in corpus() {
        let built = FileName::new(&[], Some(&typed), &[], id(), None);
        let ours: String = built.title().unwrap_or_default().to_owned();
        if ours == expected {
            checked += 1;
        } else {
            deviated.push((typed, expected, ours));
        }
    }
    // The count first: a corpus that failed to load would otherwise pass with
    // nothing compared, which is `§19.3`'s gate-wired-to-nothing in a test.
    //
    // **Every row, with no slack.** This floor was `checked > 30` against a
    // corpus of 44, and the slack was measured: thirteen rows could be deleted
    // and the test stayed green, and rows of pure punctuation — which slug to
    // nothing, so `ours == expected == ""` and each still counted as checked —
    // could be substituted for real ones. A floor with room in it is a floor
    // that lets the corpus rot down to it. Every row is compared or is a
    // documented deviation, and there is no third possibility.
    assert_eq!(
        checked + deviated.len(),
        corpus().len(),
        "a row was neither compared nor a documented deviation"
    );
    assert_eq!(
        checked,
        corpus().len() - DEVIATIONS.len(),
        "only {checked} of {} rows matched Denote",
        corpus().len()
    );

    // **And every row says something.** A row whose expected slug holds no
    // letter or digit agrees with Denote about the empty string, which is what
    // let twelve substituted rows pass. This is equality, not a floor, for the
    // same reason as above — and it runs before the coverage checks below, which
    // read the typed inputs and would otherwise be satisfied by filler.
    let substantive = corpus()
        .iter()
        .filter(|(_, expected, ..)| expected.chars().any(char::is_alphanumeric))
        .count();
    assert_eq!(
        substantive,
        corpus().len(),
        "{} row(s) slug to nothing, and a row that agrees with Denote about the empty \
         string proves nothing",
        corpus().len() - substantive
    );

    // **Every character this module removes appears in some row.** `"` and `‘`
    // appeared in none, so deleting both from `REMOVED` left the whole workspace
    // green — and produced `--say-"hi"@@…` and `--‘hello-there@@…`, names Denote
    // would never write. The doc says "the four that were missing are the curly
    // quotes"; three of the four were in the corpus.
    //
    // **And for six of the thirty this decides nothing**, which the sentence it
    // carries does not say. `:`, `"`, `/`, `|`, `?` and `*` are in `FORBIDDEN`
    // as well, and that list runs on every component — so deleting one of the
    // six from `REMOVED` changes no output, measured with `'"'` duplicated out
    // of the array and `cargo test -p nc-ids` green. The row is still worth
    // requiring: it keeps the corpus exercising the character. What it cannot
    // claim is that the corpus is what decides it, and
    // `the_two_lists_overlap_where_they_say_they_do` names the six.
    let typed_inputs: String = corpus().iter().map(|(typed, ..)| typed.clone()).collect();
    for removed in nc_ids::naming::REMOVED {
        assert!(
            typed_inputs.contains(removed),
            "`{removed}` (U+{:04X}) is removed from every component and appears in no \
             row, so deleting it from the rule would go unnoticed",
            removed as u32
        );
    }

    for (typed, expected, ours) in &deviated {
        assert!(
            DEVIATIONS.iter().any(|(input, _)| input == typed),
            "`{typed}`: Denote writes `{expected}` and this writes `{ours}`, and \
             nothing in DEVIATIONS says why"
        );
    }
    assert_eq!(
        deviated.len(),
        DEVIATIONS.len(),
        "a documented deviation stopped happening; if the rule changed, remove its \
         entry, and if the corpus changed, say so there"
    );
}

/// A behaviour of `§16.3` that no row exercises is a rule that could be deleted.
#[test]
fn the_corpus_exercises_every_rule_it_is_the_authority_for() {
    // **What the corpus has to cover, named rather than counted.** A row count
    // is not coverage: 44 rows of `MiXeD CaSe` would clear any floor and prove
    // one rule. Each entry below is a behaviour `§16.3` requires, and a corpus
    // that stops exercising one fails here by name instead of by arithmetic —
    // which is the difference between "the corpus shrank" and "nothing checks
    // that spaces collapse any more".
    /// One behaviour `§16.3` requires, and whether a row exercises it.
    ///
    /// **Measured**: with these predicates, greedily deleting every row the file
    /// still passes without reduces 44 rows to 23 — and that reduced corpus
    /// *does* catch a `slug_keyword` that stops removing `-`, which the previous
    /// table did not. What is left over is redundancy, not coverage: several rows
    /// exercise each behaviour and only one of each has to survive.
    ///
    /// **Every predicate reads the slug, not only what was typed.** Four of
    /// these looked at the typed column alone, which makes them a claim about
    /// the corpus's *input alphabet* rather than about a rule — and a corpus
    /// reduced to rows that merely contain the right characters satisfied them
    /// while a real regression went green. A behaviour is a relation between
    /// what was typed and what came out, so a predicate that names only one side
    /// is not describing the behaviour.
    type Covers = (&'static str, fn(&str, &str) -> bool);
    let covers: [Covers; 14] = [
        ("a space becomes a hyphen", |typed, slug| {
            typed.contains(' ') && slug.contains('-') && !slug.contains(' ')
        }),
        ("a run of spaces collapses to one hyphen", |typed, slug| {
            typed.contains("  ") && !slug.contains("--")
        }),
        (
            "leading and trailing separators are trimmed",
            |typed, slug| {
                let edge = |c: char| c == ' ' || c == '-' || c == '_' || c == '=';
                (typed.starts_with(edge) || typed.ends_with(edge))
                    && !slug.starts_with(edge)
                    && !slug.ends_with(edge)
            },
        ),
        ("case is folded", |typed, slug| {
            typed != typed.to_lowercase() && slug == slug.to_lowercase()
        }),
        // **The one the reduction walked through.** This asked only whether the
        // typed text held a doubled marker, so a corpus with no hyphen in it at
        // all still satisfied it — and `slug_keyword` could stop removing `-`
        // with this file green. It now requires the collapse to be visible.
        ("a doubled hyphen collapses to one", |typed, slug| {
            typed.contains("--") && !slug.contains("--") && slug.contains('-')
        }),
        // **Two predicates, not one `||`.** They were joined, and `==equals==`
        // satisfied the pair on its own — so the greedy reduction could delete
        // `a_b__c`, the only row with an underscore in it, and nothing was left
        // exercising `_`. `slug` then stopped treating `_` as a word boundary
        // with this file green, which builds `--a_b__c@@…` and reparses it as
        // title `a_b` with keyword `c`: one document silently split in two, and
        // `parse(build(x)) == x` broken. An `||` between two rules is one rule
        // and a spare.
        ("an underscore is a word boundary", |typed, slug| {
            typed.contains('_') && !slug.contains('_') && slug.contains('-')
        }),
        ("a doubled underscore collapses", |typed, slug| {
            typed.contains("__") && !slug.contains("__")
        }),
        ("a doubled equals collapses", |typed, slug| {
            typed.contains("==") && !slug.contains("==")
        }),
        (
            "a removed character joins what it separated",
            |typed, slug| typed.contains('/') && !slug.contains('/') && slug.contains("withslash"),
        ),
        ("a straight apostrophe is removed", |typed, slug| {
            typed.contains('\'') && !slug.contains('\'')
        }),
        ("a curly quote is removed", |typed, slug| {
            (typed.contains('’') || typed.contains('“') || typed.contains('‘'))
                && !slug.contains(['’', '“', '‘'])
        }),
        ("a non-Latin script survives whole", |typed, slug| {
            let outside_latin =
                |text: &str| text.chars().any(|c| ('\u{0600}'..='\u{9FFF}').contains(&c));
            outside_latin(typed) && outside_latin(slug)
        }),
        ("a Latin diacritic survives", |typed, slug| {
            let marked = |text: &str| {
                text.chars()
                    .any(|c| c.is_alphabetic() && !c.is_ascii() && (c as u32) < 0x0600)
            };
            marked(typed) && marked(slug)
        }),
        ("a symbol that is not removed survives", |typed, slug| {
            let kept = |text: &str| text.contains(['€', '£', '°']);
            kept(typed) && kept(slug)
        }),
    ];
    for (behaviour, holds) in covers {
        assert!(
            corpus()
                .iter()
                .any(|(typed, expected, ..)| holds(typed, expected)),
            "no row exercises `{behaviour}`, so the rule for it could be deleted \
             and this corpus would still agree with Denote"
        );
    }
}

#[test]
fn every_signature_and_keyword_matches_denote() {
    // The other two components, which have their own removal sets and their own
    // joiners — a title that matched while a keyword did not would be a rule
    // applied in one place.
    let mut checked = 0_usize;
    for (typed, _, signature, keyword) in corpus() {
        if DEVIATIONS.iter().any(|(input, _)| *input == typed) {
            continue;
        }
        let built = FileName::new(&[&typed], None, &[&typed], id(), None);
        let ours_signature = built.signature().join("=");
        let ours_keyword = built.keywords().join("_");
        assert_eq!(ours_signature, signature, "signature of `{typed}`");
        assert_eq!(ours_keyword, keyword, "keyword of `{typed}`");
        checked += 1;
    }
    // Every row but the documented deviations, with no slack — see the floor in
    // `every_title_matches_denote` for what the slack was measured hiding.
    assert_eq!(
        checked,
        corpus().len() - DEVIATIONS.len(),
        "only {checked} of {} rows were compared",
        corpus().len()
    );
}

/// A signature is split where `parse` says, and a hyphen is not that place.
///
/// **The delimiter `parse` splits a signature on was decided by nothing.**
/// Measured on 2026-09-08, `text.split('=')` changed to `text.split('-')` in
/// `FileName::parse`: `cargo xtask check` green with every printed line
/// identical, all eight property tests green, and
///
/// ```text
/// ==rio-de-janeiro--praia@@01J8XK2M9P51.jpg
///   signature ["riodejaneiro"]  ->  ["rio", "de", "janeiro"]
///   canonical_name proposes     ->  ==rio=de=janeiro--praia@@01J8XK2M9P51.jpg
/// ```
///
/// — a name that parses differently from how it was built, proposing a rename
/// nobody asked for, which is criterion 8's own stated failure. Nothing could
/// see it: `parsing_a_built_name_returns_what_built_it` only ever feeds `parse`
/// what `build` wrote, and `build` never renders a `-` inside a signature; the
/// raw-name generator drew signatures from an alphabet with no `-` in it
/// (repaired beside this); and the corpus row that settles it — `a-b` reading as
/// the one signature `ab`, measured against Emacs 30.2 — goes to
/// `FileName::new`, which re-splits and re-slugs whatever it is given.
///
/// The sibling line one screen down, the keywords' `split('_')`, was always
/// pinned, because *that* generator's alphabet holds its own separator.
#[test]
fn a_signature_is_split_on_its_own_delimiter() {
    let name = |text: &str| format!("{text}@@{}", id());
    for (written, signature) in [
        // The delimiter, doing its work.
        ("==a=b", vec!["a", "b"]),
        ("==rio=de=janeiro", vec!["rio", "de", "janeiro"]),
        // And a hyphen, which is *removed* rather than split on — the corpus row
        // `a-b -> ab`, read from a name instead of built into one.
        ("==a-b", vec!["ab"]),
        ("==rio-de-janeiro", vec!["riodejaneiro"]),
        // A space *is* one, because `=` is the signature's joiner and slugging
        // writes a space as the joiner — the corpus says so and this reads it
        // from a name rather than building one. A dot is not in this list
        // because it opens the extension, which is a component boundary rather
        // than a spelling of one signature.
        ("==a b", vec!["a", "b"]),
    ] {
        let read = FileName::parse(&name(written)).expect("a name a person could type");
        assert_eq!(
            read.signature(),
            signature,
            "`{written}` is one signature split where `parse` says"
        );
    }
    // The keyword half, in the same shape: its own delimiter and nothing else.
    for (written, keywords) in [
        ("__x_y", vec!["x", "y"]),
        ("__x-y", vec!["xy"]),
        ("__x=y", vec!["xy"]),
    ] {
        let read = FileName::parse(&name(written)).expect("a name a person could type");
        assert_eq!(
            read.keywords(),
            keywords,
            "`{written}` is a keyword list split where `parse` says"
        );
    }
}

#[test]
fn a_name_with_a_repeated_marker_is_split_where_denote_splits_it() {
    // What `denote-retrieve-filename-{identifier,signature,title,keywords}`
    // returned for each of these, run through Emacs 30.2:
    //
    //   --a--b@@01J8XK2M9P51           id=01J8XK2M9P51 sig=nil  title="a--b" kw=nil
    //   __x__y@@01J8XK2M9P51           id=01J8XK2M9P51 sig=nil  title=nil    kw="x"
    //   ==s1==s2--t@@01J8XK2M9P51.jpg  id=01J8XK2M9P51 sig="s1" title="t"    kw=nil
    //
    // Only a name somebody renamed by hand can hold a marker twice, and that is
    // exactly what `parse` is for. This used to read from the right and answered
    // `b` where Denote answers `a--b`, silently dropping half of what was typed.
    //
    // The title is the odd one and it is Denote's oddity, not a mistake here:
    // `denote-title-regexp`'s alternation is `\(==.*\|__.*\|@@.*\|\..*\)*$`,
    // with **no** `--.*`, so a second `--` does not end a title where it does end
    // every other component. The slug then collapses `a--b` to `a-b`.
    let doubled = FileName::parse("--a--b@@01J8XK2M9P51").expect("parses");
    assert_eq!(doubled.title(), Some("a-b"));

    let keywords = FileName::parse("__x__y@@01J8XK2M9P51").expect("parses");
    assert_eq!(keywords.keywords(), ["x"]);
    assert_eq!(keywords.title(), None);

    let signature = FileName::parse("==s1==s2--t@@01J8XK2M9P51.jpg").expect("parses");
    assert_eq!(signature.signature(), ["s1"]);
    assert_eq!(signature.title(), Some("t"));
    assert_eq!(signature.extension(), Some("jpg"));
}

#[test]
fn a_component_cannot_begin_with_a_dot_because_the_regexp_says_so() {
    // `denote-{title,keywords,signature}-regexp` capture `[^.]+?`, which cannot
    // match a dot — so a marker followed immediately by one yields nothing at
    // that position. Measured with Emacs 30.2:
    //
    //   --.x@@01J8XK2M9P51   title=nil kw=nil sig=nil
    //   __.k@@01J8XK2M9P51   title=nil kw=nil sig=nil
    //   ==.s@@01J8XK2M9P51   title=nil kw=nil sig=nil
    //
    // This used to answer `x`, `k` and `s`: the scan began one character in, so
    // a leading dot was swallowed as content rather than refusing the match.
    let dotted = FileName::parse("--.x@@01J8XK2M9P51").expect("parses");
    assert_eq!(dotted.title(), None);
    let keywords = FileName::parse("__.k@@01J8XK2M9P51").expect("parses");
    assert!(keywords.keywords().is_empty(), "{:?}", keywords.keywords());
    let signature = FileName::parse("==.s@@01J8XK2M9P51").expect("parses");
    assert!(
        signature.signature().is_empty(),
        "{:?}",
        signature.signature()
    );

    // And the case that looks the same and is not: a marker followed by anything
    // else runs to the end of the name, which is what Denote answers for
    // `--@@01J8XK2M9P51` — title `@@01J8XK2M9P51`, sluggified.
    let bare = FileName::parse("--@@01J8XK2M9P51").expect("parses");
    assert_eq!(bare.title(), Some("01j8xk2m9p51"));
}

/// The two removal lists overlap in exactly six characters.
///
/// **The corpus assertion over `REMOVED` is vacuous for those six**, because
/// `FORBIDDEN` strips them from every component and `REMOVED` only from a title,
/// a signature and a keyword. Deleting one of the six from `REMOVED` changes no
/// output — measured, `'"'` duplicated out of the array with `cargo test -p
/// nc-ids` green at 97, 5 and 8 — so a reader taking that assertion's sentence
/// at face value believes a fifth of the list is held by a corpus that cannot
/// move.
///
/// Written down as a number so the overlap cannot grow quietly: a character
/// added to `FORBIDDEN` that is already in `REMOVED` takes one more row out of
/// the corpus's reach, and that is a thing to decide rather than to discover.
#[test]
fn the_two_lists_overlap_where_they_say_they_do() {
    // `FORBIDDEN` is private, so the set is written out here — which is the
    // point: a comparison relates two things, and one of them has to be
    // independent of the thing it is about.
    const WINDOWS_FORBIDS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let mut both: Vec<char> = nc_ids::naming::REMOVED
        .iter()
        .copied()
        .filter(|character| WINDOWS_FORBIDS.contains(character))
        .collect();
    both.sort_unstable();
    assert_eq!(
        both,
        ['"', '*', '/', ':', '?', '|'],
        "the overlap between what Denote removes and what Windows forbids moved. Every \
         character in it is one the corpus assertion over `REMOVED` cannot decide, \
         because the other list strips it from every component anyway"
    );

    // And every character `REMOVED` names is gone from the components it runs
    // on. All thirty rather than the six, because this is where the guarantee
    // is: the overlap above records which of them the *corpus* cannot decide,
    // and this records that each is decided somewhere.
    //
    // The extension is deliberately not part of this: `REMOVED` is Denote's
    // punctuation rule for a title, a signature and a keyword, and an extension
    // legitimately keeps `-`, `_` and `=`. `[` is on the list and is a perfectly
    // ordinary character in a file extension — what must not reach one is what
    // the *filesystem* refuses, which is the loop below.
    for character in nc_ids::naming::REMOVED {
        let title = format!("a{character}b");
        let keyword = format!("c{character}d");
        let built = FileName::new(&[&title], Some(&title), &[&keyword], id(), Some("txt"))
            .build()
            .expect("a name");
        // The half before `@@`, because `@` is itself on the list and `@@` is
        // the marker the identifier lives behind — reading the whole name would
        // make this assertion false for one of the thirty by construction, and
        // a test that is false by construction for one row is a test somebody
        // deletes.
        let components = built.split("@@").next().expect("a name has a left half");
        assert!(
            !components.contains(character),
            "`{character}` (U+{:04X}) reached a title, a signature or a keyword: {built}",
            character as u32
        );
    }
    // What the filesystem refuses is refused in every component, the extension
    // included — that is the one a built path traverses out of when something
    // reaches it.
    for character in WINDOWS_FORBIDS {
        let title = format!("a{character}b");
        let extension = format!("j{character}pg");
        let built = FileName::new(&[], Some(&title), &[], id(), Some(&extension))
            .build()
            .expect("a name");
        assert!(
            !built.contains(character),
            "`{character}` (U+{:04X}) is forbidden by Windows and reached a built name: \
             {built}",
            character as u32
        );
    }
}
