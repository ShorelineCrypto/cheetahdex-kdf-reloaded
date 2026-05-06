/// Error types for task-based L2 activation.
use crate::prelude::CoinConfWithProtocolError;
use common::{HttpStatusCode, StatusCode};
use derive_more::Display;
use rpc_task::rpc_common::{RpcTaskStatusError, RpcTaskUserActionError};
use rpc_task::RpcTaskError;
use ser_error_derive::SerializeErrorType;
use serde_derive::Serialize;
use std::time::Duration;

pub type L2ActivationStatusError = RpcTaskStatusError;
pub type L2ActivationUserActionError = RpcTaskUserActionError;
pub type CancelL2ActivationError = RpcTaskStatusError;

#[derive(Clone, Debug, Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum L2ActivationError {
    #[display(fmt = "Layer 2 {} is already activated", _0)]
    AlreadyActivated(String),
    #[display(fmt = "Layer 2 {} config is not found", _0)]
    ConfigNotFound(String),
    #[display(fmt = "Layer 2 {} protocol parse error: {}", ticker, error)]
    ProtocolParseError {
        ticker: String,
        error: String,
    },
    #[display(fmt = "Unexpected layer 2 protocol for {}", ticker)]
    UnexpectedProtocol {
        ticker: String,
    },
    #[display(fmt = "Platform coin {} is not activated", _0)]
    PlatformNotActivated(String),
    #[display(fmt = "{} is not a valid platform for L2 {}", platform_coin_ticker, l2_ticker)]
    IncompatiblePlatform {
        platform_coin_ticker: String,
        l2_ticker: String,
    },
    #[display(fmt = "Invalid platform configuration for {}: {}", platform_coin_ticker, err)]
    InvalidPlatformConfig {
        platform_coin_ticker: String,
        err: String,
    },
    #[display(fmt = "L2 configuration parsing failed: {}", _0)]
    ConfigParseError(String),
    #[display(fmt = "Activation task timed out after {:?}", duration)]
    TaskTimedOut {
        duration: Duration,
    },
    Transport(String),
    Internal(String),
}

impl From<CoinConfWithProtocolError> for L2ActivationError {
    fn from(err: CoinConfWithProtocolError) -> Self {
        match err {
            CoinConfWithProtocolError::ConfigIsNotFound(ticker) => L2ActivationError::ConfigNotFound(ticker),
            CoinConfWithProtocolError::CoinProtocolParseError { ticker, err } => {
                L2ActivationError::ProtocolParseError {
                    ticker,
                    error: err.to_string(),
                }
            },
            CoinConfWithProtocolError::UnexpectedProtocol { ticker, .. } => {
                L2ActivationError::UnexpectedProtocol { ticker }
            },
        }
    }
}

impl From<RpcTaskError> for L2ActivationError {
    fn from(rpc_err: RpcTaskError) -> Self {
        match rpc_err {
            RpcTaskError::Timeout(duration) => L2ActivationError::TaskTimedOut { duration },
            other => L2ActivationError::Internal(other.to_string()),
        }
    }
}

impl HttpStatusCode for L2ActivationError {
    fn status_code(&self) -> StatusCode {
        match self {
            L2ActivationError::AlreadyActivated(_)
            | L2ActivationError::PlatformNotActivated(_)
            | L2ActivationError::ConfigNotFound(_)
            | L2ActivationError::UnexpectedProtocol { .. } => StatusCode::BAD_REQUEST,
            L2ActivationError::TaskTimedOut { .. } => StatusCode::REQUEST_TIMEOUT,
            L2ActivationError::ProtocolParseError { .. }
            | L2ActivationError::IncompatiblePlatform { .. }
            | L2ActivationError::InvalidPlatformConfig { .. }
            | L2ActivationError::ConfigParseError(_)
            | L2ActivationError::Transport(_)
            | L2ActivationError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
