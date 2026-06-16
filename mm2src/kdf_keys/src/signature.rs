// ECDSA signature wrappers.
//
// `Signature` — DER-serialised, variable length.
// `CompactSignature` — 65-byte recoverable signature
//   (1-byte recovery id + compressed flag + 64-byte (r||s)).

use crate::hash::H520;
use crate::Error;
use rustc_hex::{FromHex, ToHex};
use std::{fmt, ops, str};

#[derive(Clone, PartialEq, Eq)]
pub struct Signature(Vec<u8>);

impl Signature {
    /// BIP-62 low-S check (kept as a panicking placeholder — no caller
    /// in the workspace exercises it).
    pub fn check_low_s(&self) -> bool { unimplemented!("Signature::check_low_s not used by KDF") }
}

impl fmt::Debug for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0.to_hex::<String>()) }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0.to_hex::<String>()) }
}

impl ops::Deref for Signature {
    type Target = [u8];
    fn deref(&self) -> &[u8] { &self.0 }
}

impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] { &self.0 }
}

impl str::FromStr for Signature {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        s.from_hex::<Vec<u8>>()
            .map(Signature)
            .map_err(|_| Error::InvalidSignature)
    }
}

impl From<&'static str> for Signature {
    fn from(s: &'static str) -> Self { s.parse().expect("valid hex literal") }
}

impl From<Vec<u8>> for Signature {
    fn from(v: Vec<u8>) -> Self { Signature(v) }
}

impl From<Signature> for Vec<u8> {
    fn from(s: Signature) -> Vec<u8> { s.0 }
}

impl<'a> From<&'a [u8]> for Signature {
    fn from(v: &'a [u8]) -> Self { Signature(v.to_vec()) }
}

#[derive(PartialEq, Eq)]
pub struct CompactSignature(H520);

impl fmt::Debug for CompactSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0.to_hex::<String>()) }
}

impl fmt::Display for CompactSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(&self.0.to_hex::<String>()) }
}

impl ops::Deref for CompactSignature {
    type Target = [u8];
    fn deref(&self) -> &[u8] { &*self.0 }
}

impl str::FromStr for CompactSignature {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> { s.parse().map(CompactSignature).map_err(|_| Error::InvalidSignature) }
}

impl From<&'static str> for CompactSignature {
    fn from(s: &'static str) -> Self { s.parse().expect("valid hex literal") }
}

impl From<H520> for CompactSignature {
    fn from(h: H520) -> Self { CompactSignature(h) }
}

impl From<Vec<u8>> for CompactSignature {
    fn from(v: Vec<u8>) -> Self { CompactSignature(H520::from(&v[..])) }
}
