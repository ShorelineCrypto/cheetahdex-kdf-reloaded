// SPDX-License-Identifier: GPL-2.0-only
//! `keys::Address` <-> JSON string adapter (matches Bitcoin Core address encoding).

use keys::Address;
use serde::de::{Error as DeError, Unexpected, Visitor};
use serde::{Deserializer, Serialize, Serializer};
use std::fmt;

pub fn serialize<S: Serializer>(address: &Address, serializer: S) -> Result<S::Ok, S::Error> {
    address.to_string().serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Address, D::Error> {
    deserializer.deserialize_any(AddressVisitor)
}

#[derive(Default)]
pub struct AddressVisitor;

impl<'de> Visitor<'de> for AddressVisitor {
    type Value = Address;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an address")
    }

    fn visit_str<E: DeError>(self, value: &str) -> Result<Self::Value, E> {
        value
            .parse()
            .map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
    }
}

pub mod vec {
    //! Serde adapter for `Vec<keys::Address>` rendered as a JSON array of strings.

    use super::AddressVisitor;
    use keys::Address;
    use serde::de::Visitor;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(addresses: &[Address], serializer: S) -> Result<S::Ok, S::Error> {
        let rendered: Vec<String> = addresses.iter().map(Address::to_string).collect();
        rendered.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Address>, D::Error> {
        let raw: Vec<String> = Vec::<String>::deserialize(deserializer)?;
        raw.into_iter().map(|entry| AddressVisitor.visit_str(&entry)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::super::address;
    use keys::Address;
    use serde_json;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct AddressContainer {
        #[serde(with = "address")]
        address: Address,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct AddressVecContainer {
        #[serde(with = "address::vec")]
        addresses: Vec<Address>,
    }

    fn container(value: &'static str) -> AddressContainer {
        AddressContainer { address: value.into() }
    }

    #[test]
    fn btc_address_serializes_and_deserializes() {
        let payload = container("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
        assert_eq!(
            serde_json::to_string(&payload).unwrap(),
            r#"{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"}"#
        );
        assert_eq!(
            serde_json::from_str::<AddressContainer>(r#"{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"}"#).unwrap(),
            payload
        );
    }

    #[test]
    fn kmd_address_serializes_and_deserializes() {
        let payload = container("R9o9xTocqr6CeEDGDH6mEYpwLoMz6jNjMW");
        assert_eq!(
            serde_json::to_string(&payload).unwrap(),
            r#"{"address":"R9o9xTocqr6CeEDGDH6mEYpwLoMz6jNjMW"}"#
        );
        assert_eq!(
            serde_json::from_str::<AddressContainer>(r#"{"address":"R9o9xTocqr6CeEDGDH6mEYpwLoMz6jNjMW"}"#).unwrap(),
            payload
        );
    }

    #[test]
    fn vec_adapter_round_trips() {
        let json = r#"{"addresses":["1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"]}"#;
        let parsed: AddressVecContainer = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.addresses.len(), 1);
        assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
    }
}
