use crate::eth::{fee_estimation::eip1559, wei_to_gwei_decimal};
use crate::NumConversError;
use mm2_err_handle::mm_error::MmError;

use bigdecimal::BigDecimal;
use serde::Serialize;
use std::convert::TryFrom;

#[derive(Serialize)]
pub enum EstimationUnits {
    Gwei,
}

#[derive(Serialize)]
pub struct FeePerGasLevel {
    pub max_priority_fee_per_gas: BigDecimal,
    pub max_fee_per_gas: BigDecimal,
    pub min_wait_time: Option<u32>,
    pub max_wait_time: Option<u32>,
}

/// Serializable fee estimation response with values in gwei
#[derive(Serialize)]
pub struct FeePerGasEstimated {
    pub base_fee: BigDecimal,
    pub low: FeePerGasLevel,
    pub medium: FeePerGasLevel,
    pub high: FeePerGasLevel,
    pub source: String,
    pub base_fee_trend: String,
    pub priority_fee_trend: String,
    pub units: EstimationUnits,
}

impl TryFrom<eip1559::FeePerGasEstimated> for FeePerGasEstimated {
    type Error = MmError<NumConversError>;

    fn try_from(fees: eip1559::FeePerGasEstimated) -> Result<Self, Self::Error> {
        Ok(Self {
            base_fee: wei_to_gwei_decimal(fees.base_fee)?,
            low: FeePerGasLevel {
                max_fee_per_gas: wei_to_gwei_decimal(fees.low.max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_to_gwei_decimal(fees.low.max_priority_fee_per_gas)?,
                min_wait_time: fees.low.min_wait_time,
                max_wait_time: fees.low.max_wait_time,
            },
            medium: FeePerGasLevel {
                max_fee_per_gas: wei_to_gwei_decimal(fees.medium.max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_to_gwei_decimal(fees.medium.max_priority_fee_per_gas)?,
                min_wait_time: fees.medium.min_wait_time,
                max_wait_time: fees.medium.max_wait_time,
            },
            high: FeePerGasLevel {
                max_fee_per_gas: wei_to_gwei_decimal(fees.high.max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_to_gwei_decimal(fees.high.max_priority_fee_per_gas)?,
                min_wait_time: fees.high.min_wait_time,
                max_wait_time: fees.high.max_wait_time,
            },
            source: fees.source.to_string(),
            base_fee_trend: fees.base_fee_trend,
            priority_fee_trend: fees.priority_fee_trend,
            units: EstimationUnits::Gwei,
        })
    }
}
