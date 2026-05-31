// solana_types — Constants, structs, enums, error types, and core types.

use super::*;

pub const SOLANA_DEFAULT_DECIMALS: u64 = 9;
pub const LAMPORTS_DUMMY_AMOUNT: u64 = 10;

#[async_trait]
pub trait SolanaCommonOps {
    fn rpc(&self) -> &SolanaRpcClient;

    fn is_token(&self) -> bool;

    async fn check_balance_and_prepare_transfer(
        &self,
        max: bool,
        amount: BigDecimal,
        fees: u64,
    ) -> Result<PrepareTransferData, MmError<SufficientBalanceError>>;
}

impl From<RpcError> for BalanceError {
    fn from(e: RpcError) -> Self {
        match e.kind {
            RpcErrorKind::Transport(s) => BalanceError::Transport(s),
            RpcErrorKind::Decode(s) => BalanceError::InvalidResponse(s),
            RpcErrorKind::Rpc(obj) => BalanceError::Transport(format!("server error {}: {}", obj.code, obj.message)),
        }
    }
}

impl From<ParsePubkeyError> for BalanceError {
    fn from(e: ParsePubkeyError) -> Self { BalanceError::Internal(format!("{:?}", e)) }
}

impl From<RpcError> for WithdrawError {
    fn from(e: RpcError) -> Self {
        match e.kind {
            RpcErrorKind::Transport(s) => WithdrawError::Transport(s),
            RpcErrorKind::Decode(s) => WithdrawError::InternalError(s),
            RpcErrorKind::Rpc(obj) => WithdrawError::Transport(format!("server error {}: {}", obj.code, obj.message)),
        }
    }
}

impl From<ParsePubkeyError> for WithdrawError {
    fn from(e: ParsePubkeyError) -> Self { WithdrawError::InvalidAddress(format!("{:?}", e)) }
}

impl From<ProgramError> for WithdrawError {
    fn from(e: ProgramError) -> Self { WithdrawError::InternalError(format!("{:?}", e)) }
}

#[derive(Debug)]
pub enum AccountError {
    NotFundedError(String),
    ParsePubKeyError(String),
    ClientError(RpcErrorKind),
}

impl From<RpcError> for AccountError {
    fn from(e: RpcError) -> Self { AccountError::ClientError(e.kind) }
}

impl From<ParsePubkeyError> for AccountError {
    fn from(e: ParsePubkeyError) -> Self { AccountError::ParsePubKeyError(format!("{:?}", e)) }
}

impl From<AccountError> for WithdrawError {
    fn from(e: AccountError) -> Self {
        match e {
            AccountError::NotFundedError(_) => WithdrawError::ZeroBalanceToWithdrawMax,
            AccountError::ParsePubKeyError(err) => WithdrawError::InternalError(err),
            AccountError::ClientError(e) => WithdrawError::Transport(format!("{:?}", e)),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SolanaActivationParams {
    confirmation_commitment: CommitmentLevel,
    client_url: String,
}

#[derive(Debug, Display)]
pub enum SolanaFromLegacyReqErr {
    InvalidCommitmentLevel(String),
    InvalidClientParsing(json::Error),
    ClientNoAvailableNodes(String),
}

#[derive(Debug, Display)]
pub enum KeyPairCreationError {
    #[display(fmt = "Signature error: {}", _0)]
    SignatureError(ed25519_dalek::SignatureError),
    #[display(fmt = "KeyPairFromSeed error: {}", _0)]
    KeyPairFromSeed(String),
}

impl From<ed25519_dalek::SignatureError> for KeyPairCreationError {
    fn from(e: ed25519_dalek::SignatureError) -> Self { KeyPairCreationError::SignatureError(e) }
}

fn generate_keypair_from_slice(priv_key: &[u8]) -> Result<Keypair, MmError<KeyPairCreationError>> {
    let secret_key = ed25519_dalek::SecretKey::from_bytes(priv_key)?;
    let public_key = ed25519_dalek::PublicKey::from(&secret_key);
    let key_pair = ed25519_dalek::Keypair {
        secret: secret_key,
        public: public_key,
    };
    solana_keypair::keypair_from_seed(key_pair.to_bytes().as_ref())
        .map_to_mm(|e| KeyPairCreationError::KeyPairFromSeed(e.to_string()))
}

pub async fn solana_coin_from_conf_and_params(
    ticker: &str,
    conf: &Json,
    params: SolanaActivationParams,
    priv_key: &[u8],
) -> Result<SolanaCoin, String> {
    let client = SolanaRpcClient::with_commitment(params.client_url.clone(), CommitmentConfig {
        commitment: params.confirmation_commitment,
    });
    let decimals = conf["decimals"].as_u64().unwrap_or(SOLANA_DEFAULT_DECIMALS) as u8;
    let key_pair = try_s!(generate_keypair_from_slice(priv_key));
    let my_address = key_pair.pubkey().to_string();
    let spl_tokens_infos = Arc::new(Mutex::new(HashMap::new()));
    let solana_coin = SolanaCoin(Arc::new(SolanaCoinImpl {
        my_address,
        key_pair,
        ticker: ticker.to_string(),
        client,
        decimals,
        spl_tokens_infos,
    }));
    Ok(solana_coin)
}

/// pImpl idiom.
pub struct SolanaCoinImpl {
    pub(crate) ticker: String,
    pub(crate) key_pair: Keypair,
    pub(crate) client: SolanaRpcClient,
    pub(crate) decimals: u8,
    pub(crate) my_address: String,
    pub(crate) spl_tokens_infos: Arc<Mutex<HashMap<String, SplTokenInfo>>>,
}

impl Debug for SolanaCoinImpl {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult { f.write_str(&*self.ticker) }
}

#[derive(Clone, Debug)]
pub struct SolanaCoin(pub(crate) Arc<SolanaCoinImpl>);
impl Deref for SolanaCoin {
    type Target = SolanaCoinImpl;
    fn deref(&self) -> &SolanaCoinImpl { &*self.0 }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SolanaFeeDetails {
    pub amount: BigDecimal,
}
