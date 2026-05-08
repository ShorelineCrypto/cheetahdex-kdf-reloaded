// lightning_mm_coin — MmCoin trait impl for LightningCoin
use super::*;

#[async_trait]
impl MmCoin for LightningCoin {
    fn is_asset_chain(&self) -> bool {
        false
    }

    fn get_raw_transaction(&self, req: RawTransactionRequest) -> RawTransactionFut {
        Box::new(self.platform_coin().get_raw_transaction(req))
    }

    fn withdraw(&self, _req: WithdrawRequest) -> WithdrawFut {
        let fut = async move {
            MmError::err(WithdrawError::InternalError(
                "withdraw method is not supported for lightning, please use generate_invoice method instead.".into(),
            ))
        };
        Box::new(fut.boxed().compat())
    }

    fn decimals(&self) -> u8 {
        self.conf.decimals
    }

    fn convert_to_address(&self, _from: &str, _to_address_format: Json) -> Result<String, String> {
        Err(MmError::new("Address conversion is not available for LightningCoin".to_string()).to_string())
    }

    fn validate_address(&self, address: &str) -> ValidateAddressResult {
        match PublicKey::from_str(address) {
            Ok(_) => ValidateAddressResult {
                is_valid: true,
                reason: None,
            },
            Err(e) => ValidateAddressResult {
                is_valid: false,
                reason: Some(format!("Error {} on parsing node public key", e)),
            },
        }
    }

    // Todo: Implement this when implementing payments history for lightning
    fn process_history_loop(&self, _ctx: MmArc) -> Box<dyn Future<Item = (), Error = ()> + Send> {
        unimplemented!()
    }

    // Todo: Implement this when implementing payments history for lightning
    fn history_sync_status(&self) -> HistorySyncState {
        unimplemented!()
    }

    // Todo: Implement this when implementing swaps for lightning as it's is used only for swaps
    fn get_trade_fee(&self) -> Box<dyn Future<Item = TradeFee, Error = String> + Send> {
        unimplemented!()
    }

    // Todo: Implement this when implementing swaps for lightning as it's is used only for swaps
    async fn get_sender_trade_fee(
        &self,
        _value: TradePreimageValue,
        _stage: FeeApproxStage,
    ) -> TradePreimageResult<TradeFee> {
        unimplemented!()
    }

    // Todo: Implement this when implementing swaps for lightning as it's is used only for swaps
    fn get_receiver_trade_fee(&self, _stage: FeeApproxStage) -> TradePreimageFut<TradeFee> {
        unimplemented!()
    }

    // Todo: Implement this when implementing swaps for lightning as it's is used only for swaps
    async fn get_fee_to_send_taker_fee(
        &self,
        _dex_fee_amount: BigDecimal,
        _stage: FeeApproxStage,
    ) -> TradePreimageResult<TradeFee> {
        unimplemented!()
    }

    // Lightning payments are either pending, successful or failed. Once a payment succeeds there is no need to for confirmations
    // unlike onchain transactions.
    fn required_confirmations(&self) -> u64 {
        0
    }

    fn requires_notarization(&self) -> bool {
        false
    }

    fn set_required_confirmations(&self, _confirmations: u64) {}

    fn set_requires_notarization(&self, _requires_nota: bool) {}

    fn swap_contract_address(&self) -> Option<BytesJson> {
        None
    }

    fn mature_confirmations(&self) -> Option<u32> {
        None
    }

    // Todo: Implement this when implementing order matching for lightning as it's is used only for order matching
    fn coin_protocol_info(&self) -> Vec<u8> {
        unimplemented!()
    }

    // Todo: Implement this when implementing order matching for lightning as it's is used only for order matching
    fn is_coin_protocol_supported(&self, _info: &Option<Vec<u8>>) -> bool {
        unimplemented!()
    }
}
