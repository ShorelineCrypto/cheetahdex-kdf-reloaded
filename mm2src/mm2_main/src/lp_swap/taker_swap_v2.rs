//! Taker-side Swap V2 state machine.
//!
//! The taker receives the maker's negotiation, sends funding, receives the
//! maker payment + funding-spend preimage, converts funding to taker payment,
//! and finally spends the maker payment after the maker reveals the secret.
//!
//! ## State Graph (happy path)
//!
//! ```text
//! Initialize → Initialized → Negotiated → TakerFundingSent
//!   → MakerPaymentAndFundingSpendPreimgReceived → MakerPaymentConfirmed
//!   → TakerPaymentSent → TakerPaymentSpent → MakerPaymentSpent → Completed
//! ```
//!
//! Error branches:
//! - `TakerFundingRefundRequired → TakerFundingRefunded`
//! - `TakerPaymentRefundRequired → TakerPaymentRefunded`
//! - Any early state → `Aborted`

use coins::{MakerCoinSwapOpsV2, MmCoin, TakerCoinSwapOpsV2};
use common::mm_number::MmNumber;
use crypto::secret_hash_algo::SecretHashAlgo;
use mm2_core::mm_ctx::MmArc;
use mm2_state_machine::prelude::TransitionFrom;
use rpc::v1::types::{Bytes as BytesJson, H256 as H256Json};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::swap_v2_common::*;
use super::SwapConfirmationsSettings;
use keys::KeyPair;

// ────────────────────────────────────────────────────────────────────────────
// Events (persisted to DB for recovery)
// ────────────────────────────────────────────────────────────────────────────

/// Every state transition emits one of these events.  On recovery, events are
/// replayed to reconstruct the last known state.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum TakerSwapEvent {
    /// Swap initialisation completed; trade fees estimated.
    Initialized {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        taker_payment_fee: MmNumber,
        maker_payment_spend_fee: MmNumber,
    },
    /// Negotiation with maker completed.
    Negotiated {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_payment_fee: MmNumber,
        maker_payment_spend_fee: MmNumber,
    },
    /// Taker funding transaction sent on-chain.
    TakerFundingSent {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_funding: BytesJson,
    },
    /// Taker funding needs to be refunded (error path).
    TakerFundingRefundRequired {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_funding: BytesJson,
        reason: AbortReason,
    },
    /// Maker payment received along with funding-spend preimage.
    MakerPaymentAndFundingSpendPreimgReceived {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_funding: BytesJson,
        funding_spend_preimage: StoredTxPreimage,
        maker_payment: BytesJson,
    },
    /// Maker payment confirmed (when confirmation gate enabled).
    MakerPaymentConfirmed {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_funding: BytesJson,
        funding_spend_preimage: StoredTxPreimage,
        maker_payment: BytesJson,
    },
    /// Taker payment sent (funding consumed → taker payment).
    TakerPaymentSent {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_payment: BytesJson,
        maker_payment: BytesJson,
    },
    /// Same as TakerPaymentSent but preimage sending is skipped (EVM coins).
    TakerPaymentSentPreimageSendingSkipped {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredTakerNegotiationData,
        taker_payment: BytesJson,
        maker_payment: BytesJson,
    },
    /// Taker payment needs refund (error path after taker payment was created).
    TakerPaymentRefundRequired {
        taker_payment: BytesJson,
        negotiation_data: StoredTakerNegotiationData,
        reason: AbortReason,
    },
    /// Taker payment was spent by maker (maker secret extracted).
    TakerPaymentSpent {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        taker_payment_spend: BytesJson,
        maker_payment: BytesJson,
        negotiation_data: StoredTakerNegotiationData,
    },
    /// Maker payment spent by taker (happy path near-completion).
    MakerPaymentSpent {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        maker_payment_spend: BytesJson,
        negotiation_data: StoredTakerNegotiationData,
    },
    /// Taker funding was refunded.
    TakerFundingRefunded {
        funding_tx: BytesJson,
        funding_tx_refund: BytesJson,
        reason: AbortReason,
    },
    /// Taker payment was refunded.
    TakerPaymentRefunded {
        taker_payment: BytesJson,
        taker_payment_refund: BytesJson,
        reason: AbortReason,
    },
    /// Swap aborted before any on-chain commitment.
    Aborted { reason: AbortReason },
    /// Swap completed successfully.
    Completed,
}

// ────────────────────────────────────────────────────────────────────────────
// Database representation
// ────────────────────────────────────────────────────────────────────────────

/// Serialisable representation of the taker swap stored in the DB.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TakerSwapDbRepr {
    pub maker_coin: String,
    pub maker_volume: MmNumber,
    pub taker_secret: H256Json,
    pub taker_secret_hash: BytesJson,
    pub secret_hash_algo: SecretHashAlgo,
    pub started_at: u64,
    pub lock_duration: u64,
    pub taker_coin: String,
    pub taker_volume: MmNumber,
    pub taker_premium: MmNumber,
    pub dex_fee_amount: MmNumber,
    pub dex_fee_burn: MmNumber,
    pub conf_settings: SwapConfirmationsSettings,
    pub uuid: Uuid,
    pub p2p_keypair: Option<super::maker_swap_v2::SerializableKeypairBytes>,
    pub events: Vec<TakerSwapEvent>,
    pub maker_p2p_pub: BytesJson,
    pub swap_version: u8,
}

// ────────────────────────────────────────────────────────────────────────────
// State machine
// ────────────────────────────────────────────────────────────────────────────

/// The taker swap V2 state machine, parameterized over concrete coin types.
pub struct TakerSwapStateMachine<MakerCoin: MmCoin + MakerCoinSwapOpsV2, TakerCoin: MmCoin + TakerCoinSwapOpsV2> {
    pub ctx: MmArc,
    pub started_at: u64,
    pub lock_duration: u64,
    pub maker_coin: MakerCoin,
    pub maker_volume: MmNumber,
    pub taker_coin: TakerCoin,
    pub taker_volume: MmNumber,
    pub taker_premium: MmNumber,
    pub secret_hash_algo: SecretHashAlgo,
    pub conf_settings: SwapConfirmationsSettings,
    pub uuid: Uuid,
    pub p2p_topic: String,
    pub p2p_keypair: Option<KeyPair>,
    pub taker_secret: primitives::hash::H256,
    pub maker_p2p_pubkey: Vec<u8>,
    /// If true, wait for maker payment confirmation before spending funding.
    pub require_maker_payment_confirm: bool,
    /// If true, wait for maker payment spend to confirm.
    pub require_maker_payment_spend_confirm: bool,
    pub swap_version: u8,
}

impl<MakerCoin, TakerCoin> TakerSwapStateMachine<MakerCoin, TakerCoin>
where
    MakerCoin: MmCoin + MakerCoinSwapOpsV2,
    TakerCoin: MmCoin + TakerCoinSwapOpsV2,
{
    /// Maker payment confirmation timeout: `started_at + lock_duration / 3`.
    pub fn maker_payment_conf_timeout(&self) -> u64 {
        self.started_at + self.lock_duration / 3
    }

    /// Taker funding locktime: `started_at + 3 * lock_duration`.
    pub fn taker_funding_locktime(&self) -> u64 {
        self.started_at + 3 * self.lock_duration
    }

    /// Taker payment locktime: `started_at + lock_duration`.
    pub fn taker_payment_locktime(&self) -> u64 {
        self.started_at + self.lock_duration
    }

    /// Swap unique data = `uuid.as_bytes()` (different from maker's secret_hash).
    pub fn unique_data(&self) -> Vec<u8> {
        self.uuid.as_bytes().to_vec()
    }

    /// Hash of the taker secret using the configured algorithm.
    pub fn taker_secret_hash(&self) -> Vec<u8> {
        self.secret_hash_algo.hash_secret(self.taker_secret.as_slice())
    }
}

// ────────────────────────────────────────────────────────────────────────────
// States
// ────────────────────────────────────────────────────────────────────────────

/// Initial state — validates inputs and estimates fees.
#[derive(Debug, Default)]
pub struct Initialize;

/// Waiting for maker negotiation data.
#[derive(Debug)]
pub struct Initialized {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub taker_payment_fee: MmNumber,
    pub maker_payment_spend_fee: MmNumber,
}

/// Negotiation complete — ready to send taker funding.
#[derive(Debug)]
pub struct Negotiated {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_payment_fee: MmNumber,
    pub maker_payment_spend_fee: MmNumber,
}

/// Taker funding sent on-chain; waiting for MakerPaymentInfo.
#[derive(Debug)]
pub struct TakerFundingSent {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_funding: BytesJson,
}

/// Maker payment and funding-spend preimage received.
#[derive(Debug)]
pub struct MakerPaymentAndFundingSpendPreimgReceived {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_funding: BytesJson,
    pub funding_spend_preimage: StoredTxPreimage,
    pub maker_payment: BytesJson,
}

/// Maker payment confirmed (when confirmation gate enabled).
#[derive(Debug)]
pub struct MakerPaymentConfirmed {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_funding: BytesJson,
    pub funding_spend_preimage: StoredTxPreimage,
    pub maker_payment: BytesJson,
}

/// Taker payment sent (funding consumed → taker payment on-chain).
#[derive(Debug)]
pub struct TakerPaymentSent {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_payment: BytesJson,
    pub maker_payment: BytesJson,
}

/// Like TakerPaymentSent but preimage sending is skipped (EVM).
#[derive(Debug)]
pub struct TakerPaymentSentPreimageSendingSkipped {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_payment: BytesJson,
    pub maker_payment: BytesJson,
}

/// Taker payment spent by maker — we can extract the secret.
#[derive(Debug)]
pub struct TakerPaymentSpent {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub taker_payment_spend: BytesJson,
    pub maker_payment: BytesJson,
    pub negotiation_data: StoredTakerNegotiationData,
}

/// Maker payment spent by taker (happy path near-completion).
#[derive(Debug)]
pub struct MakerPaymentSpent {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub maker_payment_spend: BytesJson,
    pub negotiation_data: StoredTakerNegotiationData,
}

/// Taker funding needs to be refunded.
#[derive(Debug)]
pub struct TakerFundingRefundRequired {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredTakerNegotiationData,
    pub taker_funding: BytesJson,
    pub reason: AbortReason,
}

/// Taker payment needs to be refunded.
#[derive(Debug)]
pub struct TakerPaymentRefundRequired {
    pub taker_payment: BytesJson,
    pub negotiation_data: StoredTakerNegotiationData,
    pub reason: AbortReason,
}

/// Taker funding was successfully refunded (terminal).
#[derive(Debug)]
pub struct TakerFundingRefunded {
    pub funding_tx: BytesJson,
    pub funding_tx_refund: BytesJson,
    pub reason: AbortReason,
}

/// Taker payment was successfully refunded (terminal).
#[derive(Debug)]
pub struct TakerPaymentRefunded {
    pub taker_payment: BytesJson,
    pub taker_payment_refund: BytesJson,
    pub reason: AbortReason,
}

/// Swap completed successfully (terminal).
#[derive(Debug)]
pub struct Completed;

/// Swap aborted before on-chain commitment (terminal).
#[derive(Debug)]
pub struct Aborted {
    pub reason: AbortReason,
}

// ────────────────────────────────────────────────────────────────────────────
// Transition declarations (compile-time enforced)
// ────────────────────────────────────────────────────────────────────────────

// Happy path
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for Initialized
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for Negotiated
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerFundingSent
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for MakerPaymentAndFundingSpendPreimgReceived
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for MakerPaymentConfirmed
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerPaymentSent
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerPaymentSentPreimageSendingSkipped
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerPaymentSpent
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for MakerPaymentSpent
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for Completed
{
}

// Error / abort transitions
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerFundingRefundRequired
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerFundingRefunded
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerPaymentRefundRequired
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for TakerPaymentRefunded
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<TakerSwapStateMachine<M, T>>
    for Aborted
{
}

// NOTE: State and LastState trait implementations are deferred until
// concrete coin types implement MakerCoinSwapOpsV2 / TakerCoinSwapOpsV2.
// See maker_swap_v2.rs for the same reasoning.
