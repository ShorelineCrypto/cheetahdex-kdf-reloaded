//! # Purpose
//!
//! A single, centralised EVM error type that consolidates what the legacy
//! code spreads across seven `impl From<web3::Error>` blocks plus assorted
//! `ethabi::Error` mappings. Every alloy fallible boundary
//! (`alloy::transports::TransportError`, `alloy::contract::Error`,
//! `alloy::signers::Error`, `serde_json::Error`) funnels into the variants
//! defined here, and the existing downstream error types
//! (`Web3RpcError`, `WithdrawError`, `BalanceError`, `TradePreimageError`,
//! `RawTransactionError`) keep their public shapes via simple
//! `From<EvmError>` impls in `eth_types.rs`.
//!
//! # Public exports
//!
//! - [`EvmError`] — the unified EVM-stack error.
//! - [`EvmResult`] — `Result<T, EvmError>` shorthand for internal call-sites.
//!
//! # Invariants
//!
//! - **Category preservation:** every variant maps onto a single category
//!   in the legacy `Web3RpcError` (`Transport`, `InvalidResponse`,
//!   `Internal`); the per-variant comments below specify the mapping.
//!   Downstream classification used by retry loops and HTTP status codes
//!   is therefore unchanged after migration.
//! - **No leaking secrets:** `Display`/`Debug` strings only contain the
//!   alloy error message; they never embed transaction payloads,
//!   private keys, mnemonics, or RPC URLs (per
//!   `RELOADED-CODING-STANDARDS.md` §0).
//! - **Send + Sync + 'static:** required so the type can flow through
//!   `MmError`, async tasks, and `Box<dyn std::error::Error + Send>`
//!   conversions used by the swap state machines.

use std::error::Error as StdError;
use std::fmt;

use alloy::contract::Error as AlloyContractError;
use alloy::signers::Error as AlloySignerError;
use alloy::transports::TransportError as AlloyTransportError;

/// Unified error for every EVM RPC / contract / signer fallible boundary.
///
/// Construct one via the provided `From` impls; the variants are not
/// expected to be matched on at call-sites — error classification happens
/// once at the boundary between this module and the public coin error
/// types.
#[derive(Debug)]
pub enum EvmError {
    /// Network-layer failure (HTTP error, connection reset, timeout).
    /// Maps to `Web3RpcError::Transport` — retryable.
    Transport(String),
    /// Server returned a structurally invalid JSON-RPC response or one
    /// whose result could not be deserialised into the expected shape.
    /// Maps to `Web3RpcError::InvalidResponse` — non-retryable.
    InvalidResponse(String),
    /// Local decode error (ABI, RLP, hex). Indicates a programmer error
    /// rather than a network problem. Maps to `Web3RpcError::Internal`.
    Decode(String),
    /// Smart-contract execution error (revert, out-of-gas, invalid op).
    /// Maps to `Web3RpcError::InvalidResponse` because the chain accepted
    /// the call but produced no usable result.
    Contract(String),
    /// Signer-side failure (HSM unavailable, derivation failure, user
    /// rejection in MetaMask). Maps to `Web3RpcError::Internal`.
    Signer(String),
    /// Anything else originating in the EVM stack itself rather than a
    /// caller mistake. Maps to `Web3RpcError::Internal`.
    Internal(String),
}

/// Internal `Result` shorthand. Public coin errors continue to use
/// `MmResult<_, Web3RpcError>` etc.; conversion happens at the seam.
pub type EvmResult<T> = Result<T, EvmError>;

impl fmt::Display for EvmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvmError::Transport(m) => write!(f, "EVM transport error: {m}"),
            EvmError::InvalidResponse(m) => write!(f, "EVM invalid response: {m}"),
            EvmError::Decode(m) => write!(f, "EVM decode error: {m}"),
            EvmError::Contract(m) => write!(f, "EVM contract error: {m}"),
            EvmError::Signer(m) => write!(f, "EVM signer error: {m}"),
            EvmError::Internal(m) => write!(f, "EVM internal error: {m}"),
        }
    }
}

impl StdError for EvmError {}

impl From<AlloyTransportError> for EvmError {
    fn from(e: AlloyTransportError) -> Self {
        // alloy's `TransportError` already distinguishes transport vs
        // deserialization vs error-response cases; preserve that
        // categorisation rather than collapsing everything to Transport.
        match &e {
            AlloyTransportError::Transport(_) => EvmError::Transport(e.to_string()),
            AlloyTransportError::SerError(_) | AlloyTransportError::DeserError { .. } => {
                EvmError::InvalidResponse(e.to_string())
            },
            AlloyTransportError::ErrorResp(_) => EvmError::InvalidResponse(e.to_string()),
            AlloyTransportError::NullResp => EvmError::InvalidResponse("null response".to_owned()),
            AlloyTransportError::UnsupportedFeature(_) => EvmError::Internal(e.to_string()),
            AlloyTransportError::LocalUsageError(_) => EvmError::Internal(e.to_string()),
        }
    }
}

impl From<AlloyContractError> for EvmError {
    fn from(e: AlloyContractError) -> Self {
        match &e {
            AlloyContractError::TransportError(_) => EvmError::Transport(e.to_string()),
            AlloyContractError::AbiError(_) => EvmError::Decode(e.to_string()),
            _ => EvmError::Contract(e.to_string()),
        }
    }
}

impl From<AlloySignerError> for EvmError {
    fn from(e: AlloySignerError) -> Self {
        EvmError::Signer(e.to_string())
    }
}

impl From<serde_json::Error> for EvmError {
    fn from(e: serde_json::Error) -> Self {
        EvmError::InvalidResponse(e.to_string())
    }
}
