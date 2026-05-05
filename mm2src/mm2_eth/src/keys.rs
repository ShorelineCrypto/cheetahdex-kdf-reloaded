//! Ethereum key operations: address derivation and signature recovery.

pub use ethereum_types::{H256, H520};
use mm2_err_handle::prelude::*;

pub use ethkey::{Address, Error as EthKeyError, Signature};

/// Derives an Ethereum address from an uncompressed 65-byte ECDSA public key.
///
/// The first byte (0x04 prefix per SEC1) is stripped; the remaining 64 bytes
/// are Keccak-256-hashed and the last 20 bytes become the address.
pub fn address_from_uncompressed_pubkey(pubkey: H520) -> Address {
    #[allow(deprecated)] // old ethereum-types API
    let public = ethkey::Public::from_slice(&pubkey[1..65]);
    ethkey::public_to_address(&public)
}

/// Recovers the full uncompressed public key (H520, 65 bytes with 0x04 prefix)
/// from a message hash and its ECDSA signature.
///
/// Handles Ethereum's convention where the recovery id may have 27 added.
pub fn recover_public_key(message_hash: H256, mut sig: Signature) -> MmResult<H520, EthKeyError> {
    // Ethereum signatures sometimes encode v as 27/28 instead of 0/1.
    if sig[64] >= 27 {
        sig[64] -= 27;
    }

    let pubkey = ethkey::recover(&sig, &message_hash)?;

    // ethkey::recover returns H512 (64 bytes, x||y). Prepend the SEC1
    // uncompressed prefix byte to form a full H520.
    let mut uncompressed = H520::default();
    uncompressed[0] = 0x04;
    uncompressed[1..65].copy_from_slice(pubkey.as_ref());
    Ok(uncompressed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
