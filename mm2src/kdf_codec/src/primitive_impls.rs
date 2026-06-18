//! `Serializable`/`Deserializable` impls for primitives:
//!
//! - bool, signed/unsigned integers (i32/i64/u8/u16/u32/u64)
//! - `String` and `&str`
//! - all fixed-size hash newtypes from `kdf_primitives`
//! - `Bytes` with a 64 KiB upper bound on the decoded length
//! - `Compact` (transparent `u32` LE)
//!
//! Encodings follow the public Bitcoin protocol convention:
//! little-endian fixed integers, byte-prefix for blobs.
//!
//! KDF-original.

use crate::varint::CompactInteger;
use crate::{Deserializable, Error, Reader, Serializable, Stream};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use primitives::bytes::Bytes;
use primitives::compact::Compact;
use primitives::hash::{CipherText, EncCipherText, EquihashSolution, OutCipherText, ZkProof, ZkProofSapling, H128,
                       H160, H256, H264, H32, H48, H512, H520, H64, H96};
use std::io;

// ── Booleans ─────────────────────────────────────────────────────────────

impl Serializable for bool {
    #[inline]
    fn serialize(&self, s: &mut Stream) {
        // The Vec-backed Stream write never fails.
        let _ = s.write_u8(*self as u8);
    }

    #[inline]
    fn serialized_size(&self) -> usize { 1 }
}

impl Deserializable for bool {
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        match reader.read_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(Error::MalformedData),
        }
    }
}

// ── Fixed-size little-endian integers ────────────────────────────────────

macro_rules! impl_le_int {
    ($t:ty, $write:ident, $read:ident, $size:expr) => {
        impl Serializable for $t {
            #[inline]
            fn serialize(&self, s: &mut Stream) { let _ = s.$write::<LittleEndian>(*self); }

            #[inline]
            fn serialized_size(&self) -> usize { $size }
        }

        impl Deserializable for $t {
            #[inline]
            fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
            where
                T: io::Read,
            {
                Ok(reader.$read::<LittleEndian>()?)
            }
        }
    };
}

impl_le_int!(i32, write_i32, read_i32, 4);
impl_le_int!(i64, write_i64, read_i64, 8);
impl_le_int!(u16, write_u16, read_u16, 2);
impl_le_int!(u32, write_u32, read_u32, 4);
impl_le_int!(u64, write_u64, read_u64, 8);

// u8 is special-cased — byteorder's read_u8/write_u8 don't take an endian
// generic parameter.
impl Serializable for u8 {
    #[inline]
    fn serialize(&self, s: &mut Stream) { let _ = s.write_u8(*self); }

    #[inline]
    fn serialized_size(&self) -> usize { 1 }
}

impl Deserializable for u8 {
    #[inline]
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        Ok(reader.read_u8()?)
    }
}

// ── Strings (length-prefixed UTF-8) ──────────────────────────────────────

impl Serializable for String {
    fn serialize(&self, stream: &mut Stream) {
        let raw: &[u8] = self.as_ref();
        stream.append(&CompactInteger::from(raw.len())).append_slice(raw);
    }

    fn serialized_size(&self) -> usize {
        let raw: &[u8] = self.as_ref();
        CompactInteger::from(raw.len()).serialized_size() + raw.len()
    }
}

impl Serializable for &str {
    fn serialize(&self, stream: &mut Stream) {
        let raw = self.as_bytes();
        stream.append(&CompactInteger::from(raw.len())).append_slice(raw);
    }

    fn serialized_size(&self) -> usize {
        let raw = self.as_bytes();
        CompactInteger::from(raw.len()).serialized_size() + raw.len()
    }
}

impl Deserializable for String {
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        let bytes: Bytes = reader.read()?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

// ── Fixed-size hash newtypes ─────────────────────────────────────────────

macro_rules! impl_hash_codec {
    ($t:ty, $size:expr) => {
        impl Serializable for $t {
            fn serialize(&self, stream: &mut Stream) { stream.append_slice(&**self); }

            #[inline]
            fn serialized_size(&self) -> usize { $size }
        }

        impl Deserializable for $t {
            fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
            where
                T: io::Read,
            {
                let mut out = Self::default();
                reader.read_slice(&mut *out)?;
                Ok(out)
            }
        }
    };
}

impl_hash_codec!(H32, 4);
impl_hash_codec!(H48, 6);
impl_hash_codec!(H64, 8);
impl_hash_codec!(H96, 12);
impl_hash_codec!(H128, 16);
impl_hash_codec!(H160, 20);
impl_hash_codec!(H256, 32);
impl_hash_codec!(H264, 33);
impl_hash_codec!(H512, 64);
impl_hash_codec!(H520, 65);
impl_hash_codec!(OutCipherText, 80);
impl_hash_codec!(ZkProofSapling, 192);
impl_hash_codec!(ZkProof, 296);
impl_hash_codec!(EncCipherText, 580);
impl_hash_codec!(CipherText, 601);
impl_hash_codec!(EquihashSolution, 1344);

// ── Variable-length byte buffer with bound ───────────────────────────────

/// Hard upper bound on the number of bytes a single `Bytes` can decode
/// to. Guards against attacker-controlled CompactInteger lengths driving
/// pathological allocations.
const BYTES_DECODE_LIMIT: u64 = 65_536;

impl Serializable for Bytes {
    fn serialize(&self, stream: &mut Stream) { stream.append(&CompactInteger::from(self.len())).append_slice(self); }

    #[inline]
    fn serialized_size(&self) -> usize { CompactInteger::from(self.len()).serialized_size() + self.len() }
}

impl Deserializable for Bytes {
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        let len = reader.read::<CompactInteger>()?;
        if u64::from(len) > BYTES_DECODE_LIMIT {
            return Err(Error::MalformedData);
        }
        let mut out = Bytes::new_with_len(len.into());
        reader.read_slice(&mut out)?;
        Ok(out)
    }
}

// ── Compact target (nBits) ───────────────────────────────────────────────

impl Serializable for Compact {
    fn serialize(&self, stream: &mut Stream) { stream.append(&u32::from(*self)); }
}

impl Deserializable for Compact {
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        reader.read::<u32>().map(Compact::new)
    }
}

#[cfg(test)]
mod tests {
    use crate::{deserialize, deserialize_iterator, serialize, Error, Reader, Stream};
    use primitives::bytes::Bytes;

    #[test]
    fn read_back_to_back_integers() {
        let buffer = vec![1, 2, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0];
        let mut reader = Reader::new(&buffer);
        assert!(!reader.is_finished());
        assert_eq!(1u8, reader.read().unwrap());
        assert_eq!(2u16, reader.read().unwrap());
        assert_eq!(3u32, reader.read().unwrap());
        assert_eq!(4u64, reader.read().unwrap());
        assert!(reader.is_finished());
        assert_eq!(Error::UnexpectedEnd, reader.read::<u8>().unwrap_err());
    }

    #[test]
    fn iterate_until_exhausted() {
        let buffer = vec![1u8, 0, 2, 0, 3, 0, 4, 0];
        let result: Vec<u16> = deserialize_iterator(&buffer as &[u8])
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(result, vec![1u16, 2, 3, 4]);
    }

    #[test]
    fn stream_appends_match_byte_layout() {
        let mut stream = Stream::default();
        stream.append(&1u8).append(&2u16).append(&3u32).append(&4u64);
        let expected: Bytes = vec![1u8, 2, 0, 3, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0].into();
        assert_eq!(stream.out(), expected);
    }

    #[test]
    fn bytes_round_trip() {
        let raw: Bytes = "020145".into();
        let expected: Bytes = "0145".into();
        assert_eq!(expected, deserialize::<_, Bytes>(raw.as_ref()).unwrap());
        assert_eq!(serialize(&expected), raw);
    }

    #[test]
    fn string_round_trip() {
        for (hex, s) in [("0776657273696f6e", "version"), ("00", "")] {
            let raw: Bytes = hex.into();
            let value: String = s.into();
            assert_eq!(serialize(&value), raw);
            assert_eq!(deserialize::<_, String>(raw.as_ref()).unwrap(), value);
        }
    }

    #[test]
    fn append_slice_writes_raw() {
        let mut slice = [0u8; 4];
        slice[0] = 0x64;
        let mut stream = Stream::default();
        stream.append_slice(&slice);
        assert_eq!(stream.out(), "64000000".into());
    }
}
