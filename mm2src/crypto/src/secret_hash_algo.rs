/// Secret hashing algorithm selection for HTLC (Hash Time Locked Contract) swaps.
///
/// Atomic swaps use hash locks where the secret must be hashed to create the lock.
/// Different chains may require different hash algorithms for their scripts.

use bitcrypto::{dhash160, sha256};

/// Available hash algorithms for creating HTLC secret hashes.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum SecretHashAlgo {
    /// RIPEMD160(SHA256(secret)) — traditional Bitcoin-style double hash.
    /// Produces a 20-byte hash. Used by most UTXO coins.
    DHASH160,
    /// SHA256(secret) — single SHA-256 hash.
    /// Produces a 32-byte hash. Used by some newer protocols and EVM chains.
    SHA256,
}

impl SecretHashAlgo {
    /// Hashes the given secret using the selected algorithm.
    pub fn hash_secret(&self, secret: &[u8]) -> Vec<u8> {
        match self {
            SecretHashAlgo::DHASH160 => dhash160(secret).take().to_vec(),
            SecretHashAlgo::SHA256 => sha256(secret).take().to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dhash160_output_length() {
        let hash = SecretHashAlgo::DHASH160.hash_secret(b"test secret");
        assert_eq!(hash.len(), 20);
    }

    #[test]
    fn test_sha256_output_length() {
        let hash = SecretHashAlgo::SHA256.hash_secret(b"test secret");
        assert_eq!(hash.len(), 32);
    }

    #[test]
    fn test_hash_deterministic() {
        let secret = b"atomic swap secret";
        let h1 = SecretHashAlgo::SHA256.hash_secret(secret);
        let h2 = SecretHashAlgo::SHA256.hash_secret(secret);
        assert_eq!(h1, h2);
    }
}
