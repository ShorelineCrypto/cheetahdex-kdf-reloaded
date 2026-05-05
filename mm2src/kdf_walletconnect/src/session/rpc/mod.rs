//! Per-method WalletConnect v2 session RPC payloads.
//!
//! One submodule per session method. Each carries the request/response param
//! shapes and the IRN relay tag pair the relay uses to route the message. The
//! tag values are dictated by the WalletConnect v2 specification.

pub mod delete;
pub mod event;
pub mod extend;
pub mod ping;
pub mod propose;
pub mod settle;
pub mod update;

/// A request/response IRN tag pair for a session method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IrnTag {
    pub request: u32,
    pub response: u32,
}
