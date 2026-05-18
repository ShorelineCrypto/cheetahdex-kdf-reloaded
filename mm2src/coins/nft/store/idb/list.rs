//! `NftListStore` trait impl for [`IndexedDbNftStore`].
//!
//! Skeleton revision: every method is wired up to the trait surface but
//! returns the [`IndexedDbStoreError::Unimplemented`] marker. The
//! actual IndexedDB queries land in follow-up commits (P10.3.4.b for
//! the read-side, P10.3.4.c for mutations) so this commit can be
//! reviewed in isolation as a pure foundation change.

use crate::nft::model::{Chain, Nft, NftList, NftListFilters};
use crate::nft::store::errors::RemoveOutcome;
use crate::nft::store::idb::{unimplemented, IndexedDbNftStore, IndexedDbStoreError};
use crate::nft::store::list::NftListStore;
use async_trait::async_trait;
use mm2_err_handle::prelude::MmResult;
use mm2_number::BigUint;
use std::collections::HashSet;
use std::num::NonZeroUsize;

#[async_trait]
impl NftListStore for IndexedDbNftStore {
    type Error = IndexedDbStoreError;

    async fn ensure_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::ensure_chain")
    }

    async fn chain_ready(&self, _chain: &Chain) -> MmResult<bool, Self::Error> {
        unimplemented("NftListStore::chain_ready")
    }

    async fn list_owned(
        &self,
        _chains: Vec<Chain>,
        _take_all: bool,
        _page_size: usize,
        _page: Option<NonZeroUsize>,
        _filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error> {
        unimplemented("NftListStore::list_owned")
    }

    async fn register_owned(
        &self,
        _chain: Chain,
        _items: Vec<Nft>,
        _last_scanned_block: u64,
    ) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::register_owned")
    }

    async fn fetch_token(
        &self,
        _chain: &Chain,
        _token_address: String,
        _token_id: BigUint,
    ) -> MmResult<Option<Nft>, Self::Error> {
        unimplemented("NftListStore::fetch_token")
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
        _chain: &Chain,
        _token_address: String,
        _token_id: BigUint,
    ) -> MmResult<Option<String>, Self::Error> {
        unimplemented("NftListStore::token_balance")
    }

    async fn merge_metadata(&self, _chain: &Chain, _nft: Nft) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::merge_metadata")
    }

    async fn latest_block_in_cache(&self, _chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        unimplemented("NftListStore::latest_block_in_cache")
    }

    async fn latest_scanned_block(&self, _chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
        unimplemented("NftListStore::latest_scanned_block")
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

    async fn purge_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::purge_chain")
    }

    async fn purge_all(&self) -> MmResult<(), Self::Error> {
        unimplemented("NftListStore::purge_all")
    }
}
