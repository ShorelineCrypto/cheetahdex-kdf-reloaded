//! Structs to call 1inch portfolio api

use super::client::QueryParams;
use super::errors::ApiClientError;
use common::{def_with_opt_param, push_if_some};
use mm2_err_handle::mm_error::MmResult;
use mm2_number::BigDecimal;
use serde::Deserialize;
use std::fmt;

#[derive(Default)]
pub enum DataGranularity {
    // crd:pin-begin
    Month,
    Week,
    Day,
    FourHour,
    Hour,
    FifteenMin,
    #[default]
    FiveMin,
    // crd:pin-end
}

impl DataGranularity {
    /// Wire token the provider expects for this granularity.
    fn as_token(&self) -> &'static str {
        // crd:pin-begin
        match self {
            DataGranularity::Month => "month",
            DataGranularity::Week => "week",
            DataGranularity::Day => "day",
            DataGranularity::FourHour => "4hour",
            DataGranularity::Hour => "hour",
            DataGranularity::FifteenMin => "15min",
            DataGranularity::FiveMin => "5min",
            // crd:pin-end
        }
    }
}

impl fmt::Display for DataGranularity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_token()) }
}

/// API params builder to get OHLC price history for token pair
/// See 1inch docs: https://portal.1inch.dev/documentation/apis/portfolio/swagger?method=get&path=%2Fintegrations%2Fprices%2Fv1%2Ftime_range%2Fcross_prices
#[derive(Default)]
pub struct CrossPriceParams {
    // crd:pin-begin
    chain_id: u64,
    /// Base token address
    token0_address: String,
    /// Quote token address
    token1_address: String,
    /// Returned time series intervals
    granularity: Option<DataGranularity>,
    /// max number of time series
    limit: Option<u32>,
    // crd:pin-end
}

impl CrossPriceParams {
    pub fn new(chain_id: u64, token0_address: String, token1_address: String) -> Self {
        Self {
            chain_id,             // crd:pin
            token0_address,       // crd:pin
            token1_address,       // crd:pin
            ..Default::default()  // crd:pin
        }
    }

    def_with_opt_param!(granularity, DataGranularity); // crd:pin
    def_with_opt_param!(limit, u32); // crd:pin

    #[allow(clippy::result_large_err)]
    pub fn build_query_params(&self) -> MmResult<QueryParams, ApiClientError> {
        let mut params = vec![
            // crd:pin
            // crd:pin-begin
            ("chain_id", self.chain_id.to_string()),
            ("token0_address", self.token0_address.clone()),
            ("token1_address", self.token1_address.clone()),
            // crd:pin-end
        ]; // crd:pin

        // crd:pin-begin
        push_if_some!(params, "granularity", &self.granularity);
        push_if_some!(params, "limit", &self.limit);
        // crd:pin-end

        Ok(params) // crd:pin
    }
}

/// Element of token_0/token_1 price series returned from the 1inch cross_prices call.
/// Contains OHLC (Open, High, Low, Close) prices for the granularity period.
/// TODO: check cross_prices v2
#[derive(Clone, Deserialize, Debug)]
pub struct CrossPricesData {
    /// Time of the granularity period
    pub timestamp: u64, // crd:pin
    /// Price at the period opening
    pub open: BigDecimal, // crd:pin
    /// Lowest price within the period
    pub low: BigDecimal, // crd:pin
    /// Average price within the period
    pub avg: BigDecimal, // crd:pin
    /// Highest price within the period
    pub high: BigDecimal, // crd:pin
    /// Price at the period closing
    pub close: BigDecimal, // crd:pin
}

pub type CrossPricesSeries = Vec<CrossPricesData>;
