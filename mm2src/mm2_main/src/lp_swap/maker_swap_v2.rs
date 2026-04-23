//! Maker-side Swap V2 state machine.
//!
//! The maker initiates data exchange, receives taker funding, sends maker
//! payment, generates the funding-spend preimage, and ultimately spends the
//! taker payment to complete the swap.
//!
//! ## State Graph (happy path)
//!
//! ```text
//! Initialize → Initialized → WaitingForTakerFunding → TakerFundingReceived
//!   → MakerPaymentSentFundingSpendGenerated → TakerPaymentReceived
//!   → TakerPaymentSpent → Completed
//! ```
//!
//! Error branch: `MakerPaymentRefundRequired → MakerPaymentRefunded`
//! Abort branch:  any early state → `Aborted`

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

/// Every state transition emits one of these events, which is appended to the
/// swap's event log in storage.  On recovery, events are replayed to reach the
/// last known state.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum MakerSwapEvent {
    /// Swap initialisation completed; trade fees estimated.
    Initialized {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        maker_payment_trade_fee: MmNumber,
        taker_payment_spend_trade_fee: MmNumber,
    },
    /// Waiting for taker to send funding; negotiation data exchanged.
    WaitingForTakerFunding {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        maker_payment_trade_fee: MmNumber,
    },
    /// Taker funding transaction received and validated.
    TakerFundingReceived {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        taker_funding: BytesJson,
        maker_payment_trade_fee: MmNumber,
    },
    /// Maker payment broadcast and funding-spend preimage generated.
    MakerPaymentSentFundingSpendGenerated {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        maker_payment: BytesJson,
        taker_funding: BytesJson,
        funding_spend_preimage: StoredTxPreimage,
    },
    /// Maker payment needs to be refunded (error recovery).
    MakerPaymentRefundRequired {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        maker_payment: BytesJson,
        reason: AbortReason,
    },
    /// Maker payment was successfully refunded.
    MakerPaymentRefunded {
        maker_payment: BytesJson,
        maker_payment_refund: BytesJson,
        reason: AbortReason,
    },
    /// Taker payment transaction received and validated.
    TakerPaymentReceived {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        maker_payment: BytesJson,
        taker_payment: BytesJson,
    },
    /// Like `TakerPaymentReceived` but taker sends no spend preimage (EVM coins).
    TakerPaymentReceivedPreimageSkipped {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        negotiation_data: StoredMakerNegotiationData,
        maker_payment: BytesJson,
        taker_payment: BytesJson,
    },
    /// Taker payment was successfully spent (happy path near-completion).
    TakerPaymentSpent {
        maker_coin_start_block: u64,
        taker_coin_start_block: u64,
        maker_payment: BytesJson,
        taker_payment: BytesJson,
        taker_payment_spend: BytesJson,
        negotiation_data: StoredMakerNegotiationData,
    },
    /// Swap aborted before maker payment was sent.
    Aborted { reason: AbortReason },
    /// Swap completed successfully.
    Completed,
}

// ────────────────────────────────────────────────────────────────────────────
// Database representation
// ────────────────────────────────────────────────────────────────────────────

/// Serialisable representation of the maker swap stored in the DB.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MakerSwapDbRepr {
    pub maker_coin: String,
    pub maker_volume: MmNumber,
    pub maker_secret: H256Json,
    pub maker_secret_hash: BytesJson,
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
    pub p2p_keypair: Option<SerializableKeypairBytes>,
    pub events: Vec<MakerSwapEvent>,
    pub taker_p2p_pub: BytesJson,
    pub swap_version: u8,
}

/// Opaque serialized P2P keypair bytes.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SerializableKeypairBytes(pub Vec<u8>);

// ────────────────────────────────────────────────────────────────────────────
// State machine
// ────────────────────────────────────────────────────────────────────────────

/// The maker swap V2 state machine, parameterized over concrete coin types.
///
/// Both `MakerCoin` and `TakerCoin` must implement their respective V2 ops
/// traits.  At present no concrete coin type implements them — this struct
/// provides the scaffold that coin implementations will plug into.
pub struct MakerSwapStateMachine<MakerCoin: MmCoin + MakerCoinSwapOpsV2, TakerCoin: MmCoin + TakerCoinSwapOpsV2> {
    pub ctx: MmArc,
    pub maker_coin: MakerCoin,
    pub maker_volume: MmNumber,
    pub secret: primitives::hash::H256,
    pub secret_hash_algo: SecretHashAlgo,
    pub started_at: u64,
    pub lock_duration: u64,
    pub taker_coin: TakerCoin,
    pub taker_volume: MmNumber,
    pub taker_premium: MmNumber,
    pub conf_settings: SwapConfirmationsSettings,
    pub uuid: Uuid,
    pub p2p_topic: String,
    pub p2p_keypair: Option<KeyPair>,
    pub taker_p2p_pubkey: Vec<u8>,
    /// If true, wait for taker funding to confirm before sending maker payment.
    pub require_taker_funding_confirm: bool,
    /// If true, wait for taker payment spend to confirm before marking complete.
    pub require_taker_payment_spend_confirm: bool,
    pub swap_version: u8,
}

impl<MakerCoin, TakerCoin> MakerSwapStateMachine<MakerCoin, TakerCoin>
where
    MakerCoin: MmCoin + MakerCoinSwapOpsV2,
    TakerCoin: MmCoin + TakerCoinSwapOpsV2,
{
    /// Taker payment confirmation timeout:  `started_at + lock_duration * 2/3`.
    pub fn taker_payment_conf_timeout(&self) -> u64 {
        self.started_at + self.lock_duration * 2 / 3
    }

    /// Maker payment locktime: `started_at + 2 * lock_duration`.
    pub fn maker_payment_locktime(&self) -> u64 {
        self.started_at + 2 * self.lock_duration
    }

    /// Compute the secret hash using the configured algorithm.
    pub fn secret_hash(&self) -> Vec<u8> {
        self.secret_hash_algo.hash_secret(self.secret.as_slice())
    }

    /// Swap unique data = the secret hash (used as HTLC identifier).
    pub fn unique_data(&self) -> Vec<u8> {
        self.secret_hash()
    }
}

// ────────────────────────────────────────────────────────────────────────────
// States
// ────────────────────────────────────────────────────────────────────────────

/// Initial state — validates inputs and estimates trade fees.
#[derive(Debug, Default)]
pub struct Initialize;

/// Negotiation complete — sends MakerNegotiation, waits for TakerNegotiation.
#[derive(Debug)]
pub struct Initialized {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub maker_payment_trade_fee: MmNumber,
    pub taker_payment_spend_trade_fee: MmNumber,
}

/// Negotiation confirmed — waiting for TakerFundingInfo.
#[derive(Debug)]
pub struct WaitingForTakerFunding {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub maker_payment_trade_fee: MmNumber,
}

/// Taker funding received and validated.
#[derive(Debug)]
pub struct TakerFundingReceived {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub taker_funding: BytesJson,
    pub maker_payment_trade_fee: MmNumber,
}

/// Maker payment sent on-chain and funding-spend preimage generated.
#[derive(Debug)]
pub struct MakerPaymentSentFundingSpendGenerated {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub maker_payment: BytesJson,
    pub taker_funding: BytesJson,
    pub funding_spend_preimage: StoredTxPreimage,
}

/// Taker payment received (with preimage validation).
#[derive(Debug)]
pub struct TakerPaymentReceived {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub maker_payment: BytesJson,
    pub taker_payment: BytesJson,
}

/// Taker payment received (preimage validation skipped — EVM coins).
#[derive(Debug)]
pub struct TakerPaymentReceivedPreimageSkipped {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub maker_payment: BytesJson,
    pub taker_payment: BytesJson,
}

/// Taker payment successfully spent.
#[derive(Debug)]
pub struct TakerPaymentSpent {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub maker_payment: BytesJson,
    pub taker_payment: BytesJson,
    pub taker_payment_spend: BytesJson,
    pub negotiation_data: StoredMakerNegotiationData,
}

/// Maker payment needs to be refunded.
#[derive(Debug)]
pub struct MakerPaymentRefundRequired {
    pub maker_coin_start_block: u64,
    pub taker_coin_start_block: u64,
    pub negotiation_data: StoredMakerNegotiationData,
    pub maker_payment: BytesJson,
    pub reason: AbortReason,
}

/// Maker payment was refunded (terminal).
#[derive(Debug)]
pub struct MakerPaymentRefunded {
    pub maker_payment: BytesJson,
    pub maker_payment_refund: BytesJson,
    pub reason: AbortReason,
}

/// Swap completed successfully (terminal).
#[derive(Debug)]
pub struct Completed;

/// Swap aborted before maker payment was sent (terminal).
#[derive(Debug)]
pub struct Aborted {
    pub reason: AbortReason,
}

// ────────────────────────────────────────────────────────────────────────────
// Transition declarations (compile-time enforced)
// ────────────────────────────────────────────────────────────────────────────

// Happy path
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for Initialized
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for WaitingForTakerFunding
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for TakerFundingReceived
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for MakerPaymentSentFundingSpendGenerated
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for TakerPaymentReceived
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for TakerPaymentReceivedPreimageSkipped
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for TakerPaymentSpent
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for Completed
{
}

// Error / abort transitions
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for MakerPaymentRefundRequired
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for MakerPaymentRefunded
{
}
impl<M: MmCoin + MakerCoinSwapOpsV2, T: MmCoin + TakerCoinSwapOpsV2> TransitionFrom<MakerSwapStateMachine<M, T>>
    for Aborted
{
}

// NOTE: State and LastState trait implementations are deferred until
// concrete coin types implement MakerCoinSwapOpsV2 / TakerCoinSwapOpsV2.
// The generic state machine + non-generic state structs pattern requires
// either PhantomData or concrete types. For now the scaffold defines:
//   - Event types and DB representation for persistence
//   - State structs with all the data each state carries
//   - TransitionFrom declarations for compile-time
//     transition validation
// The on_changed() logic will be added when the state machine is
// instantiated with concrete types (e.g. via StorableStateMachine).
