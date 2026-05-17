//! Providers layer for the NFT module.
//!
//! Houses helpers that talk to external NFT metadata services and keep the
//! cached records sanitised. The module is deliberately split into small,
//! independently-testable units:
//!
//! * [`url_helpers`] — URL rewriting (legacy IPFS gateway redirects) and
//!   safe domain extraction.
//! * [`spam`] — link/spam detection over user-controlled text fields.
//! * [`http`] — thin wrappers around `mm2_net` for fetching JSON via
//!   optional proxy URLs (the proxy-signing logic is kept abstract so this
//!   module does not depend on the WalletConnect / proxy crates).
//!
//! Higher-level orchestration (paginated crawls, EthCoin updates, RPC
//! handlers) lives outside this module so that the building blocks here
//! remain reusable and free of cross-coin dependencies.

pub mod http;
pub mod spam;
pub mod url_helpers;

pub use http::{fetch_json, FetchError};
pub use spam::{
    apply_spam_protection_to_nft, apply_spam_protection_to_transfer, contains_url, is_token_uri_suspicious,
    redact_text_if_spam, SpamScanError,
};
pub use url_helpers::{decamouflage_legacy_ipfs_url, domain_of, normalise_metadata_urls};
