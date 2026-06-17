use crate::encoding::{Encodable, Encoder};
use crate::types::{Address, BlockId, ChainIndex, Hash256, PublicKey, Signature, SpendPolicy, UnlockCondition};
use crate::utils::deserialize_null_as_empty_vec;
use base64::{engine::general_purpose::STANDARD as base64, Engine as _};
use derive_more::{Add, AddAssign, Deref, Display, Div, DivAssign, From, Into, Mul, MulAssign, Sub, SubAssign, Sum};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

pub const V2_REPLAY_PREFIX: u8 = 2;

/// A currency amount in the Sia network represented in Hastings, the smallest unit of currency.
/// 1 SC = 10^24 Hastings
/// use to_string_hastings() or to_string_siacoin() to display the value.\
// TODO Alright impl Add, Sub, PartialOrd, etc
#[derive(
    Copy,
    Clone,
    Debug,
    Deref,
    Add,
    Sub,
    Mul,
    Div,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    Default,
    From,
    Into,
    Sum,
)]
pub struct Currency(pub u128);

impl Currency {
    pub const ZERO: Currency = Currency(0);

    pub const COIN: Currency = Currency(1_000_000_000_000_000_000_000_000);

    /// A default fee amount for transactions
    /// The appropriate way to calculate a txfee to use walletd's `api/txpool/fee` endpoint.
    pub const DEFAULT_FEE: Currency = Currency(20_000_000_000_000_000_000); // 0.02 SC
}

// TODO does this also need to be able to deserialize from an integer?
// walletd API returns this as a string
impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct CurrencyVisitor;

        impl<'de> serde::de::Visitor<'de> for CurrencyVisitor {
            type Value = Currency;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string representing a u128 value")
            }

            fn visit_str<E>(self, value: &str) -> Result<Currency, E>
            where
                E: serde::de::Error,
            {
                Ok(Currency(u128::from_str(value).map_err(E::custom)?))
            }
        }

        deserializer.deserialize_str(CurrencyVisitor)
    }
}

impl Serialize for Currency {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<u64> for Currency {
    fn from(value: u64) -> Self {
        Currency(value.into())
    }
}

// Currency remains the same data structure between V1 and V2 however the encoding changes
#[derive(Clone, Debug)]
pub enum CurrencyVersion<'a> {
    V1(&'a Currency),
    V2(&'a Currency),
}

impl<'a> Encodable for CurrencyVersion<'a> {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            CurrencyVersion::V1(currency) => {
                let mut buffer = [0u8; 16];

                // buffer[8..].copy_from_slice(&currency.lo.to_be_bytes());
                // buffer[..8].copy_from_slice(&currency.hi.to_be_bytes());
                buffer.copy_from_slice(&currency.to_be_bytes());

                // Trim leading zero bytes from the buffer
                let trimmed_buf = match buffer.iter().position(|&x| x != 0) {
                    Some(index) => &buffer[index..],
                    None => &buffer[..], // In case all bytes are zero
                };
                encoder.write_len_prefixed_bytes(trimmed_buf);
            },
            CurrencyVersion::V2(currency) => {
                encoder.write_u128(currency.0);
            },
        }
    }
}

/// Preimage is a 32-byte array representing the preimage of a hash used in Sia's SpendPolicy::Hash
/// Used to allow HLTC-style hashlock contracts in Sia
// TODO - this type is now effectively identical to Hash256. It only exists because Preimage once
// supported variable length preimages. Using Preimage(Hash256) would reduce code duplication, but
// we should consider changing Hash256's name as Preimage does not represent a "hash".
#[derive(Clone, Debug, Default, PartialEq, From, Into)]
pub struct Preimage(pub [u8; 32]);

impl Encodable for Preimage {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_slice(&self.0);
    }
}

impl Serialize for Preimage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Use hex::encode to convert the byte array to a lowercase hex string
        let hex_string = hex::encode(self.0);
        serializer.serialize_str(&hex_string)
    }
}

impl<'de> Deserialize<'de> for Preimage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PreimageVisitor;

        impl<'de> serde::de::Visitor<'de> for PreimageVisitor {
            type Value = Preimage;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 32 byte hex string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // Ensure the length is correct for a 32 byte hex string (64 hex characters)
                if value.len() != 64 {
                    return Err(E::invalid_length(value.len(), &self));
                }

                // Decode the hex string into a byte array
                let mut bytes = [0u8; 32];
                hex::decode_to_slice(value, &mut bytes)
                    .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(value), &self))?;

                Ok(Preimage(bytes))
            }
        }

        deserializer.deserialize_str(PreimageVisitor)
    }
}

#[derive(Debug, Error)]
pub enum PreimageError {
    #[error("Preimage:TryFrom<&[u8]>: invalid length, expected 32 bytes found: {0}")]
    InvalidSliceLength(usize),
}

impl From<Preimage> for Vec<u8> {
    fn from(preimage: Preimage) -> Self {
        preimage.0.to_vec()
    }
}

impl TryFrom<&[u8]> for Preimage {
    type Error = PreimageError;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        let slice_len = slice.len();
        if slice_len == 32 {
            let mut array = [0u8; 32];
            array.copy_from_slice(slice);
            Ok(Preimage(array))
        } else {
            Err(PreimageError::InvalidSliceLength(slice_len))
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SatisfiedPolicy {
    pub policy: SpendPolicy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub signatures: Vec<Signature>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preimages: Vec<Preimage>,
}

impl Encodable for Signature {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_slice(&self.to_bytes());
    }
}

impl Encodable for SatisfiedPolicy {
    fn encode(&self, encoder: &mut Encoder) {
        self.policy.encode(encoder);
        encoder.write_len_prefixed_vec(&self.signatures);
        encoder.write_len_prefixed_vec(&self.preimages);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StateElement {
    pub leaf_index: u64,
    #[serde(deserialize_with = "deserialize_null_as_empty_vec", default)]
    pub merkle_proof: Vec<Hash256>,
}

impl Encodable for StateElement {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.leaf_index);
        encoder.write_u64(self.merkle_proof.len() as u64);

        for proof in &self.merkle_proof {
            proof.encode(encoder);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiafundElement {
    #[serde(rename = "ID")]
    pub id: SiafundOutputId,
    pub state_element: StateElement,
    pub siafund_output: SiafundOutput,
    pub claim_start: Currency,
}

impl Encodable for SiafundElement {
    fn encode(&self, encoder: &mut Encoder) {
        self.state_element.encode(encoder);
        SiafundOutputVersion::V2(&self.siafund_output).encode(encoder);
        CurrencyVersion::V2(&self.claim_start).encode(encoder);
    }
}

/// As per, Sia Core a "SiacoinElement is a record of a SiacoinOutput within the state accumulator."
/// This type is effectively a "UTXO" in Bitcoin terms.
/// A SiacoinElement can be combined with a SatisfiedPolicy to create a SiacoinInputV2.
/// Ported from Sia Core:
/// <https://github.com/SiaFoundation/core/blob/b7ccbe54cccba5642c2bb9d721967214a4ba4e97/types/types.go#L619>
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiacoinElement {
    pub id: SiacoinOutputId,
    pub state_element: StateElement,
    pub siacoin_output: SiacoinOutput,
    pub maturity_height: u64,
}

impl Encodable for SiacoinElement {
    fn encode(&self, encoder: &mut Encoder) {
        self.state_element.encode(encoder);
        self.id.encode(encoder);
        SiacoinOutputVersion::V2(&self.siacoin_output).encode(encoder);
        encoder.write_u64(self.maturity_height);
    }
}

/// A UTXO with its corresponding ChainIndex. This is not a type in Sia core, but is helpful because
/// the ChainIndex is always needed when broadcasting a UTXO.
pub struct UtxoWithBasis {
    pub output: SiacoinElement,
    pub basis: ChainIndex,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiafundInputV2 {
    pub parent: SiafundElement,
    pub claim_address: Address,
    pub satisfied_policy: SatisfiedPolicy,
}

impl Encodable for SiafundInputV2 {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent.encode(encoder);
        self.claim_address.encode(encoder);
        self.satisfied_policy.encode(encoder);
    }
}

// https://github.com/SiaFoundation/core/blob/6c19657baf738c6b730625288e9b5413f77aa659/types/types.go#L197-L198
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SiacoinInputV1 {
    #[serde(rename = "parentID")]
    pub parent_id: SiacoinOutputId,
    #[serde(rename = "unlockConditions")]
    pub unlock_condition: UnlockCondition,
}

impl Encodable for SiacoinInputV1 {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent_id.0.encode(encoder);
        self.unlock_condition.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiacoinInputV2 {
    pub parent: SiacoinElement,
    pub satisfied_policy: SatisfiedPolicy,
}

impl Encodable for SiacoinInputV2 {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent.encode(encoder);
        self.satisfied_policy.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SiafundOutput {
    pub value: u64,
    pub address: Address,
}

// SiafundOutput remains the same data structure between V1 and V2 however the encoding changes
#[derive(Clone, Debug)]
pub enum SiafundOutputVersion<'a> {
    V1(&'a SiafundOutput),
    V2(&'a SiafundOutput),
}

impl<'a> Encodable for SiafundOutputVersion<'a> {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            SiafundOutputVersion::V1(v1) => {
                CurrencyVersion::V1(&Currency::from(v1.value)).encode(encoder);
                v1.address.encode(encoder);
            },
            SiafundOutputVersion::V2(v2) => {
                encoder.write_u64(v2.value);
                v2.address.encode(encoder);
            },
        }
    }
}

// SiacoinOutput remains the same data structure between V1 and V2 however the encoding changes
#[derive(Clone, Debug)]
pub enum SiacoinOutputVersion<'a> {
    V1(&'a SiacoinOutput),
    V2(&'a SiacoinOutput),
}

impl<'a> Encodable for SiacoinOutputVersion<'a> {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            SiacoinOutputVersion::V1(v1) => {
                CurrencyVersion::V1(&v1.value).encode(encoder);
                v1.address.encode(encoder);
            },
            SiacoinOutputVersion::V2(v2) => {
                CurrencyVersion::V2(&v2.value).encode(encoder);
                v2.address.encode(encoder);
            },
        }
    }
}

/// A Sia transaction id aka "txid"
// This could be a newtype like SiacoinOutputId with custom serde, but we have no use for this beyond
// making SiacoinOutputId::new more explicit.
pub type TransactionId = Hash256;

#[derive(Clone, Debug, PartialEq, From, Into, Deserialize, Serialize, Display, Default)]
#[serde(transparent)]
pub struct SiacoinOutputId(pub Hash256);

impl Encodable for SiacoinOutputId {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder)
    }
}

impl SiacoinOutputId {
    pub fn new(txid: TransactionId, index: u32) -> Self {
        let mut encoder = Encoder::default();
        encoder.write_distinguisher("id/siacoinoutput");
        txid.encode(&mut encoder);
        encoder.write_u64(index as u64);
        SiacoinOutputId(encoder.hash())
    }
}

#[derive(Clone, Debug, PartialEq, From, Into, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct SiafundOutputId(pub Hash256);

impl Encodable for SiafundOutputId {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder)
    }
}

#[derive(Clone, Debug, Default, PartialEq, From, Into, Deserialize, Serialize, Display)]
#[serde(transparent)]
pub struct FileContractID(pub Hash256);

impl Encodable for FileContractID {
    fn encode(&self, encoder: &mut Encoder) {
        self.0.encode(encoder)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SiacoinOutput {
    pub value: Currency,
    pub address: Address,
}

impl From<(Currency, Address)> for SiacoinOutput {
    fn from(tuple: (Currency, Address)) -> Self {
        SiacoinOutput {
            value: tuple.0,
            address: tuple.1,
        }
    }
}

impl From<(Currency, &Address)> for SiacoinOutput {
    fn from(tuple: (Currency, &Address)) -> Self {
        SiacoinOutput {
            value: tuple.0,
            address: tuple.1.clone(),
        }
    }
}

impl From<(Address, Currency)> for SiacoinOutput {
    fn from(tuple: (Address, Currency)) -> Self {
        SiacoinOutput {
            address: tuple.0,
            value: tuple.1,
        }
    }
}

impl From<(&Address, Currency)> for SiacoinOutput {
    fn from(tuple: (&Address, Currency)) -> Self {
        SiacoinOutput {
            address: tuple.0.clone(),
            value: tuple.1,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct CoveredFields {
    pub whole_transaction: bool,
    pub siacoin_inputs: Vec<u64>,
    pub siacoin_outputs: Vec<u64>,
    pub file_contracts: Vec<u64>,
    pub file_contract_revisions: Vec<u64>,
    pub storage_proofs: Vec<u64>,
    pub siafund_inputs: Vec<u64>,
    pub siafund_outputs: Vec<u64>,
    pub miner_fees: Vec<u64>,
    pub arbitrary_data: Vec<u64>,
    pub signatures: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionSignature {
    #[serde(rename = "parentID")]
    pub parent_id: Hash256,
    #[serde(default)]
    pub public_key_index: u64,
    #[serde(default)]
    pub timelock: u64,
    pub covered_fields: CoveredFields,
    pub signature: V1Signature,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct V1Signature(Vec<u8>);

impl<'de> Deserialize<'de> for V1Signature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct V1SignatureVisitor;

        impl<'de> serde::de::Visitor<'de> for V1SignatureVisitor {
            type Value = V1Signature;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a base64 encoded string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let decoded = base64.decode(value).map_err(serde::de::Error::custom)?;
                Ok(V1Signature(decoded))
            }
        }

        deserializer.deserialize_str(V1SignatureVisitor)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContract {
    pub filesize: u64,
    pub file_merkle_root: Hash256,
    pub window_start: u64,
    pub window_end: u64,
    #[serde(default)]
    pub payout: Currency,
    pub valid_proof_outputs: Vec<SiacoinOutput>,
    pub missed_proof_outputs: Vec<SiacoinOutput>,
    pub unlock_hash: Address,
    pub revision_number: u64,
}

impl Encodable for FileContract {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.filesize);
        self.file_merkle_root.encode(encoder);
        encoder.write_u64(self.window_start);
        encoder.write_u64(self.window_end);
        CurrencyVersion::V1(&self.payout).encode(encoder);
        encoder.write_u64(self.valid_proof_outputs.len() as u64);
        for so in &self.valid_proof_outputs {
            SiacoinOutputVersion::V1(so).encode(encoder);
        }
        encoder.write_u64(self.missed_proof_outputs.len() as u64);
        for so in &self.missed_proof_outputs {
            SiacoinOutputVersion::V1(so).encode(encoder);
        }
        self.unlock_hash.encode(encoder);
        encoder.write_u64(self.revision_number);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V2FileContract {
    pub capacity: u64,
    pub filesize: u64,
    pub file_merkle_root: Hash256,
    pub proof_height: u64,
    pub expiration_height: u64,
    pub renter_output: SiacoinOutput,
    pub host_output: SiacoinOutput,
    pub missed_host_value: Currency,
    pub total_collateral: Currency,
    pub renter_public_key: PublicKey,
    pub host_public_key: PublicKey,
    pub revision_number: u64,
    pub renter_signature: Signature,
    pub host_signature: Signature,
}

impl V2FileContract {
    pub fn with_nil_sigs(&self) -> V2FileContract {
        V2FileContract {
            renter_signature: Signature::default(),
            host_signature: Signature::default(),
            ..self.clone()
        }
    }
}

impl Encodable for V2FileContract {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.capacity);
        encoder.write_u64(self.filesize);
        self.file_merkle_root.encode(encoder);
        encoder.write_u64(self.proof_height);
        encoder.write_u64(self.expiration_height);
        SiacoinOutputVersion::V2(&self.renter_output).encode(encoder);
        SiacoinOutputVersion::V2(&self.host_output).encode(encoder);
        CurrencyVersion::V2(&self.missed_host_value).encode(encoder);
        CurrencyVersion::V2(&self.total_collateral).encode(encoder);
        self.renter_public_key.encode(encoder);
        self.host_public_key.encode(encoder);
        encoder.write_u64(self.revision_number);
        self.renter_signature.encode(encoder);
        self.host_signature.encode(encoder);
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V2FileContractElement {
    pub id: FileContractID,
    pub state_element: StateElement,
    pub v2_file_contract: V2FileContract,
}

impl Encodable for V2FileContractElement {
    fn encode(&self, encoder: &mut Encoder) {
        self.state_element.encode(encoder);
        self.id.encode(encoder);
        self.v2_file_contract.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct FileContractRevisionV2 {
    pub parent: V2FileContractElement,
    pub revision: V2FileContract,
}

impl FileContractRevisionV2 {
    pub fn with_nil_sigs(&self) -> FileContractRevisionV2 {
        FileContractRevisionV2 {
            revision: self.revision.with_nil_sigs(),
            ..self.clone()
        }
    }
}

impl Encodable for FileContractRevisionV2 {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent.encode(encoder);
        self.revision.encode(encoder);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct AttestationValue(pub Vec<u8>);

impl<'de> Deserialize<'de> for AttestationValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct AttestationValueVisitor;

        impl<'de> serde::de::Visitor<'de> for AttestationValueVisitor {
            type Value = AttestationValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a base64 encoded string representing the attestation value")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let decoded = base64.decode(value).map_err(serde::de::Error::custom)?;
                Ok(AttestationValue(decoded))
            }
        }

        deserializer.deserialize_str(AttestationValueVisitor)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Attestation {
    pub public_key: PublicKey,
    pub key: String,
    pub value: AttestationValue,
    pub signature: Signature,
}

impl Encodable for Attestation {
    fn encode(&self, encoder: &mut Encoder) {
        self.public_key.encode(encoder);
        encoder.write_string(&self.key);
        encoder.write_len_prefixed_bytes(&self.value.0);
        self.signature.encode(encoder);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Leaf(#[serde(with = "hex")] pub [u8; 64]);

impl TryFrom<String> for Leaf {
    type Error = hex::FromHexError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let bytes = hex::decode(value)?;
        let array = bytes.try_into().map_err(|_| hex::FromHexError::InvalidStringLength)?;
        Ok(Leaf(array))
    }
}

impl From<Leaf> for String {
    fn from(value: Leaf) -> Self {
        hex::encode(value.0)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StorageProof {
    #[serde(rename = "parentID")]
    pub parent_id: FileContractID,
    pub leaf: Leaf,
    pub proof: Vec<Hash256>,
}

impl Encodable for StorageProof {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent_id.encode(encoder);
        encoder.write_slice(&self.leaf.0);
        encoder.write_u64(self.proof.len() as u64);
        for proof in &self.proof {
            proof.encode(encoder);
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContractRevision {
    #[serde(rename = "parentID")]
    pub parent_id: FileContractID,
    #[serde(rename = "unlockConditions")]
    pub unlock_condition: UnlockCondition,
    #[serde(flatten)]
    pub file_contract: FileContract,
}

impl Encodable for FileContractRevision {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent_id.encode(encoder);
        self.unlock_condition.encode(encoder);
        encoder.write_u64(self.file_contract.revision_number);
        encoder.write_u64(self.file_contract.filesize);
        self.file_contract.file_merkle_root.encode(encoder);
        encoder.write_u64(self.file_contract.window_start);
        encoder.write_u64(self.file_contract.window_end);
        encoder.write_u64(self.file_contract.valid_proof_outputs.len() as u64);
        for so in &self.file_contract.valid_proof_outputs {
            SiacoinOutputVersion::V1(so).encode(encoder);
        }
        encoder.write_u64(self.file_contract.missed_proof_outputs.len() as u64);
        for so in &self.file_contract.missed_proof_outputs {
            SiacoinOutputVersion::V1(so).encode(encoder);
        }
        self.file_contract.unlock_hash.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiafundInputV1 {
    #[serde(rename = "parentID")]
    pub parent_id: SiafundOutputId,
    #[serde(rename = "unlockConditions")]
    pub unlock_condition: UnlockCondition,
    pub claim_address: Address,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ResolutionType {
    Renewal,
    StorageProof,
    Expiration,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct V2FileContractResolution {
    pub parent: V2FileContractElement,
    #[serde(rename = "type")]
    pub resolution_type: ResolutionType,
    pub resolution: V2FileContractResolutionWrapper,
}

impl Encodable for V2FileContractResolution {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent.encode(encoder);
        // Determine resolution type from the resolution itself, matching Go implementation
        match &self.resolution {
            V2FileContractResolutionWrapper::Renewal(_) => {
                encoder.write_u8(0);
            },
            V2FileContractResolutionWrapper::StorageProof(_) => {
                encoder.write_u8(1);
            },
            V2FileContractResolutionWrapper::Expiration => {
                encoder.write_u8(2);
            },
        }
        self.resolution.encode(encoder);
    }
}

impl<'de> Deserialize<'de> for V2FileContractResolution {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        struct V2FileContractResolutionHelper {
            parent: V2FileContractElement,
            #[serde(rename = "type")]
            resolution_type: ResolutionType,
            resolution: Value,
        }

        let helper = V2FileContractResolutionHelper::deserialize(deserializer)?;

        let resolution_data = match helper.resolution_type {
            ResolutionType::Renewal => serde_json::from_value::<V2FileContractRenewal>(helper.resolution)
                .map(|data| V2FileContractResolutionWrapper::Renewal(Box::new(data)))
                .map_err(serde::de::Error::custom),
            ResolutionType::StorageProof => serde_json::from_value::<V2StorageProof>(helper.resolution)
                .map(V2FileContractResolutionWrapper::StorageProof)
                .map_err(serde::de::Error::custom),
            // expiration is a special case because it has no data. It is just an empty object, "{}".
            ResolutionType::Expiration => match &helper.resolution {
                Value::Object(map) if map.is_empty() => Ok(V2FileContractResolutionWrapper::Expiration),
                _ => Err(serde::de::Error::custom("expected an empty map for expiration")),
            },
        }?;

        Ok(V2FileContractResolution {
            parent: helper.parent,
            resolution_type: helper.resolution_type,
            resolution: resolution_data,
        })
    }
}

impl Encodable for V2FileContractResolutionWrapper {
    fn encode(&self, encoder: &mut Encoder) {
        match self {
            V2FileContractResolutionWrapper::Renewal(r) => {
                r.encode(encoder);
            },
            V2FileContractResolutionWrapper::StorageProof(s) => {
                s.encode(encoder);
            },
            V2FileContractResolutionWrapper::Expiration => {
                // Expiration has no data, nothing to encode
            },
        }
    }
}

impl V2FileContractResolution {
    pub fn with_nil_sigs(&self) -> V2FileContractResolution {
        V2FileContractResolution {
            resolution: self.resolution.with_nil_sigs(),
            ..self.clone()
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum V2FileContractResolutionWrapper {
    Renewal(Box<V2FileContractRenewal>),
    StorageProof(V2StorageProof),
    #[serde(serialize_with = "serialize_variant_as_empty_object")]
    Expiration,
}

fn serialize_variant_as_empty_object<S>(serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str("{}")
}

impl V2FileContractResolutionWrapper {
    fn with_nil_sigs(&self) -> V2FileContractResolutionWrapper {
        match self {
            V2FileContractResolutionWrapper::Renewal(r) => {
                V2FileContractResolutionWrapper::Renewal(Box::new(r.with_nil_sigs()))
            },
            V2FileContractResolutionWrapper::StorageProof(s) => {
                V2FileContractResolutionWrapper::StorageProof(s.with_nil_merkle_proof())
            },
            V2FileContractResolutionWrapper::Expiration => V2FileContractResolutionWrapper::Expiration,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V2FileContractRenewal {
    final_renter_output: SiacoinOutput,
    final_host_output: SiacoinOutput,
    new_contract: V2FileContract,
    renter_rollover: Currency,
    host_rollover: Currency,
    renter_signature: Signature,
    host_signature: Signature,
}

impl V2FileContractRenewal {
    pub fn with_nil_sigs(&self) -> V2FileContractRenewal {
        V2FileContractRenewal {
            new_contract: self.new_contract.with_nil_sigs(),
            renter_signature: Signature::default(),
            host_signature: Signature::default(),
            ..self.clone()
        }
    }
}

// TODO unit test
impl Encodable for V2FileContractRenewal {
    fn encode(&self, encoder: &mut Encoder) {
        SiacoinOutputVersion::V2(&self.final_renter_output).encode(encoder);
        SiacoinOutputVersion::V2(&self.final_host_output).encode(encoder);
        CurrencyVersion::V2(&self.renter_rollover).encode(encoder);
        CurrencyVersion::V2(&self.host_rollover).encode(encoder);
        self.new_contract.encode(encoder);
        self.renter_signature.encode(encoder);
        self.host_signature.encode(encoder);
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct V2StorageProof {
    proof_index: ChainIndexElement,
    leaf: Leaf,
    proof: Vec<Hash256>,
}

impl V2StorageProof {
    pub fn with_nil_merkle_proof(&self) -> V2StorageProof {
        V2StorageProof {
            proof_index: ChainIndexElement {
                state_element: StateElement {
                    merkle_proof: vec![],
                    ..self.proof_index.state_element.clone()
                },
                ..self.proof_index.clone()
            },
            ..self.clone()
        }
    }
}

// TODO unit test
impl Encodable for V2StorageProof {
    fn encode(&self, encoder: &mut Encoder) {
        self.proof_index.encode(encoder);
        encoder.write_slice(&self.leaf.0);
        encoder.write_u64(self.proof.len() as u64);
        for proof in &self.proof {
            proof.encode(encoder);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChainIndexElement {
    pub id: BlockId,
    pub state_element: StateElement,
    pub chain_index: ChainIndex,
}

// TODO unit test
impl Encodable for ChainIndexElement {
    fn encode(&self, encoder: &mut Encoder) {
        self.state_element.encode(encoder);
        self.id.encode(encoder);
        self.chain_index.encode(encoder);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContractElementV1 {
    pub id: FileContractID,
    pub state_element: StateElement,
    pub file_contract: FileContractV1,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContractV1 {
    pub filesize: u64,
    pub file_merkle_root: Hash256,
    pub window_start: u64,
    pub window_end: u64,
    pub payout: Currency,
    pub valid_proof_outputs: Vec<SiacoinOutput>,
    pub missed_proof_outputs: Vec<SiacoinOutput>,
    pub unlock_hash: Address,
    pub revision_number: u64,
}

/*
While implementing
, we faced two options.
    1.) Treat every field as an Option<>
    2.) Always initialize every empty field as a Vec<>

We chose the latter as it allows for simpler encoding of this struct.
It is possible this may need to change in later implementations.
*/
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct V1Transaction {
    pub siacoin_inputs: Vec<SiacoinInputV1>,
    pub siacoin_outputs: Vec<SiacoinOutput>,
    pub file_contracts: Vec<FileContract>,
    pub file_contract_revisions: Vec<FileContractRevision>,
    pub storage_proofs: Vec<StorageProof>,
    pub siafund_inputs: Vec<SiafundInputV1>,
    pub siafund_outputs: Vec<SiafundOutput>,
    pub miner_fees: Vec<Currency>,
    pub arbitrary_data: Vec<ArbitraryData>,
    pub signatures: Vec<TransactionSignature>,
    pub id: Option<TransactionId>,
}

impl V1Transaction {
    pub fn txid(&self) -> Hash256 {
        Encoder::encode_and_hash(&V1TransactionSansSigs(self.clone()))
    }
}

impl Encodable for SiafundInputV1 {
    fn encode(&self, encoder: &mut Encoder) {
        self.parent_id.encode(encoder);
        self.unlock_condition.encode(encoder);
        self.claim_address.encode(encoder);
    }
}
// TODO possible this can just hold a ref to V1Transaction like CurrencyVersion
#[derive(Clone, Debug, Default, Deref, Deserialize, Serialize)]
pub struct V1TransactionSansSigs(V1Transaction);

impl Encodable for V1TransactionSansSigs {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_len_prefixed_vec(&self.siacoin_inputs);

        encoder.write_u64(self.siacoin_outputs.len() as u64);
        for so in &self.siacoin_outputs {
            SiacoinOutputVersion::V1(so).encode(encoder);
        }
        encoder.write_len_prefixed_vec(&self.file_contracts);
        encoder.write_len_prefixed_vec(&self.file_contract_revisions);
        encoder.write_len_prefixed_vec(&self.storage_proofs);
        encoder.write_len_prefixed_vec(&self.siafund_inputs);

        encoder.write_u64(self.siafund_outputs.len() as u64);
        for so in &self.siafund_outputs {
            SiafundOutputVersion::V1(so).encode(encoder);
        }

        encoder.write_u64(self.miner_fees.len() as u64);
        for so in &self.miner_fees {
            CurrencyVersion::V1(so).encode(encoder);
        }

        encoder.write_u64(self.arbitrary_data.len() as u64);
        for data_vec in &self.arbitrary_data {
            data_vec.encode(encoder);
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct V2Transaction {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub siacoin_inputs: Vec<SiacoinInputV2>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub siacoin_outputs: Vec<SiacoinOutput>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub siafund_inputs: Vec<SiafundInputV2>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub siafund_outputs: Vec<SiafundOutput>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_contracts: Vec<V2FileContract>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_contract_revisions: Vec<FileContractRevisionV2>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file_contract_resolutions: Vec<V2FileContractResolution>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attestations: Vec<Attestation>,
    #[serde(skip_serializing_if = "ArbitraryData::is_empty")]
    pub arbitrary_data: ArbitraryData,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_foundation_address: Option<Address>,
    pub miner_fee: Currency,
    // Basis is not part of the transaction structure. This field is unique to the Rust implementation.
    // This field is provided to keep track of the correct ChainIndex needed to broadcast the transaction.
    #[serde(skip)]
    pub basis: Option<ChainIndex>,
}

impl V2Transaction {
    pub fn with_nil_sigs(&self) -> V2Transaction {
        V2Transaction {
            file_contracts: self.file_contracts.clone(),
            file_contract_revisions: self.file_contract_revisions.clone(),
            file_contract_resolutions: self.file_contract_resolutions.clone(),
            ..self.clone()
        }
    }

    pub fn input_sig_hash(&self) -> Hash256 {
        let mut encoder = Encoder::default();
        encoder.write_distinguisher("sig/input");
        encoder.write_u8(V2_REPLAY_PREFIX);
        self.encode(&mut encoder);
        encoder.hash()
    }

    pub fn txid(&self) -> TransactionId {
        let mut encoder = Encoder::default();
        encoder.write_distinguisher("id/transaction");
        self.encode(&mut encoder);
        encoder.hash()
    }
}

// this encoding corresponds to the Go implementation's "V2TransactionSemantics" rather than "V2Transaction"
impl Encodable for V2Transaction {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_u64(self.siacoin_inputs.len() as u64);
        for si in &self.siacoin_inputs {
            si.parent.id.encode(encoder);
        }

        encoder.write_u64(self.siacoin_outputs.len() as u64);
        for so in &self.siacoin_outputs {
            SiacoinOutputVersion::V2(so).encode(encoder);
        }

        encoder.write_u64(self.siafund_inputs.len() as u64);
        for si in &self.siafund_inputs {
            si.parent.id.encode(encoder);
        }

        encoder.write_u64(self.siafund_outputs.len() as u64);
        for so in &self.siafund_outputs {
            SiafundOutputVersion::V2(so).encode(encoder);
        }

        encoder.write_u64(self.file_contracts.len() as u64);
        for fc in &self.file_contracts {
            fc.with_nil_sigs().encode(encoder);
        }

        encoder.write_u64(self.file_contract_revisions.len() as u64);
        for fcr in &self.file_contract_revisions {
            fcr.parent.id.encode(encoder);
            fcr.revision.with_nil_sigs().encode(encoder);
        }

        encoder.write_u64(self.file_contract_resolutions.len() as u64);
        for fcr in &self.file_contract_resolutions {
            fcr.parent.id.encode(encoder);
            // Encode only the resolution data (without type), matching Go V2TransactionSemantics implementation
            // The type is not encoded in V2TransactionSemantics, only the resolution data itself
            let normalized_resolution = fcr.resolution.with_nil_sigs();
            normalized_resolution.encode(encoder);
        }

        encoder.write_u64(self.attestations.len() as u64);
        for att in &self.attestations {
            att.encode(encoder);
        }

        self.arbitrary_data.encode(encoder);

        encoder.write_bool(self.new_foundation_address.is_some());
        match &self.new_foundation_address {
            Some(addr) => addr.encode(encoder),
            None => (),
        }
        CurrencyVersion::V2(&self.miner_fee).encode(encoder);
    }
}

#[derive(Clone, Debug, Default, PartialEq, From, Into)]
pub struct ArbitraryData(pub Vec<u8>);

impl ArbitraryData {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Encodable for ArbitraryData {
    fn encode(&self, encoder: &mut Encoder) {
        encoder.write_len_prefixed_bytes(&self.0);
    }
}

impl Serialize for ArbitraryData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&base64.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for ArbitraryData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ArbitraryDataVisitor;

        impl<'de> serde::de::Visitor<'de> for ArbitraryDataVisitor {
            type Value = ArbitraryData;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a base64 encoded string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let decoded = base64.decode(value).map_err(serde::de::Error::custom)?;
                Ok(ArbitraryData(decoded))
            }
        }

        deserializer.deserialize_str(ArbitraryDataVisitor)
    }
}
