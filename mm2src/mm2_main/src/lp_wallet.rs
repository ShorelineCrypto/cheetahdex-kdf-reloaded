/// Wallet management: encrypted mnemonic persistence, wallet lifecycle RPCs.
///
/// Wallets are stored as JSON files in `DB/wallets/{wallet_name}.wallet`, each
/// containing an `EncryptedMnemonicData` blob. The wallet password is verified
/// by attempting to decrypt the mnemonic — no password hash is stored on disk.
///
/// The currently active wallet name is recorded in `MmCtx::wallet_name` (write-once)
/// during startup. Only inactive wallets can be deleted.
use common::HttpStatusCode;
use crypto::{decrypt_mnemonic, encrypt_mnemonic};
use derive_more::Display;
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use ser_error_derive::SerializeErrorType;
use serde::{Deserialize, Serialize};

#[cfg(not(target_arch = "wasm32"))]
mod storage {
    use crypto::EncryptedMnemonicData;
    use mm2_core::mm_ctx::MmArc;
    use mm2_io::fs::{read_dir_async, remove_file_async};
    use std::io;
    use std::path::PathBuf;

    /// Returns the directory where wallet files are stored, creating it if needed.
    fn wallets_dir(ctx: &MmArc) -> io::Result<PathBuf> {
        let dir = ctx.wallets_dir();
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(dir)
    }

    /// Path to a specific wallet file.
    fn wallet_path(ctx: &MmArc, wallet_name: &str) -> io::Result<PathBuf> {
        Ok(wallets_dir(ctx)?.join(format!("{}.wallet", wallet_name)))
    }

    /// Save encrypted mnemonic for a wallet. Overwrites if the wallet already exists.
    pub async fn save_encrypted_passphrase(
        ctx: &MmArc,
        wallet_name: &str,
        data: &EncryptedMnemonicData,
    ) -> Result<(), String> {
        let path = wallet_path(ctx, wallet_name).map_err(|e| format!("wallet dir error: {e}"))?;
        let json = serde_json::to_string_pretty(data).map_err(|e| format!("serialize error: {e}"))?;
        mm2_io::fs::write(&path, &json.as_bytes()).map_err(|e| format!("write error: {e}"))
    }

    /// Read encrypted mnemonic for a wallet. Returns None if the wallet doesn't exist.
    pub async fn read_encrypted_passphrase(
        ctx: &MmArc,
        wallet_name: &str,
    ) -> Result<Option<EncryptedMnemonicData>, String> {
        let path = wallet_path(ctx, wallet_name).map_err(|e| format!("wallet dir error: {e}"))?;
        if !path.exists() {
            return Ok(None);
        }
        let bytes = mm2_io::fs::slurp(&path)?;
        let data: EncryptedMnemonicData =
            serde_json::from_slice(&bytes).map_err(|e| format!("corrupt wallet file: {e}"))?;
        Ok(Some(data))
    }

    /// List all wallet names (from filenames, stripping the .wallet extension).
    pub async fn read_all_wallet_names(ctx: &MmArc) -> Result<Vec<String>, String> {
        let dir = wallets_dir(ctx).map_err(|e| format!("wallet dir error: {e}"))?;
        let entries = read_dir_async(&dir).await.map_err(|e| format!("read dir error: {e}"))?;
        let names = entries
            .iter()
            .filter_map(|p| {
                let name = p.file_name()?.to_str()?.to_string();
                name.strip_suffix(".wallet").map(|n| n.to_string())
            })
            .collect();
        Ok(names)
    }

    /// Delete a wallet file. Returns error if the file doesn't exist or I/O fails.
    pub async fn delete_wallet(ctx: &MmArc, wallet_name: &str) -> Result<(), String> {
        let path = wallet_path(ctx, wallet_name).map_err(|e| format!("wallet dir error: {e}"))?;
        if !path.exists() {
            return Err(format!("Wallet '{}' not found", wallet_name));
        }
        remove_file_async(path).await.map_err(|e| format!("delete error: {e}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
use storage::{delete_wallet, read_all_wallet_names, read_encrypted_passphrase, save_encrypted_passphrase};

// --- Error types ---

/// Errors for wallet initialization and wallet management RPCs.
#[derive(Clone, Debug, Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum WalletError {
    #[display(fmt = "Invalid request: {}", _0)]
    InvalidRequest(String),
    #[display(fmt = "Invalid password")]
    InvalidPassword,
    #[display(fmt = "Wallet '{}' already exists", _0)]
    WalletAlreadyExists(String),
    #[display(fmt = "Wallet '{}' not found", _0)]
    WalletNotFound(String),
    #[display(fmt = "Cannot delete active wallet '{}'", _0)]
    CannotDeleteActiveWallet(String),
    #[display(fmt = "Storage error: {}", _0)]
    StorageError(String),
    #[display(fmt = "Encryption error: {}", _0)]
    EncryptionError(String),
    #[display(fmt = "Internal error: {}", _0)]
    Internal(String),
}

impl HttpStatusCode for WalletError {
    fn status_code(&self) -> StatusCode {
        match self {
            WalletError::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            WalletError::InvalidPassword => StatusCode::BAD_REQUEST,
            WalletError::WalletAlreadyExists(_) => StatusCode::CONFLICT,
            WalletError::WalletNotFound(_) => StatusCode::NOT_FOUND,
            WalletError::CannotDeleteActiveWallet(_) => StatusCode::BAD_REQUEST,
            WalletError::StorageError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            WalletError::EncryptionError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            WalletError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

// --- Wallet name validation ---

/// Validates wallet name: alphanumeric, dash, underscore, space. 1-64 chars.
fn validate_wallet_name(name: &str) -> Result<(), WalletError> {
    if name.is_empty() || name.len() > 64 {
        return Err(WalletError::InvalidRequest(
            "Wallet name must be 1-64 characters".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == ' ')
    {
        return Err(WalletError::InvalidRequest(
            "Wallet name may only contain alphanumeric characters, dashes, underscores, and spaces".to_string(),
        ));
    }
    Ok(())
}

// --- RPC request/response types ---

#[derive(Deserialize)]
pub struct CreateWalletRequest {
    pub wallet_name: String,
    pub password: String,
    pub mnemonic: String,
}

#[derive(Debug, Serialize)]
pub struct CreateWalletResponse {
    pub wallet_name: String,
}

#[derive(Deserialize)]
pub struct GetWalletNamesRequest {}

#[derive(Debug, Serialize)]
pub struct GetWalletNamesResponse {
    pub wallet_names: Vec<String>,
    /// The currently active wallet, if any.
    pub active_wallet: Option<String>,
}

#[derive(Deserialize)]
pub struct DeleteWalletRequest {
    pub wallet_name: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteWalletResponse {
    pub wallet_name: String,
}

// --- RPC handlers ---

/// Creates a new wallet by encrypting and persisting the given mnemonic.
///
/// The wallet must not already exist. The mnemonic is validated as BIP39 by the
/// encryption layer. After this call, the wallet can be used in future sessions
/// by providing `wallet_name` + `password` at startup.
#[cfg(not(target_arch = "wasm32"))]
pub async fn create_wallet_rpc(
    ctx: MmArc,
    req: CreateWalletRequest,
) -> Result<CreateWalletResponse, MmError<WalletError>> {
    validate_wallet_name(&req.wallet_name)?;

    if req.password.is_empty() {
        return MmError::err(WalletError::InvalidRequest("Password cannot be empty".to_string()));
    }

    // Check if wallet already exists
    let existing = read_encrypted_passphrase(&ctx, &req.wallet_name)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?;
    if existing.is_some() {
        return MmError::err(WalletError::WalletAlreadyExists(req.wallet_name));
    }

    // Encrypt the mnemonic with the password
    let encrypted = encrypt_mnemonic(&req.mnemonic, &req.password)
        .map_err(|e| MmError::new(WalletError::EncryptionError(e.to_string())))?;

    // Persist the encrypted mnemonic
    save_encrypted_passphrase(&ctx, &req.wallet_name, &encrypted)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?;

    Ok(CreateWalletResponse {
        wallet_name: req.wallet_name,
    })
}

/// Lists all wallet names and identifies the currently active wallet.
#[cfg(not(target_arch = "wasm32"))]
pub async fn get_wallet_names_rpc(
    ctx: MmArc,
    _req: GetWalletNamesRequest,
) -> Result<GetWalletNamesResponse, MmError<WalletError>> {
    let wallet_names = read_all_wallet_names(&ctx)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?;

    let active_wallet = ctx.wallet_name.as_option().and_then(|opt| opt.clone());

    Ok(GetWalletNamesResponse {
        wallet_names,
        active_wallet,
    })
}

/// Deletes an inactive wallet after verifying the password.
///
/// The password is verified by decrypting the stored mnemonic. If decryption
/// succeeds, the wallet file is removed. The currently active wallet cannot
/// be deleted — stop the node first.
#[cfg(not(target_arch = "wasm32"))]
pub async fn delete_wallet_rpc(
    ctx: MmArc,
    req: DeleteWalletRequest,
) -> Result<DeleteWalletResponse, MmError<WalletError>> {
    validate_wallet_name(&req.wallet_name)?;

    // Block deletion of the active wallet
    if let Some(Some(active)) = ctx.wallet_name.as_option() {
        if active == &req.wallet_name {
            return MmError::err(WalletError::CannotDeleteActiveWallet(req.wallet_name));
        }
    }

    // Load the encrypted mnemonic
    let encrypted = read_encrypted_passphrase(&ctx, &req.wallet_name)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?
        .ok_or_else(|| MmError::new(WalletError::WalletNotFound(req.wallet_name.clone())))?;

    // Verify password by attempting decryption
    decrypt_mnemonic(&encrypted, &req.password).map_err(|_| MmError::new(WalletError::InvalidPassword))?;

    // Password verified — delete the wallet file
    delete_wallet(&ctx, &req.wallet_name)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?;

    Ok(DeleteWalletResponse {
        wallet_name: req.wallet_name,
    })
}

// --- Startup integration ---

/// Called during `lp_init` to optionally persist the passphrase as an encrypted wallet.
/// If `wallet_name` is provided in the config, encrypts and saves the passphrase
/// (unless the wallet already exists, in which case it verifies the passphrase matches).
///
/// Returns the wallet name if set, or None for anonymous mode.
#[cfg(not(target_arch = "wasm32"))]
pub async fn initialize_wallet_passphrase(
    ctx: &MmArc,
    passphrase: &str,
    wallet_name: Option<&str>,
    wallet_password: Option<&str>,
) -> Result<Option<String>, MmError<WalletError>> {
    let (name, password) = match (wallet_name, wallet_password) {
        (Some(name), Some(password)) => (name, password),
        (None, _) => {
            // Anonymous mode — no wallet persistence
            let _ = ctx.wallet_name.pin(None);
            return Ok(None);
        },
        (Some(_), None) => {
            return MmError::err(WalletError::InvalidRequest(
                "wallet_password is required when wallet_name is set".to_string(),
            ));
        },
    };

    validate_wallet_name(name)?;

    let existing = read_encrypted_passphrase(ctx, name)
        .await
        .map_err(|e| MmError::new(WalletError::StorageError(e)))?;

    if let Some(encrypted) = existing {
        // Wallet exists — verify passphrase matches
        let stored_mnemonic =
            decrypt_mnemonic(&encrypted, password).map_err(|_| MmError::new(WalletError::InvalidPassword))?;
        if stored_mnemonic != passphrase {
            return MmError::err(WalletError::InvalidRequest(
                "Passphrase doesn't match the stored wallet. Create a new wallet to use a different passphrase"
                    .to_string(),
            ));
        }
    } else {
        // New wallet — encrypt and save
        let encrypted = encrypt_mnemonic(passphrase, password)
            .map_err(|e| MmError::new(WalletError::EncryptionError(e.to_string())))?;
        save_encrypted_passphrase(ctx, name, &encrypted)
            .await
            .map_err(|e| MmError::new(WalletError::StorageError(e)))?;
    }

    let _ = ctx.wallet_name.pin(Some(name.to_string()));
    Ok(Some(name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::block_on;
    use http::StatusCode;
    use mm2_core::mm_ctx::MmCtxBuilder;
    use serde_json::json;
    use std::env;

    /// Create a test MmCtx with a unique temp dbdir.
    fn test_ctx() -> MmArc {
        let dir = env::temp_dir().join(format!("kdf_wallet_test_{}", common::now_ms()));
        MmCtxBuilder::default()
            .with_conf(json!({"dbdir": dir.to_str().unwrap()}))
            .into_mm_arc()
    }

    #[test]
    fn test_validate_wallet_name() {
        assert!(validate_wallet_name("my-wallet_1").is_ok());
        assert!(validate_wallet_name("My Wallet").is_ok());
        assert!(validate_wallet_name("a").is_ok());
        assert!(validate_wallet_name(&"x".repeat(64)).is_ok());

        // Too long
        assert!(validate_wallet_name(&"x".repeat(65)).is_err());
        // Empty
        assert!(validate_wallet_name("").is_err());
        // Invalid chars
        assert!(validate_wallet_name("wallet/bad").is_err());
        assert!(validate_wallet_name("wallet..bad").is_err());
        assert!(validate_wallet_name("wallet\0").is_err());
    }

    #[test]
    fn test_wallet_error_status_codes() {
        assert_eq!(
            WalletError::InvalidRequest("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(WalletError::InvalidPassword.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(
            WalletError::WalletAlreadyExists("x".into()).status_code(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            WalletError::WalletNotFound("x".into()).status_code(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            WalletError::CannotDeleteActiveWallet("x".into()).status_code(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            WalletError::StorageError("x".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_wallet_lifecycle_create_list_delete() {
        let ctx = test_ctx();
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let password = "test_password_123";

        // Create wallet
        let resp = block_on(create_wallet_rpc(ctx.clone(), CreateWalletRequest {
            wallet_name: "test-wallet".to_string(),
            password: password.to_string(),
            mnemonic: mnemonic.to_string(),
        }))
        .unwrap();
        assert_eq!(resp.wallet_name, "test-wallet");

        // List wallets — should contain exactly one
        let list = block_on(get_wallet_names_rpc(ctx.clone(), GetWalletNamesRequest {})).unwrap();
        assert_eq!(list.wallet_names, vec!["test-wallet".to_string()]);
        assert_eq!(list.active_wallet, None); // no active wallet set

        // Delete with wrong password — should fail
        let err = block_on(delete_wallet_rpc(ctx.clone(), DeleteWalletRequest {
            wallet_name: "test-wallet".to_string(),
            password: "wrong_password".to_string(),
        }))
        .unwrap_err();
        assert_eq!(err.get_inner().status_code(), StatusCode::BAD_REQUEST);

        // Delete with correct password
        let resp = block_on(delete_wallet_rpc(ctx.clone(), DeleteWalletRequest {
            wallet_name: "test-wallet".to_string(),
            password: password.to_string(),
        }))
        .unwrap();
        assert_eq!(resp.wallet_name, "test-wallet");

        // List wallets — should be empty now
        let list = block_on(get_wallet_names_rpc(ctx.clone(), GetWalletNamesRequest {})).unwrap();
        assert!(list.wallet_names.is_empty());
    }

    #[test]
    fn test_create_duplicate_wallet_fails() {
        let ctx = test_ctx();
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let password = "pw123";

        block_on(create_wallet_rpc(ctx.clone(), CreateWalletRequest {
            wallet_name: "dup-wallet".to_string(),
            password: password.to_string(),
            mnemonic: mnemonic.to_string(),
        }))
        .unwrap();

        // Second create should fail with Conflict
        let err = block_on(create_wallet_rpc(ctx.clone(), CreateWalletRequest {
            wallet_name: "dup-wallet".to_string(),
            password: password.to_string(),
            mnemonic: mnemonic.to_string(),
        }))
        .unwrap_err();
        assert_eq!(err.get_inner().status_code(), StatusCode::CONFLICT);
    }

    #[test]
    fn test_delete_nonexistent_wallet_fails() {
        let ctx = test_ctx();

        let err = block_on(delete_wallet_rpc(ctx.clone(), DeleteWalletRequest {
            wallet_name: "ghost-wallet".to_string(),
            password: "any".to_string(),
        }))
        .unwrap_err();
        assert_eq!(err.get_inner().status_code(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_delete_active_wallet_blocked() {
        let ctx = test_ctx();
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let password = "pw123";

        // Create wallet
        block_on(create_wallet_rpc(ctx.clone(), CreateWalletRequest {
            wallet_name: "active-wallet".to_string(),
            password: password.to_string(),
            mnemonic: mnemonic.to_string(),
        }))
        .unwrap();

        // Set it as active
        let _ = ctx.wallet_name.pin(Some("active-wallet".to_string()));

        // Delete should be blocked
        let err = block_on(delete_wallet_rpc(ctx.clone(), DeleteWalletRequest {
            wallet_name: "active-wallet".to_string(),
            password: password.to_string(),
        }))
        .unwrap_err();
        assert_eq!(err.get_inner().status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_initialize_wallet_passphrase_new_wallet() {
        let ctx = test_ctx();
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let password = "init_test_pw";

        let result = block_on(initialize_wallet_passphrase(
            &ctx,
            mnemonic,
            Some("init-wallet"),
            Some(password),
        ));
        assert_eq!(result.unwrap(), Some("init-wallet".to_string()));

        // wallet_name should be set on ctx
        assert_eq!(ctx.wallet_name.as_option(), Some(&Some("init-wallet".to_string())));

        // File should exist
        let list = block_on(get_wallet_names_rpc(ctx.clone(), GetWalletNamesRequest {})).unwrap();
        assert!(list.wallet_names.contains(&"init-wallet".to_string()));
    }

    #[test]
    fn test_initialize_wallet_passphrase_anonymous_mode() {
        let ctx = test_ctx();
        let mnemonic = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

        let result = block_on(initialize_wallet_passphrase(&ctx, mnemonic, None, None));
        assert_eq!(result.unwrap(), None);
        assert_eq!(ctx.wallet_name.as_option(), Some(&None));
    }
}
