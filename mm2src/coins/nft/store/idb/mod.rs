//! WASM IndexedDB backend for the NFT storage traits.
//!
//! Layout differs from the SQLite backend: instead of one table per
//! chain, the IndexedDB schema keeps a single object store per concept
//! (inventory, transfer history, scan progress) and uses a `chain` index
//! plus a multi-index primary key to scope queries. This is friendlier
//! to IndexedDB's schema-migration model — adding a new chain does not
//! require a version bump.
//!
//! Each row stores the canonical [`Nft`] / [`NftTransfer`] payload as a
//! JSON string in a `payload` column, mirroring the SQLite layout. The
//! scalar fields above the payload exist solely so that filters and
//! sort orders can be expressed through IndexedDB indexes.

mod history;
mod list;
mod schema;

#[cfg(target_arch = "wasm32")]
#[cfg(test)]
mod tests;

use crate::nft::model::Chain;
use crate::nft::store::errors::NftStoreError;
use derive_more::Display;
use mm2_db::indexed_db::{DbIdentifier, DbInstance, DbLocked, DbTransactionError, IndexedDb, IndexedDbBuilder,
                         InitDbError, InitDbResult, SharedDb, WeakDb};
use mm2_err_handle::prelude::*;

pub(crate) use schema::{InventoryRow, ScanProgressRow, TransferRow};

const DB_NAME: &str = "nft_storage";
const DB_VERSION: u32 = 1;

/// Storage backend struct: owns a [`SharedDb`] handle to the underlying
/// IndexedDB instance. Cloning is cheap (it clones the `SharedDb`
/// `Arc`).
#[derive(Clone)]
pub struct IndexedDbNftStore {
    db: SharedDb<NftIndexedDb>,
}

impl IndexedDbNftStore {
    /// Wrap a pre-initialised [`SharedDb`] handle. Construction is
    /// deferred to the caller so the database lifetime can be tied to
    /// `MmCtx`.
    pub fn new(db: SharedDb<NftIndexedDb>) -> Self { Self { db } }

    /// Returns a weak handle that the trait impls can stash for
    /// asynchronous spawn-style use.
    #[allow(dead_code)]
    pub(crate) fn weak(&self) -> WeakDb<NftIndexedDb> { SharedDb::downgrade(&self.db) }

    /// Lock the shared DB, lazily constructing it on first call.
    pub(crate) async fn lock_db(&self) -> MmResult<DbLocked<'_, NftIndexedDb>, IndexedDbStoreError> {
        self.db.get_or_initialize().await.map_mm_err()
    }
}

/// Map [`Chain`] to its UPPERCASE serde label (the same encoding used
/// by the SQLite backend and the wire model).
pub(crate) fn chain_label(chain: &Chain) -> String { format!("{}", chain) }

/// Concrete `DbInstance` registered with `mm2_db::indexed_db`.
pub struct NftIndexedDb {
    pub(crate) inner: IndexedDb,
}

#[async_trait::async_trait]
impl DbInstance for NftIndexedDb {
    fn db_name() -> &'static str { DB_NAME }

    async fn init(db_id: DbIdentifier) -> InitDbResult<Self> {
        let inner = IndexedDbBuilder::new(db_id)
            .with_version(DB_VERSION)
            .with_table::<InventoryRow>()
            .with_table::<TransferRow>()
            .with_table::<ScanProgressRow>()
            .build()
            .await?;
        Ok(Self { inner })
    }
}

/// Error type for the IndexedDB backend. Wraps the lower-level driver
/// errors plus a few backend-specific cases (payload (de)serialisation,
/// pagination overflow).
#[derive(Debug, Display)]
pub enum IndexedDbStoreError {
    /// The shared database handle has been dropped.
    #[display(fmt = "IndexedDB connection has been dropped")]
    ConnectionDropped,
    /// The lower-level IndexedDB driver returned an error.
    #[display(fmt = "IndexedDB transaction error: {}", _0)]
    Transaction(String),
    /// IndexedDB initialisation failed.
    #[display(fmt = "IndexedDB init error: {}", _0)]
    Init(String),
    /// JSON (de)serialisation of a stored payload failed.
    #[display(fmt = "IndexedDB payload error: {}", _0)]
    Payload(String),
    /// Cursor traversal failed.
    #[display(fmt = "IndexedDB cursor error: {}", _0)]
    Cursor(String),
    /// Method has not yet been ported to the IndexedDB backend.
    /// These markers intentionally surface as a recognisable error so
    /// stub callers (e.g. the still-unimplemented `update_nft` and
    /// `refresh_nft_metadata` handlers) fail fast on WASM.
    #[display(fmt = "IndexedDB store: {} is not yet implemented", _0)]
    Unimplemented(&'static str),
}

impl NotMmError for IndexedDbStoreError {}
impl NftStoreError for IndexedDbStoreError {}

impl From<DbTransactionError> for IndexedDbStoreError {
    fn from(e: DbTransactionError) -> Self { IndexedDbStoreError::Transaction(e.to_string()) }
}

impl From<InitDbError> for IndexedDbStoreError {
    fn from(e: InitDbError) -> Self { IndexedDbStoreError::Init(e.to_string()) }
}

impl From<serde_json::Error> for IndexedDbStoreError {
    fn from(e: serde_json::Error) -> Self { IndexedDbStoreError::Payload(e.to_string()) }
}

/// Marker helper used by the trait impls when a method is intentionally
/// not yet implemented in this revision.
#[allow(dead_code)]
pub(crate) fn unimplemented<T>(name: &'static str) -> Result<T, MmError<IndexedDbStoreError>> {
    Err(MmError::new(IndexedDbStoreError::Unimplemented(name)))
}
