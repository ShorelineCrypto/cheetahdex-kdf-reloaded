use super::{EstimationSource, FeePerGasEstimated, FeePerGasLevel};
use crate::eth::{wei_from_gwei_decimal, Web3RpcError, Web3RpcResult};
use crate::NumConversError;
use bigdecimal::BigDecimal;
use http::StatusCode;
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::prelude::*;
use mm2_net::transport::slurp_url_with_headers;
use serde::Deserialize;
use std::convert::TryFrom;
use std::convert::TryInto;

lazy_static! {
    static ref INFURA_GAS_API_AUTH_TEST: String = std::env::var("INFURA_GAS_API_AUTH_TEST").unwrap_or_default();
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct InfuraFeePerGasLevel {
    #[serde(rename = "suggestedMaxPriorityFeePerGas")]
    pub suggested_max_priority_fee_per_gas: BigDecimal,
    #[serde(rename = "suggestedMaxFeePerGas")]
    pub suggested_max_fee_per_gas: BigDecimal,
    #[serde(rename = "minWaitTimeEstimate")]
    pub min_wait_time_estimate: u32,
    #[serde(rename = "maxWaitTimeEstimate")]
    pub max_wait_time_estimate: u32,
}

#[derive(Debug, Deserialize)]
pub(crate) struct InfuraFeePerGas {
    pub low: InfuraFeePerGasLevel,
    pub medium: InfuraFeePerGasLevel,
    pub high: InfuraFeePerGasLevel,
    #[serde(rename = "estimatedBaseFee")]
    pub estimated_base_fee: BigDecimal,
    #[serde(rename = "priorityFeeTrend")]
    pub priority_fee_trend: String,
    #[serde(rename = "baseFeeTrend")]
    pub base_fee_trend: String,
}

impl TryFrom<InfuraFeePerGas> for FeePerGasEstimated {
    type Error = MmError<NumConversError>;

    fn try_from(infura_fees: InfuraFeePerGas) -> Result<Self, Self::Error> {
        Ok(Self {
            base_fee: wei_from_gwei_decimal(&infura_fees.estimated_base_fee)?,
            low: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(&infura_fees.low.suggested_max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(&infura_fees.low.suggested_max_priority_fee_per_gas)?,
                min_wait_time: Some(infura_fees.low.min_wait_time_estimate),
                max_wait_time: Some(infura_fees.low.max_wait_time_estimate),
            },
            medium: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(&infura_fees.medium.suggested_max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(
                    &infura_fees.medium.suggested_max_priority_fee_per_gas,
                )?,
                min_wait_time: Some(infura_fees.medium.min_wait_time_estimate),
                max_wait_time: Some(infura_fees.medium.max_wait_time_estimate),
            },
            high: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(&infura_fees.high.suggested_max_fee_per_gas)?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(&infura_fees.high.suggested_max_priority_fee_per_gas)?,
                min_wait_time: Some(infura_fees.high.min_wait_time_estimate),
                max_wait_time: Some(infura_fees.high.max_wait_time_estimate),
            },
            source: EstimationSource::Infura,
            base_fee_trend: infura_fees.base_fee_trend,
            priority_fee_trend: infura_fees.priority_fee_trend,
        })
    }
}

pub(crate) struct InfuraGasApiCaller;

impl InfuraGasApiCaller {
    const INFURA_GAS_FEES_ENDPOINT: &'static str = "networks/1/suggestedGasFees";

    fn get_url_and_headers(base_url: &str) -> (String, Vec<(&'static str, &'static str)>) {
        let url = format!("{}/{}", base_url.trim_end_matches('/'), Self::INFURA_GAS_FEES_ENDPOINT);
        let headers = vec![("Authorization", INFURA_GAS_API_AUTH_TEST.as_str())];
        (url, headers)
    }

    async fn make_request(
        url: &str,
        headers: Vec<(&'static str, &'static str)>,
    ) -> Result<InfuraFeePerGas, MmError<String>> {
        let resp = slurp_url_with_headers(url, headers).await.mm_err(|e| e.to_string())?;
        if resp.0 != StatusCode::OK {
            return MmError::err(format!("{} failed with status code {}", url, resp.0));
        }
        serde_json::from_slice(&resp.2).map_to_mm(|e| e.to_string())
    }

    pub async fn fetch_fee_estimation(base_url: &str) -> Web3RpcResult<FeePerGasEstimated> {
        let (url, headers) = Self::get_url_and_headers(base_url);
        let infura_fees = Self::make_request(&url, headers)
            .await
            .mm_err(Web3RpcError::Transport)?;
        infura_fees
            .try_into()
            .mm_err(|e: NumConversError| Web3RpcError::Internal(e.to_string()))
    }
}
