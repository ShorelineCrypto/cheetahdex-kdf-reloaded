//! Bitcoin script-level constants used by transaction validation logic.
//!
//! These values are dictated by Bitcoin protocol documentation (BIP-68 and the
//! `nLockTime` semantics from the original Bitcoin core sources). They are
//! protocol-defined facts, not creative content.

/// BIP-68: when set on `nSequence`, disables relative lock-time interpretation.
pub const SEQUENCE_LOCKTIME_DISABLE_FLAG: u32 = 1u32 << 31;

/// Sentinel value for `nSequence` that disables `nLockTime` for the input.
pub const SEQUENCE_FINAL: u32 = 0xffff_ffff;

/// BIP-68: when set on `nSequence`, the relative lock-time is measured in
/// 512-second units rather than blocks.
pub const SEQUENCE_LOCKTIME_TYPE_FLAG: u32 = 1 << 22;

/// BIP-68: bitmask used to extract the relative lock-time value from `nSequence`.
pub const SEQUENCE_LOCKTIME_MASK: u32 = 0x0000_ffff;

/// `nLockTime` boundary: below this it is interpreted as a block height,
/// otherwise as a UNIX timestamp.
pub const LOCKTIME_THRESHOLD: u32 = 500_000_000;

/// 1 coin = 100_000_000 satoshis.
pub const SATOSHIS_IN_COIN: u64 = 100_000_000;
