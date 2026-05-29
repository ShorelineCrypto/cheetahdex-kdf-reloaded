//! KDF primitives — fixed-size hashes, big integers, compact difficulty,
//! and byte-vector wrappers.
//!
//! KDF-original. Phase B (B.4) replacement for `mm2_bitcoin/primitives`.

#![allow(clippy::assign_op_pattern)]
#![allow(clippy::ptr_offset_with_cast)]
#![allow(clippy::manual_div_ceil)]

use uint::construct_uint;

construct_uint! {
    /// 256-bit unsigned integer used for hash arithmetic and
    /// proof-of-work target computation.
    pub struct U256(4);
}

pub mod bytes;
pub mod compact;
pub mod hash;
