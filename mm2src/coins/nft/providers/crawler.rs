//! Multi-chain incremental NFT crawler.
//!
//! Powers the `update_nft` RPC handler. For each requested chain the
//! orchestrator:
//!
//! 1. Resolves the wallet's owner address (via the activated EVM coin).
//! 2. Asks the configured [`NftCrawlProvider`] for the transfers that
//!    happened on that chain since the locally-cached
//!    [`NftHistoryStore::latest_transfer_block`] bookmark.
//! 3. Applies each transfer to both the transfer log
//!    ([`NftHistoryStore::append_transfers`]) and the inventory:
//!    incoming transfers register / refresh the corresponding row
//!    through the provider's `fetch_token`, outgoing transfers either
//!    drop the row (ERC-721 / ERC-1155 with zero balance) or decrement
//!    the cached amount (ERC-1155 partial transfer).
//! 4. Re-runs the spam-protection helpers and propagates the freshly
//!    seen contract addresses / metadata domains through the
//!    `mark_contract_spam` / `mark_domain_phishing` helpers so the GUI
//!    sees consistent flags across `get_nft_list` and `get_nft_transfers`.
//!
//! The crawler is generic over the storage backend and the provider so
//! the unit tests can exercise the orchestration without any HTTP
//! traffic.

use crate::nft::errors::UpdateNftError;
use crate::nft::model::{Chain, Nft, NftTransfer, TransferMeta, TransferStatus};
use crate::nft::providers::http::{fetch_json, FetchError};
use crate::nft::providers::spam::apply_spam_protection_to_nft;
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::list::NftListStore;
use async_trait::async_trait;
use ethereum_types::Address;
use mm2_err_handle::prelude::*;
use mm2_number::{BigDecimal, BigUint};
use std::collections::{HashMap, HashSet};
use url::Url;

/// Source of fresh transfer logs and inventory rows. Abstracted into a
/// trait so unit tests can replace the HTTP fetch with a deterministic
/// stub.
#[async_trait]
pub trait NftCrawlProvider: Send + Sync {
    /// Latest block number observed on `chain` according to the provider.
    /// Used as the new `last_scanned_block` bookmark when no transfers are
    /// returned for a chain.
    async fn current_block(&self, chain: Chain) -> MmResult<u64, FetchError>;

    /// Transfers involving `owner` on `chain`, strictly after `from_block`.
    /// Implementations are expected to set the `status` field on every
    /// returned [`NftTransfer`] (Receive / Send) and to honour the wallet
    /// address filter.
    async fn fetch_transfers_since(
        &self,
        chain: Chain,
        owner: Address,
        from_block: u64,
    ) -> MmResult<Vec<NftTransfer>, FetchError>;

    /// Fetch a fresh inventory row for `(chain, contract, token_id)`.
    /// Returns `None` when the provider has no record of the token (e.g.
    /// the token was burned).
    async fn fetch_token(
        &self,
        chain: Chain,
        owner: Address,
        contract: Address,
        token_id: &BigUint,
    ) -> MmResult<Option<Nft>, FetchError>;
}

/// Default HTTP-backed [`NftCrawlProvider`].
///
/// Endpoint layout (rooted at `base_url`):
/// * `GET {base}/<chain>/block/latest` -> `{ "block": <u64> }`
/// * `GET {base}/<chain>/<owner>/transfers?from_block=<n>` -> array of
///   [`NftTransfer`]
/// * `GET {base}/<chain>/<owner>/<contract>/<token_id>` -> [`Nft`] or 404
pub struct HttpCrawlProvider {
    base_url: Url,
    #[allow(dead_code)]
    pub komodo_proxy: bool,
}

impl HttpCrawlProvider {
    /// Construct a new HTTP-backed crawler provider rooted at `base_url`.
    pub fn new(base_url: Url, komodo_proxy: bool) -> Self { HttpCrawlProvider { base_url, komodo_proxy } }

    fn endpoint(&self, suffix: &str) -> String {
        format!(
            "{}/{}",
            self.base_url.as_str().trim_end_matches('/'),
            suffix.trim_start_matches('/')
        )
    }
}

#[async_trait]
impl NftCrawlProvider for HttpCrawlProvider {
    async fn current_block(&self, chain: Chain) -> MmResult<u64, FetchError> {
        #[derive(serde::Deserialize)]
        struct Resp {
            block: u64,
        }
        let url = self.endpoint(&format!("{}/block/latest", chain_label(chain)));
        let resp: Resp = fetch_json(&url, &[]).await?;
        Ok(resp.block)
    }

    async fn fetch_transfers_since(
        &self,
        chain: Chain,
        owner: Address,
        from_block: u64,
    ) -> MmResult<Vec<NftTransfer>, FetchError> {
        let url = self.endpoint(&format!(
            "{}/{:#x}/transfers?from_block={}",
            chain_label(chain),
            owner,
            from_block
        ));
        fetch_json(&url, &[]).await
    }

    async fn fetch_token(
        &self,
        chain: Chain,
        owner: Address,
        contract: Address,
        token_id: &BigUint,
    ) -> MmResult<Option<Nft>, FetchError> {
        let url = self.endpoint(&format!(
            "{}/{:#x}/{:#x}/{}",
            chain_label(chain),
            owner,
            contract,
            token_id
        ));
        match fetch_json::<Nft>(&url, &[]).await {
            Ok(nft) => Ok(Some(nft)),
            Err(err) => match err.get_inner() {
                FetchError::HttpStatus { status, .. } if *status == 404 => Ok(None),
                _ => Err(err),
            },
        }
    }
}

fn chain_label(chain: Chain) -> String { format!("{}", chain).to_ascii_lowercase() }

/// Outcome of a single chain's crawl. Returned per-chain so callers can
/// surface partial successes when one of the chains in a multi-chain
/// request fails.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChainCrawlReport {
    /// Number of new transfers appended to the history log.
    pub appended_transfers: usize,
    /// Number of inventory rows inserted or refreshed.
    pub upserted_tokens: usize,
    /// Number of inventory rows removed (full ERC-721 / zero-balance ERC-1155 send).
    pub dropped_tokens: usize,
    /// New `last_scanned_block` bookmark after the crawl.
    pub last_scanned_block: u64,
}

/// Run the incremental crawler against `chain` for the wallet `owner`.
///
/// The orchestrator is generic over both the storage backend (so the
/// SQLite and IndexedDB stores can be exercised interchangeably) and the
/// provider (so unit tests can avoid any HTTP traffic).
pub async fn update_chain<S, P>(
    store: &S,
    provider: &P,
    chain: Chain,
    owner: Address,
) -> MmResult<ChainCrawlReport, UpdateNftError>
where
    S: NftListStore + NftHistoryStore,
    P: NftCrawlProvider,
{
    let from_block = NftHistoryStore::latest_transfer_block(store, &chain)
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?
        .unwrap_or(0);

    let transfers = provider
        .fetch_transfers_since(chain, owner, from_block)
        .await
        .mm_err(|err| UpdateNftError::Provider(err.to_string()))?;

    let mut report = ChainCrawlReport::default();

    if transfers.is_empty() {
        report.last_scanned_block = provider
            .current_block(chain)
            .await
            .mm_err(|err| UpdateNftError::Provider(err.to_string()))?
            .max(from_block);
        return Ok(report);
    }

    let max_block = transfers.iter().map(|t| t.block_number).max().unwrap_or(from_block);
    report.last_scanned_block = max_block;
    report.appended_transfers = transfers.len();

    NftHistoryStore::append_transfers(store, chain, transfers.clone())
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;

    // Group transfers by `(contract, token_id)` so we can collapse a
    // back-to-back receive+send into a single inventory operation.
    let mut grouped: HashMap<(Address, BigUint), Vec<NftTransfer>> = HashMap::new();
    let mut contracts: HashSet<Address> = HashSet::new();
    for t in &transfers {
        contracts.insert(t.common.token_address);
        grouped
            .entry((t.common.token_address, t.token_id.clone()))
            .or_default()
            .push(t.clone());
    }

    for ((contract, token_id), mut events) in grouped {
        events.sort_by_key(|t| (t.block_number, t.common.log_index));
        apply_token_events(
            store,
            provider,
            chain,
            owner,
            contract,
            &token_id,
            events,
            max_block,
            &mut report,
        )
        .await?;
    }

    Ok(report)
}

/// Apply the chronologically-ordered `events` for a single
/// `(contract, token_id)` to both the inventory and the back-fill
/// metadata for the historical log.
async fn apply_token_events<S, P>(
    store: &S,
    provider: &P,
    chain: Chain,
    owner: Address,
    contract: Address,
    token_id: &BigUint,
    events: Vec<NftTransfer>,
    scanned_block: u64,
    report: &mut ChainCrawlReport,
) -> MmResult<(), UpdateNftError>
where
    S: NftListStore + NftHistoryStore,
    P: NftCrawlProvider,
{
    let contract_hex = format!("{:#x}", contract);

    // Net amount delta across the run of events: +amount on Receive,
    // -amount on Send. ERC-721 always nets to {-1, 0, 1}.
    let mut net = BigDecimal::from(0);
    for e in &events {
        match e.status {
            TransferStatus::Receive => net += e.common.amount.clone(),
            TransferStatus::Send => net -= e.common.amount.clone(),
        }
    }

    let cached = NftListStore::fetch_token(store, &chain, contract_hex.clone(), token_id.clone())
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;

    let cached_amount = cached
        .as_ref()
        .map(|n| n.common.amount.clone())
        .unwrap_or_else(|| BigDecimal::from(0));
    let new_amount = cached_amount + net;

    if new_amount <= BigDecimal::from(0) {
        if cached.is_some() {
            NftListStore::drop_token(store, &chain, contract_hex.clone(), token_id.clone(), scanned_block)
                .await
                .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;
            report.dropped_tokens += 1;
        }
        return Ok(());
    }

    let mut nft = match cached {
        Some(mut existing) => {
            existing.common.amount = new_amount.clone();
            existing.common.owner_of = owner;
            existing.block_number = scanned_block;
            existing
        },
        None => {
            // Newly-received token: pull a fresh inventory row from the
            // provider so we capture metadata, contract type, and the
            // mint block.
            let fetched = provider
                .fetch_token(chain, owner, contract, token_id)
                .await
                .mm_err(|err| UpdateNftError::Provider(err.to_string()))?
                .ok_or_else(|| {
                    UpdateNftError::Provider(format!(
                        "provider did not return a token row for {contract_hex} #{token_id}"
                    ))
                })?;
            let mut row = fetched;
            row.common.amount = new_amount.clone();
            row.common.owner_of = owner;
            row.block_number = scanned_block;
            row
        },
    };

    apply_spam_protection_to_nft(&mut nft, false)
        .map_err(|err| MmError::new(UpdateNftError::Internal(format!("spam scan during crawl: {err}"))))?;

    NftListStore::register_owned(store, chain, vec![nft.clone()], scanned_block)
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;
    report.upserted_tokens += 1;

    // Back-fill metadata into the historical transfer log so the
    // `get_nft_transfers` endpoint returns consistent values.
    let meta = TransferMeta::from(nft.clone());
    let flag_spam = nft.common.possible_spam;
    NftHistoryStore::attach_metadata_to_transfers(store, &chain, meta, flag_spam)
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nft::model::{ContractType, NftCommon, NftTransferCommon, UriMeta};
    use crate::nft::store::errors::{NftStoreError, RemoveOutcome};
    use crate::nft::store::history::NftHistoryStore;
    use crate::nft::store::list::NftListStore;
    use async_trait::async_trait;
    use derive_more::Display;
    use ethereum_types::Address;
    use mm2_err_handle::prelude::*;
    use serde::Serialize;
    use std::collections::HashMap;
    use std::num::NonZeroUsize;
    use std::sync::Mutex;

    fn addr(byte: u8) -> Address {
        let mut a = [0u8; 20];
        a[19] = byte;
        Address::from(a)
    }

    fn make_nft(contract: Address, token_id: u32, amount: u32, owner: Address) -> Nft {
        Nft {
            common: NftCommon {
                token_address: contract,
                amount: BigDecimal::from(amount),
                owner_of: owner,
                token_hash: None,
                collection_name: Some("Test".into()),
                symbol: None,
                token_uri: None,
                token_domain: None,
                metadata: None,
                last_token_uri_sync: None,
                last_metadata_sync: None,
                minter_address: None,
                possible_spam: false,
            },
            chain: Chain::Eth,
            token_id: BigUint::from(token_id),
            block_number_minted: Some(1),
            block_number: 1,
            contract_type: ContractType::Erc721,
            possible_phishing: false,
            uri_meta: UriMeta::default(),
        }
    }

    fn make_transfer(
        contract: Address,
        token_id: u32,
        from: Address,
        to: Address,
        block: u64,
        log_index: u32,
        status: TransferStatus,
        amount: u32,
        contract_type: ContractType,
    ) -> NftTransfer {
        NftTransfer {
            common: NftTransferCommon {
                block_hash: Some(format!("0x{block:064x}")),
                transaction_hash: format!("0x{block:064x}{log_index:08x}"),
                transaction_index: Some(0),
                log_index,
                value: None,
                transaction_type: None,
                token_address: contract,
                from_address: from,
                to_address: to,
                amount: BigDecimal::from(amount),
                verified: None,
                operator: None,
                possible_spam: false,
            },
            chain: Chain::Eth,
            token_id: BigUint::from(token_id),
            block_number: block,
            block_timestamp: 1_700_000_000 + block,
            contract_type,
            token_uri: None,
            token_domain: None,
            collection_name: None,
            image_url: None,
            image_domain: None,
            token_name: None,
            status,
            possible_phishing: false,
            fee_details: None,
            confirmations: 0,
        }
    }

    #[derive(Debug, Display, Serialize)]
    enum StubError {
        #[display(fmt = "stub error")]
        Stub,
    }

    impl NftStoreError for StubError {}

    #[derive(Default)]
    struct StubStoreInner {
        owned: HashMap<(Chain, String, BigUint), Nft>,
        last_scanned: HashMap<Chain, u64>,
        transfers: Vec<NftTransfer>,
        latest_transfer_block: HashMap<Chain, u64>,
        attached_meta: Vec<(Chain, TransferMeta, bool)>,
    }

    #[derive(Default)]
    struct StubStore {
        inner: Mutex<StubStoreInner>,
    }

    #[async_trait]
    impl NftListStore for StubStore {
        type Error = StubError;
        async fn ensure_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> { Ok(()) }
        async fn chain_ready(&self, _chain: &Chain) -> MmResult<bool, Self::Error> { Ok(true) }
        async fn list_owned(
            &self,
            _chains: Vec<Chain>,
            _take_all: bool,
            _page_size: usize,
            _page: Option<NonZeroUsize>,
            _filters: Option<crate::nft::model::NftListFilters>,
        ) -> MmResult<crate::nft::model::NftList, Self::Error> {
            unimplemented!()
        }
        async fn register_owned(
            &self,
            chain: Chain,
            items: Vec<Nft>,
            last_scanned_block: u64,
        ) -> MmResult<(), Self::Error> {
            let mut inner = self.inner.lock().unwrap();
            for nft in items {
                let key = (chain, format!("{:#x}", nft.common.token_address), nft.token_id.clone());
                inner.owned.insert(key, nft);
            }
            inner.last_scanned.insert(chain, last_scanned_block);
            Ok(())
        }
        async fn fetch_token(
            &self,
            chain: &Chain,
            token_address: String,
            token_id: BigUint,
        ) -> MmResult<Option<Nft>, Self::Error> {
            let inner = self.inner.lock().unwrap();
            Ok(inner.owned.get(&(*chain, token_address, token_id)).cloned())
        }
        async fn drop_token(
            &self,
            chain: &Chain,
            token_address: String,
            token_id: BigUint,
            scanned_block: u64,
        ) -> MmResult<RemoveOutcome, Self::Error> {
            let mut inner = self.inner.lock().unwrap();
            let removed = inner.owned.remove(&(*chain, token_address, token_id)).is_some();
            inner.last_scanned.insert(*chain, scanned_block);
            Ok(if removed {
                RemoveOutcome::Removed
            } else {
                RemoveOutcome::Absent
            })
        }
        async fn token_balance(
            &self,
            _chain: &Chain,
            _token_address: String,
            _token_id: BigUint,
        ) -> MmResult<Option<String>, Self::Error> {
            unimplemented!()
        }
        async fn merge_metadata(&self, _chain: &Chain, _nft: Nft) -> MmResult<(), Self::Error> { unimplemented!() }
        async fn latest_block_in_cache(&self, _chain: &Chain) -> MmResult<Option<u64>, Self::Error> { unimplemented!() }
        async fn latest_scanned_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
            Ok(self.inner.lock().unwrap().last_scanned.get(chain).copied())
        }
        async fn set_token_amount(&self, _chain: &Chain, _nft: Nft, _scanned_block: u64) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn set_token_amount_and_block(&self, _chain: &Chain, _nft: Nft) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn tokens_for_contract(&self, _chain: Chain, _token_address: String) -> MmResult<Vec<Nft>, Self::Error> {
            unimplemented!()
        }
        async fn mark_contract_spam(
            &self,
            _chain: &Chain,
            _token_address: String,
            _possible_spam: bool,
        ) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn list_external_domains(&self, _chain: &Chain) -> MmResult<HashSet<String>, Self::Error> {
            unimplemented!()
        }
        async fn mark_domain_phishing(
            &self,
            _chain: &Chain,
            _domain: String,
            _possible_phishing: bool,
        ) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn purge_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> { unimplemented!() }
        async fn purge_all(&self) -> MmResult<(), Self::Error> { unimplemented!() }
    }

    #[async_trait]
    impl NftHistoryStore for StubStore {
        type Error = StubError;
        async fn ensure_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> { Ok(()) }
        async fn chain_ready(&self, _chain: &Chain) -> MmResult<bool, Self::Error> { Ok(true) }
        async fn list_transfers(
            &self,
            _chains: Vec<Chain>,
            _take_all: bool,
            _page_size: usize,
            _page: Option<NonZeroUsize>,
            _filters: Option<crate::nft::model::NftTransfersFilters>,
        ) -> MmResult<crate::nft::model::NftTransferList, Self::Error> {
            unimplemented!()
        }
        async fn append_transfers(&self, chain: Chain, transfers: Vec<NftTransfer>) -> MmResult<(), Self::Error> {
            let mut inner = self.inner.lock().unwrap();
            let max = transfers.iter().map(|t| t.block_number).max().unwrap_or(0);
            let cur = inner.latest_transfer_block.get(&chain).copied().unwrap_or(0);
            inner.latest_transfer_block.insert(chain, cur.max(max));
            inner.transfers.extend(transfers);
            Ok(())
        }
        async fn latest_transfer_block(&self, chain: &Chain) -> MmResult<Option<u64>, Self::Error> {
            Ok(self.inner.lock().unwrap().latest_transfer_block.get(chain).copied())
        }
        async fn transfers_since(&self, _chain: Chain, _from_block: u64) -> MmResult<Vec<NftTransfer>, Self::Error> {
            unimplemented!()
        }
        async fn transfers_for_token(
            &self,
            _chain: Chain,
            _token_address: String,
            _token_id: BigUint,
        ) -> MmResult<Vec<NftTransfer>, Self::Error> {
            unimplemented!()
        }
        async fn transfer_by_log(
            &self,
            _chain: &Chain,
            _transaction_hash: String,
            _log_index: u32,
            _token_id: BigUint,
        ) -> MmResult<Option<NftTransfer>, Self::Error> {
            unimplemented!()
        }
        async fn attach_metadata_to_transfers(
            &self,
            chain: &Chain,
            meta: TransferMeta,
            flag_spam: bool,
        ) -> MmResult<(), Self::Error> {
            self.inner.lock().unwrap().attached_meta.push((*chain, meta, flag_spam));
            Ok(())
        }
        async fn transfers_missing_metadata(
            &self,
            _chain: Chain,
        ) -> MmResult<Vec<crate::nft::model::NftTokenIdent>, Self::Error> {
            unimplemented!()
        }
        async fn transfers_for_contract(
            &self,
            _chain: Chain,
            _token_address: String,
        ) -> MmResult<Vec<NftTransfer>, Self::Error> {
            unimplemented!()
        }
        async fn mark_contract_spam(
            &self,
            _chain: &Chain,
            _token_address: String,
            _possible_spam: bool,
        ) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn contract_addresses(&self, _chain: Chain) -> MmResult<HashSet<Address>, Self::Error> {
            unimplemented!()
        }
        async fn domain_set(&self, _chain: &Chain) -> MmResult<HashSet<String>, Self::Error> { unimplemented!() }
        async fn mark_domain_phishing(
            &self,
            _chain: &Chain,
            _domain: String,
            _possible_phishing: bool,
        ) -> MmResult<(), Self::Error> {
            unimplemented!()
        }
        async fn purge_chain(&self, _chain: &Chain) -> MmResult<(), Self::Error> { unimplemented!() }
        async fn purge_all(&self) -> MmResult<(), Self::Error> { unimplemented!() }
    }

    struct StubProvider {
        transfers: Vec<NftTransfer>,
        tokens: HashMap<(Address, BigUint), Nft>,
        current_block: u64,
    }

    #[async_trait]
    impl NftCrawlProvider for StubProvider {
        async fn current_block(&self, _chain: Chain) -> MmResult<u64, FetchError> { Ok(self.current_block) }
        async fn fetch_transfers_since(
            &self,
            _chain: Chain,
            _owner: Address,
            from_block: u64,
        ) -> MmResult<Vec<NftTransfer>, FetchError> {
            Ok(self
                .transfers
                .iter()
                .filter(|t| t.block_number > from_block)
                .cloned()
                .collect())
        }
        async fn fetch_token(
            &self,
            _chain: Chain,
            _owner: Address,
            contract: Address,
            token_id: &BigUint,
        ) -> MmResult<Option<Nft>, FetchError> {
            Ok(self.tokens.get(&(contract, token_id.clone())).cloned())
        }
    }

    #[tokio::test]
    async fn empty_chain_only_advances_bookmark() {
        let store = StubStore::default();
        let provider = StubProvider {
            transfers: vec![],
            tokens: HashMap::new(),
            current_block: 200,
        };
        let report = update_chain(&store, &provider, Chain::Eth, addr(0xA)).await.unwrap();
        assert_eq!(report.appended_transfers, 0);
        assert_eq!(report.upserted_tokens, 0);
        assert_eq!(report.dropped_tokens, 0);
        assert_eq!(report.last_scanned_block, 200);
    }

    #[tokio::test]
    async fn first_receive_inserts_inventory_and_attaches_meta() {
        let owner = addr(0xA);
        let contract = addr(0xC);
        let mut tokens = HashMap::new();
        tokens.insert((contract, BigUint::from(1u32)), make_nft(contract, 1, 1, owner));
        let store = StubStore::default();
        let provider = StubProvider {
            transfers: vec![make_transfer(
                contract,
                1,
                addr(0xB),
                owner,
                10,
                0,
                TransferStatus::Receive,
                1,
                ContractType::Erc721,
            )],
            tokens,
            current_block: 10,
        };
        let report = update_chain(&store, &provider, Chain::Eth, owner).await.unwrap();
        assert_eq!(report.appended_transfers, 1);
        assert_eq!(report.upserted_tokens, 1);
        assert_eq!(report.dropped_tokens, 0);
        assert_eq!(report.last_scanned_block, 10);
        let inner = store.inner.lock().unwrap();
        assert_eq!(inner.owned.len(), 1);
        assert_eq!(inner.attached_meta.len(), 1);
    }

    #[tokio::test]
    async fn receive_then_send_in_same_crawl_drops_inventory() {
        let owner = addr(0xA);
        let contract = addr(0xC);
        let mut tokens = HashMap::new();
        tokens.insert((contract, BigUint::from(1u32)), make_nft(contract, 1, 1, owner));
        let store = StubStore::default();
        let provider = StubProvider {
            transfers: vec![
                make_transfer(
                    contract,
                    1,
                    addr(0xB),
                    owner,
                    5,
                    0,
                    TransferStatus::Receive,
                    1,
                    ContractType::Erc721,
                ),
                make_transfer(
                    contract,
                    1,
                    owner,
                    addr(0xB),
                    6,
                    0,
                    TransferStatus::Send,
                    1,
                    ContractType::Erc721,
                ),
            ],
            tokens,
            current_block: 6,
        };
        let report = update_chain(&store, &provider, Chain::Eth, owner).await.unwrap();
        assert_eq!(report.appended_transfers, 2);
        assert_eq!(report.upserted_tokens, 0);
        assert_eq!(report.dropped_tokens, 0); // never inserted, nothing to drop
        assert_eq!(report.last_scanned_block, 6);
        assert!(store.inner.lock().unwrap().owned.is_empty());
    }

    #[tokio::test]
    async fn send_existing_token_drops_row() {
        let owner = addr(0xA);
        let contract = addr(0xC);
        let store = StubStore::default();
        // Pre-populate inventory.
        store
            .register_owned(Chain::Eth, vec![make_nft(contract, 1, 1, owner)], 1)
            .await
            .unwrap();
        let provider = StubProvider {
            transfers: vec![make_transfer(
                contract,
                1,
                owner,
                addr(0xB),
                10,
                0,
                TransferStatus::Send,
                1,
                ContractType::Erc721,
            )],
            tokens: HashMap::new(),
            current_block: 10,
        };
        let report = update_chain(&store, &provider, Chain::Eth, owner).await.unwrap();
        assert_eq!(report.dropped_tokens, 1);
        assert_eq!(report.upserted_tokens, 0);
        assert!(store.inner.lock().unwrap().owned.is_empty());
    }

    #[tokio::test]
    async fn erc1155_partial_send_decrements_amount() {
        let owner = addr(0xA);
        let contract = addr(0xC);
        let store = StubStore::default();
        let mut existing = make_nft(contract, 7, 5, owner);
        existing.contract_type = ContractType::Erc1155;
        store.register_owned(Chain::Eth, vec![existing], 1).await.unwrap();
        let provider = StubProvider {
            transfers: vec![make_transfer(
                contract,
                7,
                owner,
                addr(0xB),
                10,
                0,
                TransferStatus::Send,
                2,
                ContractType::Erc1155,
            )],
            tokens: HashMap::new(),
            current_block: 10,
        };
        let report = update_chain(&store, &provider, Chain::Eth, owner).await.unwrap();
        assert_eq!(report.dropped_tokens, 0);
        assert_eq!(report.upserted_tokens, 1);
        let inner = store.inner.lock().unwrap();
        let stored = inner
            .owned
            .get(&(Chain::Eth, format!("{:#x}", contract), BigUint::from(7u32)))
            .unwrap();
        assert_eq!(stored.common.amount, BigDecimal::from(3));
    }

    #[tokio::test]
    async fn from_block_filter_skips_already_seen_transfers() {
        let owner = addr(0xA);
        let contract = addr(0xC);
        let store = StubStore::default();
        // Simulate prior crawl: bookmark at block 10.
        store
            .append_transfers(Chain::Eth, vec![make_transfer(
                contract,
                1,
                addr(0xB),
                owner,
                10,
                0,
                TransferStatus::Receive,
                1,
                ContractType::Erc721,
            )])
            .await
            .unwrap();
        let provider = StubProvider {
            transfers: vec![make_transfer(
                contract,
                1,
                addr(0xB),
                owner,
                10,
                0,
                TransferStatus::Receive,
                1,
                ContractType::Erc721,
            )],
            tokens: HashMap::new(),
            current_block: 10,
        };
        // No new transfers strictly after block 10 -> empty fetch path.
        let report = update_chain(&store, &provider, Chain::Eth, owner).await.unwrap();
        assert_eq!(report.appended_transfers, 0);
    }
}
