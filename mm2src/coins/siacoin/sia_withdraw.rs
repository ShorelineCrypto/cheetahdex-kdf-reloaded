//! # Siacoin withdraw flow
//!
//! Builds a signed Sia v2 transaction from an Iguana-keypair wallet in
//! response to a `withdraw` RPC. The flow is:
//!
//! 1. Pull all unspent outputs for the from-address.
//! 2. Pick a coin-selection plan (either `max` — spend everything minus
//!    fee — or `specific amount` — largest-first selection).
//! 3. Assemble the transaction (recipient output, miner fee, optional
//!    change output), sign with the keypair, and return a
//!    [`TransactionDetails`] envelope for the RPC layer.
//!
//! Coin selection is deliberately simple (largest-first, no UTXO
//! consolidation heuristics) because Sia's per-input cost model is
//! flat.
//!
//! # Invariants
//! - Only `PrivKeyPolicy::KeyPair` (Iguana) is supported. HD-wallet
//!   support is gated on upstream `sia-rust` work and is intentionally
//!   not implemented here.
//! - The miner fee is a hard-coded constant ([`TX_FEE_HASTINGS`]) for
//!   now; size-aware fee estimation is future work.

use std::str::FromStr;

use mm2_err_handle::mm_error::MmError;
use rpc::v1::types::Bytes as BytesJson;

use common::now_ms;

use crate::siacoin::{
    hastings_to_siacoin, siacoin_to_hastings, Address, ApiClientHelpers, Currency, SiaCoin, SiaFeeDetails,
    SiaFeePolicy, SiaKeypair as Keypair, SiacoinElement, SiacoinOutput, SpendPolicy, V2TransactionBuilder,
};
use crate::{
    MarketCoinOps, PrivKeyPolicy, TransactionDetails, TransactionType, WithdrawError, WithdrawRequest, WithdrawResult,
};

/// Flat miner fee applied to every withdraw transaction (10 SC, in hastings).
///
/// TODO: switch to size-aware fee estimation once the v2 builder exposes
/// a serialized-size helper.
const TX_FEE_HASTINGS: u128 = 10_000_000_000_000_000_000;

/// Result of [`SiaWithdrawBuilder::plan_inputs`]: the inputs we will
/// spend plus the derived split into recipient amount, change, and
/// total input value (all in hastings).
struct InputPlan {
    /// UTXOs that will be consumed as inputs.
    inputs: Vec<SiacoinElement>,
    /// Amount to send to the recipient.
    recipient_amount: Currency,
    /// Change returned to the from-address (`ZERO` when `max == true`).
    change_amount: Currency,
    /// Sum of `inputs.value` — equals `recipient_amount + change + fee`.
    input_sum: Currency,
}

/// Assembles, signs, and returns a Sia withdraw transaction.
pub struct SiaWithdrawBuilder<'a> {
    coin: &'a SiaCoin,
    req: WithdrawRequest,
    from_address: Address,
    key_pair: &'a Keypair,
}

impl<'a> SiaWithdrawBuilder<'a> {
    /// Construct a builder by extracting the Iguana keypair and its
    /// derived address from `coin`.
    ///
    /// # Errors
    /// - [`WithdrawError::InternalError`] when the wallet is not in
    ///   `PrivKeyPolicy::KeyPair` mode (HD/Trezor are not supported).
    #[allow(clippy::result_large_err)]
    pub fn new(coin: &'a SiaCoin, req: WithdrawRequest) -> Result<Self, MmError<WithdrawError>> {
        let (key_pair, from_address) = match &*coin.priv_key_policy {
            PrivKeyPolicy::KeyPair(kp) => (kp, kp.public().address()),
            _ => {
                return Err(WithdrawError::InternalError(
                    "Only Iguana keypair is supported for Sia coin for now!".to_string(),
                )
                .into())
            },
        };

        Ok(SiaWithdrawBuilder {
            coin,
            req,
            from_address,
            key_pair,
        })
    }

    /// Largest-first coin-selection: keep adding sorted UTXOs until
    /// `target` is met.
    ///
    /// # Errors
    /// - [`WithdrawError::NotSufficientBalance`] when the *total* of the
    ///   provided outputs is below `target` (caller has already
    ///   converted both sides to hastings).
    #[allow(clippy::result_large_err)]
    fn pick_inputs_largest_first(
        &self,
        mut candidates: Vec<SiacoinElement>,
        target: u128,
    ) -> Result<Vec<SiacoinElement>, MmError<WithdrawError>> {
        candidates.sort_by(|a, b| b.siacoin_output.value.0.cmp(&a.siacoin_output.value.0));

        let mut chosen = Vec::new();
        let mut running_sum: u128 = 0;
        for output in candidates {
            running_sum = running_sum.saturating_add(*output.siacoin_output.value);
            chosen.push(output);
            if running_sum >= target {
                return Ok(chosen);
            }
        }

        Err(MmError::new(WithdrawError::NotSufficientBalance {
            coin: self.coin.ticker().to_string(),
            available: hastings_to_siacoin(running_sum.into()),
            required: hastings_to_siacoin(target.into()),
        }))
    }

    /// Decide which UTXOs to spend and how to split the proceeds
    /// between the recipient, the miner fee, and change.
    #[allow(clippy::result_large_err)]
    fn plan_inputs(&self, available: Vec<SiacoinElement>, fee: Currency) -> Result<InputPlan, MmError<WithdrawError>> {
        if self.req.max {
            // `max` mode: drain every available UTXO, leaving only the fee behind.
            let input_sum: Currency = available.iter().map(|o| o.siacoin_output.value).sum();
            if input_sum <= fee {
                return Err(MmError::new(WithdrawError::NotSufficientBalance {
                    coin: self.coin.ticker().to_string(),
                    available: hastings_to_siacoin(input_sum),
                    required: hastings_to_siacoin(fee),
                }));
            }
            return Ok(InputPlan {
                recipient_amount: input_sum - fee,
                inputs: available,
                change_amount: Currency::ZERO,
                input_sum,
            });
        }

        // Specific-amount mode: take only what's needed plus fee, return change.
        let recipient_amount = siacoin_to_hastings(self.req.amount.clone())
            .map_err(|e| WithdrawError::InternalError(e.to_string()))?;
        let target = recipient_amount + fee;
        let inputs = self.pick_inputs_largest_first(available, target.into())?;
        let input_sum: Currency = inputs.iter().map(|o| o.siacoin_output.value).sum();
        Ok(InputPlan {
            inputs,
            recipient_amount,
            change_amount: input_sum - target,
            input_sum,
        })
    }

    /// Fetch UTXOs, plan the spend, build, sign, and return the transaction.
    pub async fn build(self) -> WithdrawResult {
        let fee = Currency(TX_FEE_HASTINGS);
        let to = Address::from_str(&self.req.to).map_err(|e| WithdrawError::InvalidAddress(e.to_string()))?;

        let unspent = self
            .coin
            .client
            .get_unspent_outputs(&self.from_address, None, None, true)
            .await
            .map_err(|e| WithdrawError::Transport(e.to_string()))?;
        let basis = unspent.basis;

        let plan = self.plan_inputs(unspent.outputs, fee)?;

        let mut tx = V2TransactionBuilder::new()
            .update_basis(basis)
            .add_siacoin_output(SiacoinOutput {
                value: plan.recipient_amount,
                address: to.clone(),
            })
            .miner_fee(fee);
        for input in plan.inputs {
            tx = tx.add_siacoin_input(input, SpendPolicy::PublicKey(self.key_pair.public()));
        }
        if plan.change_amount > Currency::ZERO {
            tx = tx.add_siacoin_output(SiacoinOutput {
                value: plan.change_amount,
                address: self.from_address.clone(),
            });
        }
        let signed = tx.sign_simple(vec![self.key_pair]).build();

        // SC-denominated views, matching the RPC TransactionDetails contract.
        let spent = hastings_to_siacoin(plan.input_sum);
        let fee_sc = hastings_to_siacoin(fee);
        let received_back = hastings_to_siacoin(plan.change_amount);

        let tx_hex = serde_json::ser::to_vec(&signed).unwrap_or_default();
        let tx_hash = signed.txid().to_string();

        Ok(TransactionDetails {
            tx_hex: BytesJson(tx_hex),
            tx_hash,
            from: vec![self.from_address.to_string()],
            to: vec![self.req.to.clone()],
            total_amount: spent.clone() - fee_sc.clone(),
            spent_by_me: spent.clone(),
            received_by_me: received_back.clone(),
            my_balance_change: received_back - spent,
            fee_details: Some(
                SiaFeeDetails {
                    coin: self.coin.ticker().to_string(),
                    policy: SiaFeePolicy::Fixed,
                    total_amount: fee_sc,
                }
                .into(),
            ),
            block_height: 0,
            coin: self.coin.ticker().to_string(),
            internal_id: vec![].into(),
            timestamp: now_ms() / 1000,
            kmd_rewards: None,
            transaction_type: TransactionType::StandardTransfer,
        })
    }
}
