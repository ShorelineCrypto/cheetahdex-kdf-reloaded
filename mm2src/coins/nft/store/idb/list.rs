//! `NftListStore` trait impl for [`IndexedDbNftStore`].
//!
//! This revision implements the read-side and chain-lifecycle subset of
//! the trait directly against the IndexedDB schema declared in
//! [`super::schema`]. The remaining mutating helpers
//! (`drop_token`, `merge_metadata`, `set_token_amount*`,
//! `tokens_for_contract`, `mark_*`, `list_external_domains`) are still
//! stubbed and surface the [`IndexedDbStoreError::Unimplemented`]
//! marker so the next slice has a clear surface to fill in.

use crate::nft::model::{Chain, Nft, NftList, NftListFilters};
use crate::nft::store::errors::RemoveOutcome;
use crate::nft::store::idb::schema::{InventoryRow, ScanProgressRow, TransferRow};
use crate::nft::store::idb::{chain_label, unimplemented, IndexedDbNftStore, IndexedDbStoreError};
use crate::nft::store::list::NftListStore;
use crate::nft::store::paginate;
use async_trait::async_trait;
use mm2_err_handle::prelude::*;
use mm2_number::BigUint;
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
        _chain: &Chain,
        _token_address: String,
        _token_id: BigUint,
        _scanned_block: u64,
    ) -> MmResult<RemoveOutcome, Self::Error> {
        unimplemented("NftListStore::drop_token")
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

    async fn merge_metadata(&self, _chain: &Chain, _nft: Nft) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::merge_metadata")
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

    async fn set_token_amount(&self, _chain: &Chain, _nft: Nft, _scanned_block: u64) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::set_token_amount")
    }

    async fn set_token_amount_and_block(&self, _chain: &Chain, _nft: Nft) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::set_token_amount_and_block")
    }

    async fn tokens_for_contract(&self, _chain: Chain, _token_address: String) -> MmResult<Vec<Nft>, Self::Error> {
        unimplemented("NftListStore::tokens_for_contract")
    }

    async fn mark_contract_spam(
        &self,
        _chain: &Chain,
        _token_address: String,
        _possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::mark_contract_spam")
    }

    async fn list_external_domains(&self, _chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        unimplemented("NftListStore::list_external_domains")
    }

    async fn mark_domain_phishing(
        &self,
        _chain: &Chain,
        _domain: String,
        _possible_phishing: bool,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::mark_domain_phishing")
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
