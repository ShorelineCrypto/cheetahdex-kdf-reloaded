//! Asynchronous trait describing storage of currently-owned NFTs.
//!
//! Implementations cache the wallet's NFT inventory per chain. They are
//! responsible for tracking the latest scanned block so that the providers
//! layer can resume incremental updates without reprocessing the entire
//! history.

use crate::nft::model::{Chain, Nft, NftList, NftListFilters};
use crate::nft::store::errors::{NftStoreError, RemoveOutcome};
use async_trait::async_trait;
use mm2_err_handle::prelude::MmResult;
use mm2_number::BigUint;
use std::collections::HashSet;
use std::num::NonZeroUsize;

/// Per-chain operations on the cached NFT inventory.
#[async_trait]
pub trait NftListStore {
    /// Backend-specific error type.
    type Error: NftStoreError;

    /// Initialise the per-chain tables/object stores. Idempotent.
    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Returns true when [`Self::ensure_chain`] has already been called for
    /// the given chain.
    async fn chain_ready(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    /// Read a paginated slice of the inventory.
    async fn list_owned(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error>;

    /// Insert or upsert a batch of NFT records and update the
    /// last-scanned-block bookmark.
    async fn register_owned(
        &self,
        chain: Chain,
        items: Vec<Nft>,
        last_scanned_block: u64,
    ) -> MmResult<(), Self::Error>;

    /// Look up a single token by `(chain, contract address, token id)`.
    async fn fetch_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<Nft>, Self::Error>;

    /// Remove a token from the inventory, advancing the last-scanned
    /// bookmark to `scanned_block`.
    async fn drop_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
        scanned_block: u64,
    ) -> MmResult<RemoveOutcome, Self::Error>;

    /// Read the cached `amount` field for a single token, if present.
    async fn token_balance(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<String>, Self::Error>;

    /// Replace an existing record with a refreshed metadata payload.
    async fn merge_metadata(&self, chain: &Chain, nft: Nft) -> MmResult<(), Self::Error>;

    /// Latest block number recorded in the inventory.
    async fn latest_block_in_cache(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// Latest block number observed by the providers layer (may be ahead of
    /// the inventory bookmark).
    async fn latest_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// Update the cached `amount` for a single token.
    async fn set_token_amount(
        &self,
        chain: &Chain,
        nft: Nft,
        scanned_block: u64,
    ) -> MmResult<(), Self::Error>;

    /// Update the cached `amount` and `block_number` for a single token in
    /// one go (used when a transfer changes both fields).
    async fn set_token_amount_and_block(&self, chain: &Chain, nft: Nft)
        -> MmResult<(), Self::Error>;

    /// All cached records that share a contract address.
    async fn tokens_for_contract(
        &self,
        chain: Chain,
        token_address: String,
    ) -> MmResult<Vec<Nft>, Self::Error>;

    /// Flip the spam flag for every record sharing a contract address.
    async fn mark_contract_spam(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error>;

    /// All unique animation/external domains observed on a chain.
    async fn list_external_domains(
        &self,
        chain: &Chain,
    ) -> MmResult<HashSet<String>, Self::Error>;

    /// Flip the phishing flag for every record sharing a metadata domain.
    async fn mark_domain_phishing(
        &self,
        chain: &Chain,
        domain: String,
        possible_phishing: bool,
    ) -> MmResult<(), Self::Error>;

    /// Wipe all NFT inventory data for one chain.
    async fn purge_chain(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Wipe NFT inventory data across every chain.
    async fn purge_all(&self) -> MmResult<(), Self::Error>;
}
