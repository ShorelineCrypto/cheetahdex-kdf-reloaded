//! TRON blockchain support for KDF-RELOADED.
//!
//! TRON is an EVM-compatible blockchain with its own address format (Base58Check
//! with 0x41 prefix), protobuf-based transaction serialization, and a dual
//! bandwidth+energy fee model. TRC20 tokens reuse ERC20 ABI encoding.

pub mod address;
pub mod api;
pub mod fee;
pub mod proto;
pub mod sign;
pub mod tx_builder;

pub use address::TronAddress;

use serde::{Deserialize, Serialize};

/// TRX uses 6 decimal places (1 TRX = 1,000,000 SUN).
pub const TRX_DECIMALS: u8 = 6;

/// HTTP request timeout for TRON API calls.
pub const TRON_API_TIMEOUT_SEC: u64 = 10;

/// TRON network variants.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum Network {
    /// TRON Mainnet
    Mainnet,
    /// TRON Shasta testnet
    Shasta,
    /// TRON Nile testnet
    Nile,
}

impl Default for Network {
    fn default() -> Self { Network::Mainnet }
}
