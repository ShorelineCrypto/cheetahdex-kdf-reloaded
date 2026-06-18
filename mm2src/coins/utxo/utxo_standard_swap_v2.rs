// ─────────────────────────────────────────────────────────────────────────────
//  V2 atomic-swap trait impls for UtxoStandardCoin (chapter 15).
//
//  This module is intentionally `include!`d into `utxo_standard.rs` so the
//  trait impls live on the same type without bloating the parent file.
//  Method bodies for the maker/taker V2 swap ops are stubbed with
//  `unimplemented!("ch15 phase 2: ...")` — script builders, dispatch
//  surface and trait wiring compile, but on-chain semantics are deferred.
// ─────────────────────────────────────────────────────────────────────────────

use crate::utxo::utxo_common;
use crate::utxo::utxo_standard::UtxoStandardCoin;
use crate::utxo::{UtxoCoinFields, UtxoTx};
use crate::{CommonSwapOpsV2, DerivationMethod, DexFee, FindPaymentSpendError, FundingTxSpend, GenPreimageResult,
            GenTakerFundingSpendArgs, GenTakerPaymentSpendArgs, MakerCoinSwapOpsV2, ParseCoinAssocTypes,
            PrivKeyPolicy, RefundFundingSecretArgs, RefundMakerPaymentSecretArgs, RefundMakerPaymentTimelockArgs,
            RefundTakerPaymentArgs, SearchForFundingSpendErr, SendMakerPaymentArgs, SendTakerFundingArgs,
            SpendMakerPaymentArgs, TakerCoinSwapOpsV2, ToBytes, TransactionErr, TxPreimageWithSig,
            ValidateMakerPaymentArgs, ValidateSwapV2TxResult, ValidateTakerFundingArgs,
            ValidateTakerFundingSpendPreimageResult, ValidateTakerPaymentSpendPreimageResult};
use async_trait::async_trait;
use keys::{Address, Error as KeysError, Public, Signature};
use mm2_err_handle::prelude::*;
use script::TransactionInputSigner;
use serialization::{deserialize, serialize, Error as SerError};
use std::str::FromStr;

/// Local newtype around `TransactionInputSigner` used as the V2 swap `Preimage`
/// associated type. Needed because the blanket `impl<T: AsRef<[u8]>> ToBytes for T`
/// makes a direct `impl ToBytes for TransactionInputSigner` impossible due to
/// coherence — wrapping in a local type lets us provide a concrete `ToBytes` impl
/// that serialises the underlying transaction.
#[derive(Clone, Debug)]
pub struct UtxoTxPreimage(pub TransactionInputSigner);

impl From<TransactionInputSigner> for UtxoTxPreimage {
    fn from(signer: TransactionInputSigner) -> Self { UtxoTxPreimage(signer) }
}

impl ToBytes for UtxoTxPreimage {
    fn to_bytes(&self) -> Vec<u8> {
        let tx: UtxoTx = self.0.clone().into();
        serialize(&tx).into()
    }
}

#[async_trait]
impl ParseCoinAssocTypes for UtxoStandardCoin {
    type Address = Address;
    type AddressParseError = KeysError;
    type Pubkey = Public;
    type PubkeyParseError = KeysError;
    type Tx = UtxoTx;
    type TxParseError = SerError;
    type Preimage = UtxoTxPreimage;
    type PreimageParseError = SerError;
    type Sig = Signature;
    type SigParseError = KeysError;

    async fn my_addr(&self) -> Self::Address {
        let fields: &UtxoCoinFields = self.as_ref();
        match fields.derivation_method {
            DerivationMethod::Iguana(ref addr) => addr.clone(),
            // HD-wallet V2 swaps require a deterministic per-swap HTLC address;
            // until that path is wired (ch15 phase 2) panic loudly so a misuse
            // surfaces in tests rather than silently signing the wrong tx.
            DerivationMethod::HDWallet(_) => {
                unimplemented!("ch15 phase 2: my_addr for UtxoStandardCoin in HD-wallet mode")
            },
        }
    }

    fn parse_address(&self, address: &str) -> Result<Self::Address, Self::AddressParseError> {
        Address::from_str(address).map_err(|_| KeysError::InvalidAddress)
    }

    fn parse_pubkey(&self, pubkey: &[u8]) -> Result<Self::Pubkey, Self::PubkeyParseError> { Public::from_slice(pubkey) }

    fn parse_tx(&self, tx: &[u8]) -> Result<Self::Tx, Self::TxParseError> { deserialize(tx) }

    fn parse_preimage(&self, preimage: &[u8]) -> Result<Self::Preimage, Self::PreimageParseError> {
        let tx: UtxoTx = deserialize(preimage)?;
        Ok(UtxoTxPreimage(tx.into()))
    }

    fn parse_signature(&self, sig: &[u8]) -> Result<Self::Sig, Self::SigParseError> {
        Ok(Signature::from(sig.to_vec()))
    }
}

#[async_trait]
impl CommonSwapOpsV2 for UtxoStandardCoin {
    fn derive_htlc_pubkey_v2(&self, _swap_unique_data: &[u8]) -> Public {
        // Mirror the V1 convention: if the priv-key policy yields a keypair,
        // use its public key. For Trezor/HD modes a per-swap derivation
        // (ch15 phase 2) will replace this stub.
        let fields: &UtxoCoinFields = self.as_ref();
        match fields.priv_key_policy {
            PrivKeyPolicy::KeyPair(ref kp) => *kp.public(),
            PrivKeyPolicy::HDWallet { ref activated_key, .. } => *activated_key.public(),
            PrivKeyPolicy::Trezor => {
                unimplemented!("ch15 phase 2: derive_htlc_pubkey_v2 under Trezor priv-key policy")
            },
        }
    }

    fn derive_htlc_pubkey_v2_bytes(&self, swap_unique_data: &[u8]) -> Vec<u8> {
        self.derive_htlc_pubkey_v2(swap_unique_data).to_vec()
    }
}

#[async_trait]
impl MakerCoinSwapOpsV2 for UtxoStandardCoin {
    async fn send_maker_payment_v2(&self, args: SendMakerPaymentArgs<'_, Self>) -> Result<UtxoTx, TransactionErr> {
        utxo_common::send_maker_payment_v2(self.clone(), args).await
    }

    async fn validate_maker_payment_v2(&self, args: ValidateMakerPaymentArgs<'_, Self>) -> ValidateSwapV2TxResult {
        utxo_common::validate_maker_payment_v2(self, args).await
    }

    async fn refund_maker_payment_v2_timelock(
        &self,
        args: RefundMakerPaymentTimelockArgs<'_>,
    ) -> Result<UtxoTx, TransactionErr> {
        utxo_common::refund_maker_payment_v2_timelock(self, args).await
    }

    async fn refund_maker_payment_v2_secret(
        &self,
        args: RefundMakerPaymentSecretArgs<'_, Self>,
    ) -> Result<UtxoTx, TransactionErr> {
        utxo_common::refund_maker_payment_v2_secret(self, args).await
    }

    async fn spend_maker_payment_v2(&self, args: SpendMakerPaymentArgs<'_, Self>) -> Result<UtxoTx, TransactionErr> {
        utxo_common::spend_maker_payment_v2(self, args).await
    }
}

#[async_trait]
impl TakerCoinSwapOpsV2 for UtxoStandardCoin {
    async fn send_taker_funding(&self, args: SendTakerFundingArgs<'_>) -> Result<UtxoTx, TransactionErr> {
        utxo_common::send_taker_funding(self.clone(), args).await
    }

    async fn validate_taker_funding(&self, args: ValidateTakerFundingArgs<'_, Self>) -> ValidateSwapV2TxResult {
        utxo_common::validate_taker_funding(self, args).await
    }

    async fn refund_taker_funding_timelock(&self, args: RefundTakerPaymentArgs<'_>) -> Result<UtxoTx, TransactionErr> {
        utxo_common::refund_taker_funding_timelock(self, args).await
    }

    async fn refund_taker_funding_secret(
        &self,
        args: RefundFundingSecretArgs<'_, Self>,
    ) -> Result<UtxoTx, TransactionErr> {
        utxo_common::refund_taker_funding_secret(self, args).await
    }

    async fn search_for_taker_funding_spend(
        &self,
        tx: &UtxoTx,
        from_block: u64,
        secret_hash: &[u8],
    ) -> Result<Option<FundingTxSpend<Self>>, SearchForFundingSpendErr> {
        utxo_common::search_for_taker_funding_spend(self, tx, from_block, secret_hash).await
    }

    async fn gen_taker_funding_spend_preimage(
        &self,
        args: &GenTakerFundingSpendArgs<'_, Self>,
        swap_unique_data: &[u8],
    ) -> GenPreimageResult<Self> {
        utxo_common::gen_taker_funding_spend_preimage(self, args, swap_unique_data).await
    }

    async fn validate_taker_funding_spend_preimage(
        &self,
        gen_args: &GenTakerFundingSpendArgs<'_, Self>,
        preimage: &TxPreimageWithSig<Self>,
    ) -> ValidateTakerFundingSpendPreimageResult {
        utxo_common::validate_taker_funding_spend_preimage(self, gen_args, preimage).await
    }

    async fn sign_and_send_taker_funding_spend(
        &self,
        preimage: &TxPreimageWithSig<Self>,
        args: &GenTakerFundingSpendArgs<'_, Self>,
        swap_unique_data: &[u8],
    ) -> Result<UtxoTx, TransactionErr> {
        utxo_common::sign_and_send_taker_funding_spend(self, preimage, args, swap_unique_data).await
    }

    async fn refund_combined_taker_payment(&self, args: RefundTakerPaymentArgs<'_>) -> Result<UtxoTx, TransactionErr> {
        utxo_common::refund_combined_taker_payment(self, args).await
    }

    async fn gen_taker_payment_spend_preimage(
        &self,
        args: &GenTakerPaymentSpendArgs<'_, Self>,
        swap_unique_data: &[u8],
    ) -> GenPreimageResult<Self> {
        utxo_common::gen_taker_payment_spend_preimage(self, args, swap_unique_data).await
    }

    async fn validate_taker_payment_spend_preimage(
        &self,
        gen_args: &GenTakerPaymentSpendArgs<'_, Self>,
        preimage: &TxPreimageWithSig<Self>,
    ) -> ValidateTakerPaymentSpendPreimageResult {
        utxo_common::validate_taker_payment_spend_preimage(self, gen_args, preimage).await
    }

    async fn sign_and_broadcast_taker_payment_spend(
        &self,
        preimage: Option<&TxPreimageWithSig<Self>>,
        gen_args: &GenTakerPaymentSpendArgs<'_, Self>,
        secret: &[u8],
        swap_unique_data: &[u8],
    ) -> Result<UtxoTx, TransactionErr> {
        utxo_common::sign_and_broadcast_taker_payment_spend(self, preimage, gen_args, secret, swap_unique_data).await
    }

    async fn find_taker_payment_spend_tx(
        &self,
        taker_payment: &UtxoTx,
        from_block: u64,
        wait_until: u64,
    ) -> MmResult<UtxoTx, FindPaymentSpendError> {
        utxo_common::find_taker_payment_spend_tx(self, taker_payment, from_block, wait_until).await
    }

    async fn extract_secret_v2(&self, secret_hash: &[u8], spend_tx: &UtxoTx) -> Result<[u8; 32], String> {
        utxo_common::extract_secret_v2(secret_hash, spend_tx)
    }
}

// Silence "unused import" — `utxo_common` is imported for the ch15-phase-2
// helpers that this module will eventually call into.
#[allow(dead_code)]
fn _ch15_imports_anchor() { let _ = utxo_common::DEFAULT_SWAP_VOUT; }
