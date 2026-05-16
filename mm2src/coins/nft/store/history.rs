//! Asynchronous trait describing storage of NFT transfer history.
//!
//! Implementations cache provider-supplied transfer logs per chain so that
//! the wallet can render the user's transaction history without re-fetching
//! the full archive on every poll.

use crate::nft::model::{Chain, NftTokenIdent, NftTransfer, NftTransferList, NftTransfersFilters, TransferMeta};
use crate::nft::store::errors::NftStoreError;
use async_trait::async_trait;
use ethereum_types::Address;
use mm2_err_handle::prelude::MmResult;
use mm2_number::BigUint;
use std::collections::HashSet;
use std::num::NonZeroUsize;

/// Per-chain operations on the cached NFT transfer history.
#[async_trait]
pub trait NftHistoryStore {
    /// Backend-specific error type.
    type Error: NftStoreError;

    /// Initialise the per-chain tables/object stores. Idempotent.
    async fn ensure_chain(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Returns true when [`Self::ensure_chain`] has already been called for
    /// the given chain.
    async fn chain_ready(&self, chain: &Chain) -> MmResult<bool, Self::Error>;

    /// Read a paginated slice of the transfer history.
    async fn list_transfers(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftTransfersFilters>,
    ) -> MmResult<NftTransferList, Self::Error>;

    /// Insert or upsert a batch of transfer records for a chain.
    async fn append_transfers(&self, chain: Chain, transfers: Vec<NftTransfer>) -> MmResult<(), Self::Error>;

    /// Latest block number recorded in the transfer history.
    async fn latest_transfer_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error>;

    /// All transfers at or after `from_block`, ordered by block ascending.
    async fn transfers_since(&self, chain: Chain, from_block: u64) -> MmResult<Vec<NftTransfer>, Self::Error>;

    /// All transfers that touched a specific `(contract address, token id)`.
    async fn transfers_for_token(
        &self,
        chain: Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Vec<NftTransfer>, Self::Error>;

    /// Look up a single transfer by `(transaction hash, log index, token id)`.
    async fn transfer_by_log(
        &self,
        chain: &Chain,
        transaction_hash: String,
        log_index: u32,
        token_id: BigUint,
    ) -> MmResult<Option<NftTransfer>, Self::Error>;

    /// Update the cached metadata fields (token name, image, …) for every
    /// transfer that touched a specific token.
    async fn attach_metadata_to_transfers(
        &self,
        chain: &Chain,
        meta: TransferMeta,
        flag_spam: bool,
    ) -> MmResult<(), Self::Error>;

    /// Identifiers of every transfer that still lacks metadata back-fills.
    async fn transfers_missing_metadata(&self, chain: Chain) -> MmResult<Vec<NftTokenIdent>, Self::Error>;

    /// All transfers that touched a specific contract address.
    async fn transfers_for_contract(
        &self,
        chain: Chain,
        token_address: String,
    ) -> MmResult<Vec<NftTransfer>, Self::Error>;

    /// Flip the spam flag for every transfer sharing a contract address.
    async fn mark_contract_spam(
        &self,
        chain: &Chain,
        token_address: String,
        possible_spam: bool,
    ) -> MmResult<(), Self::Error>;

    /// All unique contract addresses observed on a chain.
    async fn contract_addresses(&self, chain: Chain) -> MmResult<HashSet<Address>, Self::Error>;

    /// All unique metadata domains observed on a chain.
    async fn domain_set(&self, chain: &Chain) -> MmResult<HashSet<String>, Self::Error>;

    /// Flip the phishing flag for every transfer sharing a metadata domain.
    async fn mark_domain_phishing(
        &self,
        chain: &Chain,
        domain: String,
        possible_phishing: bool,
    ) -> MmResult<(), Self::Error>;

    /// Wipe all transfer history for one chain.
    async fn purge_chain(&self, chain: &Chain) -> MmResult<(), Self::Error>;

    /// Wipe transfer history across every chain.
    async fn purge_all(&self) -> MmResult<(), Self::Error>;
}
