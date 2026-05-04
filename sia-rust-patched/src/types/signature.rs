use crate::types::keypair::{PublicKey, PublicKeyError};

use curve25519_dalek::edwards::CompressedEdwardsY;
use derive_more::{From, Into};
use ed25519_dalek::ed25519::signature::{Error as SignatureCrateError, Signature as SignatureTrait};
use ed25519_dalek::{Signature as Ed25519Signature, SIGNATURE_LENGTH};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, From, Into)]
pub struct Signature(pub Ed25519Signature);

#[derive(Debug, Error)]
pub enum SignatureError {
    #[error("Signature::TryFrom<&[u8]>: failed to parse signature from slice {0}")]
    ParseSlice(#[from] ed25519_dalek::ed25519::Error),
    #[error("Signature::TryFrom<&[u8]>: invalid signature:{0:?}, corrupt R point")]
    CorruptRPointSlice(Vec<u8>),
    #[error("Signature::from_str: invalid signature:{0}, corrupt R point")]
    CorruptRPointStr(String),
    #[error("Signature::verify: invalid signature: {0}")]
    VerifyFailed(#[from] PublicKeyError),
}

impl Default for Signature {
    fn default() -> Self { Signature(Ed25519Signature::try_from([0u8; 64]).expect("00'd signature is valid")) }
}

impl fmt::Display for Signature {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", hex::encode(self.0.to_bytes())) }
}

impl Serialize for Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Signature::from_str(&s).map_err(serde::de::Error::custom)
    }
}

// trait bound of Signer for Keypair
impl SignatureTrait for Signature {
    fn from_bytes(bytes: &[u8]) -> Result<Self, SignatureCrateError> {
        // Delegate to the inner type's implementation
        Ed25519Signature::from_bytes(bytes).map(Signature)
    }
}

// trait bound of signature_crate::Signature
impl AsRef<[u8]> for Signature {
    fn as_ref(&self) -> &[u8] { self.0.as_ref() }
}

impl TryFrom<&[u8]> for Signature {
    type Error = SignatureError;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let signature = Ed25519Signature::from_bytes(bytes)
            .map(Signature)
            .map_err(SignatureError::ParseSlice)?;

        match signature.validate_r_point() {
            true => Ok(signature),
            false => Err(SignatureError::CorruptRPointSlice(bytes.to_vec())),
        }
    }
}

impl TryFrom<Vec<u8>> for Signature {
    type Error = SignatureError;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> { Signature::try_from(bytes.as_slice()) }
}

impl Signature {
    pub fn to_bytes(&self) -> [u8; SIGNATURE_LENGTH] { self.0.to_bytes() }

    /// Check if R value is a valid point on the Ed25519 curve
    pub fn validate_r_point(&self) -> bool {
        let r_bytes = &self.0.to_bytes()[0..SIGNATURE_LENGTH / 2];

        // Create a CompressedEdwardsY point from the first 32 bytes
        CompressedEdwardsY::from_slice(r_bytes).decompress().is_some()
    }

    pub fn verify(&self, message: &[u8], public_key: &PublicKey) -> Result<(), SignatureError> {
        Ok(public_key.verify(message, self)?)
    }
}

// impl fmt::LowerHex for Signature {
//     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//         // Delegate to the fmt::LowerHex implementation of the inner Ed25519Signature
//         fmt::LowerHex::fmt(&self.0, f)
//     }
// }

impl FromStr for Signature {
    type Err = SignatureError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let bytes = hex::decode(value).map_err(|_| {
            // Re-use ParseSlice with an error from trying to parse a dummy empty slice
            SignatureError::ParseSlice(ed25519_dalek::ed25519::Error::new())
        })?;
        let signature = Signature::try_from(bytes.as_slice())?;

        match signature.validate_r_point() {
            true => Ok(signature),
            false => Err(SignatureError::CorruptRPointStr(value.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    const VALID_STR: &str = "f43380794a6384e3d24d9908143c05dd37aaac8959efb65d986feb70fe289a5e26b84e0ac712af01a2f85f8727da18aae13a599a51fb066d098591e40cb26902";
    const VALID_JSON_STR: &str = r#""f43380794a6384e3d24d9908143c05dd37aaac8959efb65d986feb70fe289a5e26b84e0ac712af01a2f85f8727da18aae13a599a51fb066d098591e40cb26902""#;

    fn valid_signature() -> Signature { Signature::from_str(VALID_STR).unwrap() }

    cross_target_tests! {
        fn test_display() {
            assert_eq!(valid_signature().to_string(), VALID_STR);
        }

        fn test_debug() {
            assert_eq!(format!("{:?}", valid_signature()), "Signature(ed25519::Signature(F43380794A6384E3D24D9908143C05DD37AAAC8959EFB65D986FEB70FE289A5E26B84E0AC712AF01A2F85F8727DA18AAE13A599A51FB066D098591E40CB26902))");
        }

        fn test_serialize() {
            assert_eq!(&serde_json::to_string(&valid_signature()).unwrap(), VALID_JSON_STR);
        }

        fn test_deserialize() {
            assert_eq!(serde_json::from_str::<Signature>(VALID_JSON_STR).unwrap(), valid_signature());
        }

        fn test_invalid_hex() {
            let test_case = "g43380794a6384e3d24d9908143c05dd37aaac8959efb65d986feb70fe289a5e26b84e0ac712af01a2f85f8727da18aae13a599a51fb066d098591e40cb26902";
            let err = Signature::from_str(test_case).expect_err("no prefix");
            match err {
                SignatureError::ParseSlice(_) => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }

        fn test_invalid_r_signature() {
            let test_case = "00000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000";
            let err = Signature::from_str(test_case).expect_err("no prefix");
            match err {
                SignatureError::CorruptRPointStr(_) => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }

        fn test_invalid_length() {
            let test_case = "badc0de";
            let err = Signature::from_str(test_case).expect_err("invalid length");
            match err {
                SignatureError::ParseSlice(_) => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }
    }
}
