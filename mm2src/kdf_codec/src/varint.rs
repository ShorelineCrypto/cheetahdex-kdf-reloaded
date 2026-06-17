//! Bitcoin P2P "compact size" variable-length unsigned integer.
//!
//! Layout (per the Bitcoin protocol specification):
//!
//! | Range                          | Encoding                            |
//! |--------------------------------|-------------------------------------|
//! | `0 ..= 0xfc`                   | one byte: the value                 |
//! | `0xfd ..= 0xffff`              | `0xfd` then `u16` little-endian     |
//! | `0x10000 ..= 0xffff_ffff`      | `0xfe` then `u32` little-endian     |
//! | `≥ 0x1_0000_0000`              | `0xff` then `u64` little-endian     |
//!
//! KDF-original.

use crate::{Deserializable, Error, Reader, Serializable, Stream};
use std::{fmt, io};

/// Errors raised by [`parse_compact_int`].
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompactIntegerError {
    ParseError(String),
}

/// A Bitcoin P2P CompactSize unsigned integer.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CompactInteger(u64);

impl CompactInteger {
    /// Underlying value as a `usize`.
    pub fn as_usize(&self) -> usize {
        self.0 as usize
    }

    /// Number of bytes the value occupies on the wire.
    pub fn serialized_length(&self) -> usize {
        match self.0 {
            0..=0xfc => 1,
            0xfd..=0xffff => 3,
            0x10000..=0xffff_ffff => 5,
            _ => 9,
        }
    }

    /// Number of payload bytes that follow the length-flag byte
    /// (`0xfd` / `0xfe` / `0xff`). Returns `0` for the inline
    /// single-byte encoding.
    pub fn data_length(flag: u8) -> u8 {
        match flag {
            0xfd => 2,
            0xfe => 4,
            0xff => 8,
            _ => 0,
        }
    }
}

/// Parse a CompactInteger off the front of `buf`. Returns the value;
/// the number of bytes consumed can be recovered with
/// [`CompactInteger::serialized_length`].
pub fn parse_compact_int<T: AsRef<[u8]> + ?Sized>(buf: &T) -> Result<CompactInteger, CompactIntegerError> {
    let buf = buf.as_ref();
    if buf.is_empty() {
        return Err(CompactIntegerError::ParseError("Empty buffer!".into()));
    }
    let payload_len = CompactInteger::data_length(buf[0]) as usize;

    if payload_len == 0 {
        return Ok(buf[0].into());
    }
    if buf.len() < 1 + payload_len {
        return Err(CompactIntegerError::ParseError("Insufficient bytes!".into()));
    }

    let mut padded = [0u8; 8];
    padded[..payload_len].copy_from_slice(&buf[1..=payload_len]);
    Ok(u64::from_le_bytes(padded).into())
}

impl fmt::Display for CompactInteger {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<CompactInteger> for usize {
    fn from(c: CompactInteger) -> Self {
        c.0 as usize
    }
}

impl From<CompactInteger> for u64 {
    fn from(c: CompactInteger) -> Self {
        c.0
    }
}

impl From<u8> for CompactInteger {
    fn from(v: u8) -> Self {
        CompactInteger(v as u64)
    }
}

impl From<u16> for CompactInteger {
    fn from(v: u16) -> Self {
        CompactInteger(v as u64)
    }
}

impl From<u32> for CompactInteger {
    fn from(v: u32) -> Self {
        CompactInteger(v as u64)
    }
}

impl From<usize> for CompactInteger {
    fn from(v: usize) -> Self {
        CompactInteger(v as u64)
    }
}

impl From<u64> for CompactInteger {
    fn from(v: u64) -> Self {
        CompactInteger(v)
    }
}

impl AsRef<u64> for CompactInteger {
    fn as_ref(&self) -> &u64 {
        &self.0
    }
}

impl Serializable for CompactInteger {
    fn serialize(&self, stream: &mut Stream) {
        match self.0 {
            v @ 0..=0xfc => {
                stream.append(&(v as u8));
            },
            v @ 0xfd..=0xffff => {
                stream.append(&0xfdu8).append(&(v as u16));
            },
            v @ 0x10000..=0xffff_ffff => {
                stream.append(&0xfeu8).append(&(v as u32));
            },
            v => {
                stream.append(&0xffu8).append(&v);
            },
        }
    }

    fn serialized_size(&self) -> usize {
        self.serialized_length()
    }
}

impl Deserializable for CompactInteger {
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        let v: u64 = match reader.read::<u8>()? {
            i @ 0..=0xfc => i.into(),
            0xfd => reader.read::<u16>()?.into(),
            0xfe => reader.read::<u32>()?.into(),
            _ => reader.read::<u64>()?,
        };
        Ok(CompactInteger(v))
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_compact_int, CompactInteger, CompactIntegerError};
    use crate::{Error, Reader, Stream};
    use test_helpers::hex::force_deserialize_hex;

    #[test]
    fn data_length_decodes_flag_byte() {
        assert_eq!(CompactInteger::data_length(1), 0);
        assert_eq!(CompactInteger::data_length(253), 2);
        assert_eq!(CompactInteger::data_length(254), 4);
        assert_eq!(CompactInteger::data_length(255), 8);
    }

    #[test]
    fn parse_examples_from_spec() {
        assert_eq!(parse_compact_int(&force_deserialize_hex("0x01")).unwrap().as_usize(), 1);
        assert_eq!(
            parse_compact_int(&force_deserialize_hex("0xff0000000000000000"))
                .unwrap()
                .as_usize(),
            0
        );
        assert_eq!(
            parse_compact_int(&force_deserialize_hex("0xfe03000000"))
                .unwrap()
                .as_usize(),
            3
        );
        assert_eq!(
            parse_compact_int(&force_deserialize_hex("0xfd0001"))
                .unwrap()
                .as_usize(),
            256
        );
    }

    #[test]
    fn parse_rejects_truncated_inputs() {
        for hex in ["0xfd01", "0xfe010000", "0xff01000000000000"] {
            let buf = force_deserialize_hex(hex);
            let err = parse_compact_int(&buf).unwrap_err();
            assert_eq!(err, CompactIntegerError::ParseError("Insufficient bytes!".into()));
        }

        let err = parse_compact_int(&force_deserialize_hex("0x")).unwrap_err();
        assert_eq!(err, CompactIntegerError::ParseError("Empty buffer!".into()));
    }

    #[test]
    fn round_trip_through_stream_and_reader() {
        let values = [0u64, 0xfc, 0xfd, 0xffff, 0x10000, 0xffff_ffff, 0x1_0000_0000];

        let mut stream = Stream::default();
        for v in values {
            stream.append(&CompactInteger::from(v));
        }
        let bytes = stream.out();

        let mut reader = Reader::new(bytes.as_ref());
        for expected in values {
            let read: CompactInteger = reader.read().unwrap();
            assert_eq!(read, CompactInteger::from(expected));
        }
        assert_eq!(reader.read::<CompactInteger>().unwrap_err(), Error::UnexpectedEnd);
    }
}
