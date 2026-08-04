use crate::context::CoinsActivationContext;
use crate::prelude::*;
use crate::standalone_coin::{InitStandaloneCoinActivationOps, InitStandaloneCoinError,
                             InitStandaloneCoinInitialStatus, InitStandaloneCoinTaskHandle,
                             InitStandaloneCoinTaskManagerShared};
use async_trait::async_trait;
use coins::coin_balance::{EnableCoinBalance, IguanaWalletBalance};
use coins::utxo::rpc_clients::ElectrumRpcRequest;
use coins::utxo::{UtxoActivationParams, UtxoRpcMode};
use coins::z_coin::{z_coin_from_conf_and_params, ZCoin, ZCoinBuildError, ZcoinProtocolInfo};
use coins::{BalanceError, CoinProtocol, MarketCoinOps, PrivKeyActivationPolicy, RegisterCoinError};
use common::executor::Timer;
use crypto::hw_rpc_task::{HwRpcTaskAwaitingStatus, HwRpcTaskUserAction};
use crypto::{CryptoCtx, CryptoCtxError, CryptoInitError};
use derive_more::Display;
use futures::compat::Future01CompatExt;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use rpc_task::RpcTaskError;
use ser_error_derive::SerializeErrorType;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::future::Future;
use std::time::Duration;

pub type ZcoinTaskManagerShared = InitStandaloneCoinTaskManagerShared<ZCoin>;
pub type ZcoinRpcTaskHandle = InitStandaloneCoinTaskHandle<ZCoin>;
pub type ZcoinAwaitingStatus = HwRpcTaskAwaitingStatus;
pub type ZcoinUserAction = HwRpcTaskUserAction;

#[derive(Clone, Serialize)]
pub struct ZcoinActivationResult {
    pub current_block: u64,
    pub wallet_balance: EnableCoinBalance,
    /// The resolved shielded sync start block, exposed only when the caller
    /// supplied a `sync_start` (R39.8.0h). Absent when no start was requested.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_sync_block: Option<FirstSyncBlock>,
}

/// Details of the block from which the initial shielded sync was anchored,
/// reported back to the caller when a `sync_start` was supplied (R39.8.0h).
#[derive(Clone, Serialize)]
pub struct FirstSyncBlock {
    /// The start height the caller requested (a height directly, or the block
    /// resolved from a requested calendar date).
    pub requested: u64,
    /// Whether `requested` is below this coin's Sapling activation height.
    pub is_pre_sapling: bool,
    /// The height actually used to anchor the sync: `requested`, floored at the
    /// Sapling activation height.
    pub actual: u64,
}

impl CurrentBlock for ZcoinActivationResult {
    fn current_block(&self) -> u64 { self.current_block }
}

#[derive(Clone, Serialize)]
#[non_exhaustive]
pub enum ZcoinInProgressStatus {
    ActivatingCoin,
    Scanning,
    RequestingWalletBalance,
    Finishing,
    /// This status doesn't require the user to send `UserAction`,
    /// but it tells the user that he should confirm/decline an address on his device.
    WaitingForTrezorToConnect,
    WaitingForUserToConfirmPubkey,
}

impl InitStandaloneCoinInitialStatus for ZcoinInProgressStatus {
    fn initial_status() -> Self { ZcoinInProgressStatus::ActivatingCoin }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "rpc", content = "rpc_data")]
pub enum ZcoinRpcMode {
    Native,
    Light {
        electrum_servers: Vec<ElectrumRpcRequest>,
        light_wallet_d_servers: Vec<String>,
        /// Optional shielded sync starting point (R39.6.2). Externally dictated
        /// wire location: nested inside the Light-mode `rpc_data` alongside the
        /// server lists, exactly as sent by KDF-family wallets.
        #[serde(default)]
        sync_params: Option<ZcoinSyncParams>,
    },
}

/// Shielded sync starting point (R39.6.2). This is the externally *dictated*
/// wire shape used by KDF-family wallets for `mode.rpc_data.sync_params`:
///
/// - `{"height": <block-height>}` — start from an explicit block height;
/// - `{"date": <unix-timestamp>}` — start from the block matching a date;
/// - `"earliest"` — start from Sapling activation.
///
/// Externally tagged with lowercase variant names so the serde representation is
/// exactly the dictated JSON.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ZcoinSyncParams {
    Date(u64),
    Height(u64),
    Earliest,
}

#[derive(Deserialize)]
pub struct ZcoinActivationParams {
    pub mode: ZcoinRpcMode,
    pub required_confirmations: Option<u64>,
    pub requires_notarization: Option<bool>,
    /// Sync-throughput tuning: blocks processed per iteration (R39.6.2). The
    /// dictated wire name is `scan_blocks_per_iteration`; `blocks_per_iteration`
    /// is accepted as an alias. Defaults to 1.
    #[serde(default, alias = "scan_blocks_per_iteration")]
    pub blocks_per_iteration: Option<u32>,
    /// Sync pacing interval (R39.6.2). The public-API dictated wire name is
    /// `scan_interval_ms`; the desktop wallet sends `scan_interval`. Both are
    /// accepted as aliases. Defaults to 0.
    #[serde(default, alias = "scan_interval_ms", alias = "scan_interval")]
    pub inter_iteration_interval_ms: Option<u64>,
    /// HD account index for the shielded key derivation (R39.6.4 §2). Used only
    /// under the HD (BIP39) key policy, where the shielded spending key is
    /// derived at `m/<z_derivation_path>/account'`. Ignored under the legacy
    /// Iguana policy. Defaults to `0`.
    #[serde(default)]
    pub account: Option<u32>,
}

impl ZcoinActivationParams {
    /// The shielded sync starting point, extracted from the Light-mode
    /// `rpc_data` where the dictated wire places it. `None` in Native mode or
    /// when no sync point was supplied.
    fn sync_params(&self) -> Option<&ZcoinSyncParams> {
        match &self.mode {
            ZcoinRpcMode::Light { sync_params, .. } => sync_params.as_ref(),
            ZcoinRpcMode::Native => None,
        }
    }
}

impl TxHistory for ZcoinActivationParams {
    fn tx_history(&self) -> bool { false }
}

async fn resolve_requested_shielded_scan_start_height<F, Fut>(
    target_timestamp: u64,
    tip_height: u64,
    mut get_block_timestamp: F,
) -> Result<u64, String>
where
    F: FnMut(u64) -> Fut,
    Fut: Future<Output = Result<u64, String>>,
{
    let mut low = 0u64;
    let mut high = tip_height;
    let mut resolved_height = 0u64;

    while low <= high {
        let mid = low + (high - low) / 2;
        let block_timestamp = get_block_timestamp(mid).await?;
        if block_timestamp <= target_timestamp {
            resolved_height = mid;
            low = mid + 1;
        } else {
            high = mid.saturating_sub(1);
        }
    }

    Ok(resolved_height)
}

async fn requested_shielded_scan_start_height(
    coin: &ZCoin,
    sync_params: Option<&ZcoinSyncParams>,
) -> Result<Option<u64>, ZcoinInitError> {
    match sync_params {
        None => Ok(None),
        Some(ZcoinSyncParams::Height(height)) => Ok(Some(*height)),
        Some(ZcoinSyncParams::Earliest) => Ok(Some(coin.sapling_activation_height())),
        Some(ZcoinSyncParams::Date(target_timestamp)) => {
            let tip_height = coin
                .current_block()
                .compat()
                .await
                .map_err(ZcoinInitError::CouldNotGetBlockCount)?;
            let resolved_height =
                resolve_requested_shielded_scan_start_height(*target_timestamp, tip_height, |height| async move {
                    coin.rpc_client()
                        .get_block_timestamp(height)
                        .await
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| ZcoinInitError::CouldNotResolveSyncStartDate {
                    sync_start: target_timestamp.to_string(),
                    error,
                })?;
            Ok(Some(resolved_height))
        },
    }
}

/// Builds the [`FirstSyncBlock`] report from the resolved requested start height
/// and the coin's Sapling activation height (R39.8.0h). Returns `None` when no
/// start was requested. `actual` floors `requested` at Sapling activation, the
/// lowest height at which shielded outputs can exist.
fn first_sync_block_from_requested(requested: Option<u64>, sapling_activation_height: u64) -> Option<FirstSyncBlock> {
    requested.map(|requested| FirstSyncBlock {
        requested,
        is_pre_sapling: requested < sapling_activation_height,
        actual: requested.max(sapling_activation_height),
    })
}

#[derive(Clone, Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum ZcoinInitError {
    #[display(fmt = "Error on coin {} creation: {}", ticker, error)]
    CoinCreationError {
        ticker: String,
        error: String,
    },
    CoinIsAlreadyActivated {
        ticker: String,
    },
    HardwareWalletsAreNotSupportedYet,
    #[display(fmt = "Initialization task has timed out {:?}", duration)]
    TaskTimedOut {
        duration: Duration,
    },
    CouldNotGetBalance(String),
    CouldNotGetBlockCount(String),
    #[display(fmt = "Could not resolve sync start date {}: {}", sync_start, error)]
    CouldNotResolveSyncStartDate {
        sync_start: String,
        error: String,
    },
    #[display(
        fmt = "Shielded wallet DB scanner did not complete for {} through activation tip {}: {}",
        ticker,
        activation_tip,
        error
    )]
    ShieldedWalletDbScanIncomplete {
        ticker: String,
        activation_tip: u64,
        error: String,
    },
    Internal(String),
}

impl ZcoinInitError {
    pub fn from_build_err(build_err: ZCoinBuildError, ticker: String) -> Self {
        ZcoinInitError::CoinCreationError {
            ticker,
            error: build_err.to_string(),
        }
    }
}

impl From<BalanceError> for ZcoinInitError {
    fn from(err: BalanceError) -> Self { ZcoinInitError::CouldNotGetBalance(err.to_string()) }
}

impl From<RegisterCoinError> for ZcoinInitError {
    fn from(reg_err: RegisterCoinError) -> ZcoinInitError {
        match reg_err {
            RegisterCoinError::CoinIsInitializedAlready { coin } => {
                ZcoinInitError::CoinIsAlreadyActivated { ticker: coin }
            },
            RegisterCoinError::Internal(internal) => ZcoinInitError::Internal(internal),
        }
    }
}

impl From<RpcTaskError> for ZcoinInitError {
    fn from(rpc_err: RpcTaskError) -> Self {
        match rpc_err {
            RpcTaskError::Timeout(duration) => ZcoinInitError::TaskTimedOut { duration },
            internal_error => ZcoinInitError::Internal(internal_error.to_string()),
        }
    }
}

impl From<CryptoInitError> for ZcoinInitError {
    fn from(err: CryptoInitError) -> Self { ZcoinInitError::Internal(err.to_string()) }
}

impl From<CryptoCtxError> for ZcoinInitError {
    fn from(err: CryptoCtxError) -> Self { ZcoinInitError::Internal(err.to_string()) }
}

impl From<ZcoinInitError> for InitStandaloneCoinError {
    fn from(err: ZcoinInitError) -> Self {
        match err {
            ZcoinInitError::CoinCreationError { ticker, error } => {
                InitStandaloneCoinError::CoinCreationError { ticker, error }
            },
            ZcoinInitError::CoinIsAlreadyActivated { ticker } => {
                InitStandaloneCoinError::CoinIsAlreadyActivated { ticker }
            },
            ZcoinInitError::HardwareWalletsAreNotSupportedYet => {
                InitStandaloneCoinError::PrivKeyNotAllowed("Hardware wallets are not supported yet".into())
            },
            ZcoinInitError::TaskTimedOut { duration } => InitStandaloneCoinError::TaskTimedOut { duration },
            ZcoinInitError::CouldNotGetBalance(e)
            | ZcoinInitError::CouldNotGetBlockCount(e)
            | ZcoinInitError::CouldNotResolveSyncStartDate { error: e, .. }
            | ZcoinInitError::ShieldedWalletDbScanIncomplete { error: e, .. }
            | ZcoinInitError::Internal(e) => InitStandaloneCoinError::Internal(e),
        }
    }
}

impl TryFromCoinProtocol for ZcoinProtocolInfo {
    fn try_from_coin_protocol(proto: CoinProtocol) -> Result<Self, MmError<CoinProtocol>>
    where
        Self: Sized,
    {
        match proto {
            CoinProtocol::ZHTLC(info) => Ok(info),
            protocol => MmError::err(protocol),
        }
    }
}

#[async_trait]
impl InitStandaloneCoinActivationOps for ZCoin {
    type ActivationRequest = ZcoinActivationParams;
    type StandaloneProtocol = ZcoinProtocolInfo;
    type ActivationResult = ZcoinActivationResult;
    type ActivationError = ZcoinInitError;
    type InProgressStatus = ZcoinInProgressStatus;
    type AwaitingStatus = ZcoinAwaitingStatus;
    type UserAction = ZcoinUserAction;

    fn rpc_task_manager(activation_ctx: &CoinsActivationContext) -> &ZcoinTaskManagerShared {
        &activation_ctx.init_z_coin_task_manager
    }

    async fn init_standalone_coin(
        ctx: MmArc,
        ticker: String,
        coin_conf: Json,
        activation_request: &ZcoinActivationParams,
        mut protocol_info: ZcoinProtocolInfo,
        task_handle: &ZcoinRpcTaskHandle,
    ) -> MmResult<Self, ZcoinInitError> {
        let utxo_mode = match &activation_request.mode {
            ZcoinRpcMode::Native => UtxoRpcMode::Native,
            ZcoinRpcMode::Light { electrum_servers, .. } => UtxoRpcMode::Electrum {
                servers: electrum_servers.clone(),
                min_connected: None,
                max_connected: None,
            },
        };
        let utxo_params = UtxoActivationParams {
            mode: utxo_mode,
            utxo_merge_params: None,
            tx_history: false,
            required_confirmations: activation_request.required_confirmations,
            requires_notarization: activation_request.requires_notarization,
            address_format: None,
            gap_limit: None,
            min_addresses_number: None,
            scan_policy: Default::default(),
            priv_key_policy: PrivKeyActivationPolicy::IguanaPrivKey,
            check_utxo_maturity: None,
        };

        // Wire sync parameters from RPC request into protocol_info (R39.6.2)
        if let Some(blocks_per_iter) = activation_request.blocks_per_iteration {
            protocol_info.blocks_per_iteration = blocks_per_iter.max(1);
        }
        if let Some(inter_iter_ms) = activation_request.inter_iteration_interval_ms {
            protocol_info.inter_iteration_interval_ms = inter_iter_ms;
        }
        let crypto_ctx = CryptoCtx::from_ctx(&ctx).mm_err(Into::into)?;
        let priv_key = crypto_ctx.mm2_internal_privkey_secret();
        let coin = z_coin_from_conf_and_params(
            &ctx,
            &ticker,
            &coin_conf,
            &utxo_params,
            priv_key.as_slice(),
            activation_request.account.unwrap_or(0),
            protocol_info,
        )
        .await
        .mm_err(|e| ZcoinInitError::from_build_err(e, ticker.clone()))?;

        let requested_start_height =
            requested_shielded_scan_start_height(&coin, activation_request.sync_params()).await?;

        task_handle
            .update_in_progress_status(ZcoinInProgressStatus::Scanning)
            .mm_err(Into::into)?;
        while !coin.is_sapling_state_synced() {
            Timer::sleep(1.).await;
        }
        let activation_tip = coin
            .current_block()
            .compat()
            .await
            .map_to_mm(ZcoinInitError::CouldNotGetBlockCount)?;
        if let ZcoinRpcMode::Light {
            light_wallet_d_servers, ..
        } = &activation_request.mode
        {
            coin.fetch_lightwalletd_compact_blocks_to_height(
                light_wallet_d_servers,
                activation_tip,
                requested_start_height,
            )
            .await
            .map_err(|error| ZcoinInitError::ShieldedWalletDbScanIncomplete {
                ticker: ticker.clone(),
                activation_tip,
                error,
            })?;
        }
        coin.scan_shielded_wallet_db_to_height(activation_tip)
            .map_err(|error| ZcoinInitError::ShieldedWalletDbScanIncomplete {
                ticker: ticker.clone(),
                activation_tip,
                error,
            })?;
        Ok(coin)
    }

    async fn get_activation_result(
        &self,
        _ctx: MmArc,
        task_handle: &ZcoinRpcTaskHandle,
        activation_request: &Self::ActivationRequest,
    ) -> MmResult<Self::ActivationResult, ZcoinInitError> {
        task_handle
            .update_in_progress_status(ZcoinInProgressStatus::RequestingWalletBalance)
            .mm_err(Into::into)?;
        let current_block = self
            .current_block()
            .compat()
            .await
            .map_to_mm(ZcoinInitError::CouldNotGetBlockCount)?;

        // Expose the resolved shielded sync start point when the caller supplied
        // one (R39.8.0h). Height requests resolve without RPC; date requests are
        // resolved deterministically against the same backend used during scan.
        let requested_start_height =
            requested_shielded_scan_start_height(self, activation_request.sync_params()).await?;
        let first_sync_block =
            first_sync_block_from_requested(requested_start_height, self.sapling_activation_height());

        let balance = self.my_balance().compat().await.mm_err(Into::into)?;
        Ok(ZcoinActivationResult {
            current_block,
            wallet_balance: EnableCoinBalance::Iguana(IguanaWalletBalance {
                address: self.my_z_address_encoded(),
                balance,
            }),
            first_sync_block,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn zcoin_sync_params_match_the_dictated_wire_shapes() {
        // `{"height": N}` / `{"date": N}` / `"earliest"`, exactly as sent by
        // KDF-family wallets in `mode.rpc_data.sync_params`.
        assert_eq!(
            serde_json::from_value::<ZcoinSyncParams>(json!({ "height": 3_785_000 })).unwrap(),
            ZcoinSyncParams::Height(3_785_000)
        );
        assert_eq!(
            serde_json::from_value::<ZcoinSyncParams>(json!({ "date": 1_775_037_600u64 })).unwrap(),
            ZcoinSyncParams::Date(1_775_037_600)
        );
        assert_eq!(
            serde_json::from_value::<ZcoinSyncParams>(json!("earliest")).unwrap(),
            ZcoinSyncParams::Earliest
        );
    }

    #[test]
    fn activation_params_parse_the_desktop_wallet_wire_format() {
        // Mirrors the exact request the desktop wallet emits: sync_params nested
        // in the Light rpc_data, plus scan_blocks_per_iteration / scan_interval
        // at the activation-params level.
        let params: ZcoinActivationParams = serde_json::from_value(json!({
            "mode": {
                "rpc": "Light",
                "rpc_data": {
                    "electrum_servers": [{ "url": "electrum1.example:10008" }],
                    "light_wallet_d_servers": ["https://lightd1.example:443"],
                    "sync_params": { "height": 3_785_000 }
                }
            },
            "scan_blocks_per_iteration": 5000,
            "scan_interval": 0
        }))
        .unwrap();

        assert_eq!(params.blocks_per_iteration, Some(5000));
        assert_eq!(params.inter_iteration_interval_ms, Some(0));
        assert_eq!(params.sync_params(), Some(&ZcoinSyncParams::Height(3_785_000)));
    }

    #[test]
    fn activation_params_without_sync_params_yield_none() {
        let params: ZcoinActivationParams = serde_json::from_value(json!({
            "mode": {
                "rpc": "Light",
                "rpc_data": {
                    "electrum_servers": [{ "url": "electrum1.example:10008" }],
                    "light_wallet_d_servers": ["https://lightd1.example:443"]
                }
            }
        }))
        .unwrap();

        assert_eq!(params.sync_params(), None);
        assert_eq!(params.blocks_per_iteration, None);
    }

    #[test]
    fn resolve_requested_shielded_scan_start_height_finds_the_last_block_before_the_target_timestamp() {
        let resolved = futures::executor::block_on(resolve_requested_shielded_scan_start_height(
            350,
            3,
            |height| async move {
                let timestamps = [100u64, 200u64, 300u64, 400u64];
                Ok::<u64, String>(timestamps[height as usize])
            },
        ))
        .unwrap();

        assert_eq!(resolved, 2);
    }

    #[test]
    fn first_sync_block_is_none_without_a_requested_start() {
        assert!(first_sync_block_from_requested(None, 1_000).is_none());
    }

    #[test]
    fn first_sync_block_reports_requested_at_or_above_sapling() {
        let block = first_sync_block_from_requested(Some(5_000), 1_000).unwrap();
        assert_eq!(block.requested, 5_000);
        assert!(!block.is_pre_sapling);
        assert_eq!(block.actual, 5_000);
    }

    #[test]
    fn first_sync_block_floors_pre_sapling_request_at_activation() {
        let block = first_sync_block_from_requested(Some(500), 1_000).unwrap();
        assert_eq!(block.requested, 500);
        assert!(block.is_pre_sapling);
        assert_eq!(block.actual, 1_000);
    }
}
