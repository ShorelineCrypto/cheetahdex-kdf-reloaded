//! # Purpose
//!
//! Construction helpers for the alloy [`Provider`] used by the EVM
//! coin family. Wraps the project-specific [`AlloyTransport`] in
//! alloy's standard `RpcClient` + `RootProvider` pipeline so call-sites
//! get the full set of EIP-1474 methods (`get_balance`,
//! `get_transaction_count`, `estimate_gas`, `call`, `get_logs`,
//! `send_raw_transaction`, `get_transaction_by_hash`,
//! `get_transaction_receipt`, `get_block_number`, `get_gas_price`,
//! `get_chain_id`, `client_version`, …) as native `async fn`s.
//!
//! # Public exports
//!
//! - [`KdfProvider`] — type alias for the concrete provider used
//!   throughout the EVM stack.
//! - [`build_provider`] — convenience constructor that takes the same
//!   arguments as the legacy `Web3Transport::with_event_handlers` and
//!   returns a ready-to-use [`KdfProvider`].
//!
//! # Invariants
//!
//! - **Mainnet network:** the provider is parameterised on
//!   `alloy::network::Ethereum`, matching the only network handled by
//!   web3 today (chain-specific differences are encoded in `chain_id`,
//!   not in the network type).
//! - **`is_local = false`:** matches the legacy assumption that every
//!   configured RPC URL is a remote node. Setting this affects only
//!   alloy-internal latency hints, not wire bytes.

use alloy::network::Ethereum;
use alloy::providers::RootProvider;
use alloy::rpc::client::RpcClient;

use crate::eth::alloy_compat::transport::AlloyTransport;
use crate::RpcTransportEventHandlerShared;

/// Concrete provider type used by every EVM coin in the project.
///
/// Always parameterised on the `Ethereum` network — chain selection is
/// handled at call time via `chain_id`, not via the type system.
pub type KdfProvider = RootProvider<Ethereum>;

/// Build a [`KdfProvider`] from a list of equivalent RPC URLs and a
/// list of metrics handlers. Mirrors
/// `Web3Transport::with_event_handlers` followed by `Web3::new`.
pub fn build_provider(
    urls: Vec<String>,
    event_handlers: Vec<RpcTransportEventHandlerShared>,
) -> Result<KdfProvider, String> {
    let transport = AlloyTransport::with_event_handlers(urls, event_handlers)?;
    let client = RpcClient::new(transport, false);
    Ok(RootProvider::new(client))
}
