//! Ethereum key operations: address derivation and signature recovery.
//!
//! LP-17: original implementation that drops the
//! `ethkey = artemii235/parity-ethereum.git` GPL-licensed dependency
//! in favour of the dual-licensed `secp256k1` and `tiny-keccak`
//! crates. The public API (`Address`, `Signature`, `EthKeyError`,
//! `address_from_uncompressed_pubkey`, `recover_public_key`) is
//! preserved for downstream consumers (`crypto::metamask_ctx`).

use mm2_err_handle::prelude::*;
use secp256k1::recovery::{RecoverableSignature, RecoveryId};
use secp256k1::{Message as SecpMessage, Secp256k1};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use tiny_keccak::{Hasher, Keccak};

pub use ethereum_types::{Address, H256, H520};

/// 65-byte ECDSA signature (`r || s || v`) with hex parsing.
#[derive(Clone)]
pub struct Signature([u8; 65]);

impl Signature {
    pub fn into_bytes(self) -> [u8; 65] { self.0 }
}

impl Default for Signature {
    fn default() -> Self { Signature([0u8; 65]) }
}

impl Deref for Signature {
    type Target = [u8; 65];
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl DerefMut for Signature {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl FromStr for Signature {
    type Err = EthKeyError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(s).map_err(|_| EthKeyError::InvalidSignature)?;
        if bytes.len() != 65 {
            return Err(EthKeyError::InvalidSignature);
        }
        let mut buf = [0u8; 65];
        buf.copy_from_slice(&bytes);
        Ok(Signature(buf))
    }
}

/// Error category preserved from the legacy `ethkey::Error` for downstream
/// match arms.
#[derive(Debug)]
pub enum EthKeyError {
    InvalidSignature,
    InvalidMessage,
    InvalidPublic,
}

impl fmt::Display for EthKeyError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            EthKeyError::InvalidSignature => f.write_str("Invalid EC signature"),
            EthKeyError::InvalidMessage => f.write_str("Invalid message"),
            EthKeyError::InvalidPublic => f.write_str("Invalid public"),
        }
    }
}

impl std::error::Error for EthKeyError {}

impl From<secp256k1::Error> for EthKeyError {
    fn from(_: secp256k1::Error) -> Self { EthKeyError::InvalidSignature }
}

/// Derives an Ethereum address from an uncompressed 65-byte ECDSA public key.
///
/// The first byte (0x04 prefix per SEC1) is stripped; the remaining 64 bytes
/// are Keccak-256-hashed and the last 20 bytes become the address.
pub fn address_from_uncompressed_pubkey(pubkey: H520) -> Address {
    let hash = keccak256(&AsRef::<[u8]>::as_ref(&pubkey)[1..65]);
    let mut addr = Address::default();
    AsMut::<[u8]>::as_mut(&mut addr).copy_from_slice(&hash[12..]);
    addr
}

/// Recovers the full uncompressed public key (H520, 65 bytes with 0x04 prefix)
/// from a message hash and its ECDSA signature.
///
/// Handles Ethereum's convention where the recovery id may have 27 added.
pub fn recover_public_key(message_hash: H256, mut sig: Signature) -> MmResult<H520, EthKeyError> {
    if sig[64] >= 27 {
        sig[64] -= 27;
    }

    let recovery_id = RecoveryId::from_i32(sig[64] as i32).map_to_mm(EthKeyError::from)?;
    let recoverable = RecoverableSignature::from_compact(&sig[0..64], recovery_id).map_to_mm(EthKeyError::from)?;
    let msg = SecpMessage::from_slice(AsRef::<[u8]>::as_ref(&message_hash)).map_to_mm(|_| EthKeyError::InvalidMessage)?;
    let pubkey = Secp256k1::new().recover(&msg, &recoverable).map_to_mm(EthKeyError::from)?;

    let serialized = pubkey.serialize_uncompressed(); // [u8; 65] starting with 0x04
    let mut out = H520::default();
    AsMut::<[u8]>::as_mut(&mut out).copy_from_slice(&serialized);
    Ok(out)
}

fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut out = [0u8; 32];
    hasher.update(bytes);
    hasher.finalize(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_from_known_pubkey() {
        // Uncompressed secp256k1 pubkey (65 bytes, 0x04 prefix).
        let pubkey_hex = concat!(
            "04",
            "e68acfc0253a10620dff706b0a1b1f1f5833ea3beb3bde2250d5f271f3563606",
            "672ebc45e0b7ea2e816ecb70ca03137b1c9476eec63d4632e990020b7b6fba39",
        );
        let pubkey = H520::from_str(pubkey_hex).unwrap();
        let addr = address_from_uncompressed_pubkey(pubkey);
        assert_ne!(addr, Address::default());
    }
}
