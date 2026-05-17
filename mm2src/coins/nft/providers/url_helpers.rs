//! URL rewriting and domain helpers used when caching NFT metadata.
//!
//! When a token's `tokenURI(...)` points at a public IPFS gateway we may
//! want to redirect it to an alternate gateway (some legacy gateways
//! refuse to serve specific CIDs). [`decamouflage_legacy_ipfs_url`]
//! implements this rewrite. [`domain_of`] is a safe, allocation-free
//! wrapper for extracting the `host` part of a URL.

use crate::nft::model::UriMeta;
use url::Url;

/// Hostname of a legacy NFT IPFS gateway whose `bafy*` paths are widely
/// reported to fail with HTTP 451. We rewrite URLs that hit it to the
/// canonical `ipfs.io` gateway.
const LEGACY_IPFS_HOST: &str = "ipfs.moralis.io";
/// Path prefix on [`LEGACY_IPFS_HOST`] that triggers the rewrite.
const LEGACY_IPFS_PATH_PREFIX: &str = "/ipfs/bafy";
/// Replacement gateway base URL.
const FALLBACK_IPFS_BASE: &str = "https://ipfs.io/ipfs/";

/// Rewrite legacy IPFS gateway URLs in-place. Inputs that are not URLs
/// (or do not match the legacy host/path combination) are returned as-is
/// so this can be applied unconditionally.
pub fn decamouflage_legacy_ipfs_url(token_uri: Option<&str>) -> Option<String> {
    token_uri.map(|raw| match Url::parse(raw) {
        Ok(parsed) => {
            if parsed.host_str() == Some(LEGACY_IPFS_HOST)
                && parsed.path().starts_with(LEGACY_IPFS_PATH_PREFIX)
            {
                if let Some((_, cid_and_rest)) = parsed.path().split_once("/ipfs/") {
                    return format!("{}{}", FALLBACK_IPFS_BASE, cid_and_rest);
                }
            }
            raw.to_string()
        },
        Err(_) => raw.to_string(),
    })
}

/// Extract the registered domain from `url`. Returns `None` when the input
/// is missing, fails to parse or has no host component (relative or
/// data-URLs, for example).
pub fn domain_of(url: Option<&str>) -> Option<String> {
    url.and_then(|raw| Url::parse(raw).ok())
        .and_then(|parsed| parsed.domain().map(str::to_owned))
}

/// Apply [`decamouflage_legacy_ipfs_url`] to every URL field of `meta` and
/// repopulate the `*_domain` companion fields. Used after fetching cached
/// metadata so all stored entries share consistent host data.
pub fn normalise_metadata_urls(meta: &mut UriMeta) {
    meta.image_url = decamouflage_legacy_ipfs_url(meta.image_url.as_deref());
    meta.image_domain = domain_of(meta.image_url.as_deref());
    meta.animation_url = decamouflage_legacy_ipfs_url(meta.animation_url.as_deref());
    meta.animation_domain = domain_of(meta.animation_url.as_deref());
    meta.external_url = decamouflage_legacy_ipfs_url(meta.external_url.as_deref());
    meta.external_domain = domain_of(meta.external_url.as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_bafy_paths_get_redirected() {
        let rewritten = decamouflage_legacy_ipfs_url(Some(
            "https://ipfs.moralis.io/ipfs/bafyabc/0.json",
        ))
        .unwrap();
        assert_eq!(rewritten, "https://ipfs.io/ipfs/bafyabc/0.json");
    }

    #[test]
    fn non_legacy_urls_pass_through() {
        let original = "https://example.com/metadata/1.json";
        let same = decamouflage_legacy_ipfs_url(Some(original)).unwrap();
        assert_eq!(same, original);
    }

    #[test]
    fn other_legacy_paths_are_left_alone() {
        let original = "https://ipfs.moralis.io/ipfs/qm123";
        let same = decamouflage_legacy_ipfs_url(Some(original)).unwrap();
        assert_eq!(same, original);
    }

    #[test]
    fn unparsable_input_is_returned_verbatim() {
        let same = decamouflage_legacy_ipfs_url(Some("not a url")).unwrap();
        assert_eq!(same, "not a url");
    }

    #[test]
    fn none_input_yields_none() {
        assert!(decamouflage_legacy_ipfs_url(None).is_none());
    }

    #[test]
    fn domain_of_returns_host_for_https() {
        assert_eq!(domain_of(Some("https://example.com/x")), Some("example.com".into()));
    }

    #[test]
    fn domain_of_returns_none_for_data_url() {
        assert_eq!(domain_of(Some("data:application/json,{}")), None);
    }

    #[test]
    fn domain_of_returns_none_for_garbage() {
        assert_eq!(domain_of(Some("not a url")), None);
    }

    #[test]
    fn normalise_rewrites_all_fields_and_refreshes_domains() {
        let mut meta = UriMeta {
            image_url: Some("https://ipfs.moralis.io/ipfs/bafyimg".into()),
            animation_url: Some("https://example.com/anim.mp4".into()),
            external_url: Some("not-a-url".into()),
            ..UriMeta::default()
        };
        normalise_metadata_urls(&mut meta);
        assert_eq!(meta.image_url.as_deref(), Some("https://ipfs.io/ipfs/bafyimg"));
        assert_eq!(meta.image_domain.as_deref(), Some("ipfs.io"));
        assert_eq!(meta.animation_domain.as_deref(), Some("example.com"));
        assert_eq!(meta.external_domain, None);
    }
}
