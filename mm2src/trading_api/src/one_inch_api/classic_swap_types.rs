//! Structs to call 1inch classic swap api

use super::client::QueryParams;
use super::errors::ApiClientError;
use common::{def_with_opt_param, push_if_some};
use ethereum_types::Address;
use mm2_err_handle::mm_error::{MmError, MmResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use url::Url;

// crd:pin-begin
const ONE_INCH_MAX_SLIPPAGE: f32 = 50.0;
const ONE_INCH_MAX_FEE_SHARE: f32 = 3.0;
const ONE_INCH_MAX_GAS: u128 = 11500000;
const ONE_INCH_MAX_PARTS: u32 = 100;
const ONE_INCH_MAX_MAIN_ROUTE_PARTS: u32 = 50;
const ONE_INCH_MAX_COMPLEXITY_LEVEL: u32 = 3;
// crd:pin-end

const BAD_URL_IN_RESPONSE_ERROR: &str = "unsupported url in response";
const ONE_INCH_DOMAIN: &str = "1inch.io"; // crd:pin

/// API params builder for swap quote
#[derive(Default)]
pub struct ClassicSwapQuoteParams {
    // crd:pin-begin
    /// Source token address
    src: String,
    /// Destination token address
    dst: String,
    amount: String,
    // Optional fields
    fee: Option<f32>,
    protocols: Option<String>,
    gas_price: Option<String>,
    complexity_level: Option<u32>,
    parts: Option<u32>,
    main_route_parts: Option<u32>,
    gas_limit: Option<u128>,
    include_tokens_info: Option<bool>,
    include_protocols: Option<bool>,
    include_gas: Option<bool>,
    connector_tokens: Option<String>,
    // crd:pin-end
}

impl ClassicSwapQuoteParams {
    pub fn new(src: String, dst: String, amount: String) -> Self {
        Self {
            src,
            dst,
            amount,
            ..Default::default()
        }
    }

    // crd:pin-begin
    def_with_opt_param!(fee, f32);
    def_with_opt_param!(protocols, String);
    def_with_opt_param!(gas_price, String);
    def_with_opt_param!(complexity_level, u32);
    def_with_opt_param!(parts, u32);
    def_with_opt_param!(main_route_parts, u32);
    def_with_opt_param!(gas_limit, u128);
    def_with_opt_param!(include_tokens_info, bool);
    def_with_opt_param!(include_protocols, bool);
    def_with_opt_param!(include_gas, bool);
    def_with_opt_param!(connector_tokens, String);
    // crd:pin-end

    #[allow(clippy::result_large_err)]
    pub fn build_query_params(&self) -> MmResult<QueryParams, ApiClientError> {
        self.validate_params()?;

        let mut params = vec![
            // crd:pin-begin
            ("src", self.src.clone()),
            ("dst", self.dst.clone()),
            ("amount", self.amount.clone()),
            // crd:pin-end
        ];

        // crd:pin-begin
        push_if_some!(params, "fee", self.fee);
        push_if_some!(params, "protocols", &self.protocols);
        push_if_some!(params, "gasPrice", &self.gas_price);
        push_if_some!(params, "complexityLevel", self.complexity_level);
        push_if_some!(params, "parts", self.parts);
        push_if_some!(params, "mainRouteParts", self.main_route_parts);
        push_if_some!(params, "gasLimit", self.gas_limit);
        push_if_some!(params, "includeTokensInfo", self.include_tokens_info);
        push_if_some!(params, "includeProtocols", self.include_protocols);
        push_if_some!(params, "includeGas", self.include_gas);
        push_if_some!(params, "connectorTokens", &self.connector_tokens);
        // crd:pin-end
        Ok(params)
    }

    /// Validate params by 1inch rules (to avoid extra requests)
    #[allow(clippy::result_large_err)]
    fn validate_params(&self) -> MmResult<(), ApiClientError> {
        check_share_bounds("fee", self.fee, ONE_INCH_MAX_FEE_SHARE)?;
        check_max("complexity level", self.complexity_level, ONE_INCH_MAX_COMPLEXITY_LEVEL)?;
        check_max("gas_limit", self.gas_limit, ONE_INCH_MAX_GAS)?;
        check_max("parts", self.parts, ONE_INCH_MAX_PARTS)?;
        check_max("main route parts", self.main_route_parts, ONE_INCH_MAX_MAIN_ROUTE_PARTS)?;
        Ok(())
    }
}

/// API params builder to create a tx for swap
#[derive(Default)]
pub struct ClassicSwapCreateParams {
    // crd:pin-begin
    src: String,
    dst: String,
    amount: String,
    from: String,
    slippage: f32,
    // Optional fields
    fee: Option<f32>,
    protocols: Option<String>,
    gas_price: Option<String>,
    complexity_level: Option<u32>,
    parts: Option<u32>,
    main_route_parts: Option<u32>,
    gas_limit: Option<u128>,
    include_tokens_info: Option<bool>,
    include_protocols: Option<bool>,
    include_gas: Option<bool>,
    connector_tokens: Option<String>,
    excluded_protocols: Option<String>,
    permit: Option<String>,
    compatibility: Option<bool>,
    receiver: Option<String>,
    referrer: Option<String>,
    disable_estimate: Option<bool>,
    allow_partial_fill: Option<bool>,
    use_permit2: Option<bool>,
    // crd:pin-end
}

impl ClassicSwapCreateParams {
    pub fn new(src: String, dst: String, amount: String, from: String, slippage: f32) -> Self {
        Self {
            src,
            dst,
            amount,
            from,
            slippage,
            ..Default::default()
        }
    }

    // crd:pin-begin
    def_with_opt_param!(fee, f32);
    def_with_opt_param!(protocols, String);
    def_with_opt_param!(gas_price, String);
    def_with_opt_param!(complexity_level, u32);
    def_with_opt_param!(parts, u32);
    def_with_opt_param!(main_route_parts, u32);
    def_with_opt_param!(gas_limit, u128);
    def_with_opt_param!(include_tokens_info, bool);
    def_with_opt_param!(include_protocols, bool);
    def_with_opt_param!(include_gas, bool);
    def_with_opt_param!(connector_tokens, String);
    def_with_opt_param!(excluded_protocols, String);
    def_with_opt_param!(permit, String);
    def_with_opt_param!(compatibility, bool);
    def_with_opt_param!(receiver, String);
    def_with_opt_param!(referrer, String);
    def_with_opt_param!(disable_estimate, bool);
    def_with_opt_param!(allow_partial_fill, bool);
    def_with_opt_param!(use_permit2, bool);
    // crd:pin-end

    #[allow(clippy::result_large_err)]
    pub fn build_query_params(&self) -> MmResult<QueryParams, ApiClientError> {
        self.validate_params()?;

        let mut params = vec![
            // crd:pin-begin
            ("src", self.src.clone()),
            ("dst", self.dst.clone()),
            ("amount", self.amount.clone()),
            ("from", self.from.clone()),
            ("slippage", self.slippage.to_string()),
            // crd:pin-end
        ];

        // crd:pin-begin
        push_if_some!(params, "fee", self.fee);
        push_if_some!(params, "protocols", &self.protocols);
        push_if_some!(params, "gasPrice", &self.gas_price);
        push_if_some!(params, "complexityLevel", self.complexity_level);
        push_if_some!(params, "parts", self.parts);
        push_if_some!(params, "mainRouteParts", self.main_route_parts);
        push_if_some!(params, "gasLimit", self.gas_limit);
        push_if_some!(params, "includeTokensInfo", self.include_tokens_info);
        push_if_some!(params, "includeProtocols", self.include_protocols);
        push_if_some!(params, "includeGas", self.include_gas);
        push_if_some!(params, "connectorTokens", &self.connector_tokens);
        push_if_some!(params, "excludedProtocols", &self.excluded_protocols);
        push_if_some!(params, "permit", &self.permit);
        push_if_some!(params, "compatibility", &self.compatibility);
        push_if_some!(params, "receiver", &self.receiver);
        push_if_some!(params, "referrer", &self.referrer);
        push_if_some!(params, "disableEstimate", self.disable_estimate);
        push_if_some!(params, "allowPartialFill", self.allow_partial_fill);
        push_if_some!(params, "usePermit2", self.use_permit2);
        // crd:pin-end

        Ok(params)
    }

    /// Validate params by 1inch rules (to avoid extra requests)
    #[allow(clippy::result_large_err)]
    fn validate_params(&self) -> MmResult<(), ApiClientError> {
        check_share_bounds("slippage", Some(self.slippage), ONE_INCH_MAX_SLIPPAGE)?;
        check_share_bounds("fee", self.fee, ONE_INCH_MAX_FEE_SHARE)?;
        check_max("complexity level", self.complexity_level, ONE_INCH_MAX_COMPLEXITY_LEVEL)?;
        check_max("gas_limit", self.gas_limit, ONE_INCH_MAX_GAS)?;
        check_max("parts", self.parts, ONE_INCH_MAX_PARTS)?;
        check_max("main route parts", self.main_route_parts, ONE_INCH_MAX_MAIN_ROUTE_PARTS)?;
        Ok(())
    }
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct TokenInfo {
    // crd:pin-begin
    pub address: Address,
    pub symbol: String,
    pub name: String,
    pub decimals: u32,
    pub eip2612: bool,
    // crd:pin-end
    #[serde(rename = "isFoT", default)]
    pub is_fot: bool,
    #[serde(rename = "logoURI", with = "serde_one_inch_link")]
    pub logo_uri: String,
    pub tags: Vec<String>, // crd:pin
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProtocolInfo {
    // crd:pin-begin
    pub name: String,
    pub part: f64,
    // crd:pin-end
    #[serde(rename = "fromTokenAddress")]
    pub from_token_address: Address,
    #[serde(rename = "toTokenAddress")]
    pub to_token_address: Address,
}

/// Returned data from an API call to get quote or create swap
#[derive(Clone, Deserialize, Debug)]
pub struct ClassicSwapData {
    /// dst token amount to receive, in api is a decimal number as string
    #[serde(rename = "dstAmount")]
    pub dst_amount: String,
    #[serde(rename = "srcToken")]
    pub src_token: Option<TokenInfo>,
    #[serde(rename = "dstToken")]
    pub dst_token: Option<TokenInfo>,
    pub protocols: Option<Vec<Vec<Vec<ProtocolInfo>>>>, // crd:pin
    /// Returned from create swap call
    pub tx: Option<TxFields>, // crd:pin
    /// Returned from quote call
    pub gas: Option<u128>, // crd:pin
}

#[derive(Clone, Deserialize, Debug)]
pub struct TxFields {
    // crd:pin-begin
    pub from: Address,
    pub to: Address,
    pub data: String,
    // crd:pin-end
    /// tx value, in api is a decimal number as string
    pub value: String, // crd:pin
    /// gas price, in api is a decimal number as string
    #[serde(rename = "gasPrice")]
    pub gas_price: String,
    /// gas limit, in api is a decimal number
    pub gas: u128, // crd:pin
}

#[derive(Deserialize, Serialize)]
pub struct ProtocolImage {
    // crd:pin-begin
    pub id: String,
    pub title: String,
    // crd:pin-end
    #[serde(with = "serde_one_inch_link")]
    pub img: String, // crd:pin
    #[serde(with = "serde_one_inch_link")]
    pub img_color: String, // crd:pin
}

#[derive(Deserialize)]
pub struct ProtocolsResponse {
    pub protocols: Vec<ProtocolImage>, // crd:pin
}

#[derive(Deserialize)]
pub struct TokensResponse {
    pub tokens: HashMap<String, TokenInfo>, // crd:pin
}

mod serde_one_inch_link {
    use super::validate_one_inch_link;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    /// Just forward to the normal serializer
    pub(super) fn serialize<S>(s: &String, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize(serializer)
    }

    /// Deserialise String with checking links
    pub(super) fn deserialize<'a, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'a>,
    {
        <String as Deserialize>::deserialize(deserializer)
            .map(|value| validate_one_inch_link(&value).unwrap_or_default())
    }
}

/// Builds the [`ApiClientError::OutOfBounds`] error shared by every bounds
/// check. The lower bound advertised to callers is always zero.
#[allow(clippy::result_large_err)]
fn out_of_bounds<V, M>(param: &str, value: V, max: M) -> MmError<ApiClientError>
where
    V: ToString,
    M: ToString,
{
    MmError::new(ApiClientError::OutOfBounds {
        param: param.to_owned(),
        value: value.to_string(),
        min: 0.to_string(),
        max: max.to_string(),
    })
}

/// Range check for the `0.0..=max` "share" parameters (fee, slippage). A `None`
/// value is treated as absent and accepted.
#[allow(clippy::result_large_err)]
fn check_share_bounds(param: &str, value: Option<f32>, max: f32) -> MmResult<(), ApiClientError> {
    match value {
        Some(value) if !(0.0..=max).contains(&value) => Err(out_of_bounds(param, value, max)),
        _ => Ok(()),
    }
}

/// Upper-bound check for the unsigned-integer parameters. A `None` value is
/// treated as absent and accepted.
#[allow(clippy::result_large_err)]
fn check_max<T>(param: &str, value: Option<T>, max: T) -> MmResult<(), ApiClientError>
where
    T: PartialOrd + ToString,
{
    match value {
        Some(value) if value > max => Err(out_of_bounds(param, value, max)),
        _ => Ok(()),
    }
}

/// Validate that the URL is a subdomain of the 1inch domain.
#[allow(clippy::result_large_err)]
fn validate_one_inch_link(s: &str) -> MmResult<String, ApiClientError> {
    let host_is_trusted = Url::parse(s)
        .ok()
        .and_then(|url| url.host().map(|host| host.to_string()))
        .is_some_and(|host| host.ends_with(ONE_INCH_DOMAIN));

    if host_is_trusted {
        Ok(s.to_owned())
    } else {
        MmError::err(ApiClientError::ParseBodyError {
            error_msg: BAD_URL_IN_RESPONSE_ERROR.to_owned(),
        })
    }
}

#[test]
fn test_validate_one_inch_link() {
    assert!(validate_one_inch_link("https://cdn.1inch.io/liquidity-sources-logo/wmatic_color.png").is_ok());
    assert!(validate_one_inch_link("https://example.org/somepath/somefile.png").is_err());
    assert!(validate_one_inch_link("https://inch.io/somepath/somefile.png").is_err());
    assert!(validate_one_inch_link("127.0.0.1").is_err());
}
