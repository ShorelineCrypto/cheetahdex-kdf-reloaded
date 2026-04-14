/// BIP39 mnemonic generation, encryption, and decryption.
///
/// Provides utility functions for:
/// - Generating new BIP39 mnemonics with configurable word count
/// - Encrypting mnemonics with a password (Argon2) or seed (SLIP-0021)
/// - Decrypting previously encrypted mnemonics

use crate::decrypt::decrypt_data;
use crate::encrypt::{encrypt_data, EncryptedData};
use crate::key_derivation::{derive_keys_for_mnemonic, Argon2Params, KeyDerivationDetails, KeyDerivationError};
use bip39::Mnemonic;
use derive_more::Display;

/// Errors related to mnemonic operations.
#[derive(Debug, Display)]
pub enum MnemonicError {
    #[display(fmt = "Failed to generate mnemonic: {}", _0)]
    GenerationError(String),
    #[display(fmt = "Invalid mnemonic: {}", _0)]
    InvalidMnemonic(String),
    #[display(fmt = "Key derivation failed: {}", _0)]
    KeyDerivationFailed(KeyDerivationError),
    #[display(fmt = "Encryption failed: {}", _0)]
    EncryptionError(String),
    #[display(fmt = "Decryption failed: {}", _0)]
    DecryptionError(String),
}

impl From<KeyDerivationError> for MnemonicError {
    fn from(e: KeyDerivationError) -> Self {
        MnemonicError::KeyDerivationFailed(e)
    }
}

/// Encrypted mnemonic bundle with the key derivation details needed for decryption.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EncryptedMnemonicData {
    /// The encrypted mnemonic ciphertext, IV, and HMAC
    pub encrypted_data: EncryptedData,
    /// Key derivation method and parameters used during encryption
    pub key_derivation: KeyDerivationDetails,
}

/// Generates a new BIP39 mnemonic phrase.
///
/// # Arguments
/// * `word_count` - Number of words (12, 15, 18, 21, or 24). Must correspond to
///   valid BIP39 entropy sizes (128, 160, 192, 224, or 256 bits).
pub fn generate_mnemonic(word_count: usize) -> Result<Mnemonic, MnemonicError> {
    let entropy_bits = word_count_to_entropy_bits(word_count)?;
    let mut entropy = vec![0u8; entropy_bits / 8];
    common::os_rng(&mut entropy)
        .map_err(|e| MnemonicError::GenerationError(format!("RNG error: {e}")))?;
    Mnemonic::from_entropy(&entropy)
        .map_err(|e| MnemonicError::GenerationError(e.to_string()))
}

/// Encrypts a mnemonic phrase using a password with Argon2 key derivation.
///
/// # Arguments
/// * `mnemonic_str` - The BIP39 mnemonic phrase to encrypt
/// * `password` - User-provided password for encryption
///
/// # Security
/// Uses Argon2id with parameters tuned for reasonable security vs. performance tradeoff.
/// The salt is randomly generated per encryption.
pub fn encrypt_mnemonic(
    mnemonic_str: &str,
    password: &str,
) -> Result<EncryptedMnemonicData, MnemonicError> {
    // Validate the mnemonic first
    let _ = Mnemonic::parse_in_normalized(bip39::Language::English, mnemonic_str)
        .map_err(|e| MnemonicError::InvalidMnemonic(e.to_string()))?;

    // Generate random salt for Argon2
    let mut salt = [0u8; 32];
    common::os_rng(&mut salt)
        .map_err(|e| MnemonicError::EncryptionError(format!("RNG error: {e}")))?;

    // Default Argon2 parameters — balanced for security and performance
    let argon2_params = Argon2Params {
        memory_cost_kib: 65536, // 64 MiB
        iterations: 3,
        parallelism: 1,
    };

    let details = KeyDerivationDetails::Argon2 {
        params: argon2_params,
        salt: salt.to_vec(),
    };

    let keys = derive_keys_for_mnemonic(password.as_bytes(), &details)?;

    // Generate random IV for AES-CBC
    let mut iv = [0u8; 16];
    common::os_rng(&mut iv)
        .map_err(|e| MnemonicError::EncryptionError(format!("RNG error: {e}")))?;

    let encrypted_data = encrypt_data(
        mnemonic_str.as_bytes(),
        &keys.encryption_key,
        &iv,
        &keys.hmac_key,
    );

    Ok(EncryptedMnemonicData {
        encrypted_data,
        key_derivation: details,
    })
}

/// Decrypts a mnemonic phrase that was encrypted with [`encrypt_mnemonic`].
///
/// # Arguments
/// * `encrypted` - The encrypted mnemonic bundle
/// * `password` - The password used during encryption
pub fn decrypt_mnemonic(
    encrypted: &EncryptedMnemonicData,
    password: &str,
) -> Result<String, MnemonicError> {
    let keys = derive_keys_for_mnemonic(password.as_bytes(), &encrypted.key_derivation)?;

    let decrypted = decrypt_data(&encrypted.encrypted_data, &keys.encryption_key, &keys.hmac_key)
        .map_err(|e| MnemonicError::DecryptionError(e.to_string()))?;

    String::from_utf8(decrypted)
        .map_err(|e| MnemonicError::DecryptionError(format!("Invalid UTF-8 in decrypted mnemonic: {e}")))
}

/// Converts BIP39 word count to required entropy bit count.
fn word_count_to_entropy_bits(word_count: usize) -> Result<usize, MnemonicError> {
    // BIP39: words = (entropy_bits + checksum_bits) / 11
    //        checksum_bits = entropy_bits / 32
    // So: words * 11 = entropy_bits * 33/32
    //     entropy_bits = words * 11 * 32 / 33
    match word_count {
        12 => Ok(128),
        15 => Ok(160),
        18 => Ok(192),
        21 => Ok(224),
        24 => Ok(256),
        _ => Err(MnemonicError::GenerationError(format!(
            "Invalid word count: {word_count}. Must be 12, 15, 18, 21, or 24"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_mnemonic_12_words() {
        let mnemonic = generate_mnemonic(12).expect("should generate 12-word mnemonic");
        assert_eq!(mnemonic.word_count(), 12);
    }

    #[test]
    fn test_generate_mnemonic_24_words() {
        let mnemonic = generate_mnemonic(24).expect("should generate 24-word mnemonic");
        assert_eq!(mnemonic.word_count(), 24);
    }

    #[test]
    fn test_generate_mnemonic_invalid_count() {
        assert!(generate_mnemonic(13).is_err());
    }

    #[test]
    fn test_encrypt_decrypt_mnemonic_roundtrip() {
        let mnemonic = generate_mnemonic(12).unwrap();
        let mnemonic_str = mnemonic.to_string();
        let password = "test_password_123";

        let encrypted = encrypt_mnemonic(&mnemonic_str, password)
            .expect("encryption should succeed");
        let decrypted = decrypt_mnemonic(&encrypted, password)
            .expect("decryption should succeed");

        assert_eq!(decrypted, mnemonic_str);
    }

    #[test]
    fn test_decrypt_with_wrong_password_fails() {
        let mnemonic = generate_mnemonic(12).unwrap();
        let mnemonic_str = mnemonic.to_string();

        let encrypted = encrypt_mnemonic(&mnemonic_str, "correct_password")
            .expect("encryption should succeed");
        let result = decrypt_mnemonic(&encrypted, "wrong_password");

        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_invalid_mnemonic_fails() {
        let result = encrypt_mnemonic("not a valid mnemonic phrase at all", "password");
        assert!(result.is_err());
    }
}
