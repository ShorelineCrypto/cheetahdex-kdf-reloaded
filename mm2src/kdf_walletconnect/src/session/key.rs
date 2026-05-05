//! Session symmetric-key derivation.
//!
//! WalletConnect v2 establishes a per-session symmetric key with an x25519
//! Diffie-Hellman exchange whose shared secret is run through HKDF-SHA256 with
//! an empty salt and empty info, expanded to 32 bytes (the parameters are fixed
//! by the protocol). The relay topic is the hex-encoded SHA-256 digest of that
//! symmetric key.

use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use wc_common::SymKey;
use x25519_dalek::{PublicKey, StaticSecret};

/// Holds the ephemeral public key we advertise to the peer and the derived
/// session symmetric key. The symmetric key starts zeroed and is filled in once
/// the Diffie-Hellman exchange has run.
///
/// The serde representation is part of the externally-required on-disk session
/// record (see [`crate::session::SessionRecord`] / chapter 22 §22.5.3): the
/// `session_key` object carries `sym_key` and `public_key`, so the field
/// renames below are fixed by that interop contract and must not change. The
/// symmetric key is written in plaintext in the `open` format — the documented
/// Security-versus-compatibility departure (CODING_STANDARDS §5.1) — yet stays
/// masked in `Debug`.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionKey {
    /// Our ephemeral x25519 public key, published so the peer can converge on
    /// the same shared secret.
    #[serde(rename = "public_key")]
    diffie_public_key: SymKey,
    /// The 32-byte symmetric key derived from the exchange.
    #[serde(rename = "sym_key")]
    symmetric_key: SymKey,
}

/// Runs the dictated HKDF-SHA256 expansion over an x25519 shared secret.
fn expand_shared_secret(shared: &[u8]) -> Result<SymKey, hkdf::InvalidLength> {
    let extractor = Hkdf::<Sha256>::new(None, shared);
    let mut derived: SymKey = [0u8; 32];
    extractor.expand(&[], &mut derived)?;
    Ok(derived)
}

/// Performs the x25519 exchange and the HKDF expansion in one step.
fn derive(secret: &StaticSecret, peer_public: &SymKey) -> Result<SymKey, hkdf::InvalidLength> {
    let shared = secret.diffie_hellman(&PublicKey::from(*peer_public));
    expand_shared_secret(shared.as_bytes())
}

impl SessionKey {
    /// Creates a key wrapper for a freshly generated ephemeral public key. The
    /// symmetric key is left zeroed until [`SessionKey::generate_symmetric_key`]
    /// runs.
    pub fn new(public: PublicKey) -> Self {
        SessionKey {
            diffie_public_key: public.to_bytes(),
            symmetric_key: [0u8; 32],
        }
    }

    /// Generates a fresh ephemeral keypair from the OS CSPRNG and immediately
    /// derives the symmetric key against `peer_public`.
    pub fn from_osrng(peer_public: &SymKey) -> Result<Self, hkdf::InvalidLength> {
        Self::diffie_hellman(rand::rngs::OsRng, peer_public)
    }

    /// Generates a fresh ephemeral keypair from the supplied CSPRNG and derives
    /// the symmetric key against `peer_public`.
    pub fn diffie_hellman<T>(csprng: T, peer_public: &SymKey) -> Result<Self, hkdf::InvalidLength>
    where
        T: rand::RngCore + rand::CryptoRng,
    {
        let secret = StaticSecret::random_from_rng(csprng);
        let public = PublicKey::from(&secret);
        let symmetric_key = derive(&secret, peer_public)?;
        Ok(SessionKey {
            diffie_public_key: public.to_bytes(),
            symmetric_key,
        })
    }

    /// Derives (and stores) the symmetric key from our static secret and the
    /// peer's public key. The advertised public key set at construction time is
    /// left untouched.
    pub fn generate_symmetric_key(
        &mut self,
        secret: &StaticSecret,
        peer_public: &SymKey,
    ) -> Result<(), hkdf::InvalidLength> {
        self.symmetric_key = derive(secret, peer_public)?;
        Ok(())
    }

    /// The derived 32-byte symmetric key.
    pub fn symmetric_key(&self) -> SymKey { self.symmetric_key }

    /// Our advertised ephemeral public key.
    pub fn diffie_public_key(&self) -> SymKey { self.diffie_public_key }

    /// The relay topic: the hex-encoded SHA-256 digest of the symmetric key.
    pub fn generate_topic(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.symmetric_key);
        hex::encode(hasher.finalize())
    }
}

impl fmt::Debug for SessionKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never render the symmetric-key bytes.
        f.debug_struct("SessionKey")
            .field("symmetric_key", &"*******")
            .field("diffie_public_key", &hex::encode(self.diffie_public_key))
            .finish()
    }
}
