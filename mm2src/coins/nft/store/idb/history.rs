//! `NftHistoryStore` trait impl for [`IndexedDbNftStore`].
//!
//! Skeleton revision: every method is wired up to the trait surface but
//! returns the [`IndexedDbStoreError::Unimplemented`] marker. Real
//! IndexedDB queries follow in P10.3.4.b/.c.

use crate::nft::model::{Chain, NftTokenIdent, NftTransfer, NftTransferList, NftTransfersFilters, TransferMeta};
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::idb::{unimplemented, IndexedDbNftStore, IndexedDbStoreError};
use async_trait::async_trait;
use ethereum_types::Address;
use mm2_err_handle::prelude::MmResult;
use mm2_number::BigUint;
use std::collections::HashSet;
use std::num::NonZeroUsize;

#[async_trait]
impl NftHistoryStore for IndexedDbNftStore {
    type Error = IndexedDbStoreError;

    async fn ensure_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::ensure_chain")
    }

    async fn chain_ready(&self, _chain: &Chain) -> MmResult<bool, Self::Error> {
        unimplemented("NftHistoryStore::chain_ready")
    }

    async fn list_transfers(
        &self,
        _chains: Vec<Chain>,
        _take_all: bool,
        _page_size: usize,
        _page: Option<NonZeroUsize>,
        _filters: Option<NftTransfersFilters>,
    ) -> MmResult<NftTransferList, Self::Error> {
        unimplemented("NftHistoryStore::list_transfers")
    }

    async fn append_transfers(&self, _chain: Chain, _transfers: Vec<NftTransfer>) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::append_transfers")
    }

    async fn latest_transfer_block(&self, _chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        unimplemented("NftHistoryStore::latest_transfer_block")
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
        _chain: &Chain,
        _transaction_hash: String,
        _log_index: u32,
        _token_id: BigUint,
    ) -> MmResult<Option<NftTransfer>, Self::Error> {
        unimplemented("NftHistoryStore::transfer_by_log")
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

    async fn purge_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::purge_chain")
    }

    async fn purge_all(&self) -> MmResult<(), Self::Error> {
        unimplemented("NftHistoryStore::purge_all")
    }
}
