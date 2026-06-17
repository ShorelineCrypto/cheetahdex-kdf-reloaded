//! Coin-side RPC handlers for the NFT module.
//!
//! These handlers are wired into the JSON-RPC dispatcher in
//! `mm2_main`. They cover the read-only and database-management
//! endpoints that the GUI relies on:
//!
//! * `get_nft_list`
//! * `get_nft_metadata`
//! * `get_nft_transfers`
//! * `clear_nft_db`
//! * `refresh_nft_metadata`
//! * `update_nft`
//! * `withdraw_nft`
//!
//! `update_nft` runs the multi-chain incremental crawler over every
//! requested chain. Each chain requires a corresponding EVM coin to be
//! activated (so the wallet's owner address is known); chains whose
//! coin is not enabled are reported as `NoSuchCoin` errors but the
//! remaining chains continue to crawl.

#[cfg(not(target_arch = "wasm32"))]
use crate::lp_coinfind;
use crate::nft::context::NftCtx;
use crate::nft::errors::{ClearNftDbError, GetNftInfoError, UpdateNftError};
use crate::nft::model::{
    Chain, ClearNftDbReq, Nft, NftList, NftListReq, NftMetadataReq, NftTransferList, NftTransfersReq,
    RefreshMetadataReq, UpdateNftReq, WithdrawNftReq,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::nft::providers::HttpCrawlProvider;
use crate::nft::providers::{apply_spam_protection_to_nft, apply_spam_protection_to_transfer, HttpMetadataProvider};
use crate::nft::store::{ensure_initialised, NftHistoryStore, NftListStore};
#[cfg(not(target_arch = "wasm32"))]
use crate::MmCoinEnum;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

/// Handler for the JSON-RPC `get_nft_list` method.
///
/// Aggregates the cached inventory across the requested chains, applies
/// any pagination/spam filters and (optionally) redacts spam links from
/// user-controlled fields before returning the result.
pub async fn get_nft_list(ctx: MmArc, req: NftListReq) -> MmResult<NftList, GetNftInfoError> {
    if req.chains.is_empty() {
        return MmError::err(GetNftInfoError::InvalidRequest(
            "`chains` must contain at least one entry".to_owned(),
        ));
    }
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(GetNftInfoError::Internal)?;
    let store = nft_ctx.store();
    for chain in &req.chains {
        ensure_initialised(store, store, chain)
            .await
            .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?;
    }
    let mut list = NftListStore::list_owned(store, req.chains, req.max, req.limit, req.page_number, req.filters)
        .await
        .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?;
    if req.protect_from_spam {
        for nft in &mut list.nfts {
            apply_spam_protection_to_nft(nft, true).map_to_mm(|err| GetNftInfoError::SpamFilter(err.to_string()))?;
        }
    }
    Ok(list)
}

/// Handler for the JSON-RPC `get_nft_metadata` method.
///
/// Returns a single record from the cached inventory keyed by chain,
/// contract address and token id.
pub async fn get_nft_metadata(ctx: MmArc, req: NftMetadataReq) -> MmResult<Nft, GetNftInfoError> {
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(GetNftInfoError::Internal)?;
    let store = nft_ctx.store();
    ensure_initialised(store, store, &req.chain)
        .await
        .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?;
    let token_address_hex = format!("{:#x}", req.token_address);
    let mut nft = NftListStore::fetch_token(store, &req.chain, token_address_hex.clone(), req.token_id.clone())
        .await
        .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?
        .ok_or(GetNftInfoError::TokenNotFoundInWallet {
            token_address: token_address_hex,
            token_id: req.token_id.to_string(),
        })?;
    if req.protect_from_spam {
        apply_spam_protection_to_nft(&mut nft, true).map_to_mm(|err| GetNftInfoError::SpamFilter(err.to_string()))?;
    }
    Ok(nft)
}

/// Handler for the JSON-RPC `get_nft_transfers` method.
///
/// Returns a paginated slice of the transfer history for the requested
/// chains. Unlike the legacy reference implementation this revision does
/// not yet annotate records with on-chain confirmations: that requires
/// a live `EthCoin` and will be added together with the activation
/// integration.
pub async fn get_nft_transfers(ctx: MmArc, req: NftTransfersReq) -> MmResult<NftTransferList, GetNftInfoError> {
    if req.chains.is_empty() {
        return MmError::err(GetNftInfoError::InvalidRequest(
            "`chains` must contain at least one entry".to_owned(),
        ));
    }
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(GetNftInfoError::Internal)?;
    let store = nft_ctx.store();
    for chain in &req.chains {
        ensure_initialised(store, store, chain)
            .await
            .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?;
    }
    let mut list = NftHistoryStore::list_transfers(store, req.chains, req.max, req.limit, req.page_number, req.filters)
        .await
        .mm_err(|err| GetNftInfoError::Storage(format!("{err:?}")))?;
    if req.protect_from_spam {
        for transfer in &mut list.transfer_history {
            apply_spam_protection_to_transfer(transfer, true)
                .map_to_mm(|err| GetNftInfoError::SpamFilter(err.to_string()))?;
        }
    }
    Ok(list)
}

/// Handler for the JSON-RPC `clear_nft_db` method.
///
/// When `clear_all` is `true` every chain's cache is wiped; otherwise the
/// caller's `chains` list is used as a whitelist.
pub async fn clear_nft_db(ctx: MmArc, req: ClearNftDbReq) -> MmResult<(), ClearNftDbError> {
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(ClearNftDbError::Internal)?;
    let store = nft_ctx.store();
    if req.clear_all {
        NftListStore::purge_all(store)
            .await
            .mm_err(|err| ClearNftDbError::Storage(format!("{err:?}")))?;
        NftHistoryStore::purge_all(store)
            .await
            .mm_err(|err| ClearNftDbError::Storage(format!("{err:?}")))?;
        return Ok(());
    }
    if req.chains.is_empty() {
        return MmError::err(ClearNftDbError::InvalidRequest(
            "Nothing to clear was specified".to_owned(),
        ));
    }
    let mut errors: Vec<String> = Vec::new();
    for chain in &req.chains {
        if let Err(err) = clear_chain(store, chain).await {
            errors.push(format!("{:?}", err.into_inner()));
        }
    }
    if !errors.is_empty() {
        return MmError::err(ClearNftDbError::Storage(format!("{errors:?}")));
    }
    Ok(())
}

async fn clear_chain<S>(store: &S, chain: &Chain) -> MmResult<(), ClearNftDbError>
where
    S: NftListStore + NftHistoryStore,
{
    NftListStore::purge_chain(store, chain)
        .await
        .mm_err(|err| ClearNftDbError::Storage(format!("{err:?}")))?;
    NftHistoryStore::purge_chain(store, chain)
        .await
        .mm_err(|err| ClearNftDbError::Storage(format!("{err:?}")))?;
    Ok(())
}

/// Handler for the JSON-RPC `update_nft` method.
///
/// Runs the incremental multi-chain crawler against every chain in the
/// request. Per-chain failures are surfaced as a single aggregated
/// `Storage` error so the caller can retry the failed chains; chains
/// that succeed have their inventory and transfer log persisted before
/// the error is raised.
#[cfg(not(target_arch = "wasm32"))]
pub async fn update_nft(ctx: MmArc, req: UpdateNftReq) -> MmResult<(), UpdateNftError> {
    use crate::nft::model::ChainTicker;
    if req.chains.is_empty() {
        return MmError::err(UpdateNftError::Internal(
            "`chains` must contain at least one entry".to_owned(),
        ));
    }
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(UpdateNftError::Internal)?;
    let store = nft_ctx.store();
    let provider = HttpCrawlProvider::new(req.url, req.komodo_proxy);
    let mut errors: Vec<String> = Vec::new();
    for chain in &req.chains {
        ensure_initialised(store, store, chain)
            .await
            .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;
        let ticker = chain.coin_ticker();
        let coin = match lp_coinfind(&ctx, ticker).await.map_err(UpdateNftError::Internal)? {
            Some(MmCoinEnum::EthCoin(eth)) => eth,
            Some(_) => {
                errors.push(format!("{ticker}: not an EVM coin"));
                continue;
            },
            None => {
                errors.push(format!("{ticker}: not activated"));
                continue;
            },
        };
        let owner = coin.my_address;
        if let Err(err) = crate::nft::providers::update_chain(store, &provider, *chain, owner).await {
            errors.push(format!("{}: {:?}", ticker, err.into_inner()));
        }
    }
    if !errors.is_empty() {
        return MmError::err(UpdateNftError::Storage(format!("{errors:?}")));
    }
    Ok(())
}

/// WASM stub for `update_nft`. The crawler depends on `lp_coinfind`,
/// which currently only resolves EVM coins on native targets; the WASM
/// build path will be wired in once browser-side EVM activation lands.
#[cfg(target_arch = "wasm32")]
pub async fn update_nft(_ctx: MmArc, _req: UpdateNftReq) -> MmResult<(), UpdateNftError> {
    MmError::err(UpdateNftError::Internal(
        "update_nft is not yet supported on the WASM target".to_owned(),
    ))
}

/// Handler for the JSON-RPC `refresh_nft_metadata` method.
///
/// Re-fetches the metadata of a single cached token through the
/// configured HTTP provider, merges the fresh fields into the inventory
/// entry, and propagates them into the historical transfer log so the
/// `get_nft_transfers` endpoint surfaces the same values as
/// `get_nft_metadata`.
pub async fn refresh_nft_metadata(ctx: MmArc, req: RefreshMetadataReq) -> MmResult<(), UpdateNftError> {
    let nft_ctx = NftCtx::from_mm_ctx(&ctx).map_to_mm(UpdateNftError::Internal)?;
    let store = nft_ctx.store();
    ensure_initialised(store, store, &req.chain)
        .await
        .mm_err(|err| UpdateNftError::Storage(format!("{err:?}")))?;
    let provider = HttpMetadataProvider::new(req.url, req.komodo_proxy);
    crate::nft::providers::refresh_nft_metadata(store, &provider, req.chain, req.token_address, req.token_id).await
}

/// Handler for the JSON-RPC `withdraw_nft` method.
///
/// Builds and signs (without broadcasting) an ERC-721 `transferFrom`
/// or ERC-1155 `safeTransferFrom` transaction against the EVM coin
/// that backs the requested chain. The signed transaction is returned
/// as a `TransactionDetails` payload (serialised as JSON) so the GUI
/// can confirm and push it through `send_raw_transaction`.
pub async fn withdraw_nft(ctx: MmArc, req: WithdrawNftReq) -> MmResult<serde_json::Value, GetNftInfoError> {
    let details = crate::nft::withdraw::withdraw_nft(ctx, req).await?;
    serde_json::to_value(details)
        .map_to_mm(|err| GetNftInfoError::Internal(format!("serialise TransactionDetails: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nft::model::ClearNftDbReq;
    use db_common::async_sql_conn::AsyncConnection;
    use futures::lock::Mutex as AsyncMutex;
    use mm2_core::mm_ctx::{MmArc, MmCtxBuilder};
    use std::sync::Arc;

    async fn ctx_with_in_memory_db() -> MmArc {
        let ctx = MmCtxBuilder::new().into_mm_arc();
        let conn = AsyncConnection::open_in_memory().await.expect("open in-memory db");
        ctx.async_sqlite_connection
            .set(Arc::new(AsyncMutex::new(conn)))
            .map_err(|_| "already initialised")
            .expect("set async_sqlite_connection");
        ctx
    }

    #[tokio::test]
    async fn clear_with_no_chains_and_no_clear_all_is_invalid() {
        let ctx = ctx_with_in_memory_db().await;
        let err = clear_nft_db(
            ctx,
            ClearNftDbReq {
                chains: vec![],
                clear_all: false,
            },
        )
        .await
        .expect_err("expected InvalidRequest");
        match err.into_inner() {
            ClearNftDbError::InvalidRequest(_) => (),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_nft_list_rejects_empty_chains() {
        let ctx = ctx_with_in_memory_db().await;
        let req = NftListReq {
            chains: vec![],
            max: false,
            limit: 10,
            page_number: None,
            protect_from_spam: false,
            filters: None,
        };
        let err = get_nft_list(ctx, req).await.expect_err("expected InvalidRequest");
        match err.into_inner() {
            GetNftInfoError::InvalidRequest(_) => (),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_nft_transfers_rejects_empty_chains() {
        let ctx = ctx_with_in_memory_db().await;
        let req = NftTransfersReq {
            chains: vec![],
            filters: None,
            max: false,
            limit: 10,
            page_number: None,
            protect_from_spam: false,
        };
        let err = get_nft_transfers(ctx, req).await.expect_err("expected InvalidRequest");
        match err.into_inner() {
            GetNftInfoError::InvalidRequest(_) => (),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_nft_metadata_returns_token_not_found() {
        let ctx = ctx_with_in_memory_db().await;
        let req: NftMetadataReq = serde_json::from_value(serde_json::json!({
            "token_address": "0x0000000000000000000000000000000000000001",
            "token_id": "1",
            "chain": "ETH"
        }))
        .unwrap();
        let err = get_nft_metadata(ctx, req)
            .await
            .expect_err("expected TokenNotFoundInWallet");
        match err.into_inner() {
            GetNftInfoError::TokenNotFoundInWallet { token_id, .. } => assert_eq!(token_id, "1"),
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_nft_list_returns_empty_list_for_initialised_chain() {
        let ctx = ctx_with_in_memory_db().await;
        let req = NftListReq {
            chains: vec![Chain::Eth],
            max: false,
            limit: 10,
            page_number: None,
            protect_from_spam: false,
            filters: None,
        };
        let list = get_nft_list(ctx, req).await.expect("get_nft_list should succeed");
        assert!(list.nfts.is_empty());
        assert_eq!(list.total, 0);
    }

    #[tokio::test]
    async fn clear_all_succeeds_on_empty_db() {
        let ctx = ctx_with_in_memory_db().await;
        clear_nft_db(
            ctx,
            ClearNftDbReq {
                chains: vec![],
                clear_all: true,
            },
        )
        .await
        .expect("clear_all should succeed on an empty db");
    }
}
