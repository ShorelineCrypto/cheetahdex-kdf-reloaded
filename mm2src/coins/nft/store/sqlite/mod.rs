//! Native SQLite backend for the NFT storage traits.
//!
//! The schema is designed around a JSON-payload-per-row layout: each table
//! stores the canonical `Nft` / `NftTransfer` value as a JSON blob in the
//! `payload` column and exposes the fields needed for filtering and
//! pagination as plain scalar columns (`block_number`, `possible_spam`,
//! `possible_phishing`, `token_address`, `token_id_str`, …). This keeps the
//! schema short, lets us evolve the wire model without writing an ALTER
//! migration for every cosmetic field, and avoids exposing columns whose
//! contents we never query for in SQL.
//!
//! All tables are keyed by chain (one inventory table and one transfer
//! table per supported [`Chain`]) plus two global bookkeeping tables for
//! the per-chain scan bookmark and the schema version markers.

mod history;
mod list;
mod schema;

#[cfg(test)] mod tests;

use crate::nft::store::errors::NftStoreError;
use db_common::async_sql_conn::{AsyncConnError, AsyncConnection};
use std::sync::Arc;

/// Concrete `NftListStore` + `NftHistoryStore` implementation backed by
/// SQLite.
#[derive(Clone)]
pub struct SqliteNftStore {
    conn: Arc<AsyncConnection>,
}

impl SqliteNftStore {
    /// Wrap a pre-opened async SQLite connection. The connection is
    /// expected to be exclusive to the NFT subsystem (or at least to use
    /// table names that do not clash with other modules).
    pub fn new(conn: Arc<AsyncConnection>) -> Self { Self { conn } }

    pub(crate) fn conn(&self) -> &AsyncConnection { self.conn.as_ref() }
}

impl NftStoreError for AsyncConnError {}
