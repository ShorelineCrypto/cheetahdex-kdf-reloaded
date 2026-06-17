use crate::blake2b_internal::standard_unlock_hash;
use crate::encoding::{Encodable, Encoder};
use blake2b_simd::Params;
use chrono::{DateTime, Utc};
use derive_more::{Display, From, Into};
use hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

mod hash;
pub use hash::{Hash256, Hash256Error};

mod signature;
pub use signature::{Signature, SignatureError};

mod keypair;
pub use keypair::{Keypair, KeypairError, PublicKey, PublicKeyError};

mod spend_policy;
pub use spend_policy::*;

mod transaction;
pub use transaction::*;

mod specifier;
pub use specifier::*;

mod consensus_updates;
pub use consensus_updates::*;

const ADDRESS_HASH_LENGTH: usize = 32;
const ADDRESS_CHECKSUM_LENGTH: usize = 6;

// TODO this could probably include the checksum within the data type
// generating the checksum on the fly is how Sia Go does this however
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address(pub Hash256);

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_str = format!("{}", self);
        serializer.serialize_str(&hex_str)
    }
}

impl<'de> Deserialize<'de> for Address {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AddressVisitor;

        impl<'de> serde::de::Visitor<'de> for AddressVisitor {
            type Value = Address;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 76-character hex string representing a Sia address")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Address::from_str(value).map_err(|_| E::invalid_value(serde::de::Unexpected::Str(value), &self))
            }
        }

        deserializer.deserialize_str(AddressVisitor)
    }
}

impl Address {
    pub fn standard_address_v1(pubkey: &PublicKey) -> Self {
        let hash = standard_unlock_hash(pubkey);
        Address(hash)
    }

    pub fn from_public_key(pubkey: &PublicKey) -> Self {
        SpendPolicy::PublicKey(pubkey.clone()).address()
    }
}

impl Encodable for Address {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bytes = self.0 .0.as_ref();
        let checksum = blake2b_checksum(bytes);
        write!(f, "{}{}", hex::encode(bytes), hex::encode(checksum))
    }
}

#[derive(Debug, Error)]
pub enum AddressError {
    #[error("Address::from_str Failed to decode hex: {0}")]
    InvalidHex(#[from] hex::FromHexError),
    #[error("Address::from_str: Invalid length, expected 38 byte hex string, found: {0}")]
    InvalidLength(String),
    #[error("Address::from_str: invalid checksum, expected:{expected}, found:{found}")]
    InvalidChecksum { expected: String, found: String },
}

impl FromStr for Address {
    type Err = AddressError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // An address consists of a 32 byte blake2h hash followed by a 6 byte checksum
        let address_bytes = hex::decode(s)?;
        if address_bytes.len() != ADDRESS_HASH_LENGTH + ADDRESS_CHECKSUM_LENGTH {
            return Err(AddressError::InvalidLength(s.to_owned()));
        }

        let hash_bytes = &address_bytes[0..ADDRESS_HASH_LENGTH];
        let checksum_bytes = &address_bytes[ADDRESS_HASH_LENGTH..];

        let checksum = blake2b_checksum(hash_bytes);
        if checksum_bytes != checksum {
            return Err(AddressError::InvalidChecksum {
                expected: hex::encode(checksum),
                found: hex::encode(checksum_bytes),
            });
        }
        let inner_hash = Hash256::try_from(hash_bytes).expect("hash_bytes is 32 bytes long");
        Ok(Address(inner_hash))
    }
}

/// Return the first 6 bytes of the blake2b(preimage) hash
/// Used in generating the checksum for a Sia address
fn blake2b_checksum(preimage: &[u8]) -> [u8; 6] {
    let hash = Params::new().hash_length(32).to_state().update(preimage).finalize();
    hash.as_bytes()[0..6].try_into().expect("array is 64 bytes long")
}

#[derive(Clone, Debug, Display, PartialEq, From, Into, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlockId(pub Hash256);

impl Encodable for BlockId {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ChainIndex {
    pub height: u64,
    pub id: BlockId,
}

// TODO unit test
impl Encodable for ChainIndex {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.height);
        let block_id: Hash256 = self.id.clone().into();
        block_id.encode(encoder);
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct EventV1Transaction {
    pub transaction: V1Transaction,
    pub spent_siacoin_elements: Vec<SiacoinElement>,
    pub spent_siafund_elements: Vec<SiafundElement>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventV1ContractResolution {
    pub parent: FileContractElementV1,
    pub siacoin_element: SiacoinElement,
    pub missed: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventPayout {
    pub siacoin_element: SiacoinElement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EventType {
    Miner,
    Foundation,
    SiafundClaim,
    V1Transaction,
    V2Transaction,
    V1ContractResolution,
    V2ContractResolution,
}

#[derive(Clone, Debug, Serialize)]
pub struct Event {
    pub id: Hash256,
    pub index: ChainIndex,
    pub confirmations: u64,
    pub timestamp: DateTime<Utc>,
    #[serde(rename = "maturityHeight")]
    pub maturity_height: u64,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub data: EventDataWrapper,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevant: Option<Vec<Address>>,
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        struct EventHelper {
            id: Hash256,
            index: ChainIndex,
            confirmations: u64,
            timestamp: DateTime<Utc>,
            #[serde(rename = "maturityHeight")]
            maturity_height: u64,
            #[serde(rename = "type")]
            event_type: EventType,
            data: Value,
            relevant: Option<Vec<Address>>,
        }

        let helper = EventHelper::deserialize(deserializer)?;
        let event_data = match helper.event_type {
            EventType::Miner => serde_json::from_value::<EventPayout>(helper.data)
                .map(EventDataWrapper::MinerPayout)
                .map_err(serde::de::Error::custom),
            EventType::Foundation => serde_json::from_value::<EventPayout>(helper.data)
                .map(EventDataWrapper::FoundationPayout)
                .map_err(serde::de::Error::custom),
            EventType::SiafundClaim => serde_json::from_value::<EventPayout>(helper.data)
                .map(EventDataWrapper::ClaimPayout)
                .map_err(serde::de::Error::custom),
            EventType::V1Transaction => serde_json::from_value::<EventV1Transaction>(helper.data)
                .map(EventDataWrapper::V1Transaction)
                .map_err(serde::de::Error::custom),
            EventType::V2Transaction => serde_json::from_value::<V2Transaction>(helper.data)
                .map(EventDataWrapper::V2Transaction)
                .map_err(serde::de::Error::custom),
            EventType::V1ContractResolution => serde_json::from_value::<EventV1ContractResolution>(helper.data)
                .map(EventDataWrapper::EventV1ContractResolution)
                .map_err(serde::de::Error::custom),
            EventType::V2ContractResolution => serde_json::from_value::<EventV2ContractResolution>(helper.data)
                .map(|data| EventDataWrapper::V2FileContractResolution(Box::new(data)))
                .map_err(serde::de::Error::custom),
        }?;

        Ok(Event {
            id: helper.id,
            index: helper.index,
            confirmations: helper.confirmations,
            timestamp: helper.timestamp,
            maturity_height: helper.maturity_height,
            event_type: helper.event_type,
            data: event_data,
            relevant: helper.relevant,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum EventDataWrapper {
    MinerPayout(EventPayout),
    FoundationPayout(EventPayout),
    ClaimPayout(EventPayout),
    V2Transaction(V2Transaction),
    V2FileContractResolution(Box<EventV2ContractResolution>),
    V1Transaction(EventV1Transaction),
    EventV1ContractResolution(EventV1ContractResolution),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventV2ContractResolution {
    pub resolution: V2FileContractResolution,
    pub siacoin_element: SiacoinElement,
    pub missed: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainIndexElement {
    #[serde(flatten)]
    state_element: StateElement,
    chain_index: ChainIndex,
}
