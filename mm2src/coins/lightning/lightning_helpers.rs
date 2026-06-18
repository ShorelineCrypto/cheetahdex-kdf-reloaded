// lightning_helpers — internal inherent methods on LightningCoin
use super::*;

impl LightningCoin {
    pub(crate) fn platform_coin(&self) -> &UtxoStandardCoin { &self.platform.coin }

    #[inline]
    pub(crate) fn my_node_id(&self) -> String { self.channel_manager.get_our_node_id().to_string() }

    pub(crate) fn get_balance_msat(&self) -> (u64, u64) {
        self.channel_manager
            .list_channels()
            .iter()
            .fold((0, 0), |(spendable, unspendable), chan| {
                if chan.is_usable {
                    (
                        spendable + chan.outbound_capacity_msat,
                        unspendable + chan.balance_msat - chan.outbound_capacity_msat,
                    )
                } else {
                    (spendable, unspendable + chan.balance_msat)
                }
            })
    }

    pub(crate) fn pay_invoice(&self, invoice: Invoice) -> SendPaymentResult<PaymentInfo> {
        self.invoice_payer
            .pay_invoice(&invoice)
            .map_to_mm(|e| SendPaymentError::PaymentError(format!("{:?}", e)))?;
        let payment_hash = PaymentHash((*invoice.payment_hash()).into_inner());
        let payment_type = PaymentType::OutboundPayment {
            destination: *invoice.payee_pub_key().unwrap_or(&invoice.recover_payee_pub_key()),
        };
        let description = match invoice.description() {
            InvoiceDescription::Direct(d) => d.to_string(),
            InvoiceDescription::Hash(h) => hex::encode(h.0.into_inner()),
        };
        let payment_secret = Some(*invoice.payment_secret());
        Ok(PaymentInfo {
            payment_hash,
            payment_type,
            description,
            preimage: None,
            secret: payment_secret,
            amt_msat: invoice.amount_milli_satoshis(),
            fee_paid_msat: None,
            status: HTLCStatus::Pending,
            created_at: now_ms() / 1000,
            last_updated: now_ms() / 1000,
        })
    }

    pub(crate) fn keysend(
        &self,
        destination: PublicKey,
        amount_msat: u64,
        final_cltv_expiry_delta: u32,
    ) -> SendPaymentResult<PaymentInfo> {
        if final_cltv_expiry_delta < MIN_FINAL_CLTV_EXPIRY {
            return MmError::err(SendPaymentError::CLTVExpiryError(
                final_cltv_expiry_delta,
                MIN_FINAL_CLTV_EXPIRY,
            ));
        }
        let payment_preimage = PaymentPreimage(self.keys_manager.get_secure_random_bytes());
        self.invoice_payer
            .pay_pubkey(destination, payment_preimage, amount_msat, final_cltv_expiry_delta)
            .map_to_mm(|e| SendPaymentError::PaymentError(format!("{:?}", e)))?;
        let payment_hash = PaymentHash(Sha256::hash(&payment_preimage.0).into_inner());
        let payment_type = PaymentType::OutboundPayment { destination };

        Ok(PaymentInfo {
            payment_hash,
            payment_type,
            description: "".into(),
            preimage: Some(payment_preimage),
            secret: None,
            amt_msat: Some(amount_msat),
            fee_paid_msat: None,
            status: HTLCStatus::Pending,
            created_at: now_ms() / 1000,
            last_updated: now_ms() / 1000,
        })
    }

    pub(crate) async fn get_open_channels_by_filter(
        &self,
        filter: Option<OpenChannelsFilter>,
        paging: PagingOptionsEnum<u64>,
        limit: usize,
    ) -> ListChannelsResult<GetOpenChannelsResult> {
        let mut total_open_channels: Vec<ChannelDetailsForRPC> = self
            .channel_manager
            .list_channels()
            .into_iter()
            .map(From::from)
            .collect();

        total_open_channels.sort_by(|a, b| a.rpc_channel_id.cmp(&b.rpc_channel_id));

        let open_channels_filtered = if let Some(ref f) = filter {
            total_open_channels
                .into_iter()
                .filter(|chan| apply_open_channel_filter(chan, f))
                .collect()
        } else {
            total_open_channels
        };

        let offset = match paging {
            PagingOptionsEnum::PageNumber(page) => (page.get() - 1) * limit,
            PagingOptionsEnum::FromId(rpc_id) => open_channels_filtered
                .iter()
                .position(|x| x.rpc_channel_id == rpc_id)
                .map(|pos| pos + 1)
                .unwrap_or_default(),
        };

        let total = open_channels_filtered.len();

        let channels = if offset + limit <= total {
            open_channels_filtered[offset..offset + limit].to_vec()
        } else {
            open_channels_filtered[offset..].to_vec()
        };

        Ok(GetOpenChannelsResult {
            channels,
            skipped: offset,
            total,
        })
    }
}
