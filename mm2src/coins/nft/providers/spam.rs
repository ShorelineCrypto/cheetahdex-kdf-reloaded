//! Spam and phishing heuristics applied to user-controlled NFT text fields.
//!
//! NFT collections frequently embed clickable URLs in fields that the GUI
//! renders verbatim (collection name, symbol, token name, raw `metadata`
//! JSON). [`contains_url`] is a deliberately permissive regex that flags
//! anything resembling a link so the GUI can mask the field.
//!
//! [`is_token_uri_suspicious`] runs a separate, narrower regex over the
//! token URI itself: a few TLDs and `?`/`%`-suffixed extensions appear
//! disproportionately in known-malicious metadata.

use crate::nft::model::{Nft, NftTransfer};
use derive_more::Display;
use regex::Regex;
use serde::Serialize;
use serde_json::{Map, Value as Json};

/// Static replacement text used when redacting links from user-supplied
/// fields. Surfaced verbatim to GUI clients.
pub const REDACTION_PLACEHOLDER: &str = "URL redacted for user protection";

#[derive(Debug, Display, Serialize)]
pub enum SpamScanError {
    #[display(fmt = "Invalid spam regex: {}", _0)]
    InvalidRegex(String),
    #[display(fmt = "Failed to re-serialize redacted metadata JSON: {}", _0)]
    Serialize(String),
}

impl From<regex::Error> for SpamScanError {
    fn from(err: regex::Error) -> Self {
        SpamScanError::InvalidRegex(err.to_string())
    }
}

impl From<serde_json::Error> for SpamScanError {
    fn from(err: serde_json::Error) -> Self {
        SpamScanError::Serialize(err.to_string())
    }
}

/// Permissive URL detector. Matches anything that *looks* like a link
/// regardless of validity — the goal is to err on the side of redaction.
pub fn contains_url(text: &str) -> Result<bool, SpamScanError> {
    // Compile once per call: callers usually run on a small, bounded set
    // of fields per NFT and the regex is cheap to build.
    let url_regex = Regex::new(
        r"(?:(?:https?|ftp|file|[^:\s]+:)/?|[^:\s]+:/|\b(?:[a-z\d]+\.))(?:(?:[^\s()<>]+|\((?:[^\s()<>]+|(?:\([^\s()<>]+\)))?\))+(?:\((?:[^\s()<>]+|(?:\(?:[^\s()<>]+\)))?\)|[^\s`!()\[\]{};:'.,<>?«»“”‘’]))?",
    )?;
    Ok(url_regex.is_match(text))
}

/// Returns `true` when `token_uri` matches one of the patterns historically
/// associated with spam/phishing NFT metadata hosts.
pub fn is_token_uri_suspicious(token_uri: &str) -> Result<bool, SpamScanError> {
    // Disposable TLDs commonly seen on phishing payloads, plus image/json
    // extensions appended with `?` or `%` (used to hide redirects).
    const PATTERNS: &[&str] = &[r"\.(xyz|gq|top)(/|$)", r"\.(json|xml|jpg|png)[%?]"];
    for pattern in PATTERNS {
        if Regex::new(pattern)?.is_match(token_uri) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Replace `*text` with [`REDACTION_PLACEHOLDER`] when it contains a URL
/// and `redact` is `true`. Returns whether a URL was detected.
pub fn redact_text_if_spam(text: &mut Option<String>, redact: bool) -> Result<bool, SpamScanError> {
    match text {
        Some(value) if contains_url(value)? => {
            if redact {
                *text = Some(REDACTION_PLACEHOLDER.to_string());
            }
            Ok(true)
        },
        _ => Ok(false),
    }
}

/// Run spam detection over every user-controlled string field on `nft`.
/// When `redact` is `true`, links are replaced in-place; in all cases a
/// detection toggles `nft.common.possible_spam` to `true`.
pub fn apply_spam_protection_to_nft(nft: &mut Nft, redact: bool) -> Result<(), SpamScanError> {
    let collection_hit = redact_text_if_spam(&mut nft.common.collection_name, redact)?;
    let symbol_hit = redact_text_if_spam(&mut nft.common.symbol, redact)?;
    let token_name_hit = redact_text_if_spam(&mut nft.uri_meta.token_name, redact)?;
    let metadata_hit = scan_metadata_for_links(nft, redact)?;

    if collection_hit || symbol_hit || token_name_hit || metadata_hit {
        nft.common.possible_spam = true;
    }
    Ok(())
}

/// Run spam detection over every user-controlled string field on
/// `transfer`. As with [`apply_spam_protection_to_nft`], a hit toggles
/// `transfer.common.possible_spam`.
pub fn apply_spam_protection_to_transfer(transfer: &mut NftTransfer, redact: bool) -> Result<(), SpamScanError> {
    let collection_hit = redact_text_if_spam(&mut transfer.collection_name, redact)?;
    let token_name_hit = redact_text_if_spam(&mut transfer.token_name, redact)?;

    if collection_hit || token_name_hit {
        transfer.common.possible_spam = true;
    }
    Ok(())
}

/// Inspect `nft.common.metadata` (a free-form JSON string published with the
/// token) for spam links inside the conventional `name` field.
fn scan_metadata_for_links(nft: &mut Nft, redact: bool) -> Result<bool, SpamScanError> {
    let metadata_str = match nft.common.metadata.as_ref() {
        Some(s) => s.clone(),
        None => return Ok(false),
    };
    let mut parsed: Map<String, Json> = match serde_json::from_str(&metadata_str) {
        Ok(map) => map,
        // Non-object metadata is left alone — there is no name field to scan.
        Err(_) => return Ok(false),
    };

    let hit = scan_named_metadata_field(&mut parsed, "name", redact)?;
    if redact && hit {
        nft.common.metadata = Some(serde_json::to_string(&parsed)?);
    }
    Ok(hit)
}

fn scan_named_metadata_field(
    metadata: &mut Map<String, Json>,
    field: &str,
    redact: bool,
) -> Result<bool, SpamScanError> {
    match metadata.get(field).and_then(Json::as_str) {
        Some(text) if contains_url(text)? => {
            if redact {
                metadata.insert(field.to_string(), Json::String(REDACTION_PLACEHOLDER.to_string()));
            }
            Ok(true)
        },
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_url_detects_https() {
        assert!(contains_url("Visit https://example.com now").unwrap());
    }

    #[test]
    fn contains_url_detects_bare_domain() {
        assert!(contains_url("free mint at example.io").unwrap());
    }

    #[test]
    fn contains_url_ignores_plain_text() {
        assert!(!contains_url("Cool Cats #123").unwrap());
    }

    #[test]
    fn suspicious_tld_flagged() {
        assert!(is_token_uri_suspicious("https://airdrop.xyz/0").unwrap());
        assert!(is_token_uri_suspicious("https://x.gq").unwrap());
        assert!(is_token_uri_suspicious("https://x.top/").unwrap());
    }

    #[test]
    fn percent_or_query_after_extension_flagged() {
        assert!(is_token_uri_suspicious("https://x.com/img.png?evil").unwrap());
        assert!(is_token_uri_suspicious("https://x.com/m.json%2e").unwrap());
    }

    #[test]
    fn benign_uri_not_flagged() {
        assert!(!is_token_uri_suspicious("https://ipfs.io/ipfs/Qm123/0.json").unwrap());
    }

    #[test]
    fn redact_replaces_when_url_present() {
        let mut text = Some("Mint at https://x.com".to_string());
        let hit = redact_text_if_spam(&mut text, true).unwrap();
        assert!(hit);
        assert_eq!(text.as_deref(), Some(REDACTION_PLACEHOLDER));
    }

    #[test]
    fn redact_leaves_text_when_redact_false() {
        let original = "Mint at https://x.com".to_string();
        let mut text = Some(original.clone());
        let hit = redact_text_if_spam(&mut text, false).unwrap();
        assert!(hit);
        assert_eq!(text, Some(original));
    }

    #[test]
    fn redact_noop_when_no_url() {
        let mut text = Some("Pixel Pals".to_string());
        let hit = redact_text_if_spam(&mut text, true).unwrap();
        assert!(!hit);
        assert_eq!(text.as_deref(), Some("Pixel Pals"));
    }

    #[test]
    fn metadata_name_field_redacted() {
        let mut map: Map<String, Json> = Map::new();
        map.insert("name".to_string(), Json::String("Claim at https://x.com".into()));
        let hit = scan_named_metadata_field(&mut map, "name", true).unwrap();
        assert!(hit);
        assert_eq!(map.get("name").and_then(Json::as_str), Some(REDACTION_PLACEHOLDER));
    }

    #[test]
    fn metadata_name_field_clean() {
        let mut map: Map<String, Json> = Map::new();
        map.insert("name".to_string(), Json::String("Pixel Pals #1".into()));
        let hit = scan_named_metadata_field(&mut map, "name", true).unwrap();
        assert!(!hit);
    }

    #[test]
    fn metadata_missing_field_is_clean() {
        let mut map: Map<String, Json> = Map::new();
        let hit = scan_named_metadata_field(&mut map, "name", true).unwrap();
        assert!(!hit);
    }
}
