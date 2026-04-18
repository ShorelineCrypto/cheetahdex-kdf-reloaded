use crate::global_hd_ctx::{GlobalHDAccountArc, GlobalHDAccountCtx};
use crate::hw_client::{HwError, HwProcessingError, TrezorConnectProcessor};
use crate::hw_ctx::{HardwareWalletArc, HardwareWalletCtx};
use crate::privkey::{key_pair_from_seed, PrivKeyError};
use arrayref::array_ref;
use common::bits256;
use common::log::info;
use derive_more::Display;
use hw_common::primitives::EcdsaCurve;
use keys::{KeyPair, Public as PublicKey, Secret as Secp256k1Secret};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use parking_lot::RwLock;
use primitives::hash::H160;
use std::ops::Deref;
use std::sync::Arc;

/// The mm2 internal derivation path: `m/44'/141'/2147483647/0/0`
pub(crate) const MM2_INTERNAL_DERIVATION_PATH: &str = "m/44'/141'/2147483647/0/0";
/// The ECDSA curve used for the mm2 internal key pair.
pub(crate) const MM2_INTERNAL_ECDSA_CURVE: EcdsaCurve = EcdsaCurve::Secp256k1;

pub type CryptoInitResult<T> = Result<T, MmError<CryptoInitError>>;

#[derive(Debug, Display)]
pub enum CryptoInitError {
    NotInitialized,
    InitializedAlready,
    #[display(fmt = "Passphrase cannot be an empty string")]
    EmptyPassphrase,
    #[display(fmt = "Invalid passphrase: '{}'", _0)]
    InvalidPassphrase(PrivKeyError),
    Internal(String),
}

impl From<PrivKeyError> for CryptoInitError {
    fn from(e: PrivKeyError) -> Self {
        CryptoInitError::InvalidPassphrase(e)
    }
}

#[derive(Debug, Display)]
pub enum CryptoCtxError {
    #[display(fmt = "'CryptoCtx' is not initialized")]
    NotInitialized,
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

#[derive(Debug)]
pub enum HwCtxInitError<ProcessorError> {
    InitializingAlready,
    HwError(HwError),
    ProcessorError(ProcessorError),
}

impl<ProcessorError> From<HwProcessingError<ProcessorError>> for HwCtxInitError<ProcessorError> {
    fn from(e: HwProcessingError<ProcessorError>) -> Self {
        match e {
            HwProcessingError::HwError(hw_error) => HwCtxInitError::HwError(hw_error),
            HwProcessingError::ProcessorError(processor_error) => HwCtxInitError::ProcessorError(processor_error),
        }
    }
}

/// Determines whether the user initialized with a legacy Iguana passphrase
/// or a BIP39 mnemonic (HD wallet mode).
#[derive(Clone)]
pub enum KeyPairPolicy {
    /// Legacy mode: passphrase hashed to a single secp256k1 key pair.
    Iguana,
    /// HD wallet mode: BIP39 mnemonic with multi-coin derivation.
    GlobalHDAccount(GlobalHDAccountArc),
}

pub struct CryptoCtx {
    /// secp256k1 key pair derived from either:
    /// * Iguana passphrase (via `key_pair_from_seed`),
    /// * BIP39 passphrase at `mm2_internal_der_path` (via [`GlobalHDAccountCtx::new`]).
    secp256k1_key_pair: KeyPair,
    /// Whether we're in legacy Iguana mode or HD wallet mode.
    key_pair_policy: KeyPairPolicy,
    /// Can be initialized on [`CryptoCtx::init_hw_ctx_with_trezor`].
    hw_ctx: RwLock<HardwareWalletCtxState>,
}

impl CryptoCtx {
    pub fn from_ctx(ctx: &MmArc) -> MmResult<Arc<CryptoCtx>, CryptoCtxError> {
        let ctx_field = ctx
            .crypto_ctx
            .lock()
            .map_to_mm(|poison| CryptoCtxError::Internal(poison.to_string()))?;
        let ctx = match ctx_field.deref() {
            Some(ctx) => ctx,
            None => return MmError::err(CryptoCtxError::NotInitialized),
        };
        ctx.clone()
            .downcast()
            .map_err(|_| MmError::new(CryptoCtxError::Internal("Error casting the context field".to_owned())))
    }

    /// Returns the key pair policy (Iguana or GlobalHDAccount).
    #[inline]
    pub fn key_pair_policy(&self) -> &KeyPairPolicy {
        &self.key_pair_policy
    }

    /// This is our public ID, allowing us to be different from other peers.
    /// Also used for P2P message verification.
    #[inline]
    pub fn mm2_internal_public_id(&self) -> bits256 {
        // Compressed public key is 33 bytes; first byte is the prefix.
        let public = self.mm2_internal_pubkey();
        bits256 {
            bytes: *array_ref!(public, 1, 32),
        }
    }

    /// Returns `secp256k1` key-pair for mm2 internal purposes (P2P signing, etc.).
    ///
    /// # Security
    /// If `key_pair_policy` is `Iguana`, this key-pair is also used for coin activation.
    /// Use carefully — prefer matching on `key_pair_policy()` for coin operations.
    #[inline]
    pub fn mm2_internal_key_pair(&self) -> &KeyPair {
        &self.secp256k1_key_pair
    }

    /// Returns `secp256k1` public key for mm2 internal purposes.
    #[inline]
    pub fn mm2_internal_pubkey(&self) -> PublicKey {
        *self.secp256k1_key_pair.public()
    }

    /// Returns `secp256k1` public key as hex string.
    #[inline]
    pub fn mm2_internal_pubkey_hex(&self) -> String {
        hex::encode(&*self.mm2_internal_pubkey())
    }

    /// Returns `secp256k1` private key as `Secret` bytes.
    ///
    /// # Security
    /// If `key_pair_policy` is `Iguana`, this private key is used for coin activation.
    #[inline]
    pub fn mm2_internal_privkey_secret(&self) -> Secp256k1Secret {
        self.secp256k1_key_pair.private().secret
    }

    /// Returns `secp256k1` private key as a byte slice.
    #[inline]
    pub fn mm2_internal_privkey_slice(&self) -> &[u8] {
        self.secp256k1_key_pair.private().secret.as_slice()
    }

    #[inline]
    pub fn hw_ctx(&self) -> Option<HardwareWalletArc> {
        self.hw_ctx.read().to_option().cloned()
    }

    /// Returns an `RIPEMD160(SHA256(x))` where x is secp256k1 pubkey that identifies
    /// a Hardware Wallet device or an HD master private key.
    #[inline]
    pub fn hw_wallet_rmd160(&self) -> Option<H160> {
        self.hw_ctx.read().to_option().map(|hw_ctx| hw_ctx.rmd160())
    }

    /// Initialize with a legacy Iguana passphrase (hashed to a single key pair).
    pub fn init_with_iguana_passphrase(ctx: MmArc, passphrase: &str) -> CryptoInitResult<Arc<CryptoCtx>> {
        Self::init_crypto_ctx_with_policy_builder(ctx, passphrase, KeyPairPolicyBuilder::Iguana)
    }

    /// Initialize with a BIP39 mnemonic for HD wallet multi-coin derivation.
    pub fn init_with_global_hd_account(ctx: MmArc, passphrase: &str) -> CryptoInitResult<Arc<CryptoCtx>> {
        Self::init_crypto_ctx_with_policy_builder(ctx, passphrase, KeyPairPolicyBuilder::GlobalHDAccount)
    }

    pub async fn init_hw_ctx_with_trezor<Processor>(
        &self,
        processor: &Processor,
    ) -> MmResult<HardwareWalletArc, HwCtxInitError<Processor::Error>>
    where
        Processor: TrezorConnectProcessor + Sync,
    {
        {
            let mut state = self.hw_ctx.write();
            match state.deref() {
                HardwareWalletCtxState::NotInitialized => (),
                HardwareWalletCtxState::Initializing => return MmError::err(HwCtxInitError::InitializingAlready),
                HardwareWalletCtxState::Ready(_) => {
                    // Reset and re-initialize (GLEEC behavior: allows re-init)
                },
            }
            *state = HardwareWalletCtxState::Initializing;
        }

        let (res, new_state) = match HardwareWalletCtx::init_with_trezor(processor).await {
            Ok(hw_ctx) => (Ok(hw_ctx.clone()), HardwareWalletCtxState::Ready(hw_ctx)),
            Err(e) => (Err(e), HardwareWalletCtxState::NotInitialized),
        };

        *self.hw_ctx.write() = new_state;
        res.mm_err(HwCtxInitError::from)
    }

    /// Resets the hardware wallet context to uninitialized state.
    pub fn reset_hw_ctx(&self) {
        *self.hw_ctx.write() = HardwareWalletCtxState::NotInitialized;
    }

    /// Internal: builds the CryptoCtx using the chosen key pair policy.
    fn init_crypto_ctx_with_policy_builder(
        ctx: MmArc,
        passphrase: &str,
        policy_builder: KeyPairPolicyBuilder,
    ) -> CryptoInitResult<Arc<CryptoCtx>> {
        let mut ctx_field = ctx
            .crypto_ctx
            .lock()
            .map_to_mm(|poison| CryptoInitError::Internal(poison.to_string()))?;
        if ctx_field.is_some() {
            return MmError::err(CryptoInitError::InitializedAlready);
        }

        if passphrase.is_empty() {
            return MmError::err(CryptoInitError::EmptyPassphrase);
        }

        let (secp256k1_key_pair, key_pair_policy) = policy_builder.build(passphrase)?;
        let rmd160 = secp256k1_key_pair.public().address_hash();

        // We need a separate key pair for the legacy MmCtx fields.
        // TODO: Remove this when lp_swap and lp_ordermatch fully support CryptoCtx.
        let secp256k1_key_pair_for_legacy = key_pair_from_seed(passphrase).mm_err(Into::into)?;

        let crypto_ctx = CryptoCtx {
            secp256k1_key_pair,
            key_pair_policy,
            hw_ctx: RwLock::new(HardwareWalletCtxState::NotInitialized),
        };
        let result = Arc::new(crypto_ctx);
        *ctx_field = Some(result.clone());
        drop(ctx_field);

        // Initialize legacy MmCtx fields for backward compatibility.
        ctx.secp256k1_key_pair
            .pin(secp256k1_key_pair_for_legacy)
            .map_to_mm(CryptoInitError::Internal)?;
        ctx.rmd160.pin(rmd160).map_to_mm(CryptoInitError::Internal)?;

        info!("Public key hash: {rmd160}");
        Ok(result)
    }
}

/// Internal builder that constructs the appropriate KeyPairPolicy from a passphrase.
enum KeyPairPolicyBuilder {
    Iguana,
    GlobalHDAccount,
}

impl KeyPairPolicyBuilder {
    /// Builds a key pair and policy from the passphrase.
    /// For Iguana: hash the passphrase to a single secp256k1 key.
    /// For GlobalHDAccount: parse as BIP39 mnemonic, derive master keys.
    fn build(self, passphrase: &str) -> CryptoInitResult<(KeyPair, KeyPairPolicy)> {
        match self {
            KeyPairPolicyBuilder::Iguana => {
                let secp256k1_key_pair = key_pair_from_seed(passphrase).mm_err(Into::into)?;
                Ok((secp256k1_key_pair, KeyPairPolicy::Iguana))
            },
            KeyPairPolicyBuilder::GlobalHDAccount => {
                let (mm2_internal_key_pair, global_hd_ctx) = GlobalHDAccountCtx::new(passphrase).mm_err(Into::into)?;
                let key_pair_policy = KeyPairPolicy::GlobalHDAccount(global_hd_ctx.into_arc());
                Ok((mm2_internal_key_pair, key_pair_policy))
            },
        }
    }
}

enum HardwareWalletCtxState {
    NotInitialized,
    Initializing,
    Ready(HardwareWalletArc),
}

impl HardwareWalletCtxState {
    fn to_option(&self) -> Option<&HardwareWalletArc> {
        match self {
            HardwareWalletCtxState::Ready(hw_ctx) => Some(hw_ctx),
            _ => None,
        }
    }
}
