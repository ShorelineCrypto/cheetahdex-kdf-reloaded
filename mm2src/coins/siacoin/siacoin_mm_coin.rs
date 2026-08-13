// siacoin_mm_coin — MmCoin and WatcherOps trait implementations.

use super::*;

impl WatcherOps for SiaCoin {}

#[async_trait]
impl MmCoin for SiaCoin {
    fn is_asset_chain(&self) -> bool { false }

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

    fn decimals(&self) -> u8 { 24 }

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

    fn process_history_loop(&self, ctx: MmArc) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        // The tracking pass of CRD ch.53 §53.6: populates the coin-generic
        // runtime history store from the wallet address's walletd event log.
        let coin = self.clone();
        Box::new(
            async move {
                super::process_history_loop(coin, ctx).await;
                Ok(())
            }
            .boxed()
            .compat(),
        )
    }

    fn history_sync_status(&self) -> HistorySyncState { self.history_sync_state.lock().unwrap().clone() }

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

    fn required_confirmations(&self) -> u64 { self.required_confirmations.load(AtomicOrdering::Relaxed) }

    fn requires_notarization(&self) -> bool { false }

    fn set_required_confirmations(&self, confirmations: u64) {
        self.required_confirmations
            .store(confirmations, AtomicOrdering::Relaxed);
    }

    fn set_requires_notarization(&self, _requires_nota: bool) {}

    fn swap_contract_address(&self) -> Option<BytesJson> { None }

    fn mature_confirmations(&self) -> Option<u32> { None }

    fn coin_protocol_info(&self) -> Vec<u8> { Vec::new() }

    fn is_coin_protocol_supported(&self, _info: &Option<Vec<u8>>) -> bool { true }
}
