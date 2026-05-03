use common::StatusCode;
use derive_more::Display;
use enum_derives::EnumFromStringify;
use ethereum_types::U256;
use mm2_net::transport::SlurpError;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Display, Serialize, EnumFromStringify)]
pub enum ApiClientError {
    // crd:pin-begin
    #[from_stringify("url::ParseError")]
    InvalidParam(String),
    #[display(fmt = "Parameter {param} out of bounds, value: {value}, min: {min} max: {max}")]
    OutOfBounds {
        param: String,
        value: String,
        min: String,
        max: String,
    },
    TransportError(SlurpError),
    ParseBodyError {
        error_msg: String,
    },
    #[display(fmt = "General API error: {error_msg} description: {description}")]
    GeneralApiError {
        error_msg: String,
        description: String,
        status_code: u16,
    },
    #[display(fmt = "Allowance not enough, needed: {amount} allowance: {allowance}")]
    AllowanceNotEnough {
        error_msg: String,
        description: String,
        status_code: u16,
        /// Allowance the router still needs granted before the swap can run.
        amount: U256,
        /// Allowance currently held by the router contract.
        allowance: U256,
    },
    // crd:pin-end
}

/// `meta.type` token marking the current-allowance figure of a 400 body.
const ALLOWANCE_META_KIND: &str = "allowance"; // crd:pin
/// `meta.type` token marking the required-amount figure of a 400 body.
const AMOUNT_META_KIND: &str = "amount"; // crd:pin

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Meta {
    #[serde(rename = "type")]
    pub meta_type: String, // crd:pin
    #[serde(rename = "value")]
    pub meta_value: String, // crd:pin
}

#[derive(Debug, Deserialize)]
pub(crate) struct Error400 {
    pub error: String,               // crd:pin
    pub description: Option<String>, // crd:pin
    #[serde(rename = "statusCode")]
    pub status_code: u16, // crd:pin
    pub meta: Option<Vec<Meta>>,     // crd:pin
    #[allow(dead_code)]
    #[serde(rename = "requestId")]
    pub request_id: Option<String>, // crd:pin
}

impl Error400 {
    /// Whether the body advertises an ERC-20 allowance shortfall, i.e. carries
    /// a `meta` entry of type `allowance`.
    fn signals_allowance_shortfall(&self) -> bool {
        self.meta_entries().any(|entry| entry.meta_type == ALLOWANCE_META_KIND)
    }

    /// Decimal `value` of the first `meta` entry tagged `kind`, decoded into a
    /// `U256`. A missing entry, or a value that is not a decimal integer, both
    /// resolve to zero — the lenient decode mandated by R10-E so a malformed
    /// figure never masks an actionable shortfall.
    fn meta_amount(&self, kind: &str) -> U256 {
        self.meta_entries()
            .find(|entry| entry.meta_type == kind)
            .and_then(|entry| U256::from_dec_str(&entry.meta_value).ok())
            .unwrap_or_default()
    }

    fn meta_entries(&self) -> impl Iterator<Item = &Meta> {
        self.meta.iter().flatten()
    }
}

#[derive(Debug)]
pub(crate) enum NativeError {
    HttpError { error_msg: String, status_code: u16 },
    HttpError400(Error400),
    ParseError { error_msg: String },
}

impl NativeError {
    pub(crate) fn new(status_code: StatusCode, body: Value) -> Self {
        // The structured parameter-error envelope only ships on 400 responses;
        // every other status carries no more than the plain `error` string.
        match status_code {
            StatusCode::BAD_REQUEST => match serde_json::from_value::<Error400>(body) {
                Ok(parsed) => NativeError::HttpError400(parsed),
                Err(decode_err) => NativeError::ParseError {
                    error_msg: format!("could not parse error response: {decode_err}"),
                },
            },
            other => NativeError::HttpError {
                error_msg: body["error"].as_str().unwrap_or_default().to_owned(),
                status_code: other.into(),
            },
        }
    }
}

impl ApiClientError {
    /// Lift a low-level [`NativeError`] into the public client error.
    ///
    /// A 400 body may embed an ERC-20 allowance shortfall inside its `meta`
    /// array; when that is recognised the required and current allowances are
    /// promoted into [`ApiClientError::AllowanceNotEnough`]. Anything else
    /// collapses onto the general-API, body-parse, or generic variants.
    pub(crate) fn from_native_error(api_error: NativeError) -> ApiClientError {
        match api_error {
            NativeError::HttpError { error_msg, status_code } => ApiClientError::GeneralApiError {
                error_msg,
                description: Default::default(),
                status_code,
            },
            NativeError::ParseError { error_msg } => ApiClientError::ParseBodyError { error_msg },
            NativeError::HttpError400(body) if body.signals_allowance_shortfall() => {
                // Read both figures before moving the owned string fields out.
                let amount = body.meta_amount(AMOUNT_META_KIND);
                let allowance = body.meta_amount(ALLOWANCE_META_KIND);
                ApiClientError::AllowanceNotEnough {
                    error_msg: body.error,
                    description: body.description.unwrap_or_default(),
                    status_code: body.status_code,
                    amount,
                    allowance,
                }
            },
            NativeError::HttpError400(body) => ApiClientError::GeneralApiError {
                error_msg: body.error,
                description: body.description.unwrap_or_default(),
                status_code: body.status_code,
            },
        }
    }
}
