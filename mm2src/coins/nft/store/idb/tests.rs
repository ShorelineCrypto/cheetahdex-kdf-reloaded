//! `wasm-bindgen-test` smoke tests for the IndexedDB NFT backend
//! skeleton.
//!
//! At this revision the backend exposes the trait surface but every
//! method returns `IndexedDbStoreError::Unimplemented`. The smoke test
//! verifies that:
//! * the `NftIndexedDb` instance can be constructed (i.e. the schema
//!   passes the upgrade callback at version 1);
//! * a method call on the wrapper surfaces the `Unimplemented` marker
//!   instead of panicking.

use crate::nft::model::Chain;
use crate::nft::store::idb::{IndexedDbNftStore, IndexedDbStoreError, NftIndexedDb};
use crate::nft::store::list::NftListStore;
use mm2_core::mm_ctx::MmCtxBuilder;
use mm2_db::indexed_db::ConstructibleDb;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test::wasm_bindgen_test]
async fn schema_initialises_and_unimplemented_marker_surfaces() {
    let ctx = MmCtxBuilder::new().with_test_db_namespace().into_mm_arc();
    let shared = ConstructibleDb::<NftIndexedDb>::new_shared(&ctx);
    // Force the database to construct so we exercise the upgrade callback.
    shared.get_or_initialize().await.expect("init db");
    let store = IndexedDbNftStore::new(shared);
    let err = store
        .ensure_chain(&Chain::Eth)
        .await
        .expect_err("ensure_chain is intentionally unimplemented")
        .into_inner();
    match err {
        IndexedDbStoreError::Unimplemented(name) => assert_eq!(name, "NftListStore::ensure_chain"),
        other => panic!("expected Unimplemented marker, got {other:?}"),
    }
}
