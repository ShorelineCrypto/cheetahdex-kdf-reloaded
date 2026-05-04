//! Re-export hub for the Tendermint coin module.
//!
//! This file provides any remaining items that don't belong in the
//! responsibility-specific modules (types, helpers, swap_ops,
//! market_ops, mm_coin) and re-exports them for external use.
//!
//! The actual public API is surfaced through `tendermint/mod.rs`:
//!   `pub use tendermint_coin::*;`
//!   `pub use tendermint_types::*;`
//!   `pub use tendermint_token::*;`

use super::tendermint_types::*;
use cosmrs::AccountId;
use mm2_err_handle::prelude::*;
use std::str::FromStr;

/// Initialise a list of HTTP RPC clients from configuration nodes.
///
/// Each `RpcNode` is validated and turned into an `HttpClient`.
/// Returns the full list of clients, or an error with the invalid URLs.
pub(crate) fn init_rpc_clients(nodes: &[RpcNode]) -> MmResult<Vec<super::rpc::HttpClient>, TendermintInitErrorKind> {
    use mm2_err_handle::prelude::*;

    let mut clients = Vec::new();
    let mut errors = Vec::new();

    for node in nodes {
        match super::rpc::HttpClient::new(node.url.as_str()) {
            Ok(client) => clients.push(client),
            Err(e) => errors.push(format!("Url {} is invalid: {}", node.url, e)),
        }
    }

    if !errors.is_empty() {
        let combined: String = errors.join(", ");
        return MmError::err(TendermintInitErrorKind::RpcClientInitError(combined));
    }

    Ok(clients)
}
