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

impl SwapVersion {
    /// Returns `true` when this is the legacy (V1) swap protocol.
    pub fn is_legacy(&self) -> bool {
        self.version == LEGACY_SWAP_VERSION
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
}
