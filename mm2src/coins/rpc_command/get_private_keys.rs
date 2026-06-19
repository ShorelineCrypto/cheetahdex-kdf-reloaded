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

/// Key-export mode discriminator (R-K4). Absent / `iguana` selects the reduced,
/// always-available activated-coins export; `hd` selects the opt-in HD superset.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum GetPrivateKeysMode {
    #[default]
    Iguana,
    Hd,
}

#[derive(Deserialize)]
pub struct GetPrivateKeysRequest {
    /// Tickers of coins whose keys should be exported. In the reduced (default)
    /// form these MUST be activated; the opt-in superset relaxes that to any
    /// coin defined in the node configuration.
    pub coins: Vec<String>,
    /// Export mode (R-K4). Defaults to `iguana` (reduced form) when absent.
    #[serde(default)]
    pub mode: GetPrivateKeysMode,
    /// Opt-in: export coins that are merely configured, not activated.
    #[serde(default)]
    pub offline: bool,
    /// Opt-in (`hd` mode only): first BIP-44 address index of the range.
    #[serde(default)]
    pub start_index: Option<u32>,
    /// Opt-in (`hd` mode only): last BIP-44 address index of the range (inclusive).
    #[serde(default)]
    pub end_index: Option<u32>,
    /// Opt-in (`hd` mode only): BIP-44 account index.
    #[serde(default)]
    pub account_index: Option<u32>,
    /// Opt-in: include the shielded `viewing_key` for ZHTLC coins.
    #[serde(default)]
    pub include_shielded: bool,
}

impl GetPrivateKeysRequest {
    /// True when the request asks for any capability beyond the reduced,
    /// always-available activated-coins export — i.e. any part of the opt-in
    /// superset of R-K4 (HD mode, offline/no-activation export, HD index range
    /// parameters, or shielded viewing-key export).
    pub fn requests_superset(&self) -> bool {
        self.mode == GetPrivateKeysMode::Hd
            || self.offline
            || self.start_index.is_some()
            || self.end_index.is_some()
            || self.account_index.is_some()
            || self.include_shielded
    }
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
    #[display(
        fmt = "Insecure key export is disabled; set `allow_insecure_key_export=true` in MM2.json to enable the offline/HD/ZHTLC export superset"
    )]
    InsecureExportDisabled,
    #[display(fmt = "Key-export mode not yet implemented: {}", _0)]
    SupersetNotYetImplemented(String),
}

impl HttpStatusCode for GetPrivateKeysError {
    fn status_code(&self) -> StatusCode {
        match self {
            GetPrivateKeysError::CoinNotActive(_) => StatusCode::BAD_REQUEST,
            GetPrivateKeysError::KeyExportFailed { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            GetPrivateKeysError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            GetPrivateKeysError::HardwareWalletNotSupported => StatusCode::BAD_REQUEST,
            GetPrivateKeysError::InsecureExportDisabled => StatusCode::FORBIDDEN,
            GetPrivateKeysError::SupersetNotYetImplemented(_) => StatusCode::NOT_IMPLEMENTED,
        }
    }
}

impl From<CryptoCtxError> for GetPrivateKeysError {
    fn from(e: CryptoCtxError) -> Self { GetPrivateKeysError::Internal(e.to_string()) }
}

// ── Opt-in switch ───────────────────────────────────────────────────────

/// Read the `allow_insecure_key_export` switch (R-K1) from the node config.
///
/// Default-false: only a configuration value that is the boolean `true`
/// enables the opt-in superset. This mirrors the `allow_weak_password`
/// convention but is a **separate** switch governing a different threat model.
pub fn allow_insecure_key_export(ctx: &MmArc) -> bool {
    ctx.conf["allow_insecure_key_export"].as_bool() == Some(true)
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
    // Secure-by-default gate (R-K1/R-K4): any request reaching for the
    // offline/no-activation, HD-range, or shielded-viewing-key superset is
    // refused unless the operator has opted in via `allow_insecure_key_export`.
    // The reduced activated-coins path below is unaffected when no superset
    // field is set, regardless of the switch (R-K3).
    if req.requests_superset() && !allow_insecure_key_export(&ctx) {
        return MmError::err(GetPrivateKeysError::InsecureExportDisabled);
    }

    // Refuse if this is a Trezor/HW session — hardware wallets never expose keys to the host.
    let crypto_ctx = CryptoCtx::from_ctx(&ctx).mm_err(GetPrivateKeysError::from)?;
    if crypto_ctx.hw_ctx().is_some() {
        return MmError::err(GetPrivateKeysError::HardwareWalletNotSupported);
    }

    // TODO(ch07b): implement the opt-in superset (R-K4) — offline export of
    // merely-configured coins, HD per-derivation-path ranges bounded at 100
    // addresses per call, and ZHTLC shielded `viewing_key` / `z_derivation_path`
    // export — together with the untagged-union response shape. Until then the
    // switch is genuinely consumed (the gate above) and an opted-in superset
    // request returns a typed not-implemented error rather than a panic.
    if req.requests_superset() {
        let mode = match req.mode {
            GetPrivateKeysMode::Hd => "hd",
            GetPrivateKeysMode::Iguana => "iguana (offline/shielded)",
        };
        return MmError::err(GetPrivateKeysError::SupersetNotYetImplemented(mode.to_owned()));
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
        assert_eq!(
            GetPrivateKeysError::InsecureExportDisabled.status_code(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            GetPrivateKeysError::SupersetNotYetImplemented("hd".into()).status_code(),
            StatusCode::NOT_IMPLEMENTED
        );
    }

    fn reduced_req(coins: Vec<String>) -> GetPrivateKeysRequest {
        GetPrivateKeysRequest {
            coins,
            mode: GetPrivateKeysMode::Iguana,
            offline: false,
            start_index: None,
            end_index: None,
            account_index: None,
            include_shielded: false,
        }
    }

    #[test]
    fn test_requests_superset() {
        // Reduced (always-available) form requests no superset capability.
        assert!(!reduced_req(vec!["RICK".into()]).requests_superset());

        // Each opt-in capability independently flags the request as superset.
        let hd = GetPrivateKeysRequest {
            mode: GetPrivateKeysMode::Hd,
            ..reduced_req(vec![])
        };
        assert!(hd.requests_superset());
        assert!(GetPrivateKeysRequest {
            offline: true,
            ..reduced_req(vec![])
        }
        .requests_superset());
        assert!(GetPrivateKeysRequest {
            start_index: Some(0),
            ..reduced_req(vec![])
        }
        .requests_superset());
        assert!(GetPrivateKeysRequest {
            end_index: Some(5),
            ..reduced_req(vec![])
        }
        .requests_superset());
        assert!(GetPrivateKeysRequest {
            account_index: Some(1),
            ..reduced_req(vec![])
        }
        .requests_superset());
        assert!(GetPrivateKeysRequest {
            include_shielded: true,
            ..reduced_req(vec![])
        }
        .requests_superset());
    }

    #[test]
    fn test_allow_insecure_key_export_switch() {
        use mm2_core::mm_ctx::MmCtxBuilder;
        use serde_json::json;

        // Absent → false.
        let ctx = MmCtxBuilder::new().with_conf(json!({})).into_mm_arc();
        assert!(!allow_insecure_key_export(&ctx));

        // Explicit false → false.
        let ctx = MmCtxBuilder::new()
            .with_conf(json!({ "allow_insecure_key_export": false }))
            .into_mm_arc();
        assert!(!allow_insecure_key_export(&ctx));

        // Truthy only for the boolean `true` (mirrors `allow_weak_password`).
        let ctx = MmCtxBuilder::new()
            .with_conf(json!({ "allow_insecure_key_export": "true" }))
            .into_mm_arc();
        assert!(!allow_insecure_key_export(&ctx));

        let ctx = MmCtxBuilder::new()
            .with_conf(json!({ "allow_insecure_key_export": true }))
            .into_mm_arc();
        assert!(allow_insecure_key_export(&ctx));
    }

    #[test]
    fn test_superset_gated_when_switch_off() {
        use common::block_on;
        use mm2_core::mm_ctx::MmCtxBuilder;
        use serde_json::json;

        // Switch off + a superset request → refused before any crypto/coin work.
        let ctx = MmCtxBuilder::new().with_conf(json!({})).into_mm_arc();
        let req = GetPrivateKeysRequest {
            offline: true,
            ..reduced_req(vec!["RICK".into()])
        };
        match block_on(get_private_keys(ctx, req)) {
            Ok(_) => panic!("expected InsecureExportDisabled"),
            Err(e) => {
                assert_eq!(e.get_inner().status_code(), StatusCode::FORBIDDEN);
                assert!(matches!(
                    e.into_inner(),
                    GetPrivateKeysError::InsecureExportDisabled
                ));
            },
        }
    }
}