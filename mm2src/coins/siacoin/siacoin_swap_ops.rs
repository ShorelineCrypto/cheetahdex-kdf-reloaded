// siacoin_swap_ops — SwapOps trait implementation and internal swap helper methods.

use super::*;
use crate::{HtlcPubkeyError, SWAP_HTLC_PUBKEY_LEN};
use common::log::warn;
use keys::Public;

// ── Properly-typed swap methods (called by trait impls) ──────────────

impl SiaCoin {
    /// `utxo_from_txid` looks the payment's own event up by txid and then
    /// its address's unspent outputs (`ApiClientHelpers::utxo_from_txid`,
    /// sia-rust). A single unretried lookup here turned a very short
    /// walletd indexing lag into a hard swap failure: both
    /// `send_maker_spends_taker_payment` and `send_taker_spends_maker_payment`
    /// call this immediately after `wait_for_confirmations` has *just*
    /// polled the same event as sufficiently confirmed through the same
    /// endpoint (`GetEventRequest`) -- but `wait_for_confirmations` itself
    /// retries on any error, so a walletd blip there is invisible, while
    /// this spend-side lookup had no such tolerance and failed the whole
    /// swap outright (observed on a real testnet swap: a 404 "event not
    /// found" roughly 200ms after the wait step's last successful poll of
    /// the same txid). Retrying here brings this lookup's resilience in
    /// line with the wait step's.
    async fn utxo_from_txid_with_retry(
        &self,
        txid: &TransactionId,
        vout_index: u32,
    ) -> Result<sia_rust::types::UtxoWithBasis, Box<client_error::UtxoFromTxidError>> {
        const ATTEMPTS: u8 = 5;
        const RETRY_DELAY_SECONDS: f64 = 3.;
        let mut last_err = None;
        for attempt in 1..=ATTEMPTS {
            match self.client.utxo_from_txid(txid, vout_index).await {
                Ok(utxo) => return Ok(utxo),
                Err(e) => {
                    warn!(
                        "utxo_from_txid({}, {}) attempt {}/{} failed, retrying: {}",
                        txid, vout_index, attempt, ATTEMPTS, e
                    );
                    last_err = Some(e);
                    if attempt < ATTEMPTS {
                        Timer::sleep(RETRY_DELAY_SECONDS).await;
                    }
                },
            }
        }
        Err(Box::new(
            last_err.expect("loop runs at least once, so this is always Some"),
        ))
    }

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
            .add_siacoin_output((self.fee_address.clone(), trade_fee_amount).into())
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

        let htlc_utxo = self.utxo_from_txid_with_retry(&taker_payment_txid, 0).await?;

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

        let htlc_utxo = self.utxo_from_txid_with_retry(&maker_payment_txid, 0).await?;

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

        if fee_tx.siacoin_outputs[0].address != self.fee_address {
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

        let htlc_utxo = self.utxo_from_txid_with_retry(&sia_args.payment_tx.txid(), 0).await?;

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

    async fn validate_htlc_payment(
        &self,
        input: ValidatePaymentInput,
        payment_owner: ValidatingPaymentOwner,
    ) -> Result<(), SiaValidateHtlcPaymentError> {
        let sia_args = SiaValidatePaymentInputArgs::try_from_validate_payment_input(input, payment_owner)?;

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

// ── Ed25519 keys in the fixed-width swap field (CRD ch.51 R64) ───────

/// Width of the native ed25519 public key Sia signs with.
const ED25519_PUBKEY_LEN: usize = 32;

/// Place an ed25519 public key in the fixed-width swap field.
///
/// R64 dictates both halves of the convention: the 32 native bytes take the
/// field's *leading* positions and the final byte is zero. A leading pad or a
/// non-zero pad is not interoperable, so neither may be varied.
fn ed25519_pubkey_to_swap_field(pubkey: &PublicKey) -> [u8; SWAP_HTLC_PUBKEY_LEN] {
    let mut field = [0u8; SWAP_HTLC_PUBKEY_LEN];
    field[..ED25519_PUBKEY_LEN].copy_from_slice(&pubkey.to_bytes());
    field
}

/// Read an ed25519 public key back out of the fixed-width swap field.
///
/// R64's receive half: require exactly the bound width, then take the *leading*
/// 32 bytes as the native key. The final byte is ignored rather than checked —
/// the rule constrains what this node sends, not what it will accept.
fn ed25519_pubkey_from_swap_field(field: &[u8]) -> MmResult<PublicKey, HtlcPubkeyError> {
    if field.len() != SWAP_HTLC_PUBKEY_LEN {
        return MmError::err(HtlcPubkeyError::UnexpectedLength(field.len()));
    }
    PublicKey::from_bytes(&field[..ED25519_PUBKEY_LEN])
        .map_to_mm(|e| HtlcPubkeyError::NotOnCurve("ed25519", e.to_string()))
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
        // We are the taker here (validating the payment the maker sent us),
        // so the counterparty is the maker.
        let fut = async move {
            coin.validate_htlc_payment(input, ValidatingPaymentOwner::Maker)
                .await
                .map_err(|e| e.to_string())
        };
        Box::new(fut.boxed().compat())
    }

    fn validate_taker_payment(&self, input: ValidatePaymentInput) -> Box<dyn Future<Item = (), Error = String> + Send> {
        let coin = self.clone();
        // We are the maker here (validating the payment the taker sent us),
        // so the counterparty is the taker.
        let fut = async move {
            coin.validate_htlc_payment(input, ValidatingPaymentOwner::Taker)
                .await
                .map_err(|e| e.to_string())
        };
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
        // Every non-test caller of this trait method is `TakerSwap`/`MakerSwap`
        // `recover_funds`, and only there (grep confirms it -- neither is
        // reached from the normal swap FSM's own spend-detection/wait loop,
        // which uses `wait_for_confirmations`/`tx_details_from_event`
        // instead). `Ok(None)` here used to mean "not yet implemented" while
        // *claiming* "confirmed not spent" -- indistinguishable, from the
        // caller's side, from a real, checked answer. `recover_funds` treats
        // `Ok(None)` as license to fall through to a refund attempt (see
        // taker_swap.rs/maker_swap.rs), so a Sia swap whose payment actually
        // *was* already spent by the counterparty would silently attempt a
        // doomed refund instead of failing with a message that says why.
        // An explicit error routes through `try_s!` at the call site and
        // surfaces as a clean RPC error instead -- "fails gracefully" is the
        // honest behavior until this is genuinely implemented (needs a
        // live Sia swap to verify a real spend-vs-refund witness
        // disambiguation against, which this environment has no way to do).
        Err("search_for_swap_tx_spend_my is not yet implemented for Sia; \
             cannot determine whether this payment has already been spent, \
             so it is not safe to recover automatically"
            .to_owned())
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
        // See `search_for_swap_tx_spend_my` above -- same reasoning, same
        // fix, mirrored for the counterparty-payment side of `recover_funds`.
        Err("search_for_swap_tx_spend_other is not yet implemented for Sia; \
             cannot determine whether this payment has already been spent, \
             so it is not safe to recover automatically"
            .to_owned())
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

    /// Sia signs with ed25519, so the key it puts on the negotiation wire is
    /// its own 32-byte key padded into the 33-byte field by the convention of
    /// ch.51 R64 — never the node's secp256k1 key, which Sia cannot sign with,
    /// and which is why `node_secp_pubkey` is ignored here.
    fn derive_htlc_pubkey(&self, _node_secp_pubkey: &Public) -> MmResult<[u8; SWAP_HTLC_PUBKEY_LEN], HtlcPubkeyError> {
        let keypair = self
            .my_keypair()
            .map_to_mm(|e| HtlcPubkeyError::NotAvailable(e.to_string()))?;
        Ok(ed25519_pubkey_to_swap_field(&keypair.public()))
    }

    /// The counterparty's key for a Sia HTLC is an ed25519 key in the same
    /// 33-byte field (R63, R64): exactly the bound width, with the leading 32
    /// bytes a well-formed curve point. This is the check the Sia swap
    /// transactions would otherwise fail much later, mid-swap.
    fn validate_other_pubkey(&self, raw_pubkey: &[u8]) -> MmResult<(), HtlcPubkeyError> {
        ed25519_pubkey_from_swap_field(raw_pubkey).map(|_| ())
    }
}

#[cfg(test)]
mod swap_field_tests {
    use super::*;

    fn test_pubkey() -> PublicKey {
        SiaKeypair::from_private_bytes(&[1u8; 32])
            .expect("32 bytes is a valid ed25519 secret key")
            .public()
    }

    /// ch.51 R64 send half: native bytes in the leading positions, final byte
    /// zero. Both are dictated by the deployed format, so both are asserted.
    #[test]
    fn ed25519_key_occupies_the_field_by_a_trailing_zero_pad() {
        let pubkey = test_pubkey();
        let field = ed25519_pubkey_to_swap_field(&pubkey);

        assert_eq!(field.len(), SWAP_HTLC_PUBKEY_LEN);
        assert_eq!(&field[..ED25519_PUBKEY_LEN], pubkey.as_bytes());
        assert_eq!(field[SWAP_HTLC_PUBKEY_LEN - 1], 0);
    }

    /// R64 receive half: the same field validates, and the key recovered from
    /// it is the one that was sent.
    #[test]
    fn ed25519_swap_field_round_trips() {
        let pubkey = test_pubkey();
        let field = ed25519_pubkey_to_swap_field(&pubkey);

        assert_eq!(ed25519_pubkey_from_swap_field(&field).unwrap(), pubkey);
    }

    /// R63: exactly 33 bytes. A bare 32-byte ed25519 key is the honest short
    /// case and is still refused — the field width does not follow the curve.
    #[test]
    fn only_the_bound_width_is_accepted() {
        let pubkey = test_pubkey();

        for field in [pubkey.as_bytes(), &[0u8; SWAP_HTLC_PUBKEY_LEN + 1][..], &[][..]] {
            assert_eq!(
                ed25519_pubkey_from_swap_field(field).unwrap_err().into_inner(),
                HtlcPubkeyError::UnexpectedLength(field.len())
            );
        }
    }

    /// A field of the right width whose leading bytes are not a curve point is
    /// rejected, rather than carried into a swap transaction that cannot be
    /// satisfied.
    #[test]
    fn a_bound_width_field_that_is_not_a_curve_point_is_rejected() {
        // y = 2 has no matching x on the Edwards curve, so this encodes no point.
        let mut field = [0u8; SWAP_HTLC_PUBKEY_LEN];
        field[0] = 2;

        assert!(matches!(
            ed25519_pubkey_from_swap_field(&field).unwrap_err().into_inner(),
            HtlcPubkeyError::NotOnCurve("ed25519", _)
        ));
    }
}
