use primitives::bytes::Bytes;
use primitives::hash::H256;
use serialization::{serialize, Deserializable, Reader, Serializable, Stream};
use std::io;

use crate::header::BlockHeader;

/// Minimum on-the-wire size for a Bitcoin-style block header (version, prev,
/// merkle root, time, bits, nonce — 4 + 32 + 32 + 4 + 4 + 4 = 80 bytes).
pub const MIN_RAW_HEADER_SIZE: usize = 80;

/// Owning wrapper around the raw header bytes as transmitted on the wire.
///
/// Keeping the bytes as-is (rather than always re-serialising the parsed
/// `BlockHeader`) is important for SPV: hash digests and partial field reads
/// (`extract_merkle_root`, `parent`) must always reflect the *exact* bytes the
/// peer sent us, even across multi-coin variants whose canonical encoder might
/// differ from a particular peer's emission.
#[derive(Default, PartialEq, Clone, Eq, Hash)]
pub struct RawBlockHeader(Bytes);

#[derive(Debug, Clone, PartialEq)]
pub enum RawHeaderError {
    WrongLengthHeader { min_length: usize },
}

impl RawBlockHeader {
    /// Constructs from raw bytes; rejects anything shorter than the standard
    /// Bitcoin 80-byte header.
    pub fn new<B: Into<Bytes>>(bytes: B) -> Result<RawBlockHeader, RawHeaderError> {
        let inner = bytes.into();
        if inner.len() < MIN_RAW_HEADER_SIZE {
            return Err(RawHeaderError::WrongLengthHeader {
                min_length: MIN_RAW_HEADER_SIZE,
            });
        }
        Ok(RawBlockHeader(inner))
    }

    /// Block hash = DSHA-256 of the entire raw header bytes.
    pub fn digest(&self) -> H256 {
        crypto::dhash256(&self.0)
    }

    /// Bytes 36..68 of the standard Bitcoin header layout.
    pub fn extract_merkle_root(&self) -> H256 {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.0[36..68]);
        H256::from(out)
    }

    /// Bytes 4..36 of the standard Bitcoin header layout.
    pub fn parent(&self) -> H256 {
        let mut out = [0u8; 32];
        out.copy_from_slice(&self.0[4..36]);
        H256::from(out)
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for RawBlockHeader {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<BlockHeader> for RawBlockHeader {
    fn from(header: BlockHeader) -> Self {
        RawBlockHeader(serialize(&header).take().into())
    }
}

impl Serializable for RawBlockHeader {
    fn serialize(&self, stream: &mut Stream) {
        stream.append(&self.0);
    }
}

impl Deserializable for RawBlockHeader {
    fn deserialize<R: io::Read>(reader: &mut Reader<R>) -> Result<Self, serialization::Error>
    where
        Self: Sized,
    {
        let bytes: Bytes = reader.read()?;
        RawBlockHeader::new(bytes).map_err(|_| serialization::Error::MalformedData)
    }
}

impl std::fmt::Debug for RawBlockHeader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hex_repr: String = hex::ToHex::to_hex(&self.0[..]);
        f.debug_tuple("RawBlockHeader").field(&hex_repr).finish()
    }
}
