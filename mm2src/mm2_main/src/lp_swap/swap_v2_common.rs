//! Common types and helpers shared between Maker V2 and Taker V2 swap state machines.
//!
//! ## Confirmation / Visibility Policy (V2 Protocol)
//!
//! The V2 protocol introduces a "funding" step before the actual payment.  The
//! maker validates taker funding in mempool (0-conf by default), then sends the
//! maker payment.  Both sides have configurable confirmation gates:
//!
//! * `require_taker_funding_confirm_before_maker_payment` (maker side, default: false)
//! * `require_maker_payment_confirm_before_funding_spend`  (taker side, default: true)
//! * `require_taker_payment_spend_confirm`                 (maker side, default: true)
//! * `require_maker_payment_spend_confirm`                 (taker side, default: true)
//!
//! When a confirmation gate is enabled we wait for `min(configured_confs, 1)` block
//! confirmations before proceeding.  If disabled, mempool visibility suffices.
//!
//! ### Visibility grace
//!
//! On chains with delayed mempool propagation we poll every
//! [`SWAP_TX_VISIBILITY_POLL_SECS`] seconds, giving up after
//! [`SWAP_TX_VISIBILITY_GRACE_SECS`].

use common::executor::{spawn, Timer};
use common::log::info;
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use rpc::v1::types::Bytes as BytesJson;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::swap_lock::{SwapLock, SwapLockOps};

// ────────────────────────────────────────────────────────────────────────────
// Constants
// ────────────────────────────────────────────────────────────────────────────

/// Maximum time (seconds) to wait for a transaction to appear in the mempool
/// before considering it missing.
pub const SWAP_TX_VISIBILITY_GRACE_SECS: f64 = 30.0;

/// Polling interval when checking mempool visibility.
pub const SWAP_TX_VISIBILITY_POLL_SECS: f64 = 1.0;

/// P2P negotiation timeout — how long each side waits for the counterparty's
/// negotiation messages before aborting.
pub const NEGOTIATION_TIMEOUT_SEC: u64 = 90;

/// The topic prefix used for V2 swap P2P messages.
pub const SWAP_V2_PREFIX: &str = "swapv2";

// ────────────────────────────────────────────────────────────────────────────
// V2 swap P2P messages  (serde-JSON based, no protobuf yet)
// ────────────────────────────────────────────────────────────────────────────

/// V2 negotiation data sent by the maker at swap start.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MakerNegotiation {
    pub started_at: u64,
    pub payment_locktime: u64,
    pub secret_hash: BytesJson,
    pub maker_coin_htlc_pub: BytesJson,
    pub taker_coin_htlc_pub: BytesJson,
    pub maker_coin_swap_contract: Option<BytesJson>,
    pub taker_coin_swap_contract: Option<BytesJson>,
    pub taker_coin_address: String,
}

/// The taker's response to maker negotiation.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TakerNegotiation {
    /// The taker agrees and provides their negotiation data.
    Continue(TakerNegotiationData),
    /// The taker aborts with a reason.
    Abort(String),
}

/// Taker negotiation data, sent inside `TakerNegotiation::Continue`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TakerNegotiationData {
    pub started_at: u64,
    pub funding_locktime: u64,
    pub payment_locktime: u64,
    pub taker_secret_hash: BytesJson,
    pub maker_coin_htlc_pub: BytesJson,
    pub taker_coin_htlc_pub: BytesJson,
    pub maker_coin_swap_contract: Option<BytesJson>,
    pub taker_coin_swap_contract: Option<BytesJson>,
}

/// Maker's confirmation that negotiation succeeded.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MakerNegotiated {
    pub negotiated: bool,
    pub reason: Option<String>,
}

/// Taker tells maker about the funding transaction.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TakerFundingInfo {
    pub tx_bytes: BytesJson,
    pub next_step_instructions: Option<Vec<u8>>,
}

/// Maker tells taker about the maker payment and funding-spend preimage.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MakerPaymentInfo {
    pub tx_bytes: BytesJson,
    pub next_step_instructions: Option<Vec<u8>>,
    pub funding_preimage_sig: BytesJson,
    pub funding_preimage_tx: BytesJson,
}

/// Taker tells maker about the taker payment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TakerPaymentInfo {
    pub tx_bytes: BytesJson,
    pub next_step_instructions: Option<Vec<u8>>,
}

/// Taker sends the preimage for the taker payment spend.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TakerPaymentSpendPreimage {
    pub signature: BytesJson,
    pub tx_preimage: BytesJson,
}

/// Multiplexed V2 swap message.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SwapV2Msg {
    MakerNegotiation(MakerNegotiation),
    TakerNegotiation(TakerNegotiation),
    MakerNegotiated(MakerNegotiated),
    TakerFundingInfo(TakerFundingInfo),
    MakerPaymentInfo(MakerPaymentInfo),
    TakerPaymentInfo(TakerPaymentInfo),
    TakerPaymentSpendPreimage(TakerPaymentSpendPreimage),
}

/// In-memory store for V2 swap P2P messages, analogous to `SwapMsgStore` for V1.
#[derive(Debug, Default)]
pub struct SwapV2MsgStore {
    pub maker_negotiation: Option<MakerNegotiation>,
    pub taker_negotiation: Option<TakerNegotiation>,
    pub maker_negotiated: Option<MakerNegotiated>,
    pub taker_funding_info: Option<TakerFundingInfo>,
    pub maker_payment_info: Option<MakerPaymentInfo>,
    pub taker_payment_info: Option<TakerPaymentInfo>,
    pub taker_payment_spend_preimage: Option<TakerPaymentSpendPreimage>,
}

// ────────────────────────────────────────────────────────────────────────────
// Error / abort types
// ────────────────────────────────────────────────────────────────────────────

/// Reason a V2 swap was aborted.
#[derive(Clone, Debug, Deserialize, Display, Serialize)]
pub enum AbortReason {
    #[display(fmt = "Negotiation timed out")]
    NegotiationTimeout,
    #[display(fmt = "Negotiation failed: {}", _0)]
    NegotiationFailed(String),
    #[display(fmt = "Failed to send coin tx: {}", _0)]
    FailedToSendTx(String),
    #[display(fmt = "Failed to validate tx: {}", _0)]
    FailedToValidateTx(String),
    #[display(fmt = "Confirmation timed out: {}", _0)]
    ConfirmationTimeout(String),
    #[display(fmt = "Funding spend error: {}", _0)]
    FundingSpendError(String),
    #[display(fmt = "Taker aborted: {}", _0)]
    TakerAborted(String),
    #[display(fmt = "Internal error: {}", _0)]
    InternalError(String),
}

/// Errors produced by the V2 state machine infrastructure itself.
#[derive(Debug, Display)]
pub enum SwapStateMachineError {
    #[display(fmt = "Storage error: {}", _0)]
    Storage(String),
    #[display(fmt = "Reentrancy lock error: {}", _0)]
    ReentrancyLock(String),
    #[display(fmt = "Recreate error: {}", _0)]
    Recreate(String),
}

/// Errors when recreating a state machine from stored events.
#[derive(Debug, Display)]
pub enum SwapRecreateError {
    #[display(fmt = "No events to recreate from")]
    NoEvents,
    #[display(fmt = "Coin not found: {}", _0)]
    CoinNotFound(String),
    #[display(fmt = "Coin not active: {}", _0)]
    CoinNotActive(String),
    #[display(fmt = "Internal: {}", _0)]
    Internal(String),
}

/// Context needed to recreate coin instances during swap recovery.
pub struct SwapRecreateCtx<MakerCoin, TakerCoin> {
    pub maker_coin: MakerCoin,
    pub taker_coin: TakerCoin,
}

// ────────────────────────────────────────────────────────────────────────────
// Active swap tracking
// ────────────────────────────────────────────────────────────────────────────

/// Metadata about a running V2 swap, stored in `SwapsContext` for UI queries.
#[derive(Clone, Debug)]
pub struct ActiveSwapV2Info {
    pub uuid: Uuid,
    pub maker_coin: String,
    pub taker_coin: String,
    pub swap_type: SwapV2Type,
}

/// Distinguishes maker V2 from taker V2 swaps in storage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SwapV2Type {
    MakerV2 = 1,
    TakerV2 = 2,
}

// ────────────────────────────────────────────────────────────────────────────
// Serializable preimage (for DB persistence)
// ────────────────────────────────────────────────────────────────────────────

/// A preimage + signature pair stored as raw bytes in the DB.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredTxPreimage {
    pub preimage: BytesJson,
    pub signature: BytesJson,
}

// ────────────────────────────────────────────────────────────────────────────
// Negotiation data (what gets stored in events for reconstruction)
// ────────────────────────────────────────────────────────────────────────────

/// Stored negotiation data from the maker side (used in maker events).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredMakerNegotiationData {
    pub taker_secret_hash: BytesJson,
    pub taker_coin_htlc_pub: BytesJson,
    pub maker_coin_htlc_pub: BytesJson,
    pub taker_coin_swap_contract: Option<BytesJson>,
    pub maker_coin_swap_contract: Option<BytesJson>,
    pub taker_payment_locktime: u64,
    pub taker_funding_locktime: u64,
}

/// Stored negotiation data from the taker side (used in taker events).
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StoredTakerNegotiationData {
    pub maker_secret_hash: BytesJson,
    pub maker_coin_htlc_pub: BytesJson,
    pub taker_coin_htlc_pub: BytesJson,
    pub maker_coin_swap_contract: Option<BytesJson>,
    pub taker_coin_swap_contract: Option<BytesJson>,
    pub maker_payment_locktime: u64,
    pub taker_coin_address: String,
}

// ────────────────────────────────────────────────────────────────────────────
// Reentrancy lock helpers
// ────────────────────────────────────────────────────────────────────────────

/// Acquire a reentrancy lock for the given swap UUID.
pub async fn acquire_reentrancy_lock_impl(
    ctx: &MmArc,
    uuid: Uuid,
    ttl_sec: f64,
) -> Result<SwapLock, MmError<SwapStateMachineError>> {
    match SwapLock::lock(ctx, uuid, ttl_sec).await {
        Ok(Some(lock)) => Ok(lock),
        Ok(None) => MmError::err(SwapStateMachineError::ReentrancyLock(format!(
            "Swap {} is already locked by another instance",
            uuid
        ))),
        Err(e) => MmError::err(SwapStateMachineError::ReentrancyLock(e.to_string())),
    }
}

/// Spawn a background loop that periodically touches the swap lock to renew its TTL.
pub fn spawn_reentrancy_lock_renew(lock: SwapLock, interval_sec: f64) {
    spawn(async move {
        loop {
            Timer::sleep(interval_sec).await;
            if let Err(e) = lock.touch().await {
                // If renewal fails the lock will eventually expire, allowing recovery.
                info!("Failed to renew swap lock: {}", e);
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swap_v2_msg_serde_roundtrip() {
        let msg = SwapV2Msg::MakerNegotiation(MakerNegotiation {
            started_at: 1234567890,
            payment_locktime: 9999,
            secret_hash: BytesJson::from(vec![1, 2, 3]),
            maker_coin_htlc_pub: BytesJson::from(vec![4, 5, 6]),
            taker_coin_htlc_pub: BytesJson::from(vec![7, 8, 9]),
            maker_coin_swap_contract: None,
            taker_coin_swap_contract: None,
            taker_coin_address: "R9abc123".into(),
        });
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SwapV2Msg = serde_json::from_str(&json).unwrap();
        match decoded {
            SwapV2Msg::MakerNegotiation(n) => {
                assert_eq!(n.started_at, 1234567890);
                assert_eq!(n.payment_locktime, 9999);
                assert_eq!(n.taker_coin_address, "R9abc123");
            },
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_taker_negotiation_abort_serde() {
        let msg = SwapV2Msg::TakerNegotiation(TakerNegotiation::Abort("bad coin".into()));
        let json = serde_json::to_string(&msg).unwrap();
        let decoded: SwapV2Msg = serde_json::from_str(&json).unwrap();
        match decoded {
            SwapV2Msg::TakerNegotiation(TakerNegotiation::Abort(reason)) => {
                assert_eq!(reason, "bad coin");
            },
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_stored_tx_preimage_serde() {
        let stored = StoredTxPreimage {
            preimage: BytesJson::from(vec![0xAA, 0xBB]),
            signature: BytesJson::from(vec![0xCC, 0xDD]),
        };
        let json = serde_json::to_string(&stored).unwrap();
        let back: StoredTxPreimage = serde_json::from_str(&json).unwrap();
        assert_eq!(back.preimage.0, vec![0xAA, 0xBB]);
        assert_eq!(back.signature.0, vec![0xCC, 0xDD]);
    }

    #[test]
    fn test_abort_reason_display() {
        let reason = AbortReason::NegotiationTimeout;
        assert_eq!(format!("{}", reason), "Negotiation timed out");

        let reason = AbortReason::FailedToSendTx("insufficient funds".into());
        assert!(format!("{}", reason).contains("insufficient funds"));
    }
}
