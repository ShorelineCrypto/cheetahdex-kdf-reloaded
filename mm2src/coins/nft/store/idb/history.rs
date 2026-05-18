//! `NftHistoryStore` trait impl for [`IndexedDbNftStore`].
//!
//! This revision is the full IndexedDB implementation: lifecycle,
//! reads, and the mutating helpers that flip spam / phishing flags or
//! back-fill metadata into transfer entries.

use crate::nft::model::{Chain, NftTokenIdent, NftTransfer, NftTransferList, NftTransfersFilters, TransferMeta};
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::idb::schema::{ScanProgressRow, TransferRow};
use crate::nft::store::idb::{chain_label, IndexedDbNftStore, IndexedDbStoreError};
use crate::nft::store::paginate;
use async_trait::async_trait;
use ethereum_types::Address;
use mm2_err_handle::prelude::*;
use mm2_number::BigUint;
use serde_json::Value as Json;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::str::FromStr;

/// Helper that turns a stored [`TransferRow::payload`] back into the
/// canonical [`NftTransfer`] value.
fn payload_to_transfer(row: &TransferRow) -> Result<NftTransfer, MmError<IndexedDbStoreError>> {
    serde_json::from_str::<NftTransfer>(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))
}

/// Build a [`TransferRow`] from a fully-populated [`NftTransfer`].
fn transfer_row_from(chain: &Chain, tr: &NftTransfer) -> Result<TransferRow, MmError<IndexedDbStoreError>> {
    let payload = serde_json::to_string(tr).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    Ok(TransferRow {
        chain: chain_label(chain),
        transaction_hash: tr.common.transaction_hash.clone(),
        log_index: tr.common.log_index,
        token_id_str: tr.token_id.to_string(),
        token_address: format!("{:?}", tr.common.token_address),
        block_number: tr.block_number,
        block_timestamp: tr.block_timestamp,
        possible_spam: u32::from(tr.common.possible_spam),
        possible_phishing: u32::from(tr.possible_phishing),
        status: tr.status.to_string(),
        token_domain: tr.token_domain.clone(),
        image_domain: tr.image_domain.clone(),
        payload,
    })
}

/// Apply the `NftTransfersFilters` to a stored row.
fn passes_transfer_filters(row: &TransferRow, filters: &Option<NftTransfersFilters>) -> bool {
    let f = match filters {
        Some(f) => f,
        None => return true,
    };
    if f.receive && row.status != "Receive" {
        return false;
    }
    if f.send && row.status != "Send" {
        return false;
    }
    if let Some(from) = f.from_date {
        if row.block_timestamp < from {
            return false;
        }
    }
    if let Some(to) = f.to_date {
        if row.block_timestamp > to {
            return false;
        }
    }
    if f.exclude_spam && row.possible_spam != 0 {
        return false;
    }
    if f.exclude_phishing && row.possible_phishing != 0 {
        return false;
    }
    true
}

#[async_trait]
impl NftHistoryStore for IndexedDbNftStore {
    type Error = IndexedDbStoreError;

    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        // Lifecycle is shared with the list store: a single
        // `ScanProgressRow` per chain marks readiness for both.
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        let chain_str = chain_label(chain);
        if table
            .get_item_by_unique_index("chain", chain_str.clone())
            .await
            .map_mm_err()?
            .is_none()
        {
            table
                .add_item(&ScanProgressRow {
                    chain: chain_str,
                    last_scanned_block: 0,
                })
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn chain_ready(&self, chain: &Chain) -> MmResult<bool, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        Ok(table
            .get_item_by_unique_index("chain", chain_label(chain))
            .await
            .map_mm_err()?
            .is_some())
    }

    async fn list_transfers(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftTransfersFilters>,
    ) -> MmResult<NftTransferList, Self::Error> {
        if chains.is_empty() {
            return Ok(NftTransferList {
                transfer_history: vec![],
                skipped: 0,
                total: 0,
            });
        }
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let mut rows: Vec<TransferRow> = Vec::new();
        let mut unfiltered_total: usize = 0;
        for chain in &chains {
            let chain_rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
            unfiltered_total = unfiltered_total.saturating_add(chain_rows.len());
            for (_id, row) in chain_rows {
                if passes_transfer_filters(&row, &filters) {
                    rows.push(row);
                }
            }
        }
        let total = rows.len();
        let skipped = unfiltered_total.saturating_sub(total);
        // Sort newest-first by (block_number, block_timestamp) DESC, mirroring SQLite.
        rows.sort_by(|a, b| {
            b.block_number
                .cmp(&a.block_number)
                .then_with(|| b.block_timestamp.cmp(&a.block_timestamp))
        });
        let (offset, limit) = paginate(take_all, page_size, page, total);
        let mut out = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for row in rows.into_iter().skip(offset).take(limit) {
            out.push(payload_to_transfer(&row)?);
        }
        Ok(NftTransferList {
            transfer_history: out,
            skipped,
            total,
        })
    }

    async fn append_transfers(&self, chain: Chain, transfers: Vec<NftTransfer>) -> MmResult<(), Self::Error> {
        let mut prepared = Vec::with_capacity(transfers.len());
        for tr in &transfers {
            prepared.push(transfer_row_from(&chain, tr)?);
        }
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        for row in &prepared {
            table
                .replace_item_by_unique_index(
                    "chain_log",
                    vec![
                        row.chain.clone(),
                        row.transaction_hash.clone(),
                        row.log_index.to_string(),
                        row.token_id_str.clone(),
                    ],
                    row,
                )
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn latest_transfer_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        Ok(rows.into_iter().map(|(_id, r)| r.block_number).max())
    }

    async fn transfers_since(&self, chain: Chain, from_block: u64) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(&chain)).await.map_mm_err()?;
        let mut filtered: Vec<TransferRow> = rows
            .into_iter()
            .filter_map(|(_id, r)| if r.block_number >= from_block { Some(r) } else { None })
            .collect();
        // Ascending order to match the SQLite backend.
        filtered.sort_by(|a, b| a.block_number.cmp(&b.block_number));
        let mut out = Vec::with_capacity(filtered.len());
        for row in &filtered {
            out.push(payload_to_transfer(row)?);
        }
        Ok(out)
    }

    async fn transfers_for_token(
        &self,
        chain: Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let key = vec![chain_label(&chain), token_address, token_id.to_string()];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let mut rows = table
            .get_items("chain_contract_token", key)
            .await
            .map_mm_err()?
            .into_iter()
            .map(|(_id, r)| r)
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| b.block_number.cmp(&a.block_number));
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(payload_to_transfer(row)?);
        }
        Ok(out)
    }

    async fn transfer_by_log(
        &self,
        chain: &Chain,
        transaction_hash: String,
        log_index: u32,
        token_id: BigUint,
    ) -> MmResult<Option<NftTransfer>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let key = vec![
            chain_label(chain),
            transaction_hash,
            log_index.to_string(),
            token_id.to_string(),
        ];
        let row = table.get_item_by_unique_index("chain_log", key).await.map_mm_err()?;
        match row {
            Some((_id, r)) => Ok(Some(payload_to_transfer(&r)?)),
            None => Ok(None),
        }
    }

    async fn attach_metadata_to_transfers(
        &self,
        chain: &Chain,
        meta: TransferMeta,
        flag_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let key = vec![
            chain_label(chain),
            meta.token_address.clone(),
            meta.token_id.to_string(),
        ];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain_contract_token", key).await.map_mm_err()?;
        for (_id, row) in rows {
            let updated = update_transfer_attach_metadata(&row, &meta, flag_spam)?;
            table
                .replace_item_by_unique_index(
                    "chain_log",
                    vec![
                        updated.chain.clone(),
                        updated.transaction_hash.clone(),
                        updated.log_index.to_string(),
                        updated.token_id_str.clone(),
                    ],
                    &updated,
                )
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn transfers_missing_metadata(&self, chain: Chain) -> MmResult<Vec<NftTokenIdent>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(&chain)).await.map_mm_err()?;
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut out: Vec<NftTokenIdent> = Vec::new();
        for (_id, row) in rows {
            if !seen.insert((row.token_address.clone(), row.token_id_str.clone())) {
                continue;
            }
            let payload: Json =
                serde_json::from_str(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
            let collection_name = payload.get("collection_name").and_then(|v| v.as_str());
            let token_name = payload.get("token_name").and_then(|v| v.as_str());
            if collection_name.is_none() || token_name.is_none() {
                let token_id = BigUint::from_str(&row.token_id_str)
                    .map_err(|e| MmError::new(IndexedDbStoreError::Payload(e.to_string())))?;
                out.push(NftTokenIdent {
                    token_address: row.token_address,
                    token_id,
                });
            }
        }
        Ok(out)
    }

    async fn transfers_for_contract(
        &self,
        chain: Chain,
        token_address: String,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        let key = vec![chain_label(&chain), token_address];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let mut rows = table
            .get_items("chain_contract", key)
            .await
            .map_mm_err()?
            .into_iter()
            .map(|(_id, r)| r)
            .collect::<Vec<_>>();
        rows.sort_by(|a, b| b.block_number.cmp(&a.block_number));
        let mut out = Vec::with_capacity(rows.len());
        for row in &rows {
            out.push(payload_to_transfer(row)?);
        }
        Ok(out)
    }

    async fn mark_contract_spam(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let key = vec![chain_label(chain), token_address];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain_contract", key).await.map_mm_err()?;
        for (_id, row) in rows {
            let updated = update_transfer_payload_spam(&row, possible_spam)?;
            table
                .replace_item_by_unique_index(
                    "chain_log",
                    vec![
                        updated.chain.clone(),
                        updated.transaction_hash.clone(),
                        updated.log_index.to_string(),
                        updated.token_id_str.clone(),
                    ],
                    &updated,
                )
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn contract_addresses(&self, chain: Chain) -> MmResult<HashSet<Address>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(&chain)).await.map_mm_err()?;
        let mut out: HashSet<Address> = HashSet::new();
        for (_id, row) in rows {
            let trimmed = row.token_address.strip_prefix("0x").unwrap_or(&row.token_address);
            let address =
                Address::from_str(trimmed).map_err(|e| MmError::new(IndexedDbStoreError::Payload(e.to_string())))?;
            out.insert(address);
        }
        Ok(out)
    }

    async fn domain_set(&self, chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        let mut out = HashSet::new();
        for (_id, row) in rows {
            if let Some(d) = row.token_domain {
                out.insert(d);
            }
            if let Some(d) = row.image_domain {
                out.insert(d);
            }
        }
        Ok(out)
    }

    async fn mark_domain_phishing(
        &self,
        chain: &Chain,
        domain: String,
        possible_phishing: bool,
    ) -> MmResult<(), Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<TransferRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        for (_id, row) in rows {
            let matches_domain = row.token_domain.as_deref() == Some(domain.as_str())
                || row.image_domain.as_deref() == Some(domain.as_str());
            if !matches_domain {
                continue;
            }
            let updated = update_transfer_payload_phishing(&row, possible_phishing)?;
            table
                .replace_item_by_unique_index(
                    "chain_log",
                    vec![
                        updated.chain.clone(),
                        updated.transaction_hash.clone(),
                        updated.log_index.to_string(),
                        updated.token_id_str.clone(),
                    ],
                    &updated,
                )
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn purge_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        // History purge is symmetric with `NftListStore::purge_chain`
        // but only touches the transfers + scan_progress stores.
        let chain_str = chain_label(chain);
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        txn.table::<TransferRow>()
            .await
            .map_mm_err()?
            .delete_items_by_index("chain", chain_str.clone())
            .await
            .map_mm_err()?;
        txn.table::<ScanProgressRow>()
            .await
            .map_mm_err()?
            .delete_item_by_unique_index("chain", chain_str)
            .await
            .map_mm_err()?;
        Ok(())
    }

    async fn purge_all(&self) -> MmResult<(), Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        txn.table::<TransferRow>()
            .await
            .map_mm_err()?
            .clear()
            .await
            .map_mm_err()?;
        txn.table::<ScanProgressRow>()
            .await
            .map_mm_err()?
            .clear()
            .await
            .map_mm_err()?;
        Ok(())
    }
}

/// Set a JSON string field to either `Some(value)` or JSON `null`,
/// matching the SQLite backend's `set_json_string` helper.
fn set_json_string(obj: &mut serde_json::Map<String, Json>, key: &str, value: &Option<String>) {
    match value {
        Some(v) => {
            obj.insert(key.to_string(), Json::String(v.clone()));
        },
        None => {
            obj.insert(key.to_string(), Json::Null);
        },
    }
}

/// Apply a [`TransferMeta`] back-fill to a stored transfer row. Updates
/// both the JSON payload (so the wire shape returned by `list_transfers`
/// reflects the new metadata) and the indexed `token_domain` /
/// `image_domain` / `possible_spam` columns, mirroring the SQLite
/// backend.
fn update_transfer_attach_metadata(
    row: &TransferRow,
    meta: &TransferMeta,
    flag_spam: bool,
) -> Result<TransferRow, MmError<IndexedDbStoreError>> {
    let mut payload: Json =
        serde_json::from_str(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    if let Some(obj) = payload.as_object_mut() {
        set_json_string(obj, "token_uri", &meta.token_uri);
        set_json_string(obj, "token_domain", &meta.token_domain);
        set_json_string(obj, "collection_name", &meta.collection_name);
        set_json_string(obj, "image_url", &meta.image_url);
        set_json_string(obj, "image_domain", &meta.image_domain);
        set_json_string(obj, "token_name", &meta.token_name);
        if let Some(common) = obj.get_mut("common").and_then(|c| c.as_object_mut()) {
            common.insert("possible_spam".to_string(), Json::Bool(flag_spam));
        }
        // Mirror the flattened spam flag used by the SQLite backend so
        // round-trip JSON deserialisation stays consistent.
        obj.insert("possible_spam".to_string(), Json::Bool(flag_spam));
    }
    let serialized = serde_json::to_string(&payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    let mut updated = row.clone();
    updated.token_domain = meta.token_domain.clone();
    updated.image_domain = meta.image_domain.clone();
    updated.possible_spam = u32::from(flag_spam);
    updated.payload = serialized;
    Ok(updated)
}

/// Flip the spam flag on a transfer row in-place; mirrors
/// [`crate::nft::store::idb::list::update_inventory_payload_spam`] but
/// for the transfer schema.
fn update_transfer_payload_spam(
    row: &TransferRow,
    possible_spam: bool,
) -> Result<TransferRow, MmError<IndexedDbStoreError>> {
    let mut payload: Json =
        serde_json::from_str(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    if let Some(obj) = payload.as_object_mut() {
        if let Some(common) = obj.get_mut("common").and_then(|c| c.as_object_mut()) {
            common.insert("possible_spam".to_string(), Json::Bool(possible_spam));
        }
        obj.insert("possible_spam".to_string(), Json::Bool(possible_spam));
    }
    let serialized = serde_json::to_string(&payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    let mut updated = row.clone();
    updated.possible_spam = u32::from(possible_spam);
    updated.payload = serialized;
    Ok(updated)
}

/// Flip the phishing flag on a transfer row in-place.
fn update_transfer_payload_phishing(
    row: &TransferRow,
    possible_phishing: bool,
) -> Result<TransferRow, MmError<IndexedDbStoreError>> {
    let mut payload: Json =
        serde_json::from_str(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("possible_phishing".to_string(), Json::Bool(possible_phishing));
    }
    let serialized = serde_json::to_string(&payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    let mut updated = row.clone();
    updated.possible_phishing = u32::from(possible_phishing);
    updated.payload = serialized;
    Ok(updated)
}
