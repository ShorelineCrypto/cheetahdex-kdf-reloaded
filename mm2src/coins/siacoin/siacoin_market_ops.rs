// siacoin_market_ops — MarketCoinOps trait implementation.

use super::*;

impl MarketCoinOps for SiaCoin {
    fn ticker(&self) -> &str { &self.conf.ticker }

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

    fn sign_message_hash(&self, _message: &str) -> Option<[u8; 32]> { None }

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

    fn base_coin_balance(&self) -> BalanceFut<BigDecimal> { Box::new(self.my_balance().map(|res| res.spendable)) }

    fn platform_ticker(&self) -> &str { self.ticker() }

    /// `tx` is hex per the `MarketCoinOps::send_raw_tx` contract (see its doc
    /// comment in `lp_coins_traits.rs`) — the same convention `BytesJson`
    /// produces for the `tx_hex` field a withdraw preview returns, and what
    /// `send_raw_transaction`/`lp_coins_ops.rs` and every other coin's
    /// `send_raw_tx` already assume (e.g. `utxo_common_tx::send_raw_tx`
    /// hex-decodes first). A Sia transaction's actual serialization is JSON,
    /// not raw binary, so this decodes the hex to get that JSON back and
    /// hands off to `send_raw_tx_bytes`, which does the real parse-and-
    /// broadcast work directly on bytes. This used to skip the hex step and
    /// JSON-parse the still-hex-encoded string directly, which failed on the
    /// wallet's very first real character with a "trailing characters at
    /// line 1 column 2" error — column 2 because a leading hex digit like
    /// `7` or `0` is itself a complete one-character JSON number literal, so
    /// the parser reported the next hex digit as unexpected trailing input
    /// instead of reporting the real problem.
    fn send_raw_tx(&self, tx: &str) -> Box<dyn Future<Item = String, Error = String> + Send> {
        let bytes = try_fus!(hex::decode(tx).map_err(|e| e.to_string()));
        self.send_raw_tx_bytes(&bytes)
    }

    /// `tx` is the raw serialized transaction, matching every other coin's
    /// `send_raw_tx_bytes` — for Sia that serialization is JSON text as
    /// bytes, which is exactly what callers already hand it (`swap_watcher`,
    /// `lp_network`'s P2P relay) and exactly what `V2Transaction`'s own
    /// `tx_hex()` produces (`siacoin_types.rs`). Does the actual parse and
    /// broadcast; `send_raw_tx` above only exists to unwrap the hex layer
    /// external callers add on top of this.
    fn send_raw_tx_bytes(&self, tx: &[u8]) -> Box<dyn Future<Item = String, Error = String> + Send> {
        let client = self.client.clone();
        let transaction: V2Transaction = try_fus!(serde_json::from_slice(tx).map_err(|e| e.to_string()));

        let fut = async move {
            let txid = transaction.txid().to_string();
            client
                .broadcast_transaction(&transaction)
                .await
                .map_err(|e| e.to_string())?;
            Ok(txid)
        };
        Box::new(fut.boxed().compat())
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

    fn display_priv_key(&self) -> Result<String, String> { Err("SiaCoin::display_priv_key: Unsupported".to_string()) }

    fn min_tx_amount(&self) -> BigDecimal { hastings_to_siacoin(1u64.into()) }

    fn min_trading_vol(&self) -> MmNumber { hastings_to_siacoin(1u64.into()).into() }
}

#[cfg(test)]
mod send_raw_tx_tests {
    use super::*;

    /// `send_raw_tx` receives its input as hex — the `MarketCoinOps` contract
    /// (`lp_coins_traits.rs`) and what a withdraw preview's `BytesJson`-
    /// encoded `tx_hex` field actually produces — but a Sia transaction's own
    /// serialization is JSON, not raw binary. Confirms the hex layer
    /// round-trips: hex-encoding a transaction's JSON bytes and hex-decoding
    /// them back recovers bytes that parse into an equivalent transaction —
    /// the exact two steps `send_raw_tx` now performs before handing off to
    /// `send_raw_tx_bytes`.
    #[test]
    fn hex_encoded_json_tx_round_trips_to_the_same_transaction() {
        let tx = V2TransactionBuilder::new().build();
        let json_bytes = serde_json::to_vec(&tx).expect("V2Transaction always serializes");
        let hex_tx = hex::encode(&json_bytes);

        let decoded = hex::decode(&hex_tx).expect("send_raw_tx's own hex-decode step");
        let recovered: V2Transaction = serde_json::from_slice(&decoded).expect("send_raw_tx_bytes's own parse step");

        assert_eq!(recovered.txid().to_string(), tx.txid().to_string());
    }

    /// Regression for the reported bug: before the fix, `send_raw_tx` parsed
    /// its still-hex-encoded input as a generic `serde_json::Value` first
    /// (matching the old code's actual first step — not a direct typed
    /// parse, which fails with a different message). A hex string's leading
    /// digit is itself a complete one-character JSON number literal, so that
    /// generic parse always succeeded on one character and then failed on
    /// the next with exactly "trailing characters at line 1 column 2" — the
    /// error text from the bug report — without ever reaching the real
    /// transaction.
    #[test]
    fn json_parsing_the_still_hex_encoded_string_fails_the_way_the_bug_report_did() {
        let tx = V2TransactionBuilder::new().build();
        let hex_tx = hex::encode(serde_json::to_vec(&tx).unwrap());

        let err = serde_json::from_str::<serde_json::Value>(&hex_tx)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("trailing characters at line 1 column 2"),
            "expected the old bug's exact failure mode, got: {err}"
        );
    }
}
