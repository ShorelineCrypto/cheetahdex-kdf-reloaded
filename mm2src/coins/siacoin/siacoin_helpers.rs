// siacoin_helpers — Constructor, builder, and internal utility methods.

use super::*;

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

        Ok(SiaCoinBuilder::new(conf, key_pair, request).build(_ctx).await?)
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

    async fn build(self, ctx: &MmArc) -> Result<SiaCoin, SiaCoinBuilderError> {
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

        // Resolve the DEX fee destination from the network configuration
        // (per-netid; see `mm2_net_config`).
        let net_cfg = mm2_net_config::net_config_or_panic(ctx.netid());
        let fee_pubkey_bytes = hex::decode(net_cfg.dex_fee_pubkey_ed25519())
            .map_err(|e| SiaCoinBuilderError::FeePubkeyHex(e.to_string()))?;
        let fee_public_key =
            PublicKey::from_bytes(&fee_pubkey_bytes).map_err(|e| SiaCoinBuilderError::FeePubkey(e.to_string()))?;
        let fee_address = Address::from_public_key(&fee_public_key);

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
            fee_address,
        })
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
