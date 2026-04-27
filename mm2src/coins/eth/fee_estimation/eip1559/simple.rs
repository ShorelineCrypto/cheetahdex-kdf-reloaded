use super::{EstimationSource, FeePerGasEstimated, FeePerGasLevel, PriorityLevelId, FEE_PRIORITY_LEVEL_N};
use crate::eth::web3_transport::FeeHistoryResult;
use crate::eth::{wei_from_gwei_decimal, wei_to_gwei_decimal, EthCoin, Web3RpcError, Web3RpcResult};
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::or_mm_error::OrMmError;
use mm2_err_handle::prelude::MapMmError;

use bigdecimal::BigDecimal;
use ethereum_types::U256;
use futures::compat::Future01CompatExt;
use num_traits::FromPrimitive;
use web3::types::BlockNumber;

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
        let fee_history_namespace: crate::eth::web3_transport::EthFeeHistoryNamespace<_> = coin.web3.api();
        let res = fee_history_namespace
            .eth_fee_history(
                U256::from(Self::history_depth()),
                BlockNumber::Latest,
                Self::history_percentiles(),
            )
            .compat()
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
        fee_history: &FeeHistoryResult,
    ) -> Web3RpcResult<FeePerGasLevel> {
        let level_index = level as usize;
        let level_rewards = fee_history
            .priority_rewards
            .as_ref()
            .or_mm_err(|| Web3RpcError::Internal("expected reward in eth_feeHistory".into()))?
            .iter()
            .map(|rewards| rewards.get(level_index).copied().unwrap_or_else(|| U256::from(0)))
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

    fn calculate_with_history(fee_history: &FeeHistoryResult) -> Web3RpcResult<FeePerGasEstimated> {
        let latest_base_fee = fee_history
            .base_fee_per_gas
            .first()
            .copied()
            .unwrap_or_else(|| U256::from(0));
        let latest_base_fee_gwei = wei_to_gwei_decimal(latest_base_fee).unwrap_or_else(|_| BigDecimal::from(0));

        let predicted_base_fee = Self::predict_base_fee(&fee_history.base_fee_per_gas);
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
