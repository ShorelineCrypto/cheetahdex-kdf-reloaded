/// Key derivation functions for mnemonic encryption.
///
/// Supports two key derivation methods:
/// - **Argon2**: Memory-hard password-based KDF, suitable for user-provided passwords
/// - **SLIP-0021**: Deterministic symmetric-key derivation from a master seed
///
/// Both methods derive a pair of keys: one for AES encryption and one for HMAC authentication.
use crate::slip21;
use derive_more::Display;
use zeroize::Zeroize;

/// Parameters for Argon2 key derivation.
///
/// These should be tuned for the target platform. Higher values increase
/// resistance to brute-force attacks but also increase computation time.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Argon2Params {
    /// Memory cost in KiB (e.g., 65536 = 64 MiB)
    pub memory_cost_kib: u32,
    /// Number of iterations (time cost)
    pub iterations: u32,
    /// Degree of parallelism
    pub parallelism: u32,
}

/// Specifies which key derivation method to use for mnemonic encryption.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum KeyDerivationDetails {
    /// Argon2id password-based key derivation.
    /// Used when encrypting mnemonic with a user-provided password.
    #[serde(rename = "argon2")]
    Argon2 { params: Argon2Params, salt: Vec<u8> },
    /// SLIP-0021 deterministic key derivation from a master seed.
    /// Used when encrypting mnemonic with its own seed (self-encryption).
    #[serde(rename = "slip0021")]
    SLIP0021,
}

/// Errors during key derivation.
#[derive(Debug, Display)]
pub enum KeyDerivationError {
    #[display(fmt = "Argon2 key derivation failed: {}", _0)]
    Argon2Error(String),
    #[display(fmt = "SLIP-0021 key derivation failed: {}", _0)]
    Slip0021Error(String),
}

/// A pair of derived keys: one for encryption and one for HMAC authentication.
/// Both keys are zeroized on drop.
pub struct DerivedKeys {
    pub encryption_key: [u8; 32],
    pub hmac_key: [u8; 32],
}

impl Drop for DerivedKeys {
    fn drop(&mut self) {
        self.encryption_key.zeroize();
        self.hmac_key.zeroize();
    }
}

/// Derives encryption and HMAC keys for mnemonic encryption/decryption.
///
/// # Arguments
/// * `password_or_seed` - Either a user password (Argon2) or BIP39 seed bytes (SLIP-0021)
/// * `details` - The key derivation method and parameters
///
/// # Returns
/// A `DerivedKeys` struct containing the 32-byte encryption key and 32-byte HMAC key.
pub fn derive_keys_for_mnemonic(
    password_or_seed: &[u8],
    details: &KeyDerivationDetails,
) -> Result<DerivedKeys, KeyDerivationError> {
    match details {
        KeyDerivationDetails::Argon2 { params, salt } => {
            derive_encryption_authentication_keys_argon2(password_or_seed, params, salt)
        },
        KeyDerivationDetails::SLIP0021 => derive_encryption_authentication_keys_slip0021(password_or_seed),
    }
}

/// Derives keys using Argon2id (memory-hard password-based KDF).
///
/// Produces 64 bytes of output: first 32 for encryption, last 32 for HMAC.
fn derive_encryption_authentication_keys_argon2(
    password: &[u8],
    params: &Argon2Params,
    salt: &[u8],
) -> Result<DerivedKeys, KeyDerivationError> {
    use argon2::{Algorithm, Argon2, Params, Version};

    let argon2_params = Params::new(params.memory_cost_kib, params.iterations, params.parallelism, Some(64))
        .map_err(|e| KeyDerivationError::Argon2Error(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    let mut output = [0u8; 64];
    argon2
        .hash_password_into(password, salt, &mut output)
        .map_err(|e| KeyDerivationError::Argon2Error(e.to_string()))?;

    let mut encryption_key = [0u8; 32];
    let mut hmac_key = [0u8; 32];
    encryption_key.copy_from_slice(&output[..32]);
    hmac_key.copy_from_slice(&output[32..]);
    output.zeroize();

    Ok(DerivedKeys {
        encryption_key,
        hmac_key,
    })
}

/// Derives keys using SLIP-0021 deterministic symmetric-key derivation.
///
/// Uses two different SLIP-0021 key paths to derive independent encryption and HMAC keys.
fn derive_encryption_authentication_keys_slip0021(seed: &[u8]) -> Result<DerivedKeys, KeyDerivationError> {
    let encryption_key = slip21::derive_key_from_path(seed, &slip21::ENCRYPTION_KEY_PATH)
        .map_err(|e| KeyDerivationError::Slip0021Error(e.to_string()))?;
    let hmac_key = slip21::derive_key_from_path(seed, &slip21::AUTHENTICATION_KEY_PATH)
        .map_err(|e| KeyDerivationError::Slip0021Error(e.to_string()))?;

    Ok(DerivedKeys {
        encryption_key,
        hmac_key,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_key_derivation() {
        let password = b"test password";
        let salt = b"random_salt_1234";
        let params = Argon2Params {
            memory_cost_kib: 1024, // Low for tests
            iterations: 1,
            parallelism: 1,
        };
        let details = KeyDerivationDetails::Argon2 {
            params,
            salt: salt.to_vec(),
        };

        let keys = derive_keys_for_mnemonic(password, &details).expect("Argon2 derivation should succeed");

        // Keys should be different from each other
        assert_ne!(keys.encryption_key, keys.hmac_key);
        // Keys should be non-zero
        assert_ne!(keys.encryption_key, [0u8; 32]);
        assert_ne!(keys.hmac_key, [0u8; 32]);
    }

    #[test]
    fn test_argon2_deterministic() {
        let password = b"test password";
        let salt = b"saltsalt"; // Argon2 requires at least 8 bytes of salt
        let params = Argon2Params {
            memory_cost_kib: 1024,
            iterations: 1,
            parallelism: 1,
        };
        let details = KeyDerivationDetails::Argon2 {
            params,
            salt: salt.to_vec(),
        };

        let keys1 = derive_keys_for_mnemonic(password, &details).unwrap();
        let keys2 = derive_keys_for_mnemonic(password, &details).unwrap();

        assert_eq!(keys1.encryption_key, keys2.encryption_key);
        assert_eq!(keys1.hmac_key, keys2.hmac_key);
    }

    #[test]
    fn test_slip0021_key_derivation() {
        let seed = [0xABu8; 64]; // Simulated BIP39 seed
        let details = KeyDerivationDetails::SLIP0021;

        let keys = derive_keys_for_mnemonic(&seed, &details).expect("SLIP-0021 derivation should succeed");

        assert_ne!(keys.encryption_key, keys.hmac_key);
        assert_ne!(keys.encryption_key, [0u8; 32]);
    }
}
