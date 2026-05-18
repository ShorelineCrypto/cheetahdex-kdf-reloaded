use derive_more::Display;
use jsonrpc_core::{Error as RpcError, ErrorCode as RpcErrorCode};
use mm2_err_handle::prelude::*;
use serde_derive::{Deserialize, Serialize};
use web3::Error as Web3Error;

/// MetaMask uses JSON-RPC error code 4001 for user-rejected requests.
const USER_REJECTED_CODE: RpcErrorCode = RpcErrorCode::ServerError(4001);

pub type MetamaskResult<T> = MmResult<T, MetamaskError>;

/// Errors originating from MetaMask / EIP-1193 interactions.
#[derive(Debug, Display)]
pub enum MetamaskError {
    #[display(fmt = "ETH provider not found")]
    EthProviderNotFound,
    #[display(fmt = "Expected exactly one selected ETH account")]
    ExpectedOneEthAccount,
    #[display(fmt = "Active account does not match the original")]
    UnexpectedAccountSelected,
    #[display(fmt = "Error serializing RPC arguments: {_0}")]
    ErrorSerializingArguments(String),
    #[display(fmt = "Error deserializing RPC result: {_0}")]
    ErrorDeserializingMethodResult(String),
    #[display(fmt = "User rejected the request")]
    UserCancelled,
    #[display(fmt = "RPC error: {_0:?}")]
    Rpc(RpcError),
    #[display(fmt = "Transport error: {_0:?}")]
    Transport(String),
    #[display(fmt = "Internal error: {_0}")]
    Internal(String),
}

impl From<Web3Error> for MetamaskError {
    fn from(e: Web3Error) -> Self {
        match e {
            Web3Error::Decoder(msg) | Web3Error::InvalidResponse(msg) => {
                MetamaskError::ErrorDeserializingMethodResult(msg)
            },
            Web3Error::Transport(tr) => MetamaskError::Transport(tr.to_string()),
            Web3Error::Rpc(rpc) => {
                if rpc.code == USER_REJECTED_CODE {
                    MetamaskError::UserCancelled
                } else {
                    MetamaskError::Rpc(rpc)
                }
            },
            Web3Error::Io(io) => MetamaskError::Transport(io.to_string()),
            other => MetamaskError::Internal(other.to_string()),
        }
    }
}

/// Fieldless enumeration of MetaMask-related errors for RPC responses.
///
/// Only includes error variants that the GUI/CLI must handle specifically.
#[derive(Clone, Debug, Deserialize, Display, Serialize, PartialEq)]
pub enum MetamaskRpcError {
    EthProviderNotFound,
    #[display(fmt = "User rejected the request")]
    UserCancelled,
    #[display(fmt = "Unexpected ETH account selected — re-select or re-initialize MetaMask")]
    UnexpectedAccountSelected,
    #[display(fmt = "MetaMask context not initialized — activate via 'task::connect_metamask::init'")]
    MetamaskCtxNotInitialized,
}

/// Marker trait for RPC error types that can wrap a [`MetamaskRpcError`].
pub trait WithMetamaskRpcError {
    fn metamask_rpc_error(err: MetamaskRpcError) -> Self;
}

/// Marker trait for RPC error types that have an "internal error" variant.
pub trait WithInternal {
    fn internal(err: String) -> Self;
}

/// Converts a [`MetamaskError`] into any RPC error type that implements
/// both [`WithMetamaskRpcError`] and [`WithInternal`].
pub fn from_metamask_error<T>(err: MetamaskError) -> T
where
    T: WithMetamaskRpcError + WithInternal,
{
    match err {
        MetamaskError::EthProviderNotFound => T::metamask_rpc_error(MetamaskRpcError::EthProviderNotFound),
        MetamaskError::UnexpectedAccountSelected => T::metamask_rpc_error(MetamaskRpcError::UnexpectedAccountSelected),
        other => T::internal(other.to_string()),
    }
}
