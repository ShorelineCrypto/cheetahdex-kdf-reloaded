//! Storage abstractions for the NFT module.
//!
//! Storage is split into two narrow async traits, one for the per-wallet
//! token list and one for the historical transfer log. Sub-phases P10.3.3
//! and P10.3.4 implement these traits over SQLite (native) and IndexedDB
//! (WASM) respectively. Higher-level callers (RPC handlers, the providers
//! layer) only depend on the traits, not on the concrete backends.

pub mod errors;
pub mod history;
pub mod list;

#[cfg(target_arch = "wasm32")]
pub mod idb;

#[cfg(not(target_arch = "wasm32"))]
pub mod sqlite;

pub use errors::{NftStoreError, RemoveOutcome};
pub use history::NftHistoryStore;
pub use list::NftListStore;

use crate::nft::model::Chain;
use std::num::NonZeroUsize;

/// Resolves an inclusive (offset, limit) window from the page-style fields
/// shared by `get_nft_list` and `get_nft_transfers`.
///
/// * When `take_all` is true, the window covers the full result set
///   (`offset = 0`, `limit = total`).
/// * When `page` is `Some`, the window starts at `(page - 1) * page_size`
///   and is capped to `page_size` entries.
/// * Otherwise the window starts at zero and is capped to `page_size`.
pub fn paginate(take_all: bool, page_size: usize, page: Option<NonZeroUsize>, total: usize) -> (usize, usize) {
    if take_all {
        return (0, total);
    }
    match page {
        Some(p) => ((p.get() - 1) * page_size, page_size),
        None => (0, page_size),
    }
}

/// Convenience wrapper over the per-chain initialisation helpers exposed
/// by the two storage traits. Calling code can use this when it needs to
/// make sure that both stores are ready before issuing operations.
#[allow(dead_code)]
pub async fn ensure_initialised<L, H, E>(
    list_store: &L,
    history_store: &H,
    chain: &Chain,
) -> Result<(), mm2_err_handle::prelude::MmError<E>>
where
    L: NftListStore<Error = E>,
    H: NftHistoryStore<Error = E>,
    E: NftStoreError,
{
    list_store.ensure_chain(chain).await?;
    history_store.ensure_chain(chain).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroUsize;

    #[test]
    fn paginate_returns_full_range_when_take_all_is_true() {
        assert_eq!(paginate(true, 10, NonZeroUsize::new(2), 42), (0, 42));
    }

    #[test]
    fn paginate_uses_page_when_provided() {
        let page = NonZeroUsize::new(3);
        assert_eq!(paginate(false, 10, page, 100), (20, 10));
    }

    #[test]
    fn paginate_defaults_to_first_page_when_unset() {
        assert_eq!(paginate(false, 25, None, 100), (0, 25));
    }
}
