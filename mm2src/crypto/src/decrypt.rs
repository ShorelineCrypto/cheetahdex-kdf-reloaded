/// AES-256-CBC decryption with HMAC-SHA256 verification.
///
/// Counterpart to encrypt.rs. Always verifies the HMAC tag before decrypting
/// to prevent padding oracle and other attacks.
use crate::encrypt::EncryptedData;
use aes::Aes256;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use derive_more::Display;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type Aes256CbcDec = cbc::Decryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

/// Errors that can occur during decryption.
#[derive(Debug, Display)]
pub enum DecryptionError {
    /// HMAC verification failed — data may have been tampered with.
    #[display(fmt = "HMAC verification failed: data integrity check failed")]
    HmacMismatch,
    /// IV has incorrect length (expected 16 bytes for AES).
    #[display(fmt = "Invalid IV length: expected 16 bytes, got {}", _0)]
    InvalidIvLength(usize),
    /// AES-CBC decryption failed (e.g., invalid padding).
    #[display(fmt = "Decryption failed: invalid padding or corrupted ciphertext")]
    DecryptionFailed,
}

/// Decrypts data that was encrypted with [`crate::encrypt::encrypt_data`].
///
/// Performs HMAC-SHA256 verification before decryption to ensure data integrity.
/// This is critical to prevent padding oracle attacks.
///
/// # Arguments
/// * `encrypted_data` - The encrypted bundle (ciphertext, IV, HMAC tag)
/// * `key` - 32-byte AES-256 decryption key (same as used for encryption)
/// * `hmac_key` - 32-byte HMAC key (same as used for encryption)
///
/// # Errors
/// Returns `DecryptionError` if HMAC verification fails or decryption fails.
pub fn decrypt_data(
    encrypted_data: &EncryptedData,
    key: &[u8; 32],
    hmac_key: &[u8; 32],
) -> Result<Vec<u8>, DecryptionError> {
    // Step 1: Verify HMAC before any decryption (prevents padding oracle attacks)
    let mut mac =
        HmacSha256::new_from_slice(hmac_key).expect("HMAC-SHA256 accepts any key length, 32 bytes is always valid");
    mac.update(&encrypted_data.iv);
    mac.update(&encrypted_data.encrypted);
    mac.verify_slice(&encrypted_data.hmac)
        .map_err(|_| DecryptionError::HmacMismatch)?;

    // Step 2: Extract 16-byte IV
    let iv: [u8; 16] = encrypted_data
        .iv
        .as_slice()
        .try_into()
        .map_err(|_| DecryptionError::InvalidIvLength(encrypted_data.iv.len()))?;

    // Step 3: Decrypt with AES-256-CBC + PKCS7 unpadding
    Aes256CbcDec::new(key.into(), &iv.into())
        .decrypt_padded_vec_mut::<Pkcs7>(&encrypted_data.encrypted)
        .map_err(|_| DecryptionError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encrypt::encrypt_data;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let plaintext = b"Hello, HD wallet world!";
        let key = [0x42u8; 32];
        let iv = [0x13u8; 16];
        let hmac_key = [0x37u8; 32];

        let encrypted = encrypt_data(plaintext, &key, &iv, &hmac_key);
        let decrypted = decrypt_data(&encrypted, &key, &hmac_key).expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_fails_with_wrong_hmac_key() {
        let plaintext = b"sensitive data";
        let key = [0x42u8; 32];
        let iv = [0x13u8; 16];
        let hmac_key = [0x37u8; 32];
        let wrong_hmac_key = [0x99u8; 32];

        let encrypted = encrypt_data(plaintext, &key, &iv, &hmac_key);
        let result = decrypt_data(&encrypted, &key, &wrong_hmac_key);

        assert!(matches!(result, Err(DecryptionError::HmacMismatch)));
    }

    #[test]
    fn test_decrypt_fails_with_tampered_ciphertext() {
        let plaintext = b"do not tamper";
        let key = [0x42u8; 32];
        let iv = [0x13u8; 16];
        let hmac_key = [0x37u8; 32];

        let mut encrypted = encrypt_data(plaintext, &key, &iv, &hmac_key);
        // Tamper with the ciphertext
        if let Some(byte) = encrypted.encrypted.first_mut() {
            *byte ^= 0xff;
        }
        let result = decrypt_data(&encrypted, &key, &hmac_key);

        assert!(matches!(result, Err(DecryptionError::HmacMismatch)));
    }

    #[test]
    fn test_decrypt_empty_plaintext() {
        let plaintext = b"";
        let key = [0x42u8; 32];
        let iv = [0x13u8; 16];
        let hmac_key = [0x37u8; 32];

        let encrypted = encrypt_data(plaintext, &key, &iv, &hmac_key);
        let decrypted = decrypt_data(&encrypted, &key, &hmac_key).expect("decryption should succeed");

        assert_eq!(decrypted, plaintext);
    }
}
