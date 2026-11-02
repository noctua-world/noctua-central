//! The machine flake: ten bits that say which computer minted an identifier.
//!
//! `NOCTUA.md §16.2`. The hard part of a Snowflake is not the layout, it is
//! getting a machine identifier of bounded width that is unique **without a
//! coordinator** — and the device already has exactly one strong, unique
//! cryptographic identity, its iroh ed25519 key (`§13.2`, `§14.5`). Deriving the
//! flake from it means the flake is deterministic, reproducible after a
//! reinstall on the same device, and needs no registry to allocate.
//!
//! **Ten bits collide, and that is expected rather than a flaw.** A thousand and
//! twenty-four values cannot be unique across arbitrarily many devices, so the
//! flake is registered on the `Device` entity at pairing time and the realm
//! rejects one it already holds for a different device. The device then tries the
//! next candidate in a deterministic sequence. Collisions are therefore resolved,
//! recorded, and never silent.
//!
//! **What is deliberately not here.** Pairing, the `Device` entity and the realm
//! that answers "is this taken?" are a later stage. What exists now is the
//! derivation, the candidate sequence, and the rule that a flake is never
//! regenerated on a device that already has one — because that rule has to be
//! true before anything mints, not after.

use nc_catalog::ErrorCode;

/// How many bits the flake occupies inside an identifier.
pub const BITS: u32 = 10;

/// One more than the largest flake.
pub const RANGE: u16 = 1 << BITS;

/// How many candidates the sequence offers before giving up.
///
/// The bound exists so that a device on a misconfigured realm fails loudly
/// instead of hashing forever. Sixty-four draws from a thousand and twenty-four
/// values are, by the birthday bound, about two repeats — the sequence is a
/// sequence of *hashes*, not a permutation, so a candidate may recur and the
/// caller simply tries the next. A realm that rejected all sixty-four holds
/// hundreds of paired devices, which is far past anything this platform is for.
pub const CANDIDATES: u32 = 64;

/// A machine-flake value that does not fit the layout.
///
/// **Sealed, because an error that names a legal value is a lie a log keeps.**
/// The field was `pub` with no invariant — the only public struct field in this
/// crate — so `FlakeOutOfRange { value: 5 }` rendered *"a machine flake of 5 is
/// outside 0..1024"* under `NC-VAL-0001`, about a value `from_value` accepts.
/// The constructor is `from_value`'s refusal; `value()` reads it back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct FlakeOutOfRange {
    /// The value that was handed in.
    /// Read it back with [`FlakeOutOfRange::value_given`]; the field itself is
    /// not public, so an error naming a legal value cannot be built.
    value: u16,
}

impl FlakeOutOfRange {
    /// The value that was refused.
    #[must_use]
    pub const fn value_given(&self) -> u16 {
        self.value
    }

    /// The registry code this failure carries.
    ///
    /// **Not `IdsNotAnIdentifier`, which is about text.** That code's
    /// description is *"Text handed to an identifier type is not one"* and its
    /// message tells a person to check the value they typed — and `from_value`
    /// takes a `u16`. There is no text, and nobody typed it. `ValidationOutOfRange`
    /// says what this is: a number outside the limit the catalog declares.
    #[must_use]
    pub const fn code(self) -> nc_catalog::ErrorCode {
        nc_catalog::ErrorCode::ValidationOutOfRange
    }

    /// The bounds the code's sentence interpolates, ready for `render`.
    ///
    /// **`code()` alone left the sentence half-rendered.** `NC-VAL-0001` says
    /// *"{field} must be between {min} and {max}."* and this is the only place
    /// in the tree that raises it, so a caller holding the code had the braces
    /// and nowhere to get the numbers — the declaration was published, checked
    /// against both locales, and unreachable from the one failure that carries
    /// it.
    ///
    /// **`field` is deliberately not here.** `catalog/errors.toml` says it is a
    /// localised label and not a column name: this crate does not know the
    /// reader's locale, and a hardcoded `"machine flake"` would put an English
    /// noun inside a Portuguese sentence. The caller supplies it, and `render`
    /// refuses if they forget rather than showing `{field}`.
    #[must_use]
    pub const fn bounds(self) -> [(&'static str, &'static str); 2] {
        [("min", "0"), ("max", HIGHEST)]
    }

    /// The label and both bounds, which is everything `NC-VAL-0001` interpolates.
    ///
    /// The label is the caller's — see [`Self::bounds`] — so this takes it and
    /// returns what `nc_catalog::i18n::render` wants, in one call. It exists
    /// because the two-step version was the kind of seam a caller skips: the
    /// obvious next line after `code()` is to print `message()`, and that shows
    /// a reader `{field} must be between {min} and {max}.`
    #[must_use]
    pub const fn with_label(self, field: &str) -> [(&str, &str); 3] {
        [("field", field), ("min", "0"), ("max", HIGHEST)]
    }
}

/// The highest flake there is, spelled for the sentence that names it.
///
/// Written rather than formatted because `bounds` is `const`, and checked
/// against `RANGE` by a test — a spelled number that drifts from the layout is
/// the failure this whole tree writes commands instead of values to avoid.
const HIGHEST: &str = "1023";

impl core::fmt::Display for FlakeOutOfRange {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            formatter,
            "a machine flake of {} is outside 0..{RANGE}",
            self.value
        )
    }
}

impl core::error::Error for FlakeOutOfRange {}

/// Which machine an identifier came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MachineFlake(u16);

impl MachineFlake {
    /// The value, always below [`RANGE`].
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }

    /// A flake from a raw value.
    ///
    /// # Errors
    ///
    /// Returns [`FlakeOutOfRange`] when the value does not fit in [`BITS`] bits.
    /// It is an error rather than a mask: a caller that truncated silently would
    /// produce identifiers claiming to come from a machine that is not this one.
    ///
    /// **The error was a bare `u16`.** It carried no `Display`, no `Error` impl
    /// and no registry code, so the one thing a caller could do with it was
    /// print a number — `§15.1` makes an error's identity data, and a `u16` is
    /// data about the *value*, not about the failure.
    pub const fn from_value(value: u16) -> Result<Self, FlakeOutOfRange> {
        if value >= RANGE {
            return Err(FlakeOutOfRange { value });
        }
        Ok(Self(value))
    }

    /// The `attempt`-th candidate flake for a device key.
    ///
    /// Every candidate is BLAKE3 over a domain, the key and the attempt number,
    /// each length-prefixed — **candidate zero included**. The sequence is
    /// therefore deterministic: the same device walking the same sequence after
    /// a reinstall reaches the same flake it had.
    ///
    /// This said *"candidate zero is BLAKE3 of the key"*, which is the defect
    /// the body below records repairing: a bare hash of the key collides with
    /// any other bare hash of the same bytes anywhere in the platform, and
    /// `the_first_candidate_is_domain_separated_like_the_rest` asserts the
    /// opposite of this sentence twelve screens down. A public rustdoc is what a
    /// caller reads.
    #[must_use]
    pub fn candidate(device_key: &[u8], attempt: u32) -> Self {
        // **Length-prefixed, and on every attempt including the first.** An
        // earlier version wrote a fixed delimiter and called it a length prefix,
        // and it was not one: a device key ending in those very bytes followed
        // by a counter produced the same hasher input as a different key at a
        // different attempt, so two devices shared a candidate. Measured, with
        // the colliding pair in the tests below. Attempt zero was also a bare
        // BLAKE3 of the key, with no domain at all, so the flake seed collided
        // with any other bare hash of the same bytes anywhere in the platform.
        const DOMAIN: &str = "noctua-central/ids/machine-flake/v1";

        let mut hasher = blake3::Hasher::new();
        hasher.update(&(DOMAIN.len() as u64).to_le_bytes());
        hasher.update(DOMAIN.as_bytes());
        hasher.update(&(device_key.len() as u64).to_le_bytes());
        hasher.update(device_key);
        hasher.update(&attempt.to_le_bytes());
        let digest = hasher.finalize();
        let bytes = digest.as_bytes();
        let raw = u16::from_le_bytes([bytes[0], bytes[1]]);
        Self(raw % RANGE)
    }

    /// Every candidate, in order, for a device walking the sequence at pairing.
    pub fn candidates(device_key: &[u8]) -> impl Iterator<Item = Self> + '_ {
        (0..CANDIDATES).map(move |attempt| Self::candidate(device_key, attempt))
    }

    /// The code emitted when every candidate was taken.
    #[must_use]
    pub const fn exhausted() -> ErrorCode {
        ErrorCode::IdsFlakeExhausted
    }
}

impl core::fmt::Display for MachineFlake {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{CANDIDATES, MachineFlake, RANGE};

    const KEY: &[u8] = b"an ed25519 public key would be thirty-two bytes";
    const OTHER: &[u8] = b"a different device's ed25519 public key entirely";

    #[test]
    fn running_out_of_candidates_is_its_own_code() {
        // `exhausted()` was the only reference to `IdsFlakeExhausted`, with no
        // caller and no reader, so it could be repointed to anything. What it
        // means is specific: every candidate for this device collided with a
        // flake already in use, and `§16.2` says a flake is never regenerated on
        // an existing device — so the answer is not to try again.
        assert_eq!(MachineFlake::exhausted().area(), "IDS");
        assert_eq!(
            MachineFlake::exhausted(),
            nc_catalog::ErrorCode::IdsFlakeExhausted
        );
        // And it is not the same answer as a clock problem, which is the other
        // reason minting stops.
        assert_ne!(
            MachineFlake::exhausted(),
            nc_catalog::ErrorCode::IdsClockBackwards
        );
        // **The number itself, written out.** `CANDIDATES > 1` is a floor with
        // everything in it: `CANDIDATES = 10` left the whole `nc-ids` suite
        // green, and the effective floor was ten set by accident, by
        // `the_candidate_sequence_moves` calling `.take(10)`. Sixty-four is a
        // birthday-bound decision — its doc comment argues for that number and
        // not for "more than one" — so it is written here the way this
        // repository writes every mapping it refuses to derive.
        assert_eq!(
            CANDIDATES, 64,
            "CANDIDATES changed; the number is a birthday-bound decision its doc comment \
             argues for, so change that argument here too and say why"
        );

        // **And the sequence a device actually walks.** The fix above pinned
        // the *constant* and left the *iterator* free: `(0..CANDIDATES)` could
        // be written `(0..10)` with the whole crate green, because the only two
        // observations of `candidates()` are `.take(10)` and `.take(8)`. That
        // is the same accident one level down — a device pairing into a realm
        // with a hundred devices already in it would walk ten candidates,
        // report `IdsFlakeExhausted`, and be stuck, because `§16.2` says a flake
        // is never regenerated on an existing device.
        assert_eq!(
            MachineFlake::candidates(KEY).count(),
            64,
            "the sequence a device walks is not the length the constant claims"
        );
        // **And how many of them are distinct**, which is the number that
        // actually decides. A repeat is a candidate the realm has already
        // refused, so the budget is the distinct count and not the length: for
        // this key it is 60, and over 200,000 synthetic keys the worst was 54,
        // with 68 keys below 56 — the number this comment carried, measured over
        // two thousand. The doc's "about two repeats" is the mean, and the tail
        // is five times that — worth writing down where the number is claimed,
        // and worth re-measuring, because a tail seen over two thousand draws is
        // not the tail.
        let distinct: std::collections::BTreeSet<u16> = MachineFlake::candidates(KEY)
            .map(MachineFlake::value)
            .collect();
        assert!(
            distinct.len() >= 56,
            "only {} of 64 candidates are distinct for this key, which is worse than \
             any of two thousand keys measured",
            distinct.len()
        );
    }

    #[test]
    fn the_derivation_is_pinned_by_a_measured_vector() {
        // **Two tests here pass with the thing they are named for deleted.**
        // `the_first_candidate_is_domain_separated_like_the_rest` passes with
        // domain separation removed entirely — it only compares against a bare
        // `blake3(KEY)`, and the key's length prefix plus the attempt suffix
        // already make those differ, so the domain is never the reason. And
        // `a_key_that_looks_like_another_key_plus_a_counter_does_not_collide`
        // passes with the key length prefix removed: its forged pair is the one
        // that collided under the *old* delimiter scheme.
        //
        // With a fixed-width attempt suffix the encoding stays injective without
        // the key prefix, so neither is a live collision — the gap is between the
        // comments and the tests. One vector closes it: it pins the domain, both
        // prefixes and the suffix at once.
        //
        // **Changing any of them renumbers every paired device's flake**, and
        // `§16.2` says a flake is never regenerated on an existing device. That
        // must never be something a refactor does quietly.
        assert_eq!(MachineFlake::candidate(KEY, 0).value(), 160);
        assert_eq!(MachineFlake::candidate(KEY, 1).value(), 764);
        assert_eq!(MachineFlake::candidate(KEY, 2).value(), 196);
        assert_eq!(MachineFlake::candidate(KEY, 3).value(), 542);
    }

    #[test]
    fn a_flake_always_fits_in_its_ten_bits() {
        for attempt in 0..CANDIDATES {
            assert!(MachineFlake::candidate(KEY, attempt).value() < RANGE);
        }
    }

    #[test]
    fn the_same_key_derives_the_same_flake_after_a_reinstall() {
        // The property that makes this need no registry: a device that was wiped
        // and set up again reaches the flake it had.
        //
        // **`f(x) == f(x)` is not that property.** This was two calls to the
        // same pure function with the same arguments in the same process, which
        // is true of any deterministic function whatever it computes — the
        // shape the comments around it spend paragraphs condemning, three
        // functions away from them. What survives a reinstall is a value, so
        // the value is what is written down: the same four candidates the
        // measured vector below pins, spelled here as what a *device* gets back
        // rather than as what the hash produces.
        //
        // The two spellings are the point. `the_derivation_is_pinned_by_a_measured_vector`
        // fails when the derivation changes; this fails when the derivation
        // changes *and says what that costs* — a device that comes back after a
        // wipe is a different machine to the realm, and `§16.2` gives it no way
        // to ask for the flake it had.
        const AFTER_A_WIPE: [u16; 4] = [160, 764, 196, 542];
        // **Through `candidates()`, which is what a device walks.** Asserting
        // on `candidate(KEY, n)` — the pure function — pinned the derivation
        // and left the *sequence* free: `(0..CANDIDATES)` written `.rev()` kept
        // the length at 64, kept every distinct-count assertion, and handed a
        // device that came back after a wipe flake **367** where it had 160.
        // Order is the whole property here, because `§16.2` gives a device no
        // way to ask for the flake it had.
        let walked: Vec<u16> = MachineFlake::candidates(KEY)
            .take(AFTER_A_WIPE.len())
            .map(MachineFlake::value)
            .collect();
        assert_eq!(
            walked,
            AFTER_A_WIPE.to_vec(),
            "the first candidates a device walks are no longer what they were, so one \
             wiped and set up again would pair as a different machine"
        );
        // And the sequence is the pure function in order, so neither can move
        // without the other.
        for (attempt, expected) in AFTER_A_WIPE.iter().enumerate() {
            let attempt = u32::try_from(attempt).expect("four of them");
            assert_eq!(
                MachineFlake::candidate(KEY, attempt).value(),
                *expected,
                "candidate {attempt} for this key is no longer what it was"
            );
        }
        // And a key one byte different reaches somewhere else, or the constants
        // above would be satisfied by a derivation that ignores its input.
        let mut other = KEY.to_vec();
        other[0] ^= 1;
        assert_ne!(
            MachineFlake::candidate(&other, 0).value(),
            AFTER_A_WIPE[0],
            "another device's key reaches the same first candidate"
        );
    }

    #[test]
    fn different_keys_usually_derive_different_flakes() {
        // "Usually" is the honest word: ten bits collide, which is why the
        // candidate sequence exists at all. What this asserts is that the
        // derivation is not degenerate — a hash that ignored its input would
        // give every device the same flake and nothing else would notice.
        let mut seen = std::collections::BTreeSet::new();
        for index in 0_u32..200 {
            let key = format!("device key number {index}");
            seen.insert(MachineFlake::candidate(key.as_bytes(), 0));
        }
        assert!(
            seen.len() > 150,
            "200 distinct keys produced only {} flakes, which is not a hash",
            seen.len()
        );
    }

    #[test]
    fn the_candidate_sequence_moves() {
        // A sequence whose entries repeated would retry the taken flake forever.
        let candidates: Vec<_> = MachineFlake::candidates(KEY).take(10).collect();
        let distinct: std::collections::BTreeSet<_> = candidates.iter().collect();
        assert!(
            distinct.len() >= 9,
            "ten candidates gave {} distinct flakes",
            distinct.len()
        );
    }

    #[test]
    fn two_keys_do_not_share_a_candidate_sequence() {
        // Element by element, not `assert_ne!` on two vectors: comparing whole
        // sequences passes when one entry in eight differs, and it is the pair
        // that agrees *everywhere* that would have caught the missing length
        // prefix below.
        let ours: Vec<_> = MachineFlake::candidates(KEY).take(8).collect();
        let theirs: Vec<_> = MachineFlake::candidates(OTHER).take(8).collect();
        let shared = ours.iter().zip(&theirs).filter(|(a, b)| a == b).count();
        assert!(shared < 4, "{shared} of 8 candidates are shared");
    }

    #[test]
    fn a_key_that_looks_like_another_key_plus_a_counter_does_not_collide() {
        // The exact pair that used to collide, when the salt was a fixed
        // delimiter rather than a length prefix: appending the delimiter and a
        // counter to one key produced the same hasher input as that key at that
        // attempt.
        let mut forged = KEY.to_vec();
        forged.extend_from_slice(b"\x00nc-flake");
        forged.extend_from_slice(&5_u32.to_le_bytes());
        assert_ne!(
            MachineFlake::candidate(KEY, 5),
            MachineFlake::candidate(&forged, 0),
            "a crafted key still shares a candidate with a real one"
        );
    }

    #[test]
    fn the_first_candidate_is_domain_separated_like_the_rest() {
        // Attempt zero used to be a bare BLAKE3 of the key, so the flake seed
        // collided with any other bare hash of the same bytes.
        let bare = blake3::hash(KEY);
        let bytes = bare.as_bytes();
        let unseparated = u16::from_le_bytes([bytes[0], bytes[1]]) % RANGE;
        assert_ne!(MachineFlake::candidate(KEY, 0).value(), unseparated);
    }

    #[test]
    fn a_value_too_large_for_the_field_is_refused_rather_than_masked() {
        assert_eq!(
            MachineFlake::from_value(RANGE),
            Err(super::FlakeOutOfRange { value: RANGE })
        );
        // And it says which failure it is, in the vocabulary a log is queried
        // by — a bare `u16` could say only what the value was.
        assert_eq!(
            super::FlakeOutOfRange { value: RANGE }.code(),
            nc_catalog::ErrorCode::ValidationOutOfRange
        );
        assert_eq!(
            MachineFlake::from_value(RANGE - 1).map(MachineFlake::value),
            Ok(RANGE - 1)
        );
    }
}

#[cfg(test)]
mod the_bounds_reach_the_sentence {
    use super::{HIGHEST, MachineFlake, RANGE};

    /// The spelled bound is the layout's, and a change to `BITS` fails here.
    #[test]
    fn the_highest_flake_is_the_one_the_layout_allows() {
        assert_eq!(HIGHEST, (RANGE - 1).to_string());
    }

    /// The failure's own values fill its own sentence, in both locales.
    ///
    /// This is what `bounds` exists for, and it is asserted through `render`
    /// rather than by comparing strings: a `bounds` naming `minimum` instead of
    /// `min` would satisfy any test that only read it back.
    #[test]
    fn the_refusal_renders_with_no_brace_left_in_it() {
        let refusal = MachineFlake::from_value(u16::MAX).expect_err("out of range");
        for locale in nc_catalog::Locale::ALL {
            let values = refusal.with_label("machine flake");
            let sentence = nc_catalog::i18n::render(refusal.code().message_key(), *locale, &values)
                .expect("the code's key resolves")
                .expect("every placeholder has a value");
            assert!(!sentence.contains('{'), "{sentence}");
            // **The order, not only the presence.** Asserting that `HIGHEST`
            // appears somewhere is true of `[("min", HIGHEST), ("max", "0")]`
            // too, and `cargo xtask check` printed `check passed` with the
            // sentence reading *"machine flake must be between 1023 and 0."* in
            // both locales. A pair of bounds is an ordered pair.
            let low = sentence
                .find(" 0")
                .expect("the low bound is in the sentence");
            let high = sentence
                .find(HIGHEST)
                .expect("the high bound is in the sentence");
            assert!(
                low < high,
                "the bounds are the wrong way round in {}: {sentence}",
                locale.tag()
            );
        }
    }

    /// `bounds()` names the right keys *and* the right values.
    ///
    /// **It had no reader at all.** Its one caller was the test below, which
    /// asserts only that `render` refuses the missing `field` — and `fill`
    /// returns on the *first* unsupplied placeholder, which both locales put
    /// first. So it short-circuited before reading anything `bounds()` said, and
    /// `[("max", "999"), ("min", "17")]` — wrong values, wrong order — passed
    /// `cargo xtask check`. That is exactly the failure `bounds`' own doc names
    /// (*a `bounds` naming `minimum` instead of `min`*) and the one its sibling
    /// records measuring (*machine flake must be between 1023 and 0*).
    ///
    /// Held against `with_label`, which is tied to the layout through `HIGHEST`,
    /// rather than against a written pair: two copies of the same numbers is how
    /// this got here.
    #[test]
    fn the_bounds_are_the_labelled_ones_without_the_label() {
        let refusal = MachineFlake::from_value(u16::MAX).expect_err("out of range");
        let labelled = refusal.with_label("machine flake");
        assert_eq!(
            refusal.bounds()[..],
            labelled[1..],
            "`bounds` and `with_label` are two spellings of the same pair, and they \
             disagree"
        );
        // And the pair really is the layout's, in the order the sentence reads.
        assert_eq!(refusal.bounds(), [("min", "0"), ("max", HIGHEST)]);
    }

    /// The label the caller handed in is the noun the sentence shows.
    ///
    /// **Every other test in this module passes the same two words**, so
    /// `with_label` could ignore its argument and write the placeholder's own
    /// name instead: measured, `[("field", field), …]` written
    /// `[("field", "field"), …]` left `cargo test -p nc-ids` green at 106, 6 and
    /// 8. No brace is left in the sentence either way and the bounds are still
    /// in the right order, so both sibling tests hold — what is lost is the one
    /// thing this function exists for, which is that the noun is the caller's
    /// and therefore in the reader's language.
    ///
    /// `contains` rather than `starts_with`: both locales put the subject first
    /// today, and a locale that did not would still have to interpolate it.
    #[test]
    fn the_label_the_caller_gave_is_the_noun_in_the_sentence() {
        let refusal = MachineFlake::from_value(u16::MAX).expect_err("out of range");
        for locale in nc_catalog::Locale::ALL {
            let sentence = nc_catalog::i18n::render(
                refusal.code().message_key(),
                *locale,
                &refusal.with_label("marca de m\u{e1}quina"),
            )
            .expect("the code's key resolves")
            .expect("every placeholder has a value");
            assert!(
                sentence.contains("marca de m\u{e1}quina"),
                "the label in {} is not the one the caller gave: {sentence}",
                locale.tag()
            );
        }
    }

    #[test]
    /// Without the caller's label the sentence is refused, not half-rendered.
    ///
    /// The floor under the doc comment above: if `field` were quietly supplied
    /// somewhere, this would render and the comment would be wrong.
    fn the_label_is_the_callers_and_its_absence_is_refused() {
        let refusal = MachineFlake::from_value(u16::MAX).expect_err("out of range");
        let error = nc_catalog::i18n::render(
            refusal.code().message_key(),
            nc_catalog::Locale::En,
            &refusal.bounds(),
        )
        .expect("the code's key resolves")
        .expect_err("`field` is the caller's and was not supplied");
        assert_eq!(error, "field");
    }
}

#[cfg(test)]
mod the_whole_sequence_is_pinned {
    use super::{CANDIDATES, MachineFlake};

    /// Every candidate a device walks, in order — not the first four.
    ///
    /// **The sequence was pinned for 4 of 64.** The first four values were held
    /// against a written vector, the first ten for distinctness, and the total
    /// distinct count against `>= 56` where the real figure is 60 — four slots of
    /// slack. A reviewer reversed attempts 10..63, leaving the multiset and the
    /// first ten untouched, and `cargo xtask check` passed. A device whose first
    /// ten candidates are already taken then lands on a different flake after a
    /// wipe, which is the one thing `§16.2` gives it no way to recover from: the
    /// realm already holds its old flake for what it will treat as another
    /// machine.
    ///
    /// So the whole sequence is written down. It is a pure function of a key
    /// this test owns, so it can only change if the derivation changes — which is
    /// exactly the event that must not pass silently.
    #[test]
    fn the_sixty_four_candidates_are_the_ones_a_device_walked_before() {
        const KEY: &[u8] = b"an ed25519 public key would be thirty-two bytes";
        const WALKED: [u16; 64] = [
            160, 764, 196, 542, 590, 181, 607, 808, 32, 672, 370, 213, 289, 24, 152, 761, 330, 393,
            909, 320, 605, 1009, 862, 219, 344, 399, 126, 905, 670, 816, 499, 1023, 321, 408, 647,
            125, 338, 521, 90, 790, 160, 126, 616, 364, 404, 749, 657, 257, 602, 935, 829, 328,
            761, 59, 325, 942, 225, 551, 903, 390, 303, 370, 770, 367,
        ];
        let walked: Vec<u16> = MachineFlake::candidates(KEY)
            .map(MachineFlake::value)
            .collect();
        assert_eq!(
            walked.len(),
            CANDIDATES as usize,
            "the sequence is a different length than it was"
        );
        assert_eq!(
            walked,
            WALKED.to_vec(),
            "the candidate sequence moved. A device that is wiped and set up again \
             walks this list from the start, and the realm still holds its old flake \
             for what it will now treat as a different machine (§16.2)"
        );
    }
}
