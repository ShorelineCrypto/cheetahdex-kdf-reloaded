use super::*;

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum MmCoinEnum {
    UtxoCoin(UtxoStandardCoin),
    QtumCoin(QtumCoin),
    Qrc20Coin(Qrc20Coin),
    EthCoin(EthCoin),
    #[cfg(not(target_arch = "wasm32"))]
    ZCoin(ZCoin),
    Bch(BchCoin),
    SlpToken(SlpToken),
    #[cfg(not(target_arch = "wasm32"))]
    SolanaCoin(SolanaCoin),
    #[cfg(not(target_arch = "wasm32"))]
    SplToken(SplToken),
    #[cfg(not(target_arch = "wasm32"))]
    LightningCoin(LightningCoin),
    SiaCoin(siacoin::SiaCoin),
    TendermintCoin(tendermint::TendermintCoin),
    TendermintToken(tendermint::TendermintToken),
    Test(TestCoin),
}
impl From<UtxoStandardCoin> for MmCoinEnum {
    fn from(c: UtxoStandardCoin) -> MmCoinEnum { MmCoinEnum::UtxoCoin(c) }
}
impl From<EthCoin> for MmCoinEnum {
    fn from(c: EthCoin) -> MmCoinEnum { MmCoinEnum::EthCoin(c) }
}
impl From<TestCoin> for MmCoinEnum {
    fn from(c: TestCoin) -> MmCoinEnum { MmCoinEnum::Test(c) }
}
#[cfg(not(target_arch = "wasm32"))]
impl From<SolanaCoin> for MmCoinEnum {
    fn from(c: SolanaCoin) -> MmCoinEnum { MmCoinEnum::SolanaCoin(c) }
}
#[cfg(not(target_arch = "wasm32"))]
impl From<SplToken> for MmCoinEnum {
    fn from(c: SplToken) -> MmCoinEnum { MmCoinEnum::SplToken(c) }
}
impl From<QtumCoin> for MmCoinEnum {
    fn from(coin: QtumCoin) -> Self { MmCoinEnum::QtumCoin(coin) }
}
impl From<Qrc20Coin> for MmCoinEnum {
    fn from(c: Qrc20Coin) -> MmCoinEnum { MmCoinEnum::Qrc20Coin(c) }
}
impl From<BchCoin> for MmCoinEnum {
    fn from(c: BchCoin) -> MmCoinEnum { MmCoinEnum::Bch(c) }
}
impl From<SlpToken> for MmCoinEnum {
    fn from(c: SlpToken) -> MmCoinEnum { MmCoinEnum::SlpToken(c) }
}
#[cfg(not(target_arch = "wasm32"))]
impl From<LightningCoin> for MmCoinEnum {
    fn from(c: LightningCoin) -> MmCoinEnum { MmCoinEnum::LightningCoin(c) }
}
#[cfg(not(target_arch = "wasm32"))]
impl From<ZCoin> for MmCoinEnum {
    fn from(c: ZCoin) -> MmCoinEnum { MmCoinEnum::ZCoin(c) }
}
impl From<siacoin::SiaCoin> for MmCoinEnum {
    fn from(c: siacoin::SiaCoin) -> MmCoinEnum { MmCoinEnum::SiaCoin(c) }
}
impl From<tendermint::TendermintCoin> for MmCoinEnum {
    fn from(c: tendermint::TendermintCoin) -> MmCoinEnum { MmCoinEnum::TendermintCoin(c) }
}
impl From<tendermint::TendermintToken> for MmCoinEnum {
    fn from(c: tendermint::TendermintToken) -> MmCoinEnum { MmCoinEnum::TendermintToken(c) }
}
impl Deref for MmCoinEnum {
    type Target = dyn MmCoin;
    fn deref(&self) -> &dyn MmCoin {
        match self {
            MmCoinEnum::UtxoCoin(ref c) => c,
            MmCoinEnum::QtumCoin(ref c) => c,
            MmCoinEnum::Qrc20Coin(ref c) => c,
            MmCoinEnum::EthCoin(ref c) => c,
            MmCoinEnum::Bch(ref c) => c,
            MmCoinEnum::SlpToken(ref c) => c,
            #[cfg(not(target_arch = "wasm32"))]
            MmCoinEnum::LightningCoin(ref c) => c,
            #[cfg(not(target_arch = "wasm32"))]
            MmCoinEnum::ZCoin(ref c) => c,
            MmCoinEnum::Test(ref c) => c,
            #[cfg(not(target_arch = "wasm32"))]
            MmCoinEnum::SolanaCoin(ref c) => c,
            #[cfg(not(target_arch = "wasm32"))]
            MmCoinEnum::SplToken(ref c) => c,
            MmCoinEnum::SiaCoin(ref c) => c,
            MmCoinEnum::TendermintCoin(ref c) => c,
            MmCoinEnum::TendermintToken(ref c) => c,
        }
    }
}
impl MmCoinEnum {
    pub fn is_utxo_in_native_mode(&self) -> bool {
        match self {
            MmCoinEnum::UtxoCoin(ref c) => c.as_ref().rpc_client.is_native(),
            MmCoinEnum::QtumCoin(ref c) => c.as_ref().rpc_client.is_native(),
            MmCoinEnum::Qrc20Coin(ref c) => c.as_ref().rpc_client.is_native(),
            MmCoinEnum::Bch(ref c) => c.as_ref().rpc_client.is_native(),
            MmCoinEnum::SlpToken(ref c) => c.as_ref().rpc_client.is_native(),
            #[cfg(all(not(target_arch = "wasm32"), feature = "zhtlc"))]
            MmCoinEnum::ZCoin(ref c) => c.as_ref().rpc_client.is_native(),
            _ => false,
        }
    }
}
pub struct CoinsContext {
    /// A map from a currency ticker symbol to the corresponding coin.
    /// Similar to `LP_coins`.
    pub(crate) coins: AsyncMutex<HashMap<String, MmCoinEnum>>,
    pub(crate) balance_update_handlers: AsyncMutex<Vec<Box<dyn BalanceTradeFeeUpdatedHandler + Send + Sync>>>,
    pub(crate) withdraw_task_manager: WithdrawTaskManagerShared,
    pub(crate) create_account_manager: CreateAccountTaskManagerShared,
    pub(crate) scan_addresses_manager: ScanAddressesTaskManagerShared,
    pub(crate) account_balance_task_manager: AccountBalanceTaskManagerShared,
    #[cfg(target_arch = "wasm32")]
    pub(crate) tx_history_db: SharedDb<TxHistoryDb>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) hd_wallet_db: SharedDb<HDWalletDb>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) block_headers_storage_db: SharedDb<BlockHeaderStorageDb>,
}
#[derive(Debug)]
pub struct CoinIsAlreadyActivatedErr {
    pub ticker: String,
}
#[derive(Debug)]
pub struct PlatformIsAlreadyActivatedErr {
    pub ticker: String,
}
impl CoinsContext {
    /// Obtains a reference to this crate context, creating it if necessary.
    pub fn from_ctx(ctx: &MmArc) -> Result<Arc<CoinsContext>, String> {
        Ok(try_s!(from_ctx(&ctx.coins_ctx, move || {
            Ok(CoinsContext {
                coins: AsyncMutex::new(HashMap::new()),
                balance_update_handlers: AsyncMutex::new(vec![]),
                withdraw_task_manager: WithdrawTaskManager::new_shared(),
                create_account_manager: CreateAccountTaskManager::new_shared(),
                scan_addresses_manager: ScanAddressesTaskManager::new_shared(),
                account_balance_task_manager: AccountBalanceTaskManager::new_shared(),
                #[cfg(target_arch = "wasm32")]
                tx_history_db: ConstructibleDb::new_shared(ctx),
                #[cfg(target_arch = "wasm32")]
                hd_wallet_db: ConstructibleDb::new_shared(ctx),
                #[cfg(target_arch = "wasm32")]
                block_headers_storage_db: ConstructibleDb::new_shared(ctx),
            })
        })))
    }

    pub async fn add_coin(&self, coin: MmCoinEnum) -> Result<(), MmError<CoinIsAlreadyActivatedErr>> {
        let mut coins = self.coins.lock().await;
        if coins.contains_key(coin.ticker()) {
            return MmError::err(CoinIsAlreadyActivatedErr {
                ticker: coin.ticker().into(),
            });
        }

        coins.insert(coin.ticker().into(), coin);
        Ok(())
    }

    pub async fn add_platform_with_tokens(
        &self,
        platform: MmCoinEnum,
        tokens: Vec<MmCoinEnum>,
    ) -> Result<(), MmError<PlatformIsAlreadyActivatedErr>> {
        let mut coins = self.coins.lock().await;
        if coins.contains_key(platform.ticker()) {
            return MmError::err(PlatformIsAlreadyActivatedErr {
                ticker: platform.ticker().into(),
            });
        }

        coins.insert(platform.ticker().into(), platform);

        // Tokens can't be activated without platform coin so we can safely insert them without checking prior existence
        for token in tokens {
            coins.insert(token.ticker().into(), token);
        }
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn tx_history_db(&self) -> TxHistoryResult<TxHistoryDbLocked<'_>> {
        Ok(self.tx_history_db.get_or_initialize().await.map_mm_err()?)
    }
}
/// This enum is used in coin activation requests.
#[derive(Copy, Clone, Debug, Deserialize, Serialize)]
pub enum PrivKeyActivationPolicy {
    /// Use whatever key policy the CryptoCtx was initialized with (Iguana or GlobalHD).
    ContextPrivKey,
    /// Force legacy Iguana single-key mode.
    IguanaPrivKey,
    Trezor,
}
impl PrivKeyActivationPolicy {
    /// The function can be used as a default deserialization constructor:
    /// `#[serde(default = "PrivKeyActivationPolicy::context_priv_key")]`
    pub fn context_priv_key() -> PrivKeyActivationPolicy { PrivKeyActivationPolicy::ContextPrivKey }

    /// The function can be used as a default deserialization constructor:
    /// `#[serde(default = "PrivKeyActivationPolicy::iguana_priv_key")]`
    pub fn iguana_priv_key() -> PrivKeyActivationPolicy { PrivKeyActivationPolicy::IguanaPrivKey }

    /// The function can be used as a default deserialization constructor:
    /// `#[serde(default = "PrivKeyActivationPolicy::trezor")]`
    pub fn trezor() -> PrivKeyActivationPolicy { PrivKeyActivationPolicy::Trezor }
}
#[derive(Debug)]
pub enum PrivKeyPolicy<T> {
    /// Legacy single-key (Iguana passphrase hashed to one key pair).
    KeyPair(T),
    /// HD wallet mode: derived key at a specific BIP44 path, with the root extended key
    /// available for deriving additional addresses.
    HDWallet {
        /// The key pair derived at the user's chosen address path.
        activated_key: T,
        /// The BIP32 root extended private key for deriving additional coin keys.
        bip39_secp_priv_key: bip32::ExtendedPrivateKey<secp256k1::SecretKey>,
    },
    Trezor,
}
impl<T> PrivKeyPolicy<T> {
    pub fn key_pair(&self) -> Option<&T> {
        match self {
            PrivKeyPolicy::KeyPair(key_pair) => Some(key_pair),
            PrivKeyPolicy::HDWallet { activated_key, .. } => Some(activated_key),
            PrivKeyPolicy::Trezor => None,
        }
    }

    pub fn key_pair_or_err(&self) -> Result<&T, MmError<PrivKeyNotAllowed>> {
        self.key_pair()
            .or_mm_err(|| PrivKeyNotAllowed::HardwareWalletNotSupported)
    }

    /// Returns true if this is an HD wallet policy.
    pub fn is_hd_wallet(&self) -> bool { matches!(self, PrivKeyPolicy::HDWallet { .. }) }
}
#[derive(Clone)]
pub enum PrivKeyBuildPolicy {
    IguanaPrivKey(IguanaPrivKey),
    GlobalHDAccount(GlobalHDAccountArc),
    Trezor,
}
impl PrivKeyBuildPolicy {
    /// Detects the `PrivKeyBuildPolicy` from the `CryptoCtx` key pair policy.
    pub fn detect_priv_key_policy(ctx: &MmArc) -> MmResult<PrivKeyBuildPolicy, CryptoCtxError> {
        let crypto_ctx = CryptoCtx::from_ctx(ctx)?;
        match crypto_ctx.key_pair_policy() {
            KeyPairPolicy::Iguana => Ok(PrivKeyBuildPolicy::IguanaPrivKey(
                crypto_ctx.mm2_internal_privkey_secret(),
            )),
            KeyPairPolicy::GlobalHDAccount(global_hd) => Ok(PrivKeyBuildPolicy::GlobalHDAccount(global_hd.clone())),
        }
    }
}
#[derive(Debug)]
pub enum DerivationMethod<Address, HDWallet> {
    Iguana(Address),
    HDWallet(HDWallet),
}
impl<Address, HDWallet> DerivationMethod<Address, HDWallet> {
    pub fn iguana(&self) -> Option<&Address> {
        match self {
            DerivationMethod::Iguana(my_address) => Some(my_address),
            DerivationMethod::HDWallet(_) => None,
        }
    }

    pub fn iguana_or_err(&self) -> MmResult<&Address, UnexpectedDerivationMethod> {
        self.iguana()
            .or_mm_err(|| UnexpectedDerivationMethod::IguanaPrivKeyUnavailable)
    }

    pub fn hd_wallet(&self) -> Option<&HDWallet> {
        match self {
            DerivationMethod::Iguana(_) => None,
            DerivationMethod::HDWallet(hd_wallet) => Some(hd_wallet),
        }
    }

    pub fn hd_wallet_or_err(&self) -> MmResult<&HDWallet, UnexpectedDerivationMethod> {
        self.hd_wallet()
            .or_mm_err(|| UnexpectedDerivationMethod::HDWalletUnavailable)
    }

    /// # Panic
    ///
    /// Panic if the address mode is [`DerivationMethod::HDWallet`].
    pub fn unwrap_iguana(&self) -> &Address { self.iguana_or_err().unwrap() }
}
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "protocol_data")]
pub enum CoinProtocol {
    UTXO,
    QTUM,
    QRC20 {
        platform: String,
        contract_address: String,
    },
    ETH {
        /// Optional EVM chain id carried under `protocol_data` in newer coins
        /// configs. The coin builder reads the authoritative `chain_id` from the
        /// top-level coin config, so this field is accepted for
        /// forward-compatibility and is otherwise unused.
        #[serde(default)]
        chain_id: Option<u64>,
    },
    ERC20 {
        platform: String,
        contract_address: String,
    },
    SLPTOKEN {
        platform: String,
        token_id: H256Json,
        decimals: u8,
        required_confirmations: Option<u64>,
    },
    BCH {
        slp_prefix: String,
    },
    #[cfg(not(target_arch = "wasm32"))]
    LIGHTNING {
        platform: String,
        network: BlockchainNetwork,
        confirmations: PlatformCoinConfirmations,
    },
    #[cfg(not(target_arch = "wasm32"))]
    SOLANA,
    #[cfg(not(target_arch = "wasm32"))]
    SPLTOKEN {
        platform: String,
        token_contract_address: String,
        decimals: u8,
    },
    /// ZHTLC (Zcash-HTLC) shielded coin, e.g. ARRR/PIRATE and the ZOMBIE test coin.
    /// Production configs (ARRR) carry the Zcash consensus parameters, checkpoint
    /// block and z-derivation path under `protocol_data`; the bare test configs
    /// (ZOMBIE) omit it. Captured as an optional value so both shapes deserialize.
    ///
    /// NOTE: the ZCoin builder currently derives its consensus parameters from
    /// hardcoded Zcash-mainnet constants (which match ARRR mainnet); this payload
    /// is accepted for config compatibility but not yet consumed.
    #[cfg(not(target_arch = "wasm32"))]
    ZHTLC(Option<serde_json::Value>),
    SIA,
    TENDERMINT {
        account_prefix: String,
        chain_id: String,
    },
    TENDERMINTTOKEN {
        platform: String,
        denom: String,
        decimals: u8,
    },
    /// Native TRON coin (TRX). Carries the network identity so the
    /// activation layer can pick the right set of full-node URLs and
    /// chain parameters.
    TRX {
        #[serde(default)]
        network: crate::eth::tron::Network,
    },
    /// TRC20 token deployed on TRON. `platform` is the parent TRX coin
    /// ticker; `contract_address` is the on-chain TRC20 contract in
    /// hex (with `0x41` prefix) or Base58Check.
    TRC20 {
        platform: String,
        contract_address: String,
    },
}

impl CoinProtocol {
    /// Deserialize a coin's `protocol` config value, tolerating the standard
    /// `{"type": "ETH"}` form that omits `protocol_data`.
    ///
    /// `CoinProtocol` uses adjacent tagging (`content = "protocol_data"`), and the
    /// `ETH` variant is a struct variant (`chain_id`), so serde otherwise rejects a
    /// bare `{"type": "ETH"}` with `missing field protocol_data` even though its only
    /// field is optional. Backfill an empty `protocol_data` for exactly that case and
    /// defer to the derived deserialization (all other variants are untouched), so
    /// both `{"type": "ETH"}` and `{"type": "ETH", "protocol_data": {"chain_id": N}}`
    /// parse. This is the canonical way to parse a coin `protocol` from config.
    pub fn from_conf_json(mut protocol: serde_json::Value) -> serde_json::Result<CoinProtocol> {
        let is_eth = protocol.get("type").and_then(serde_json::Value::as_str) == Some("ETH");
        if is_eth && protocol.get("protocol_data").is_none() {
            if let Some(obj) = protocol.as_object_mut() {
                obj.insert(
                    "protocol_data".to_owned(),
                    serde_json::Value::Object(serde_json::Map::new()),
                );
            }
        }
        serde_json::from_value(protocol)
    }
}

pub enum RpcClientType {
    Native,
    Electrum,
    Ethereum,
}
impl ToString for RpcClientType {
    fn to_string(&self) -> String {
        match self {
            RpcClientType::Native => "native".into(),
            RpcClientType::Electrum => "electrum".into(),
            RpcClientType::Ethereum => "ethereum".into(),
        }
    }
}
#[derive(Clone)]
pub struct CoinTransportMetrics {
    /// Using a weak reference by default in order to avoid circular references and leaks.
    pub(crate) metrics: MetricsWeak,
    /// Name of coin the rpc client is intended to work with.
    pub(crate) ticker: String,
    /// RPC client type.
    pub(crate) client: String,
}
impl CoinTransportMetrics {
    pub(crate) fn new(metrics: MetricsWeak, ticker: String, client: RpcClientType) -> CoinTransportMetrics {
        CoinTransportMetrics {
            metrics,
            ticker,
            client: client.to_string(),
        }
    }

    pub(crate) fn into_shared(self) -> RpcTransportEventHandlerShared { Arc::new(self) }
}
impl RpcTransportEventHandler for CoinTransportMetrics {
    fn debug_info(&self) -> String { "CoinTransportMetrics".into() }

    fn on_outgoing_request(&self, data: &[u8]) {
        mm_counter!(self.metrics, "rpc_client.traffic.out", data.len() as u64,
            "coin" => self.ticker.clone(), "client" => self.client.clone());
        mm_counter!(self.metrics, "rpc_client.request.count", 1,
            "coin" => self.ticker.clone(), "client" => self.client.clone());
    }

    fn on_incoming_response(&self, data: &[u8]) {
        mm_counter!(self.metrics, "rpc_client.traffic.in", data.len() as u64,
            "coin" => self.ticker.clone(), "client" => self.client.clone());
        mm_counter!(self.metrics, "rpc_client.response.count", 1,
            "coin" => self.ticker.clone(), "client" => self.client.clone());
    }

    fn on_connected(&self, _address: String) -> Result<(), String> {
        // Handle a new connected endpoint if necessary.
        // Now just return the Ok
        Ok(())
    }
}
#[async_trait]
impl BalanceTradeFeeUpdatedHandler for CoinsContext {
    async fn balance_updated(&self, coin: &MmCoinEnum, new_balance: &BigDecimal) {
        for sub in self.balance_update_handlers.lock().await.iter() {
            sub.balance_updated(coin, new_balance).await
        }
    }
}

#[cfg(test)]
mod coin_protocol_tests {
    use super::CoinProtocol;
    use serde_json::json;

    #[test]
    fn eth_protocol_accepts_bare_and_explicit_protocol_data() {
        // The standard `{"type":"ETH"}` form (no protocol_data) must parse.
        match CoinProtocol::from_conf_json(json!({"type": "ETH"})).unwrap() {
            CoinProtocol::ETH { chain_id } => assert_eq!(chain_id, None),
            other => panic!("expected ETH, got {:?}", other),
        }
        // Explicit empty protocol_data.
        match CoinProtocol::from_conf_json(json!({"type": "ETH", "protocol_data": {}})).unwrap() {
            CoinProtocol::ETH { chain_id } => assert_eq!(chain_id, None),
            other => panic!("expected ETH, got {:?}", other),
        }
        // protocol_data carrying chain_id is preserved.
        match CoinProtocol::from_conf_json(json!({"type": "ETH", "protocol_data": {"chain_id": 137}})).unwrap() {
            CoinProtocol::ETH { chain_id } => assert_eq!(chain_id, Some(137)),
            other => panic!("expected ETH, got {:?}", other),
        }
    }

    #[test]
    fn other_variants_are_unaffected() {
        // A struct variant with required fields still needs its protocol_data.
        assert!(CoinProtocol::from_conf_json(json!({"type": "ERC20"})).is_err());
        CoinProtocol::from_conf_json(json!({
            "type": "ERC20",
            "protocol_data": {"platform": "ETH", "contract_address": "0x0"}
        }))
        .unwrap();
        // A unit variant still parses.
        assert!(matches!(
            CoinProtocol::from_conf_json(json!({"type": "UTXO"})).unwrap(),
            CoinProtocol::UTXO
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn zhtlc_accepts_bare_and_full_protocol_data() {
        // Bare ZHTLC (e.g. the ZOMBIE test coin) carries no protocol_data.
        match CoinProtocol::from_conf_json(json!({"type": "ZHTLC"})).unwrap() {
            CoinProtocol::ZHTLC(pd) => assert!(pd.is_none()),
            other => panic!("expected ZHTLC, got {:?}", other),
        }
        // Production ZHTLC (ARRR/PIRATE) ships consensus params, a checkpoint block
        // and the z-derivation path under protocol_data; all of it must be accepted.
        let arrr = json!({
            "type": "ZHTLC",
            "protocol_data": {
                "consensus_params": {
                    "overwinter_activation_height": 152855,
                    "sapling_activation_height": 152855,
                    "blossom_activation_height": null,
                    "heartwood_activation_height": null,
                    "canopy_activation_height": null,
                    "coin_type": 133,
                    "hrp_sapling_extended_spending_key": "secret-extended-key-main",
                    "hrp_sapling_extended_full_viewing_key": "zxviews",
                    "hrp_sapling_payment_address": "zs",
                    "b58_pubkey_address_prefix": [28, 184],
                    "b58_script_address_prefix": [28, 189]
                },
                "check_point_block": {
                    "height": 1900000,
                    "time": 1652512363,
                    "hash": "44797f3bb78323a7717007f1e289a3689e0b5b3433385dbd8e6f6a1700000000",
                    "sapling_tree": "01e40c26f4"
                },
                "z_derivation_path": "m/32'/141'"
            }
        });
        match CoinProtocol::from_conf_json(arrr).unwrap() {
            CoinProtocol::ZHTLC(pd) => assert!(pd.is_some()),
            other => panic!("expected ZHTLC, got {:?}", other),
        }
    }
}
