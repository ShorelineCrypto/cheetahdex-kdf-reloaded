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
use mm2_state_machine::storable_state_machine::{StateMachineDbRepr, StateMachineStorage};
use rpc::v1::types::{Bytes as BytesJson, H256 as H256Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::swap_lock::{SwapLock, SwapLockOps};
use super::{maker_swap_v2::MakerSwapDbRepr, maker_swap_v2::MakerSwapEvent};
use super::{taker_swap_v2::TakerSwapDbRepr, taker_swap_v2::TakerSwapEvent};

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
    #[display(fmt = "Maker aborted: {}", _0)]
    MakerAborted(String),
    #[display(fmt = "Internal error: {}", _0)]
    InternalError(String),
    // ── Taker-side abort reasons ──
    #[display(fmt = "Failed to send payment: {}", _0)]
    FailedToSendPayment(String),
    #[display(fmt = "Did not receive maker payment: {}", _0)]
    DidNotReceiveMakerPayment(String),
    #[display(fmt = "Failed to parse maker payment: {}", _0)]
    FailedToParseMakerPayment(String),
    #[display(fmt = "Failed to parse funding spend preimage: {}", _0)]
    FailedToParseFundingSpendPreimg(String),
    #[display(fmt = "Failed to parse funding spend signature: {}", _0)]
    FailedToParseFundingSpendSig(String),
    #[display(fmt = "Maker payment validation failed: {}", _0)]
    MakerPaymentValidationFailed(String),
    #[display(fmt = "Funding spend preimage validation failed: {}", _0)]
    FundingSpendPreimageValidationFailed(String),
    #[display(fmt = "Maker payment not confirmed in time: {}", _0)]
    MakerPaymentNotConfirmedInTime(String),
    #[display(fmt = "Failed to generate spend preimage: {}", _0)]
    FailedToGenerateSpendPreimage(String),
    #[display(fmt = "Maker did not spend taker payment in time: {}", _0)]
    MakerDidNotSpendInTime(String),
    #[display(fmt = "Could not extract maker secret: {}", _0)]
    CouldNotExtractSecret(String),
    #[display(fmt = "Failed to spend maker payment: {}", _0)]
    FailedToSpendMakerPayment(String),
    #[display(fmt = "Maker payment spend not confirmed in time: {}", _0)]
    MakerPaymentSpendNotConfirmedInTime(String),
    #[display(fmt = "Taker funding refund failed: {}", _0)]
    TakerFundingRefundFailed(String),
    #[display(fmt = "Taker payment refund failed: {}", _0)]
    TakerPaymentRefundFailed(String),
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

// ────────────────────────────────────────────────────────────────────────────
// StateMachineDbRepr impls
// ────────────────────────────────────────────────────────────────────────────

impl StateMachineDbRepr for MakerSwapDbRepr {
    type Event = MakerSwapEvent;

    fn add_event(&mut self, event: Self::Event) {
        self.events.push(event);
    }
}

impl StateMachineDbRepr for TakerSwapDbRepr {
    type Event = TakerSwapEvent;

    fn add_event(&mut self, event: Self::Event) {
        self.events.push(event);
    }
}

// ────────────────────────────────────────────────────────────────────────────
// V2 Swap Storage — Native (SQLite)
// ────────────────────────────────────────────────────────────────────────────

cfg_native! {
    use async_trait::async_trait;
    use crypto::secret_hash_algo::SecretHashAlgo;
    use db_common::sqlite::rusqlite::params;
    use serde_json;
    use std::str::FromStr;
    use super::{MAKER_SWAP_V2_TYPE, TAKER_SWAP_V2_TYPE};

    fn secret_hash_algo_to_i64(algo: SecretHashAlgo) -> i64 {
        match algo {
            SecretHashAlgo::DHASH160 => 0,
            SecretHashAlgo::SHA256 => 1,
        }
    }

    /// SQLite-backed storage for V2 maker swaps.
    pub struct MakerSwapStorage {
        ctx: MmArc,
    }

    impl MakerSwapStorage {
        pub fn new(ctx: MmArc) -> Self { MakerSwapStorage { ctx } }
        pub fn get_ctx(&self) -> MmArc { self.ctx.clone() }
    }

    #[async_trait]
    impl StateMachineStorage for MakerSwapStorage {
        type MachineId = Uuid;
        type DbRepr = MakerSwapDbRepr;
        type Error = MmError<SwapStateMachineError>;

        async fn store_repr(&mut self, id: Self::MachineId, repr: Self::DbRepr) -> Result<(), Self::Error> {
            insert_swap_v2_maker(&self.ctx, &id, &repr)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn get_repr(&self, id: Self::MachineId) -> Result<Self::DbRepr, Self::Error> {
            get_swap_repr::<MakerSwapDbRepr>(&self.ctx, &id, MAKER_SWAP_V2_TYPE)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn has_record_for(&mut self, id: &Self::MachineId) -> Result<bool, Self::Error> {
            has_swap_v2_record(&self.ctx, id)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn store_event(&mut self, id: Self::MachineId, event: MakerSwapEvent) -> Result<(), Self::Error> {
            append_swap_v2_event::<MakerSwapEvent>(&self.ctx, &id, &event)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn get_unfinished(&self) -> Result<Vec<Self::MachineId>, Self::Error> {
            get_unfinished_swap_uuids(&self.ctx, MAKER_SWAP_V2_TYPE)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn mark_finished(&mut self, id: Self::MachineId) -> Result<(), Self::Error> {
            mark_swap_v2_finished(&self.ctx, &id)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }
    }

    /// SQLite-backed storage for V2 taker swaps.
    pub struct TakerSwapStorage {
        ctx: MmArc,
    }

    impl TakerSwapStorage {
        pub fn new(ctx: MmArc) -> Self { TakerSwapStorage { ctx } }
        pub fn get_ctx(&self) -> MmArc { self.ctx.clone() }
    }

    #[async_trait]
    impl StateMachineStorage for TakerSwapStorage {
        type MachineId = Uuid;
        type DbRepr = TakerSwapDbRepr;
        type Error = MmError<SwapStateMachineError>;

        async fn store_repr(&mut self, id: Self::MachineId, repr: Self::DbRepr) -> Result<(), Self::Error> {
            insert_swap_v2_taker(&self.ctx, &id, &repr)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn get_repr(&self, id: Self::MachineId) -> Result<Self::DbRepr, Self::Error> {
            get_swap_repr::<TakerSwapDbRepr>(&self.ctx, &id, TAKER_SWAP_V2_TYPE)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn has_record_for(&mut self, id: &Self::MachineId) -> Result<bool, Self::Error> {
            has_swap_v2_record(&self.ctx, id)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn store_event(&mut self, id: Self::MachineId, event: TakerSwapEvent) -> Result<(), Self::Error> {
            append_swap_v2_event::<TakerSwapEvent>(&self.ctx, &id, &event)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn get_unfinished(&self) -> Result<Vec<Self::MachineId>, Self::Error> {
            get_unfinished_swap_uuids(&self.ctx, TAKER_SWAP_V2_TYPE)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }

        async fn mark_finished(&mut self, id: Self::MachineId) -> Result<(), Self::Error> {
            mark_swap_v2_finished(&self.ctx, &id)
                .map_to_mm(|e| SwapStateMachineError::Storage(e.to_string()))
        }
    }

    // ── SQL helper functions ────────────────────────────────────────────

    /// Insert a new V2 swap record into the my_swaps table.
    /// For maker swaps: my_coin = maker_coin, other_coin = taker_coin.
    /// For taker swaps: my_coin = taker_coin, other_coin = maker_coin.
    fn insert_swap_v2_maker(ctx: &MmArc, uuid: &Uuid, repr: &MakerSwapDbRepr) -> Result<(), String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        let events_json = serde_json::to_string(&serde_json::json!([])).unwrap();
        let maker_vol_str = repr.maker_volume.to_decimal().to_string();
        let taker_vol_str = repr.taker_volume.to_decimal().to_string();
        let premium_str = repr.taker_premium.to_decimal().to_string();
        let dex_fee_str = repr.dex_fee_amount.to_decimal().to_string();
        let dex_fee_burn_str = repr.dex_fee_burn.to_decimal().to_string();
        let secret: Vec<u8> = repr.maker_secret.0.to_vec();
        let secret_hash: Vec<u8> = repr.maker_secret_hash.to_vec();
        let secret_hash_algo: i64 = secret_hash_algo_to_i64(repr.secret_hash_algo);
        let p2p_privkey: Vec<u8> = repr.p2p_keypair.as_ref().map(|k| k.0.clone()).unwrap_or_default();
        let other_p2p_pub: Vec<u8> = repr.taker_p2p_pub.to_vec();

        conn.execute(
            "INSERT INTO my_swaps (
                my_coin, other_coin, uuid, started_at, swap_type, is_finished, events_json,
                maker_volume, taker_volume, premium, dex_fee, dex_fee_burn,
                secret, secret_hash, secret_hash_algo, p2p_privkey, lock_duration,
                maker_coin_confs, maker_coin_nota, taker_coin_confs, taker_coin_nota,
                other_p2p_pub, swap_version
            ) VALUES (?1,?2,?3,?4,?5,0,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
            params![
                repr.maker_coin, repr.taker_coin, uuid_str, repr.started_at as i64,
                MAKER_SWAP_V2_TYPE as i64, events_json,
                maker_vol_str, taker_vol_str, premium_str, dex_fee_str, dex_fee_burn_str,
                secret, secret_hash, secret_hash_algo, p2p_privkey,
                repr.lock_duration as i64,
                repr.conf_settings.maker_coin_confs as i64, repr.conf_settings.maker_coin_nota as i64,
                repr.conf_settings.taker_coin_confs as i64, repr.conf_settings.taker_coin_nota as i64,
                other_p2p_pub, repr.swap_version as i64,
            ],
        )
        .map(|_| ())
        .map_err(|e| format!("Failed to insert V2 maker swap {}: {}", uuid, e))
    }

    fn insert_swap_v2_taker(ctx: &MmArc, uuid: &Uuid, repr: &TakerSwapDbRepr) -> Result<(), String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        let events_json = serde_json::to_string(&serde_json::json!([])).unwrap();
        let maker_vol_str = repr.maker_volume.to_decimal().to_string();
        let taker_vol_str = repr.taker_volume.to_decimal().to_string();
        let premium_str = repr.taker_premium.to_decimal().to_string();
        let dex_fee_str = repr.dex_fee_amount.to_decimal().to_string();
        let dex_fee_burn_str = repr.dex_fee_burn.to_decimal().to_string();
        let secret: Vec<u8> = repr.taker_secret.0.to_vec();
        let secret_hash: Vec<u8> = repr.taker_secret_hash.to_vec();
        let secret_hash_algo: i64 = secret_hash_algo_to_i64(repr.secret_hash_algo);
        let p2p_privkey: Vec<u8> = repr.p2p_keypair.as_ref().map(|k| k.0.clone()).unwrap_or_default();
        let other_p2p_pub: Vec<u8> = repr.maker_p2p_pub.to_vec();

        conn.execute(
            "INSERT INTO my_swaps (
                my_coin, other_coin, uuid, started_at, swap_type, is_finished, events_json,
                maker_volume, taker_volume, premium, dex_fee, dex_fee_burn,
                secret, secret_hash, secret_hash_algo, p2p_privkey, lock_duration,
                maker_coin_confs, maker_coin_nota, taker_coin_confs, taker_coin_nota,
                other_p2p_pub, swap_version
            ) VALUES (?1,?2,?3,?4,?5,0,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22)",
            params![
                repr.taker_coin, repr.maker_coin, uuid_str, repr.started_at as i64,
                TAKER_SWAP_V2_TYPE as i64, events_json,
                maker_vol_str, taker_vol_str, premium_str, dex_fee_str, dex_fee_burn_str,
                secret, secret_hash, secret_hash_algo, p2p_privkey,
                repr.lock_duration as i64,
                repr.conf_settings.maker_coin_confs as i64, repr.conf_settings.maker_coin_nota as i64,
                repr.conf_settings.taker_coin_confs as i64, repr.conf_settings.taker_coin_nota as i64,
                other_p2p_pub, repr.swap_version as i64,
            ],
        )
        .map(|_| ())
        .map_err(|e| format!("Failed to insert V2 taker swap {}: {}", uuid, e))
    }

    /// Check if a V2 swap record exists for the given UUID.
    fn has_swap_v2_record(ctx: &MmArc, uuid: &Uuid) -> Result<bool, String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM my_swaps WHERE uuid = ?1",
                params![uuid_str],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to check swap record for {}: {}", uuid, e))?;
        Ok(count > 0)
    }

    /// Append an event to the events_json array of a V2 swap.
    fn append_swap_v2_event<E: Serialize>(ctx: &MmArc, uuid: &Uuid, event: &E) -> Result<(), String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();

        // Read existing events
        let events_str: String = conn
            .query_row(
                "SELECT events_json FROM my_swaps WHERE uuid = ?1",
                params![uuid_str],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to read events for {}: {}", uuid, e))?;

        let mut events: Vec<serde_json::Value> = serde_json::from_str(&events_str)
            .map_err(|e| format!("Failed to parse events_json for {}: {}", uuid, e))?;

        let event_val = serde_json::to_value(event)
            .map_err(|e| format!("Failed to serialize event for {}: {}", uuid, e))?;
        events.push(event_val);

        let updated = serde_json::to_string(&events)
            .map_err(|e| format!("Failed to serialize updated events for {}: {}", uuid, e))?;

        conn.execute(
            "UPDATE my_swaps SET events_json = ?1 WHERE uuid = ?2",
            params![updated, uuid_str],
        )
        .map(|_| ())
        .map_err(|e| format!("Failed to update events for swap {}: {}", uuid, e))
    }

    /// Get the full swap DB repr for a V2 swap.
    /// We reconstruct it from the row columns + deserialized events.
    fn get_swap_repr<R: for<'de> Deserialize<'de>>(ctx: &MmArc, uuid: &Uuid, swap_type: u8) -> Result<R, String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();

        let row = conn
            .query_row(
                "SELECT my_coin, other_coin, started_at, events_json,
                        maker_volume, taker_volume, premium, dex_fee, dex_fee_burn,
                        secret, secret_hash, secret_hash_algo, p2p_privkey,
                        lock_duration, maker_coin_confs, maker_coin_nota,
                        taker_coin_confs, taker_coin_nota, other_p2p_pub, swap_version
                 FROM my_swaps WHERE uuid = ?1",
                params![uuid_str],
                |row| {
                    Ok(SwapV2Row {
                        my_coin: row.get(0)?,
                        other_coin: row.get(1)?,
                        started_at: row.get::<_, i64>(2)? as u64,
                        events_json: row.get(3)?,
                        maker_volume: row.get(4)?,
                        taker_volume: row.get(5)?,
                        premium: row.get(6)?,
                        dex_fee: row.get(7)?,
                        dex_fee_burn: row.get(8)?,
                        secret: row.get(9)?,
                        secret_hash: row.get(10)?,
                        secret_hash_algo: row.get::<_, i64>(11)? as u8,
                        p2p_privkey: row.get(12)?,
                        lock_duration: row.get::<_, i64>(13)? as u64,
                        maker_coin_confs: row.get::<_, i64>(14)? as u64,
                        maker_coin_nota: row.get::<_, i64>(15)? != 0,
                        taker_coin_confs: row.get::<_, i64>(16)? as u64,
                        taker_coin_nota: row.get::<_, i64>(17)? != 0,
                        other_p2p_pub: row.get(18)?,
                        swap_version: row.get::<_, i64>(19)? as u8,
                    })
                },
            )
            .map_err(|e| format!("Failed to read swap repr for {}: {}", uuid, e))?;

        // Build the full repr as JSON, then deserialize to R.
        // This works because MakerSwapDbRepr and TakerSwapDbRepr are both Deserialize.
        let events_val: serde_json::Value = serde_json::from_str(&row.events_json)
            .map_err(|e| format!("Failed to parse events_json for {}: {}", uuid, e))?;

        let mut secret_arr = [0u8; 32];
        let len = row.secret.len().min(32);
        secret_arr[..len].copy_from_slice(&row.secret[..len]);
        let secret_h256 = H256Json::from(secret_arr);

        let p2p_keypair = if row.p2p_privkey.iter().any(|&b| b != 0) {
            Some(serde_json::json!(row.p2p_privkey))
        } else {
            None
        };

        let secret_hash_algo_str = match row.secret_hash_algo {
            1 => "SHA256",
            _ => "DHASH160",
        };

        let conf_settings = serde_json::json!({
            "maker_coin_confs": row.maker_coin_confs,
            "maker_coin_nota": row.maker_coin_nota,
            "taker_coin_confs": row.taker_coin_confs,
            "taker_coin_nota": row.taker_coin_nota,
        });

        let repr_json = if swap_type == MAKER_SWAP_V2_TYPE {
            serde_json::json!({
                "maker_coin": row.my_coin,
                "maker_volume": row.maker_volume,
                "maker_secret": secret_h256,
                "maker_secret_hash": row.secret_hash,
                "secret_hash_algo": secret_hash_algo_str,
                "started_at": row.started_at,
                "lock_duration": row.lock_duration,
                "taker_coin": row.other_coin,
                "taker_volume": row.taker_volume,
                "taker_premium": row.premium,
                "dex_fee_amount": row.dex_fee,
                "dex_fee_burn": row.dex_fee_burn,
                "conf_settings": conf_settings,
                "uuid": uuid_str,
                "p2p_keypair": p2p_keypair,
                "events": events_val,
                "taker_p2p_pub": row.other_p2p_pub,
                "swap_version": row.swap_version,
            })
        } else {
            serde_json::json!({
                "maker_coin": row.other_coin,
                "maker_volume": row.maker_volume,
                "taker_secret": secret_h256,
                "taker_secret_hash": row.secret_hash,
                "secret_hash_algo": secret_hash_algo_str,
                "started_at": row.started_at,
                "lock_duration": row.lock_duration,
                "taker_coin": row.my_coin,
                "taker_volume": row.taker_volume,
                "taker_premium": row.premium,
                "dex_fee_amount": row.dex_fee,
                "dex_fee_burn": row.dex_fee_burn,
                "conf_settings": conf_settings,
                "uuid": uuid_str,
                "p2p_keypair": p2p_keypair,
                "events": events_val,
                "maker_p2p_pub": row.other_p2p_pub,
                "swap_version": row.swap_version,
            })
        };

        serde_json::from_value(repr_json)
            .map_err(|e| format!("Failed to deserialize swap repr for {}: {}", uuid, e))
    }

    /// Helper struct to hold a row from my_swaps.
    struct SwapV2Row {
        my_coin: String,
        other_coin: String,
        started_at: u64,
        events_json: String,
        maker_volume: String,
        taker_volume: String,
        premium: String,
        dex_fee: String,
        dex_fee_burn: String,
        secret: Vec<u8>,
        secret_hash: Vec<u8>,
        secret_hash_algo: u8,
        p2p_privkey: Vec<u8>,
        lock_duration: u64,
        maker_coin_confs: u64,
        maker_coin_nota: bool,
        taker_coin_confs: u64,
        taker_coin_nota: bool,
        other_p2p_pub: Vec<u8>,
        swap_version: u8,
    }

    /// Get UUIDs of all unfinished V2 swaps of the given type.
    fn get_unfinished_swap_uuids(ctx: &MmArc, swap_type: u8) -> Result<Vec<Uuid>, String> {
        let conn = ctx.sqlite_connection();
        let mut stmt = conn
            .prepare("SELECT uuid FROM my_swaps WHERE is_finished = 0 AND swap_type = ?1")
            .map_err(|e| format!("Failed to prepare unfinished swaps query: {}", e))?;

        let uuids = stmt
            .query_map(params![swap_type as i64], |row| {
                let uuid_str: String = row.get(0)?;
                Ok(uuid_str)
            })
            .map_err(|e| format!("Failed to query unfinished swaps: {}", e))?
            .filter_map(|r| r.ok())
            .filter_map(|s| Uuid::from_str(&s).ok())
            .collect();

        Ok(uuids)
    }

    /// Mark a V2 swap as finished.
    fn mark_swap_v2_finished(ctx: &MmArc, uuid: &Uuid) -> Result<(), String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        conn.execute(
            "UPDATE my_swaps SET is_finished = 1 WHERE uuid = ?1",
            params![uuid_str],
        )
        .map(|_| ())
        .map_err(|e| format!("Failed to mark swap {} as finished: {}", uuid, e))
    }

    /// Read all events for a V2 swap from the DB (for recovery).
    pub fn read_swap_v2_events<E: for<'de> Deserialize<'de>>(ctx: &MmArc, uuid: &Uuid) -> Result<Vec<E>, String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        let events_str: String = conn
            .query_row(
                "SELECT events_json FROM my_swaps WHERE uuid = ?1",
                params![uuid_str],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to read events for {}: {}", uuid, e))?;
        serde_json::from_str(&events_str)
            .map_err(|e| format!("Failed to deserialize events for {}: {}", uuid, e))
    }

    /// Read the swap_type for a given UUID (for dispatch during RPC).
    pub fn get_swap_type(ctx: &MmArc, uuid: &Uuid) -> Result<u8, String> {
        let conn = ctx.sqlite_connection();
        let uuid_str = uuid.to_string();
        let swap_type: i64 = conn
            .query_row(
                "SELECT swap_type FROM my_swaps WHERE uuid = ?1",
                params![uuid_str],
                |row| row.get(0),
            )
            .map_err(|e| format!("Failed to read swap_type for {}: {}", uuid, e))?;
        Ok(swap_type as u8)
    }
}

// ────────────────────────────────────────────────────────────────────────────
// V2 Swap Storage — WASM (IndexedDB)
// ────────────────────────────────────────────────────────────────────────────

cfg_wasm32! {
    use async_trait::async_trait;
    use serde_json;
    use super::{MAKER_SWAP_V2_TYPE, TAKER_SWAP_V2_TYPE};

    /// IndexedDB-backed storage for V2 maker swaps.
    pub struct MakerSwapStorage {
        ctx: MmArc,
    }

    impl MakerSwapStorage {
        pub fn new(ctx: MmArc) -> Self { MakerSwapStorage { ctx } }
        pub fn get_ctx(&self) -> MmArc { self.ctx.clone() }
    }

    #[async_trait]
    impl StateMachineStorage for MakerSwapStorage {
        type MachineId = Uuid;
        type DbRepr = MakerSwapDbRepr;
        type Error = MmError<SwapStateMachineError>;

        async fn store_repr(&mut self, _id: Self::MachineId, _repr: Self::DbRepr) -> Result<(), Self::Error> {
            // TODO: WASM IndexedDB storage
            Ok(())
        }

        async fn get_repr(&self, _id: Self::MachineId) -> Result<Self::DbRepr, Self::Error> {
            MmError::err(SwapStateMachineError::Storage("WASM get_repr not yet implemented".into()))
        }

        async fn has_record_for(&mut self, _id: &Self::MachineId) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn store_event(&mut self, _id: Self::MachineId, _event: MakerSwapEvent) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_unfinished(&self) -> Result<Vec<Self::MachineId>, Self::Error> {
            Ok(vec![])
        }

        async fn mark_finished(&mut self, _id: Self::MachineId) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    /// IndexedDB-backed storage for V2 taker swaps.
    pub struct TakerSwapStorage {
        ctx: MmArc,
    }

    impl TakerSwapStorage {
        pub fn new(ctx: MmArc) -> Self { TakerSwapStorage { ctx } }
        pub fn get_ctx(&self) -> MmArc { self.ctx.clone() }
    }

    #[async_trait]
    impl StateMachineStorage for TakerSwapStorage {
        type MachineId = Uuid;
        type DbRepr = TakerSwapDbRepr;
        type Error = MmError<SwapStateMachineError>;

        async fn store_repr(&mut self, _id: Self::MachineId, _repr: Self::DbRepr) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_repr(&self, _id: Self::MachineId) -> Result<Self::DbRepr, Self::Error> {
            MmError::err(SwapStateMachineError::Storage("WASM get_repr not yet implemented".into()))
        }

        async fn has_record_for(&mut self, _id: &Self::MachineId) -> Result<bool, Self::Error> {
            Ok(false)
        }

        async fn store_event(&mut self, _id: Self::MachineId, _event: TakerSwapEvent) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn get_unfinished(&self) -> Result<Vec<Self::MachineId>, Self::Error> {
            Ok(vec![])
        }

        async fn mark_finished(&mut self, _id: Self::MachineId) -> Result<(), Self::Error> {
            Ok(())
        }
    }
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
