//! EIP-712 typed structured data hashing.
//!
//! Implements the encoding and hashing algorithm described in
//! [EIP-712](https://eips.ethereum.org/EIPS/eip-712) for signing typed data.
//!
//! # Overview
//!
//! 1. Define domain and message types with [`TypeDef`] and [`FieldKind`].
//! 2. Assemble them into an [`Eip712`] struct.
//! 3. Call [`hash_typed_data`] to produce the 32-byte digest ready for signing.

// web3::Error is large due to error-chain; nothing we can do about it.
#![allow(clippy::result_large_err)]

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use std::str::FromStr;
use tiny_keccak::{Hasher, Keccak};
use web3::error::ErrorKind as Web3ErrorKind;

/// 32-byte hash output.
pub type H256 = [u8; 32];

/// Name of the mandatory domain separator type.
const DOMAIN_TYPE_NAME: &str = "EIP712Domain";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Describes one named field within a structured type.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TypedField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
}

/// A named structured type with an ordered list of fields.
///
/// Use the builder methods to define types ergonomically:
/// ```ignore
/// TypeDef::new("Mail")
///     .field("from", FieldKind::Address)
///     .field("contents", FieldKind::String);
/// ```
#[derive(Clone, Debug)]
pub struct TypeDef {
    pub name: String,
    pub fields: Vec<TypedField>,
}

impl TypeDef {
    /// Domain separator type (must be named `EIP712Domain`).
    pub fn domain() -> Self {
        Self {
            name: DOMAIN_TYPE_NAME.to_string(),
            fields: Vec::new(),
        }
    }

    /// Arbitrary custom type.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            fields: Vec::new(),
        }
    }

    /// Appends a field and returns `&mut Self` for chaining.
    pub fn field(&mut self, name: &str, kind: FieldKind) -> &mut Self {
        self.fields.push(TypedField {
            name: name.to_string(),
            field_type: kind.to_string(),
        });
        self
    }
}

/// The supported EIP-712 atomic and reference types.
#[derive(Clone, Debug)]
pub enum FieldKind {
    Bool,
    String,
    Uint256,
    Address,
    Bytes32,
    /// A reference to another structured type by name.
    Struct(String),
}

impl fmt::Display for FieldKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Bool => f.write_str("bool"),
            Self::String => f.write_str("string"),
            Self::Uint256 => f.write_str("uint256"),
            Self::Address => f.write_str("address"),
            Self::Bytes32 => f.write_str("bytes32"),
            Self::Struct(s) => f.write_str(s),
        }
    }
}

impl FromStr for FieldKind {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bool" => Ok(Self::Bool),
            "string" => Ok(Self::String),
            "uint256" => Ok(Self::Uint256),
            "address" => Ok(Self::Address),
            "bytes32" => Ok(Self::Bytes32),
            other => Ok(Self::Struct(other.to_string())),
        }
    }
}

/// The top-level EIP-712 typed data container.
///
/// Generic over the domain and message types, both of which must be
/// serializable to JSON objects so the encoder can walk their fields.
#[derive(Debug, Serialize)]
pub struct Eip712<D, M> {
    /// Custom type definitions (including `EIP712Domain`).
    pub types: IndexMap<String, Vec<TypedField>>,
    /// The domain separator values.
    pub domain: D,
    /// The primary type name for the signed message.
    #[serde(rename = "primaryType")]
    pub primary_type: String,
    /// The actual message data to sign.
    pub message: M,
}

// ---------------------------------------------------------------------------
// Core hashing entry point
// ---------------------------------------------------------------------------

/// Hashes EIP-712 typed structured data, returning the 32-byte digest
/// suitable for ECDSA signing.
///
/// Implements `keccak256("\x19\x01" ‖ domainSeparator ‖ hashStruct(message))`
/// per the specification.
pub fn hash_typed_data<D, M>(data: Eip712<D, M>) -> Result<H256, web3::Error>
where
    D: Serialize,
    M: Serialize,
{
    let types = &data.types;

    let domain_json = serde_json::to_value(&data.domain).map_err(encode_err)?;
    let message_json = serde_json::to_value(&data.message).map_err(encode_err)?;

    let domain_hash = hash_struct(types, DOMAIN_TYPE_NAME, &domain_json)?;
    let message_hash = hash_struct(types, &data.primary_type, &message_json)?;

    // EIP-191 version 0x01: "\x19\x01" prefix
    let mut buf = Vec::with_capacity(2 + 32 + 32);
    buf.push(0x19);
    buf.push(0x01);
    buf.extend_from_slice(&domain_hash);
    buf.extend_from_slice(&message_hash);

    Ok(keccak256(&buf))
}

// ---------------------------------------------------------------------------
// Internal encoding (per EIP-712 spec)
// ---------------------------------------------------------------------------

type TypeRegistry = IndexMap<String, Vec<TypedField>>;

fn hash_struct(types: &TypeRegistry, type_name: &str, data: &serde_json::Value) -> Result<[u8; 32], web3::Error> {
    let mut encoded = type_hash(types, type_name)?;

    let fields = types
        .get(type_name)
        .ok_or_else(|| encode_err(format!("unknown type '{}'", type_name)))?;

    for field in fields {
        let val = &data[&field.name];
        let field_bytes = encode_field(types, &field.field_type, val, Some(&field.name))?;
        encoded.extend_from_slice(&field_bytes);
    }

    Ok(keccak256(&encoded))
}

fn type_hash(types: &TypeRegistry, type_name: &str) -> Result<Vec<u8>, web3::Error> {
    let encoded_type = encode_type_string(types, type_name)?;
    Ok(keccak256(encoded_type.as_bytes()).to_vec())
}

/// Builds the canonical type encoding string, including sorted dependencies.
///
/// E.g. `Mail(Person from,Person to,string contents)Person(string name,address wallet)`
fn encode_type_string(types: &TypeRegistry, primary: &str) -> Result<String, web3::Error> {
    let mut deps = collect_dependencies(types, primary);
    // Remove the primary from deps and sort the rest alphabetically.
    deps.remove(primary);
    let mut sorted_deps: Vec<&str> = deps.into_iter().collect();
    sorted_deps.sort_unstable();

    let mut result = format_single_type(types, primary)?;
    for dep in sorted_deps {
        result.push_str(&format_single_type(types, dep)?);
    }
    Ok(result)
}

fn format_single_type(types: &TypeRegistry, name: &str) -> Result<String, web3::Error> {
    let fields = types
        .get(name)
        .ok_or_else(|| encode_err(format!("unknown type '{}'", name)))?;

    let params: Vec<String> = fields.iter().map(|f| format!("{} {}", f.field_type, f.name)).collect();
    Ok(format!("{}({})", name, params.join(",")))
}

/// Recursively collects all struct types referenced by `type_name`.
fn collect_dependencies<'a>(types: &'a TypeRegistry, type_name: &'a str) -> HashSet<&'a str> {
    let mut deps = HashSet::new();
    collect_deps_recursive(types, type_name, &mut deps);
    deps
}

fn collect_deps_recursive<'a>(types: &'a TypeRegistry, type_name: &'a str, visited: &mut HashSet<&'a str>) {
    if !visited.insert(type_name) {
        return;
    }
    if let Some(fields) = types.get(type_name) {
        for field in fields {
            if types.contains_key(&field.field_type) {
                collect_deps_recursive(types, &field.field_type, visited);
            }
        }
    }
}

/// Encodes a single field value according to its EIP-712 type.
fn encode_field(
    types: &TypeRegistry,
    field_type: &str,
    value: &serde_json::Value,
    field_name: Option<&str>,
) -> Result<Vec<u8>, web3::Error> {
    // If the field type is a known custom struct, hash it recursively.
    if types.contains_key(field_type) {
        let hash = hash_struct(types, field_type, value)?;
        return Ok(hash.to_vec());
    }

    match field_type {
        "bool" => encode_bool(value, field_name),
        "string" => encode_string(value, field_name),
        "uint256" => encode_uint256(value, field_name),
        "address" => encode_address(value, field_name),
        "bytes32" => encode_bytes32(value, field_name),
        other => Err(encode_err(format!(
            "unsupported type '{}' for field {:?}",
            other,
            field_name.unwrap_or("<root>")
        ))),
    }
}

fn encode_bool(val: &serde_json::Value, ctx: Option<&str>) -> Result<Vec<u8>, web3::Error> {
    let b = val.as_bool().ok_or_else(|| type_error("bool", val, ctx))?;
    let mut out = [0u8; 32];
    if b {
        out[31] = 1;
    }
    Ok(out.to_vec())
}

fn encode_string(val: &serde_json::Value, ctx: Option<&str>) -> Result<Vec<u8>, web3::Error> {
    let s = val.as_str().ok_or_else(|| type_error("string", val, ctx))?;
    Ok(keccak256(s.as_bytes()).to_vec())
}

fn encode_uint256(val: &serde_json::Value, ctx: Option<&str>) -> Result<Vec<u8>, web3::Error> {
    let num_str = match val {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        _ => return Err(type_error("uint256", val, ctx)),
    };

    // Parse as u128 first; full U256 support can be extended later if needed.
    let n: u128 = num_str
        .parse()
        .map_err(|e| encode_err(format!("invalid uint256 '{}': {} (field {:?})", num_str, e, ctx)))?;

    let mut out = [0u8; 32];
    out[16..32].copy_from_slice(&n.to_be_bytes());
    Ok(out.to_vec())
}

fn encode_address(val: &serde_json::Value, ctx: Option<&str>) -> Result<Vec<u8>, web3::Error> {
    let s = val.as_str().ok_or_else(|| type_error("address", val, ctx))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    validate_hex(s, ctx)?;
    let bytes = hex::decode(s).map_err(|e| encode_err(format!("invalid address hex: {} (field {:?})", e, ctx)))?;
    if bytes.len() != 20 {
        return Err(encode_err(format!(
            "address must be 20 bytes, got {} (field {:?})",
            bytes.len(),
            ctx
        )));
    }
    let mut out = [0u8; 32];
    out[12..32].copy_from_slice(&bytes);
    Ok(out.to_vec())
}

fn encode_bytes32(val: &serde_json::Value, ctx: Option<&str>) -> Result<Vec<u8>, web3::Error> {
    let s = val.as_str().ok_or_else(|| type_error("bytes32", val, ctx))?;
    let s = s.strip_prefix("0x").unwrap_or(s);
    validate_hex(s, ctx)?;
    let bytes = hex::decode(s).map_err(|e| encode_err(format!("invalid bytes32 hex: {} (field {:?})", e, ctx)))?;
    if bytes.len() != 32 {
        return Err(encode_err(format!(
            "bytes32 must be 32 bytes, got {} (field {:?})",
            bytes.len(),
            ctx
        )));
    }
    Ok(bytes)
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Keccak::v256();
    let mut out = [0u8; 32];
    hasher.update(data);
    hasher.finalize(&mut out);
    out
}

fn validate_hex(s: &str, ctx: Option<&str>) -> Result<(), web3::Error> {
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(encode_err(format!("invalid hex characters (field {:?})", ctx)));
    }
    Ok(())
}

fn type_error(expected: &str, found: &serde_json::Value, ctx: Option<&str>) -> web3::Error {
    encode_err(format!(
        "expected {} but found {:?} (field {:?})",
        expected,
        found,
        ctx.unwrap_or("<root>"),
    ))
}

fn encode_err<E: fmt::Display>(e: E) -> web3::Error {
    Web3ErrorKind::Decoder(e.to_string()).into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference test from the EIP-712 specification (the "Mail" example).
    #[test]
    fn eip712_mail_example() {
        let mut types = IndexMap::new();

        types.insert(
            "EIP712Domain".to_string(),
            vec![
                TypedField {
                    name: "name".into(),
                    field_type: "string".into(),
                },
                TypedField {
                    name: "version".into(),
                    field_type: "string".into(),
                },
                TypedField {
                    name: "chainId".into(),
                    field_type: "uint256".into(),
                },
                TypedField {
                    name: "verifyingContract".into(),
                    field_type: "address".into(),
                },
            ],
        );

        types.insert(
            "Person".to_string(),
            vec![
                TypedField {
                    name: "name".into(),
                    field_type: "string".into(),
                },
                TypedField {
                    name: "wallet".into(),
                    field_type: "address".into(),
                },
            ],
        );

        types.insert(
            "Mail".to_string(),
            vec![
                TypedField {
                    name: "from".into(),
                    field_type: "Person".into(),
                },
                TypedField {
                    name: "to".into(),
                    field_type: "Person".into(),
                },
                TypedField {
                    name: "contents".into(),
                    field_type: "string".into(),
                },
            ],
        );

        #[derive(Serialize)]
        struct Domain {
            name: String,
            version: String,
            #[serde(rename = "chainId")]
            chain_id: u64,
            #[serde(rename = "verifyingContract")]
            verifying_contract: String,
        }

        #[derive(Serialize)]
        struct Person {
            name: String,
            wallet: String,
        }

        #[derive(Serialize)]
        struct Mail {
            from: Person,
            to: Person,
            contents: String,
        }

        let data = Eip712 {
            types,
            domain: Domain {
                name: "Ether Mail".into(),
                version: "1".into(),
                chain_id: 1,
                verifying_contract: "0xCcCCccccCCCCcCCCCCCcCcCccCcCCCcCcccccccC".into(),
            },
            primary_type: "Mail".into(),
            message: Mail {
                from: Person {
                    name: "Cow".into(),
                    wallet: "0xCD2a3d9F938E13CD947Ec05AbC7FE734Df8DD826".into(),
                },
                to: Person {
                    name: "Bob".into(),
                    wallet: "0xbBbBBBBbbBBBbbbBbbBbbbbBBbBbbbbBbBbbBBbB".into(),
                },
                contents: "Hello, Bob!".into(),
            },
        };

        let hash = hash_typed_data(data).unwrap();
        // Expected hash from the EIP-712 specification reference implementation.
        let expected = "be609aee343fb3c4b28e1df9e632fca64fcfaede20f02e86244efddf30957bd2";
        assert_eq!(hex::encode(hash), expected);
    }

    #[test]
    fn type_encoding_string() {
        let mut types = IndexMap::new();
        types.insert(
            "Person".to_string(),
            vec![
                TypedField {
                    name: "name".into(),
                    field_type: "string".into(),
                },
                TypedField {
                    name: "wallet".into(),
                    field_type: "address".into(),
                },
            ],
        );
        types.insert(
            "Mail".to_string(),
            vec![
                TypedField {
                    name: "from".into(),
                    field_type: "Person".into(),
                },
                TypedField {
                    name: "to".into(),
                    field_type: "Person".into(),
                },
                TypedField {
                    name: "contents".into(),
                    field_type: "string".into(),
                },
            ],
        );

        let encoded = encode_type_string(&types, "Mail").unwrap();
        assert_eq!(
            encoded,
            "Mail(Person from,Person to,string contents)Person(string name,address wallet)"
        );
    }

    #[test]
    fn field_kind_round_trip() {
        let kinds = ["bool", "string", "uint256", "address", "bytes32", "Person"];
        for s in &kinds {
            let k: FieldKind = s.parse().unwrap();
            assert_eq!(&k.to_string(), s);
        }
    }
}
