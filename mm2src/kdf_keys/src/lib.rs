// Bitcoin-family key, address and signature primitives.
//
// Clean-room implementation. Origin: public protocol specifications
// (BIP-32 / BIP-141 / BIP-144 / BIP-173 / BIP-350, BCH CashAddr,
// ZIP-203). Hashing primitives come from the KDF-original `kdf_crypto`
// crate; secp256k1 ECDSA from the upstream `secp256k1` crate
// (CC0-1.0 / MIT).

#![allow(unused_assignments)] // CashAddress fields used in cfg-gated code paths

use lazy_static::lazy_static;
use secp256k1::{Secp256k1, SignOnly, VerifyOnly};
use std::fmt;

pub use primitives::{bytes, hash};

mod address;
mod cashaddr;
mod display;
mod error;
mod keypair;
mod network;
mod private;
mod public;
mod segwit;
mod signature;

pub use address::{Address, AddressFormat, Type};
pub use cashaddr::{AddressType as CashAddrType, CashAddress, NetworkPrefix};
pub use display::DisplayLayout;
pub use error::Error;
pub use keypair::KeyPair;
pub use network::Network;
pub use private::Private;
pub use public::Public;
pub use segwit::SegwitAddress;
pub use signature::{CompactSignature, Signature};

use hash::{H160, H256};

/// Raw 32-byte ECDSA secret key.
pub type Secret = H256;

/// Raw 32-byte signable message digest.
pub type Message = H256;

lazy_static! {
    /// Process-wide secp256k1 verifier (verify-only context).
    pub static ref SECP_VERIFY: Secp256k1<VerifyOnly> = Secp256k1::verification_only();
    /// Process-wide secp256k1 signer (sign-only context).
    pub static ref SECP_SIGN: Secp256k1<SignOnly> = Secp256k1::signing_only();
}

/// A Bitcoin-family `script_pubkey` hash payload. Either a 20-byte
/// `RIPEMD160(SHA256(pubkey|script))` (used by P2PKH/P2SH/P2WPKH) or a
/// 32-byte `SHA256(script)` (used by P2WSH).
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AddressHashEnum {
    AddressHash(H160),
    WitnessScriptHash(H256),
}

impl AddressHashEnum {
    pub fn default_address_hash() -> Self { AddressHashEnum::AddressHash(H160::default()) }

    pub fn default_witness_script_hash() -> Self { AddressHashEnum::WitnessScriptHash(H256::default()) }

    pub fn copy_from_slice(&mut self, src: &[u8]) {
        match self {
            AddressHashEnum::AddressHash(h) => h.copy_from_slice(src),
            AddressHashEnum::WitnessScriptHash(s) => s.copy_from_slice(src),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        match self {
            AddressHashEnum::AddressHash(h) => h.to_vec(),
            AddressHashEnum::WitnessScriptHash(s) => s.to_vec(),
        }
    }

    pub fn is_address_hash(&self) -> bool { matches!(self, AddressHashEnum::AddressHash(_)) }
    pub fn is_witness_script_hash(&self) -> bool { matches!(self, AddressHashEnum::WitnessScriptHash(_)) }
}

impl fmt::Display for AddressHashEnum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AddressHashEnum::AddressHash(h) => f.write_str(&h.to_string()),
            AddressHashEnum::WitnessScriptHash(s) => f.write_str(&s.to_string()),
        }
    }
}

impl From<H160> for AddressHashEnum {
    fn from(hash: H160) -> Self { AddressHashEnum::AddressHash(hash) }
}
