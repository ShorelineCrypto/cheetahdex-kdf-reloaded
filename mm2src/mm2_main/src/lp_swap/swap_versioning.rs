//! # Purpose
//! Negotiates the on-the-wire swap protocol version between maker and taker.
//!
//! # Public exports
//! - [`SwapVersion`] — the wire-serialized version tag (`{ "version": N }`).
//! - [`LEGACY_SWAP_VERSION`], [`TPU_SWAP_VERSION`], [`NFT_SWAP_V2_VERSION`] — known protocol numbers.
//!
//! # Invariants
//! - The struct shape `{ "version": u8 }` is wire-compatible with peers.
//! - `Default` resolves to legacy (V1) so that messages from pre-versioning
//!   nodes deserialize without error.
//! - [`SwapVersion::is_legacy`] is referenced from `serde(skip_serializing_if = ...)`
//!   attributes; renaming or removing it breaks order-message backward
//!   compatibility.

use serde::{Deserialize, Serialize};

/// Legacy (V1) swap protocol identifier.
///
/// Selected when either side is a pre-versioning node or explicitly opts out
/// of the upgraded protocol.
pub const LEGACY_SWAP_VERSION: u8 = 1;

/// Trading Protocol Upgrade (V2) identifier.
///
/// Both sides must advertise this version AND the coin pair must implement
/// the V2 swap-ops traits before a V2 swap is dispatched.
pub const TPU_SWAP_VERSION: u8 = 2;

/// NFT swap V2 identifier.
///
/// Extends [`TPU_SWAP_VERSION`] with maker-side ERC-721 / ERC-1155 HTLC
/// entrypoints. Negotiated only when both sides advertise it AND the
/// maker coin's `EthCoin` reports a configured `nft_swap_v2_contract`.
/// NFT swaps are NFT-for-fungible only — the taker side runs the existing
/// fungible TPU path.
pub const NFT_SWAP_V2_VERSION: u8 = 3;

/// Wire-serialized swap protocol version tag.
///
/// Carried on order, reservation, and connection messages as
/// `{ "version": N }`. Omitting the field on the wire deserializes to
/// [`LEGACY_SWAP_VERSION`] via [`SwapVersion::default`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SwapVersion {
    pub version: u8,
}

impl SwapVersion {
    /// Returns `true` when this tag is the legacy (V1) protocol.
    ///
    /// Used by `serde(skip_serializing_if = "SwapVersion::is_legacy")` on
    /// order-message fields to keep the legacy wire format unchanged for
    /// nodes that never upgrade.
    pub fn is_legacy(&self) -> bool {
        self.version == LEGACY_SWAP_VERSION
    }

    /// Returns `true` when this tag is V2 (TPU) or any later state-machine
    /// variant such as NFT V2.
    ///
    /// Use this predicate to gate dispatch into the state-machine swap path.
    pub fn is_v2_or_higher(&self) -> bool {
        self.version >= TPU_SWAP_VERSION
    }

    /// Returns `true` when this tag is exactly the NFT swap V2 protocol.
    pub fn is_nft_v2(&self) -> bool {
        self.version == NFT_SWAP_V2_VERSION
    }

    /// Returns the version actually executed by a maker/taker pair.
    ///
    /// Picks the lowest of the two advertised tags so that a peer that
    /// only knows version `n` can still complete a swap with a peer that
    /// knows `n + 1`.
    pub fn negotiate(maker: SwapVersion, taker: SwapVersion) -> SwapVersion {
        SwapVersion {
            version: maker.version.min(taker.version),
        }
    }
}

impl Default for SwapVersion {
    /// Defaults to [`LEGACY_SWAP_VERSION`] so that messages from pre-versioning
    /// nodes deserialize without losing the swap.
    fn default() -> Self {
        SwapVersion {
            version: LEGACY_SWAP_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version_tag(n: u8) -> SwapVersion {
        SwapVersion { version: n }
    }

    #[test]
    fn should_default_to_legacy_when_constructed_via_default() {
        assert!(SwapVersion::default().is_legacy());
    }

    #[test]
    fn should_report_not_legacy_when_version_is_v2() {
        assert!(!version_tag(TPU_SWAP_VERSION).is_legacy());
    }

    #[test]
    fn should_roundtrip_through_serde_json() {
        let original = version_tag(TPU_SWAP_VERSION);
        let encoded = serde_json::to_string(&original).expect("serialize");
        let decoded: SwapVersion = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(original, decoded);
    }

    #[test]
    fn should_default_to_legacy_when_field_missing_from_payload() {
        // Simulates a message authored by a node that predates the swap_version field.
        #[derive(Deserialize)]
        struct LegacyPayload {
            #[serde(default)]
            swap_version: SwapVersion,
        }
        let payload: LegacyPayload = serde_json::from_str("{}").expect("deserialize");
        assert!(payload.swap_version.is_legacy());
    }

    #[test]
    fn should_recognise_nft_v2_tag() {
        assert!(version_tag(NFT_SWAP_V2_VERSION).is_nft_v2());
        assert!(!version_tag(TPU_SWAP_VERSION).is_nft_v2());
        assert!(!version_tag(LEGACY_SWAP_VERSION).is_nft_v2());
    }

    #[test]
    fn should_classify_tpu_and_nft_as_v2_or_higher() {
        assert!(!version_tag(LEGACY_SWAP_VERSION).is_v2_or_higher());
        assert!(version_tag(TPU_SWAP_VERSION).is_v2_or_higher());
        assert!(version_tag(NFT_SWAP_V2_VERSION).is_v2_or_higher());
    }

    #[test]
    fn should_negotiate_minimum_when_versions_differ() {
        // Both legacy yields legacy.
        assert_eq!(SwapVersion::negotiate(version_tag(1), version_tag(1)).version, 1);

        // Maker advertises NFT V2 while taker only knows TPU; pair settles on TPU.
        assert_eq!(
            SwapVersion::negotiate(version_tag(NFT_SWAP_V2_VERSION), version_tag(TPU_SWAP_VERSION)).version,
            TPU_SWAP_VERSION
        );

        // A legacy peer on either side forces the whole pair back to legacy.
        assert_eq!(
            SwapVersion::negotiate(version_tag(NFT_SWAP_V2_VERSION), version_tag(LEGACY_SWAP_VERSION)).version,
            LEGACY_SWAP_VERSION
        );

        // Pair where both advertise NFT V2 settles on NFT V2.
        assert_eq!(
            SwapVersion::negotiate(version_tag(NFT_SWAP_V2_VERSION), version_tag(NFT_SWAP_V2_VERSION)).version,
            NFT_SWAP_V2_VERSION
        );
    }
}
