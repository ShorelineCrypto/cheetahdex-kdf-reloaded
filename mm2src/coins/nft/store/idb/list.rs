//! `NftListStore` trait impl for [`IndexedDbNftStore`].
//!
//! This revision is the full IndexedDB implementation: lifecycle,
//! reads, and the mutating helpers that mark inventory rows as spam /
//! phishing or refresh their cached metadata.

use crate::nft::model::{Chain, Nft, NftList, NftListFilters};
use crate::nft::store::errors::RemoveOutcome;
use crate::nft::store::idb::schema::{InventoryRow, ScanProgressRow, TransferRow};
use crate::nft::store::idb::{chain_label, IndexedDbNftStore, IndexedDbStoreError};
use crate::nft::store::list::NftListStore;
use crate::nft::store::paginate;
use async_trait::async_trait;
use mm2_err_handle::prelude::*;
use mm2_number::BigUint;
use serde_json::Value as Json;
use std::collections::HashSet;
use std::num::NonZeroUsize;

/// Helper that turns a stored [`InventoryRow::payload`] back into the
/// canonical [`Nft`] value.
fn payload_to_nft(row: &InventoryRow) -> Result<Nft, MmError<IndexedDbStoreError>> {
    serde_json::from_str::<Nft>(&row.payload).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))
}

/// Build an [`InventoryRow`] from a fully-populated [`Nft`]. Mirrors the
/// SQLite backend's column extraction so the two backends stay in sync.
fn inventory_row_from_nft(chain: &Chain, nft: &Nft) -> Result<InventoryRow, MmError<IndexedDbStoreError>> {
    let payload = serde_json::to_string(nft).map_err(|e| MmError::new(IndexedDbStoreError::from(e)))?;
    Ok(InventoryRow {
        chain: chain_label(chain),
        token_address: format!("{:?}", nft.common.token_address),
        token_id_str: nft.token_id.to_string(),
        block_number: nft.block_number,
        possible_spam: u32::from(nft.common.possible_spam),
        possible_phishing: u32::from(nft.possible_phishing),
        contract_type: nft.contract_type.to_string(),
        image_domain: nft.uri_meta.image_domain.clone(),
        animation_domain: nft.uri_meta.animation_domain.clone(),
        external_domain: nft.uri_meta.external_domain.clone(),
        payload,
    })
}

/// Apply the `NftListFilters` to an in-memory inventory row.
fn passes_list_filters(row: &InventoryRow, filters: &Option<NftListFilters>) -> bool {
    if let Some(f) = filters {
        if f.exclude_spam && row.possible_spam != 0 {
            return false;
        }
        if f.exclude_phishing && row.possible_phishing != 0 {
            return false;
        }
    }
    true
}

#[async_trait]
impl NftListStore for IndexedDbNftStore {
    type Error = IndexedDbStoreError;

    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        let chain_str = chain_label(chain);
        let existing = table
            .get_item_by_unique_index("chain", chain_str.clone())
            .await
            .map_mm_err()?;
        if existing.is_none() {
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
        let row = table
            .get_item_by_unique_index("chain", chain_label(chain))
            .await
            .map_mm_err()?;
        Ok(row.is_some())
    }

    async fn list_owned(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error> {
        if chains.is_empty() {
            return Ok(NftList {
                nfts: vec![],
                skipped: 0,
                total: 0,
            });
        }
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        // Pull every row for the requested chains. IndexedDB does not
        // support "OR" across chain values cheaply, so we issue one
        // index lookup per chain and concatenate.
        let mut rows: Vec<InventoryRow> = Vec::new();
        let mut unfiltered_total: usize = 0;
        for chain in &chains {
            let chain_rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
            unfiltered_total = unfiltered_total.saturating_add(chain_rows.len());
            for (_id, row) in chain_rows {
                if passes_list_filters(&row, &filters) {
                    rows.push(row);
                }
            }
        }
        let total = rows.len();
        let skipped = unfiltered_total.saturating_sub(total);
        // Sort newest-first by block_number to match the SQLite ordering.
        rows.sort_by(|a, b| b.block_number.cmp(&a.block_number));
        let (offset, limit) = paginate(take_all, page_size, page, total);
        let mut nfts = Vec::with_capacity(limit.min(total.saturating_sub(offset)));
        for row in rows.into_iter().skip(offset).take(limit) {
            nfts.push(payload_to_nft(&row)?);
        }
        Ok(NftList { nfts, skipped, total })
    }

    async fn register_owned(
        &self,
        chain: Chain,
        items: Vec<Nft>,
        last_scanned_block: u64,
    ) -> MmResult<(), Self::Error> {
        let chain_str = chain_label(&chain);
        // Pre-build all rows in the caller thread so we surface
        // serialisation errors before opening the DB transaction.
        let mut prepared = Vec::with_capacity(items.len());
        for nft in &items {
            prepared.push(inventory_row_from_nft(&chain, nft)?);
        }
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let inv_table = txn.table::<InventoryRow>().await.map_mm_err()?;
        for row in &prepared {
            inv_table
                .replace_item_by_unique_index(
                    "chain_contract_token",
                    vec![row.chain.clone(), row.token_address.clone(), row.token_id_str.clone()],
                    row,
                )
                .await
                .map_mm_err()?;
        }
        let scan_table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        let new_row = ScanProgressRow {
            chain: chain_str.clone(),
            last_scanned_block,
        };
        scan_table
            .replace_item_by_unique_index("chain", chain_str, &new_row)
            .await
            .map_mm_err()?;
        Ok(())
    }

    async fn fetch_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<Nft>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let key = vec![chain_label(chain), token_address, token_id.to_string()];
        let row = table
            .get_item_by_unique_index("chain_contract_token", key)
            .await
            .map_mm_err()?;
        match row {
            Some((_id, r)) => Ok(Some(payload_to_nft(&r)?)),
            None => Ok(None),
        }
    }

    async fn drop_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
        scanned_block: u64,
    ) -> MmResult<RemoveOutcome, Self::Error> {
        let chain_str = chain_label(chain);
        let key = vec![chain_str.clone(), token_address, token_id.to_string()];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let inv_table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let removed_ids = inv_table
            .delete_items_by_index("chain_contract_token", key)
            .await
            .map_mm_err()?;
        let scan_table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        let new_row = ScanProgressRow {
            chain: chain_str.clone(),
            last_scanned_block: scanned_block,
        };
        scan_table
            .replace_item_by_unique_index("chain", chain_str, &new_row)
            .await
            .map_mm_err()?;
        Ok(if removed_ids.is_empty() {
            RemoveOutcome::Absent
        } else {
            RemoveOutcome::Removed
        })
    }

    async fn token_balance(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<String>, Self::Error> {
        let nft = self.fetch_token(chain, token_address, token_id).await.map_mm_err()?;
        Ok(nft.map(|n| n.common.amount.to_string()))
    }

    async fn merge_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        // Mirrors the SQLite backend: the providers layer is responsible
        // for merging the metadata into the `Nft` value before calling us;
        // we simply replace the existing record.
        let block = nft.block_number;
        self.register_owned(*chain, vec![nft], block).await
    }

    async fn latest_block_in_cache(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        Ok(rows.into_iter().map(|(_id, r)| r.block_number).max())
    }

    async fn latest_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<ScanProgressRow>().await.map_mm_err()?;
        let row = table
            .get_item_by_unique_index("chain", chain_label(chain))
            .await
            .map_mm_err()?;
        Ok(row.map(|(_id, r)| r.last_scanned_block))
    }

    async fn set_token_amount(&self, chain: &Chain, nft: Nft, scanned_block: u64) -> MmResult<(), Self::Error> {
        self.register_owned(*chain, vec![nft], scanned_block).await
    }

    async fn set_token_amount_and_block(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error> {
        let block = nft.block_number;
        self.register_owned(*chain, vec![nft], block).await
    }

    async fn tokens_for_contract(&self, chain: Chain, token_address: String) -> MmResult<Vec<Nft>, Self::Error> {
        let key = vec![chain_label(&chain), token_address];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
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
            out.push(payload_to_nft(row)?);
        }
        Ok(out)
    }

    async fn mark_contract_spam(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        let chain_str = chain_label(chain);
        let key = vec![chain_str.clone(), token_address];
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let rows = table.get_items("chain_contract", key).await.map_mm_err()?;
        for (_id, row) in rows {
            let updated = update_inventory_payload_spam(&row, possible_spam)?;
            table
                .replace_item_by_unique_index(
                    "chain_contract_token",
                    vec![
                        updated.chain.clone(),
                        updated.token_address.clone(),
                        updated.token_id_str.clone(),
                    ],
                    &updated,
                )
                .await
                .map_mm_err()?;
        }
        Ok(())
    }

    async fn list_external_domains(&self, chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        let mut out = HashSet::new();
        for (_id, row) in rows {
            if let Some(d) = row.image_domain {
                out.insert(d);
            }
            if let Some(d) = row.animation_domain {
                out.insert(d);
            }
            if let Some(d) = row.external_domain {
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
        let table = txn.table::<InventoryRow>().await.map_mm_err()?;
        let rows = table.get_items("chain", chain_label(chain)).await.map_mm_err()?;
        for (_id, row) in rows {
            let matches_domain = row.image_domain.as_deref() == Some(domain.as_str())
                || row.animation_domain.as_deref() == Some(domain.as_str())
                || row.external_domain.as_deref() == Some(domain.as_str());
            if !matches_domain {
                continue;
            }
            let updated = update_inventory_payload_phishing(&row, possible_phishing)?;
            table
                .replace_item_by_unique_index(
                    "chain_contract_token",
                    vec![
                        updated.chain.clone(),
                        updated.token_address.clone(),
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
        let chain_str = chain_label(chain);
        let locked = self.lock_db().await.map_mm_err()?;
        let txn = locked.inner.transaction().await.map_mm_err()?;
        txn.table::<InventoryRow>()
            .await
            .map_mm_err()?
            .delete_items_by_index("chain", chain_str.clone())
            .await
            .map_mm_err()?;
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
        txn.table::<InventoryRow>()
            .await
            .map_mm_err()?
            .clear()
            .await
            .map_mm_err()?;
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

/// Flip the spam flag on an inventory row in-place and re-serialise the
/// JSON payload so the row column and the embedded `Nft` value stay
/// consistent. Mirrors the SQLite backend, which mutates both the
/// `possible_spam` column and the embedded `common.possible_spam` /
/// top-level `possible_spam` JSON fields.
fn update_inventory_payload_spam(
    row: &InventoryRow,
    possible_spam: bool,
) -> Result<InventoryRow, MmError<IndexedDbStoreError>> {
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

/// Flip the phishing flag on an inventory row in-place. Mirrors
/// [`update_inventory_payload_spam`] but updates the
/// `possible_phishing` column / JSON field instead.
fn update_inventory_payload_phishing(
    row: &InventoryRow,
    possible_phishing: bool,
) -> Result<InventoryRow, MmError<IndexedDbStoreError>> {
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
