//! Write side of the codec: the [`Stream`] buffer, the [`Serializable`]
//! trait, and the `serialize*` helpers. Per-feature behaviour (currently
//! just witness inclusion) is signalled through the bitmask carried in
//! `Stream::flags` and queried by impls via `include_transaction_witness`.
//!
//! KDF-original.

use crate::varint::CompactInteger;
use primitives::bytes::Bytes;
use std::borrow::Borrow;
use std::io::{self, Write};

/// Bitmask flag: when set, the `Transaction` `Serializable` impl emits
/// the witness section (BIP-141 marker / flag / per-input witness stack).
/// When cleared, witnesses are omitted — the legacy pre-segwit layout.
pub const SERIALIZE_TRANSACTION_WITNESS: u32 = 0x40000000;

/// Convenience: serialize `t` with default flags (no witness).
pub fn serialize<T>(t: &T) -> Bytes
where
    T: Serializable,
{
    let mut s = Stream::default();
    s.append(t);
    s.out()
}

/// Convenience: serialize `t` with explicit `flags`.
pub fn serialize_with_flags<T>(t: &T, flags: u32) -> Bytes
where
    T: Serializable,
{
    let mut s = Stream::with_flags(flags);
    s.append(t);
    s.out()
}

/// Convenience: serialize `[K]` as a length-prefixed list of `T`.
pub fn serialize_list<T, K>(items: &[K]) -> Bytes
where
    T: Serializable,
    K: Borrow<T>,
{
    let mut s = Stream::default();
    s.append_list(items);
    s.out()
}

/// Total wire size of a length-prefixed list (default flags).
pub fn serialized_list_size<T, K>(items: &[K]) -> usize
where
    T: Serializable,
    K: Borrow<T>,
{
    CompactInteger::from(items.len()).serialized_size()
        + items
            .iter()
            .map(Borrow::borrow)
            .map(Serializable::serialized_size)
            .sum::<usize>()
}

/// Total wire size of a length-prefixed list with explicit `flags`
/// (forwarded to per-item `serialized_size_with_flags`).
pub fn serialized_list_size_with_flags<T, K>(items: &[K], flags: u32) -> usize
where
    T: Serializable,
    K: Borrow<T>,
{
    CompactInteger::from(items.len()).serialized_size()
        + items
            .iter()
            .map(Borrow::borrow)
            .map(|i| Serializable::serialized_size_with_flags(i, flags))
            .sum::<usize>()
}

/// Trait implemented by every type that can be written to the wire
/// through a [`Stream`].
pub trait Serializable {
    /// Emit the value into `s`.
    fn serialize(&self, s: &mut Stream);

    /// Predicted wire length (default flags). The default falls back on
    /// running the serializer into a temporary buffer.
    fn serialized_size(&self) -> usize
    where
        Self: Sized,
    {
        serialize(self).len()
    }

    /// Predicted wire length with explicit flags. The default also falls
    /// back on a temporary serialization.
    fn serialized_size_with_flags(&self, flags: u32) -> usize
    where
        Self: Sized,
    {
        serialize_with_flags(self, flags).len()
    }
}

/// In-memory write buffer for codec output. Carries a feature flag mask
/// queried by impls (see [`SERIALIZE_TRANSACTION_WITNESS`]).
#[derive(Default)]
pub struct Stream {
    buffer: Vec<u8>,
    flags: u32,
}

impl Stream {
    /// Empty stream (no flags set).
    pub fn new() -> Self {
        Stream {
            buffer: Vec::new(),
            flags: 0,
        }
    }

    /// Stream with the given flag mask.
    pub fn with_flags(flags: u32) -> Self {
        Stream {
            buffer: Vec::new(),
            flags,
        }
    }

    /// Whether the witness section should be included by the
    /// `Transaction` `Serializable` impl.
    pub fn include_transaction_witness(&self) -> bool { (self.flags & SERIALIZE_TRANSACTION_WITNESS) != 0 }

    /// Serialize `t` and append it.
    pub fn append<T>(&mut self, t: &T) -> &mut Self
    where
        T: Serializable,
    {
        t.serialize(self);
        self
    }

    /// Append raw bytes.
    pub fn append_slice(&mut self, bytes: &[u8]) -> &mut Self {
        // The underlying `Vec<u8>` write never fails, so the `Result` is
        // intentionally discarded.
        let _ = self.buffer.write_all(bytes);
        self
    }

    /// Append `items` as a length-prefixed list.
    pub fn append_list<T, K>(&mut self, items: &[K]) -> &mut Self
    where
        T: Serializable,
        K: Borrow<T>,
    {
        CompactInteger::from(items.len()).serialize(self);
        for it in items {
            it.borrow().serialize(self);
        }
        self
    }

    /// Consume the stream and return the accumulated bytes.
    pub fn out(self) -> Bytes { self.buffer.into() }
}

impl Write for Stream {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> { self.buffer.write(buf) }

    #[inline]
    fn flush(&mut self) -> Result<(), io::Error> { self.buffer.flush() }
}
