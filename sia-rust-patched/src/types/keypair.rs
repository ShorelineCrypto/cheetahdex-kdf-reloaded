use curve25519_dalek::edwards::CompressedEdwardsY;
use ed25519_dalek::{ExpandedSecretKey, PublicKey as Ed25519PublicKey, SecretKey,
                    SignatureError as Ed25519SignatureError, Signer, Verifier, SECRET_KEY_LENGTH};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

use crate::types::{Address, Signature, SpendPolicy};

/// A Sia Public-Private Keypair
/// The purpose of this wrapper type is to limit the functionality of underlying ed25519 types.
/// The inner fields are not public by design.
/// We must not allow the consumer to create an invalid ed25519 Keypair or edit the PublicKey after creation.
/// see <https://github.com/advisories/GHSA-w5vr-6qhr-36cc>
pub struct Keypair {
    public: PublicKey,
    private: PrivateKey,
}

#[derive(Debug, Error)]
pub enum KeypairError {
    #[error("Keypair::verify: invalid signature {0}")]
    VerifySignature(#[from] PublicKeyError),
    #[error("Keypair::from_private_bytes: invalid private key {0}")]
    InvalidPrivateKey(#[from] Ed25519SignatureError),
}

impl Signer<Signature> for Keypair {
    /// Sign a message with this keypair's secret key.
    fn try_sign(&self, message: &[u8]) -> Result<Signature, Ed25519SignatureError> {
        let expanded: ExpandedSecretKey = (&self.private.0).into();
        Ok(Signature::from(expanded.sign(message, &self.public.0)))
    }
}

impl Keypair {
    pub fn from_private_bytes(bytes: &[u8]) -> Result<Self, KeypairError> {
        let secret = SecretKey::from_bytes(bytes)?;
        let public = PublicKey(Ed25519PublicKey::from(&secret));
        let private = PrivateKey(secret);
        Ok(Keypair { public, private })
    }

    pub fn sign(&self, message: &[u8]) -> Signature { Signer::sign(self, message) }

    /// Verify a signature of a message with this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), KeypairError> {
        Ok(self.public.verify(message, signature)?)
    }

    pub fn public(&self) -> PublicKey { self.public.clone() }

    pub fn private(&self) -> [u8; SECRET_KEY_LENGTH] { self.private.0.to_bytes() }
}

struct PrivateKey(SecretKey);

#[derive(Clone, Debug, PartialEq)]
pub struct PublicKey(pub Ed25519PublicKey);

#[derive(Debug, Error)]
pub enum PublicKeyError {
    #[error("invalid public key length: expected 32 byte hex string prefixed with 'ed25519:', found {0}")]
    InvalidLength(String),
    #[error("public key invalid hex: expected 32 byte hex string, found {0}")]
    InvalidHex(String),
    #[error("public key invalid: corrupt curve point {0}")]
    CorruptPoint(String),
    #[error("public key invalid: from_bytes failed {0}")]
    ParseBytes(Ed25519SignatureError),
    #[error("PublicKey::verify: invalid signature {0}")]
    VerifySignature(#[from] Ed25519SignatureError),
}

impl Verifier<Signature> for PublicKey {
    /// Verify a signature of a message with this keypair's public key.
    fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), Ed25519SignatureError> {
        self.0.verify(message, &signature.0)
    }
}

impl PublicKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PublicKeyError> {
        let public_key = Ed25519PublicKey::from_bytes(bytes)
            .map(PublicKey)
            .map_err(PublicKeyError::ParseBytes)?;

        match public_key.validate_point() {
            true => Ok(public_key),
            false => Err(PublicKeyError::CorruptPoint(hex::encode(bytes))),
        }
    }

    /// Check if public key is a valid point on the Ed25519 curve
    pub fn validate_point(&self) -> bool {
        // Create a CompressedEdwardsY point from the first 32 bytes
        CompressedEdwardsY::from_slice(&self.0.to_bytes())
            .decompress()
            .is_some()
    }

    pub fn as_bytes(&self) -> &[u8] { self.0.as_bytes() }

    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_bytes() }

    // Method for parsing a hex string without the "ed25519:" prefix
    pub fn from_str_no_prefix(hex_str: &str) -> Result<Self, PublicKeyError> {
        let mut bytes = [0u8; 32];
        hex::decode_to_slice(hex_str, &mut bytes).map_err(|_| PublicKeyError::InvalidHex(hex_str.to_string()))?;

        let public_key = Self::from_bytes(&bytes)?;

        match public_key.validate_point() {
            true => Ok(public_key),
            false => Err(PublicKeyError::CorruptPoint(hex::encode(bytes))),
        }
    }

    /// Generate the default v1 address from the public key
    pub fn v1_address(&self) -> Address { SpendPolicy::unlock_condition(vec![self.clone()], 0, 1).address() }

    /// Generate the default v2 address from the public key
    pub fn address(&self) -> Address { SpendPolicy::PublicKey(self.clone()).address() }

    /// Verify a signature of a message with this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), PublicKeyError> {
        Ok(Verifier::verify(self, message, signature)?)
    }
}

impl FromStr for PublicKey {
    type Err = PublicKeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(hex_str) = s.strip_prefix("ed25519:") {
            PublicKey::from_str_no_prefix(hex_str)
        } else {
            Err(PublicKeyError::InvalidHex(s.to_string()))
        }
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PublicKeyVisitor;

        impl<'de> serde::de::Visitor<'de> for PublicKeyVisitor {
            type Value = PublicKey;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string prefixed with 'ed25519:' and followed by a 64-character hex string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if let Some(hex_str) = value.strip_prefix("ed25519:") {
                    PublicKey::from_str_no_prefix(hex_str)
                        .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(value), &self))
                } else {
                    Err(E::invalid_value(serde::de::Unexpected::Str(value), &self))
                }
            }
        }

        deserializer.deserialize_str(PublicKeyVisitor)
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "ed25519:{:02x}", self) }
}

impl fmt::LowerHex for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", hex::encode(self.as_bytes())) }
}
