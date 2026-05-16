//! RPC request payloads for the NFT module.

use crate::nft::model::chain::Chain;
use crate::nft::serde_helpers::{default_page_size, token_id_from_string};
use ethereum_types::Address;
use mm2_number::BigUint;
use serde::Deserialize;
use std::num::NonZeroUsize;
use url::Url;

/// `get_nft_list` request payload.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NftListReq {
    /// Chains to enumerate.
    pub chains: Vec<Chain>,
    /// When `true`, return every record on the requested chains and ignore
    /// `limit`/`page_number`.
    #[serde(default)]
    pub max: bool,
    /// Maximum number of records to return per page when `max` is `false`.
    #[serde(default = "default_page_size")]
    pub limit: usize,
    /// 1-based page number when `max` is `false`.
    pub page_number: Option<NonZeroUsize>,
    /// Apply spam-protection helpers before returning the response.
    #[serde(default)]
    pub protect_from_spam: bool,
    /// Optional spam/phishing filters.
    pub filters: Option<NftListFilters>,
}

/// Filters that can be applied to a `get_nft_list` query.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct NftListFilters {
    /// Drop records flagged as spam.
    #[serde(default)]
    pub exclude_spam: bool,
    /// Drop records whose metadata domain matches a known phishing host.
    #[serde(default)]
    pub exclude_phishing: bool,
}

/// `get_nft_metadata` request payload.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NftMetadataReq {
    /// Address of the NFT contract.
    pub token_address: Address,
    /// Token identifier (string-encoded `BigUint`).
    #[serde(deserialize_with = "token_id_from_string")]
    pub token_id: BigUint,
    /// Chain on which the contract lives.
    pub chain: Chain,
    /// Apply spam-protection helpers before returning the response.
    #[serde(default)]
    pub protect_from_spam: bool,
}

/// `refresh_nft_metadata` request payload.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct RefreshMetadataReq {
    /// Address of the NFT contract.
    pub token_address: Address,
    /// Token identifier (string-encoded `BigUint`).
    #[serde(deserialize_with = "token_id_from_string")]
    pub token_id: BigUint,
    /// Chain on which the contract lives.
    pub chain: Chain,
    /// Provider base URL used to fetch the fresh metadata payload.
    pub url: Url,
    /// Anti-spam/anti-phishing service base URL used to validate the refreshed
    /// metadata.
    pub url_antispam: Url,
    /// When `true`, route the metadata request through the Komodo proxy.
    #[serde(default)]
    pub komodo_proxy: bool,
}

/// `get_nft_transfers` request payload.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NftTransfersReq {
    /// Chains to enumerate.
    pub chains: Vec<Chain>,
    /// Optional filters to apply to the result set.
    pub filters: Option<NftTransfersFilters>,
    /// When `true`, return every record on the requested chains and ignore
    /// `limit`/`page_number`.
    #[serde(default)]
    pub max: bool,
    /// Maximum number of records to return per page when `max` is `false`.
    #[serde(default = "default_page_size")]
    pub limit: usize,
    /// 1-based page number when `max` is `false`.
    pub page_number: Option<NonZeroUsize>,
    /// Apply spam-protection helpers before returning the response.
    #[serde(default)]
    pub protect_from_spam: bool,
}

/// Filters that can be applied to a `get_nft_transfers` query.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct NftTransfersFilters {
    /// Include incoming transfers.
    #[serde(default)]
    pub receive: bool,
    /// Include outgoing transfers.
    #[serde(default)]
    pub send: bool,
    /// Lower bound on `block_timestamp` (inclusive).
    pub from_date: Option<u64>,
    /// Upper bound on `block_timestamp` (inclusive).
    pub to_date: Option<u64>,
    /// Drop records flagged as spam.
    #[serde(default)]
    pub exclude_spam: bool,
    /// Drop records whose metadata domain matches a known phishing host.
    #[serde(default)]
    pub exclude_phishing: bool,
}

/// `update_nft` request payload.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UpdateNftReq {
    /// Chains to update.
    pub chains: Vec<Chain>,
    /// Provider base URL used to refresh the NFT cache.
    pub url: Url,
    /// Anti-spam/anti-phishing service base URL.
    pub url_antispam: Url,
    /// When `true`, route the metadata request through the Komodo proxy.
    #[serde(default)]
    pub komodo_proxy: bool,
}

/// Composite key identifying a single NFT inside the cache (token contract
/// address as a hex string + token identifier).
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq)]
pub struct NftTokenIdent {
    /// Lower-case hex-encoded NFT contract address.
    pub token_address: String,
    /// Token identifier within the contract.
    pub token_id: BigUint,
}

/// `clear_nft_db` request payload.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ClearNftDbReq {
    /// Chains to wipe. Ignored when `clear_all` is `true`.
    #[serde(default)]
    pub chains: Vec<Chain>,
    /// When `true`, wipe NFT data for every chain regardless of `chains`.
    #[serde(default)]
    pub clear_all: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nft_list_req_uses_defaults() {
        let req: NftListReq = serde_json::from_value(json!({"chains": ["ETH"]})).unwrap();
        assert_eq!(req.chains, vec![Chain::Eth]);
        assert!(!req.max);
        assert_eq!(req.limit, 10);
        assert!(req.page_number.is_none());
        assert!(!req.protect_from_spam);
        assert!(req.filters.is_none());
    }

    #[test]
    fn nft_metadata_req_parses_token_id_string() {
        let req: NftMetadataReq = serde_json::from_value(json!({
            "token_address": "0x0000000000000000000000000000000000000001",
            "token_id": "999999999999999999999",
            "chain": "POLYGON"
        }))
        .unwrap();
        assert_eq!(req.chain, Chain::Polygon);
        assert_eq!(req.token_id.to_string(), "999999999999999999999");
    }

    #[test]
    fn nft_metadata_req_rejects_numeric_token_id() {
        let outcome: Result<NftMetadataReq, _> = serde_json::from_value(json!({
            "token_address": "0x0000000000000000000000000000000000000001",
            "token_id": 1,
            "chain": "ETH"
        }));
        assert!(outcome.is_err());
    }

    #[test]
    fn refresh_metadata_req_round_trip() {
        let req: RefreshMetadataReq = serde_json::from_value(json!({
            "token_address": "0x0000000000000000000000000000000000000002",
            "token_id": "1",
            "chain": "BSC",
            "url": "https://example.com/",
            "url_antispam": "https://antispam.example.com/"
        }))
        .unwrap();
        assert_eq!(req.chain, Chain::Bsc);
        assert!(!req.komodo_proxy);
    }

    #[test]
    fn clear_nft_db_req_supports_clear_all() {
        let req: ClearNftDbReq = serde_json::from_value(json!({"clear_all": true})).unwrap();
        assert!(req.clear_all);
        assert!(req.chains.is_empty());
    }

    #[test]
    fn nft_transfers_filters_have_defaults() {
        let filters: NftTransfersFilters = serde_json::from_value(json!({})).unwrap();
        assert_eq!(filters, NftTransfersFilters::default());
    }
}
