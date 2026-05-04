// Siacoin module — adapted from upstream KDF for the reloaded-gplv2-base fork.
//
// Key adaptations from upstream:
// - Uses fork's older positional-parameter SwapOps trait (not struct-based args)
// - Uses `PrivKeyPolicy::KeyPair` (not `Iguana`)
// - No AbortableQueue/AbortableSystem (fork lacks this infrastructure)
// - TransactionDetails uses tx_hex/tx_hash (no TransactionData enum)
// - Fork's `extract_secret` returns `Vec<u8>` not `[u8; 32]`
// - Fork's `can_refund_htlc` returns `Box<dyn Future<...>>` not async

use super::{
    BalanceError, CoinBalance, CoinsContext, HistorySyncState, MarketCoinOps, MmCoin, RawTransactionError,
    RawTransactionFut, RawTransactionRequest, RawTransactionResult, SignRawTransactionRequest, SignatureError, SwapOps,
    TradeFee, TransactionDetails, TransactionEnum, TransactionErr, TransactionType, VerificationError,
};
use crate::siacoin::sia_withdraw::SiaWithdrawBuilder;
use crate::{
    BalanceFut, CanRefundHtlc, DexFee, FeeApproxStage, FoundSwapTxSpend, NegotiateSwapContractAddrErr,
    PrivKeyBuildPolicy, PrivKeyPolicy, RawTransactionRes, SignatureResult, TradePreimageFut, TradePreimageResult,
    TradePreimageValue, Transaction, TxFeeDetails, ValidateAddressResult, ValidateFeeArgs, ValidatePaymentInput,
    VerificationResult, WatcherOps, WithdrawFut, WithdrawRequest,
};
use async_trait::async_trait;
use bigdecimal::BigDecimal;
use bitcrypto::sha256;
use common::executor::Timer;
use common::log::{debug, info};
use common::mm_number::MmNumber;
use common::now_ms;
use common::DEX_FEE_PUBKEY_ED25519;
use derive_more::{Display, From, Into};
use ed25519_dalek_bip32::DerivationPath as DalekDerivationPath;
use futures::compat::Future01CompatExt;
use futures::{FutureExt, TryFutureExt};
use futures01::Future;
use hex;
use keys::KeyPair;
use mm2_core::mm_ctx::MmArc;
use num_traits::ToPrimitive;
use rpc::v1::types::Bytes as BytesJson;
use serde_json::Value as Json;
// expose all of sia-rust so mm2_main can use it via coins::siacoin::sia_rust
pub use sia_rust;
pub use sia_rust::transport::client::{ApiClient as SiaApiClient, ApiClientHelpers};
pub use sia_rust::transport::endpoints::{
    AddressesEventsRequest, ConsensusTipRequest, GetAddressUtxosRequest, GetEventRequest, TxpoolBroadcastRequest,
    TxpoolTransactionsRequest, TxpoolTransactionsResponse,
};
pub use sia_rust::types::{
    Address, Currency, Event, EventDataWrapper, EventPayout, EventType, Hash256, Hash256Error, Keypair as SiaKeypair,
    KeypairError, Preimage, PreimageError, PublicKey, PublicKeyError, SiacoinElement, SiacoinOutput, SiacoinOutputId,
    SpendPolicy, TransactionId, V1Transaction, V2Transaction,
};
pub use sia_rust::utils::{V2TransactionBuilder, V2TransactionBuilderError};
use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use mm2_err_handle::prelude::*;

pub mod error;
pub use error::SiaCoinNewError;
use error::*;

pub mod sia_hd_wallet;
mod sia_withdraw;

pub use sia_rust::transport::client::{error as client_error, Client as SiaClient};

pub type SiaCoin = SiaCoinGeneric<SiaClient>;
pub type SiaClientConf = <SiaClient as SiaApiClient>::Conf;

lazy_static! {
    pub static ref FEE_PUBLIC_KEY_BYTES: Vec<u8> =
        hex::decode(DEX_FEE_PUBKEY_ED25519).expect("DEX_FEE_PUBKEY_ED25519 is a valid hex string");
    pub static ref FEE_PUBLIC_KEY: PublicKey =
        PublicKey::from_bytes(&FEE_PUBLIC_KEY_BYTES).expect("DEX_FEE_PUBKEY_ED25519 is a valid PublicKey");
    pub static ref FEE_ADDR: Address = Address::from_public_key(&FEE_PUBLIC_KEY);
    pub static ref SINGLE_ADDRESS_MODE_PATH: DalekDerivationPath =
        DalekDerivationPath::from_str("m/44'/1991'/0'/0'/0'").expect("Valid single address mode path");
}

/// The index of the HTLC output in the transaction that locks the funds
const HTLC_VOUT_INDEX: u32 = 0;

#[derive(Clone)]
pub struct SiaCoinGeneric<T: SiaApiClient + ApiClientHelpers> {
    pub conf: SiaCoinConf,
    pub priv_key_policy: Arc<PrivKeyPolicy<SiaKeypair>>,
    pub client: Arc<T>,
    pub history_sync_state: Arc<Mutex<HistorySyncState>>,
    required_confirmations: Arc<AtomicU64>,
}

impl WatcherOps for SiaCoin {}

#[derive(Clone, Debug, Deserialize)]
pub struct SiaCoinConf {
    #[serde(rename = "coin")]
    pub ticker: String,
    pub required_confirmations: u64,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SiaCoinActivationRequest {
    #[serde(default)]
    pub tx_history: bool,
    pub required_confirmations: Option<u64>,
    pub gap_limit: Option<u32>,
    pub client_conf: SiaClientConf,
}

#[derive(Debug, Display)]
pub enum SiaCoinFromLegacyReqErr {
    InvalidRequiredConfs(serde_json::Error),
    InvalidGapLimit(serde_json::Error),
    InvalidClientConf(serde_json::Error),
}

impl SiaCoinActivationRequest {
    pub fn from_legacy_req(req: &Json) -> Result<Self, MmError<SiaCoinFromLegacyReqErr>> {
        let tx_history = req["tx_history"].as_bool().unwrap_or_default();
        let required_confirmations = serde_json::from_value(req["required_confirmations"].clone())
            .map_to_mm(SiaCoinFromLegacyReqErr::InvalidRequiredConfs)?;
        let gap_limit =
            serde_json::from_value(req["gap_limit"].clone()).map_to_mm(SiaCoinFromLegacyReqErr::InvalidGapLimit)?;
        let client_conf =
            serde_json::from_value(req["client_conf"].clone()).map_to_mm(SiaCoinFromLegacyReqErr::InvalidClientConf)?;

        Ok(SiaCoinActivationRequest {
            tx_history,
            required_confirmations,
            gap_limit,
            client_conf,
        })
    }
}

impl SiaCoin {
    pub async fn new(
        _ctx: &MmArc,
        json_conf: Json,
        request: &SiaCoinActivationRequest,
        priv_key_policy: PrivKeyBuildPolicy,
    ) -> Result<Self, MmError<SiaCoinNewError>> {
        let key_pair = match priv_key_policy {
            PrivKeyBuildPolicy::IguanaPrivKey(priv_key) => SiaKeypair::from_private_bytes(priv_key.as_slice())?,
            PrivKeyBuildPolicy::GlobalHDAccount(global_hd_account) => {
                let extended_key = global_hd_account
                    .derive_ed25519_signing_key(&SINGLE_ADDRESS_MODE_PATH)
                    .map_err(|e| e.into_inner())?;
                SiaKeypair::from_private_bytes(extended_key.signing_key.as_bytes())?
            },
            _ => return Err(SiaCoinNewError::UnsupportedPrivKeyPolicy.into()),
        };

        let conf: SiaCoinConf = serde_json::from_value(json_conf)?;

        Ok(SiaCoinBuilder::new(conf, key_pair, request).build().await?)
    }
}

pub struct SiaCoinBuilder<'a> {
    conf: SiaCoinConf,
    key_pair: SiaKeypair,
    request: &'a SiaCoinActivationRequest,
}

impl<'a> SiaCoinBuilder<'a> {
    pub fn new(conf: SiaCoinConf, key_pair: SiaKeypair, request: &'a SiaCoinActivationRequest) -> Self {
        SiaCoinBuilder {
            conf,
            key_pair,
            request,
        }
    }

    async fn build(self) -> Result<SiaCoin, SiaCoinBuilderError> {
        let history_sync_state = if self.request.tx_history {
            HistorySyncState::NotStarted
        } else {
            HistorySyncState::NotEnabled
        };

        let required_confirmations: AtomicU64 = self
            .request
            .required_confirmations
            .unwrap_or(self.conf.required_confirmations)
            .into();

        Ok(SiaCoin {
            conf: self.conf,
            client: Arc::new(
                SiaClient::new(self.request.client_conf.clone())
                    .await
                    .map_err(SiaCoinBuilderError::Client)?,
            ),
            priv_key_policy: PrivKeyPolicy::KeyPair(self.key_pair).into(),
            history_sync_state: Mutex::new(history_sync_state).into(),
            required_confirmations: required_confirmations.into(),
        })
    }
}

/// Convert hastings representation to "coin" amount
fn hastings_to_siacoin(hastings: Currency) -> BigDecimal {
    let hastings: u128 = hastings.into();
    let divisor: BigDecimal = "1000000000000000000000000".parse().expect("valid decimal");
    let hastings_bd: BigDecimal = hastings.to_string().parse().expect("u128 is valid BigDecimal");
    hastings_bd / divisor
}

/// Convert "coin" representation to hastings amount
fn siacoin_to_hastings(siacoin: BigDecimal) -> Result<Currency, SiacoinToHastingsError> {
    let multiplier: BigDecimal = "1000000000000000000000000".parse().expect("valid decimal");
    let hastings = siacoin.clone() * multiplier;
    // Parse as string to get u128
    let hastings_str = hastings.with_scale(0).to_string();
    hastings_str
        .parse::<u128>()
        .map_err(|_| SiacoinToHastingsError::BigDecimalToU128(siacoin))
        .map(Currency)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SiaCoinProtocolInfo;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum SiaFeePolicy {
    Fixed,
    HastingsPerByte(Currency),
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SiaFeeDetails {
    pub coin: String,
    pub policy: SiaFeePolicy,
    pub total_amount: BigDecimal,
}

// From impl for SiaFeeDetails -> TxFeeDetails is in lp_coins.rs

// ── MmCoin trait impl ────────────────────────────────────────────────

#[async_trait]
impl MmCoin for SiaCoin {
    fn is_asset_chain(&self) -> bool {
        false
    }

    fn withdraw(&self, req: WithdrawRequest) -> WithdrawFut {
        let coin = self.clone();
        let fut = async move {
            let builder = SiaWithdrawBuilder::new(&coin, req)?;
            builder.build().await
        };
        Box::new(fut.boxed().compat())
    }

    fn get_raw_transaction(&self, _req: RawTransactionRequest) -> RawTransactionFut {
        Box::new(futures01::future::err(MmError::new(
            RawTransactionError::NotImplemented {
                coin: self.ticker().to_string(),
            },
        )))
    }

    fn decimals(&self) -> u8 {
        24
    }

    fn convert_to_address(&self, from: &str, _to_address_format: Json) -> Result<String, String> {
        Ok(from.to_string())
    }

    fn validate_address(&self, address: &str) -> ValidateAddressResult {
        match Address::from_str(address) {
            Ok(_) => ValidateAddressResult {
                is_valid: true,
                reason: None,
            },
            Err(e) => ValidateAddressResult {
                is_valid: false,
                reason: Some(e.to_string()),
            },
        }
    }

    fn process_history_loop(&self, _ctx: MmArc) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        // tx history loop not yet implemented for Sia in this fork
        Box::new(futures01::future::ok(()))
    }

    fn history_sync_status(&self) -> HistorySyncState {
        self.history_sync_state.lock().unwrap().clone()
    }

    fn get_trade_fee(&self) -> Box<dyn Future<Item = TradeFee, Error = String> + Send> {
        Box::new(futures01::future::ok(TradeFee {
            coin: self.ticker().to_string(),
            amount: MmNumber::from("0.00001"),
            paid_from_trading_vol: false,
        }))
    }

    async fn get_sender_trade_fee(
        &self,
        _value: TradePreimageValue,
        _stage: FeeApproxStage,
    ) -> TradePreimageResult<TradeFee> {
        Ok(TradeFee {
            coin: self.ticker().to_string(),
            amount: MmNumber::from("0.00001"),
            paid_from_trading_vol: false,
        })
    }

    fn get_receiver_trade_fee(&self, _stage: FeeApproxStage) -> TradePreimageFut<TradeFee> {
        Box::new(futures01::future::ok(TradeFee {
            coin: self.ticker().to_string(),
            amount: MmNumber::from("0.00001"),
            paid_from_trading_vol: false,
        }))
    }

    async fn get_fee_to_send_taker_fee(
        &self,
        _dex_fee_amount: BigDecimal,
        _stage: FeeApproxStage,
    ) -> TradePreimageResult<TradeFee> {
        Ok(TradeFee {
            coin: self.ticker().to_string(),
            amount: MmNumber::from("0.00001"),
            paid_from_trading_vol: false,
        })
    }

    fn required_confirmations(&self) -> u64 {
        self.required_confirmations.load(AtomicOrdering::Relaxed)
    }

    fn requires_notarization(&self) -> bool {
        false
    }

    fn set_required_confirmations(&self, confirmations: u64) {
        self.required_confirmations
            .store(confirmations, AtomicOrdering::Relaxed);
    }

    fn set_requires_notarization(&self, _requires_nota: bool) {}

    fn swap_contract_address(&self) -> Option<BytesJson> {
        None
    }

    fn mature_confirmations(&self) -> Option<u32> {
        None
    }

    fn coin_protocol_info(&self) -> Vec<u8> {
        Vec::new()
    }

    fn is_coin_protocol_supported(&self, _info: &Option<Vec<u8>>) -> bool {
        true
    }
}

// ── MarketCoinOps trait impl ─────────────────────────────────────────

impl MarketCoinOps for SiaCoin {
    fn ticker(&self) -> &str {
        &self.conf.ticker
    }

    fn my_address(&self) -> Result<String, String> {
        let key_pair = match &*self.priv_key_policy {
            PrivKeyPolicy::KeyPair(key_pair) => key_pair,
            _ => return Err("SiaCoin::my_address: Unexpected Key Derivation Method.".to_string()),
        };
        let address = key_pair.public().address();
        Ok(address.to_string())
    }

    fn get_public_key(&self) -> Result<String, MmError<super::UnexpectedDerivationMethod>> {
        let public_key = match &*self.priv_key_policy {
            PrivKeyPolicy::KeyPair(key_pair) => key_pair.public(),
            _ => return MmError::err(super::UnexpectedDerivationMethod::IguanaPrivKeyUnavailable),
        };
        Ok(public_key.to_string())
    }

    fn sign_message_hash(&self, _message: &str) -> Option<[u8; 32]> {
        None
    }

    fn sign_message(&self, _message: &str) -> SignatureResult<String> {
        MmError::err(SignatureError::InternalError(
            "SiaCoin::sign_message: Unsupported".to_string(),
        ))
    }

    fn verify_message(&self, _signature: &str, _message: &str, _address: &str) -> VerificationResult<bool> {
        MmError::err(VerificationError::InternalError(
            "SiaCoin::verify_message: Unsupported".to_string(),
        ))
    }

    fn my_balance(&self) -> BalanceFut<CoinBalance> {
        let coin = self.clone();
        let fut = async move {
            let my_address = match &*coin.priv_key_policy {
                PrivKeyPolicy::KeyPair(key_pair) => key_pair.public().address(),
                _ => {
                    return MmError::err(BalanceError::UnexpectedDerivationMethod(
                        super::UnexpectedDerivationMethod::IguanaPrivKeyUnavailable,
                    ))
                },
            };
            let balance = coin
                .client
                .address_balance(my_address)
                .await
                .map_to_mm(|e| BalanceError::Transport(e.to_string()))?;
            Ok(CoinBalance {
                spendable: hastings_to_siacoin(balance.siacoins),
                unspendable: hastings_to_siacoin(balance.immature_siacoins),
            })
        };
        Box::new(fut.boxed().compat())
    }

    fn base_coin_balance(&self) -> BalanceFut<BigDecimal> {
        Box::new(self.my_balance().map(|res| res.spendable))
    }

    fn platform_ticker(&self) -> &str {
        self.ticker()
    }

    fn send_raw_tx(&self, tx: &str) -> Box<dyn Future<Item = String, Error = String> + Send> {
        let client = self.client.clone();
        let tx = tx.to_owned();

        let fut = async move {
            let tx: Json = serde_json::from_str(&tx).map_err(|e| e.to_string())?;
            let transaction = serde_json::from_str::<V2Transaction>(&tx.to_string()).map_err(|e| e.to_string())?;
            let txid = transaction.txid().to_string();

            client
                .broadcast_transaction(&transaction)
                .await
                .map_err(|e| e.to_string())?;
            Ok(txid)
        };
        Box::new(fut.boxed().compat())
    }

    fn send_raw_tx_bytes(&self, tx: &[u8]) -> Box<dyn Future<Item = String, Error = String> + Send> {
        let tx: V2Transaction = try_fus!(serde_json::from_slice(tx).map_err(|e| e.to_string()));
        let str_tx = try_fus!(serde_json::to_string(&tx).map_err(|e| e.to_string()));
        self.send_raw_tx(&str_tx)
    }

    fn wait_for_confirmations(
        &self,
        tx: &[u8],
        confirmations: u64,
        _requires_nota: bool,
        wait_until: u64,
        check_every: u64,
    ) -> Box<dyn Future<Item = (), Error = String> + Send> {
        let tx: SiaTransaction = try_fus!(serde_json::from_slice(tx)
            .map_err(|e| format!("siacoin wait_for_confirmations payment_tx deser failed: {}", e)));
        let txid = tx.txid();
        let client = self.client.clone();
        let tx_request = GetEventRequest { txid: txid.clone() };

        let fut = async move {
            loop {
                if now_ms() / 1000 > wait_until {
                    return ERR!(
                        "Waited too long until {} for payment {} to be received",
                        wait_until,
                        tx.txid()
                    );
                }

                match client.dispatcher(tx_request.clone()).await {
                    Ok(event) => {
                        if event.confirmations >= confirmations {
                            return Ok(());
                        }
                    },
                    Err(e) => info!("Waiting for confirmation of Sia txid {}: {}", txid, e),
                }

                Timer::sleep(check_every as f64).await;
            }
        };

        Box::new(fut.boxed().compat())
    }

    fn wait_for_tx_spend(
        &self,
        transaction: &[u8],
        wait_until: u64,
        _from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let tx_bytes = transaction.to_vec();
        let client = self.client.clone();

        let fut = async move {
            let tx = SiaTransaction::try_from(tx_bytes).map_err(|e| TransactionErr::Plain(e.to_string()))?;
            let htlc_lock_txid = tx.txid();
            let output_id = SiacoinOutputId::new(htlc_lock_txid.clone(), HTLC_VOUT_INDEX);
            let check_every = 10f64;

            loop {
                let found_in_mempool = client
                    .dispatcher(TxpoolTransactionsRequest)
                    .await
                    .unwrap_or_default()
                    .v2transactions
                    .into_iter()
                    .find(|tx| tx.siacoin_inputs.iter().any(|input| input.parent.id == output_id));

                if let Some(tx) = found_in_mempool {
                    return Ok(TransactionEnum::SiaTransaction(SiaTransaction(tx)));
                }

                let found_in_block = client.find_where_utxo_spent(&output_id).await;

                match found_in_block {
                    Ok(Some(tx)) => return Ok(TransactionEnum::SiaTransaction(SiaTransaction(tx))),
                    Err(e) => debug!("SiaCoin::wait_for_tx_spend: find_where_utxo_spent failed: {}", e),
                    _ => (),
                }

                if now_ms() / 1000 >= wait_until {
                    return Err(TransactionErr::Plain(format!(
                        "Timed out waiting for spend of txid:{} vout 0",
                        htlc_lock_txid
                    )));
                }

                Timer::sleep(check_every).await;
            }
        };

        Box::new(fut.boxed().compat())
    }

    fn tx_enum_from_bytes(&self, bytes: &[u8]) -> Result<TransactionEnum, String> {
        let tx: V2Transaction = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        Ok(TransactionEnum::SiaTransaction(SiaTransaction(tx)))
    }

    fn current_block(&self) -> Box<dyn Future<Item = u64, Error = String> + Send> {
        let client = self.client.clone();
        let height_fut = async move { client.current_height().await.map_err(|e| e.to_string()) }
            .boxed()
            .compat();
        Box::new(height_fut)
    }

    fn display_priv_key(&self) -> Result<String, String> {
        Err("SiaCoin::display_priv_key: Unsupported".to_string())
    }

    fn min_tx_amount(&self) -> BigDecimal {
        hastings_to_siacoin(1u64.into())
    }

    fn min_trading_vol(&self) -> MmNumber {
        hastings_to_siacoin(1u64.into()).into()
    }
}

// ── Internal helpers ─────────────────────────────────────────────────

impl SiaCoin {
    pub fn my_keypair(&self) -> Result<&SiaKeypair, SiaCoinMyKeypairError> {
        match &*self.priv_key_policy {
            PrivKeyPolicy::KeyPair(keypair) => Ok(keypair),
            _ => Err(SiaCoinMyKeypairError::PrivKeyPolicy),
        }
    }
}

// ── Properly-typed swap methods (called by trait impls) ──────────────

impl SiaCoin {
    async fn new_send_taker_fee(
        &self,
        dex_fee: &DexFee,
        uuid: &[u8],
        _fee_addr: &[u8],
    ) -> Result<TransactionEnum, SendTakerFeeError> {
        let uuid_type_check = Uuid::from_slice(uuid)?;

        match uuid_type_check.get_version_num() {
            4 => (),
            version => return Err(SendTakerFeeError::UuidVersion(version)),
        }

        let trade_fee_amount = match dex_fee {
            DexFee::Standard(mm_num) => siacoin_to_hastings(BigDecimal::from(mm_num.clone()))?,
            other => return Err(SendTakerFeeError::DexFeeVariant(other.to_string())),
        };

        let my_keypair = self.my_keypair()?;

        let tx = V2TransactionBuilder::new()
            .miner_fee(Currency::DEFAULT_FEE)
            .add_siacoin_output((FEE_ADDR.clone(), trade_fee_amount).into())
            .fund_tx_single_source(&self.client, &my_keypair.public())
            .await?
            .arbitrary_data(uuid.to_vec().into())
            .add_change_output(&my_keypair.public().address())
            .sign_simple(vec![my_keypair])
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_send_maker_payment(
        &self,
        time_lock: u32,
        _maker_pub: &[u8],
        taker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
    ) -> Result<TransactionEnum, SendMakerPaymentError> {
        let my_keypair = self.my_keypair()?;
        let maker_public_key = my_keypair.public();

        if taker_pub.len() != 33 {
            return Err(SendMakerPaymentError::InvalidTakerPublicKeyLength(taker_pub.to_vec()));
        }
        let taker_public_key = PublicKey::from_bytes(&taker_pub[..32])?;

        let secret_hash = Hash256::try_from(secret_hash)?;

        let htlc_spend_policy =
            SpendPolicy::atomic_swap(&taker_public_key, &maker_public_key, time_lock as u64, &secret_hash);

        let trade_amount = siacoin_to_hastings(amount)?;

        let tx = V2TransactionBuilder::new()
            .miner_fee(Currency::DEFAULT_FEE)
            .add_siacoin_output((htlc_spend_policy.address(), trade_amount).into())
            .fund_tx_single_source(&self.client, &my_keypair.public())
            .await?
            .add_change_output(&my_keypair.public().address())
            .sign_simple(vec![my_keypair])
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_send_taker_payment(
        &self,
        time_lock: u32,
        _taker_pub: &[u8],
        maker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
    ) -> Result<TransactionEnum, SendTakerPaymentError> {
        let my_keypair = self.my_keypair()?;
        let taker_public_key = my_keypair.public();

        if maker_pub.len() != 33 {
            return Err(SendTakerPaymentError::InvalidMakerPublicKeyLength(maker_pub.to_vec()));
        }
        let maker_public_key = PublicKey::from_bytes(&maker_pub[..32])?;

        let secret_hash = Hash256::try_from(secret_hash)?;

        let htlc_spend_policy =
            SpendPolicy::atomic_swap(&maker_public_key, &taker_public_key, time_lock as u64, &secret_hash);

        let trade_amount = siacoin_to_hastings(amount)?;

        let tx = V2TransactionBuilder::new()
            .miner_fee(Currency::DEFAULT_FEE)
            .add_siacoin_output((htlc_spend_policy.address(), trade_amount).into())
            .fund_tx_single_source(&self.client, &my_keypair.public())
            .await?
            .add_change_output(&my_keypair.public().address())
            .sign_simple(vec![my_keypair])
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_send_maker_spends_taker_payment(
        &self,
        taker_payment_tx: &[u8],
        time_lock: u32,
        taker_pub: &[u8],
        secret: &[u8],
        secret_hash: &[u8],
    ) -> Result<TransactionEnum, MakerSpendsTakerPaymentError> {
        let my_keypair = self.my_keypair()?;
        let maker_public_key = my_keypair.public();

        if taker_pub.len() != 33 {
            return Err(MakerSpendsTakerPaymentError::InvalidTakerPublicKeyLength(
                taker_pub.to_vec(),
            ));
        }
        let taker_public_key = PublicKey::from_bytes(&taker_pub[..32])?;

        let _taker_payment_tx = SiaTransaction::try_from(taker_payment_tx.to_vec())?;
        let taker_payment_txid = _taker_payment_tx.txid();

        let secret = Preimage::try_from(secret)?;
        let secret_hash = Hash256::try_from(secret_hash)?;

        let input_spend_policy =
            SpendPolicy::atomic_swap_success(&maker_public_key, &taker_public_key, time_lock as u64, &secret_hash);

        let htlc_utxo = self
            .client
            .utxo_from_txid(&taker_payment_txid, 0)
            .await
            .map_err(Box::new)?;

        let miner_fee = Currency::DEFAULT_FEE;
        let htlc_utxo_amount = htlc_utxo.output.siacoin_output.value;

        let tx = V2TransactionBuilder::new()
            .miner_fee(miner_fee)
            .add_siacoin_output((maker_public_key.address(), htlc_utxo_amount - miner_fee).into())
            .add_siacoin_input(htlc_utxo.output, input_spend_policy)
            .satisfy_atomic_swap_success(my_keypair, secret, 0u32)?
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_send_taker_spends_maker_payment(
        &self,
        maker_payment_tx: &[u8],
        time_lock: u32,
        maker_pub: &[u8],
        secret: &[u8],
        secret_hash: &[u8],
    ) -> Result<TransactionEnum, TakerSpendsMakerPaymentError> {
        let my_keypair = self.my_keypair()?;
        let taker_public_key = my_keypair.public();

        if maker_pub.len() != 33 {
            return Err(TakerSpendsMakerPaymentError::InvalidMakerPublicKeyLength(
                maker_pub.to_vec(),
            ));
        }
        let maker_public_key = PublicKey::from_bytes(&maker_pub[..32])?;

        let _maker_payment_tx = SiaTransaction::try_from(maker_payment_tx.to_vec())?;
        let maker_payment_txid = _maker_payment_tx.txid();

        let secret = Preimage::try_from(secret)?;
        let secret_hash = Hash256::try_from(secret_hash)?;

        let input_spend_policy =
            SpendPolicy::atomic_swap_success(&taker_public_key, &maker_public_key, time_lock as u64, &secret_hash);

        let htlc_utxo = self
            .client
            .utxo_from_txid(&maker_payment_txid, 0)
            .await
            .map_err(Box::new)?;

        let miner_fee = Currency::DEFAULT_FEE;
        let htlc_utxo_amount = htlc_utxo.output.siacoin_output.value;

        let tx = V2TransactionBuilder::new()
            .miner_fee(miner_fee)
            .add_siacoin_output((taker_public_key.address(), htlc_utxo_amount - miner_fee).into())
            .add_siacoin_input_with_basis(htlc_utxo, input_spend_policy)
            .satisfy_atomic_swap_success(my_keypair, secret, 0u32)?
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_validate_fee_impl(&self, args: ValidateFeeArgs<'_>) -> Result<(), ValidateFeeError> {
        let args = SiaValidateFeeArgs::try_from(args)?;

        let peer_tx = args.fee_tx.0.clone();
        let fee_txid = peer_tx.txid();

        let found_in_block = self.client.get_event(&fee_txid).await;

        let fee_tx = match found_in_block {
            Ok(event) => {
                let tx = match event.data {
                    EventDataWrapper::V2Transaction(tx) => tx,
                    _ => return Err(ValidateFeeError::EventVariant(event)),
                };

                let confirmed_at_height = event.index.height;
                if confirmed_at_height < args.min_block_number {
                    return Err(ValidateFeeError::MininumConfirmedHeight {
                        txid: tx.txid(),
                        min_block_number: args.min_block_number,
                    });
                }
                tx
            },
            Err(e) => {
                debug!(
                    "SiaCoin::new_validate_fee: fee_tx not found on chain {}, checking mempool",
                    e
                );
                match self.client.get_unconfirmed_transaction(&fee_txid).await? {
                    Some(tx) => {
                        let current_height = self.client.current_height().await?;
                        if current_height < args.min_block_number {
                            return Err(ValidateFeeError::MininumMempoolHeight {
                                txid: tx.txid(),
                                min_block_number: args.min_block_number,
                            });
                        }
                        tx
                    },
                    None => return Err(ValidateFeeError::TxNotFound(fee_txid.clone())),
                }
            },
        };

        if !fee_tx
            .siacoin_inputs
            .into_iter()
            .all(|input| input.satisfied_policy.policy.address() == args.taker_public_key.address())
        {
            return Err(ValidateFeeError::InputsOrigin(fee_txid.clone()));
        }

        match fee_tx.siacoin_outputs.len() {
            1 | 2 => (),
            outputs_length => {
                return Err(ValidateFeeError::VoutLength {
                    txid: fee_txid.clone(),
                    outputs_length,
                })
            },
        }

        if fee_tx.siacoin_outputs[0].address != *FEE_ADDR {
            return Err(ValidateFeeError::InvalidFeeAddress {
                txid: fee_txid.clone(),
                address: fee_tx.siacoin_outputs[0].address.clone(),
            });
        }

        if fee_tx.siacoin_outputs[0].value != args.dex_fee_amount {
            return Err(ValidateFeeError::InvalidFeeAmount {
                txid: fee_txid.clone(),
                expected: args.dex_fee_amount,
                actual: fee_tx.siacoin_outputs[0].value,
            });
        }

        let fee_tx_uuid = Uuid::from_slice(&fee_tx.arbitrary_data.0)?;
        if fee_tx_uuid != args.uuid {
            return Err(ValidateFeeError::InvalidUuid {
                txid: fee_txid.clone(),
                expected: args.uuid,
                actual: fee_tx_uuid,
            });
        }

        Ok(())
    }

    async fn send_refund_htlc(
        &self,
        payment_tx: &[u8],
        time_lock: u32,
        other_pubkey: &[u8],
        secret_hash: &[u8],
    ) -> Result<TransactionEnum, SendRefundHltcError> {
        let my_keypair = self.my_keypair()?;
        let refund_public_key = my_keypair.public();

        let sia_args = SiaRefundPaymentArgs::try_from_positional(payment_tx, time_lock, other_pubkey, secret_hash)?;

        let input_spend_policy = SpendPolicy::atomic_swap_refund(
            &sia_args.success_public_key,
            &refund_public_key,
            sia_args.time_lock,
            &sia_args.secret_hash,
        );

        let htlc_utxo = self
            .client
            .utxo_from_txid(&sia_args.payment_tx.txid(), 0)
            .await
            .map_err(Box::new)?;

        let miner_fee = Currency::DEFAULT_FEE;
        let htlc_utxo_amount = htlc_utxo.output.siacoin_output.value;

        let tx = V2TransactionBuilder::new()
            .miner_fee(miner_fee)
            .add_siacoin_output((my_keypair.public().address(), htlc_utxo_amount - miner_fee).into())
            .add_siacoin_input_with_basis(htlc_utxo, input_spend_policy)
            .satisfy_atomic_swap_refund(my_keypair, 0u32)?
            .build();

        self.client.broadcast_transaction(&tx).await?;

        Ok(TransactionEnum::SiaTransaction(tx.into()))
    }

    async fn new_check_if_my_payment_sent(
        &self,
        time_lock: u32,
        _my_pub: &[u8],
        other_pub: &[u8],
        secret_hash: &[u8],
        _search_from_block: u64,
        amount: BigDecimal,
    ) -> Result<Option<TransactionEnum>, SiaCheckIfMyPaymentSentError> {
        let sia_args = SiaCheckIfMyPaymentSentArgs::try_from_positional(time_lock, other_pub, secret_hash, amount)?;

        let my_keypair = self.my_keypair()?;
        let refund_public_key = my_keypair.public();

        let spend_policy = SpendPolicy::atomic_swap(
            &sia_args.success_public_key,
            &refund_public_key,
            sia_args.time_lock,
            &sia_args.secret_hash,
        );
        let htlc_address = spend_policy.address();

        let events_result = self.client.get_address_events(htlc_address).await;
        let events = match events_result {
            Ok(events) => events,
            Err(_) => return Ok(None),
        };

        let event = match events.len() {
            0 => return Ok(None),
            _ => events[0].clone(),
        };

        let tx = match event.data {
            EventDataWrapper::V2Transaction(tx) => tx,
            wrong_variant => return Err(SiaCheckIfMyPaymentSentError::EventVariant(wrong_variant)),
        };

        Ok(Some(SiaTransaction(tx).into()))
    }

    #[allow(clippy::result_large_err)]
    fn sia_extract_secret(
        &self,
        expected_hash_slice: &[u8],
        spend_tx: &[u8],
    ) -> Result<Vec<u8>, SiaCoinSiaExtractSecretError> {
        let tx = SiaTransaction::try_from(spend_tx)?;
        let expected_hash = Hash256::try_from(expected_hash_slice)?;

        let found_secret =
            tx.0.siacoin_inputs
                .iter()
                .flat_map(|input| input.satisfied_policy.preimages.iter())
                .find(|extracted_secret| {
                    let check_secret_hash = Hash256(sha256(&extracted_secret.0).take());
                    check_secret_hash == expected_hash
                });

        found_secret
            .map(|secret| secret.0.to_vec())
            .ok_or(SiaCoinSiaExtractSecretError::FailedToExtract { tx, expected_hash })
    }

    async fn sia_can_refund_htlc(&self, locktime: u64) -> Result<CanRefundHtlc, SiaCoinSiaCanRefundHtlcError> {
        let median_timestamp = self.client.get_median_timestamp().await?;

        if locktime < median_timestamp {
            return Ok(CanRefundHtlc::CanRefundNow);
        }
        Ok(CanRefundHtlc::HaveToWait(locktime - median_timestamp))
    }

    async fn validate_htlc_payment(&self, input: ValidatePaymentInput) -> Result<(), SiaValidateHtlcPaymentError> {
        let sia_args = SiaValidatePaymentInputArgs::try_from(input)?;

        let my_keypair = self.my_keypair()?;
        let success_public_key = my_keypair.public();
        let refund_public_key = sia_args.other_pub;

        let htlc_address = SpendPolicy::atomic_swap(
            &success_public_key,
            &refund_public_key,
            sia_args.time_lock,
            &sia_args.secret_hash,
        )
        .address();

        let expected_htlc_output = SiacoinOutput {
            value: sia_args.amount,
            address: htlc_address,
        };

        let htlc_output = match sia_args.payment_tx.0.siacoin_outputs.get(HTLC_VOUT_INDEX as usize) {
            Some(output) => output,
            None => {
                return Err(SiaValidateHtlcPaymentError::InvalidOutputLength {
                    expected: HTLC_VOUT_INDEX + 1,
                    actual: sia_args.payment_tx.0.siacoin_outputs.len() as u32,
                    txid: sia_args.payment_tx.0.txid(),
                })
            },
        };

        if *htlc_output != expected_htlc_output {
            return Err(SiaValidateHtlcPaymentError::InvalidOutput {
                expected: expected_htlc_output,
                actual: htlc_output.clone(),
                txid: sia_args.payment_tx.0.txid(),
            });
        }

        Ok(())
    }
}

// ── SwapOps trait impl ───────────────────────────────────────────────

#[async_trait]
impl SwapOps for SiaCoin {
    fn send_taker_fee(&self, dex_fee: &DexFee, fee_addr: &[u8], uuid: &[u8]) -> super::TransactionFut {
        let coin = self.clone();
        let dex_fee = dex_fee.clone();
        let fee_addr = fee_addr.to_vec();
        let uuid = uuid.to_vec();
        let fut = async move {
            coin.new_send_taker_fee(&dex_fee, &uuid, &fee_addr)
                .await
                .map_err(|e| TransactionErr::Plain(e.to_string()))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_maker_payment(
        &self,
        time_lock: u32,
        maker_pub: &[u8],
        taker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let maker_pub = maker_pub.to_vec();
        let taker_pub = taker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            coin.new_send_maker_payment(time_lock, &maker_pub, &taker_pub, &secret_hash, amount)
                .await
                .map_err(|e| TransactionErr::Plain(e.to_string()))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_payment(
        &self,
        time_lock: u32,
        taker_pub: &[u8],
        maker_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let taker_pub = taker_pub.to_vec();
        let maker_pub = maker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            coin.new_send_taker_payment(time_lock, &taker_pub, &maker_pub, &secret_hash, amount)
                .await
                .map_err(|e| TransactionErr::Plain(e.to_string()))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_maker_spends_taker_payment(
        &self,
        taker_payment_tx: &[u8],
        time_lock: u32,
        taker_pub: &[u8],
        secret: &[u8],
        _htlc_privkey: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let taker_payment_tx = taker_payment_tx.to_vec();
        let taker_pub = taker_pub.to_vec();
        let secret = secret.to_vec();
        // Use secret to derive secret_hash for the HTLC
        let secret_hash_bytes: Vec<u8> = sha256(&secret).take().to_vec();
        let fut = async move {
            coin.new_send_maker_spends_taker_payment(
                &taker_payment_tx,
                time_lock,
                &taker_pub,
                &secret,
                &secret_hash_bytes,
            )
            .await
            .map_err(|e| TransactionErr::Plain(e.to_string()))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_spends_maker_payment(
        &self,
        maker_payment_tx: &[u8],
        time_lock: u32,
        maker_pub: &[u8],
        secret: &[u8],
        _htlc_privkey: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let maker_payment_tx = maker_payment_tx.to_vec();
        let maker_pub = maker_pub.to_vec();
        let secret = secret.to_vec();
        let secret_hash_bytes: Vec<u8> = sha256(&secret).take().to_vec();
        let fut = async move {
            coin.new_send_taker_spends_maker_payment(
                &maker_payment_tx,
                time_lock,
                &maker_pub,
                &secret,
                &secret_hash_bytes,
            )
            .await
            .map_err(|e| TransactionErr::Plain(e.to_string()))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_taker_refunds_payment(
        &self,
        taker_payment_tx: &[u8],
        time_lock: u32,
        maker_pub: &[u8],
        secret_hash: &[u8],
        _htlc_privkey: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let taker_payment_tx = taker_payment_tx.to_vec();
        let maker_pub = maker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            coin.send_refund_htlc(&taker_payment_tx, time_lock, &maker_pub, &secret_hash)
                .await
                .map_err(|e| TransactionErr::Plain(format!("taker refund: {}", e)))
        };
        Box::new(fut.boxed().compat())
    }

    fn send_maker_refunds_payment(
        &self,
        maker_payment_tx: &[u8],
        time_lock: u32,
        taker_pub: &[u8],
        secret_hash: &[u8],
        _htlc_privkey: &[u8],
        _swap_contract_address: &Option<BytesJson>,
    ) -> super::TransactionFut {
        let coin = self.clone();
        let maker_payment_tx = maker_payment_tx.to_vec();
        let taker_pub = taker_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let fut = async move {
            coin.send_refund_htlc(&maker_payment_tx, time_lock, &taker_pub, &secret_hash)
                .await
                .map_err(|e| TransactionErr::Plain(format!("maker refund: {}", e)))
        };
        Box::new(fut.boxed().compat())
    }

    fn validate_fee(&self, args: ValidateFeeArgs<'_>) -> Box<dyn Future<Item = (), Error = String> + Send> {
        let coin = self.clone();
        // Extract everything we need from the borrowed args before moving into the future
        let fee_tx_bytes = args.fee_tx.tx_hex();
        let expected_sender = args.expected_sender.to_vec();
        let fee_addr = args.fee_addr.to_vec();
        let dex_fee = args.dex_fee.clone();
        let min_block_number = args.min_block_number;
        let uuid = args.uuid.to_vec();

        let fut = async move {
            // Re-parse the tx from bytes to reconstruct ValidateFeeArgs with proper lifetimes
            let tx_enum = coin
                .tx_enum_from_bytes(&fee_tx_bytes)
                .map_err(|e| format!("Failed to parse fee tx: {}", e))?;
            let validate_args = ValidateFeeArgs {
                fee_tx: &tx_enum,
                expected_sender: &expected_sender,
                fee_addr: &fee_addr,
                dex_fee: &dex_fee,
                min_block_number,
                uuid: &uuid,
            };
            coin.new_validate_fee_impl(validate_args)
                .await
                .map_err(|e| e.to_string())
        };
        Box::new(fut.boxed().compat())
    }

    fn validate_maker_payment(&self, input: ValidatePaymentInput) -> Box<dyn Future<Item = (), Error = String> + Send> {
        let coin = self.clone();
        let fut = async move { coin.validate_htlc_payment(input).await.map_err(|e| e.to_string()) };
        Box::new(fut.boxed().compat())
    }

    fn validate_taker_payment(&self, input: ValidatePaymentInput) -> Box<dyn Future<Item = (), Error = String> + Send> {
        let coin = self.clone();
        let fut = async move { coin.validate_htlc_payment(input).await.map_err(|e| e.to_string()) };
        Box::new(fut.boxed().compat())
    }

    fn check_if_my_payment_sent(
        &self,
        time_lock: u32,
        my_pub: &[u8],
        other_pub: &[u8],
        secret_hash: &[u8],
        search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Box<dyn Future<Item = Option<TransactionEnum>, Error = String> + Send> {
        let coin = self.clone();
        let my_pub = my_pub.to_vec();
        let other_pub = other_pub.to_vec();
        let secret_hash = secret_hash.to_vec();
        let amount = BigDecimal::from(0); // amount not used in payment sent check
        let fut = async move {
            coin.new_check_if_my_payment_sent(time_lock, &my_pub, &other_pub, &secret_hash, search_from_block, amount)
                .await
                .map_err(|e| e.to_string())
        };
        Box::new(fut.boxed().compat())
    }

    async fn search_for_swap_tx_spend_my(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        // Not yet implemented for Sia
        Ok(None)
    }

    async fn search_for_swap_tx_spend_other(
        &self,
        _time_lock: u32,
        _other_pub: &[u8],
        _secret_hash: &[u8],
        _tx: &[u8],
        _search_from_block: u64,
        _swap_contract_address: &Option<BytesJson>,
    ) -> Result<Option<FoundSwapTxSpend>, String> {
        // Not yet implemented for Sia
        Ok(None)
    }

    fn extract_secret(&self, secret_hash: &[u8], spend_tx: &[u8]) -> Result<Vec<u8>, String> {
        self.sia_extract_secret(secret_hash, spend_tx)
            .map_err(|e| e.to_string())
    }

    fn can_refund_htlc(&self, locktime: u64) -> Box<dyn Future<Item = CanRefundHtlc, Error = String> + Send + '_> {
        let fut = async move { self.sia_can_refund_htlc(locktime).await.map_err(|e| e.to_string()) };
        Box::new(fut.boxed().compat())
    }

    fn negotiate_swap_contract_addr(
        &self,
        _other_side_address: Option<&[u8]>,
    ) -> Result<Option<BytesJson>, MmError<NegotiateSwapContractAddrErr>> {
        Ok(None)
    }

    fn get_htlc_key_pair(&self) -> Option<KeyPair> {
        // Sia uses ed25519 keys, not secp256k1 KeyPair. Return None.
        None
    }
}

// ── SiaTransaction ───────────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, From, Into)]
#[serde(transparent)]
pub struct SiaTransaction(pub V2Transaction);

impl fmt::Display for SiaTransaction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match serde_json::to_string(self) {
            Ok(json) => write!(f, "{}", json),
            Err(err) => write!(f, "Failed to serialize SiaTransaction:{:?} to JSON: {}", self, err),
        }
    }
}

impl SiaTransaction {
    pub fn txid(&self) -> Hash256 {
        self.0.txid()
    }
}

impl TryFrom<SiaTransaction> for Vec<u8> {
    type Error = SiaTransactionError;

    fn try_from(tx: SiaTransaction) -> Result<Self, Self::Error> {
        serde_json::ser::to_vec(&tx).map_err(SiaTransactionError::ToVec)
    }
}

impl TryFrom<&[u8]> for SiaTransaction {
    type Error = SiaTransactionError;

    fn try_from(tx_slice: &[u8]) -> Result<Self, Self::Error> {
        serde_json::de::from_slice(tx_slice).map_err(SiaTransactionError::FromVec)
    }
}

impl TryFrom<Vec<u8>> for SiaTransaction {
    type Error = SiaTransactionError;

    fn try_from(tx: Vec<u8>) -> Result<Self, Self::Error> {
        serde_json::de::from_slice(&tx).map_err(SiaTransactionError::FromVec)
    }
}

impl Transaction for SiaTransaction {
    fn tx_hex(&self) -> Vec<u8> {
        serde_json::ser::to_vec(self).unwrap_or_default()
    }

    fn tx_hash(&self) -> BytesJson {
        BytesJson(self.txid().0.to_vec())
    }
}

// ── SiaTransactionTypes ──────────────────────────────────────────────

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub enum SiaTransactionTypes {
    V1Transaction(V1Transaction),
    V2Transaction(V2Transaction),
    EventPayout(EventPayout),
}

// ── Internal arg conversion types ────────────────────────────────────

/// Sia typed ValidateFeeArgs
#[derive(Clone, Debug)]
struct SiaValidateFeeArgs {
    fee_tx: SiaTransaction,
    taker_public_key: PublicKey,
    dex_fee_amount: Currency,
    min_block_number: u64,
    uuid: Uuid,
}

impl<'a> TryFrom<ValidateFeeArgs<'a>> for SiaValidateFeeArgs {
    type Error = SiaValidateFeeArgsError;

    fn try_from(args: ValidateFeeArgs<'a>) -> Result<Self, Self::Error> {
        let fee_tx = match args.fee_tx {
            TransactionEnum::SiaTransaction(tx) => tx.clone(),
            _ => return Err(SiaValidateFeeArgsError::TxEnumVariant),
        };

        if args.expected_sender.len() != 33 {
            return Err(SiaValidateFeeArgsError::InvalidTakerPublicKeyLength(
                args.expected_sender.to_vec(),
            ));
        }

        let expected_sender_public_key = PublicKey::from_bytes(&args.expected_sender[..32])?;

        let dex_fee_amount = match args.dex_fee {
            DexFee::Standard(mm_num) => siacoin_to_hastings(BigDecimal::from(mm_num.clone()))?,
            other => return Err(SiaValidateFeeArgsError::DexFeeVariant(other.to_string())),
        };

        let uuid = Uuid::from_slice(args.uuid)?;

        match uuid.get_version_num() {
            4 => (),
            version => return Err(SiaValidateFeeArgsError::UuidVersion(version)),
        }

        Ok(SiaValidateFeeArgs {
            fee_tx,
            taker_public_key: expected_sender_public_key,
            dex_fee_amount,
            min_block_number: args.min_block_number,
            uuid,
        })
    }
}

/// Sia typed RefundPaymentArgs (adapted for fork's positional params)
pub struct SiaRefundPaymentArgs {
    payment_tx: SiaTransaction,
    time_lock: u64,
    success_public_key: PublicKey,
    secret_hash: Hash256,
}

impl SiaRefundPaymentArgs {
    fn try_from_positional(
        payment_tx_bytes: &[u8],
        time_lock: u32,
        other_pubkey: &[u8],
        secret_hash: &[u8],
    ) -> Result<Self, SiaRefundPaymentArgsError> {
        let payment_tx = SiaTransaction::try_from(payment_tx_bytes.to_vec())?;

        if other_pubkey.len() != 33 {
            return Err(SiaRefundPaymentArgsError::InvalidOtherPublicKeyLength(
                other_pubkey.to_vec(),
            ));
        }
        let success_public_key = PublicKey::from_bytes(&other_pubkey[..32])?;

        let secret_hash = Hash256::try_from(secret_hash)?;

        Ok(SiaRefundPaymentArgs {
            payment_tx,
            time_lock: time_lock as u64,
            success_public_key,
            secret_hash,
        })
    }
}

/// Sia typed CheckIfMyPaymentSentArgs (adapted for positional params)
struct SiaCheckIfMyPaymentSentArgs {
    time_lock: u64,
    success_public_key: PublicKey,
    secret_hash: Hash256,
    #[allow(dead_code)]
    amount: Currency,
}

impl SiaCheckIfMyPaymentSentArgs {
    fn try_from_positional(
        time_lock: u32,
        other_pub: &[u8],
        secret_hash: &[u8],
        amount: BigDecimal,
    ) -> Result<Self, SiaCheckIfMyPaymentSentArgsError> {
        if other_pub.len() != 33 {
            return Err(SiaCheckIfMyPaymentSentArgsError::InvalidOtherPublicKeyLength(
                other_pub.to_vec(),
            ));
        }
        let success_public_key = PublicKey::from_bytes(&other_pub[..32])?;
        let secret_hash = Hash256::try_from(secret_hash)?;
        let amount = siacoin_to_hastings(amount)?;

        Ok(SiaCheckIfMyPaymentSentArgs {
            time_lock: time_lock as u64,
            success_public_key,
            secret_hash,
            amount,
        })
    }
}

/// Sia typed ValidatePaymentInput
#[derive(Clone, Debug)]
struct SiaValidatePaymentInputArgs {
    payment_tx: SiaTransaction,
    time_lock: u64,
    other_pub: PublicKey,
    secret_hash: Hash256,
    amount: Currency,
}

impl TryFrom<ValidatePaymentInput> for SiaValidatePaymentInputArgs {
    type Error = SiaValidatePaymentInputError;

    fn try_from(args: ValidatePaymentInput) -> Result<Self, Self::Error> {
        let payment_tx = SiaTransaction::try_from(args.payment_tx.to_vec())?;

        // The "other_pub" in ValidatePaymentInput is split into taker_pub and maker_pub
        // For Sia, we use taker_pub as the "other" party (the one who can reveal the secret)
        let other_pub_bytes = &args.taker_pub;
        if other_pub_bytes.len() != 33 {
            return Err(SiaValidatePaymentInputError::InvalidOtherPublicKeyLength(
                other_pub_bytes.clone(),
            ));
        }
        let other_pub = PublicKey::from_bytes(&other_pub_bytes[..32])?;

        let secret_hash = Hash256::try_from(args.secret_hash.as_slice())?;
        let amount = siacoin_to_hastings(args.amount)?;

        Ok(SiaValidatePaymentInputArgs {
            payment_tx,
            time_lock: args.time_lock as u64,
            other_pub,
            secret_hash,
            amount,
        })
    }
}

// ── Debug impl ───────────────────────────────────────────────────────

impl fmt::Debug for SiaCoinGeneric<SiaClient> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SiaCoin({})", self.conf.ticker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use std::str::FromStr;

    #[test]
    fn test_siacoin_from_hastings_u128_max() {
        let hastings = u128::MAX;
        let siacoin = hastings_to_siacoin(hastings.into());
        assert_eq!(
            siacoin,
            BigDecimal::from_str("340282366920938.463463374607431768211455").unwrap()
        );
    }

    #[test]
    fn test_siacoin_from_hastings_total_supply() {
        let hastings = 57769875000000000000000000000000000u128;
        let siacoin = hastings_to_siacoin(hastings.into());
        assert_eq!(siacoin, BigDecimal::from_str("57769875000").unwrap());
    }

    #[test]
    fn test_siacoin_to_hastings_supply() {
        let siacoin = BigDecimal::from_str("57769875000").unwrap();
        let hastings = siacoin_to_hastings(siacoin).unwrap();
        assert_eq!(hastings, Currency(57769875000000000000000000000000000));
    }

    #[test]
    fn test_sia_transaction_serde_roundtrip() {
        let j = json!(
            {"siacoinInputs":[{"parent":{"id":"0f088eddda5320f8453a55349063abe43ba5b282631d5d2b9e684548f083055a","stateElement":{"leafIndex":3,"merkleProof":["ff9ce7f558df52b35d40fda59a8ab6d5ffb3dfab029992d02d1e929e0a36b6eb","b37a5387883748f73c1475ca85c8f3200eef09126c44824d0f44574109dabedc","0f1d4ef5b1bf0e6eb45240e717a1d548326f8379878944c6536fc73989cf2e7a","f85d8f6578bc2db41e8a206a060a10386c18505b533f64a5ceff574d602b57bd","c21c0b980cd4184996558b49e8b901c70cadb17fd27c62f472a2707f0eb6b092","482e9402c0c5e43d599b9683ba25224f74499f89e9bc33249794ff5e9f55e337","a29aabab81d0cf20e1bd33bb3e1138f1628a1337eb0430e4de47cf695eb25897","aeb60668a7e0ee0232f81642626104a1d4eb9ce3d0d9cf81196de48112e3ea41"]},"siacoinOutput":{"value":"299999000000000000000000000000","address":"c34caa97740668de2bbdb7174572ed64c861342bf27e80313cbfa02e9251f52e30aad3892533"},"maturityHeight":11},"satisfiedPolicy":{"policy":{"type":"pk","policy":"ed25519:a729be53dae7b0ed812f2a123ce93556014bbad8516ba6b1b496a112b46bbd97"},"signatures":["160e79ac52e0eaab5e92bd1675604a94b56ec58fdd0be3f3a842a4ece07d794f7ee1e8cc8f29b596bf71b2dc594df53347b9a4bcbec46fe09244ce6d3f6a6708"]}}],"siacoinOutputs":[{"value":"50000000000000000000000","address":"71731d7efe821794742c72a8376f56355b3c8a1984b861ccd42eed77a779a26626ea26ced3e2"},{"value":"299998949990000000000000000000","address":"c34caa97740668de2bbdb7174572ed64c861342bf27e80313cbfa02e9251f52e30aad3892533"}],"minerFee":"10000000000000000000"}
        );
        let tx = serde_json::from_value::<V2Transaction>(j).unwrap();
        let sia_tx = SiaTransaction(tx);

        let vec = serde_json::ser::to_vec(&sia_tx).unwrap();
        let tx2: SiaTransaction = serde_json::from_slice(&vec).unwrap();

        assert_eq!(sia_tx, tx2);
    }

    #[test]
    fn test_sia_fee_pubkey_init() {
        let pubkey_bytes: Vec<u8> = hex::decode(DEX_FEE_PUBKEY_ED25519).unwrap();
        let pubkey = PublicKey::from_bytes(&FEE_PUBLIC_KEY_BYTES).unwrap();
        assert_eq!(pubkey_bytes, *FEE_PUBLIC_KEY_BYTES);
        assert_eq!(pubkey, *FEE_PUBLIC_KEY);
    }

    #[test]
    fn test_siacoin_from_hastings_coin() {
        let coin = hastings_to_siacoin(Currency::COIN);
        assert_eq!(coin, BigDecimal::from(1));
    }

    #[test]
    fn test_siacoin_from_hastings_zero() {
        let coin = hastings_to_siacoin(Currency::ZERO);
        assert_eq!(coin, BigDecimal::from(0));
    }

    #[test]
    fn test_siacoin_to_hastings_coin() {
        let coin = BigDecimal::from(1);
        let hastings = siacoin_to_hastings(coin).unwrap();
        assert_eq!(hastings, Currency::COIN);
    }

    #[test]
    fn test_siacoin_to_hastings_zero() {
        let coin = BigDecimal::from(0);
        let hastings = siacoin_to_hastings(coin).unwrap();
        assert_eq!(hastings, Currency::ZERO);
    }

    #[test]
    fn test_siacoin_to_hastings_one() {
        let coin = serde_json::from_str::<BigDecimal>("0.000000000000000000000001").unwrap();
        let hastings = siacoin_to_hastings(coin).unwrap();
        assert_eq!(hastings, Currency(1));
    }
}
