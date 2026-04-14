/// AES-256-CBC encryption with HMAC-SHA256 authentication.
///
/// Provides authenticated encryption for sensitive data like BIP39 mnemonics.
/// The encryption scheme:
/// 1. Encrypt plaintext with AES-256-CBC using a random IV
/// 2. Compute HMAC-SHA256 over (IV || ciphertext) for authentication
///
/// Always verify HMAC before decrypting (done in decrypt.rs).

use aes::Aes256;
use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
use hmac::{Hmac, Mac};
use sha2::Sha256;

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type HmacSha256 = Hmac<Sha256>;

/// Encrypted data bundle containing ciphertext, IV, and HMAC tag.
///
/// The HMAC covers (IV || ciphertext) to provide integrity and authenticity.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EncryptedData {
    /// AES-256-CBC ciphertext (PKCS7-padded)
    pub encrypted: Vec<u8>,
    /// 16-byte initialization vector used for CBC mode
    pub iv: Vec<u8>,
    /// HMAC-SHA256 tag over (IV || ciphertext)
    pub hmac: Vec<u8>,
}

/// Encrypts `data` using AES-256-CBC and produces an HMAC-SHA256 authentication tag.
///
/// # Arguments
/// * `data` - Plaintext bytes to encrypt
/// * `key` - 32-byte AES-256 encryption key
/// * `iv` - 16-byte initialization vector (must be randomly generated per encryption)
/// * `hmac_key` - 32-byte HMAC authentication key (must differ from encryption key)
///
/// # Security
/// - The IV must be generated from a cryptographically secure random source.
/// - The encryption key and HMAC key must be derived independently.
pub fn encrypt_data(data: &[u8], key: &[u8; 32], iv: &[u8; 16], hmac_key: &[u8; 32]) -> EncryptedData {
    // Encrypt with AES-256-CBC + PKCS7 padding
    let encrypted = Aes256CbcEnc::new(key.into(), iv.into()).encrypt_padded_vec_mut::<Pkcs7>(data);

    // Compute HMAC-SHA256 over (IV || ciphertext) for authenticated encryption
    let mut mac =
        HmacSha256::new_from_slice(hmac_key).expect("HMAC-SHA256 accepts any key length, 32 bytes is always valid");
    mac.update(iv);
    mac.update(&encrypted);
    let hmac = mac.finalize().into_bytes().to_vec();

    EncryptedData {
        encrypted,
        iv: iv.to_vec(),
        hmac,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_data_produces_valid_structure() {
        let data = b"test plaintext data";
        let key = [0x42u8; 32];
        let iv = [0x13u8; 16];
        let hmac_key = [0x37u8; 32];

        let result = encrypt_data(data, &key, &iv, &hmac_key);

        // Ciphertext should be padded to AES block boundary (16 bytes)
        assert!(!result.encrypted.is_empty());
        assert_eq!(result.encrypted.len() % 16, 0);
        assert_eq!(result.iv.len(), 16);
        assert_eq!(result.hmac.len(), 32); // SHA-256 output
    }

    #[test]
    fn test_encrypt_different_ivs_produce_different_ciphertext() {
        let data = b"same plaintext";
        let key = [0x42u8; 32];
        let hmac_key = [0x37u8; 32];

        let result1 = encrypt_data(data, &key, &[0x01u8; 16], &hmac_key);
        let result2 = encrypt_data(data, &key, &[0x02u8; 16], &hmac_key);

        assert_ne!(result1.encrypted, result2.encrypted);
        assert_ne!(result1.hmac, result2.hmac);
    }
}
