//! SHA-256, so a file this tree pins is the one another repository records.
//!
//! # Why this is here and not a dependency
//!
//! `NOCTUA.md §2.3` asks for our own solution where one is reasonable, and
//! this one is: FIPS 180-4 is a page of arithmetic with no options and no
//! configuration, and the vectors its tests hold are the standard's own. It
//! is the function `noctua-design` already keeps for the same purpose, written
//! again here because no crate of that repository is a dependency of this
//! tool, and a crate would be a supply-chain edge for sixty lines of shifts.
//!
//! **It guards no secret.** What it is pointed at is a file of this tree,
//! compared with a constant of this tree, so whoever can change one can change
//! the other and no collision is worth forging: the question it answers is
//! whether a byte changed. It is SHA-256 rather than the BLAKE3 this tool
//! already links because the other side of that question is a `SHA256SUMS`
//! line `sha256sum` wrote, in `noctua-design`. The primitive `§2.3` names as
//! not viable in-house is one a secret rests on, which this is not.
//!
//! It hashes a whole message held in memory rather than streaming, because
//! the largest thing it is pointed at is a few kilobytes of text.

/// The round constants: the first thirty-two bits of the fractional parts of
/// the cube roots of the first sixty-four primes.
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// The initial state: the first thirty-two bits of the fractional parts of the
/// square roots of the first eight primes.
const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// The first working variable, by the letter the standard gives it.
///
/// Named indices into one array rather than eight bindings: a round moves
/// every variable one place along, which is one rotation of the array. `d`
/// has no name because nothing reads it but that rotation.
const A: usize = 0;
/// The second.
const B: usize = 1;
/// The third.
const C: usize = 2;
/// The fifth, which is the one the round adds its first sum to.
const E: usize = 4;
/// The sixth.
const F: usize = 5;
/// The seventh.
const G: usize = 6;
/// The eighth.
const H: usize = 7;

/// The SHA-256 of `message`, in lower-case hexadecimal, as `sha256sum` writes it.
pub(crate) fn hex(message: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut state = INITIAL;
    let padded = padded(message);
    // Whole blocks and nothing left over, which the padding guarantees.
    let (blocks, _) = padded.as_chunks::<64>();
    for block in blocks {
        let mut schedule = [0_u32; 64];
        for (word, bytes) in schedule.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for index in 16..64 {
            let early = schedule[index - 15];
            let late = schedule[index - 2];
            let s0 = early.rotate_right(7) ^ early.rotate_right(18) ^ (early >> 3);
            let s1 = late.rotate_right(17) ^ late.rotate_right(19) ^ (late >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }
        let mut work = state;
        for (constant, word) in K.iter().zip(schedule) {
            let sum1 =
                work[E].rotate_right(6) ^ work[E].rotate_right(11) ^ work[E].rotate_right(25);
            let choose = (work[E] & work[F]) ^ (!work[E] & work[G]);
            let first = work[H]
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(*constant)
                .wrapping_add(word);
            let sum0 =
                work[A].rotate_right(2) ^ work[A].rotate_right(13) ^ work[A].rotate_right(22);
            let majority = (work[A] & work[B]) ^ (work[A] & work[C]) ^ (work[B] & work[C]);
            // Every variable takes its neighbour's value, which is the
            // rotation; then the two the round computes are written over it:
            // `e` is the old `d` plus the first sum, and `a` is both sums.
            work.rotate_right(1);
            work[E] = work[E].wrapping_add(first);
            work[A] = first.wrapping_add(sum0.wrapping_add(majority));
        }
        for (slot, value) in state.iter_mut().zip(work) {
            *slot = slot.wrapping_add(value);
        }
    }
    let mut out = String::with_capacity(64);
    for word in state {
        let _ = write!(out, "{word:08x}");
    }
    out
}

/// `message` padded as the standard pads it, to a whole number of 64-byte blocks.
///
/// A one bit, zeros up to 56 bytes modulo 64, and the message's length in
/// bits as a big-endian `u64`, which the standard counts modulo `2^64`.
fn padded(message: &[u8]) -> Vec<u8> {
    // A `usize` is no wider than a `u64` on any target this tool builds for,
    // so the conversion does not fail and its fallback is never taken.
    let bits = u64::try_from(message.len())
        .unwrap_or(u64::MAX)
        .wrapping_mul(8);
    let mut padded = Vec::with_capacity(message.len() + 72);
    padded.extend_from_slice(message);
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bits.to_be_bytes());
    padded
}

#[cfg(test)]
mod tests {
    use super::hex;

    /// The vectors FIPS 180-4's examples give, and the empty message.
    #[test]
    fn the_standard_s_own_vectors() {
        assert_eq!(
            hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            hex(b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmn\
                  hijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu"),
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1"
        );
        assert_eq!(
            hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    /// Each side of the two places a hand-written padding breaks.
    ///
    /// The last length that fits one block with its padding and the first that
    /// needs another, at the first two block boundaries; each digest is the
    /// one `sha256sum` printed for the same bytes.
    #[test]
    fn each_side_of_the_padding_boundaries() {
        for (length, digest) in [
            (
                55,
                "9f4390f8d30c2dd92ec9f095b65e2b9ae9b0a925a5258e241c9f1e910f734318",
            ),
            (
                56,
                "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a",
            ),
            (
                63,
                "7d3e74a05d7db15bce4ad9ec0658ea98e3f06eeecf16b4c6fff2da457ddc2f34",
            ),
            (
                64,
                "ffe054fe7ae0cb6dc65c3af9b61d5209f439851db43d0ba5997337df154668eb",
            ),
            (
                119,
                "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb",
            ),
            (
                120,
                "2f3d335432c70b580af0e8e1b3674a7c020d683aa5f73aaaedfdc55af904c21c",
            ),
        ] {
            assert_eq!(hex(&vec![b'a'; length]), digest, "{length} byte(s)");
        }
    }
}
