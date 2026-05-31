//! Compact (nBits) representation of a 256-bit proof-of-work target.
//!
//! Algorithm follows the BIP-22 / Bitcoin Core reference encoding:
//! the 32-bit `nBits` field packs an 8-bit byte-length exponent in the
//! high byte and a 24-bit mantissa in the low three bytes. The decoded
//! target is `mantissa * 256^(exponent - 3)`. The negative-flag bit
//! (`0x00800000`) is reserved per the spec but is never set on a
//! validly-mined block.
//!
//! KDF-original. The encoding rule and edge-case handling are taken
//! directly from the public BIP-22 / Bitcoin Core specification, not from
//! any third-party implementation.

use crate::U256;

/// Compact difficulty target (the `nBits` field of a block header).
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Compact(u32);

impl From<u32> for Compact {
    fn from(u: u32) -> Self { Compact(u) }
}

impl From<Compact> for u32 {
    fn from(c: Compact) -> Self { c.0 }
}

impl From<U256> for Compact {
    fn from(u: U256) -> Self { Compact::from_u256(u) }
}

impl From<Compact> for U256 {
    fn from(c: Compact) -> Self {
        // Discard the overflow / negative diagnostic — the consumer side
        // only cares about the numeric target value, even when the
        // encoding is technically out of range.
        c.to_u256().unwrap_or_else(|x| x)
    }
}

impl Compact {
    /// Construct from a raw `u32` `nBits` value.
    pub fn new(u: u32) -> Self { Compact(u) }

    /// Maximum representable target.
    pub fn max_value() -> Self { U256::max_value().into() }

    /// Decode the compact form into a `U256` target.
    ///
    /// Returns `Ok(target)` on a well-formed encoding. Returns
    /// `Err(target)` if the encoding is either negative or overflowing —
    /// the partial result is still returned for diagnostic purposes.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_u256(&self) -> Result<U256, U256> {
        let exponent = self.0 >> 24;
        let mut mantissa = self.0 & 0x007fffff;

        let target = if exponent <= 3 {
            mantissa >>= 8 * (3 - exponent as usize);
            U256::from(mantissa)
        } else {
            U256::from(mantissa) << (8 * (exponent as usize - 3))
        };

        let negative = mantissa != 0 && (self.0 & 0x00800000) != 0;
        let overflow = (mantissa != 0 && exponent > 34)
            || (mantissa > 0xff && exponent > 33)
            || (mantissa > 0xffff && exponent > 32);

        if negative || overflow {
            Err(target)
        } else {
            Ok(target)
        }
    }

    /// Encode a `U256` target into compact form.
    pub fn from_u256(value: U256) -> Self {
        let mut size = value.bits().div_ceil(8);
        let mut mantissa = if size <= 3 {
            (value.low_u64() << (8 * (3 - size))) as u32
        } else {
            let shifted = value >> (8 * (size - 3));
            shifted.low_u32()
        };

        // The high bit of the mantissa is the negative-sign flag. If a
        // valid (positive) target lights it, shift the mantissa right by
        // a byte and bump the exponent to keep the value the same with
        // the sign bit clear.
        if (mantissa & 0x00800000) != 0 {
            mantissa >>= 8;
            size += 1;
        }

        debug_assert_eq!(mantissa & !0x007fffff, 0);
        debug_assert!(size < 256);
        Compact(mantissa | ((size as u32) << 24))
    }

    /// Approximate floating-point difficulty (target ratio relative to
    /// the well-known `0x1d00ffff` baseline). Used for human-readable
    /// reporting only.
    #[allow(clippy::wrong_self_convention)]
    pub fn to_f64(&self) -> f64 {
        let max_body = f64::from(0x00ffffu32).ln();
        let scaland = f64::from(256u32).ln();
        let ln_mantissa = f64::from(self.0 & 0x00ffffff).ln();
        let exponent_term = scaland * f64::from(0x1d - ((self.0 & 0xff000000) >> 24));
        (max_body - ln_mantissa + exponent_term).exp()
    }
}

#[cfg(test)]
mod tests {
    use super::{Compact, U256};

    #[test]
    fn decode_canonical_examples() {
        assert_eq!(Compact::new(0x01003456).to_u256(), Ok(0u32.into()));
        assert_eq!(Compact::new(0x01123456).to_u256(), Ok(0x12u32.into()));
        assert_eq!(Compact::new(0x02008000).to_u256(), Ok(0x80u32.into()));
        assert_eq!(Compact::new(0x05009234).to_u256(), Ok(0x92340000u64.into()));
        assert!(Compact::new(0x04923456).to_u256().is_err()); // negative
        assert_eq!(Compact::new(0x04123456).to_u256(), Ok(0x12345600u64.into()));
    }

    #[test]
    fn encode_canonical_examples() {
        assert_eq!(Compact::new(0x0203e800), Compact::from_u256(U256::from(1000u64)));

        let max_target = U256::from(2).pow(U256::from(256 - 32)) - U256::from(1);
        assert_eq!(Compact::new(0x1d00ffff), Compact::from_u256(max_target));
    }

    #[test]
    fn round_trip_well_formed_encodings() {
        let cases = [0x1d00ffffu32, 0x05009234u32];
        for nbits in cases {
            let c = Compact::new(nbits);
            let r = Compact::from_u256(c.to_u256().unwrap());
            assert_eq!(c, r, "round-trip mismatch for 0x{:08x}", nbits);
        }
    }

    #[test]
    fn difficulty_baseline() {
        let nbits = Compact::new(0x1b0404cb);
        assert_eq!(nbits.to_f64(), 16307.420938523994f64);
    }
}
