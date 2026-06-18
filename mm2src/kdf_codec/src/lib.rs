//! Bitcoin-style binary codec used across the KDF UTXO machinery.
//!
//! Public surface is split into four modules:
//!
//! - [`varint`] — `CompactInteger` and `parse_compact_int`
//! - [`reader`] — `Reader`, `Deserializable`, `Error`, `CoinVariant`,
//!   `ReadIterator`, and the `deserialize` helpers
//! - [`stream`] — `Stream`, `Serializable`, the `serialize` helpers,
//!   and the `SERIALIZE_TRANSACTION_WITNESS` flag
//! - [`primitive_impls`] — `Serializable`/`Deserializable` for built-in
//!   integers, `String`, the `H*` hash newtypes, `Bytes`, and `Compact`
//! - [`list`] — `List<T>` wrapper that round-trips a `Vec<T>` with a
//!   length prefix
//!
//! KDF-original. Phase B (B.5) replacement for `mm2_bitcoin/serialization`.

mod list;
mod primitive_impls;
mod reader;
mod stream;
mod varint;

pub use primitives::{bytes, compact, hash};

pub use list::List;
pub use reader::{deserialize, deserialize_iterator, CoinVariant, Deserializable, Error, ReadIterator, Reader};
pub use stream::{serialize, serialize_list, serialize_with_flags, serialized_list_size,
                 serialized_list_size_with_flags, Serializable, Stream, SERIALIZE_TRANSACTION_WITNESS};
pub use varint::{parse_compact_int, CompactInteger};
