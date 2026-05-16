//! Per-token metadata (name, description, media URLs, attributes).
//!
//! NFT marketplaces are inconsistent about which JSON field carries the
//! image URL — historically `image` was used, then `image_url` appeared
//! alongside it, and many minters publish both pointing to the same
//! resource. [`UriMeta`] models both, plus the optional animation,
//! external link and free-form attribute fields.

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

/// Token metadata derived from `tokenURI(...)` and on-chain `metadata`
/// payloads.
///
/// `raw_image_url` mirrors the legacy `image` field; `image_url` mirrors
/// the newer `image_url` field. They are exposed separately so that
/// downstream consumers can keep the original layout when re-serializing
/// while still presenting a single canonical URL via [`UriMeta::merge_in`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct UriMeta {
    /// Original `image` field as published on-chain or by the metadata host.
    #[serde(rename = "image")]
    pub raw_image_url: Option<String>,
    /// Canonical image URL surfaced to clients.
    pub image_url: Option<String>,
    /// Domain extracted from the image URL (used by spam/phishing filters).
    pub image_domain: Option<String>,
    /// Friendly token name.
    #[serde(rename = "name")]
    pub token_name: Option<String>,
    /// Free-form description.
    pub description: Option<String>,
    /// Trait/attribute payload as published; preserved verbatim.
    pub attributes: Option<Json>,
    /// Animation URL (video/audio/3D asset).
    pub animation_url: Option<String>,
    /// Domain extracted from the animation URL.
    pub animation_domain: Option<String>,
    /// Marketplace landing-page URL.
    pub external_url: Option<String>,
    /// Domain extracted from the external URL.
    pub external_domain: Option<String>,
    /// Optional sub-document with extra image details (dimensions, mime, …).
    pub image_details: Option<Json>,
}

impl UriMeta {
    /// Fill any field that is currently `None` with the corresponding value
    /// from `other`, leaving `raw_image_url` untouched. Used when merging
    /// metadata from `tokenURI(…)` with on-chain `metadata` strings.
    pub fn merge_in(&mut self, other: UriMeta) {
        if self.image_url.is_none() {
            self.image_url = other.raw_image_url.or(other.image_url);
        }
        if self.token_name.is_none() {
            self.token_name = other.token_name;
        }
        if self.description.is_none() {
            self.description = other.description;
        }
        if self.attributes.is_none() {
            self.attributes = other.attributes;
        }
        if self.animation_url.is_none() {
            self.animation_url = other.animation_url;
        }
        if self.external_url.is_none() {
            self.external_url = other.external_url;
        }
        if self.image_details.is_none() {
            self.image_details = other.image_details;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_keeps_existing_fields() {
        let mut base = UriMeta {
            image_url: Some("a".into()),
            token_name: Some("Alpha".into()),
            ..Default::default()
        };
        let other = UriMeta {
            raw_image_url: Some("b".into()),
            image_url: Some("c".into()),
            token_name: Some("Beta".into()),
            description: Some("filled".into()),
            ..Default::default()
        };
        base.merge_in(other);
        assert_eq!(base.image_url.as_deref(), Some("a"));
        assert_eq!(base.token_name.as_deref(), Some("Alpha"));
        assert_eq!(base.description.as_deref(), Some("filled"));
    }

    #[test]
    fn merge_prefers_raw_image_url_when_image_url_missing() {
        let mut base = UriMeta::default();
        let other = UriMeta {
            raw_image_url: Some("from-image".into()),
            image_url: Some("from-image-url".into()),
            ..Default::default()
        };
        base.merge_in(other);
        assert_eq!(base.image_url.as_deref(), Some("from-image"));
    }

    #[test]
    fn deserialize_recognises_image_and_name_aliases() {
        let raw = json!({
            "image": "ipfs://abc",
            "name": "My NFT",
            "description": "demo"
        });
        let meta: UriMeta = serde_json::from_value(raw).unwrap();
        assert_eq!(meta.raw_image_url.as_deref(), Some("ipfs://abc"));
        assert_eq!(meta.token_name.as_deref(), Some("My NFT"));
        assert_eq!(meta.description.as_deref(), Some("demo"));
    }
}
