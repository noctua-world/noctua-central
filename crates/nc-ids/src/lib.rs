//! Identifiers: what a thing is called, and what it is called on disk.
//!
//! Four things live here, and they are together because they are one decision
//! seen from four sides (`NOCTUA.md §16`):
//!
//! * [`Snowflake`] — the identifier everything the platform creates carries.
//! * [`MachineFlake`] — the ten bits inside it that say which computer minted it.
//! * [`OpaqueId`] — the *other* identifier, whose shape is deliberately not
//!   decided yet, held as something nothing can depend on the inside of.
//! * [`FileName`] — the naming scheme a saved file is proposed under.
//!
//! **Nothing here talks to a database, a clock it did not take, or the network.**
//! Minting takes its clock as a parameter, so both of the hazards that produce
//! duplicate identifiers can be tested without waiting for a real millisecond to
//! pass — and a test that waits is a test somebody eventually deletes.

#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]
// Every doctest of this crate forbids `unsafe` as the crate does: a
// doctest is a crate of its own, outside the root's `forbid`.
#![doc(test(attr(forbid(unsafe_code))))]
pub mod base32;
pub mod flake;
pub mod naming;
pub mod snowflake;

pub use flake::MachineFlake;
pub use naming::{FileName, NameError};
pub use snowflake::EPOCH_MS;
pub use snowflake::{Ceiling, CeilingRefused, Clock, MintError, Minter, Snowflake, SystemClock};

/// An **opaque identifier** (`NOCTUA.md §16.1`), held as what it is: an opaque string
/// of bounded length.
///
/// **Its shape is an open question** (`§5.1`), deliberately, and belongs to a
/// much later stage. This type exists so that nothing built before then
/// forecloses the answer: it derives no ordering, offers no accessor for a
/// prefix, a byte or a version field, and does no parsing. Do not build an
/// index that assumes its internals, and do not derive anything from them — the
/// whole point of holding it this way is that a `UUIDv7`, a ULID or something
/// else entirely can arrive later without a migration.
///
/// **What it does not do is make the inside unreachable.** It hands out the
/// whole string: [`OpaqueId::as_str`] is what every store and every wire format
/// needs, and `id.as_str().as_bytes()[0]` and `sort_by_key(|id| id.as_str())`
/// both compile. The guarantee is about what this type *invites* — one reader,
/// whole, with no shape-shaped accessor beside it — not about what a determined
/// caller cannot write. A doc that overstates a guarantee is worse than one
/// that states a narrower one, because the narrower claim is the one a reviewer
/// can check; `as_str`'s own paragraph states it the same way.
///
/// It **is** allowed into the central log, because an opaque identifier is not
/// user data (`§15.4`).
/// # Why there is no ordering
///
/// Deriving `PartialOrd` and `Ord` would give lexicographic order over the
/// bytes — and for a `ULID` or a `UUIDv7`, that order **is** time order. `§5.1`
/// is `[OPEN]` precisely so nothing built now depends on an opaque identifier's
/// structure, and a derived `Ord` is how something would:
/// a `BTreeMap<OpaqueId, _>` or a `sort()` reads as innocuous and quietly
/// assumes the shape this repository has not chosen yet. `Eq` and `Hash` are
/// enough to use one as a `HashMap` key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpaqueId(String);

/// The longest an opaque identifier may be.
///
/// Bounded because everything that stores one needs a column width and a buffer
/// size before the shape is decided. 128 is comfortably past every candidate,
/// measured rather than guessed: a hyphenated UUID is 36, a ULID 26, the URN
/// form of a UUID 45, a `did:key` 56, and a multibase-encoded BLAKE3 digest 67.
/// A bound of 64 would exclude the last two, which is exactly the foreclosing
/// this type exists to avoid.
pub const OPAQUE_ID_MAX: usize = 128;

/// Why a string is not an acceptable opaque identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpaqueIdError {
    /// It is empty, or longer than [`OPAQUE_ID_MAX`].
    Length {
        /// How long it was.
        bytes: usize,
    },
    /// It holds a character that would not survive being written into a log
    /// line, a file name or a URL.
    Character(char),
}

impl OpaqueIdError {
    /// The registry code this failure carries.
    ///
    /// See [`crate::base32::Base32Error::code`] for why the three identifier
    /// failures share one.
    #[must_use]
    pub const fn code(self) -> nc_catalog::ErrorCode {
        nc_catalog::ErrorCode::IdsNotAnIdentifier
    }
}

impl core::fmt::Display for OpaqueIdError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Length { bytes } => write!(
                formatter,
                "an identifier of {bytes} bytes is outside 1..={OPAQUE_ID_MAX}"
            ),
            Self::Character(character) => {
                write!(
                    formatter,
                    "`{}` cannot appear in an identifier",
                    nc_canon::shown(*character)
                )
            }
        }
    }
}

impl core::error::Error for OpaqueIdError {}

impl OpaqueId {
    /// Takes a string as an opaque identifier.
    ///
    /// # Errors
    ///
    /// Refuses anything outside `1..=`[`OPAQUE_ID_MAX`] bytes, and anything
    /// holding a character that is not an ASCII letter, digit, `-`, `_`, `.` or
    /// `:`.
    ///
    /// The character rule is not about the shape the answer will take — it is
    /// about the one place an identifier must never be able to reach: a
    /// newline, a control character or a quote would let one forge a line in the
    /// central log or a field in a record.
    ///
    /// **`.` and `:` are allowed**, although `.` is a file-name delimiter
    /// and `/` escapes a URL path: excluding them on those grounds would
    /// exclude `did:key:z…`, `urn:uuid:…` and every multibase form — live
    /// candidates for a content-addressed identifier in a project whose `§13.1`
    /// is BLAKE3 content addressing — and `§5.1` is open precisely so nothing
    /// forecloses them. Escaping for a file name, a URL or a log line belongs
    /// at those boundaries, each of which already has its own rule; `/` stays
    /// out because it is the one character that changes what a path *means*
    /// rather than how it reads.
    pub fn new(text: impl Into<String>) -> Result<Self, OpaqueIdError> {
        let text = text.into();
        if text.is_empty() || text.len() > OPAQUE_ID_MAX {
            return Err(OpaqueIdError::Length { bytes: text.len() });
        }
        if let Some(bad) = text
            .chars()
            .find(|c| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':')))
        {
            return Err(OpaqueIdError::Character(bad));
        }
        Ok(Self(text))
    }

    /// The identifier a device's public key is derived to: the 64 lower-case
    /// hexadecimal digits of a domain-separated BLAKE3 digest of it.
    ///
    /// **Derived, never copied** (`§5.1`), and minted here rather than at the
    /// caller, because a crate that reaches the central log may not mint one
    /// from text at all. **A digest, so nothing a caller hands in can be read
    /// back out**: thirty-two bytes chosen to spell something come out as
    /// digits that spell nothing, which is what lets the log's wall name this
    /// a route that carries no caller text. This decides nothing about the
    /// shape `§5.1` leaves open: it is one opaque string, with no accessor for
    /// any part of it.
    #[must_use]
    pub fn of_device_key(public: &[u8; 32]) -> Self {
        use core::fmt::Write as _;
        const DOMAIN: &str = "noctua-central/ids/device-identifier/v1";
        let mut hasher = blake3::Hasher::new();
        hasher.update(&(DOMAIN.len() as u64).to_le_bytes());
        hasher.update(DOMAIN.as_bytes());
        hasher.update(public);
        let mut text = String::with_capacity(64);
        for byte in hasher.finalize().as_bytes() {
            // Writing to a `String` cannot fail.
            let _ = write!(text, "{byte:02x}");
        }
        Self(text)
    }

    /// The identifier as text, for storing and for sending.
    ///
    /// Deliberately a whole string, and deliberately the only *accessor*: there
    /// is none for a prefix, a byte or a version field, because every one of
    /// those would be something a later stage could come to depend on before
    /// `§5.1` is answered. It is not the only way to read the value — [`Display`]
    /// prints the same string, which is why `an_opaque_identifier_holds_no_shape`
    /// pins both and refuses a third.
    ///
    /// [`Display`]: core::fmt::Display
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl core::fmt::Display for OpaqueId {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    /// A device's key is derived to an identifier of its own, the same each
    /// time, holding none of the key's bytes.
    ///
    /// **And one the constructor of text would take**, so it is stored and
    /// sent like any other.
    #[test]
    fn a_device_key_is_derived_to_an_identifier_of_its_own() {
        use core::fmt::Write as _;
        let mut key = [0_u8; 32];
        let zero = super::OpaqueId::of_device_key(&key);
        // **Frozen**: every journal a device keeps names its keeper this way,
        // so a derivation that moved is every history's keeper renamed.
        assert_eq!(
            zero.as_str(),
            "e1a15cfa5b6144c05041ca067a920cc24a5099dcb9dd2e10f37b1d240d52a4a4",
            "the derivation moved"
        );
        assert_eq!(zero, super::OpaqueId::of_device_key(&key));
        assert_eq!(
            super::OpaqueId::new(zero.as_str()),
            Ok(zero.clone()),
            "the derived text is one the constructor of text admits"
        );
        key[31] = 1;
        assert_ne!(super::OpaqueId::of_device_key(&key), zero);
        // **And the bytes do not come back out**: thirty-two bytes that spell
        // something are not in what they derive, written either way.
        let spelled = *b"ana, born 1988-04-12, id 1234567";
        let derived = super::OpaqueId::of_device_key(&spelled);
        let mut hex = String::new();
        for byte in spelled {
            let _ = write!(hex, "{byte:02x}");
        }
        assert!(!derived.as_str().contains(&hex[..16]) && !derived.as_str().contains("ana"));
    }

    /// Every identifier failure says which failure it is.
    ///
    /// **All three `NC-IDS-0006` producers, asserted.** Without this, two of
    /// them set to plainly wrong codes — `CanonNumber` and `DbMigrationFailed`
    /// — leave the whole suite green and `check passed`. A code exists so a log
    /// can be queried by it; a code nothing pins is a code that drifts to
    /// whatever the last edit left behind.
    #[test]
    fn every_identifier_failure_carries_the_code_it_should() {
        use nc_catalog::ErrorCode;

        assert_eq!(
            super::OpaqueId::new("")
                .expect_err("empty is not an identifier")
                .code(),
            ErrorCode::IdsNotAnIdentifier
        );
        assert_eq!(
            super::OpaqueId::new("a\u{1}b")
                .expect_err("a control character is not one either")
                .code(),
            ErrorCode::IdsNotAnIdentifier
        );
        assert_eq!(
            super::base32::decode("")
                .expect_err("empty decodes to nothing")
                .code(),
            ErrorCode::IdsNotAnIdentifier
        );
        assert_eq!(
            super::base32::decode("ZZZZZZZZZZZZZZZZZZ")
                .expect_err("too long is not an identifier")
                .code(),
            ErrorCode::IdsNotAnIdentifier
        );
        // And the flake, which is a number rather than text and carries the code
        // whose description is true of it.
        assert_eq!(
            super::MachineFlake::from_value(u16::MAX)
                .expect_err("out of range")
                .code(),
            ErrorCode::ValidationOutOfRange
        );
        // The floor: these are three different codes in the registry, so a
        // `code()` that answered one value for everything would satisfy the
        // lines above only if they all named it.
        assert_ne!(
            ErrorCode::IdsNotAnIdentifier,
            ErrorCode::ValidationOutOfRange
        );
    }

    use super::{OPAQUE_ID_MAX, OpaqueId, OpaqueIdError};

    /// The two declared widths are the numbers their paragraphs argue for.
    ///
    /// **Pinned against the argument, not only against themselves.** Without
    /// this, `OPAQUE_ID_MAX` lowered from 128 to 67 and `MAX_WIDTH` raised from
    /// 13 to 20 each leave `cargo xtask check` green. 67 is exactly the length
    /// of the longest candidate this holder exists to keep possible — a
    /// multibase BLAKE3 digest — so it leaves no headroom at all, which is the
    /// same foreclosing mistake this crate's own doc gives as the reason
    /// against 64. And `MAX_WIDTH` is otherwise caught when it *falls*
    /// (`encode(u64::MAX).len() <= MAX_WIDTH`) and not when it rises, so a
    /// decoder that accepts a twenty-character identifier from some other
    /// scheme would be one digit away.
    #[test]
    fn every_declared_width_is_the_number_its_paragraph_argues_for() {
        assert_eq!(
            OPAQUE_ID_MAX, 128,
            "comfortably past every candidate §5.1 is still choosing between — the \
             longest, a multibase BLAKE3 digest, is 67 — because a bound with no \
             headroom forecloses the open question it is holding room for"
        );
        assert_eq!(
            crate::base32::MAX_WIDTH,
            13,
            "32^13 > 2^64, so thirteen is the ceiling: a longer string is either not \
             an identifier or an identifier from a scheme this is not"
        );
    }

    #[test]
    fn every_candidate_shape_is_an_acceptable_opaque_identifier() {
        // §5.1 is open. This asserts the holder does not accidentally exclude
        // any of the answers it is waiting for.
        for candidate in [
            "018f3b2c-9f4a-7c1d-8e5f-2a6b4c8d0e1f", // UUIDv7, hyphenated
            "01ARZ3NDEKTSV4RRFFQ69G5FAV",           // ULID
            "018f3b2c9f4a7c1d8e5f2a6b4c8d0e1f",     // UUID, unhyphenated
            "urn:uuid:018f3b2c-9f4a-7c1d-8e5f-2a6b4c8d0e1f", // the URN form
            "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK", // a DID
            "b3:af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262", // multibase
        ] {
            assert!(OpaqueId::new(candidate).is_ok(), "{candidate}");
        }
        // **And every punctuation character the rule admits, by itself.** The
        // six candidates above carry `-` and `:`, and `tampering.rs`'s probes
        // carry `.` — so without this `_` would be in the documented set and
        // decided by nothing. Measured that way, `'_'` dropped from the
        // `matches!` in `OpaqueId::new`: `cargo xtask check --offline` answers
        // `check passed`, every verdict line identical to a clean run. `_` is
        // half of RFC 4648's base64url alphabet, which every multibase and
        // JWK-thumbprint candidate for `§5.1` uses, and this holder exists so
        // that nothing forecloses those answers — a rule that refuses one is
        // the *too strong* shape, green only because nothing legitimate has
        // tried yet.
        //
        // Written out per character rather than derived from the `matches!`,
        // which would be the same list twice and agree with itself however it
        // changed.
        for admitted in ['-', '_', '.', ':'] {
            let candidate = format!("a{admitted}b");
            assert!(
                OpaqueId::new(candidate.clone()).is_ok(),
                "`{admitted}` is in this holder's documented character set and is refused: \
                 {candidate}"
            );
        }
    }

    #[test]
    fn a_character_that_would_forge_a_log_line_is_refused() {
        assert_eq!(OpaqueId::new("a\nb"), Err(OpaqueIdError::Character('\n')));
        assert_eq!(OpaqueId::new("a\"b"), Err(OpaqueIdError::Character('"')));
        assert_eq!(OpaqueId::new("a b"), Err(OpaqueIdError::Character(' ')));
        // `/` is the one that changes what a path means rather than how it reads.
        assert_eq!(OpaqueId::new("a/b"), Err(OpaqueIdError::Character('/')));
    }

    /// Both refusals name the value and the bound they were judged against.
    ///
    /// **The third identifier failure in this crate carries the test its two
    /// siblings carry.** `base32` and `flake` each have one; with this sentence
    /// rendered by nothing, `{bytes}` written `{}`-of-`chars().count()` and the
    /// bound written backwards both survive the whole workspace. A length
    /// refusal that says *"an identifier of 3 bytes is outside 1..=64"* about a
    /// value of 3 *characters* and 6 bytes is a sentence that sends a reader to
    /// the wrong end of their input.
    #[test]
    fn each_refusal_names_the_value_and_the_bound_it_was_judged_against() {
        // Six bytes and three characters, so the two numbers cannot be confused
        // for one another.
        let long = "\u{e9}".repeat(OPAQUE_ID_MAX);
        let refused = OpaqueId::new(long.clone()).expect_err("past the bound");
        let said = refused.to_string();
        assert!(
            said.contains(&long.len().to_string()),
            "the refusal does not name the number of bytes it judged: {said}"
        );
        assert!(
            said.contains(&OPAQUE_ID_MAX.to_string()),
            "the refusal does not name the bound: {said}"
        );
        // **The order, not only the presence.** `1..=64` written `64..=1` names
        // both numbers and tells a reader the opposite of the rule.
        let low = said.find("1..=").expect("the bound reads upwards: {said}");
        let high = said
            .find(&OPAQUE_ID_MAX.to_string())
            .expect("the bound names its maximum");
        assert!(low < high, "the bound is the wrong way round: {said}");

        // And the character refusal names the character, through the escape
        // that keeps a newline out of a diagnostic.
        let forged = OpaqueId::new("a\nb").expect_err("a newline is refused");
        assert!(
            forged.to_string().contains("U+000A"),
            "the refusal does not name the character it refused: {forged}"
        );
    }

    /// A letter that is not an ASCII letter is not one of these.
    ///
    /// **The grammar is ASCII by test, not only by spelling.** Measured without
    /// this, `is_ascii_alphanumeric`
    /// written `is_alphanumeric`: `cargo xtask check` prints `check passed`,
    /// exit 0, zero `warn` lines, every verdict identical to a clean run. Three
    /// places in this fleet state the ASCII grammar as fact, one of them
    /// criterion 3's own floor — *"the identifier fields cannot hold one"* —
    /// and that floor is a pairwise collision test whose corpus holds a single
    /// precomposed non-ASCII string, so it cannot see this.
    ///
    /// What it admits is a homoglyph: `асme` in Cyrillic `а` and `с` is a
    /// different realm from `acme` and looks like it in every log line, every
    /// diagnostic and every review — and the realm is the column
    /// `Journal::append` scopes a write by.
    ///
    /// Every candidate shape `§5.1` leaves open is ASCII, so this refuses none
    /// of them.
    #[test]
    fn a_letter_outside_ascii_is_not_a_letter_here() {
        for forged in [
            // Cyrillic `а` and `с`, which render as the Latin ones.
            "\u{430}\u{441}me",
            // A precomposed accent, and the same word decomposed.
            "caf\u{e9}",
            "cafe\u{301}",
            // A full-width digit, which `is_alphanumeric` accepts.
            "realm\u{ff11}",
        ] {
            let refused = OpaqueId::new(forged)
                .expect_err("a character outside ASCII was admitted into an identifier");
            assert!(
                matches!(refused, OpaqueIdError::Character(_)),
                "`{forged}` was refused for its length rather than for what is in it: \
                 {refused:?}"
            );
        }
        // And the floor: the ASCII spelling of the same word is a legal one, so
        // this is a rule about the alphabet rather than one that refuses
        // everything.
        assert!(OpaqueId::new("acme").is_ok());
        assert!(OpaqueId::new("cafe").is_ok());
    }

    #[test]
    fn a_length_outside_the_bound_is_refused_with_the_length() {
        assert_eq!(OpaqueId::new(""), Err(OpaqueIdError::Length { bytes: 0 }));
        let long = "a".repeat(OPAQUE_ID_MAX + 1);
        assert_eq!(
            OpaqueId::new(long),
            Err(OpaqueIdError::Length {
                bytes: OPAQUE_ID_MAX + 1
            })
        );
    }
}

#[cfg(test)]
mod diagnostic_tests {
    use crate::{OpaqueId, Snowflake, naming::FileName};

    /// Every refusal this crate can hand a reader says the right numbers.
    ///
    /// **Two of them are checked only here.** The sibling test,
    /// `no_refusal_carries_the_character_it_refuses`, renders `OpaqueIdError`,
    /// `Base32Error` and `NameError`, and neither `FlakeOutOfRange` nor
    /// `Display for Snowflake`. Measured without this, each alone:
    ///
    /// ```text
    /// "outside 0..{RANGE}"  ->  "outside 0..{BITS}"     workspace suite green
    /// value_given -> self.value + 1                     workspace suite green
    /// Display for Snowflake -> &self.to_crockford()[1..]  workspace suite green
    /// ```
    ///
    /// The first makes a bound contradict itself — *"a machine flake of 65535
    /// is outside 0..10"* for a bound of 1024, which is the `§15.1`
    /// shape `base32.rs` has a whole module about. The third is worse than a
    /// wrong sentence: `{}` on a `Snowflake` becomes a *second spelling* of one
    /// identity, eleven characters where the layout is twelve,
    /// and `against_denote.rs` interpolates one into a name — so the corpus
    /// passes with a short identifier in it, because `decode` is lenient about
    /// width and `encode` re-pads.
    ///
    /// The numbers are read from the layout rather than written out, so a
    /// reworded sentence still passes; what is pinned is that the sentence names
    /// the value it refused and the bound it refused it against.
    #[test]
    fn every_refusal_names_the_value_and_the_bound_it_was_judged_against() {
        let refusal =
            crate::MachineFlake::from_value(u16::MAX).expect_err("a flake past the layout");
        assert_eq!(
            refusal.value_given(),
            u16::MAX,
            "the refusal hands back a value other than the one it refused"
        );
        let said = refusal.to_string();
        assert!(
            said.contains(&u16::MAX.to_string()),
            "the refusal does not name the value it refused: {said}"
        );
        assert!(
            said.contains(&crate::flake::RANGE.to_string()),
            "the refusal names a bound other than the layout's, so it can contradict \
             itself — `outside 0..10` for a bound of 1024: {said}"
        );

        // **And an identifier written with `{}` is the identifier.** A second
        // spelling of one identity is what `canonical_name` exists to stop.
        let id = Snowflake::from_crockford("01J8XK2M9P51").expect("a valid identifier");
        assert_eq!(
            id.to_string(),
            id.to_crockford(),
            "`{{}}` on an identifier writes something other than its Crockford text"
        );
        assert_eq!(
            id.to_string(),
            "01J8XK2M9P51",
            "the layout is twelve characters and this is not that name"
        );
    }

    #[test]
    fn no_refusal_carries_the_character_it_refuses() {
        // **The messages that exist *because* a character would forge a line
        // must not be the one place that character appears unescaped.**
        // `OpaqueId::new`'s own doc says the rule exists because "a newline, a
        // control character or a quote would let one forge a line in the
        // central log or a field in a record" — so its refusal does not write
        // the newline out raw.
        //
        // `§15.1` asks a diagnostic to point at an exact location. A message
        // with a line break in it does not.
        //
        // **And the list is `nc-canon`'s rule, not a copy of it.** A copy of
        // eight characters — that crate's ten minus `U+000B` and `U+0085` —
        // would let `OpaqueId`'s character rule be widened to admit exactly
        // those two with `check passed`, and an identifier holding a vertical
        // tab and a NEL would go into the central log and into a journal
        // entry's hashed form. One rule written twice with the second copy
        // narrower is the shape this tree carries three paragraphs about, and
        // `nc-canon`'s own doc says the list is in one place *so that a gap is
        // visible*. Asked of the predicate, a character added there is
        // exercised here without anybody remembering to.
        let forging: Vec<char> = ('\u{0}'..='\u{9f}')
            .chain(['\u{2028}', '\u{2029}', '"', '\\'])
            .filter(|character| nc_canon::is_unsafe_in_a_message(*character))
            .collect();
        assert!(
            forging.len() > 60,
            "{} character(s) were judged unsafe, so this loop is asking a predicate that \
             has stopped answering",
            forging.len()
        );
        for bad in forging {
            let rendered = [
                OpaqueId::new(format!("a{bad}b"))
                    .expect_err("not an identifier")
                    .to_string(),
                Snowflake::from_crockford(&format!("01{bad}"))
                    .expect_err("not base32")
                    .to_string(),
                FileName::parse(&format!("@@01{bad}"))
                    .expect_err("not a name")
                    .to_string(),
            ];
            for message in rendered {
                assert!(
                    !message.contains(bad),
                    "a refusal of U+{:04X} contains one: {message:?}",
                    bad as u32
                );
                // Not `|| !message.contains(bad)`, which the assertion above
                // has already established and which therefore made this whole
                // second assertion always true — leaving `shown` free to stop
                // naming the character at all.
                assert!(
                    message.contains(&format!("U+{:04X}", bad as u32)),
                    "a refusal that does not name what it refused: {message}"
                );
            }
        }
    }
}

/// `OpaqueId` carries no ordering, and nothing here reads its bytes.
///
/// Its own module because the question is about the type's *surface* rather
/// than about a value: what is asked is what the compiler will accept, and the
/// answer has to be arranged rather than asserted.
#[cfg(test)]
mod an_opaque_identifier_holds_no_shape {
    use super::OpaqueId;

    /// The name of the public method this line declares, if it declares one.
    ///
    /// **Not `strip_prefix("pub fn ")`.** `pub const fn` is the ordinary
    /// spelling for a cheap accessor, and under that prefix a reader shaped
    /// exactly like the version field the paragraph above `as_str` says does
    /// not exist could be added without this census seeing it — while its own
    /// message goes on asserting what the inherent surface *is*. Rust's
    /// function prefix is a closed grammar, so this strips it rather than
    /// matching one spelling of it.
    fn a_public_method(line: &str) -> Option<&str> {
        let mut rest = line.trim_start().strip_prefix("pub")?;
        rest = match rest.strip_prefix('(') {
            Some(inside) => inside[inside.find(')')? + 1..].trim_start(),
            None => rest.trim_start(),
        };
        for qualifier in ["const ", "async ", "unsafe ", "safe "] {
            if let Some(after) = rest.strip_prefix(qualifier) {
                rest = after.trim_start();
            }
        }
        if let Some(after) = rest.strip_prefix("extern ") {
            rest = after.trim_start();
            if let Some(abi) = rest.strip_prefix('"') {
                rest = abi[abi.find('"')? + 1..].trim_start();
            }
        }
        rest.strip_prefix("fn ")?.split(['(', '<']).next()
    }

    /// The trait this line implements for the identifier, however it is written.
    ///
    /// **A generic impl begins `impl<` and an impl on a reference
    /// ends `for &'a OpaqueId {`.** Requiring the line to begin exactly `impl `
    /// and end exactly ` for OpaqueId {` sees neither —
    /// and `impl<'a> IntoIterator for &'a OpaqueId` with `type Item = u8` is a
    /// byte-by-byte reader of the identifier, precisely the accessor the
    /// paragraph above `as_str` promises does not exist.
    fn a_trait_on_the_identifier(line: &str) -> Option<String> {
        let rest = line.trim_start().strip_prefix("impl")?;
        // `impl<'a, T>` — the generics belong to the impl, not to the trait.
        let rest = match rest.strip_prefix('<') {
            Some(inside) => &inside[inside.find('>')? + 1..],
            None => rest,
        };
        // **A `where` clause is not part of the receiver.** Joining a wrapped
        // header hands this everything up to the opening brace, and rustfmt
        // puts `where Self: Sized,` in there — so `rsplit_once(" for ")` alone
        // compares against `OpaqueId where Self: Sized, {` and the impl becomes
        // invisible. The join widens the rule on one axis and would narrow it
        // on the one beside it, and the narrow spelling is the one the
        // formatter produces.
        let rest = rest.split_once(" where ").map_or(rest, |(head, _)| head);
        let (named, implemented_for) = rest.rsplit_once(" for ")?;
        // **A receiver is a reference, a lifetime, and a mutability, in any
        // combination.** Written for exactly `&'a OpaqueId` — the line its own
        // doc names as the byte-by-byte reader `as_str`'s paragraph promises
        // does not exist — the rule lets `&'a mut OpaqueId` walk straight
        // through it one spelling along unless `mut` is stripped. Measured as a
        // pure function that way: `&'a OpaqueId` gives the trait,
        // `&'a mut OpaqueId` gives nothing, and a real `IntoIterator` yielding
        // `u8` stands in the crate with the census reporting the surface
        // unchanged. Peeled in a loop, so the order cannot matter. **And `&` is
        // not Rust's only fundamental wrapper.** `Box` is the other, so
        // `impl <ForeignTrait> for Box<OpaqueId>` is legal in this crate by the
        // same orphan rule that makes `&'a OpaqueId` legal, and it is reachable
        // as `Box::new(id).into_iter()`. `Pin` peels beside it, and the path
        // qualifier is dropped first because `std::pin::Pin<Box<…>>` is how the
        // same impl is spelled one step along.
        let peeled = implemented_for.trim().trim_end_matches('{').trim();
        let mut receiver = peeled;
        loop {
            let shorter = receiver
                .strip_prefix('&')
                .map(str::trim_start)
                .or_else(|| receiver.strip_prefix("mut ").map(str::trim_start))
                .or_else(|| {
                    receiver
                        .strip_prefix('\'')
                        .and_then(|tail| tail.split_once(' '))
                        .map(|(_, tail)| tail.trim_start())
                })
                .or_else(|| {
                    receiver
                        .rsplit("::")
                        .next()
                        .filter(|last| *last != receiver)
                })
                .or_else(|| {
                    receiver
                        .strip_prefix("Box<")
                        .or_else(|| receiver.strip_prefix("Pin<"))
                        .and_then(|inside| inside.strip_suffix('>'))
                        .map(str::trim)
                });
            match shorter {
                Some(rest) if rest != receiver => receiver = rest,
                _ => break,
            }
        }
        // **And the type on the other side of the ` for ` is not this
        // question.** Reading the trait path too, so that
        // `impl From<&OpaqueId> for Vec<u8>` would be reported, needs a rule
        // measured **too strong on the tree as it stands**:
        // `impl Extend<OpaqueId> for Bag`, a container that reads the inside of
        // no identifier, turns the suite red, and the widened receiver goes on
        // to read this file's own fixture literals as source. Each widening of
        // this reader is what the next spelling beats.
        //
        // So it is not asked to decide whether an impl is an *accessor* from
        // its text, and asks the one question text can answer: **is the self
        // type `OpaqueId`**. What a self type of something else can hide is put
        // to the compiler in `no_wrapper_and_no_alias_reads_the_identifier`,
        // whose bounds are a written list and are the contract. What neither
        // holds is written there, plainly, and is not claimed as covered.
        (receiver.trim_end_matches(['{', '}', ' ']).trim() == "OpaqueId")
            .then(|| named.trim().to_owned())
    }

    /// This source's lines, with a wrapped `impl` header joined into one.
    ///
    /// **The formatter this repository enforces manufactures the blind
    /// spot.** `a_trait_on_the_identifier` reads an impl whose header fits on
    /// one line: it asks `rsplit_once(" for ")` of the same line it
    /// found `impl` on. Past a hundred columns rustfmt wraps that header
    /// onto `impl <Trait>` / `    for OpaqueId` / `{` and calls that canonical
    /// — so without the join a byte-by-byte reader written with a long trait
    /// path is invisible to the census *and* clean under `cargo fmt --check`,
    /// and writing it on one line and running this tree's own `cargo fmt`
    /// produces the wrapped form. A receiver widened while the line boundary
    /// stays where it is would be one rule widened on one axis and not on the
    /// one beside it.
    fn with_the_impl_headers_joined(source: &str) -> (Vec<String>, usize) {
        let mut out: Vec<String> = Vec::new();
        let mut carried: Option<String> = None;
        let mut refused = 0_usize;
        for line in source.lines() {
            let trimmed = line.trim();
            // **A header carrying a comment is refused, not parsed.** Reading
            // one correctly needs Rust's literal grammar — escapes, raw strings
            // by hash count, byte strings, and the `'` that is a char rather
            // than a lifetime — and a stripper without it lets a `/*` inside a
            // *string literal* open a comment that runs to the end of the file;
            // this crate's own `lib.rs` carries such a literal, so four lines
            // of real code below it would be read as a comment. One ordinary
            // `const OPENER: &str = "/*";` above any impl would hide a
            // byte-by-byte reader of the identifier, with `cargo fmt --check`
            // clean.
            //
            // Neither way of keeping the parse holds, measured: a
            // per-line reset of the depth loses a legitimate wrapped header,
            // and a quote toggle walks through `"\"/*"` and through a balanced
            // pair — which is exactly what `lib.rs` already holds. So this does
            // what the rest of this tree does with something it cannot judge:
            // it says so. The count is held below, so a header written this way
            // is a red tree rather than a silence, and the guarantee itself
            // sits on the compiler probe.
            if (carried.is_some() || trimmed.starts_with("impl")) && a_comment_is_in(trimmed) {
                carried = None;
                refused += 1;
                continue;
            }
            let ends = the_item_is_finished_on(trimmed);
            if let Some(mut header) = carried.take() {
                header.push(' ');
                header.push_str(trimmed);
                if ends {
                    out.push(header);
                } else {
                    carried = Some(header);
                }
            } else if trimmed.starts_with("impl") && !ends {
                carried = Some(trimmed.to_owned());
            } else {
                out.push(trimmed.to_owned());
            }
        }
        out.extend(carried);
        (out, refused)
    }

    /// Whether a comment begins, ends or sits anywhere on this line.
    ///
    /// Deliberately the widest possible reading — a `//` inside a string is a
    /// comment to this — because it decides what is **refused**, and refusing a
    /// line that did not need it costs a number moving, while reading one that
    /// did costs a reader of the identifier nobody can see.
    fn a_comment_is_in(line: &str) -> bool {
        line.contains("//") || line.contains("/*") || line.contains("*/")
    }

    /// Whether the item this line belongs to is finished on it.
    ///
    /// **The question is the braces, not the last character.** Asked
    /// as `ends_with('{') || ends_with(';')`, an empty-body impl
    /// — `impl Trait for Type {}`, of which this crate carries five — is taken
    /// for an unfinished wrapped header and has the next header glued onto it,
    /// so neither is read as itself and the census reports a trait whose name
    /// is two headers.
    ///
    /// A suffix test is the wrong shape one spelling along as well: rustfmt
    /// leaves a trailing `// comment` alone, so `impl Marker for E {} // note`
    /// ends with neither brace nor semicolon. The comment is dropped and the
    /// opening brace is looked for anywhere on what is left.
    fn the_item_is_finished_on(line: &str) -> bool {
        let letters: Vec<char> = line.chars().collect();
        let mut angle = 0_usize;
        let mut brace = 0_usize;
        let mut opens = false;
        let mut at = 0;
        while at < letters.len() {
            let two: String = letters[at..(at + 2).min(letters.len())].iter().collect();
            // **An arrow is not a closing angle.** Read as one,
            // `impl T<fn() -> u8, { 4 * 2 }>` drops the depth to zero on the
            // `->`, so the const block's brace is taken for the body's and the
            // wrap below is never joined.
            if two == "->" {
                at += 2;
                continue;
            }
            match letters[at] {
                // **And a `<` inside a const block is a less-than.** The depth
                // is read outside the braces only, so `impl T<{ 1 < 2 }>` leaves
                // it where it was rather than one too high at the real brace.
                '<' if brace == 0 => angle += 1,
                '>' if brace == 0 => angle = angle.saturating_sub(1),
                '{' => {
                    opens |= angle == 0 && brace == 0;
                    brace += 1;
                }
                '}' => brace = brace.saturating_sub(1),
                _ => {}
            }
            at += 1;
        }
        opens
    }

    /// The ways in are two, and the paragraph above the type names them.
    ///
    /// **A guarantee stated no wider than it holds.** Saying this type "offers
    /// no way to read a byte" while `as_str` hands out the whole string
    /// — `id.as_str().as_bytes()[0]` compiles — would make the sentence a
    /// reader relies on false, with nothing to say so. The narrower claim is
    /// the checkable one: there is one accessor, it returns the value
    /// whole, `Display` prints the same string, and there is no accessor shaped
    /// like a prefix, a byte or a version field. This pins that, so
    /// an `as_bytes`, a `prefix`, an `AsRef<str>` or a `Deref` added later
    /// fails here rather than making the paragraph quietly wrong.
    ///
    /// **What this does not judge**: whether a caller derives structure from the
    /// string it is handed. Nothing in a library can stop that; what a library
    /// can do is not offer a shortcut, and that is what is asserted.
    ///
    /// The source is read from the first `impl OpaqueId {` onwards, which is the
    /// real one — this module is below it, and the spelling appears here only
    /// inside a string.
    #[test]
    fn the_only_readers_are_the_two_the_documents_name() {
        // **Five files, all read.** `OpaqueId` is `pub` at the crate root, so
        // `base32.rs`, `flake.rs`, `naming.rs` and `snowflake.rs` may each add
        // an inherent accessor or a trait impl to it — and a census whose
        // message asserts what the inherent surface *is* cannot look only at
        // `lib.rs`. A first-occurrence `find` is the same hole one step nearer:
        // a second `impl OpaqueId` block below the first would be invisible. So
        // the crate's whole `src/` is read, and every block in it, not the
        // first.
        let mut methods: Vec<String> = Vec::new();
        let mut traits: Vec<String> = Vec::new();
        let directory = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"));
        let mut read = 0_usize;
        // **What the reader would not guess at**, held so a header written with
        // a comment is a red tree rather than a silence. See the paragraph in
        // `with_the_impl_headers_joined`.
        let mut refused = 0_usize;
        for entry in std::fs::read_dir(directory).expect("this crate has sources") {
            let path = entry.expect("a source is readable").path();
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("a source is readable");
            read += 1;
            let mut from = 0;
            while let Some(at) = source[from..].find("impl OpaqueId {") {
                let at = from + at;
                let open = source[at..].find('{').expect("the block opens") + at;
                let mut depth = 0_i32;
                let mut end = open;
                for (offset, letter) in source[open..].char_indices() {
                    match letter {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + offset;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                methods.extend(
                    source[open..end]
                        .lines()
                        .filter_map(|line| a_public_method(line))
                        .map(str::to_owned),
                );
                from = end.max(at + 1);
            }
            traits.extend(
                the_readable_impl_headers(&source, &mut refused)
                    .iter()
                    .filter_map(|line| a_trait_on_the_identifier(line)),
            );
        }
        assert!(
            read >= 5,
            "{read} source(s) were read out of this crate's `src/`, and it has five. A \
             census that reads fewer files than the crate holds agrees with a surface it \
             never looked at"
        );
        assert_eq!(
            refused, REFUSED_AS_UNREADABLE,
            "this crate's sources carry {refused} impl header(s) with a comment on them and \
             {REFUSED_AS_UNREADABLE} is what is written down. The census refuses such a \
             header rather than guessing at it, so one arriving is a trait impl nobody \
             here can see — write the impl without the comment, or move the comment above \
             it"
        );
        methods.sort();
        traits.sort();
        assert_eq!(
            methods,
            ["as_str", "new", "of_device_key"],
            "`OpaqueId`'s inherent surface is not the reader and the two constructors \
             the crate root and `as_str` describe. A second reader is a second thing a \
             later stage can come to depend on before `NOCTUA.md §5.1` is answered, and \
             the paragraph above the type would go on saying there is one"
        );
        assert_eq!(
            traits,
            ["core::fmt::Display"],
            "`OpaqueId` implements a trait the documents do not account for. An \
             `AsRef<str>`, a `Deref` or a `From` is another way to the string, and \
             `as_str`'s paragraph says which ways there are"
        );
    }

    /// `OpaqueId` implements no ordering, and this is what says so.
    ///
    /// **More than the paragraph above the type.** Without
    /// this, `PartialOrd, Ord` on the derive list leaves the entire gate
    /// at `check passed` — and with `Ord`, `BTreeMap<OpaqueId, _>`
    /// and `.sort()` compile. For a ULID or a `UUIDv7` that byte order **is**
    /// time order, so the thing `NOCTUA.md §5.1` is `[OPEN]` to prevent —
    /// something built now depending on a strong identifier's structure —
    /// arrives as an innocuous line in a later stage.
    ///
    /// **How this asks the compiler rather than the source text.** An inherent
    /// associated constant is preferred over a trait's, but only when its
    /// bounds hold; when they do not, the blanket trait impl answers instead.
    /// So `Probe::<T>::ORDERED` is `true` exactly when `T: Ord`. The control
    /// below is what keeps this from being a test that cannot fail: `String`
    /// is ordered, and if the probe stopped working it would report `false`
    /// for that too and this test would still pass on the line that matters.
    #[test]
    #[expect(
        clippy::assertions_on_constants,
        reason = "the constant is the measurement: `Probe::<T>::ORDERED` is resolved by \
                  the compiler from whether `T: Ord` holds, so an assertion that is \
                  constant at run time is one that was decided at compile time — which \
                  is the only place this question can be asked"
    )]
    fn an_opaque_identifier_cannot_be_ordered() {
        /// The type the question is asked about, so `T` has somewhere to sit.
        struct Probe<T>(std::marker::PhantomData<T>);

        /// The answer for a type that is not ordered.
        trait Unordered {
            /// Reached only when the inherent constant's bound does not hold.
            const ORDERED: bool = false;
        }

        impl<T> Unordered for Probe<T> {}

        impl<T: Ord> Probe<T> {
            /// The answer for a type that is ordered, preferred when it applies.
            const ORDERED: bool = true;
        }

        assert!(
            <Probe<String>>::ORDERED,
            "the probe cannot tell an ordered type from an unordered one, so what it \
             says about `OpaqueId` means nothing"
        );
        assert!(
            !<Probe<OpaqueId>>::ORDERED,
            "`OpaqueId` implements an ordering again. `NOCTUA.md §5.1` is `[OPEN]`, and a \
             derived `Ord` over the bytes is how a `BTreeMap` or a `sort()` comes to \
             depend on the shape this repository has not chosen — see the paragraph \
             above the type"
        );
    }

    /// The spellings of a byte reader this census has been beaten by.
    ///
    /// Each row is an impl that, written into the crate, a weaker census
    /// reports the surface unchanged over. They are read as text rather than
    /// compiled, because compiling one would be adding the accessor the crate
    /// promises not to have — and the assertion is that the census sees *one*
    /// trait in each, not that it sees a particular name, so the row for the
    /// wrapped header does not have to spell a hundred columns of trait path
    /// twice.
    #[test]
    fn the_spellings_of_a_reader_this_census_has_been_beaten_by() {
        for (what, source, reads) in the_spellings_this_census_has_been_beaten_by() {
            // **The name, not the count.** Asked as `found.len() == 1`, the two
            // rows about a swallowed header pass with the joiner broken: the
            // glued line still ends `for OpaqueId {`, so the census answers one
            // trait — whose name is two headers run together. A fixture that
            // counts what it should be reading is the shape this file's own
            // census refuses.
            let mut refused = 0_usize;
            let found: Vec<String> = the_readable_impl_headers(source, &mut refused)
                .iter()
                .filter_map(|line| a_trait_on_the_identifier(line))
                .collect();
            assert_eq!(
                found,
                [reads.to_owned()],
                "{what}: the census read {found:?} out of this impl, so a byte-by-byte \
                 reader written this way stands in the crate under a name nobody wrote"
            );
        }
    }

    /// The impls this census has been beaten by, and what it must read out of
    /// each.
    ///
    /// Its own function because inline the table would cross clippy's
    /// hundred-line bound. Each row is a spelling that, written into the crate,
    /// a weaker census misses, and the expectation is the trait's **name**
    /// rather than a count — asked as a count, two of these rows pass with the
    /// reader broken.
    /// <!-- allow-logging: `the_spellings_this_census_has_been_beaten_by` hands back fixture impls written as literals in this file, and nothing a caller chose -->
    fn the_spellings_this_census_has_been_beaten_by()
    -> [(&'static str, &'static str, &'static str); 11] {
        [
            (
                "a header rustfmt wrapped, which is what it produces past a hundred columns",
                "impl ReadsEveryByteOfAnIdentifier\n    for OpaqueId\n{\n",
                "ReadsEveryByteOfAnIdentifier",
            ),
            (
                "a mutable reference",
                "impl<'a> IntoIterator for &'a mut OpaqueId {\n",
                "IntoIterator",
            ),
            (
                "one of Rust's two fundamental wrappers",
                "impl IntoIterator for Box<OpaqueId> {\n",
                "IntoIterator",
            ),
            (
                "the same wrapper written as a qualified path",
                "impl IntoIterator for std::pin::Pin<Box<OpaqueId>> {\n",
                "IntoIterator",
            ),
            (
                "a header rustfmt wrapped around a `where` clause",
                "impl core::borrow::Borrow<str> for OpaqueId\nwhere\n    Self: Sized,\n{\n",
                "core::borrow::Borrow<str>",
            ),
            (
                "an empty-body impl above the header it used to swallow",
                "impl Marker for OpaqueIdError {}\nimpl core::fmt::Display for OpaqueId {\n",
                "core::fmt::Display",
            ),
            (
                "a const generic argument written as a block, which rustfmt wraps",
                "impl ReadsEveryByte<{ 4 * 2 }>\n    for OpaqueId\n{\n",
                "ReadsEveryByte<{ 4 * 2 }>",
            ),
            (
                "an empty body on the identifier, which is what a defaulted method arrives as",
                "impl ReadsEveryByte for OpaqueId {}\n",
                "ReadsEveryByte",
            ),
            (
                "a function pointer beside a const block, whose arrow is not a closing angle",
                "impl ReadsEveryByte<fn() -> u8, { 4 * 2 }>\n    for OpaqueId\n{\n",
                "ReadsEveryByte<fn() -> u8, { 4 * 2 }>",
            ),
            (
                "a greater-than inside a const block, which is not a closing angle",
                "impl ReadsEveryByte<{ 2 > 1 }, { 3 }>\n    for OpaqueId\n{\n",
                "ReadsEveryByte<{ 2 > 1 }, { 3 }>",
            ),
            (
                "a less-than inside a const block, which is not an opening angle",
                "impl ReadsEveryByte<{ 1 < 2 }> for OpaqueId {\n    fn read(&self) -> u8 {\n",
                "ReadsEveryByte<{ 1 < 2 }>",
            ),
        ]
    }

    /// A header carrying a comment is refused, and the refusal is counted.
    ///
    /// **These are spellings a parser would need Rust's literal grammar
    /// for.** Each is a real impl that rustfmt keeps verbatim, and a stripper
    /// without that grammar lets a `/*` inside a string literal open a comment
    /// running to the end of the file — as it would in this crate's
    /// own `lib.rs`. The reader refuses them. What makes a refusal a gate
    /// rather than a hole is the count beside it: the census holds how many its
    /// sources carry, so one arriving is a red tree.
    ///
    /// **What this does not judge**: whether such a header would have been an
    /// accessor. It judges that the census does not pretend to know.
    #[test]
    fn a_header_carrying_a_comment_is_refused_rather_than_guessed_at() {
        for (what, source, still_read) in [
            (
                "an empty body with the trailing comment rustfmt leaves alone",
                "impl ReadsEveryByte for OpaqueId {} // a note\n",
                &[][..],
            ),
            (
                "a block comment on the receiver's side of the ` for `",
                "impl ReadsEveryByte for /* the identifier */ OpaqueId {\n",
                &[][..],
            ),
            (
                "a line comment holding a brace, with the ` for ` on the next line",
                "impl ReadsEveryByte // a note with a { in it\n    for OpaqueId\n{\n",
                &[][..],
            ),
            (
                "a block comment naming a path, on the receiver's side",
                "impl IntoIterator for OpaqueId /* one per core::primitive::u8 */ {\n",
                &[][..],
            ),
            (
                "a block comment spanning the wrap, with a brace in its continuation",
                "impl ReadsEveryByte /* a note\n   with a { in it */\n    for OpaqueId\n{\n",
                &[][..],
            ),
            (
                "a header carrying a block comment with a URL in it",
                "impl core::fmt::Display /* https://doc.rust-lang.org */ for OpaqueId {\n\
                     fn fmt(&self, to: &mut core::fmt::Formatter) -> core::fmt::Result {\n",
                &[][..],
            ),
            (
                "the same, with the trailing comment rustfmt leaves alone",
                "impl Marker for OpaqueIdError {} // nothing in it, on purpose\n\
                 impl core::fmt::Display for OpaqueId {\n",
                // The refused line is refused and the clean header under it is
                // still read, which is the whole point of refusing a line rather
                // than giving up on the file.
                &["core::fmt::Display"][..],
            ),
            (
                "a comment on the continuation of a wrapped header",
                "impl ReadsEveryByte\n    for /* the identifier */ OpaqueId\n{\n",
                &[][..],
            ),
            (
                "a refused continuation drops the header it was continuing",
                "impl ReadsEveryByte\n    for /* the identifier */ OpaqueId\n\
                 impl core::fmt::Display for OpaqueId {\n",
                &["core::fmt::Display"][..],
            ),
            (
                "an unterminated block comment, which closes on no line at all",
                "impl ReadsEveryByte for OpaqueId /* and it never closes\n{\n",
                &[][..],
            ),
        ] {
            let mut refused = 0_usize;
            let found: Vec<String> = the_readable_impl_headers(source, &mut refused)
                .iter()
                .filter_map(|line| a_trait_on_the_identifier(line))
                .collect();
            let read: Vec<String> = still_read.iter().map(|&name| name.to_owned()).collect();
            assert_eq!(
                (found.as_slice(), refused),
                (read.as_slice(), 1),
                "{what}: the census read {found:?} out of a header it cannot lex, or did \
                 not count the refusal — and a refusal nobody counts is the silence this \
                 replaced"
            );
        }
    }

    /// The lines of this source, with a wrapped `impl` header joined into one.
    ///
    /// One name for the pair, because the count is not optional: a caller that
    /// took the lines and dropped the refusals would be the silence again.
    fn the_readable_impl_headers(source: &str, refused: &mut usize) -> Vec<String> {
        let (lines, unread) = with_the_impl_headers_joined(source);
        *refused += unread;
        lines
    }

    /// How many impl headers in this crate's sources carry a comment.
    ///
    /// Zero, and it is written down rather than asserted as a property: a
    /// header written with a comment on it is legal Rust that rustfmt keeps, so
    /// this is a number to move in the edit that writes one — and moving it is
    /// the moment to ask whether the impl below it is an accessor.
    const REFUSED_AS_UNREADABLE: usize = 0;

    /// No wrapper and no alias hands out a byte or a string of the identifier.
    ///
    /// **A text census cannot undo an alias.** `pub type Identifier = OpaqueId;`
    /// beside `impl IntoIterator for &Identifier` is a real byte-by-byte reader
    /// that no spelling of the source walk above can see, because the source
    /// says `Identifier` and the walk compares against `OpaqueId`; the same goes
    /// for a receiver written as a qualified path. The compiler resolves both,
    /// so the question is asked there — the way
    /// [`an_opaque_identifier_cannot_be_ordered`] asks its own.
    ///
    /// The bounds are the shapes `as_str`'s paragraph promises do
    /// not exist: something to walk the bytes with, something that lends the
    /// string out, and something that derefs to it.
    ///
    /// **What this does not judge**: a trait nobody named here. The census
    /// above is what notices an unfamiliar one; this is what notices a familiar
    /// one written so the census cannot see it.
    #[test]
    #[expect(
        clippy::assertions_on_constants,
        reason = "each constant is the measurement: it is resolved by the compiler from \
                  whether the bound holds, so an assertion that is constant at run time \
                  is one that was decided at compile time — which is the only place an \
                  alias or a wrapper can be seen through"
    )]
    fn no_wrapper_and_no_alias_reads_the_identifier() {
        // **The controls, so this cannot be a test that passes because the
        // probe stopped working.** Each is a type that really does offer the
        // way in, and each must answer `true` on the same mechanism.
        assert!(
            <Probe<Vec<u8>>>::WALKS,
            "the probe cannot see a walkable type"
        );
        assert!(<Probe<String>>::LENDS, "the probe cannot see a lent string");
        assert!(<Probe<String>>::DEREFS, "the probe cannot see a deref");
        assert!(
            <Probe<String>>::BORROWS,
            "the probe cannot see a borrowed string"
        );
        assert!(<Probe<&str>>::CONVERTS, "the probe cannot see a conversion");
        assert!(
            <Probe<String>>::INDEXES,
            "the probe cannot see an indexable type"
        );
        assert!(
            <Probe<String>>::BYTES,
            "the probe cannot see a conversion into bytes"
        );

        assert!(
            !<Probe<OpaqueId>>::WALKS
                && !<Probe<&OpaqueId>>::WALKS
                && !<Probe<&mut OpaqueId>>::WALKS
                && !<Probe<Box<OpaqueId>>>::WALKS,
            "a byte-by-byte reader of `OpaqueId` is reachable — through the type, a \
             reference to it, or one of Rust's fundamental wrappers around it. \
             `as_str`'s paragraph says there is no accessor shaped like a byte, and \
             `NOCTUA.md §5.1` is `[OPEN]`"
        );
        // **Every bound is asked of the three forms a caller already has**, and
        // the one that is not is the one that cannot be.
        //
        // **The cells are enumerated, and the enumeration is the contract.**
        // The bounds are a hand-written cross-product of (bound x wrapper), and
        // a cross-product has no last cell: with `&mut` asked of `WALKS` alone,
        // `impl From<&mut OpaqueId> for Vec<u8>` — a byte-by-byte reader, whose
        // self type is `Vec<u8>` so the census cannot see it either — stands in
        // the crate with nothing red.
        //
        // **Removing cells loses real coverage.** `Box` on `WALKS` is the only
        // cell asking about a `Box` at all, so without it
        // `impl IntoIterator for Box<Identifier>` — an alias inside one — is
        // reachable with nothing red: the census compares its peeled receiver
        // against the literal `OpaqueId`. The argument that `T`, `&T` and
        // `&mut T` are "what a caller holds without constructing anything" does
        // not survive contact: `&mut id` needs a `mut` binding while
        // `(&&id).into_iter()` needs neither a binding nor a constructor, and
        // escapes anyway. A principle that sounds defensible is worse than an
        // enumeration that admits what it is.
        //
        // So the cells are written out and `Box` is on the six bounds that can
        // hold it. It is **not** on `DEREFS`, and that is measured rather than
        // assumed: `Box<T>: Deref` holds for every `T` in Rust, so asking it
        // there refuses a tree with nothing wrong with it — the same reason
        // written down for `&T`.
        //
        // **What this does not ask**, as a list rather than as a principle:
        // `Rc<OpaqueId>`, `&&OpaqueId`, `Pin<Box<OpaqueId>>`, and an alias
        // inside any of them. Each is reachable and each is outside this, and
        // the next cell anyone needs is added here beside its measurement.
        // `BORROWS` is asked of the reference too, because `core`'s three
        // blanket impls leave `impl Borrow<str> for &OpaqueId` perfectly
        // writable, and an alias would hide it from the text census as well.
        // `&T` derefs for every `T`, so asking `DEREFS` of a reference is a
        // question whose answer is yes for every type in Rust and would refuse
        // a tree with nothing wrong with it. `BYTES` and `CONVERTS` are both
        // asked of the type *and* the reference, because the conversion can be
        // written on either: asked of the type alone,
        // `impl From<&OpaqueId> for String` is a second way to the whole string
        // that neither half sees.
        assert!(
            !<Probe<OpaqueId>>::LENDS
                && !<Probe<&OpaqueId>>::LENDS
                && !<Probe<&mut OpaqueId>>::LENDS
                && !<Probe<Box<OpaqueId>>>::LENDS,
            "`OpaqueId` lends its string out through `AsRef`, which is a second way in \
             beside `as_str` and the one `as_str`'s paragraph names first"
        );
        assert!(
            // Asked of the type only: see the paragraph above the lending bound.
            !<Probe<OpaqueId>>::DEREFS,
            "`OpaqueId` derefs, so every method of what it derefs to is on it — which is \
             the widest way in of the three and the hardest to notice"
        );
        // **And the three the text census cannot ask about at all.** Its whole
        // question is what stands after the last ` for `, so it can only see an
        // impl whose *self type* is `OpaqueId` — and
        // `impl From<OpaqueId> for String`, which is a way to the string, has
        // `String` there. Widening the text rule to any line mentioning the
        // type is measured too strong: it reports the crate's own
        // `OpaqueIdError` impls, because that name contains this one. A
        // `Borrow<str>` is the same shape as the `AsRef` above, and a range
        // taken out of the type is the prefix accessor `as_str`'s paragraph
        // names first.
        assert!(
            !<Probe<OpaqueId>>::BORROWS
                && !<Probe<&OpaqueId>>::BORROWS
                && !<Probe<&mut OpaqueId>>::BORROWS
                && !<Probe<Box<OpaqueId>>>::BORROWS,
            "`OpaqueId` borrows as its string, which is a second way in beside `as_str` \
             and the one a `HashMap<OpaqueId, _>` lookup by `&str` would use"
        );
        assert!(
            !<Probe<OpaqueId>>::CONVERTS
                && !<Probe<&OpaqueId>>::CONVERTS
                && !<Probe<&mut OpaqueId>>::CONVERTS
                && !<Probe<Box<OpaqueId>>>::CONVERTS,
            "`OpaqueId` converts into a `String`, so `impl From<OpaqueId> for String` \
             stands in the crate — the one direction of `From` the census above cannot \
             see, because the type it names is on the wrong side of its ` for `"
        );
        assert!(
            !<Probe<OpaqueId>>::INDEXES
                && !<Probe<&OpaqueId>>::INDEXES
                && !<Probe<&mut OpaqueId>>::INDEXES
                && !<Probe<Box<OpaqueId>>>::INDEXES,
            "a range can be taken out of `OpaqueId`, which is the accessor shaped like a \
             prefix that `as_str`'s paragraph says does not exist"
        );
        // **The one the written residual names.**
        // `impl From<&Identifier> for Vec<u8>` is where the text rule gives
        // out, and the probe is where that is asked: `Into<Vec<u8>>`, of the
        // type and of the reference, because the conversion can be written on
        // either.
        assert!(
            !<Probe<OpaqueId>>::BYTES
                && !<Probe<&OpaqueId>>::BYTES
                && !<Probe<&mut OpaqueId>>::BYTES
                && !<Probe<Box<OpaqueId>>>::BYTES,
            "`OpaqueId` converts into its bytes, which is the byte-by-byte reader \
             `as_str`'s paragraph says does not exist — and it is written with the \
             identifier on the far side of the ` for `, where no text rule here looks"
        );
    }

    /// The type each question is asked about, so `T` has somewhere to sit.
    struct Probe<T>(std::marker::PhantomData<T>);

    /// The answers for a type that offers none of the ways in.
    trait NoWayIn {
        /// Reached only when the walking bound does not hold.
        const WALKS: bool = false;
        /// Reached only when the lending bound does not hold.
        const LENDS: bool = false;
        /// Reached only when the deref bound does not hold.
        const DEREFS: bool = false;
        /// Reached only when the borrowing bound does not hold.
        const BORROWS: bool = false;
        /// Reached only when the conversion bound does not hold.
        const CONVERTS: bool = false;
        /// Reached only when the indexing bound does not hold.
        const INDEXES: bool = false;
        /// Reached only when the byte-conversion bound does not hold.
        const BYTES: bool = false;
    }

    impl<T> NoWayIn for Probe<T> {}

    impl<T: IntoIterator> Probe<T> {
        /// The answer for a type whose items can be walked.
        const WALKS: bool = true;
    }

    impl<T: AsRef<str>> Probe<T> {
        /// The answer for a type that lends its string out.
        const LENDS: bool = true;
    }

    impl<T: std::ops::Deref> Probe<T> {
        /// The answer for a type that derefs to something else.
        const DEREFS: bool = true;
    }

    impl<T: core::borrow::Borrow<str>> Probe<T> {
        /// The answer for a type that borrows as its string.
        const BORROWS: bool = true;
    }

    impl<T: Into<String>> Probe<T> {
        /// The answer for a type that converts into a string.
        const CONVERTS: bool = true;
    }

    impl<T: core::ops::Index<core::ops::Range<usize>>> Probe<T> {
        /// The answer for a type a range can be taken out of.
        const INDEXES: bool = true;
    }

    impl<T: Into<Vec<u8>>> Probe<T> {
        /// The answer for a type that converts into its bytes.
        const BYTES: bool = true;
    }
}
