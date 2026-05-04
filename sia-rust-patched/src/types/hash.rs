use crate::encoding::{Encodable, Encoder};
use hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::convert::TryFrom;
use std::fmt::{self, Display};
use std::str::FromStr;
use thiserror::Error;

/*
TODO:
Hash256 once required custom serde and encoding due to handling various prefixes based on the context.
These prefixes are now removed, so helpers like serde_as and derive_more could be used to reduce
boilerplate.
 */

#[derive(Debug, Error)]
pub enum Hash256Error {
    #[error("Hash256::from_str invalid hex: expected 32 byte hex string, found {0}")]
    InvalidHex(String),
    #[error("Hash256::from_str invalid length: expected 32 byte hex string, found {0}")]
    InvalidLength(String),
    #[error("Hash256::TryFrom<&[u8]> invalid slice length: expected 32 byte slice, found {0:?}")]
    InvalidSliceLength(Vec<u8>),
}

/// A 256 bit number representing a blake2b or sha256 hash in Sia's consensus protocol and APIs.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct Hash256(pub [u8; 32]);

impl Encodable for Hash256 {
    fn encode(&self, encoder: &mut Encoder) { encoder.write_slice(&self.0); }
}

impl Serialize for Hash256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Hash256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Hash256::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Hash256 {
    type Err = Hash256Error;

    fn from_str(hex_str: &str) -> Result<Self, Hash256Error> {
        if hex_str.len() != 64 {
            return Err(Hash256Error::InvalidLength(hex_str.to_string()));
        }

        let mut bytes = [0u8; 32];
        match hex::decode_to_slice(hex_str, &mut bytes) {
            Ok(_) => Ok(Hash256(bytes)),
            Err(_) => Err(Hash256Error::InvalidHex(hex_str.to_string())),
        }
    }
}

impl Display for Hash256 {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", hex::encode(self.0)) }
}

impl TryFrom<&[u8]> for Hash256 {
    type Error = Hash256Error;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let slice_len = slice.len();
        if slice_len == 32 {
            let mut array = [0u8; 32];
            array.copy_from_slice(slice);
            Ok(Hash256(array))
        } else {
            Err(Hash256Error::InvalidSliceLength(slice.to_owned()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    cross_target_tests! {
        fn test_default() {
            let hash = Hash256::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
            assert_eq!(hash, Hash256::default());
        }

        fn test_valid() {
            let hash = Hash256::from_str("c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();
            assert_eq!(hash.to_string(), "c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        }

        fn test_display() {
            let hash = Hash256::from_str("c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();
            assert_eq!(hash.to_string(), "c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        }

        fn test_serialize() {
            let hash = Hash256::from_str("c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();
            let serialized = serde_json::to_string(&hash).unwrap();
            assert_eq!(&serialized, r#""c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee""#);
        }

        fn test_deserialize() {
            let hash = Hash256::from_str("c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee").unwrap();
            let deserialized: Hash256 = serde_json::from_str(r#""c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee""#).unwrap();
            assert_eq!(deserialized, hash);
        }

        fn test_invalid_hex() {
            let err = Hash256::from_str("c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeg").expect_err("no prefix");
            let expected = "c0ffeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeg";
            match err {
                Hash256Error::InvalidHex(ref e) if expected == e => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }

        fn test_invalid_length() {
            let err = Hash256::from_str("badc0de").expect_err("invalid length");
            let expected = "badc0de";
            match err {
                Hash256Error::InvalidLength(ref e) if expected == e => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }

        fn test_from_str_valid() {
            let hash = Hash256::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
            assert_eq!(hash, Hash256::default())
        }

        fn test_from_str_invalid_length() {
            let err = Hash256::from_str("badc0de").expect_err("invalid length");
            let expected = "badc0de";
            match err {
                Hash256Error::InvalidLength(ref e) if expected == e => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }

        fn test_from_str_invalid_hex() {
            let err = Hash256::from_str("g00000000000000000000000000000000000000000000000000000000000000e").expect_err("invalid hex");
            let expected = "g00000000000000000000000000000000000000000000000000000000000000e";
            match err {
                Hash256Error::InvalidHex(ref e) if expected == e => (),
                _ => panic!("unexpected error: {:?}", err),
            }
        }
    }
}
