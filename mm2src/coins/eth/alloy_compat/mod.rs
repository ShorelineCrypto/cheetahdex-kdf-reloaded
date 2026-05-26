//! # Purpose
//!
//! Bridge layer between the legacy `rust-web3` types pervasive in the EVM
//! coin code and the modern `alloy` Ethereum stack. The module exists for
//! the duration of LP-17 and is intended to shrink to nothing once every
//! call-site speaks alloy directly.
//!
//! # Public exports
//!
//! - [`types`] — type aliases mapping legacy `web3::types::*` names onto
//!   `alloy_primitives` / `alloy_rpc_types_eth` so call-sites can keep
//!   using the names they already use (`H256`, `BlockNumber`, `CallRequest`).
//! - [`error`] — a single `EvmError` enum that consolidates the seven
//!   `From<web3::Error>` impls scattered across `eth_types.rs` into one
//!   centralised mapping driven by `alloy::transports::TransportError`,
//!   `alloy::contract::Error`, and `alloy::signers::Error`.
//!
//! # Invariants
//!
//! - **Wire compatibility:** the JSON shapes of the alloy types re-exported
//!   through [`types`] are byte-equal to the `web3::types::*` shapes for
//!   every field name we serialise (`hash`, `from`, `to`, `gas`,
//!   `gasPrice`, `maxFeePerGas`, `maxPriorityFeePerGas`, `value`,
//!   `data`/`input`, `nonce`, `chainId`, `accessList`, …). Call-sites
//!   relying on those wire names continue to work unchanged.
//! - **Error category preservation:** `EvmError` variants (`Transport`,
//!   `InvalidResponse`, `Decode`, `Contract`, `Signer`, `Internal`) map
//!   one-to-one onto the existing `Web3RpcError`/`WithdrawError`/
//!   `BalanceError`/`TradePreimageError` cases so downstream error
//!   classification (used by retry logic and HTTP status mapping) is
//!   unchanged.
//! - **No facades:** this module never re-exposes a `Web3<T>` namespace
//!   nor a `futures 0.1`-style `impl Future<Item, Error>`. Call-sites
//!   migrate to native `async fn` returns.
//! - **GPL-clean:** all code below is original to the kdf-reloaded
//!   project; the `alloy` crate family is dual-licensed MIT/Apache-2.0
//!   and therefore GPLv2-compatible (per LP-17 prompt §0).

pub mod error;
pub mod provider;
pub mod send_shim;
pub mod transport;
pub mod types;

pub use error::EvmError;
pub use provider::{build_provider, KdfProvider};
pub use send_shim::assert_send_future;
pub use transport::AlloyTransport;
