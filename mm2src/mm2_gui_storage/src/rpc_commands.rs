use crate::account::storage::AccountStorageError;
// crd:pin-begin
use crate::account::{AccountId, AccountInfo, AccountWithCoins, AccountWithEnabledFlag, EnabledAccountId,
                     MAX_ACCOUNT_DESCRIPTION_LENGTH, MAX_ACCOUNT_NAME_LENGTH, MAX_TICKER_LENGTH};
// crd:pin-end
use crate::context::AccountContext;
use common::{HttpStatusCode, StatusCode, SuccessResponse};
use derive_more::Display;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_number::BigDecimal;
use ser_error_derive::SerializeErrorType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")] // crd:pin
pub enum AccountRpcError {
    // crd:pin-begin
    #[display(fmt = "Account name is too long, expected shorter or equal to {max_len}")]
    NameTooLong { max_len: usize },
    #[display(fmt = "Account description is too long, expected shorter or equal to {max_len}")]
    DescriptionTooLong { max_len: usize },
    #[display(fmt = "Coin ticker is too long, expected shorter or equal to {max_len}")]
    TickerTooLong { max_len: usize },
    #[display(fmt = "No such account {_0:?}")]
    NoSuchAccount(AccountId),
    #[display(fmt = "No enabled account yet. Consider using 'enable_account' RPC")]
    NoEnabledAccount,
    #[display(fmt = "Account {_0:?} exists already")]
    AccountExistsAlready(AccountId),
    #[display(fmt = "Error loading account: {_0}")]
    ErrorLoadingAccount(String),
    #[display(fmt = "Error saving changes in accounts storage: {_0}")]
    ErrorSavingAccount(String),
    #[display(fmt = "Internal error: {_0}")]
    Internal(String),
    // crd:pin-end
}

impl From<AccountStorageError> for AccountRpcError {
    fn from(e: AccountStorageError) -> Self {
        use AccountStorageError as Storage;
        match e {
            Storage::NoSuchAccount(id) => AccountRpcError::NoSuchAccount(id), // crd:pin
            Storage::NoEnabledAccount => AccountRpcError::NoEnabledAccount,   // crd:pin
            Storage::AccountExistsAlready(id) => AccountRpcError::AccountExistsAlready(id), // crd:pin
            Storage::Internal(msg) => AccountRpcError::Internal(msg),         // crd:pin
            // Both flavours of read failure -- a raw load fault and a decode
            // fault -- are surfaced to the caller under one load-error category.
            Storage::ErrorLoading(msg) => AccountRpcError::ErrorLoadingAccount(msg),
            Storage::ErrorDeserializing(msg) => AccountRpcError::ErrorLoadingAccount(msg),
            // The two write failures -- a raw save fault and an encode fault --
            // likewise fold into a single save-error category.
            Storage::ErrorSaving(msg) => AccountRpcError::ErrorSavingAccount(msg),
            Storage::ErrorSerializing(msg) => AccountRpcError::ErrorSavingAccount(msg),
        }
    }
}

impl HttpStatusCode for AccountRpcError {
    fn status_code(&self) -> StatusCode {
        use AccountRpcError as Rpc;
        // Only failures owned by the storage layer are server-side (5xx). Every
        // other variant describes a request the caller can correct, so the
        // residual arm answers 4xx without re-listing each client-error case.
        match self {
            Rpc::ErrorLoadingAccount(_) | Rpc::ErrorSavingAccount(_) | Rpc::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR // crd:pin
            },
            _ => StatusCode::BAD_REQUEST,
        }
    }
}

#[derive(Deserialize)]
pub struct NewAccount<Id> {
    // crd:pin-begin
    account_id: Id,
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    balance_usd: BigDecimal,
    // crd:pin-end
}

impl<Id> From<NewAccount<Id>> for AccountInfo
where
    AccountId: From<Id>,
{
    fn from(new: NewAccount<Id>) -> Self {
        AccountInfo {
            account_id: AccountId::from(new.account_id),
            name: new.name,
            description: new.description,
            balance_usd: new.balance_usd,
        }
    }
}

#[derive(Deserialize)]
pub struct EnableAccountRequest {
    #[serde(flatten)] // crd:pin
    policy: EnableAccountPolicy,
}

#[derive(Deserialize)]
#[serde(tag = "policy")] // crd:pin
#[serde(rename_all = "snake_case")] // crd:pin
pub enum EnableAccountPolicy {
    // crd:pin-begin
    Existing(EnabledAccountId),
    New(NewAccount<EnabledAccountId>),
    // crd:pin-end
}

#[derive(Deserialize)]
pub struct AddAccountRequest {
    #[serde(flatten)] // crd:pin
    account: NewAccount<AccountId>,
}

#[derive(Deserialize)]
pub struct DeleteAccountRequest {
    account_id: AccountId, // crd:pin
}

#[derive(Deserialize)]
pub struct SetAccountNameRequest {
    // crd:pin-begin
    account_id: AccountId,
    name: String,
    // crd:pin-end
}

#[derive(Deserialize)]
pub struct SetAccountDescriptionRequest {
    // crd:pin-begin
    account_id: AccountId,
    description: String,
    // crd:pin-end
}

#[derive(Deserialize)]
pub struct CoinRequest {
    // crd:pin-begin
    account_id: AccountId,
    tickers: Vec<String>,
    // crd:pin-end
}

#[derive(Deserialize)]
pub struct GetAccountsRequest;

#[derive(Deserialize)]
pub struct GetAccountCoinsRequest {
    account_id: AccountId, // crd:pin
}

#[derive(Serialize)]
pub struct GetAccountCoinsResponse {
    // crd:pin-begin
    account_id: AccountId,
    coins: BTreeSet<String>,
    // crd:pin-end
}

#[derive(Deserialize)]
pub struct GetEnabledAccountRequest;

#[derive(Deserialize)]
pub struct SetBalanceRequest {
    // crd:pin-begin
    account_id: AccountId,
    balance_usd: BigDecimal,
    // crd:pin-end
}

/// Selects the active ("enabled") account, optionally creating it first.
///
/// The branch is chosen by [`EnableAccountPolicy`]:
/// * [`EnableAccountPolicy::Existing`] -- the account must already be present;
///   `enable_account` reports [`AccountRpcError::NoSuchAccount`] otherwise.
/// * [`EnableAccountPolicy::New`] -- the account is uploaded first, which fails
///   with [`AccountRpcError::AccountExistsAlready`] if the id is taken, and then
///   enabled.
///
/// Hardware-wallet identities cannot reach this handler: [`EnabledAccountId`]
/// has no `hw` variant, so deserialization rejects them up front.
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
/// Resolves the per-context account-storage handle, mapping a context-init
/// failure to [`AccountRpcError::Internal`].
fn account_context(ctx: &MmArc) -> MmResult<Arc<AccountContext>, AccountRpcError> {
    AccountContext::from_ctx(ctx).map_to_mm(AccountRpcError::Internal)
}

pub async fn enable_account(ctx: MmArc, req: EnableAccountRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    let account_ctx = account_context(&ctx)?;

    let target = match req.policy {
        EnableAccountPolicy::New(pending) => {
            let new_id = pending.account_id;
            account_ctx
                .storage()
                .await
                .map_mm_err()?
                .upload_account(AccountInfo::from(pending))
                .await
                .map_mm_err()?;
            new_id
        },
        EnableAccountPolicy::Existing(account_id) => account_id,
    };

    account_ctx
        .storage()
        .await
        .map_mm_err()?
        .enable_account(target)
        .await
        .map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Persists [`AddAccountRequest::account`], failing with
/// [`AccountRpcError::AccountExistsAlready`] if the identity is already stored.
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
pub async fn add_account(ctx: MmArc, req: AddAccountRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    validate_new_account(&req.account)?;

    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage
        .upload_account(AccountInfo::from(req.account))
        .await
        .map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Removes the [`DeleteAccountRequest::account_id`] account, failing with
/// [`AccountRpcError::NoSuchAccount`] when it is absent.
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
pub async fn delete_account(ctx: MmArc, req: DeleteAccountRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage.delete_account(req.account_id).await.map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Enumerates every stored account, tagging exactly one as enabled.
/// Returns [`AccountRpcError::NoEnabledAccount`] when nothing has been enabled.
///
/// # Note
///
/// Results come back ordered by `AccountId`.
// crd:pin-begin
pub async fn get_accounts(
    ctx: MmArc,
    _req: GetAccountsRequest,
) -> MmResult<Vec<AccountWithEnabledFlag>, AccountRpcError> {
    // crd:pin-end
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    let accounts = storage.load_accounts_with_enabled_flag().await.map_mm_err()?;
    // The backing `BTreeMap` is keyed by `AccountId`, so draining its values
    // preserves that ordering.
    Ok(accounts.into_values().collect())
}

/// Lists the activated coins of [`GetAccountCoinsRequest::account_id`].
///
/// # Note
///
/// The returned tickers are sorted.
// crd:pin-begin
pub async fn get_account_coins(
    ctx: MmArc,
    req: GetAccountCoinsRequest,
) -> MmResult<GetAccountCoinsResponse, AccountRpcError> {
    // crd:pin-end
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    let coins = storage.load_account_coins(req.account_id.clone()).await.map_mm_err()?;
    Ok(GetAccountCoinsResponse {
        account_id: req.account_id, // crd:pin
        coins,                      // crd:pin
    })
}

/// Returns the enabled account along with its activated coins.
/// Returns [`AccountRpcError::NoEnabledAccount`] when nothing has been enabled.
///
/// # Note
///
/// The returned coins are sorted.
// crd:pin-begin
pub async fn get_enabled_account(
    ctx: MmArc,
    _req: GetEnabledAccountRequest,
) -> MmResult<AccountWithCoins, AccountRpcError> {
    // crd:pin-end
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage.load_enabled_account_with_coins().await.map_mm_err()
}

/// Overwrites an account's display name.
pub async fn set_account_name(ctx: MmArc, req: SetAccountNameRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    validate_account_name(&req.name).map_mm_err()?;

    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage.set_name(req.account_id, req.name).await.map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Overwrites an account's description.
// crd:pin-begin
pub async fn set_account_description(
    ctx: MmArc,
    req: SetAccountDescriptionRequest,
) -> MmResult<SuccessResponse, AccountRpcError> {
    // crd:pin-end
    validate_account_desc(&req.description)?;

    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage
        .set_description(req.account_id, req.description)
        .await
        .map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Overwrites an account's cached fiat balance.
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
pub async fn set_account_balance(ctx: MmArc, req: SetBalanceRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    // The wire type is already a `BigDecimal`, so there is nothing to validate.
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage
        .set_balance(req.account_id, req.balance_usd)
        .await
        .map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Appends [`CoinRequest::tickers`] to the activated set of
/// [`CoinRequest::account_id`].
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
pub async fn activate_coins(ctx: MmArc, req: CoinRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    validate_tickers(&req.tickers)?;

    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage.activate_coins(req.account_id, req.tickers).await.map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Removes [`CoinRequest::tickers`] from the activated set of
/// [`CoinRequest::account_id`].
///
/// # Important
///
/// Only storage is affected; MarketMaker state is untouched.
pub async fn deactivate_coins(ctx: MmArc, req: CoinRequest) -> MmResult<SuccessResponse, AccountRpcError> {
    let account_ctx = account_context(&ctx)?;
    let storage = account_ctx.storage().await.map_mm_err()?;
    storage
        .deactivate_coins(req.account_id, req.tickers)
        .await
        .map_mm_err()?;
    Ok(SuccessResponse::new())
}

/// Returns the supplied too-long error when `value` exceeds `max` bytes.
fn ensure_max_len<F>(value: &str, max: usize, too_long: F) -> MmResult<(), AccountRpcError>
where
    F: FnOnce(usize) -> AccountRpcError,
{
    if value.len() > max {
        MmError::err(too_long(max))
    } else {
        Ok(())
    }
}

/// Validates both the name and the description of a to-be-created account.
fn validate_new_account<Id>(account: &NewAccount<Id>) -> MmResult<(), AccountRpcError> {
    validate_account_name(&account.name)?;
    validate_account_desc(&account.description)
}

/// Rejects names longer than [`MAX_ACCOUNT_NAME_LENGTH`].
fn validate_account_name(name: &str) -> MmResult<(), AccountRpcError> {
    ensure_max_len(name, MAX_ACCOUNT_NAME_LENGTH, |max_len| AccountRpcError::NameTooLong {
        max_len,
    })
}

/// Rejects descriptions longer than [`MAX_ACCOUNT_DESCRIPTION_LENGTH`].
fn validate_account_desc(description: &str) -> MmResult<(), AccountRpcError> {
    ensure_max_len(description, MAX_ACCOUNT_DESCRIPTION_LENGTH, |max_len| {
        AccountRpcError::DescriptionTooLong { max_len }
    })
}

/// Rejects the first ticker longer than [`MAX_TICKER_LENGTH`].
fn validate_tickers(tickers: &[String]) -> MmResult<(), AccountRpcError> {
    match tickers.iter().find(|ticker| ticker.len() > MAX_TICKER_LENGTH) {
        Some(_) => MmError::err(AccountRpcError::TickerTooLong {
            max_len: MAX_TICKER_LENGTH,
        }),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn description_too_long_reports_description_limit() {
        let over_long = "x".repeat(MAX_ACCOUNT_DESCRIPTION_LENGTH + 1);
        match validate_account_desc(&over_long).unwrap_err().into_inner() {
            AccountRpcError::DescriptionTooLong { max_len } => {
                assert_eq!(max_len, MAX_ACCOUNT_DESCRIPTION_LENGTH);
            },
            other => panic!("expected DescriptionTooLong, got {other}"),
        }
    }

    #[test]
    fn description_at_limit_is_accepted() {
        let at_limit = "x".repeat(MAX_ACCOUNT_DESCRIPTION_LENGTH);
        assert!(validate_account_desc(&at_limit).is_ok());
    }
}
