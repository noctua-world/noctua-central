//! Crockford base32, which is how a Snowflake appears in a file name.
//!
//! Crockford rather than RFC 4648 because these names are read aloud and typed
//! by hand: the alphabet omits `I`, `L`, `O` and `U`, so there is no letter that
//! can be confused with a digit and no accidental profanity. Decoding accepts
//! the confusable characters and maps them to what they look like, which is what
//! makes a name somebody retyped from a screen still resolve.
//!
//! # The width, which the specification pins down without saying so
//!
//! `NOCTUA.md §16.3` writes exactly one name —
//! `==familia=fotos--praia-de-copacabana__brasil@@01J8XK2M9P51.jpg` — and says
//! *reproduce this scheme exactly, do not "improve" it*. That identifier is
//! **twelve** characters and **begins with a zero**, and its value does not need
//! that zero: encoded with no padding it is `1J8XK2M9P51`, eleven. So the one
//! example the specification writes down is itself padded, to twelve, and
//! reproducing it exactly means padding to twelve.
//!
//! Padding to **thirteen**, the width a full `u64` takes, does not rebuild the
//! example, and `§16.3` asks for the example reproduced; twelve is the width
//! that does it.
//!
//! **The cost is measured rather than guessed, and it is the same either way.**
//! Twelve characters hold values below `32^12 = 2^60`; with twenty-two bits of
//! flake and sequence below the timestamp, that is `2^38 = 274_877_906_944`
//! milliseconds — **8.7 years** from the epoch of 2026-01-01, so names grow to
//! thirteen characters in **2034**. From that day an alphabetical sort of a
//! directory stops matching a chronological one, whatever the minimum width is.
//! Sorting by identifier *value* still works, and that is what anything reading
//! these names should do.

/// The alphabet, in value order. 32 characters, no `I`, `L`, `O` or `U`.
const ALPHABET: [u8; 32] = *b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// The most characters a 64-bit value can take.
///
/// `32^13 > 2^64`, so thirteen is the ceiling. Decoding refuses anything longer,
/// because a longer string is either not an identifier or an identifier from a
/// scheme this is not.
pub const MAX_WIDTH: usize = 13;

/// The fewest characters an encoding is written with.
///
/// Twelve, because that is what `NOCTUA.md §16.3`'s worked example is. See the
/// module docs: the example's leading zero is padding, and reproducing the
/// example exactly is what the developer chose.
pub const MIN_WIDTH: usize = 12;

/// Why some text is not a Crockford base32 identifier.
///
/// An enum rather than a bare `char`, because the failures are different and
/// a `char` could only express one: an empty string would be reported as the
/// null character, and a value with bits outside the layout as its *first*
/// character — which is usually a perfectly valid digit, pointing the reader at
/// the wrong place (`NOCTUA.md §15.1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base32Error {
    /// There was nothing to decode.
    Empty,
    /// More characters than a 64-bit value can take.
    TooLong {
        /// How many there were.
        length: usize,
    },
    /// The right number of characters, and a value too large for the layout.
    ///
    /// **A separate variant, because the two say different things to whoever
    /// reads them.** They shared `TooLong`, and at exactly [`MAX_WIDTH`]
    /// characters the message rendered as "13 characters is more than the 13 a
    /// 64-bit value takes" — a sentence that contradicts itself, which is the
    /// `§15.1` failure this enum's own doc says it exists to avoid. Thirteen
    /// Crockford digits can encode more than 64 bits, so the width is legal and
    /// the value is not.
    Overflows {
        /// How many characters there were. Legal, which is the point.
        length: usize,
        /// How many bits the value was being decoded into.
        ///
        /// **64 for a raw decode and 63 for a `Snowflake`**, not 64 whoever
        /// raises it. `Snowflake::from_crockford` refuses a value with bits
        /// outside the 63 its layout uses, and saying *"this value needs more
        /// than the 64 bits an identifier has"* there would
        /// contradict `base32::decode` on the same text, which
        /// answers `Ok(9223372036854775808)` — proof it fits in
        /// 64. `from_crockford`'s own doc says 63. That is the same
        /// self-contradiction this variant is split out of `TooLong` to end,
        /// one layer along.
        bits: u32,
    },
    /// A character the alphabet does not hold.
    NotBase32 {
        /// Which one.
        character: char,
        /// Its offset.
        at: usize,
    },
}

impl Base32Error {
    /// The registry code this failure carries.
    ///
    /// **Every failure here carries one.** `§15.1` makes an error's identity
    /// data rather than a string at a call site, and a type with no code cannot
    /// be journalled, cannot be logged as a `LogValue` and cannot be queried
    /// for — which is the whole reason the registry exists. `NC-IDS-0006` is
    /// shared with [`crate::OpaqueIdError`] because both say the same thing to
    /// a reader: what you handed me is not an identifier.
    /// [`crate::MachineFlake::from_value`] is **not** one of them — it takes
    /// a `u16`, so there is no text and nobody typed it, and it
    /// carries `NC-VAL-0001` instead.
    #[must_use]
    pub const fn code(self) -> nc_catalog::ErrorCode {
        nc_catalog::ErrorCode::IdsNotAnIdentifier
    }
}

impl core::fmt::Display for Base32Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Empty => formatter.write_str("there is nothing to decode"),
            Self::TooLong { length } => write!(
                formatter,
                "{length} characters is more than the {MAX_WIDTH} a 64-bit value takes"
            ),
            Self::Overflows { length, bits } => write!(
                formatter,
                "{length} characters is a legal width, and this value needs more than the \
                 {bits} bits it is being decoded into"
            ),
            Self::NotBase32 { character, at } => {
                write!(
                    formatter,
                    "`{}` at {at} is not Crockford base32",
                    nc_canon::shown(*character)
                )
            }
        }
    }
}

impl core::error::Error for Base32Error {}

/// Encodes a value, padded to [`MIN_WIDTH`] and no further.
#[must_use]
pub fn encode(value: u64) -> String {
    let mut digits = Vec::with_capacity(MAX_WIDTH);
    let mut rest = value;
    while rest > 0 {
        // Five bits, and the alphabet has thirty-two entries, so the index is
        // always inside it.
        digits.push(ALPHABET[usize::try_from(rest & 0x1f).unwrap_or(0)]);
        rest >>= 5;
    }
    while digits.len() < MIN_WIDTH {
        digits.push(b'0');
    }
    digits.reverse();
    digits.into_iter().map(char::from).collect()
}

/// Decodes a value, accepting the confusable characters Crockford defines.
///
/// # Errors
///
/// Returns [`Base32Error`] naming what was wrong and where.
pub fn decode(text: &str) -> Result<u64, Base32Error> {
    let length = text.chars().count();
    if length == 0 {
        return Err(Base32Error::Empty);
    }
    if length > MAX_WIDTH {
        return Err(Base32Error::TooLong { length });
    }
    let mut value = 0_u64;
    for (at, character) in text.chars().enumerate() {
        let digit = match character.to_ascii_uppercase() {
            // Crockford's own decoding rules: these three are read as what they
            // resemble, because a person copying a name off a screen will type
            // the letter that looks like the digit. `U` is excluded from the
            // alphabet on purpose and is not a confusable, so it is an error
            // rather than a silent zero.
            'O' => 0,
            'I' | 'L' => 1,
            other => {
                let byte =
                    u8::try_from(other).map_err(|_| Base32Error::NotBase32 { character, at })?;
                let Some(position) = ALPHABET.iter().position(|candidate| *candidate == byte)
                else {
                    return Err(Base32Error::NotBase32 { character, at });
                };
                u64::try_from(position).map_err(|_| Base32Error::NotBase32 { character, at })?
            }
        };
        value = value
            .checked_mul(32)
            .and_then(|shifted| shifted.checked_add(digit))
            .ok_or(Base32Error::Overflows { length, bits: 64 })?;
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::{Base32Error, MAX_WIDTH, MIN_WIDTH, decode, encode};

    #[test]
    fn a_value_survives_the_round_trip() {
        for value in [0, 1, 31, 32, 1_234_567_890, u64::MAX >> 1, u64::MAX] {
            assert_eq!(decode(&encode(value)), Ok(value), "{value}");
        }
    }

    #[test]
    fn the_specification_s_own_example_rebuilds_byte_for_byte() {
        // NOCTUA.md §16.3 writes down exactly one name. Padding to thirteen means
        // it does not rebuild; not padding at all means it does not either,
        // because its identifier has a leading zero its value does not need.
        let example = "01J8XK2M9P51";
        let value = decode(example).expect("the specification's own example");
        assert_eq!(encode(value), example);
    }

    #[test]
    fn zero_is_the_minimum_width_of_zeros() {
        assert_eq!(encode(0), "0".repeat(MIN_WIDTH));
        assert_eq!(decode("0"), Ok(0));
    }

    #[test]
    fn the_width_grows_past_the_minimum_only_when_the_value_needs_it() {
        // Measured rather than asserted in prose: 2^60 is the transition that
        // makes an alphabetical sort stop matching a chronological one, and the
        // module header gives the year it happens.
        assert_eq!(encode(1).len(), MIN_WIDTH);
        assert_eq!(encode((1 << 60) - 1).len(), MIN_WIDTH);
        assert_eq!(encode(1 << 60).len(), MIN_WIDTH + 1);
        assert!(encode(u64::MAX).len() <= MAX_WIDTH);
    }

    #[test]
    fn the_confusable_characters_decode_to_what_they_look_like() {
        // Somebody retyping a name off a screen types the letter. This is what
        // makes that name still resolve.
        assert_eq!(decode("O"), Ok(0));
        assert_eq!(decode("I"), Ok(1));
        assert_eq!(decode("L"), Ok(1));
        assert_eq!(
            decode("u"),
            Err(Base32Error::NotBase32 {
                character: 'u',
                at: 0
            })
        );
    }

    #[test]
    fn a_lenient_spelling_resolves_and_re_encodes_to_the_canonical_one() {
        // Decoding is lenient on purpose, so a name somebody retyped off a
        // screen still resolves — and re-encoding is what gives a caller the one
        // spelling to propose instead. `FileName::canonical_name` is that caller.
        let canonical = "01J8XK2M9P51";
        for lenient in ["01j8xk2m9p51", "0lJ8XK2M9P5I", "001J8XK2M9P51"] {
            let value = decode(lenient).unwrap_or_else(|why| panic!("`{lenient}`: {why}"));
            assert_eq!(
                encode(value),
                canonical,
                "`{lenient}` should re-encode canonically"
            );
        }
    }

    #[test]
    fn a_failure_names_the_character_and_where_it_was() {
        assert_eq!(decode(""), Err(Base32Error::Empty));
        assert_eq!(
            decode("01U"),
            Err(Base32Error::NotBase32 {
                character: 'U',
                at: 2
            })
        );
        let long = "Z".repeat(MAX_WIDTH + 1);
        assert_eq!(
            decode(&long),
            Err(super::Base32Error::TooLong {
                length: super::MAX_WIDTH + 1
            })
        );
    }

    #[test]
    fn a_value_past_sixty_four_bits_is_refused_rather_than_wrapped() {
        // Thirteen `Z`s is above `u64::MAX`, and wrapping would silently name a
        // different identifier.
        assert_eq!(
            decode(&"Z".repeat(MAX_WIDTH)),
            Err(Base32Error::Overflows {
                length: MAX_WIDTH,
                bits: 64,
            })
        );
    }
}

#[cfg(test)]
mod width_tests {
    use super::{Base32Error, MAX_WIDTH, decode};

    #[test]
    fn every_refusal_renders_a_sentence_that_does_not_contradict_itself() {
        // **`Display` executed, not only formatted on failure.**
        // `assert_eq!(…, "{error}")` only formats when it fails, so without
        // this every message in these crates goes unrendered — and `TooLong` at
        // exactly `MAX_WIDTH` could read "13 characters is more than the 13 a
        // 64-bit value takes", which is the §15.1 failure this enum's own doc
        // says it exists to avoid.
        let rendered = |error: Base32Error| error.to_string();
        let width = rendered(Base32Error::TooLong {
            length: MAX_WIDTH + 1,
        });
        assert!(width.contains(&(MAX_WIDTH + 1).to_string()), "{width}");
        assert!(width.contains(&MAX_WIDTH.to_string()), "{width}");

        let overflow = rendered(Base32Error::Overflows {
            length: MAX_WIDTH,
            bits: 64,
        });
        assert!(overflow.contains("legal width"), "{overflow}");
        assert_ne!(width, overflow, "the two refusals read the same");

        // And no message a caller can actually be shown may say a number is more
        // than itself. `TooLong` is reachable only above `MAX_WIDTH` — that is
        // what separates it from `Overflows` — so the assertion is over what
        // `decode` can produce rather than over what the type can hold.
        for length in [MAX_WIDTH + 1, MAX_WIDTH + 2, 50] {
            let text = rendered(Base32Error::TooLong { length });
            assert!(
                !text.contains(&format!("{length} characters is more than the {length}")),
                "a refusal contradicts itself: {text}"
            );
        }
        assert_eq!(
            decode(&"0".repeat(MAX_WIDTH)),
            Ok(0),
            "a legal width has to decode, or `TooLong` would be reachable at it"
        );
        assert_ne!(rendered(Base32Error::Empty), "");
        assert!(
            rendered(Base32Error::NotBase32 {
                character: 'u',
                at: 3
            })
            .contains('u')
        );
    }

    #[test]
    fn a_long_string_of_zeros_is_refused_for_its_width_and_not_for_its_value() {
        // The **length** refusal, on its own. Fourteen `Z`s are rejected by the
        // overflow check behind it with the identical `TooLong { length: 14 }`
        // — so through them the two branches are indistinguishable, and
        // deleting the width check leaves `decode(&"0".repeat(50))` returning
        // `Ok(0)`.
        let zeros = "0".repeat(50);
        assert_eq!(decode(&zeros), Err(Base32Error::TooLong { length: 50 }));
        // A string of zeros exactly at the limit decodes, so the bound is checked
        // on both sides rather than only above.
        let at_limit = "0".repeat(MAX_WIDTH);
        assert_eq!(decode(&at_limit), Ok(0));
        assert_eq!(
            decode(&"0".repeat(MAX_WIDTH + 1)),
            Err(super::Base32Error::TooLong {
                length: super::MAX_WIDTH + 1
            })
        );
    }
}

#[cfg(test)]
mod the_alphabet_is_crockfords {
    use super::{ALPHABET, decode, encode};

    /// The alphabet is the published one, character for character.
    ///
    /// **A round trip is invariant under any permutation of the table.** With
    /// `W` and `X` transposed, every round trip still holds — and so do
    /// `nc-journal`'s pinned BLAKE3 vectors, because those hash the value and
    /// not its spelling. What comes out is a private alphabet no other Crockford
    /// reader decodes:
    ///
    /// ```text
    /// encode(28)          = 00000000000X     (Crockford: …W)
    /// decode("0000000000W") = Ok(29)         (Crockford: 28)
    /// ```
    ///
    /// This module's header chooses Crockford because these names are read aloud
    /// and typed by hand, which is a claim about *other people's* decoders. So
    /// the table is written out here as a constant of its own, and the encoding
    /// is checked against values taken from the specification rather than from
    /// this file.
    #[test]
    fn the_table_is_the_published_one() {
        // Douglas Crockford's base32: the digits, then the uppercase letters with
        // `I`, `L`, `O` and `U` left out — `I` and `L` because they read as `1`,
        // `O` because it reads as `0`, and `U` to keep an accidental obscenity
        // out of an identifier a person may have to read out.
        const PUBLISHED: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        assert_eq!(&ALPHABET[..], PUBLISHED);
        for excluded in *b"ILOU" {
            assert!(
                !ALPHABET.contains(&excluded),
                "`{}` is in the table and Crockford leaves it out",
                excluded as char
            );
        }
    }

    /// Values whose spelling comes from the specification, not from this crate.
    ///
    /// Each is a number whose base-32 digits land on a letter the permutation
    /// above would move, so a transposed table fails here rather than round-
    /// tripping quietly. `MIN_WIDTH` pads on the left, which is why the expected
    /// strings carry leading zeroes.
    #[test]
    fn the_spellings_are_the_published_ones() {
        for (value, digits) in [
            (0_u64, "0"),
            (9, "9"),
            (10, "A"),
            (17, "H"),
            // 18 is `J`, because `I` is not in the table.
            (18, "J"),
            (19, "K"),
            (20, "M"),
            (27, "V"),
            (28, "W"),
            (29, "X"),
            (30, "Y"),
            (31, "Z"),
            (32, "10"),
            (1023, "ZZ"),
        ] {
            let written = encode(value);
            assert!(
                written.ends_with(digits),
                "{value} is written `{digits}` in Crockford base32 and this wrote \
                 `{written}`"
            );
            assert_eq!(decode(&written), Ok(value), "{written}");
        }
    }

    /// The excluded letters decode as what they look like — except `U`.
    ///
    /// Crockford's decoder is deliberately forgiving about the three characters
    /// left out for being confusable, and this one follows it: `I` and `L` read
    /// as `1`, `O` as `0`. `U` is the odd one out, left out to keep an accidental
    /// obscenity out of an identifier somebody may have to read aloud, and it has
    /// no digit it looks like — so it is refused.
    ///
    /// Written down because nothing else types one. Every other test here round
    /// trips through `encode`, which never emits any of these four, so without
    /// this the entire forgiving branch of the decoder is reachable only from
    /// outside this crate.
    #[test]
    fn a_letter_the_table_leaves_out_reads_as_the_digit_it_resembles() {
        for (confusable, digit) in [("I", "1"), ("L", "1"), ("O", "0")] {
            assert_eq!(
                decode(confusable),
                decode(digit),
                "`{confusable}` is Crockford's spelling of `{digit}` and this read it \
                 as something else"
            );
        }
        assert!(
            decode("U").is_err(),
            "`U` resembles no digit and is left out on purpose; accepting it would put \
             a character into the alphabet that `encode` can never produce"
        );
    }

    /// The width rule counts characters, and a byte count is not one.
    ///
    /// **A length standing in for the property it measures.** Without this,
    /// `text.len()` for `text.chars().count()` leaves the crate green and makes
    /// seven `é` refuse as `TooLong { length: 14 }` — *"14 characters is more
    /// than the 13 a 64-bit value takes"* — for a string that is seven
    /// characters long. It can only mis-diagnose an identifier that is already
    /// illegal, since every Crockford character is ASCII, but the enum exists so
    /// that a refusal does not point the reader at the wrong thing, and this
    /// module already carries
    /// `every_refusal_renders_a_sentence_that_does_not_contradict_itself`.
    #[test]
    fn a_multi_byte_character_is_refused_for_being_one_and_not_for_a_width() {
        // Seven characters, fourteen bytes: short enough to pass the width rule
        // whichever way it is spelled, so this fails only on the substitution.
        let refused = super::decode("ééééééé");
        assert_eq!(
            refused,
            Err(super::Base32Error::NotBase32 {
                character: 'é',
                at: 0
            }),
            "a seven-character string was judged by its byte count"
        );
        // And the width rule still is one, so this cannot pass by never
        // refusing for width at all.
        let long = super::decode(&"1".repeat(super::MAX_WIDTH + 1));
        assert_eq!(
            long,
            Err(super::Base32Error::TooLong {
                length: super::MAX_WIDTH + 1
            })
        );
    }
}
