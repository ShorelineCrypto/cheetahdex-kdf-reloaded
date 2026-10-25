// siacoin_helpers — Constructor, builder, and internal utility methods.

use super::*;

impl SiaCoin {
    pub async fn new(
        _ctx: &MmArc,
        json_conf: Json,
        request: &SiaCoinActivationRequest,
        priv_key_policy: PrivKeyBuildPolicy,
    ) -> Result<Self, MmError<SiaCoinNewError>> {
        // `derivation_method` mirrors the priv-key-policy branch it's built alongside:
        // Iguana yields a fixed single address, GlobalHDAccount yields a real
        // `SiaHDWallet` (CRD ch.20 D1) rooted at `m/44'/1991'`. `key_pair` itself is
        // still always the single-key/single-address-mode key (`SINGLE_ADDRESS_MODE_PATH`,
        // identical to account 0/External/address 0 of the HD wallet -- proved by
        // `sia_hd_wallet.rs`'s own `account_0_external_0_matches_the_existing_single_address_mode_path`
        // test), because every existing signing call site (`my_keypair()`, swap ops,
        // withdraw) still only knows how to spend from one active key (ch.20 D1's own
        // "still open" note; the HD wallet built here backs read-only account/address
        // discovery via `CoinWithDerivationMethod`/`HDWalletBalanceOps`, not spending).
        let (key_pair, derivation_method) = match priv_key_policy {
            PrivKeyBuildPolicy::IguanaPrivKey(priv_key) => {
                let key_pair = SiaKeypair::from_private_bytes(priv_key.as_slice())?;
                let address = key_pair.public().address();
                (key_pair, DerivationMethod::Iguana(address))
            },
            PrivKeyBuildPolicy::GlobalHDAccount(global_hd_account) => {
                let extended_key = global_hd_account
                    .derive_ed25519_signing_key(&SINGLE_ADDRESS_MODE_PATH)
                    .map_err(|e| e.into_inner())?;
                let key_pair = SiaKeypair::from_private_bytes(extended_key.signing_key.as_bytes())?;
                let gap_limit = request.gap_limit.unwrap_or(DEFAULT_HD_GAP_LIMIT);
                let hd_wallet = SiaHDWallet::new(&global_hd_account, gap_limit).map_err(|e| e.into_inner())?;
                (key_pair, DerivationMethod::HDWallet(hd_wallet))
            },
            _ => return Err(SiaCoinNewError::UnsupportedPrivKeyPolicy.into()),
        };

        let conf: SiaCoinConf = serde_json::from_value(json_conf)?;

        Ok(SiaCoinBuilder::new(conf, key_pair, derivation_method, request)
            .build(_ctx)
            .await?)
    }
}

pub struct SiaCoinBuilder<'a> {
    conf: SiaCoinConf,
    key_pair: SiaKeypair,
    derivation_method: DerivationMethod<Address, SiaHDWallet>,
    request: &'a SiaCoinActivationRequest,
}

impl<'a> SiaCoinBuilder<'a> {
    pub fn new(
        conf: SiaCoinConf,
        key_pair: SiaKeypair,
        derivation_method: DerivationMethod<Address, SiaHDWallet>,
        request: &'a SiaCoinActivationRequest,
    ) -> Self {
        SiaCoinBuilder {
            conf,
            key_pair,
            derivation_method,
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
        let netid = ctx.netid();
        let net_cfg = mm2_net_config::net_config_for(netid).ok_or(SiaCoinBuilderError::UnsupportedNetId(netid))?;
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
            derivation_method: Arc::new(self.derivation_method),
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
