use crypto::dhash256;
use primitives::hash::H256;
use serialization::{Deserializable, Error as ReaderError, Reader};
use std::io;

/// Result of `ReadAndHash::read_and_hash` — captures the deserialized value
/// alongside its DSHA-256 hash (computed over the raw bytes the reader saw)
/// and the total byte size that was consumed.
pub struct HashedData<T> {
    pub size: usize,
    pub hash: H256,
    pub data: T,
}

/// Helper trait for streaming a value out of a reader while incrementally
/// hashing every byte the reader yields. Used by indexed-block flows where the
/// header / transaction hash is needed alongside the parsed value.
pub trait ReadAndHash {
    fn read_and_hash<T>(&mut self) -> Result<HashedData<T>, ReaderError>
    where
        T: Deserializable;
}

impl<R> ReadAndHash for Reader<R>
where
    R: io::Read,
{
    fn read_and_hash<T>(&mut self) -> Result<HashedData<T>, ReaderError>
    where
        T: Deserializable,
    {
        let mut size = 0usize;
        let mut bytes_seen: Vec<u8> = Vec::new();
        let data = self.read_with_proxy(|chunk| {
            size += chunk.len();
            bytes_seen.extend_from_slice(chunk);
        })?;
        Ok(HashedData {
            size,
            hash: dhash256(&bytes_seen),
            data,
        })
    }
}
