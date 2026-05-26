//! # Purpose
//!
//! Type aliases mapping legacy `web3::types::*` names to their `alloy`
//! equivalents. Keeping the legacy names lets per-file migrations focus
//! on logic and futures-style changes rather than identifier churn.
//!
//! # Public exports
//!
//! Primitives — `Address`, `H256`, `U256`, `Bytes`.
//! RPC types — `Block`, `BlockId`, `BlockNumber`, `Log`, `Transaction`,
//! `TransactionReceipt`, `CallRequest`, `Filter`.
//!
//! # Invariants
//!
//! - JSON-serialised shapes match `web3::types::*` field-for-field for
//!   every wire-stable field (see `mod.rs` invariants).
//! - `H256` is `alloy::primitives::B256`; both are `[u8; 32]` newtypes
//!   with byte-identical hex serde reps.
//! - `BlockNumber` is `alloy::rpc::types::eth::BlockNumberOrTag`; the
//!   serde rep accepts both decimal numbers and tag strings (`"latest"`,
//!   `"pending"`, etc.) just like the web3 type did.
//! - `CallRequest` is `alloy::rpc::types::eth::TransactionRequest` —
//!   wider than the web3 `CallRequest` (it carries every transaction
//!   field, not just call inputs) but every web3 `CallRequest` field
//!   is a strict subset, so existing constructors compile and serialise
//!   identically.

pub use alloy::primitives::{Address, Bytes, B256 as H256, U256};
pub use alloy::rpc::types::eth::{
    Block, BlockId, BlockNumberOrTag as BlockNumber, Filter, Log, Transaction, TransactionReceipt,
    TransactionRequest as CallRequest,
};
