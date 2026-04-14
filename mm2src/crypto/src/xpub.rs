/// XPub prefix conversion utilities.
///
/// Bitcoin-like extended public keys (xpubs) use a 4-byte magic prefix to indicate
/// the key type (xpub, ypub, zpub, etc.). This module provides utilities to detect
/// and convert between prefix formats.
///
/// Common prefixes (mainnet):
/// - `xpub` (0x0488B21E) — BIP44/BIP32 P2PKH
/// - `ypub` (0x049D7CB2) — BIP49 P2WPKH-in-P2SH
/// - `zpub` (0x04B24746) — BIP84 native P2WPKH

use derive_more::Display;

/// Standard xpub magic prefix bytes (0x0488B21E).
const XPUB_MAGIC: [u8; 4] = [0x04, 0x88, 0xB2, 0x1E];

/// Errors during xpub conversion.
#[derive(Debug, Display)]
pub enum XpubError {
    #[display(fmt = "Invalid base58 encoding: {}", _0)]
    InvalidBase58(String),
    #[display(fmt = "Invalid xpub length: expected at least 4 bytes for magic prefix")]
    InvalidLength,
}

/// Utility for converting extended public key prefixes.
pub struct XPubConverter;

impl XPubConverter {
    /// Checks whether the given xpub string uses the standard `xpub` prefix (0x0488B21E).
    pub fn is_standard_xpub(xpub: &str) -> Result<bool, XpubError> {
        let decoded = bs58::decode(xpub)
            .with_check(None)
            .into_vec()
            .map_err(|e| XpubError::InvalidBase58(e.to_string()))?;

        if decoded.len() < 4 {
            return Err(XpubError::InvalidLength);
        }

        Ok(decoded[..4] == XPUB_MAGIC)
    }

    /// Replaces the 4-byte magic prefix of an extended public key with the standard `xpub` prefix.
    ///
    /// This is useful when working with hardware wallets or other systems that return
    /// xpubs with non-standard prefixes (ypub, zpub, etc.) but we need them in standard format.
    pub fn replace_magic_prefix(xpub: &str) -> Result<String, XpubError> {
        let decoded = bs58::decode(xpub)
            .with_check(None)
            .into_vec()
            .map_err(|e| XpubError::InvalidBase58(e.to_string()))?;

        if decoded.len() < 4 {
            return Err(XpubError::InvalidLength);
        }

        let mut modified = decoded;
        modified[..4].copy_from_slice(&XPUB_MAGIC);

        Ok(bs58::encode(&modified).with_check().into_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_standard_xpub() {
        // A real xpub would be much longer, but for unit testing the prefix check:
        let xpub = "xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet8";
        assert!(XPubConverter::is_standard_xpub(xpub).unwrap());
    }
}
