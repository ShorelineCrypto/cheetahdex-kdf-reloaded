//! `NftHistoryStore` trait impl for [`IndexedDbNftStore`].
//!
//! This revision implements the read-side and chain-lifecycle subset
//! against the IndexedDB schema. Mutating helpers other than
//! `append_transfers` are still stubbed and surface
//! [`IndexedDbStoreError::Unimplemented`] until the next slice.

use crate::nft::model::{Chain, NftTokenIdent, NftTransfer, NftTransferList, NftTransfersFilters, TransferMeta};
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::idb::schema::{ScanProgressRow, TransferRow};
use crate::nft::store::idb::{chain_label, unimplemented, IndexedDbNftStore, IndexedDbStoreError};
use crate::nft::store::paginate;
use async_trait::async_trait;
use ethereum_types::Address;
use mm2_err_handle::prelude::*;
use mm2_number::BigUint;
use std::collections::HashSet;
use std::num::NonZeroUsize;

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

    async fn transfers_since(&self, _chain: Chain, _from_block: u64) -> MmResult<Vec<NftTransfer>, Self::Error> {
        unimplemented("NftHistoryStore::transfers_since")
    }

    async fn transfers_for_token(
        &self,
        _chain: Chain,
        _token_address: String,
        _token_id: BigUint,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        unimplemented("NftHistoryStore::transfers_for_token")
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
        _chain: &Chain,
        _meta: TransferMeta,
        _flag_spam: bool,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::attach_metadata_to_transfers")
    }

    async fn transfers_missing_metadata(&self, _chain: Chain) -> MmResult<Vec<NftTokenIdent>, Self::Error> {
        unimplemented("NftHistoryStore::transfers_missing_metadata")
    }

    async fn transfers_for_contract(
        &self,
        _chain: Chain,
        _token_address: String,
    ) -> MmResult<Vec<NftTransfer>, Self::Error> {
        unimplemented("NftHistoryStore::transfers_for_contract")
    }

    async fn mark_contract_spam(
        &self,
        _chain: &Chain,
        _token_address: String,
        _possible_spam: bool,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::mark_contract_spam")
    }

    async fn contract_addresses(&self, _chain: Chain) -> MmResult<HashSet<Address>, Self::Error> {
        unimplemented("NftHistoryStore::contract_addresses")
    }

    async fn domain_set(&self, _chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
        unimplemented("NftHistoryStore::domain_set")
    }

    async fn mark_domain_phishing(
        &self,
        _chain: &Chain,
        _domain: String,
        _possible_phishing: bool,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::mark_domain_phishing")
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
