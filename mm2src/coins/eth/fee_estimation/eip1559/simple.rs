use super::{EstimationSource, FeePerGasEstimated, FeePerGasLevel, PriorityLevelId, FEE_PRIORITY_LEVEL_N};
use crate::eth::{wei_from_gwei_decimal, wei_to_gwei_decimal, EthCoin, Web3RpcError, Web3RpcResult};
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::or_mm_error::OrMmError;
use mm2_err_handle::prelude::MapMmError;

// LP-17: alloy replaces the custom `EthFeeHistoryNamespace` over
// `web3::Web3`. Wire-level `eth_feeHistory` parameters and the
// returned JSON shape are identical; only the in-memory numeric
// type differs (alloy uses `u128` for wei amounts where the legacy
// `FeeHistoryResult` used `U256`).
use crate::eth::alloy_compat::assert_send_future;
use alloy::providers::Provider;
use alloy::rpc::types::eth::{BlockNumberOrTag, FeeHistory};
use bigdecimal::BigDecimal;
use ethereum_types::U256;
use num_traits::FromPrimitive;

/// `ethereum_types::U256` (LP-13 legacy) does not implement
/// `From<u128>`; alloy emits `u128` for fee values. Splits the
/// 128-bit value into two 64-bit limbs to bridge cleanly without
/// pulling in an extra conversion crate.
#[inline]
fn u128_to_u256(v: u128) -> U256 {
    let lo = v as u64;
    let hi = (v >> 64) as u64;
    let mut bytes = [0u8; 32];
    bytes[16..24].copy_from_slice(&hi.to_be_bytes());
    bytes[24..32].copy_from_slice(&lo.to_be_bytes());
    U256::from_big_endian(&bytes)
}

/// Simple priority fee per gas estimator based on fee history.
/// Used as fallback when no external gas api provider is available.
pub(crate) struct FeePerGasSimpleEstimator;

impl FeePerGasSimpleEstimator {
    const FEE_PRIORITY_DEPTH: u64 = 5;
    const HISTORY_PERCENTILES: [f64; FEE_PRIORITY_LEVEL_N] = [25.0, 50.0, 75.0];
    const BASE_FEE_PERCENTILE: f64 = 75.0;
    const PRIORITY_FEE_PERCENTILES: [f64; FEE_PRIORITY_LEVEL_N] = [50.0, 50.0, 50.0];
    const ADJUST_BASE_FEE: [f64; FEE_PRIORITY_LEVEL_N] = [1.1, 1.175, 1.25];
    const ADJUST_PRIORITY_FEE: [f64; FEE_PRIORITY_LEVEL_N] = [1.0, 1.0, 1.0];

    pub fn history_depth() -> u64 {
        Self::FEE_PRIORITY_DEPTH
    }

    pub fn history_percentiles() -> &'static [f64] {
        &Self::HISTORY_PERCENTILES
    }

    fn percentile_of(v: &[U256], percent: f64) -> U256 {
        let mut v_mut = v.to_owned();
        v_mut.sort();

        let percent = percent.clamp(0.0, 100.0);
        let value_pos = ((v_mut.len() - 1) as f64 * percent / 100.0).round() as usize;
        v_mut[value_pos]
    }

    /// Estimate gas priority fees using eth_feeHistory
    pub async fn estimate_fee_by_history(coin: &EthCoin) -> Web3RpcResult<FeePerGasEstimated> {
        let provider = coin.alloy_provider();
        let res = assert_send_future(provider.get_fee_history(
            Self::history_depth(),
            BlockNumberOrTag::Latest,
            Self::history_percentiles(),
        ))
        .await;

        match res {
            Ok(fee_history) => Ok(Self::calculate_with_history(&fee_history)?),
            Err(_) => MmError::err(Web3RpcError::Internal("eth_feeHistory request failed".into())),
        }
    }

    fn predict_base_fee(base_fees: &[U256]) -> U256 {
        Self::percentile_of(base_fees, Self::BASE_FEE_PERCENTILE)
    }

    fn priority_fee_for_level(
        level: PriorityLevelId,
        base_fee_gwei: BigDecimal,
        fee_history: &FeeHistory,
    ) -> Web3RpcResult<FeePerGasLevel> {
        let level_index = level as usize;
        let level_rewards = fee_history
            .reward
            .as_ref()
            .or_mm_err(|| Web3RpcError::Internal("expected reward in eth_feeHistory".into()))?
            .iter()
            .map(|rewards| u128_to_u256(rewards.get(level_index).copied().unwrap_or(0u128)))
            .collect::<Vec<_>>();

        let max_priority_fee_per_gas = Self::percentile_of(&level_rewards, Self::PRIORITY_FEE_PERCENTILES[level_index]);
        let max_priority_fee_per_gas_gwei =
            wei_to_gwei_decimal(max_priority_fee_per_gas).unwrap_or_else(|_| BigDecimal::from(0));

        let base_fee_mult =
            BigDecimal::from_f64(Self::ADJUST_BASE_FEE[level_index]).unwrap_or_else(|| BigDecimal::from(0));
        let priority_fee_mult =
            BigDecimal::from_f64(Self::ADJUST_PRIORITY_FEE[level_index]).unwrap_or_else(|| BigDecimal::from(0));

        let max_fee_per_gas_dec = base_fee_gwei * base_fee_mult + max_priority_fee_per_gas_gwei * priority_fee_mult;

        Ok(FeePerGasLevel {
            max_priority_fee_per_gas,
            max_fee_per_gas: wei_from_gwei_decimal(&max_fee_per_gas_dec)
                .mm_err(|e| Web3RpcError::Internal(e.to_string()))?,
            min_wait_time: None,
            max_wait_time: None,
        })
    }

    fn calculate_with_history(fee_history: &FeeHistory) -> Web3RpcResult<FeePerGasEstimated> {
        // Convert alloy's `u128` base-fee samples into the legacy
        // `U256` shape that the percentile / multiplier helpers expect.
        let base_fees: Vec<U256> = fee_history.base_fee_per_gas.iter().copied().map(u128_to_u256).collect();
        let latest_base_fee = base_fees.first().copied().unwrap_or_else(|| U256::from(0));
        let latest_base_fee_gwei = wei_to_gwei_decimal(latest_base_fee).unwrap_or_else(|_| BigDecimal::from(0));

        let predicted_base_fee = Self::predict_base_fee(&base_fees);
        Ok(FeePerGasEstimated {
            base_fee: predicted_base_fee,
            low: Self::priority_fee_for_level(PriorityLevelId::Low, latest_base_fee_gwei.clone(), fee_history)?,
            medium: Self::priority_fee_for_level(PriorityLevelId::Medium, latest_base_fee_gwei.clone(), fee_history)?,
            high: Self::priority_fee_for_level(PriorityLevelId::High, latest_base_fee_gwei, fee_history)?,
            source: EstimationSource::Simple,
            base_fee_trend: String::default(),
            priority_fee_trend: String::default(),
        })
    }
}
