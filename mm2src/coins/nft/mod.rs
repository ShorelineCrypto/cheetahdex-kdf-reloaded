//! Non-fungible token (NFT) support for KDF-RELOADED.
//!
//! This module groups everything that is required to keep track of NFT
//! ownership, transfer history and withdrawals on the EVM-family chains
//! that are supported by the framework. The intent is to expose a small,
//! well-typed surface that the dispatcher and storage backends can build
//! on top of, while keeping the network and persistence layers swappable.
//!
//! Sub-modules introduced in P10.3.1:
//! * [`model`] — public request/response/data types and the small
//!   enums (chain, contract type, transfer status, …) that they share.
//! * [`errors`] — error hierarchies returned by the upcoming RPC handlers.
//! * [`serde_helpers`] — ser/de helpers for fields that travel as JSON
//!   strings (token IDs, optional `BigUint` amounts).
//!
//! Storage traits, SQLite/IndexedDB backends, providers and RPC handlers
//! are added in subsequent P10.3.x phases.

pub mod errors;
pub mod model;
pub mod serde_helpers;
pub mod store;

pub use errors::{
    ClearNftDbError, GetNftInfoError, LockDbError, MetadataFetchError, ParseChainError,
    ParseContractTypeError, ParseTransferStatusError, SpamFilterError, TransferConfirmationsError,
    UpdateNftError, UpdateSpamPhishingError,
};
pub use model::{
    Chain, ChainTicker, ClearNftDbReq, ContractType, Nft, NftCommon, NftInfo, NftList,
    NftListFilters, NftListReq, NftMetadataReq, NftTokenIdent, NftTransfer, NftTransferCommon,
    NftTransferList, NftTransfersFilters, NftTransfersReq, RefreshMetadataReq, TransferMeta,
    TransferStatus, UpdateNftReq, UriMeta, WithdrawErc1155, WithdrawErc721, WithdrawNftReq,
};
pub use store::{NftHistoryStore, NftListStore, NftStoreError, RemoveOutcome};
