//! Hash and checksum primitives used across KDF UTXO machinery.
//!
//! All algorithms exposed here are public specifications:
//!
//! | Function       | Spec                                                  |
//! |----------------|-------------------------------------------------------|
//! | `sha1`         | FIPS PUB 180-4 §6.1                                   |
//! | `sha256`       | FIPS PUB 180-4 §6.2                                   |
//! | `ripemd160`    | RIPEMD-160 (Dobbertin / Bosselaers / Preneel, 1996)   |
//! | `keccak256`    | NIST FIPS 202 (Keccak-f[1600], pre-padding variant)   |
//! | `groestl512`   | Groestl SHA-3 candidate (Gauravaram et al., 2011)     |
//! | `siphash24`    | SipHash-2-4 (Aumasson & Bernstein, 2012)              |
//!
//! The composed helpers (`dhash160`, `dhash256`, `dgroestl512`, `dkeccak256`)
//! and the [`ChecksumType`] dispatch enum are KDF's own taxonomy: they encode
//! the per-coin checksum convention used by atomic-swap UTXO peers
//! (`DSHA256` for Bitcoin-derived, `DGROESTL512` for Groestlcoin,
//! `KECCAK256` for SmartCash). The wrappers themselves are mechanical
//! adaptations to the `H160`/`H256`/`H512`/`H32` types defined by KDF.
//!
//! KDF-original. Phase B replacement for `bitcrypto`.

use groestl::Groestl512;
use primitives::hash::{H160, H256, H32, H512};
use ripemd::{Digest as _, Ripemd160};
use sha1::Sha1;
use sha2::Sha256;
use sha3::Keccak256;
use siphasher::sip::SipHasher24;
use std::hash::Hasher;

/// Per-coin checksum dispatch.
///
/// KDF-defined taxonomy: each variant encodes a coin family's address and
/// payload checksum convention, used when constructing or validating WIF
/// keys, base58 addresses, and similar 4-byte appended-checksum payloads.
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ChecksumType {
    /// Double SHA-256: Bitcoin and the vast majority of UTXO derivatives.
    #[default]
    DSHA256,
    /// Double Groestl-512: Groestlcoin.
    DGROESTL512,
    /// Single Keccak-256 (legacy pre-NIST-padding form): SmartCash.
    KECCAK256,
}

/// RIPEMD-160 of the given input.
#[inline]
pub fn ripemd160(input: &[u8]) -> H160 {
    let mut hasher = Ripemd160::new();
    hasher.update(input);
    let out: [u8; 20] = hasher.finalize().into();
    out.into()
}

/// SHA-1 of the given input. Returned as H160 (160-bit fixed array)
/// to match the legacy KDF API surface.
#[inline]
pub fn sha1(input: &[u8]) -> H160 {
    let mut hasher = Sha1::new();
    hasher.update(input);
    let out: [u8; 20] = hasher.finalize().into();
    out.into()
}

/// SHA-256 of the given input.
#[inline]
pub fn sha256(input: &[u8]) -> H256 {
    let mut hasher = Sha256::new();
    hasher.update(input);
    let out: [u8; 32] = hasher.finalize().into();
    out.into()
}

/// Groestl-512 of the given input.
#[inline]
pub fn groestl512(input: &[u8]) -> H512 {
    let mut hasher = Groestl512::new();
    hasher.update(input);
    let out: [u8; 64] = hasher.finalize().into();
    out.into()
}

/// Keccak-256 of the given input (pre-NIST padding form, as used by
/// SmartCash and Ethereum).
#[inline]
pub fn keccak256(input: &[u8]) -> H256 {
    let mut hasher = Keccak256::new();
    hasher.update(input);
    let out: [u8; 32] = hasher.finalize().into();
    out.into()
}

/// Keccak-256 applied twice (KDF's `dkeccak256` helper).
#[inline]
pub fn dkeccak256(input: &[u8]) -> H256 { keccak256(keccak256(input).as_slice()) }

/// `RIPEMD160(SHA256(input))` — Bitcoin's `HASH160`.
#[inline]
pub fn dhash160(input: &[u8]) -> H160 { ripemd160(sha256(input).as_slice()) }

/// `SHA256(SHA256(input))` — Bitcoin's `HASH256`.
#[inline]
pub fn dhash256(input: &[u8]) -> H256 { sha256(sha256(input).as_slice()) }

/// `Groestl512(Groestl512(input))` — Groestlcoin's address checksum primitive.
#[inline]
pub fn dgroestl512(input: &[u8]) -> H512 { groestl512(groestl512(input).as_slice()) }

/// SipHash-2-4 keyed PRF.
#[inline]
pub fn siphash24(key0: u64, key1: u64, input: &[u8]) -> u64 {
    let mut hasher = SipHasher24::new_with_keys(key0, key1);
    hasher.write(input);
    hasher.finish()
}

/// Compute the canonical 4-byte address/payload checksum for the given
/// coin family.
///
/// `DSHA256`     → first 4 bytes of `dhash256(data)`.
/// `DGROESTL512` → first 4 bytes of `dgroestl512(data)`.
/// `KECCAK256`   → first 4 bytes of `keccak256(data)`.
#[inline]
pub fn checksum(data: &[u8], sum_type: &ChecksumType) -> H32 {
    let mut result = H32::default();
    match sum_type {
        ChecksumType::DSHA256 => result.copy_from_slice(&dhash256(data).as_slice()[0..4]),
        ChecksumType::DGROESTL512 => result.copy_from_slice(&dgroestl512(data).as_slice()[0..4]),
        ChecksumType::KECCAK256 => result.copy_from_slice(&keccak256(data).as_slice()[0..4]),
    }
    result
}

#[cfg(test)]
mod tests {
    //! Reference vectors below are the canonical outputs of standard test
    //! inputs (`b"hello"`, the SipHash-2-4 spec test vector, etc.). They
    //! match the previous bitcrypto baseline byte-for-byte and lock the
    //! KDF-defined `ChecksumType` dispatch in place.

    use super::*;
    use primitives::bytes::Bytes;

    #[test]
    fn ripemd160_hello() {
        let expected: H160 = "108f07b8382412612c048d07d13f814118445acd".into();
        assert_eq!(ripemd160(b"hello"), expected);
    }

    #[test]
    fn sha1_hello() {
        let expected: H160 = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into();
        assert_eq!(sha1(b"hello"), expected);
    }

    #[test]
    fn sha256_hello() {
        let expected: H256 = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into();
        assert_eq!(sha256(b"hello"), expected);
    }

    #[test]
    fn dhash160_hello_and_pubkey() {
        let expected: H160 = "b6a9c8c230722b7c748331a8b450f05566dc7d0f".into();
        assert_eq!(dhash160(b"hello"), expected);

        let expected: H160 = "865c71bfc7e314709207ab9e7e205c6f8e453d08".into();
        let bytes: Bytes =
            "210292be03ed9475445cc24a34a115c641a67e4ff234ccb08cb4c5cea45caa526cb26ead6ead6ead6ead6eadac".into();
        assert_eq!(dhash160(&bytes), expected);
    }

    #[test]
    fn dhash256_hello() {
        let expected: H256 = "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".into();
        assert_eq!(dhash256(b"hello"), expected);
    }

    #[test]
    fn siphash24_spec_vector() {
        // Aumasson & Bernstein SipHash-2-4 test vector: keys
        // 0x0706050403020100 / 0x0F0E0D0C0B0A0908 with input [0u8].
        let expected = 0x74f839c593dc67fd_u64;
        assert_eq!(siphash24(0x0706050403020100, 0x0F0E0D0C0B0A0908, &[0; 1]), expected);
    }

    #[test]
    fn checksum_dsha256_hello() {
        assert_eq!(checksum(b"hello", &ChecksumType::DSHA256), H32::from("9595c9df"));
    }
}
