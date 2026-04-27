use super::{EstimationSource, FeePerGasEstimated, FeePerGasLevel, FEE_PRIORITY_LEVEL_N};
use crate::eth::{wei_from_gwei_decimal, Web3RpcError, Web3RpcResult};
use crate::NumConversError;
use mm2_err_handle::mm_error::MmError;
use mm2_err_handle::prelude::*;
use mm2_net::transport::slurp_url_with_headers;

use bigdecimal::BigDecimal;
use http::StatusCode;
use serde::Deserialize;
use std::convert::TryFrom;
use std::convert::TryInto;

lazy_static! {
    static ref BLOCKNATIVE_GAS_API_AUTH_TEST: String =
        std::env::var("BLOCKNATIVE_GAS_API_AUTH_TEST").unwrap_or_default();
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BlocknativeBlockPrices {
    #[serde(rename = "baseFeePerGas")]
    pub base_fee_per_gas: BigDecimal,
    #[serde(rename = "estimatedPrices")]
    pub estimated_prices: Vec<BlocknativeEstimatedPrices>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct BlocknativeEstimatedPrices {
    #[serde(rename = "maxPriorityFeePerGas")]
    pub max_priority_fee_per_gas: BigDecimal,
    #[serde(rename = "maxFeePerGas")]
    pub max_fee_per_gas: BigDecimal,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BlocknativeBlockPricesResponse {
    #[serde(rename = "blockPrices")]
    pub block_prices: Vec<BlocknativeBlockPrices>,
}

impl TryFrom<BlocknativeBlockPricesResponse> for FeePerGasEstimated {
    type Error = MmError<NumConversError>;

    fn try_from(block_prices: BlocknativeBlockPricesResponse) -> Result<Self, Self::Error> {
        if block_prices.block_prices.is_empty() {
            return Ok(FeePerGasEstimated::default());
        }
        if block_prices.block_prices[0].estimated_prices.len() < FEE_PRIORITY_LEVEL_N {
            return Ok(FeePerGasEstimated::default());
        }
        // BlockNative returns prices sorted by confidence: high(0), medium(1), low(2)
        Ok(Self {
            base_fee: wei_from_gwei_decimal(&block_prices.block_prices[0].base_fee_per_gas)?,
            low: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[2].max_fee_per_gas,
                )?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[2].max_priority_fee_per_gas,
                )?,
                min_wait_time: None,
                max_wait_time: None,
            },
            medium: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[1].max_fee_per_gas,
                )?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[1].max_priority_fee_per_gas,
                )?,
                min_wait_time: None,
                max_wait_time: None,
            },
            high: FeePerGasLevel {
                max_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[0].max_fee_per_gas,
                )?,
                max_priority_fee_per_gas: wei_from_gwei_decimal(
                    &block_prices.block_prices[0].estimated_prices[0].max_priority_fee_per_gas,
                )?,
                min_wait_time: None,
                max_wait_time: None,
            },
            source: EstimationSource::Blocknative,
            base_fee_trend: String::default(),
            priority_fee_trend: String::default(),
        })
    }
}

pub(crate) struct BlocknativeGasApiCaller;

impl BlocknativeGasApiCaller {
    const ENDPOINT: &'static str = "gasprices/blockprices";

    fn get_url_and_headers(base_url: &str) -> (String, Vec<(&'static str, &'static str)>) {
        let url = format!(
            "{}/{}?confidenceLevels=10&confidenceLevels=50&confidenceLevels=90&withBaseFees=true",
            base_url.trim_end_matches('/'),
            Self::ENDPOINT
        );
        let headers = vec![("Authorization", BLOCKNATIVE_GAS_API_AUTH_TEST.as_str())];
        (url, headers)
    }

    async fn make_request(
        url: &str,
        headers: Vec<(&'static str, &'static str)>,
    ) -> Result<BlocknativeBlockPricesResponse, MmError<String>> {
        let resp = slurp_url_with_headers(url, headers).await.mm_err(|e| e.to_string())?;
        if resp.0 != StatusCode::OK {
            return MmError::err(format!("{} failed with status code {}", url, resp.0));
        }
        serde_json::from_slice(&resp.2).map_to_mm(|e| e.to_string())
    }

    pub async fn fetch_fee_estimation(base_url: &str) -> Web3RpcResult<FeePerGasEstimated> {
        let (url, headers) = Self::get_url_and_headers(base_url);
        let block_prices = Self::make_request(&url, headers)
            .await
            .mm_err(Web3RpcError::Transport)?;
        block_prices
            .try_into()
            .mm_err(|e: NumConversError| Web3RpcError::Internal(e.to_string()))
    }
}
