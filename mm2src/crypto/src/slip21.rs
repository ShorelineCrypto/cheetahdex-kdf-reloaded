/// SLIP-0021 symmetric key derivation from master secret.
///
/// Implements [SLIP-0021](https://github.com/satoshilabs/slips/blob/master/slip-0021.md)
/// for deriving symmetric keys from a BIP39 seed using HMAC-SHA512.
///
/// Used to derive encryption/authentication keys for mnemonic self-encryption,
/// where the mnemonic's own seed protects the encrypted mnemonic.
use derive_more::Display;
use hmac::{Hmac, Mac};
use sha2::Sha512;

type HmacSha512 = Hmac<Sha512>;

/// SLIP-0021 key path for mnemonic encryption key.
pub const ENCRYPTION_KEY_PATH: [&str; 2] = ["SLIP-0021", "Encryption key"];

/// SLIP-0021 key path for mnemonic authentication (HMAC) key.
pub const AUTHENTICATION_KEY_PATH: [&str; 2] = ["SLIP-0021", "Authentication key"];

/// Errors during SLIP-0021 key derivation.
#[derive(Debug, Display)]
pub enum Slip21Error {
    #[display(fmt = "SLIP-0021 derivation error: {}", _0)]
    DerivationError(String),
}

/// Derives a 32-byte symmetric key using the SLIP-0021 hierarchical derivation scheme.
///
/// Starting from a master seed, derives a key by iterating through each path label:
/// 1. Compute master node: HMAC-SHA512(key="Symmetric key seed", data=seed)
/// 2. For each label in path: HMAC-SHA512(key=parent_key, data=0x00 || label_bytes)
/// 3. Return the first 32 bytes of the final HMAC-SHA512 output as the derived key.
///
/// # Arguments
/// * `seed` - BIP39 seed bytes (typically 64 bytes)
/// * `path` - Slice of string labels defining the derivation path
pub fn derive_key_from_path(seed: &[u8], path: &[&str]) -> Result<[u8; 32], Slip21Error> {
    // Master node: HMAC-SHA512(key="Symmetric key seed", data=seed)
    let mut mac = HmacSha512::new_from_slice(b"Symmetric key seed")
        .expect("HMAC-SHA512 accepts any key length; static key is always valid");
    mac.update(seed);
    let mut node = mac.finalize().into_bytes();

    // Iterate through path labels, deriving child nodes
    for label in path {
        let mut child_mac =
            HmacSha512::new_from_slice(&node[..32]).map_err(|e| Slip21Error::DerivationError(e.to_string()))?;
        // Each child derivation prepends 0x00 to the label
        child_mac.update(&[0x00]);
        child_mac.update(label.as_bytes());
        node = child_mac.finalize().into_bytes();
    }

    // Return first 32 bytes as the derived symmetric key
    let mut key = [0u8; 32];
    key.copy_from_slice(&node[..32]);
    Ok(key)
}

/// Encrypts mnemonic data using SLIP-0021 derived keys.
///
/// This provides deterministic encryption where the mnemonic's own seed
/// is used to derive the encryption keys.
pub fn encrypt_with_slip21(mnemonic_data: &[u8], seed: &[u8]) -> Result<crate::encrypt::EncryptedData, Slip21Error> {
    let encryption_key = derive_key_from_path(seed, &ENCRYPTION_KEY_PATH)?;
    let authentication_key = derive_key_from_path(seed, &AUTHENTICATION_KEY_PATH)?;

    // Generate random IV
    let mut iv = [0u8; 16];
    common::os_rng(&mut iv).map_err(|e| Slip21Error::DerivationError(format!("RNG error: {e}")))?;

    Ok(crate::encrypt::encrypt_data(
        mnemonic_data,
        &encryption_key,
        &iv,
        &authentication_key,
    ))
}

/// Decrypts mnemonic data using SLIP-0021 derived keys.
pub fn decrypt_with_slip21(
    encrypted_data: &crate::encrypt::EncryptedData,
    seed: &[u8],
) -> Result<Vec<u8>, Slip21Error> {
    let encryption_key = derive_key_from_path(seed, &ENCRYPTION_KEY_PATH)?;
    let authentication_key = derive_key_from_path(seed, &AUTHENTICATION_KEY_PATH)?;

    crate::decrypt::decrypt_data(encrypted_data, &encryption_key, &authentication_key)
        .map_err(|e| Slip21Error::DerivationError(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slip21_derive_key_deterministic() {
        let seed = [0x42u8; 64];
        let key1 = derive_key_from_path(&seed, &ENCRYPTION_KEY_PATH).unwrap();
        let key2 = derive_key_from_path(&seed, &ENCRYPTION_KEY_PATH).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_slip21_different_paths_different_keys() {
        let seed = [0x42u8; 64];
        let enc_key = derive_key_from_path(&seed, &ENCRYPTION_KEY_PATH).unwrap();
        let auth_key = derive_key_from_path(&seed, &AUTHENTICATION_KEY_PATH).unwrap();
        assert_ne!(enc_key, auth_key);
    }

    #[test]
    fn test_slip21_encrypt_decrypt_roundtrip() {
        let mnemonic = b"abandon abandon abandon abandon abandon about";
        let seed = [0x42u8; 64];

        let encrypted = encrypt_with_slip21(mnemonic, &seed).unwrap();
        let decrypted = decrypt_with_slip21(&encrypted, &seed).unwrap();

        assert_eq!(decrypted, mnemonic);
    }
}
