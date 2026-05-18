//! Object-store layouts and `TableSignature` impls for the IndexedDB
//! NFT backend.
//!
//! Each `TableSignature` describes a single object store; rows live as
//! JSON-serialised structs with a few scalar columns hoisted out for use
//! as IndexedDB indexes. The schema mirrors the SQLite backend's data
//! shape, but flattens "one-table-per-chain" into "one-store-per-concept
//! + chain index" so that adding a new chain requires no migration.

use mm2_db::indexed_db::{DbUpgrader, OnUpgradeResult, TableSignature};
use serde::{Deserialize, Serialize};

/// Object-store name for the per-wallet NFT inventory.
pub(crate) const INVENTORY_TABLE: &str = "nft_inventory";
/// Object-store name for the historical NFT transfer log.
pub(crate) const TRANSFERS_TABLE: &str = "nft_transfers";
/// Object-store name for the per-chain "last scanned block" bookmark.
pub(crate) const SCAN_PROGRESS_TABLE: &str = "nft_scan_progress";

/// One row in the inventory store. The owned-NFT payload itself is
/// kept as a JSON string so the wire shape can evolve without bumping
/// the IndexedDB schema version every time a cosmetic field is added.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct InventoryRow {
    /// Chain label (`"ETH"`, `"POLYGON"`, …) — used as a single-field
    /// non-unique index for per-chain queries.
    pub(crate) chain: String,
    /// Hex-encoded NFT contract address.
    pub(crate) token_address: String,
    /// Decimal-encoded token id; stored as a string so it can survive
    /// `BigUint` values that exceed `u64`.
    pub(crate) token_id_str: String,
    /// Block number stored as `u64` for index-friendly sorting.
    pub(crate) block_number: u64,
    /// 0 / 1 (kept numeric so it can be filtered through an index).
    pub(crate) possible_spam: u32,
    /// 0 / 1 (kept numeric so it can be filtered through an index).
    pub(crate) possible_phishing: u32,
    /// `"ERC721"` / `"ERC1155"` (kept stringly-typed for index access).
    pub(crate) contract_type: String,
    /// Optional metadata domains (used by phishing filtering).
    pub(crate) image_domain: Option<String>,
    pub(crate) animation_domain: Option<String>,
    pub(crate) external_domain: Option<String>,
    /// Canonical [`crate::nft::model::Nft`] payload as JSON.
    pub(crate) payload: String,
}

impl TableSignature for InventoryRow {
    fn table_name() -> &'static str {
        INVENTORY_TABLE
    }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if old_version == 0 && new_version >= 1 {
            let table = upgrader.create_table(Self::table_name())?;
            // Per-chain queries.
            table.create_index("chain", false)?;
            // Pagination by block.
            table.create_index("block_number", false)?;
            // Spam / phishing filters.
            table.create_index("possible_spam", false)?;
            table.create_index("possible_phishing", false)?;
            // Domain-based phishing flips.
            table.create_index("image_domain", false)?;
            table.create_index("animation_domain", false)?;
            table.create_index("external_domain", false)?;
            // (chain, token_address) lookup for "all tokens of contract".
            table.create_multi_index("chain_contract", &["chain", "token_address"], false)?;
            // Unique primary-key index used for fetch / replace / drop.
            table.create_multi_index(
                "chain_contract_token",
                &["chain", "token_address", "token_id_str"],
                true,
            )?;
        }
        Ok(())
    }
}

/// One row in the transfer-history store.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TransferRow {
    /// Chain label.
    pub(crate) chain: String,
    /// 0x-prefixed transaction hash.
    pub(crate) transaction_hash: String,
    pub(crate) log_index: u32,
    pub(crate) token_id_str: String,
    /// Hex-encoded NFT contract address.
    pub(crate) token_address: String,
    pub(crate) block_number: u64,
    pub(crate) block_timestamp: u64,
    pub(crate) possible_spam: u32,
    pub(crate) possible_phishing: u32,
    /// Direction relative to the wallet (`"Receive"` / `"Send"`).
    pub(crate) status: String,
    pub(crate) token_domain: Option<String>,
    pub(crate) image_domain: Option<String>,
    /// Canonical [`crate::nft::model::NftTransfer`] payload as JSON.
    pub(crate) payload: String,
}

impl TableSignature for TransferRow {
    fn table_name() -> &'static str {
        TRANSFERS_TABLE
    }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if old_version == 0 && new_version >= 1 {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_index("chain", false)?;
            table.create_index("block_number", false)?;
            table.create_index("possible_spam", false)?;
            table.create_index("possible_phishing", false)?;
            table.create_index("token_domain", false)?;
            table.create_index("image_domain", false)?;
            // (chain, token_address) and (chain, token_address, token_id_str)
            // for "transfers of contract" / "transfers of token".
            table.create_multi_index("chain_contract", &["chain", "token_address"], false)?;
            table.create_multi_index(
                "chain_contract_token",
                &["chain", "token_address", "token_id_str"],
                false,
            )?;
            // Unique log lookup: (chain, transaction_hash, log_index, token_id_str).
            table.create_multi_index(
                "chain_log",
                &["chain", "transaction_hash", "log_index", "token_id_str"],
                true,
            )?;
        }
        Ok(())
    }
}

/// One row in the per-chain scan-progress bookmark store.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ScanProgressRow {
    /// Chain label (used as the primary key).
    pub(crate) chain: String,
    /// Last block scanned by the providers layer.
    pub(crate) last_scanned_block: u64,
}

impl TableSignature for ScanProgressRow {
    fn table_name() -> &'static str {
        SCAN_PROGRESS_TABLE
    }

    fn on_upgrade_needed(upgrader: &DbUpgrader, old_version: u32, new_version: u32) -> OnUpgradeResult<()> {
        if old_version == 0 && new_version >= 1 {
            let table = upgrader.create_table(Self::table_name())?;
            table.create_index("chain", true)?;
        }
        Ok(())
    }
}
