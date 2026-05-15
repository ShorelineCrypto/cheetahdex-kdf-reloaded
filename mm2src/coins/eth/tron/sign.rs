//! TRON transaction signing.
//!
//! TRON uses SHA-256 (not keccak256) to hash the protobuf-encoded transaction
//! raw data, then signs with secp256k1. The resulting 65-byte signature is
//! R(32) || S(32) || V(1), where V ∈ {0, 1}.

use derive_more::Display;
use ethereum_types::H256;
use ethkey::{sign, Secret, Signature as EthSignature};
use prost::Message;

use super::proto::TransactionRaw;

/// Errors during TRON transaction signing.
#[derive(Debug, Display)]
pub enum TronSignError {
    #[display(fmt = "Failed to encode transaction raw data")]
    EncodeError,
    #[display(fmt = "Signing failed: {}", _0)]
    SigningFailed(String),
}

impl std::error::Error for TronSignError {}

/// SHA-256 hash of protobuf-encoded `TransactionRaw`.
///
/// This is the "txID" in TRON's terminology and the message that gets signed.
pub fn hash_transaction_raw(raw: &TransactionRaw) -> H256 {
    use sha2::{Digest, Sha256};
    let encoded = raw.encode_to_vec();
    let hash = Sha256::digest(&encoded);
    H256::from_slice(&hash)
}

/// Sign a TRON transaction hash with the given secret key.
///
/// Returns a 65-byte signature: R(32) || S(32) || V(1), V ∈ {0, 1}.
pub fn sign_tron_hash(secret: &Secret, tx_hash: &H256) -> Result<Vec<u8>, TronSignError> {
    let sig: EthSignature = sign(secret, tx_hash).map_err(|e| TronSignError::SigningFailed(e.to_string()))?;
    // ethkey::Signature stores [r(32) | s(32) | v(1)] with v ∈ {0, 1},
    // which matches TRON's expected format.
    let mut bytes = vec![0u8; 65];
    bytes[..32].copy_from_slice(sig.r());
    bytes[32..64].copy_from_slice(sig.s());
    bytes[64] = sig.v();
    Ok(bytes)
}

/// Convenience: hash a `TransactionRaw` and sign it in one step.
pub fn sign_transaction_raw(secret: &Secret, raw: &TransactionRaw) -> Result<(H256, Vec<u8>), TronSignError> {
    let tx_hash = hash_transaction_raw(raw);
    let sig = sign_tron_hash(secret, &tx_hash)?;
    Ok((tx_hash, sig))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ethkey::KeyPair;
    use std::str::FromStr;

    fn test_keypair() -> KeyPair {
        // Well-known test private key (DO NOT use in production).
        let secret =
            Secret::from_str("0000000000000000000000000000000000000000000000000000000000000001").expect("valid secret");
        KeyPair::from_secret(secret).expect("valid keypair")
    }

    #[test]
    fn test_hash_transaction_raw_deterministic() {
        let raw = TransactionRaw {
            ref_block_bytes: vec![0x01, 0x02],
            ref_block_hash: vec![0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a],
            expiration: 1700000000000,
            contract: vec![],
            timestamp: 1699999900000,
            fee_limit: 0,
        };
        let h1 = hash_transaction_raw(&raw);
        let h2 = hash_transaction_raw(&raw);
        assert_eq!(h1, h2, "same input must produce same hash");
        assert_ne!(h1, H256::zero(), "hash should not be zero");
    }

    #[test]
    fn test_hash_changes_with_input() {
        let raw1 = TransactionRaw {
            ref_block_bytes: vec![0x01, 0x02],
            ref_block_hash: vec![0; 8],
            expiration: 1700000000000,
            contract: vec![],
            timestamp: 1700000000000,
            fee_limit: 0,
        };
        let mut raw2 = raw1.clone();
        raw2.timestamp = 1700000000001; // change one field
        assert_ne!(hash_transaction_raw(&raw1), hash_transaction_raw(&raw2));
    }

    #[test]
    fn test_sign_tron_hash_produces_65_bytes() {
        let kp = test_keypair();
        let hash = H256::from_slice(&[0xAB; 32]);
        let sig = sign_tron_hash(kp.secret(), &hash).unwrap();
        assert_eq!(sig.len(), 65);
        // V must be 0 or 1.
        assert!(sig[64] == 0 || sig[64] == 1);
    }

    #[test]
    fn test_sign_transaction_raw_roundtrip() {
        let kp = test_keypair();
        let raw = TransactionRaw {
            ref_block_bytes: vec![0xAB, 0xCD],
            ref_block_hash: vec![1, 2, 3, 4, 5, 6, 7, 8],
            expiration: 1700000060000,
            contract: vec![],
            timestamp: 1700000000000,
            fee_limit: 100_000_000,
        };
        let (tx_hash, sig) = sign_transaction_raw(kp.secret(), &raw).unwrap();

        // The tx_hash should equal re-computing the hash.
        assert_eq!(tx_hash, hash_transaction_raw(&raw));

        // Signature should be valid 65 bytes.
        assert_eq!(sig.len(), 65);
        assert!(sig[64] == 0 || sig[64] == 1);
    }

    #[test]
    fn test_different_keys_produce_different_signatures() {
        let hash = H256::from_slice(&[0x42; 32]);

        let s1 = Secret::from_str("0000000000000000000000000000000000000000000000000000000000000001").expect("valid");
        let s2 = Secret::from_str("0000000000000000000000000000000000000000000000000000000000000002").expect("valid");

        let sig1 = sign_tron_hash(&s1, &hash).unwrap();
        let sig2 = sign_tron_hash(&s2, &hash).unwrap();
        assert_ne!(sig1, sig2);
    }
}
