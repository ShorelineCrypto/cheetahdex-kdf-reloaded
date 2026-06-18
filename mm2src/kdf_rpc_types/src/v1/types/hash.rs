// SPDX-License-Identifier: GPL-2.0-only
//! Fixed-size hash wrappers (H160 / H256 / H264) serialized as lowercase hex.

use primitives::hash::{H160 as PrimitivesH160, H256 as PrimitivesH256, H264 as PrimitivesH264};
use rustc_hex::{FromHex, ToHex};
use serde::de::{Error as DeError, Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash as StdHash, Hasher};
use std::str::FromStr;

macro_rules! json_hash {
    ($wire_name:ident, $primitive:ident, $byte_count:expr) => {
        #[derive(Clone, Copy)]
        pub struct $wire_name(pub [u8; $byte_count]);

        impl Default for $wire_name {
            fn default() -> Self { Self([0u8; $byte_count]) }
        }

        impl<T> From<T> for $wire_name
        where
            $primitive: From<T>,
        {
            fn from(value: T) -> Self { Self($primitive::from(value).take()) }
        }

        impl FromStr for $wire_name {
            type Err = <$primitive as FromStr>::Err;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let parsed: $primitive = value.parse()?;
                Ok(Self(parsed.take()))
            }
        }

        #[allow(clippy::from_over_into)]
        impl Into<$primitive> for $wire_name {
            fn into(self) -> $primitive { $primitive::from(self.0) }
        }

        #[allow(clippy::from_over_into)]
        impl Into<Vec<u8>> for $wire_name {
            fn into(self) -> Vec<u8> { self.0.to_vec() }
        }

        impl fmt::Debug for $wire_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                let hex_text: String = self.0.to_hex();
                formatter.write_str(&hex_text)
            }
        }

        impl fmt::LowerHex for $wire_name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                for byte in &self.0[..] {
                    write!(formatter, "{:02x}", byte)?;
                }
                Ok(())
            }
        }

        impl PartialEq for $wire_name {
            fn eq(&self, other: &Self) -> bool { self.0[..] == other.0[..] }
        }

        impl Eq for $wire_name {}

        impl Ord for $wire_name {
            fn cmp(&self, other: &Self) -> Ordering { self.0[..].cmp(&other.0[..]) }
        }

        impl PartialOrd for $wire_name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
        }

        impl StdHash for $wire_name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                let primitive: $primitive = $primitive::from(self.0);
                primitive.hash(state);
            }
        }

        impl Serialize for $wire_name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let hex_text: String = self.0.to_hex();
                serializer.serialize_str(&hex_text)
            }
        }

        impl<'de> Deserialize<'de> for $wire_name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct HashHexVisitor;

                impl<'de> Visitor<'de> for HashHexVisitor {
                    type Value = $wire_name;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str("a hash string")
                    }

                    fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
                        if value.len() != $byte_count * 2 {
                            return Err(E::invalid_value(Unexpected::Str(value), &self));
                        }
                        let parsed: Vec<u8> = value
                            .from_hex()
                            .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))?;
                        let mut buffer = [0u8; $byte_count];
                        buffer.copy_from_slice(&parsed);
                        Ok($wire_name(buffer))
                    }

                    fn visit_string<E: DeError>(self, value: String) -> Result<Self::Value, E> {
                        self.visit_str(&value)
                    }
                }

                deserializer.deserialize_identifier(HashHexVisitor)
            }
        }
    };
}

json_hash!(H160, PrimitivesH160, 20);
json_hash!(H256, PrimitivesH256, 32);
json_hash!(H264, PrimitivesH264, 33);

impl H256 {
    /// Returns a copy with byte order reversed (matches Bitcoin Core's "txid" field convention).
    #[inline]
    pub fn reversed(&self) -> Self {
        let mut clone = *self;
        clone.0.reverse();
        clone
    }
}

#[cfg(test)]
mod tests {
    use super::H256;
    use primitives::hash::H256 as PrimitivesH256;
    use std::str::FromStr;

    #[test]
    fn debug_yields_hex_string() {
        let raw = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
        let parsed = H256::from(raw);
        assert_eq!(format!("{:?}", parsed), raw);
    }

    #[test]
    fn from_str_round_trips_and_rejects_garbage() {
        let raw = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
        let parsed = H256::from_str(raw).unwrap();
        assert_eq!(format!("{:?}", parsed), raw);
        assert!(H256::from_str("zzz").is_err());
    }

    #[test]
    fn into_primitives_hash_preserves_bytes() {
        let raw = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
        let json_form = H256::from(raw);
        let primitive_form = PrimitivesH256::from(raw);
        let converted: PrimitivesH256 = json_form.into();
        assert_eq!(converted, primitive_form);
    }
}
