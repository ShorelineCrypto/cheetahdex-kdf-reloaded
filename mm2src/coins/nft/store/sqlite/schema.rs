//! Per-chain table-name helpers and CREATE TABLE statements for the
//! SQLite NFT backend.

use crate::nft::model::{Chain, ChainTicker};
use db_common::sqlite::rusqlite::Result as SqlResult;
use db_common::sqlite::SafeTableName;

/// Bookmark table tracking the last block scanned by the providers layer
/// for each chain.
pub(super) const SCAN_PROGRESS_TABLE: &str = "nft_chain_progress";

/// Returns the safe inventory table name for `chain`
/// (`<nft_ticker>_inventory`, e.g. `NFT_ETH_inventory`).
pub(super) fn inventory_table(chain: &Chain) -> SqlResult<SafeTableName> {
    SafeTableName::new(&format!("{}_inventory", chain.nft_ticker()))
}

/// Returns the safe transfer-history table name for `chain`
/// (`<nft_ticker>_transfers`).
pub(super) fn transfers_table(chain: &Chain) -> SqlResult<SafeTableName> {
    SafeTableName::new(&format!("{}_transfers", chain.nft_ticker()))
}

/// CREATE TABLE statement for the per-chain inventory.
pub(super) fn create_inventory_sql(chain: &Chain) -> SqlResult<String> {
    let table = inventory_table(chain)?;
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} (
            token_address     TEXT    NOT NULL,
            token_id_str      TEXT    NOT NULL,
            block_number      INTEGER NOT NULL,
            possible_spam     INTEGER NOT NULL DEFAULT 0,
            possible_phishing INTEGER NOT NULL DEFAULT 0,
            contract_type     TEXT    NOT NULL,
            image_domain      TEXT,
            animation_domain  TEXT,
            external_domain   TEXT,
            payload           TEXT    NOT NULL,
            PRIMARY KEY (token_address, token_id_str)
        );",
        table.inner()
    ))
}

/// CREATE TABLE statement for the per-chain transfer history.
pub(super) fn create_transfers_sql(chain: &Chain) -> SqlResult<String> {
    let table = transfers_table(chain)?;
    Ok(format!(
        "CREATE TABLE IF NOT EXISTS {} (
            transaction_hash  TEXT    NOT NULL,
            log_index         INTEGER NOT NULL,
            token_id_str      TEXT    NOT NULL,
            token_address     TEXT    NOT NULL,
            block_number      INTEGER NOT NULL,
            block_timestamp   INTEGER NOT NULL,
            possible_spam     INTEGER NOT NULL DEFAULT 0,
            possible_phishing INTEGER NOT NULL DEFAULT 0,
            status            TEXT    NOT NULL,
            token_domain      TEXT,
            image_domain      TEXT,
            payload           TEXT    NOT NULL,
            PRIMARY KEY (transaction_hash, log_index, token_id_str)
        );",
        table.inner()
    ))
}

/// CREATE TABLE statement for the global scan-progress bookmark.
pub(super) fn create_scan_progress_sql() -> String {
    format!(
        "CREATE TABLE IF NOT EXISTS {} (
            chain              TEXT PRIMARY KEY,
            last_scanned_block INTEGER NOT NULL DEFAULT 0
        );",
        SCAN_PROGRESS_TABLE
    )
}

/// SQL fragment used to test whether a table already exists.
pub(super) const TABLE_EXISTS_SQL: &str = "SELECT name FROM sqlite_master WHERE type='table' AND name=?1;";
