use std::ops::BitXor;

use crate::error::{Error, LengthErrorKind, LengthResult};

mod builder;

#[allow(clippy::module_name_repetitions)]
pub use crate::sha::builder::ShaHashBuilder;

/// Length of a SHA-1 hash.
pub const SHA_HASH_LEN: usize = 20;

/// SHA-1 hash wrapper type for performing operations on the hash.
#[allow(clippy::module_name_repetitions)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct ShaHash {
    hash: [u8; SHA_HASH_LEN],
}

impl ShaHash {
    /// Create a `ShaHash` by hashing the given bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> ShaHash {
        ShaHashBuilder::new().add_bytes(bytes).build()
    }

    /// Create a `ShaHash` directly from the given hash.
    ///
    /// # Errors
    ///
    /// It would error if the hash is the wrong group.
    pub fn from_hash(hash: &[u8]) -> LengthResult<ShaHash> {
        if hash.len() == SHA_HASH_LEN {
            let mut my_hash = [0u8; SHA_HASH_LEN];

            my_hash.iter_mut().zip(hash.iter()).map(|(dst, src)| *dst = *src).count();

            Ok(ShaHash { hash: my_hash })
        } else {
            Err(Error::new(LengthErrorKind::LengthExpected, SHA_HASH_LEN))
        }
    }

    #[must_use]
    pub fn bits(&self) -> Bits<'_> {
        Bits::new(&self.hash)
    }

    #[must_use]
    pub fn len() -> usize {
        SHA_HASH_LEN
    }
}

impl AsRef<[u8]> for ShaHash {
    fn as_ref(&self) -> &[u8] {
        &self.hash
    }
}

impl From<ShaHash> for [u8; SHA_HASH_LEN] {
    fn from(val: ShaHash) -> Self {
        val.hash
    }
}

impl From<[u8; SHA_HASH_LEN]> for ShaHash {
    fn from(sha_hash: [u8; SHA_HASH_LEN]) -> ShaHash {
        ShaHash { hash: sha_hash }
    }
}

impl PartialEq<[u8]> for ShaHash {
    fn eq(&self, other: &[u8]) -> bool {
        let is_equal = other.len() == self.hash.len();

        self.hash
            .iter()
            .zip(other.iter())
            .fold(is_equal, |prev, (h, o)| prev && h == o)
    }
}

impl BitXor<ShaHash> for ShaHash {
    type Output = ShaHash;

    fn bitxor(mut self, rhs: ShaHash) -> ShaHash {
        for (src, dst) in rhs.hash.iter().zip(self.hash.iter_mut()) {
            *dst ^= *src;
        }

        self
    }
}

// ----------------------------------------------------------------------------//

/// Representation of a bit after a xor operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum XorRep {
    /// Bits were equal (1).
    Diff,
    /// Bits were not equal (0).
    Same,
}

// ----------------------------------------------------------------------------//

/// Representation of a bit.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum BitRep {
    /// Bit is set (1).
    Set,
    /// Bit is not set (0).
    Unset,
}

impl PartialEq<XorRep> for BitRep {
    fn eq(&self, other: &XorRep) -> bool {
        matches!((self, other), (&BitRep::Set, &XorRep::Diff) | (&BitRep::Unset, &XorRep::Same))
    }
}

/// Iterator over some bits.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct Bits<'a> {
    bytes: &'a [u8],
    bit_pos: usize,
}

impl<'a> Bits<'a> {
    fn new(bytes: &'a [u8]) -> Bits<'a> {
        Bits { bytes, bit_pos: 0 }
    }
}

#[allow(clippy::copy_iterator)]
impl<'a> Iterator for Bits<'a> {
    type Item = BitRep;

    fn next(&mut self) -> Option<BitRep> {
        if self.bit_pos < self.bytes.len() * 8 {
            let byte_index = self.bit_pos / 8;
            let bit_offset = 7 - (self.bit_pos % 8);
            let bit_value = self.bytes[byte_index] >> bit_offset;

            self.bit_pos += 1;

            Some(bit_value).map(|x| if x == 1 { BitRep::Set } else { BitRep::Unset })
        } else {
            None
        }
    }
}

// ----------------------------------------------------------------------------//

#[cfg(test)]
mod tests {
    use super::{ShaHash, XorRep};

    #[test]
    fn positive_no_leading_zeroes() {
        let zero_bits = ShaHash::from([0u8; super::SHA_HASH_LEN]);
        let one_bits = ShaHash::from([255u8; super::SHA_HASH_LEN]);

        let xor_hash = zero_bits ^ one_bits;

        let leading_zeroes = xor_hash.bits().take_while(|&n| n == XorRep::Same).count();
        assert!(leading_zeroes == 0);
    }

    #[test]
    fn positive_all_leading_zeroes() {
        let first_one_bits = ShaHash::from([255u8; super::SHA_HASH_LEN]);
        let second_one_bits = ShaHash::from([255u8; super::SHA_HASH_LEN]);

        let xor_hash = first_one_bits ^ second_one_bits;

        let leading_zeroes = xor_hash.bits().take_while(|&n| n == XorRep::Same).count();
        assert!(leading_zeroes == (super::SHA_HASH_LEN * 8));
    }

    #[test]
    fn positive_one_leading_zero() {
        let zero_bits = ShaHash::from([0u8; super::SHA_HASH_LEN]);

        let mut bytes = [255u8; super::SHA_HASH_LEN];
        bytes[0] = 127;
        let mostly_one_bits = ShaHash::from(bytes);

        let xor_hash = zero_bits ^ mostly_one_bits;

        let leading_zeroes = xor_hash.bits().take_while(|&n| n == XorRep::Same).count();
        assert!(leading_zeroes == 1);
    }

    #[test]
    fn positive_one_trailing_zero() {
        let zero_bits = ShaHash::from([0u8; super::SHA_HASH_LEN]);

        let mut bytes = [255u8; super::SHA_HASH_LEN];
        bytes[super::SHA_HASH_LEN - 1] = 254;
        let mostly_zero_bits = ShaHash::from(bytes);

        let xor_hash = zero_bits ^ mostly_zero_bits;

        let leading_zeroes = xor_hash.bits().take_while(|&n| n == XorRep::Same).count();
        assert!(leading_zeroes == 0);
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: Error { kind: LengthExpected, length: 20, index: None }"
    )]
    fn negative_from_hash_too_long() {
        let bits = [0u8; super::SHA_HASH_LEN + 1];

        ShaHash::from_hash(&bits).unwrap();
    }

    #[test]
    #[should_panic(
        expected = "called `Result::unwrap()` on an `Err` value: Error { kind: LengthExpected, length: 20, index: None }"
    )]
    fn negative_from_hash_too_short() {
        let bits = [0u8; super::SHA_HASH_LEN - 1];

        ShaHash::from_hash(&bits).unwrap();
    }
}
