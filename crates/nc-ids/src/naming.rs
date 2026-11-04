//! The file-naming scheme, built and parsed by one module.
//!
//! ```text
//! ==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg
//!   └ signature   └ title              └ keywords └ id      └ ext
//! ```
//!
//! `NOCTUA.md §16.3` gives that example and says: **reproduce this scheme
//! exactly, do not "improve" it.** So the delimiters, their order and their
//! meanings are transcribed rather than designed. What is designed is everything
//! the scheme implies and does not state.
//!
//! **Why the builder and the parser are in one file.** §16.3 asks for them to be
//! written together and property-tested, and calls that the single
//! highest-value test in the module — because a name that parses differently
//! from how it was built does not fail anywhere. It silently splits one document
//! into two, and a knowledge graph fragments a file at a time with nothing to
//! notice.
//!
//! **Normalisation happens at construction, and at parse.** [`FileName::new`]
//! slugifies its inputs and [`FileName::parse`] slugifies what it reads, so a
//! value of this type is always already in the form a name encodes — which is
//! what makes `parse(build(x)) == x` an identity rather than an
//! approximation. A parse that wrote its fields raw would let a parsed name
//! hold spaces, punctuation and uppercase that `new` exists to strip,
//! and `build` would emit them.
//!
//! **Truncation happens at construction too**, for the same reason. Truncating
//! in `build` would make a long title produce a name that does not round-trip
//! to the value that produced it — and a property test whose every title is
//! short enough to fit would never find it.
//!
//! **It is a proposal.** The user may rename the file. Identity depends on the
//! ID (`§13.1`), so a renamed file is still the same file, and this module's job
//! is to offer a good name rather than to own one.

use nc_catalog::ErrorCode;

use crate::snowflake::Snowflake;

/// The `--` that reappears with the title.
const DELIMITER: usize = 2;

/// The longest name this will build, in bytes.
///
/// 255 is what every filesystem this platform runs on allows for one component.
/// The title is what gets shortened to fit, never the ID: a truncated ID names a
/// different file, or no file.
pub const MAX_BYTES: usize = 255;

/// One proposed file name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    signature: Vec<String>,
    title: Option<String>,
    keywords: Vec<String>,
    id: Snowflake,
    extension: Option<String>,
}

/// Why a name could not be parsed or built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    /// There is no `@@` section, so the name carries no identifier.
    NoIdentifier,
    /// The identifier is not Crockford base32.
    BadIdentifier(crate::base32::Base32Error),
    /// Even with an empty title the name is longer than [`MAX_BYTES`].
    TooLong {
        /// What it would have been.
        bytes: usize,
    },
}

impl NameError {
    /// The catalog code this maps to.
    #[must_use]
    pub const fn code(self) -> ErrorCode {
        match self {
            Self::NoIdentifier | Self::BadIdentifier(_) => ErrorCode::IdsNameUnparsable,
            Self::TooLong { .. } => ErrorCode::IdsNameTooLong,
        }
    }
}

impl core::fmt::Display for NameError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoIdentifier => formatter.write_str("the name has no `@@` identifier section"),
            // **A sentence does not fit in a noun slot.** Read as "the
            // identifier {why}", where `{why}` is a standalone sentence, three
            // of the four renderings would be ungrammatical: *"the identifier
            // 16 characters is more than the 13 a 64-bit value takes"*. The
            // identifier is the subject; what was wrong with it follows.
            Self::BadIdentifier(why) => write!(formatter, "the identifier is unreadable: {why}"),
            Self::TooLong { bytes } => write!(
                formatter,
                "the name would be {bytes} bytes, and a filesystem component holds {MAX_BYTES}"
            ),
        }
    }
}

impl core::error::Error for NameError {
    /// **The cause, which a variant that wraps one returns.** A handler
    /// walking `source()` would otherwise see nothing behind `BadIdentifier`,
    /// and the base32 error it holds is the whole explanation.
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        match self {
            Self::BadIdentifier(why) => Some(why),
            Self::NoIdentifier | Self::TooLong { .. } => None,
        }
    }
}

impl FileName {
    /// A name, with every part normalised into the form the scheme encodes.
    #[must_use]
    pub fn new(
        signature: &[&str],
        title: Option<&str>,
        keywords: &[&str],
        id: Snowflake,
        extension: Option<&str>,
    ) -> Self {
        let mut name = Self {
            // Split on `=` after slugging, because a signature part holding a
            // space becomes two parts in the name — so the value has to hold
            // two as well, or it does not equal what its own name encodes.
            signature: signature
                .iter()
                .filter_map(|part| slug_signature(part))
                // **The filter cannot fire here, and it stays for the reason
                // the three other unreachable clauses in this file stay:
                // `slug_signature` leaves no leading, trailing or doubled `=`,
                // so `split('=')` yields no empty piece.** `parse`'s copy of
                // this line *can* fire — a name somebody typed may hold `==a==b`
                // — and the two reading the same way is what makes a parsed name
                // normalise exactly like a built one. Saying so is better than
                // a reader finding a clause with no test and not knowing whether
                // it is dead or merely uncovered.
                .flat_map(|part| {
                    part.split('=')
                        .filter(|piece| !piece.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .collect(),
            title: title.and_then(slug_title),
            keywords: keywords
                .iter()
                .filter_map(|word| slug_keyword(word))
                .collect(),
            id,
            extension: extension.and_then(slug_extension),
        };
        name.shorten();
        name
    }

    /// Shortens the title until the name fits, at construction.
    ///
    /// The title and never the identifier: a truncated identifier names a
    /// different file, or no file (`§16.3`). It happens here rather than in
    /// [`FileName::build`] because a value that does not equal what its own
    /// name encodes breaks the law §16.3 calls the highest-value test in the
    /// module — and a property test whose every title is short enough to fit
    /// would never find it.
    ///
    /// A signature or a keyword set that does not fit on its own is left alone
    /// and reported by `build`: shortening the title would not have helped, and
    /// silently dropping somebody's keywords is worse than saying so.
    fn shorten(&mut self) {
        if self.render(self.title.as_deref()).len() <= MAX_BYTES {
            return;
        }
        let without = self.render(None);
        let Some(room) = MAX_BYTES.checked_sub(without.len() + DELIMITER) else {
            // **No room for a title *and* its delimiter, but the rest may still
            // fit.** Returning here left the title in place and let `build`
            // refuse a name that dropping the title would have made legal —
            // for a `render(None)` of exactly `MAX_BYTES - 1` or `MAX_BYTES`,
            // which is a two-byte window somebody would hit once and never
            // reproduce. The doc says a signature or a keyword set that does not
            // fit *on its own* is left alone; this is the case where it does.
            if without.len() <= MAX_BYTES {
                self.title = None;
            }
            return;
        };
        // Unreachable if it were written as a guard, so it is written as what it
        // is: reaching here means `render(Some(title))` is longer than
        // `render(None)`, and with no title those are the same string — so the
        // first line of this function would already have returned. A branch no
        // input can reach is a branch no test can cover, and one nobody can tell
        // is dead from a branch that merely has no test yet.
        let title = self
            .title
            .as_deref()
            .expect("a name with no title is never longer than itself");
        // Byte-wise, at a character boundary: a truncation that split a
        // multi-byte character would produce a name the filesystem refuses.
        //
        // **`min` never chooses**, and it is written anyway: this line is only
        // reached when `render(Some(title))` is longer than `render(None)` by
        // more than the budget, so `room` is already below `title.len()`. It
        // reads as the bound rather than as an arithmetic that happens to hold,
        // and a slice that is not bounded by the string it slices is a panic
        // rather than a wrong answer.
        let mut end = room.min(title.len());
        while end > 0 && !title.is_char_boundary(end) {
            end -= 1;
        }
        let shortened = title[..end].trim_end_matches('-').to_owned();
        self.title = if shortened.is_empty() {
            None
        } else {
            Some(shortened)
        };
    }

    /// The signature parts.
    #[must_use]
    pub fn signature(&self) -> &[String] {
        &self.signature
    }

    /// The title, if there is one.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    /// The keywords.
    #[must_use]
    pub fn keywords(&self) -> &[String] {
        &self.keywords
    }

    /// The identifier, which is what identity depends on.
    #[must_use]
    pub const fn id(&self) -> Snowflake {
        self.id
    }

    /// The extension, if there is one.
    #[must_use]
    pub fn extension(&self) -> Option<&str> {
        self.extension.as_deref()
    }

    /// Renders the name.
    ///
    /// Never shortens anything: [`FileName::new`] already did, so what this
    /// writes is exactly what the value holds. That is what makes §16.3's law —
    /// parsing a name recovers the fields that produced it — true for every
    /// input rather than only for short ones.
    ///
    /// # Errors
    ///
    /// Returns [`NameError::TooLong`] when the name does not fit even with no
    /// title at all, which means the signature or the keywords are the problem.
    pub fn build(&self) -> Result<String, NameError> {
        let full = self.render(self.title.as_deref());
        if full.len() <= MAX_BYTES {
            return Ok(full);
        }
        Err(NameError::TooLong { bytes: full.len() })
    }

    fn render(&self, title: Option<&str>) -> String {
        let mut out = String::new();
        if !self.signature.is_empty() {
            out.push_str("==");
            out.push_str(&self.signature.join("="));
        }
        if let Some(title) = title {
            out.push_str("--");
            out.push_str(title);
        }
        if !self.keywords.is_empty() {
            out.push_str("__");
            out.push_str(&self.keywords.join("_"));
        }
        out.push_str("@@");
        out.push_str(&self.id.to_crockford());
        if let Some(extension) = &self.extension {
            out.push('.');
            out.push_str(extension);
        }
        out
    }

    /// The canonical spelling of a name, when the one given is not it.
    ///
    /// `None` means the name is already what this would write. Some means two
    /// files could carry different names claiming one identity — a lowercase
    /// identifier, a padded one, an `I` typed for a `1` — and this is the name
    /// to propose instead.
    ///
    /// # Errors
    ///
    /// Whatever [`FileName::parse`] returns, **and [`NameError::TooLong`]**,
    /// which `parse` does not: this builds the parsed name back, and a name that
    /// parses can still be one this scheme will not write. Measured, a 316-byte
    /// name where `parse` answers `Ok` and this answers
    /// `TooLong { bytes: 316 }`. A caller matching on the sentence above
    /// handles a variant it was told could not arrive.
    pub fn canonical_name(name: &str) -> Result<Option<String>, NameError> {
        let canonical = Self::parse(name)?.build()?;
        Ok(if canonical == name {
            None
        } else {
            Some(canonical)
        })
    }

    /// Reads a name back.
    ///
    /// Every field is normalised on the way in, so a value that came from a name
    /// somebody typed by hand is in the same form as one built here — and
    /// `build` cannot emit the spaces, punctuation or uppercase that `new`
    /// exists to strip.
    ///
    /// # Errors
    ///
    /// Returns [`NameError::NoIdentifier`] when there is no `@@` section, and
    /// [`NameError::BadIdentifier`] when what follows it is not base32.
    pub fn parse(name: &str) -> Result<Self, NameError> {
        // **Denote's own rule, and not a reading of it.** Every one of the four
        // component regexps in `denote.el` has the same shape —
        // `MARKER\(\[^.\]+?\)\(==.*\|--.*\|__.*\|@@.*\|\..*\)*$` — so a
        // component is the **leftmost** marker followed by the shortest run of
        // characters after which the rest of the name begins with another marker
        // or a dot.
        //
        // Scanning from the right, on the reasoning that finding the mandatory
        // identifier first stops a title from shifting the sections, is sound
        // reasoning and not Denote's behaviour: given `__x__y@@…` Denote reads
        // the keyword `x` and such a scan reads `y`, silently dropping the half
        // a person had typed. Only a name somebody renamed by hand can contain
        // a second marker — `build` emits one of each — which is exactly the
        // input `parse` exists for.
        //
        // The title is the exception in the other direction and `terminators`
        // explains it: `--a--b@@…` is the title `a--b` to Denote, because its
        // regexp alone does not end at `--`.
        let (id_text, after_id) = field(name, "@@").ok_or(NameError::NoIdentifier)?;
        let id = Snowflake::from_crockford(id_text).map_err(NameError::BadIdentifier)?;
        let extension = after_id.strip_prefix('.').map(ToOwned::to_owned);

        let signature: Vec<String> = field(name, "==").map_or_else(Vec::new, |(text, _)| {
            text.split('=')
                .filter(|part| !part.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        });
        let title = field(name, "--").map(|(text, _)| text.to_owned());
        let keywords: Vec<String> = field(name, "__").map_or_else(Vec::new, |(text, _)| {
            text.split('_')
                .filter(|word| !word.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        });

        // Through the constructor, so a parsed name is normalised exactly like
        // a built one. Written raw, a value could hold spaces, punctuation and
        // uppercase that `new` exists to strip — and `build` would emit them
        // again.
        let signature: Vec<&str> = signature.iter().map(String::as_str).collect();
        let keywords: Vec<&str> = keywords.iter().map(String::as_str).collect();
        Ok(Self::new(
            &signature,
            title.as_deref(),
            &keywords,
            id,
            extension.as_deref(),
        ))
    }
}

/// What ends a component, per marker.
///
/// Three of `denote.el`'s four regexps end a component at any of the four
/// markers. **The title's does not end at `--`** — its alternation is
/// `\(==.*\|__.*\|@@.*\|\..*\)*$`, with no `--.*` — so a title may contain
/// a doubled hyphen and Denote reads `--a--b@@…` as the title `a--b`. Measured
/// against Emacs 30.2 with the real `denote-retrieve-filename-title`, not
/// inferred: the omission looks like an oversight and is the behaviour, so this
/// reproduces it rather than tidying it.
const fn terminators(marker: &str) -> &'static [&'static str] {
    // `match` on a `&str` is not allowed in a const fn, and the marker is one of
    // four literals this module passes itself.
    if marker.as_bytes()[0] == b'-' {
        &["==", "__", "@@"]
    } else {
        &["==", "--", "__", "@@"]
    }
}

/// One component of a Denote name, and what follows it.
///
/// The Rust spelling of `denote.el`'s component regexps. `marker` is found at
/// its **leftmost** occurrence; the content is the shortest non-empty run after
/// it such that the remainder is empty or begins with one of that component's
/// terminators or a `.`. A component whose content would be empty is absent,
/// which is what the `+?` in the regexp says.
fn field<'a>(name: &'a str, marker: &str) -> Option<(&'a str, &'a str)> {
    let ends = terminators(marker);
    // Every occurrence, not only the first. The regexp is anchored at `$`, so
    // `string-match` moves the marker rightwards until the whole pattern
    // matches — and the content is `[^.]+?`, which cannot match a dot at all. A
    // marker followed immediately by one therefore yields nothing at that
    // position. Measured against Emacs 30.2: `denote-retrieve-filename-title`
    // on `--.x@@01J8XK2M9P51` is nil, where a first-occurrence reading answers
    // `x`.
    let mut from = 0;
    while let Some(at) = name[from..].find(marker) {
        // One value, not two: `start` and the next `from` would always be
        // equal. And the slice is infallible — every marker is ASCII and `find`
        // returns a byte offset that is a character boundary — so a `?` here
        // would be a silent "component absent" path no input could reach.
        from += at + marker.len();
        let rest = &name[from..];
        if rest.starts_with('.') || rest.is_empty() {
            continue;
        }
        for (offset, character) in rest.char_indices().skip(1) {
            let remainder = &rest[offset..];
            if character == '.' || ends.iter().any(|end| remainder.starts_with(end)) {
                return Some((&rest[..offset], remainder));
            }
        }
        // Nothing ended it, so the component runs to the end of the name — the
        // `*` in the regexp allowing zero suffixes.
        return Some((rest, ""));
    }
    None
}

/// The punctuation Denote removes from every component.
///
/// Transcribed from `denote-sluggify-title`'s own regexp in `denote.el`, and
/// **counted against it**: Denote's set is thirty characters, and twenty-six
/// would miss the curly quotes — the typographic apostrophe is the most common
/// punctuation mark there is in a real title, and without it `don't stop` comes
/// out as `don-t-stop` instead of `dont-stop`, and as a signature
/// as `don=t`. Public so the corpus test beside this crate can assert that
/// every one of them appears in some row: a character on this list that no row
/// exercises is a rule nothing checks. With `"` and `‘` in no row, deleting
/// both from this list leaves the whole workspace green and produces
/// names Denote would never write.
///
/// **Six of these are also in `FORBIDDEN`, and for those the corpus assertion
/// decides nothing.** `:`, `"`, `/`, `|`, `?` and `*` are stripped from a title
/// by this list and from *every* component by that one, so deleting one of the
/// six here changes no output at all. That is not a hole: the character is
/// still refused, by the rule written for the component the extension is. It
/// does mean the sentence the corpus assertion carries — "so deleting it from
/// the rule would go unnoticed" — is true of the twenty-four this list alone
/// decides, and `the_two_lists_overlap_where_they_say_they_do` is what holds
/// the six.
pub const REMOVED: [char; 30] = [
    '[', ']', '{', '}', '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '+', '\'', '"', '?', ',',
    '.', '|', ';', ':', '~', '`', '/', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
];

/// Every character Windows forbids in a path component.
///
/// `NOCTUA.md §16.3` requires respecting "the characters Windows forbids", and
/// this is that set, complete: `< > : " / \\ | ? *`. Denote's own [`REMOVED`]
/// covers six of them incidentally, and holding only the three it does not
/// would be a rule written for one component and applied to another.
///
/// **The extension is the component that does not run `REMOVED`**, because a
/// real extension may contain `-`, `_` and `=`. So a character only
/// in `REMOVED` reaches a built name through it, which is why this set is
/// complete: with `/` only there, the "name" would be a path:
///
/// ```text
/// extension "j/pg"             →  --t@@01J8XK2M9P51.j/pg
/// extension "../../etc/passwd" →  --t@@01J8XK2M9P51./../etc/passwd
/// ```
///
/// The second is a traversal out of whatever directory a caller meant to write
/// in, produced by a function whose whole job is to return one path
/// component. `parse(build(x)) == x` still holds, which is why criterion 8
/// cannot see it.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Whether a character cannot appear in a file name at all.
///
/// **The C0 range, `U+0000` to `U+001F`, and nothing else.** Windows rejects
/// every one of them in a path, and a tab or a newline in a name breaks every
/// line-oriented tool that will ever list it. Denote does not remove them —
/// `denote-slug-hyphenate` matches only a literal space, so a tab survives into
/// the name it produces — which is the second and last place this module adds to
/// Denote, and `§16.3` records both.
///
/// Not `char::is_control()`, which is Unicode's `Cc` category and therefore
/// **also C1** (`U+0080`–`U+009F`). Windows permits those, Denote keeps them,
/// and removing them would be a third addition that no document records — a
/// title `a\u{85}b` quietly becoming `ab`. The addition has to be exactly as
/// wide as its justification.
const fn unusable(character: char) -> bool {
    (character as u32) <= 0x1f
}

/// A title: Denote's `denote-sluggify-title`.
///
/// Removes the punctuation above and `=`, lowercases, turns spaces and
/// underscores into `-`, collapses runs and trims the ends. Accents are kept,
/// which is what Denote does.
fn slug_title(text: &str) -> Option<String> {
    slug(text, &['='], '-')
}

/// A signature part: Denote's `denote-sluggify-signature`.
///
/// The same, except that `-` is *removed* and the joiner is `=` — which is why
/// `§16.3`'s `==familia=fotos` is one signature of two parts rather than two of
/// anything.
///
/// **An open question, and it is written here because it cannot be settled
/// here.** An `=` *inside* a signature part is kept and collapsed by this, and
/// Denote's own punctuation rule removes `=` from a title outright. Whether
/// `denote-sluggify-signature` removes it too is a question only Emacs can
/// answer, and `denote.el` is not installed on this machine — so this is not a
/// third deviation from Denote until somebody measures it, and it is not
/// fidelity either. `cargo test --test against_denote` passes with the `=`
/// removed as well as kept, which is what says the corpus has no row that
/// decides it: no row holds an interior `=` in a signature.
///
/// What that costs if the answer is *removed*: `a=b` as a signature part
/// becomes `a=b` here and `ab` there, so one name in each scheme. The corpus
/// row named *"a doubled equals collapses"* reads the **title** column, where
/// `=` is removed rather than collapsed, so its name describes neither what it
/// exercises nor this.
fn slug_signature(text: &str) -> Option<String> {
    slug(text, &['-'], '=')
}

/// An extension.
///
/// **Denote does not sluggify one at all**, and neither does this beyond what a
/// filesystem forces: lowercased, with whitespace and the characters Windows
/// forbids removed. Running it through the keyword rule turns `tar.gz` into
/// `targz`, which is the single most common compound extension there is — and
/// the dot is safe, because the parser takes everything after the *first* dot
/// following the identifier.
fn slug_extension(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        // **`unusable` too**, not only the whitespace controls — and those only
        // because `char::is_whitespace` happens to cover them. Without it,
        // `FileName::new(…, Some("a\u{0}b")).build()` returns a name with a raw
        // NUL in it, which no filesystem accepts as a byte of a path. Every
        // other component filters it; the extension is where a rule written
        // once has to be applied twice.
        .filter(|c| !c.is_whitespace() && !unusable(*c) && !FORBIDDEN.contains(c) && *c != '@')
        .flat_map(char::to_lowercase)
        .collect();
    let trimmed = collapse(cleaned.trim_matches('.'));
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Runs of a delimiter become one, which is `denote--replace-consecutive-token-characters`.
///
/// **This is what makes `parse(build(x)) == x` true, and the extension is where
/// it is needed.** Every other component's sluggifier removes its delimiters
/// outright; the extension keeps `=`, `-` and `_` because a real one may
/// contain them. Keeping them is right and not collapsing them is not: `new`
/// with the extension `==a` would build `@@01J8XK2M9P51.==a`, and `parse` would
/// read the `==` as the start of a signature and return a name with a signature
/// the original never had. `= =a` reaches the same place, because removing the
/// space creates the pair.
///
/// Denote collapses `_`, `=`, `@` and `-` this way; `-` is exempt only in a
/// title, and an extension is not a title.
///
/// **`@` is in the set and cannot arrive here**, because the extension's own
/// filter removes it before this runs — `@@` is the identifier's delimiter and a
/// component holding one would split the name. It stays in the list so the rule
/// matches `denote--replace-consecutive-token-characters` rather than matching
/// what this one caller happens to need, and so that a second caller does not
/// have to rediscover it.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous = None;
    for character in text.chars() {
        let doubled = matches!(character, '-' | '_' | '=' | '@') && previous == Some(character);
        if !doubled {
            out.push(character);
        }
        previous = Some(character);
    }
    out
}

/// A keyword: Denote's `denote-sluggify-keyword`.
///
/// **The words are joined, not separated.** Denote removes `-`, `_`, `=` and the
/// spaces themselves, so "rio de janeiro" becomes the single keyword
/// `riodejaneiro`. That is the rule the user named, and it is also what keeps
/// `parse(build(x)) == x` true: `_` separates keywords, so a keyword that
/// contained one would come back as several.
fn slug_keyword(text: &str) -> Option<String> {
    // `_` and the space are dropped by `slug` itself — they are the word
    // boundary — so passing them again would change nothing and read as though
    // the keyword rule needed something extra. `-` and `=` genuinely do: Denote's
    // keyword set removes both, and they are two of the scheme's delimiters.
    slug(text, &['-', '='], '\u{0}')
}

/// Denote's sluggification, parameterised by what each component removes and
/// what it joins with.
///
/// **Accents are kept**, because Denote keeps them. `denote-slug-remove-accents`
/// exists in `denote.el` and is called from nowhere in it: the default
/// `denote-file-name-slug-functions` maps every component to
/// `denote-sluggify-{title,signature,keyword}`, none of which folds. Measured
/// against Emacs 30.2, `Café naïve` becomes `café-naïve` and `Zażółć gęślą jaźń`
/// becomes `zażółć-gęślą-jaźń`.
///
/// This module does not fold. A hand-written decomposition table misaligned by
/// two entries from `U+014C` on turns `Łódź żółw` into `łody-zołw` — a
/// different word, in a file name that claims to be the title somebody
/// typed. Not folding is not only fidelity to Denote; it removes the
/// possibility of that class of defect. Denote offers folding to those who want
/// it, as a slug function they install themselves, and this does not install
/// it.
fn slug(text: &str, also_removed: &[char], joiner: char) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut pending = false;
    for character in text.chars() {
        if unusable(character)
            || REMOVED.contains(&character)
            || FORBIDDEN.contains(&character)
            || also_removed.contains(&character)
        {
            continue;
        }
        let lowered: String = character.to_lowercase().collect();
        // A joiner already in the content is a word boundary, not content: Denote
        // collapses runs of it, and that collapse is what stops a title holding
        // `--` from splitting the name at the wrong place when it is read back.
        // Not `&& !lowered.is_empty()`: `char::to_lowercase` yields at least one
        // character for every input, so the guard can never be the thing that
        // decides — and it reads as though an empty lowering were possible.
        let lowered = if lowered.chars().all(|c| c == joiner) {
            String::new()
        } else {
            lowered
        };
        // **Whatever survived removal is kept**, which is Denote's rule and not
        // an alphanumeric filter. Such a filter drops a currency symbol, an em
        // dash, a degree sign and CJK punctuation — and leaves a word boundary
        // behind each — so `5 € + 3 £` would become `5-3` where Denote writes
        // `5-€-3-£`.
        //
        // **A word boundary is a literal space or an underscore, and nothing
        // else.** `denote-slug-hyphenate`'s regexp is `_\| +` — one or more
        // U+0020, or an underscore — so a tab and a non-breaking space are
        // ordinary content to Denote and stay in the name. A tab cannot stay in
        // ours, but that is `unusable`'s job above: it is removed as an illegal
        // character, not silently reinterpreted as a space, because those two
        // produce different names and only one of them is Denote's.
        let kept: String = lowered.chars().filter(|c| *c != ' ' && *c != '_').collect();
        if kept.is_empty() {
            // A character that survived removal and is not alphanumeric — a
            // space, a dash where the component keeps one — is a word boundary.
            // With no joiner (a keyword) it simply disappears, which is how
            // Denote joins the words.
            pending = joiner != '\u{0}' && !out.is_empty();
        } else {
            if pending {
                out.push(joiner);
                pending = false;
            }
            out.push_str(&kept);
        }
    }
    // **No trim, because there is nothing to trim.** An
    // `out.trim_matches(joiner)` at the end would be unreachable by
    // construction: `pending` is set only when `out` is non-empty and is
    // flushed only immediately before a non-empty `kept`, so a joiner can never
    // be the first or the last character. Deleting such a trim leaves the whole
    // gate green — which is `§19.5`'s own instrument turned on this file, and
    // the answer is to remove the code rather than to write a test for a branch
    // nothing reaches. Leading and trailing separators are still trimmed; the
    // `pending` mechanism above is what does it, and
    // `the_corpus_exercises_every_rule_it_is_the_authority_for` proves a row
    // exercises it.
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod code_tests {
    use super::NameError;
    use crate::base32::Base32Error;
    use nc_catalog::ErrorCode;

    #[test]
    fn a_name_worth_shortening_is_told_apart_from_one_that_is_not_a_name() {
        // `code` has no other caller, so this is what checks the distinction it
        // draws. It is the distinction that matters to whoever handles the
        // failure: a name that is too long is theirs to shorten, and a name
        // with no identifier is not a Denote name at all and never will be. One
        // code for both would have a caller offering to truncate a file it
        // should refuse.
        assert_eq!(
            NameError::TooLong { bytes: 300 }.code(),
            ErrorCode::IdsNameTooLong
        );
        for error in [
            NameError::NoIdentifier,
            NameError::BadIdentifier(Base32Error::Empty),
        ] {
            let shown = error.to_string();
            let code = error.code();
            assert_eq!(code, ErrorCode::IdsNameUnparsable, "{shown}");
            assert_ne!(code, NameError::TooLong { bytes: 300 }.code(), "{shown}");
        }
    }
}

#[cfg(test)]
mod tests {
    /// A bad identifier reads as a sentence, and its cause is reachable.
    ///
    /// **Each of the four renderings is grammatical** — a message splicing a
    /// standalone sentence into a noun slot reads *"the identifier 16
    /// characters is more than the 13 a 64-bit value takes"*. And `source()`
    /// returns the base32 error the variant holds, so a handler walking the
    /// chain can see it.
    #[test]
    fn a_bad_identifier_reads_as_a_sentence_and_names_its_cause() {
        use core::error::Error as _;

        let refused = FileName::parse("--title@@ZZZZZZZZZZZZZZZZ.md")
            .expect_err("sixteen characters is not an identifier");
        let said = refused.to_string();
        assert!(
            said.starts_with("the identifier is unreadable: "),
            "the sentence still splices a sentence into a noun slot: {said}"
        );
        assert!(
            refused.source().is_some(),
            "the base32 error behind it is not reachable: {refused:?}"
        );
    }

    use super::{FileName, MAX_BYTES, NameError, slug_keyword, slug_signature, slug_title};
    use crate::snowflake::Snowflake;

    /// The declared byte limit is the number its own paragraph argues for.
    ///
    /// **A bound pinned against its argument, not only against itself.** Every
    /// other test and both property tests read `MAX_BYTES`
    /// (`built.len() <= MAX_BYTES`), so the bound is compared with itself:
    /// raising it is caught by a 300-character fixture and *lowering* it is
    /// not. Measured at 100 without this, `cargo xtask check` passes — and
    /// every title would be silently shortened to a fraction of what the
    /// filesystem allows, with the paragraph's claim about filesystems
    /// unchecked in the one direction a typo goes.
    #[test]
    fn the_byte_limit_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            MAX_BYTES, 255,
            "255 is what every filesystem this platform runs on allows for one \
             component, and a smaller number silently shortens every title"
        );
    }

    /// `build` refuses at the byte after the last one that fits, and not later.
    ///
    /// **`MAX_BYTES` is pinned, and so is the line that reads it.** The property
    /// suite asserts `<=`, and this crate's other length equalities are on names
    /// that fit or are far past the bound, so widening `build`'s own comparison
    /// is invisible to them — measured without this:
    ///
    /// ```text
    /// if full.len() <= MAX_BYTES + 1 {   -> cargo test -p nc-ids green,
    /// if full.len() <= MAX_BYTES + 5 {      cargo xtask check green
    ///   render(None) of 256 bytes: Err(TooLong) -> Ok(256)
    /// ```
    ///
    /// and `FileName::build()` then hands back a name no filesystem accepts as
    /// one component. The property tests *would* catch it — they assert
    /// `built.len() <= MAX_BYTES` — but no generator produces a `render(None)`
    /// of exactly 256, so the assertion never meets the window. A bound compared
    /// only against itself is the shape this repository names.
    ///
    /// Both sides, because narrowing it is the other way to be wrong: at exactly
    /// `MAX_BYTES` the name is legal and must be built.
    #[test]
    fn the_refusal_is_at_the_byte_after_the_last_one_that_fits() {
        // A signature alone, so `shorten` has no title to work with and `build`
        // decides on its own. The identifier and the `==` are the fixed part.
        let fixed = FileName::new(&["a"], None, &[], id(), None)
            .build()
            .expect("one letter fits")
            .len()
            - 1;
        for (length, fits) in [(MAX_BYTES - fixed, true), (MAX_BYTES - fixed + 1, false)] {
            let signature = "a".repeat(length);
            let built = FileName::new(&[signature.as_str()], None, &[], id(), None).build();
            assert_eq!(
                built.is_ok(),
                fits,
                "a name of {} byte(s) is {}: {built:?}",
                fixed + length,
                if fits { "legal" } else { "one byte too long" }
            );
            if let Ok(name) = built {
                assert_eq!(name.len(), MAX_BYTES, "the legal one is exactly the limit");
            }
        }
    }

    /// `shorten` reaches for the title at the byte after the last one that fits.
    ///
    /// The mirror of the test above, on the other line that reads `MAX_BYTES`:
    /// widening `shorten`'s early return leaves a title in place that would have
    /// been cut to fit, and `build` then refuses a name that was legal.
    #[test]
    fn a_title_is_shortened_at_the_byte_after_the_last_one_that_fits() {
        let fixed = FileName::new(&["a"], None, &[], id(), None)
            .build()
            .expect("one letter fits")
            .len()
            - 1;
        // A signature that fills the name exactly, and a title beside it. There
        // is no room for `--` and a character, so the title goes and the name is
        // built rather than refused — which is what `shorten`'s own comment says
        // and what widening its early return breaks.
        let signature = "a".repeat(MAX_BYTES - fixed);
        let name = FileName::new(&[signature.as_str()], Some("bb"), &[], id(), None)
            .build()
            .expect("a title that cannot fit is dropped rather than refused");
        assert_eq!(
            name.len(),
            MAX_BYTES,
            "the name without its title is exactly the limit"
        );
        assert!(
            !name.contains("--"),
            "the title had no room and should have gone: {name}"
        );
    }

    fn id() -> Snowflake {
        Snowflake::from_crockford("01J8XK2M9P51").expect("the specification's own example")
    }

    /// A name that must be shortened uses every byte it is entitled to.
    ///
    /// **`DELIMITER` pinned in both directions.** The property suite asserts
    /// `<=`, and this crate's other length equalities are on names that keep no
    /// shortened title, so making the truncation budget *smaller* is invisible
    /// to them: `DELIMITER = 50` builds a 207-byte name where 255 is allowed, and
    /// `parse(build(x)) == x` still holds because `parse` reads back whatever
    /// `build` shortened to. Every long title would silently lose 48 bytes it is
    /// entitled to.
    ///
    /// Equality, because a name that had to be shortened is a name that used
    /// the whole budget — anything less is the budget being wrong.
    #[test]
    fn a_shortened_name_uses_the_whole_budget() {
        let long = "a".repeat(400);
        let built = FileName::new(&["family"], Some(&long), &["holiday"], id(), Some("jpeg"))
            .build()
            .expect("a name that is shortened is still a name");
        assert_eq!(
            built.len(),
            MAX_BYTES,
            "a title long enough to be cut left {} of {MAX_BYTES} bytes unused, so the \
             truncation budget is smaller than the limit it is derived from",
            MAX_BYTES - built.len()
        );
    }

    #[test]
    fn the_worked_example_from_the_specification_parses_into_its_parts() {
        // The one name NOCTUA.md §16.3 writes down. If this stops holding, the
        // scheme has been "improved".
        let parsed =
            FileName::parse("==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg")
                .expect("the specification's example");
        assert_eq!(parsed.signature(), ["familia", "fotos"]);
        assert_eq!(parsed.title(), Some("praia-de-copacabana"));
        assert_eq!(parsed.keywords(), ["brasil"]);
        assert_eq!(parsed.extension(), Some("jpg"));
        assert_eq!(parsed.id(), id());
    }

    /// The specification's own example, rebuilt byte for byte.
    ///
    /// **The round trip is invariant under any layout `field()` can
    /// decode.** `FileName::parse` re-derives every component
    /// through `FileName::new`, so criterion 8's property
    /// — `parse(build(x)) == x` — holds whatever order `render` writes the four
    /// components in and whatever character joins the signature's parts, and
    /// the one worked example `NOCTUA.md §16.3` writes down is otherwise only
    /// *parsed*. Measured without this, two clauses planted together
    /// — `render`'s signature block moved after its title block,
    /// and `join("=")` written `join("_")`:
    ///
    /// ```text
    /// check passed, ok 1248 test(s), and the whole report byte-identical
    /// --praia-de-copacabana==familia=fotos__brasil@@01J8XK2M9P51.jpg
    /// ==familia_fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg
    /// ```
    ///
    /// Neither is a name Denote writes, and `_` is Denote's *keyword*
    /// separator. `canonical_name`'s whole job is to propose the one spelling
    /// of an identity, and under either plant it proposes renaming every
    /// correctly named file on disk. This file already reasons *from* the
    /// property that falsifies
    /// — `a_title_ends_at_a_signature_a_person_typed_after_it` argues
    /// that `build` emits the signature before the title, which this holds.
    ///
    /// Both directions, because the parse test beside this one is not enough: a
    /// name is built from its parts, and the same name is parsed and rebuilt.
    #[test]
    fn the_worked_example_from_the_specification_rebuilds_byte_for_byte() {
        const EXAMPLE: &str = "==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg";
        let built = FileName::new(
            &["familia", "fotos"],
            Some("praia-de-copacabana"),
            &["brasil"],
            id(),
            Some("jpg"),
        )
        .build()
        .expect("the specification's example builds");
        assert_eq!(
            built, EXAMPLE,
            "the components are written in an order the specification does not"
        );
        let again = FileName::parse(EXAMPLE)
            .expect("the specification's example")
            .build()
            .expect("it builds again");
        assert_eq!(
            again, EXAMPLE,
            "a name read from disk and written back is not the name that was read"
        );
    }

    #[test]
    fn an_absent_field_takes_its_delimiter_with_it() {
        let name = FileName::new(&[], None, &[], id(), Some("txt"))
            .build()
            .expect("builds");
        assert!(!name.contains("=="), "{name}");
        assert!(!name.contains("--"), "{name}");
        assert!(!name.contains("__"), "{name}");
        assert!(name.starts_with("@@"), "{name}");
    }

    #[test]
    fn content_cannot_be_mistaken_for_structure() {
        // A title holding every delimiter. If any survived, the parser would
        // read the name as having sections the builder never wrote.
        let name = FileName::new(&["a=b"], Some("x--y__z@@w.q"), &["k_l"], id(), Some("md"))
            .build()
            .expect("builds");
        let parsed = FileName::parse(&name).expect("parses");
        // Every delimiter is dropped from the content and leaves a joiner where
        // it stood, so the words survive and the structure cannot be forged.
        // Every delimiter is either removed or collapsed into a single joiner,
        // so a title holding `--` cannot split the name at the wrong place.
        assert_eq!(parsed.title(), Some("x-y-zwq"));
        // `=` is a signature's own joiner, so `a=b` is two parts — which is what
        // §16.3's `==familia=fotos` is. `-` is removed from a signature and `_`
        // from a keyword: Denote's rules per component, not one rule for all.
        assert_eq!(parsed.signature(), ["a", "b"]);
        assert_eq!(parsed.keywords(), ["kl"]);
    }

    #[test]
    fn spaces_become_the_joiner_each_field_uses() {
        let name = FileName::new(
            &[],
            Some("praia de copacabana"),
            &["rio de janeiro"],
            id(),
            None,
        );
        assert_eq!(name.title(), Some("praia-de-copacabana"));
        assert_eq!(name.keywords(), ["riodejaneiro"]);
    }

    #[test]
    fn a_title_in_a_script_that_is_not_latin_survives() {
        // The name is a proposal shown to the person who wrote the title. An
        // ASCII fold would hand them an empty one.
        let name = FileName::new(&[], Some("写真 の アルバム"), &[], id(), None);
        assert_eq!(name.title(), Some("写真-の-アルバム"));
    }

    #[test]
    fn the_punctuation_set_is_denote_s_including_the_curly_quotes() {
        // The four curly quotes a set of twenty-six would miss, and the
        // apostrophe is the one a real title actually contains.
        let name = FileName::new(&[], Some("don\u{2019}t stop"), &[], id(), None);
        assert_eq!(name.title(), Some("dont-stop"));
        let quoted = FileName::new(&[], Some("\u{201c}hello\u{201d} there"), &[], id(), None);
        assert_eq!(quoted.title(), Some("hello-there"));
    }

    #[test]
    fn whatever_survives_removal_is_kept_which_is_denote_s_rule() {
        // An alphanumeric filter is not Denote's: it drops a currency symbol,
        // an em dash and a degree sign, and leaves a word boundary behind each.
        let name = FileName::new(&[], Some("5 \u{20ac} + 3 \u{a3}"), &[], id(), None);
        assert_eq!(name.title(), Some("5-\u{20ac}-3-\u{a3}"));
        let degrees = FileName::new(&[], Some("temp 20\u{b0}C"), &[], id(), None);
        assert_eq!(degrees.title(), Some("temp-20\u{b0}c"));
    }

    #[test]
    fn accents_are_kept_because_denote_keeps_them() {
        // Measured against Emacs 30.2, not reasoned about:
        // `denote-slug-remove-accents` is defined in `denote.el` and called from
        // nowhere in it, and the default `denote-file-name-slug-functions` maps
        // every component to a sluggifier that does not fold. Each expectation
        // below is what `denote-sluggify-and-apply-rules` returned.
        for (typed, expected) in [
            ("Café naïve", "café-naïve"),
            ("Zażółć gęślą jaźń", "zażółć-gęślą-jaźń"),
            ("Łódź żółw", "łódź-żółw"),
            ("Tiếng Việt Đường phố", "tiếng-việt-đường-phố"),
            ("Praia de Copacabana à noite", "praia-de-copacabana-à-noite"),
        ] {
            let name = FileName::new(&[], Some(typed), &[], id(), None);
            assert_eq!(name.title(), Some(expected), "{typed}");
        }
    }

    #[test]
    fn a_combining_mark_is_content_and_not_something_to_strip() {
        // The same rule seen from the other side. `a` followed by a combining
        // grave is what a macOS filesystem hands back for `à`, and Denote keeps
        // it — so a fold here would make the same title produce two different
        // names depending on which machine typed it.
        let decomposed = FileName::new(&[], Some("a\u{300}gua"), &[], id(), None);
        assert_eq!(decomposed.title(), Some("a\u{300}gua"));
    }

    /// Every combining mark in the block is content, not something to strip.
    ///
    /// **Every mark, not one of 112.** With only `U+0300` here, and all 44
    /// corpus rows NFC, adding `U+0301` to `slug`'s removal clause leaves the
    /// whole workspace green: measured, `cargo xtask check`
    /// prints `check passed`. A title pasted from macOS arrives decomposed and
    /// the same title typed on Linux arrives composed, so a fold here makes one
    /// title into two file names, and `parse(build(x)) == x` still holds
    /// because stripping is idempotent — the round-trip cannot see it.
    #[test]
    fn no_combining_mark_is_stripped_from_a_title() {
        for point in 0x0300..=0x036F_u32 {
            let mark = char::from_u32(point).expect("the block is all characters");
            let title = format!("a{mark}gua");
            let name = FileName::new(&[], Some(&title), &[], id(), None);
            assert_eq!(
                name.title(),
                Some(title.as_str()),
                "`U+{point:04X}` was removed from a title. A fold here makes the same \
                 title produce two different names depending on which machine typed it, \
                 and the round-trip property cannot see it because stripping is \
                 idempotent"
            );
        }
    }

    /// The keywords a name carries come back in the order they were given.
    ///
    /// **This observes a name with two of them.** `FileName::new` neither sorts
    /// nor dedups, and without this no assertion in the workspace reads
    /// a `keywords()` of length two or more — the corpus has one keyword per
    /// row by construction, and the round-trip property is invariant under any
    /// idempotent permutation. So sorting and not sorting are otherwise
    /// indistinguishable to the whole suite: measured, `name.keywords.sort();`
    /// inserted before `shorten()` leaves `cargo xtask check`
    /// at `check passed`.
    ///
    /// Real Denote's `denote-sort-keywords` defaults to `t`; this crate keeps
    /// the order it was given, and that is a decision a test holds.
    #[test]
    fn the_keywords_keep_the_order_they_were_given() {
        let given = ["zeta", "alpha", "mu"];
        let name = FileName::new(&[], Some("a title"), &given, id(), None);
        assert_eq!(
            name.keywords(),
            given,
            "the keywords came back in another order. Nothing else in this workspace \
             reads a name with more than one, so a sort added here would be invisible"
        );
        // And the round trip keeps it, which is what a reader of the file name
        // depends on.
        let built = name.build().expect("a valid name");
        let parsed = FileName::parse(&built).expect("what this crate built, it parses");
        assert_eq!(
            parsed.keywords(),
            given,
            "`{built}` round-tripped to another order"
        );
    }

    #[test]
    fn a_letter_with_no_decomposition_is_not_special_either() {
        // `ø` and `đ` have no canonical decomposition, so even Denote's optional
        // accent remover leaves them. A hand-written table once folded `ø` to
        // `o`, which is a different letter in the languages that use it.
        let name = FileName::new(&[], Some("Ø đ"), &[], id(), None);
        assert_eq!(name.title(), Some("ø-đ"));
    }

    #[test]
    fn an_extension_is_cleaned_of_what_a_filesystem_refuses_and_of_its_own_dots() {
        // Two guards inside `slug_extension`, which without this test could
        // each be deleted with the rest of the suite green. The backslash one
        // matters most: `DEVIATIONS` in `crates/nc-ids/tests/against_denote.rs`
        // documents removing `<`, `>` and `\` from a title, and without it the
        // extension is the one component where the same characters go through
        // untouched — so a built name could carry a path separator.
        let backslash = FileName::new(&[], Some("t"), &[], id(), Some("j\\pg"));
        assert_eq!(backslash.extension(), Some("jpg"));
        let angled = FileName::new(&[], Some("t"), &[], id(), Some("j<p>g"));
        assert_eq!(angled.extension(), Some("jpg"));

        // And the leading and trailing dots, which the parser uses as the
        // boundary. Kept, `"..md.."` re-parses as `".md.."`, which is the round
        // trip broken in the other direction.
        let dotted = FileName::new(&[], Some("t"), &[], id(), Some("..md.."));
        assert_eq!(dotted.extension(), Some("md"));
        // A dot *inside* one stays, because that is what a compound extension is.
        let compound = FileName::new(&[], Some("t"), &[], id(), Some("tar.gz"));
        assert_eq!(compound.extension(), Some("tar.gz"));

        // And the two guards the cases above leave undriven. `@@` opens the
        // identifier, so an `@` in an extension would put a second one in the
        // name; and a space is the word boundary every other component removes.
        let at = FileName::new(&[], Some("t"), &[], id(), Some("j@pg"));
        assert_eq!(at.extension(), Some("jpg"));
        let spaced = FileName::new(&[], Some("t"), &[], id(), Some("j p g"));
        assert_eq!(spaced.extension(), Some("jpg"));
        let built = FileName::new(&[], Some("t"), &[], id(), Some("j@@pg"))
            .build()
            .expect("builds");
        assert_eq!(
            built.matches("@@").count(),
            1,
            "a second `@@` would move where the identifier starts: {built}"
        );
    }

    #[test]
    fn a_title_is_dropped_when_it_and_its_delimiter_are_what_does_not_fit() {
        // A two-byte window. `render(None)` fits, `render(None) + "--" + title`
        // does not, and the subtraction that computes the room for a title
        // underflows — so `shorten` would return with the title still attached
        // and `build` refuse a name that dropping the title would have made
        // legal.
        //
        // The window is found by construction rather than guessed at: grow a
        // keyword until the title-less form is exactly `MAX_BYTES`, then ask
        // for a name with a title. **Both widths reached, or this test fails.**
        // An arm that would `continue` when the exact byte count is not hit
        // lets a change in the identifier's width or in the delimiters make the
        // loop skip silently and the branch it exists for go untested — the
        // vacuous shape §19.5 is about, arriving through the fixture.
        let mut exercised = 0_usize;
        for target in [MAX_BYTES - 1, MAX_BYTES] {
            let mut width = 1;
            let keyword = loop {
                let keyword = "k".repeat(width);
                let name = FileName::new(&[], None, &[&keyword], id(), None);
                let length = name.build().map_or(usize::MAX, |built| built.len());
                assert!(
                    width < MAX_BYTES * 2,
                    "no keyword width produces a title-less name of {target} bytes"
                );
                if length >= target {
                    break keyword;
                }
                width += 1;
            };
            let bare = FileName::new(&[], None, &[&keyword], id(), None)
                .build()
                .expect("a keyword that fits alone is a legal name");
            assert_eq!(
                bare.len(),
                target,
                "the search overshot, so the two-byte window was never entered"
            );
            let titled = FileName::new(&[], Some("x"), &[&keyword], id(), None);
            let built = titled
                .build()
                .unwrap_or_else(|error| panic!("a keyword that fits alone was refused: {error}"));
            assert!(built.len() <= MAX_BYTES, "{} bytes", built.len());
            assert_eq!(titled.title(), None, "the title should have been dropped");
            exercised += 1;
        }
        assert_eq!(exercised, 2, "the two-byte window was not entered twice");
    }

    /// A title shortened to nothing is no title rather than an empty one.
    ///
    /// The test above is the window where there is no room for the title *and*
    /// its delimiter; this is the byte after it, where there is room for the
    /// delimiter and none for the title. `end` is then zero, the shortened title
    /// is the empty string, and `Some("")` is not `None`: `render` writes the
    /// `--` for it, so the name carries a delimiter with nothing behind it —
    /// a name Denote would never write, and one that parses back with no title,
    /// so the value no longer equals what its own name encodes.
    ///
    /// Measured: `if shortened.is_empty()` written `if false` leaves the rest
    /// of `cargo test -p nc-ids` green.
    #[test]
    fn a_title_shortened_to_nothing_is_no_title_rather_than_an_empty_one() {
        // Found by construction, like the window above: grow a keyword until
        // the title-less name is exactly `MAX_BYTES - DELIMITER`.
        let target = MAX_BYTES - super::DELIMITER;
        let mut width = 1;
        let keyword = loop {
            let keyword = "k".repeat(width);
            let length = FileName::new(&[], None, &[&keyword], id(), None)
                .build()
                .map_or(usize::MAX, |built| built.len());
            assert!(
                width < MAX_BYTES * 2,
                "no keyword width produces a title-less name of {target} bytes"
            );
            if length >= target {
                break keyword;
            }
            width += 1;
        };
        let bare = FileName::new(&[], None, &[&keyword], id(), None)
            .build()
            .expect("a keyword that fits alone is a legal name");
        assert_eq!(
            bare.len(),
            target,
            "the search overshot, so the one-byte window was never entered"
        );

        let titled = FileName::new(&[], Some("x"), &[&keyword], id(), None);
        assert_eq!(titled.title(), None, "a title cut to nothing is no title");
        let built = titled
            .build()
            .unwrap_or_else(|error| panic!("a keyword that fits alone was refused: {error}"));
        assert!(
            !built.contains("--"),
            "the name carries a title delimiter with nothing behind it: {built}"
        );
        assert_eq!(
            FileName::parse(&built).expect("a built name parses"),
            titled,
            "the value does not equal what its own name encodes"
        );
    }

    #[test]
    fn a_component_that_ends_exactly_at_its_marker_is_absent_rather_than_empty() {
        // `field()`'s `rest.is_empty()` arm: a marker at the very end of the
        // name, with nothing after it at all, which Denote reads as an absent
        // component. Without the arm the loop over `rest.char_indices().skip(1)`
        // runs zero times and falls through to "the component runs to the end
        // of the name" — returning an empty component, so a name ending in a
        // bare marker would parse as having a title, a signature or a keyword
        // set that is the empty string.
        for (name, what) in [
            ("@@01J8XK2M9P51__", "keywords"),
            ("@@01J8XK2M9P51--", "a title"),
            ("@@01J8XK2M9P51==", "a signature"),
        ] {
            let parsed = FileName::parse(name).expect("the identifier is there");
            assert!(
                parsed.keywords().is_empty()
                    && parsed.title().is_none()
                    && parsed.signature().is_empty(),
                "a bare trailing marker became {what}: {parsed:?}"
            );
        }
        // **And at `field` itself**, because the constructors above normalise an
        // empty component away and so cannot tell "absent" from "present and
        // empty". Without the `is_empty()` arm this returns `Some(("", ""))`,
        // which is a component that exists and is the empty string — and every
        // caller that asked `is_some()` rather than looking inside would then
        // report a title on a name that has none.
        assert_eq!(super::field("@@01J8XK2M9P51--", "--"), None);
        assert_eq!(super::field("@@01J8XK2M9P51__", "__"), None);
    }

    #[test]
    fn an_extension_cannot_carry_a_path_separator_into_the_name() {
        // **`build()` never returns a path.** With `slug_extension` filtering a
        // `FORBIDDEN` of three characters while the other six Windows forbids
        // live in Denote's `REMOVED` — which the extension deliberately does not
        // run, because a real extension may contain `-`, `_` and `=` — `/`
        // reaches a built name, and with `..` beside it the name escapes the
        // directory a caller meant to write in. `parse(build(x)) == x` holds
        // throughout.
        for (extension, why) in [
            ("j/pg", "a separator"),
            ("../../etc/passwd", "a traversal"),
            ("a:b", "a drive separator"),
            ("a\"b", "a quote"),
            ("a|b", "a pipe"),
            ("a?b", "a wildcard"),
            ("a*b", "a wildcard"),
        ] {
            let name = FileName::new(&[], Some("t"), &[], id(), Some(extension));
            let built = name.build().expect("the rest of the name is legal");
            assert_eq!(
                std::path::Path::new(&built).components().count(),
                1,
                "{why} in an extension made `{built}` more than one path component"
            );
            for character in built.chars() {
                assert!(
                    !matches!(
                        character,
                        '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
                    ),
                    "{character:?} reached `{built}`"
                );
            }
        }
    }

    #[test]
    fn an_extension_cannot_carry_a_control_character_into_the_name() {
        // `slug_extension` calls `unusable`, so no control character reaches a
        // built name — not only the whitespace ones, which
        // `char::is_whitespace` happens to cover. A NUL cannot be a byte of a
        // path on any filesystem there is.
        for control in ['\u{0}', '\u{1}', '\u{7}', '\u{1b}', '\u{1f}'] {
            let built = FileName::new(&[], None, &[], id(), Some(&format!("a{control}b")));
            assert_eq!(built.extension(), Some("ab"), "U+{:04X}", control as u32);
            let name = built.build().expect("builds");
            assert!(
                !name.chars().any(|c| (c as u32) <= 0x1f),
                "a control character reached the name: {name:?}"
            );
        }
    }

    #[test]
    fn an_extension_cannot_carry_a_doubled_delimiter_into_the_name() {
        // Acceptance criterion 8, and the one component whose sluggifier does
        // not remove the scheme's delimiters. Uncollapsed, `Some("==a")` builds
        // `@@01J8XK2M9P51.==a`, and `parse` reads the `==` as the start of a
        // signature — so a round trip through the public constructor returns a
        // name with a signature the original never had.
        for typed in ["==a", "= =a", "--a", "__b", "a==b"] {
            let built = FileName::new(&[], None, &[], id(), Some(typed));
            let name = built.build().expect("builds");
            let read = FileName::parse(&name).expect("parses");
            assert_eq!(read, built, "{typed} did not survive its own name: {name}");
        }
    }

    #[test]
    fn a_control_character_is_removed_rather_than_read_as_a_space() {
        // The second and last addition to Denote in this module. Denote's
        // `denote-slug-hyphenate` matches `_\| +` — a literal space — so a tab
        // survives into the name it produces, and Windows rejects every C0
        // character in a path.
        //
        // Removed, not turned into a boundary: Denote reads `a<tab>b` as one
        // word, so `ab` is the nearer of the two answers available and `a-b`
        // would invent a boundary nobody typed.
        let tabbed = FileName::new(&[], Some("a\tb"), &[], id(), None);
        assert_eq!(tabbed.title(), Some("ab"));
        let newline = FileName::new(&[], Some("one\ntwo"), &[], id(), None);
        assert_eq!(newline.title(), Some("onetwo"));
        // And a non-breaking space is **not** a control character and **not**
        // Denote's boundary, so it stays. This is the assertion that keeps the
        // rule above from quietly becoming "anything space-like".
        let nbsp = FileName::new(&[], Some("a\u{a0}b"), &[], id(), None);
        assert_eq!(nbsp.title(), Some("a\u{a0}b"));
        // And a C1 control, which Windows permits and Denote keeps. With
        // `is_control` as the test here — Unicode's `Cc` category — this would
        // become `ab`, a third addition to Denote that no document records.
        let c1 = FileName::new(&[], Some("a\u{85}b"), &[], id(), None);
        assert_eq!(c1.title(), Some("a\u{85}b"));
        let del = FileName::new(&[], Some("a\u{7f}b"), &[], id(), None);
        assert_eq!(del.title(), Some("a\u{7f}b"));
    }

    #[test]
    fn a_multi_word_keyword_becomes_one_word_as_denote_writes_it() {
        // `denote-sluggify-keyword` removes the spaces rather than replacing
        // them, so the words join. That is Denote's answer to the fact that `_`
        // both joins and separates, and it keeps the keyword whole.
        let name = FileName::new(&[], None, &["rio de janeiro"], id(), None);
        assert_eq!(name.keywords(), ["riodejaneiro"]);
        let built = name.build().expect("builds");
        assert_eq!(FileName::parse(&built).expect("parses"), name);
    }

    #[test]
    fn a_parsed_name_is_normalised_exactly_like_a_built_one() {
        // Written raw, a parsed value could hold spaces and uppercase that
        // `new` exists to strip — and `build` would emit them.
        let parsed = FileName::parse("--Hello World!@@01J8XK2M9P51").expect("parses");
        assert_eq!(parsed.title(), Some("hello-world"));
        assert_eq!(
            parsed.build().expect("builds"),
            "--hello-world@@01J8XK2M9P51"
        );
    }

    #[test]
    fn a_long_title_is_shortened_at_construction_so_the_name_still_round_trips() {
        // Shortened by `build` instead, a long title would produce a name that
        // does not parse back to the value that produced it.
        let long = "a".repeat(400);
        let name = FileName::new(&["family"], Some(&long), &["holiday"], id(), Some("jpeg"));
        let built = name.build().expect("builds");
        assert!(built.len() <= MAX_BYTES, "{} bytes", built.len());
        assert_eq!(FileName::parse(&built).expect("parses"), name);
    }

    #[test]
    fn a_long_title_is_shortened_and_the_identifier_is_not() {
        let long = "a".repeat(400);
        let name = FileName::new(&["family"], Some(&long), &["holiday"], id(), Some("jpeg"))
            .build()
            .expect("builds");
        assert!(name.len() <= MAX_BYTES, "{} bytes", name.len());
        assert!(
            name.contains(&format!("@@{}", id().to_crockford())),
            "the id was shortened"
        );
        assert_eq!(
            FileName::parse(&name).expect("parses").extension(),
            Some("jpeg")
        );
    }

    #[test]
    fn a_name_that_cannot_fit_even_without_a_title_is_refused() {
        // Refused rather than silently truncated somewhere else: the caller gave
        // a signature or keywords that do not fit, and shortening the title —
        // the only thing this is allowed to shorten — would not have helped.
        let huge: Vec<&str> = Vec::new();
        let keyword = "k".repeat(300);
        let name = FileName::new(&huge, None, &[&keyword], id(), None).build();
        assert!(matches!(name, Err(NameError::TooLong { .. })), "{name:?}");
    }

    #[test]
    fn a_compound_extension_survives() {
        // `tar.gz` stays `tar.gz`, not `targz`. Denote does not sluggify an
        // extension, and the dot is safe because the parser splits at the first
        // one after the identifier.
        let name = FileName::new(&[], Some("backup"), &[], id(), Some("TAR.GZ"));
        assert_eq!(name.extension(), Some("tar.gz"));
        let built = name.build().expect("builds");
        assert_eq!(FileName::parse(&built).expect("parses"), name);
    }

    #[test]
    fn a_name_that_is_not_the_canonical_spelling_has_one_proposed() {
        // Decoding is lenient on purpose, so one identifier has many spellings —
        // and `NOCTUA.md §13.1` makes the identifier what identity depends on.
        // This is how a caller learns that two files claiming one identity are
        // spelled differently, and what to rename them to.
        let lenient = "--praia@@01j8xk2m9p51.jpg";
        assert_eq!(
            FileName::canonical_name(lenient).expect("parses"),
            Some("--praia@@01J8XK2M9P51.jpg".to_owned())
        );
        assert_eq!(
            FileName::canonical_name("--praia@@01J8XK2M9P51.jpg").expect("parses"),
            None,
            "a name already canonical needs no proposal"
        );
    }

    /// A name that parses and cannot be built says so, rather than saying it is
    /// already canonical.
    ///
    /// **The `?` that carries it, decided here.** `canonical_name`'s
    /// own `# Errors` paragraph documents this exact case — *"a name that
    /// parses can still be one this scheme will not write"* — and
    /// written `build().unwrap_or_else(|_| name.to_owned())`, without this the
    /// whole gate stays green: `check passed`, and the function then
    /// answers `Ok(None)`, meaning *already the canonical spelling*, for a name
    /// it cannot write at all. A caller renaming files by that answer leaves
    /// the pair it was asked about exactly as it found them.
    #[test]
    fn a_name_that_parses_and_cannot_be_built_says_so() {
        // Over the bound in the keywords, which `parse` reads and `build`
        // refuses. Nothing here is a title, so no shortening applies.
        let long = format!("__{}@@01J8XK2M9P51.jpg", "keyword_".repeat(40));
        assert!(
            FileName::parse(&long).is_ok(),
            "the fixture has to be one `parse` accepts, or this proves nothing"
        );
        // The refusal `build` gives, not one written out here: the number is
        // the length of the name this scheme *would* write, which is not the
        // length of the one it was handed.
        let refused = FileName::parse(&long)
            .expect("parses")
            .build()
            .expect_err("a name over the bound is not one this scheme writes");
        assert!(
            matches!(refused, NameError::TooLong { .. }),
            "the fixture is refused for some other reason: {refused:?}"
        );
        assert_eq!(
            FileName::canonical_name(&long),
            Err(refused),
            "a name this scheme will not write was reported as one already spelled right"
        );

        // And the same door one step earlier: a name that does not parse is not
        // a name that is already spelled right either. Written
        // `let Ok(parsed) = Self::parse(name) else { return Ok(None) }` the
        // whole crate stays green over the row above.
        assert_eq!(
            FileName::canonical_name("--just-a-title.jpg"),
            Err(NameError::NoIdentifier),
            "a name with no identifier in it was reported as one already spelled right"
        );
    }

    #[test]
    fn a_name_with_no_identifier_is_refused_by_name() {
        assert_eq!(
            FileName::parse("--just-a-title.jpg"),
            Err(NameError::NoIdentifier)
        );
        assert!(matches!(
            FileName::parse("@@not-base32-U.jpg"),
            Err(NameError::BadIdentifier(_))
        ));
    }

    #[test]
    fn slugging_nothing_yields_nothing() {
        assert_eq!(slug_title("---"), None);
        assert_eq!(slug_title(""), None);
        assert_eq!(slug_keyword("   "), None);
        assert_eq!(slug_signature("=="), None);
    }

    /// A title ends at `==`, and a hand-typed name is the only thing that asks.
    ///
    /// **`terminators` has one clause only this decides.** Deleting `"=="` from
    /// the title's list leaves the rest of the gate at `check passed`,
    /// while `--t==s@@…` parses as the title `ts` and the signature `s` — the
    /// title swallowing a field it does not own. The property test cannot reach
    /// it: `build` emits the signature *before* the title, so no generated name
    /// carries `==` after `--`, and `raw_name` draws titles from an alphabet
    /// with no `=` in it. Only a name a person renamed by hand has this shape,
    /// which the paragraph above `parse` calls exactly the input `parse` exists
    /// for.
    ///
    /// The sibling clauses are each decided by a test already. `--` is
    /// deliberately absent from the title's terminators, and the test that says
    /// so
    /// is `against_denote.rs`'s `a_name_with_a_repeated_marker_is_split_where_denote_splits_it`.
    #[test]
    fn a_title_ends_at_a_signature_a_person_typed_after_it() {
        let name = FileName::parse("--t==s@@01J8XK2M9P51").expect("a parsable name");
        assert_eq!(
            name.title.as_deref(),
            Some("t"),
            "the title ran past `==` and took the signature with it"
        );
        assert_eq!(
            name.signature,
            vec!["s".to_owned()],
            "the signature after a hand-typed title was not read as one"
        );

        // And with every marker after the title, in the order Denote's own
        // regexp lists them, so the clause is decided for each rather than for
        // the first one that happens to be tried.
        let all = FileName::parse("--abc==def__k@@01J8XK2M9P51.md").expect("a parsable name");
        assert_eq!(all.title.as_deref(), Some("abc"));
        assert_eq!(all.signature, vec!["def".to_owned()]);
        assert_eq!(all.keywords, vec!["k".to_owned()]);
        assert_eq!(all.extension.as_deref(), Some("md"));
    }

    /// The extension is what follows a **dot** after the identifier.
    ///
    /// **Everything after the identifier is not the extension**, and taking it
    /// wholesale leaves the rest of `cargo test -p nc-ids` green: `@@…==s` then
    /// reports the extension `=s`, and `build` proposes renaming a real file to
    /// one carrying an extension it never had. `parse(build(x)) == x` cannot
    /// see this — the wrong answer is a fixed point — which is why criterion 8
    /// is not where this belongs.
    #[test]
    fn a_marker_after_the_identifier_is_not_an_extension() {
        let signed = FileName::parse("@@01J8XK2M9P51==s").expect("a parsable name");
        assert_eq!(
            signed.extension, None,
            "a signature after the identifier was read as an extension"
        );
        let keyworded = FileName::parse("--t@@01J8XK2M9P51__k").expect("a parsable name");
        assert_eq!(
            keyworded.extension, None,
            "a keyword after the identifier was read as an extension"
        );
        // The dot still is one, so this test cannot pass by refusing every
        // extension there is.
        let dotted = FileName::parse("--t@@01J8XK2M9P51.md").expect("a parsable name");
        assert_eq!(dotted.extension.as_deref(), Some("md"));
    }

    /// Dropping the title is what `shorten` does when dropping it is enough.
    ///
    /// **The guard it does that under,
    /// decided.** Removing `if without.len() <= MAX_BYTES` leaves the rest of
    /// the suite green and drops the caller's title from names that are over
    /// the bound *with or without* it — so the value loses a field it was
    /// given, and `TooLong { bytes }`, which is documented as what the name
    /// would have been, reports a number that is the length of no name at all.
    #[test]
    fn a_title_is_kept_when_dropping_it_would_not_help() {
        let keyword = "k".repeat(300);
        let held = FileName::new(
            &[],
            Some("a-title"),
            &[keyword.as_str()],
            Snowflake::from_crockford("01J8XK2M9P51").expect("a valid identifier"),
            Some("md"),
        );
        assert_eq!(
            held.title.as_deref(),
            Some("a-title"),
            "the title was dropped by a shortening that could not have helped"
        );
        let Err(NameError::TooLong { bytes }) = held.build() else {
            panic!("a 300-byte keyword must not fit");
        };
        // The same name without the title, so the number is compared against a
        // measurement rather than against a constant somebody typed.
        let untitled = FileName::new(
            &[],
            None,
            &[keyword.as_str()],
            Snowflake::from_crockford("01J8XK2M9P51").expect("a valid identifier"),
            Some("md"),
        );
        let Err(NameError::TooLong { bytes: without }) = untitled.build() else {
            panic!("a 300-byte keyword must not fit without a title either");
        };
        assert_eq!(
            bytes - without,
            "--a-title".len(),
            "`TooLong` reports what the name would have been, and the title is not in it"
        );
        assert!(
            bytes > MAX_BYTES,
            "`TooLong` reported {bytes}, which is not over the bound it refused for"
        );
    }
}
