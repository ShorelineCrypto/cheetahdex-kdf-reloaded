//! WASM (browser) backend for [`AccountStorage`].
//!
//! ## Status: stub, P7.1.b placeholder
//!
//! Browser web-wallet parity requires an IndexedDB-backed
//! implementation of the `AccountStorage` trait. That implementation
//! has not yet been clean-room ported to the reloaded branch; doing
//! so means designing a small object-store schema and wiring it
//! through `mm2_db::indexed_db`, which is a non-trivial standalone
//! work package.
//!
//! In the meantime, this module provides a no-op stub that
//! satisfies the `AccountStorage` trait so the workspace compiles
//! for `wasm32-unknown-unknown` (P7.1.a CI safety net) and the rest
//! of the WASM-build effort (P7.1.c, P7.1.d) can proceed in
//! parallel. Every method returns
//! `AccountStorageError::Internal("WasmAccountStorage stub: P7.1.b")`
//! at runtime, so a browser build that actually invokes a
//! `gui_storage::*` RPC will fail loudly with a clear marker rather
//! than silently corrupt user state.
//!
//! Tracking: RELOADED-PLAN.md P7.1.b — replace this stub with a real
//! IndexedDB-backed implementation following the same trait
//! contract as `sqlite_storage::SqliteAccountStorage`.

use crate::account::storage::{AccountStorage, AccountStorageError, AccountStorageResult};
use crate::account::{AccountId, AccountInfo, AccountWithCoins, AccountWithEnabledFlag, EnabledAccountId};
use async_trait::async_trait;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_number::BigDecimal;
use std::collections::{BTreeMap, BTreeSet};

const STUB_MSG: &str = "WasmAccountStorage stub: P7.1.b — IndexedDB backend not yet ported";

/// Stub WASM `AccountStorage`. See module docs.
pub(crate) struct WasmAccountStorage {
    _ctx: MmArc,
}

impl WasmAccountStorage {
    pub fn new(ctx: &MmArc) -> Self { WasmAccountStorage { _ctx: ctx.clone() } }

    fn not_implemented<T>() -> AccountStorageResult<T> {
        MmError::err(AccountStorageError::Internal(STUB_MSG.to_string()))
    }
}

#[async_trait]
impl AccountStorage for WasmAccountStorage {
    async fn init(&self) -> AccountStorageResult<()> {
        // `init` is called eagerly during context construction; returning
        // `Ok(())` here keeps the platform bootstrap path quiet. Any
        // subsequent state-changing call will surface the stub error.
        Ok(())
    }

    async fn load_account_coins(&self, _account_id: AccountId) -> AccountStorageResult<BTreeSet<String>> {
        Self::not_implemented()
    }

    async fn load_accounts(&self) -> AccountStorageResult<BTreeMap<AccountId, AccountInfo>> { Self::not_implemented() }

    async fn load_accounts_with_enabled_flag(
        &self,
    ) -> AccountStorageResult<BTreeMap<AccountId, AccountWithEnabledFlag>> {
        Self::not_implemented()
    }

    async fn load_enabled_account_id(&self) -> AccountStorageResult<EnabledAccountId> { Self::not_implemented() }

    async fn load_enabled_account_with_coins(&self) -> AccountStorageResult<AccountWithCoins> {
        Self::not_implemented()
    }

    async fn enable_account(&self, _account_id: EnabledAccountId) -> AccountStorageResult<()> {
        Self::not_implemented()
    }

    async fn upload_account(&self, _account: AccountInfo) -> AccountStorageResult<()> { Self::not_implemented() }

    async fn delete_account(&self, _account_id: AccountId) -> AccountStorageResult<()> { Self::not_implemented() }

    async fn set_name(&self, _account_id: AccountId, _name: String) -> AccountStorageResult<()> {
        Self::not_implemented()
    }

    async fn set_description(&self, _account_id: AccountId, _description: String) -> AccountStorageResult<()> {
        Self::not_implemented()
    }

    async fn set_balance(&self, _account_id: AccountId, _balance_usd: BigDecimal) -> AccountStorageResult<()> {
        Self::not_implemented()
    }

    async fn activate_coins(&self, _account_id: AccountId, _tickers: Vec<String>) -> AccountStorageResult<()> {
        Self::not_implemented()
    }

    async fn deactivate_coins(&self, _account_id: AccountId, _tickers: Vec<String>) -> AccountStorageResult<()> {
        Self::not_implemented()
    }
}
