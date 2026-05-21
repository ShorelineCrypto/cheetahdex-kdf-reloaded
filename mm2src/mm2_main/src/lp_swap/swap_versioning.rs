//! Swap protocol version negotiation.
//!
//! `SwapVersion` wraps a u8 version number carried on orders and reservation
//! messages.  Legacy nodes that don't carry the field default to V1 (legacy
//! swap protocol).  Both sides must advertise V2 for a V2 swap to occur.

use serde::{Deserialize, Serialize};

/// Protocol version for atomic swaps.
///
/// Serialized as `{ "version": N }`.  When the field is omitted in P2P
/// messages the default (V1/legacy) is assumed, providing backward
/// compatibility with nodes that predate swap versioning.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SwapVersion {
    pub version: u8,
}

/// Legacy (V1) swap protocol — the only protocol currently executed.
pub const LEGACY_SWAP_VERSION: u8 = 1;

/// Trading Protocol Upgrade (V2) — state-machine-based.  
/// Dispatching to V2 requires both sides to advertise this version AND the
/// coin pair to implement the V2 swap ops traits.
pub const TPU_SWAP_VERSION: u8 = 2;

/// NFT swap V2 — Trading Protocol Upgrade extended with maker-side ERC-721
/// and ERC-1155 HTLC entrypoints (P10.3.7.d). A pair negotiates this only
/// when **both sides** advertise it AND the maker coin's `EthCoin` reports
/// a configured `nft_swap_v2_contract`. The taker side runs the existing
/// fungible TPU path \u2014 NFT swaps are NFT-for-fungible only.
pub const NFT_SWAP_V2_VERSION: u8 = 3;

impl SwapVersion {
    /// Returns `true` when this is the legacy (V1) swap protocol.
    pub fn is_legacy(&self) -> bool {
        self.version == LEGACY_SWAP_VERSION
    }

    /// Returns `true` when this is V2 (TPU) or any later state-machine
    /// variant (e.g. NFT V2). Use this to gate state-machine dispatch.
    pub fn is_v2_or_higher(&self) -> bool {
        self.version >= TPU_SWAP_VERSION
    }

    /// Returns `true` when this is the NFT swap V2 protocol.
    pub fn is_nft_v2(&self) -> bool {
        self.version == NFT_SWAP_V2_VERSION
    }

    /// Negotiate the swap version actually executed by a maker/taker pair.
    /// Returns the highest version both sides advertise (`min(maker, taker)`),
    /// safely defaulting to legacy when either side is on V1.
    pub fn negotiate(maker: SwapVersion, taker: SwapVersion) -> SwapVersion {
        SwapVersion {
            version: maker.version.min(taker.version),
        }
    }
}

impl Default for SwapVersion {
    /// Default to legacy so that deserialization of messages from pre-version nodes is safe.
    fn default() -> Self {
        SwapVersion {
            version: LEGACY_SWAP_VERSION,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_legacy() {
        assert!(SwapVersion::default().is_legacy());
    }

    #[test]
    fn test_v2_is_not_legacy() {
        let v2 = SwapVersion {
            version: TPU_SWAP_VERSION,
        };
        assert!(!v2.is_legacy());
    }

    #[test]
    fn test_serde_roundtrip() {
        let orig = SwapVersion {
            version: TPU_SWAP_VERSION,
        };
        let json = serde_json::to_string(&orig).unwrap();
        let back: SwapVersion = serde_json::from_str(&json).unwrap();
        assert_eq!(orig, back);
    }

    #[test]
    fn test_missing_field_defaults_to_legacy() {
        // Simulates a message from an old node that doesn't include swap_version
        #[derive(Deserialize)]
        struct Msg {
            #[serde(default)]
            swap_version: SwapVersion,
        }
        let msg: Msg = serde_json::from_str("{}").unwrap();
        assert!(msg.swap_version.is_legacy());
    }

    // P10.3.7.d — NFT-aware version negotiation
    fn v(n: u8) -> SwapVersion {
        SwapVersion { version: n }
    }

    #[test]
    fn nft_v2_is_recognised() {
        assert!(v(NFT_SWAP_V2_VERSION).is_nft_v2());
        assert!(!v(TPU_SWAP_VERSION).is_nft_v2());
        assert!(!v(LEGACY_SWAP_VERSION).is_nft_v2());
    }

    #[test]
    fn is_v2_or_higher_covers_tpu_and_nft() {
        assert!(!v(LEGACY_SWAP_VERSION).is_v2_or_higher());
        assert!(v(TPU_SWAP_VERSION).is_v2_or_higher());
        assert!(v(NFT_SWAP_V2_VERSION).is_v2_or_higher());
    }

    #[test]
    fn negotiate_picks_minimum_version() {
        // Both legacy → legacy
        assert_eq!(SwapVersion::negotiate(v(1), v(1)).version, 1);
        // Maker wants NFT V2, taker only TPU → fall back to TPU
        assert_eq!(
            SwapVersion::negotiate(v(NFT_SWAP_V2_VERSION), v(TPU_SWAP_VERSION)).version,
            TPU_SWAP_VERSION
        );
        // Either side legacy forces legacy
        assert_eq!(
            SwapVersion::negotiate(v(NFT_SWAP_V2_VERSION), v(LEGACY_SWAP_VERSION)).version,
            LEGACY_SWAP_VERSION
        );
        // Both NFT V2 → NFT V2
        assert_eq!(
            SwapVersion::negotiate(v(NFT_SWAP_V2_VERSION), v(NFT_SWAP_V2_VERSION)).version,
            NFT_SWAP_V2_VERSION
        );
    }

    #[test]
    fn negotiate_is_commutative() {
        for (a, b) in [(1, 2), (1, 3), (2, 3), (3, 1)] {
            assert_eq!(SwapVersion::negotiate(v(a), v(b)), SwapVersion::negotiate(v(b), v(a)));
        }
    }
}
