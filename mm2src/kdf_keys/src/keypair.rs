// Linked secp256k1 secret + public pair.
//
// `random_compressed()` derives an ephemeral secret with the OS RNG;
// `from_keypair` lets the caller bring their own (sec, pub) tuple
// (used by HD-wallet derivation paths).

use crate::hash::{H264, H520};
use crate::{Error, Private, Public, Secret, SECP_SIGN};
use crypto::ChecksumType;
use secp256k1::{PublicKey, SecretKey};
use std::fmt;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct KeyPair {
    private: Private,
    public: Public,
}

impl KeyPair {
    pub fn private(&self) -> &Private {
        &self.private
    }
    pub fn private_bytes(&self) -> [u8; 32] {
        self.private.secret.take()
    }
    pub fn private_ref(&self) -> &[u8; 32] {
        &self.private.secret
    }
    pub fn public(&self) -> &Public {
        &self.public
    }
    pub fn public_slice(&self) -> &[u8] {
        &self.public
    }

    pub fn from_private(private: Private) -> Result<KeyPair, Error> {
        let secret = SecretKey::from_slice(&*private.secret)?;
        let pub_key = PublicKey::from_secret_key(&SECP_SIGN, &secret);
        let public = if private.compressed {
            let mut h = H264::default();
            h.copy_from_slice(&pub_key.serialize());
            Public::Compressed(h)
        } else {
            let mut h = H520::default();
            h.copy_from_slice(&pub_key.serialize_uncompressed());
            Public::Normal(h)
        };
        Ok(KeyPair { private, public })
    }

    pub fn from_keypair(sec: SecretKey, public: PublicKey, prefix: u8) -> Self {
        let mut secret = Secret::default();
        secret.copy_from_slice(&sec[..]);
        let mut p = H520::default();
        p.copy_from_slice(&public.serialize_uncompressed());
        KeyPair {
            private: Private {
                prefix,
                secret,
                compressed: false,
                checksum_type: ChecksumType::DSHA256,
            },
            public: Public::Normal(p),
        }
    }

    pub fn random_compressed() -> Self {
        let secret = SecretKey::new(&mut rand::thread_rng());
        let pub_key = PublicKey::from_secret_key(&SECP_SIGN, &secret);
        KeyPair {
            private: Private {
                prefix: 0,
                secret: (*secret.as_ref()).into(),
                compressed: true,
                checksum_type: ChecksumType::default(),
            },
            public: Public::Compressed(pub_key.serialize().into()),
        }
    }
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.private.fmt(f)?;
        writeln!(f, "public: {:?}", self.public)
    }
}

impl fmt::Display for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "private: {}", self.private)?;
        writeln!(f, "public: {}", self.public)
    }
}

#[cfg(test)]
mod tests {
    use super::KeyPair;
    use crypto::dhash256;

    // Bitcoin Core reference vectors:
    // https://github.com/bitcoin/bitcoin/blob/master/src/test/key_tests.cpp
    const SECRET_0: &str = "5KSCKP8NUyBZPCCQusxRwgmz9sfvJQEgbGukmmHepWw5Bzp95mu";
    const SECRET_1: &str = "5HxWvvfubhXpYYpS3tJkw6fq9jE9j18THftkZjHHfmFiWtmAbrj";
    const SECRET_2: &str = "5KC4ejrDjv152FGwP386VD1i2NYc5KkfSMyv1nGy1VGDxGHqVY3";
    const SECRET_1C: &str = "Kwr371tjA9u2rFSMZjTNun2PXXP3WPZu2afRHTcta6KxEUdm1vEw";
    const SECRET_2C: &str = "L3Hq7a8FEQwJkW1M2GNKDW28546Vp5miewcCzSqUD9kCAXrJdS3g";
    const SIGN_1: &str = "304402205dbbddda71772d95ce91cd2d14b592cfbc1dd0aabd6a394b6c2d377bbe59d31d022014ddda21494a4e221f0824f0b8b924c43fa43c0ad57dccdaa11f81a6bd4582f6";
    const SIGN_2: &str = "3044022052d8a32079c11e79db95af63bb9600c5b04f21a9ca33dc129c2bfa8ac9dc1cd5022061d8ae5e0f6c1a16bde3719c64c2fd70e404b6428ab9a69566962e8771b5944d";

    fn compressed(secret: &'static str) -> bool {
        KeyPair::from_private(secret.into()).unwrap().private().compressed
    }

    fn signed(secret: &'static str, msg: &[u8], sig: &'static str) -> bool {
        let m = dhash256(msg);
        let kp = KeyPair::from_private(secret.into()).unwrap();
        kp.private().sign(&m).unwrap() == sig.into()
    }

    fn verified(secret: &'static str, msg: &[u8], sig: &'static str) -> bool {
        let m = dhash256(msg);
        let kp = KeyPair::from_private(secret.into()).unwrap();
        kp.public().verify(&m, &sig.into()).unwrap()
    }

    #[test]
    fn compression_flag_roundtrip() {
        assert!(!compressed(SECRET_0));
        assert!(!compressed(SECRET_1));
        assert!(!compressed(SECRET_2));
        assert!(compressed(SECRET_1C));
        assert!(compressed(SECRET_2C));
    }

    #[test]
    fn deterministic_signing() {
        let m = b"Very deterministic message";
        assert!(signed(SECRET_1, m, SIGN_1));
        assert!(signed(SECRET_1C, m, SIGN_1));
        assert!(signed(SECRET_2, m, SIGN_2));
        assert!(signed(SECRET_2C, m, SIGN_2));
        assert!(!signed(SECRET_2C, b"", SIGN_2));
    }

    #[test]
    fn verify_known_signatures() {
        let m = b"Very deterministic message";
        assert!(verified(SECRET_1, m, SIGN_1));
        assert!(verified(SECRET_1C, m, SIGN_1));
        assert!(verified(SECRET_2, m, SIGN_2));
        assert!(verified(SECRET_2C, m, SIGN_2));
        assert!(!verified(SECRET_2C, b"", SIGN_2));
    }
}
