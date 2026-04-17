//! # `get_private_keys` RPC — export private keys for activated coins.
//!
//! Returns the private key, public key, and address for each requested coin.
//! Coins must be activated before calling this method.
//!
//! ## Security
//!
//! - Private keys are sensitive material; call only over localhost / trusted channels.
//! - Keys are serialized once for the response and not persisted or logged.

use common::HttpStatusCode;
use crypto::{CryptoCtx, CryptoCtxError};
use derive_more::Display;
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use ser_error_derive::SerializeErrorType;
use serde::{Deserialize, Serialize};

use crate::{lp_coinfind, MarketCoinOps};

// ── Request / Response types ────────────────────────────────────────────

#[derive(Deserialize)]
pub struct GetPrivateKeysRequest {
    /// Tickers of activated coins whose keys should be exported.
    pub coins: Vec<String>,
}

/// Per-coin key information returned to the caller.
#[derive(Serialize)]
pub struct CoinKeyInfo {
    pub coin: String,
    pub address: String,
    pub priv_key: String,
    /// Hex-encoded compressed public key.
    pub pubkey: String,
}

#[derive(Serialize)]
pub struct GetPrivateKeysResponse {
    pub keys: Vec<CoinKeyInfo>,
}

// ── Error type ──────────────────────────────────────────────────────────

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum GetPrivateKeysError {
    #[display(fmt = "Coin not activated: {}", _0)]
    CoinNotActive(String),
    #[display(fmt = "Key export failed for {}: {}", ticker, reason)]
    KeyExportFailed { ticker: String, reason: String },
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
    #[display(fmt = "Hardware wallets do not expose private keys")]
    HardwareWalletNotSupported,
}

impl HttpStatusCode for GetPrivateKeysError {
    fn status_code(&self) -> StatusCode {
        match self {
            GetPrivateKeysError::CoinNotActive(_) => StatusCode::BAD_REQUEST,
            GetPrivateKeysError::KeyExportFailed { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            GetPrivateKeysError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GetPrivateKeysError::HardwareWalletNotSupported => StatusCode::BAD_REQUEST,
        }
    }
}

impl From<CryptoCtxError> for GetPrivateKeysError {
    fn from(e: CryptoCtxError) -> Self {
        GetPrivateKeysError::Internal(e.to_string())
    }
}

// ── Handler ─────────────────────────────────────────────────────────────

/// Export private keys for the requested activated coins.
///
/// For Iguana wallets, each coin returns a single private key (the passphrase-derived key
/// in the coin's native format — WIF for UTXO, hex for EVM, etc.).
///
/// Hardware wallets are rejected — they never expose private keys to the host.
pub async fn get_private_keys(
    ctx: MmArc,
    req: GetPrivateKeysRequest,
) -> Result<GetPrivateKeysResponse, MmError<GetPrivateKeysError>> {
    // Refuse if this is a Trezor/HW session — hardware wallets never expose keys to the host.
    let crypto_ctx = CryptoCtx::from_ctx(&ctx).mm_err(GetPrivateKeysError::from)?;
    if crypto_ctx.hw_ctx().is_some() {
        return MmError::err(GetPrivateKeysError::HardwareWalletNotSupported);
    }

    let mut keys = Vec::with_capacity(req.coins.len());

    for ticker in &req.coins {
        let coin = match lp_coinfind(&ctx, ticker).await {
            Ok(Some(c)) => c,
            Ok(None) => return MmError::err(GetPrivateKeysError::CoinNotActive(ticker.clone())),
            Err(e) => {
                return MmError::err(GetPrivateKeysError::Internal(format!(
                    "Error looking up {}: {}",
                    ticker, e
                )))
            },
        };

        // Derive the private key string in the coin's native format.
        let priv_key = coin
            .display_priv_key()
            .map_err(|e| GetPrivateKeysError::KeyExportFailed {
                ticker: ticker.clone(),
                reason: e,
            })
            .map_to_mm(|e| e)?;

        // Derive the address and public key.
        let address = coin
            .my_address()
            .map_err(|e| GetPrivateKeysError::KeyExportFailed {
                ticker: ticker.clone(),
                reason: e,
            })
            .map_to_mm(|e| e)?;

        let pubkey = coin
            .get_public_key()
            .map_err(|e| GetPrivateKeysError::KeyExportFailed {
                ticker: ticker.clone(),
                reason: e.to_string(),
            })
            .map_to_mm(|e| e)?;

        keys.push(CoinKeyInfo {
            coin: ticker.clone(),
            address,
            priv_key,
            pubkey,
        });
    }

    Ok(GetPrivateKeysResponse { keys })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_status_codes() {
        assert_eq!(
            GetPrivateKeysError::CoinNotActive("X".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            GetPrivateKeysError::HardwareWalletNotSupported.status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            GetPrivateKeysError::Internal("x".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
}
