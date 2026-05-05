use crate::eip_1193_provider::Eip1193Provider;
use crate::metamask_error::{MetamaskError, MetamaskResult};
use futures::lock::{Mutex as AsyncMutex, MutexGuard as AsyncMutexGuard};
use itertools::Itertools;
use lazy_static::lazy_static;
use mm2_err_handle::prelude::*;
use mm2_eth::typed_data::{Eip712, H256};
use serde::Serialize;
use serde_json::{json, Value as Json};
use web3::helpers::CallFuture;
use web3::Transport;

lazy_static! {
    /// Serialises MetaMask requests: only one in-flight at a time so the
    /// active chain ID cannot change mid-request.
    static ref SESSION_GUARD: AsyncMutex<()> = AsyncMutex::new(());
}

/// Tries to detect a browser-injected MetaMask (EIP-1193) provider.
pub fn detect_metamask_provider() -> MetamaskResult<Eip1193Provider> {
    Eip1193Provider::detect().or_mm_err(|| MetamaskError::EthProviderNotFound)
}

/// An exclusive session with the MetaMask extension.
///
/// Acquiring a session locks a global mutex so that chain-switching and
/// signing cannot interleave across concurrent tasks.
pub struct MetamaskSession<'a> {
    transport: &'a Eip1193Provider,
    _guard: AsyncMutexGuard<'a, ()>,
}

impl<'a> MetamaskSession<'a> {
    /// Acquires the global session lock.
    pub async fn lock(transport: &'a Eip1193Provider) -> Self {
        MetamaskSession {
            transport,
            _guard: SESSION_GUARD.lock().await,
        }
    }

    /// Requests the user's active ETH account via `eth_requestAccounts`.
    ///
    /// Expects exactly one account; returns an error otherwise.
    pub async fn eth_request_account(&self) -> MetamaskResult<String> {
        let accounts: Vec<String> = CallFuture::new(self.transport.execute("eth_requestAccounts", vec![])).await?;
        accounts
            .into_iter()
            .exactly_one()
            .map_to_mm(|_| MetamaskError::ExpectedOneEthAccount)
    }

    /// Asks MetaMask to switch to the given EVM chain.
    pub async fn wallet_switch_ethereum_chain(&self, chain_id: u64) -> Result<(), web3::Error> {
        let req = json!({
            "chainId": format!("0x{chain_id:x}"),
        });
        CallFuture::new(self.transport.execute("wallet_switchEthereumChain", vec![req])).await
    }

    /// Signs EIP-712 typed data via `eth_signTypedData_v4` and returns the
    /// message hash together with the hex-encoded signature.
    ///
    /// `user_address` must match the currently active MetaMask account.
    pub async fn sign_typed_data_v4<Domain, Message>(
        &self,
        user_address: String,
        request: Eip712<Domain, Message>,
    ) -> MetamaskResult<(H256, String)>
    where
        Domain: Serialize,
        Message: Serialize,
    {
        let addr_json = Json::String(user_address);
        let request_json =
            serde_json::to_string(&request).map_to_mm(|e| MetamaskError::ErrorSerializingArguments(e.to_string()))?;

        let hash = mm2_eth::typed_data::hash_typed_data(request)
            .map_err(|e| MetamaskError::Internal(format!("EIP-712 hashing error: {e}")))?;

        let signature: String = CallFuture::new(
            self.transport
                .execute("eth_signTypedData_v4", vec![addr_json, Json::String(request_json)]),
        )
        .await?;

        Ok((hash, signature))
    }
}
