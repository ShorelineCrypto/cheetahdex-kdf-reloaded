//! `wasm-bindgen-test` smoke tests for the IndexedDB NFT backend.
//!
//! Covers the chain-lifecycle and read/write round-trips for the
//! methods implemented in P10.3.4.b. Methods still backed by the
//! `Unimplemented` marker (e.g. `drop_token`,
//! `attach_metadata_to_transfers`) are out of scope for this revision
//! and are exercised by their respective slices when implemented.

use crate::nft::model::{Chain, ContractType, Nft, NftCommon, NftListFilters};
use crate::nft::model::{NftTransfer, NftTransferCommon, TransferStatus};
use crate::nft::store::history::NftHistoryStore;
use crate::nft::store::idb::{IndexedDbNftStore, NftIndexedDb};
use crate::nft::store::list::NftListStore;
use ethereum_types::Address;
use mm2_core::mm_ctx::MmCtxBuilder;
use mm2_db::indexed_db::ConstructibleDb;
use mm2_number::{BigDecimal, BigUint};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

fn fresh_store() -> IndexedDbNftStore {
    let ctx = MmCtxBuilder::new().with_test_db_namespace().into_mm_arc();
    let shared = ConstructibleDb::<NftIndexedDb>::new_shared(&ctx);
    IndexedDbNftStore::new(shared)
}

fn sample_nft(token_id: u64, block: u64, possible_spam: bool) -> Nft {
    let token_address = Address::from("0x00000000000000000000000000000000000000A1");
    let owner = Address::from("0x00000000000000000000000000000000000000A2");
    Nft {
        common: NftCommon {
            token_address,
            amount: BigDecimal::from(1u32),
            owner_of: owner,
            token_hash: None,
            collection_name: Some("Test".to_owned()),
            symbol: None,
            token_uri: None,
            token_domain: None,
            metadata: None,
            last_token_uri_sync: None,
            last_metadata_sync: None,
            minter_address: None,
            possible_spam,
        },
        chain: Chain::Eth,
        token_id: BigUint::from(token_id),
        block_number_minted: Some(block),
        block_number: block,
        contract_type: ContractType::Erc721,
        possible_phishing: false,
        uri_meta: Default::default(),
    }
}

fn sample_transfer(token_id: u64, block: u64, ts: u64, status: TransferStatus) -> NftTransfer {
    let token_address = Address::from("0x00000000000000000000000000000000000000A1");
    NftTransfer {
        common: NftTransferCommon {
            block_hash: None,
            transaction_hash: format!("0x{:064x}", block),
            transaction_index: Some(0),
            log_index: 0,
            value: None,
            transaction_type: None,
            token_address,
            from_address: Address::from("0x00000000000000000000000000000000000000A2"),
            to_address: Address::from("0x00000000000000000000000000000000000000A3"),
            amount: BigDecimal::from(1u32),
            verified: Some(1),
            operator: None,
            possible_spam: false,
        },
        chain: Chain::Eth,
        token_id: BigUint::from(token_id),
        block_number: block,
        block_timestamp: ts,
        contract_type: ContractType::Erc721,
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

#[wasm_bindgen_test::wasm_bindgen_test]
async fn ensure_chain_then_chain_ready_returns_true() {
    let store = fresh_store();
    assert!(!NftListStore::chain_ready(&store, &Chain::Eth).await.unwrap());
    NftListStore::ensure_chain(&store, &Chain::Eth).await.unwrap();
    assert!(NftListStore::chain_ready(&store, &Chain::Eth).await.unwrap());
    assert_eq!(
        NftListStore::latest_scanned_block(&store, &Chain::Eth).await.unwrap(),
        Some(0)
    );
}

#[wasm_bindgen_test::wasm_bindgen_test]
async fn register_and_fetch_round_trip_through_inventory() {
    let store = fresh_store();
    NftListStore::ensure_chain(&store, &Chain::Eth).await.unwrap();
    let nft = sample_nft(1, 100, false);
    store.register_owned(Chain::Eth, vec![nft.clone()], 100).await.unwrap();
    let fetched = store
        .fetch_token(
            &Chain::Eth,
            format!("{:?}", nft.common.token_address),
            BigUint::from(1u32),
        )
        .await
        .unwrap();
    assert_eq!(fetched, Some(nft));
    assert_eq!(
        NftListStore::latest_scanned_block(&store, &Chain::Eth).await.unwrap(),
        Some(100)
    );
}

#[wasm_bindgen_test::wasm_bindgen_test]
async fn list_owned_paginates_and_skips_spam() {
    let store = fresh_store();
    NftListStore::ensure_chain(&store, &Chain::Eth).await.unwrap();
    let mut nfts = Vec::new();
    for id in 1u64..=4 {
        let n = sample_nft(id, 100 + id, id == 2 /* spam */);
        nfts.push(n);
    }
    store.register_owned(Chain::Eth, nfts, 200).await.unwrap();
    let filters = Some(NftListFilters {
        exclude_spam: true,
        exclude_phishing: false,
    });
    let list = store
        .list_owned(vec![Chain::Eth], false, 2, std::num::NonZeroUsize::new(1), filters)
        .await
        .unwrap();
    assert_eq!(list.total, 3);
    assert_eq!(list.skipped, 1);
    assert_eq!(list.nfts.len(), 2);
    // Sorted newest-first by block_number.
    assert_eq!(list.nfts[0].token_id, BigUint::from(4u32));
    assert_eq!(list.nfts[1].token_id, BigUint::from(3u32));
}

#[wasm_bindgen_test::wasm_bindgen_test]
async fn append_transfers_and_lookup_by_log() {
    let store = fresh_store();
    NftHistoryStore::ensure_chain(&store, &Chain::Eth).await.unwrap();
    let t1 = sample_transfer(1, 100, 1_000, TransferStatus::Receive);
    let t2 = sample_transfer(2, 110, 1_100, TransferStatus::Send);
    store
        .append_transfers(Chain::Eth, vec![t1.clone(), t2.clone()])
        .await
        .unwrap();
    assert_eq!(store.latest_transfer_block(&Chain::Eth).await.unwrap(), Some(110));
    let found = store
        .transfer_by_log(
            &Chain::Eth,
            t1.common.transaction_hash.clone(),
            t1.common.log_index,
            t1.token_id.clone(),
        )
        .await
        .unwrap();
    assert_eq!(found, Some(t1));
    let listed = store
        .list_transfers(vec![Chain::Eth], true, 0, None, None)
        .await
        .unwrap();
    assert_eq!(listed.total, 2);
    assert_eq!(listed.transfer_history.len(), 2);
    // Sorted newest-first.
    assert_eq!(listed.transfer_history[0].block_number, 110);
}

#[wasm_bindgen_test::wasm_bindgen_test]
async fn purge_chain_clears_both_stores_and_bookmark() {
    let store = fresh_store();
    NftListStore::ensure_chain(&store, &Chain::Eth).await.unwrap();
    let nft = sample_nft(1, 100, false);
    store.register_owned(Chain::Eth, vec![nft], 100).await.unwrap();
    let tr = sample_transfer(1, 100, 1_000, TransferStatus::Receive);
    store.append_transfers(Chain::Eth, vec![tr]).await.unwrap();
    NftListStore::purge_chain(&store, &Chain::Eth).await.unwrap();
    assert!(!NftListStore::chain_ready(&store, &Chain::Eth).await.unwrap());
    let list = store.list_owned(vec![Chain::Eth], true, 0, None, None).await.unwrap();
    assert_eq!(list.total, 0);
    let history = store
        .list_transfers(vec![Chain::Eth], true, 0, None, None)
        .await
        .unwrap();
    assert_eq!(history.total, 0);
}
