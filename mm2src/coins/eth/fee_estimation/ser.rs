//! # JSON-serialization shim for fee estimates
//!
//! The internal [`eip1559::FeePerGasEstimated`] uses wei-denominated `U256`
//! values so the engine can do exact arithmetic. RPC consumers prefer
//! human-readable gwei `BigDecimal` values, so this module exposes a parallel
//! [`FeePerGasEstimated`] (and its [`FeePerGasLevel`]) with the same shape but
//! gwei-denominated fields and a `units` discriminator so consumers can be
//! sure of the unit they're reading.
//!
//! The conversion is one-way (engine → JSON) via [`TryFrom`]; the gwei
//! representation is **not** parsed back into the engine.
//!
//! # Invariants
//! - JSON field names are part of the RPC contract; do not rename.
//! - Every numeric field is in **gwei** as a [`BigDecimal`].

use std::convert::TryFrom;

use bigdecimal::BigDecimal;
use serde::Serialize;

use mm2_err_handle::mm_error::MmError;

use crate::eth::{fee_estimation::eip1559, wei_to_gwei_decimal};
use crate::NumConversError;

/// Discriminator for the `units` JSON field.
///
/// Currently always `Gwei`; reserved as an enum so consumers can branch on
/// future units (e.g. wei) without a breaking schema change.
#[derive(Serialize)]
pub enum EstimationUnits {
    Gwei,
}

/// Per-tier fee shape exposed over the wire (gwei).
#[derive(Serialize)]
pub struct FeePerGasLevel {
    pub max_priority_fee_per_gas: BigDecimal,
    pub max_fee_per_gas: BigDecimal,
    pub min_wait_time: Option<u32>,
    pub max_wait_time: Option<u32>,
}

impl TryFrom<eip1559::FeePerGasLevel> for FeePerGasLevel {
    type Error = MmError<NumConversError>;

    fn try_from(level: eip1559::FeePerGasLevel) -> Result<Self, Self::Error> {
        Ok(Self {
            max_priority_fee_per_gas: wei_to_gwei_decimal(level.max_priority_fee_per_gas)?,
            max_fee_per_gas: wei_to_gwei_decimal(level.max_fee_per_gas)?,
            min_wait_time: level.min_wait_time,
            max_wait_time: level.max_wait_time,
        })
    }
}

/// JSON-shaped fee estimate, gwei-denominated.
///
/// Mirror of [`eip1559::FeePerGasEstimated`] for serialization to RPC
/// consumers. The `source` is the lowercase backend name, the trend strings
/// are passed through verbatim, and `units` is always `gwei`.
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
            low: fees.low.try_into()?,
            medium: fees.medium.try_into()?,
            high: fees.high.try_into()?,
            source: fees.source.to_string(),
            base_fee_trend: fees.base_fee_trend,
            priority_fee_trend: fees.priority_fee_trend,
            units: EstimationUnits::Gwei,
        })
    }
}
