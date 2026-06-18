//! Data types describing NFTs, their transfers and the requests/responses
//! exchanged with the GUI client.
//!
//! The module is split per responsibility so that storage and providers can
//! depend on small, focused units instead of a single 800-line file:
//!
//! * [`chain`] — supported blockchain enumeration and contract-type tag.
//! * [`metadata`] — `UriMeta` and helpers describing per-token metadata.
//! * [`nft`] — collection-token data (`Nft`, `NftCommon`, `NftInfo`, `NftList`).
//! * [`transfer`] — historical transfer entries and listings.
//! * [`request`] — RPC request payloads (list, metadata, refresh, …).
//! * [`withdraw`] — NFT withdraw request enums.

pub mod chain;
pub mod metadata;
pub mod nft;
pub mod request;
pub mod transfer;
pub mod withdraw;

pub use chain::{Chain, ChainTicker, ContractType};
pub use metadata::UriMeta;
pub use nft::{Nft, NftCommon, NftInfo, NftList};
pub use request::{ClearNftDbReq, NftListFilters, NftListReq, NftMetadataReq, NftTokenIdent, NftTransfersFilters,
                  NftTransfersReq, RefreshMetadataReq, UpdateNftReq};
pub use transfer::{NftTransfer, NftTransferCommon, NftTransferList, TransferMeta, TransferStatus};
pub use withdraw::{WithdrawErc1155, WithdrawErc721, WithdrawNftReq};
