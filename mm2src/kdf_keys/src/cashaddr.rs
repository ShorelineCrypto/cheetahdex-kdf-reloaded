// Bitcoin Cash CashAddr (BCH spec).
//
// Reference: https://github.com/bitcoincashorg/bitcoincash.org/blob/master/spec/cashaddr.md
//
// This module is a from-spec implementation. The wire format is a
// human-readable network prefix, a `:` separator, and a base32-encoded
// payload terminated by a 40-bit BCH checksum.

use std::fmt;
use std::str::FromStr;

const CHARSET: [char; 32] = [
    'q', 'p', 'z', 'r', 'y', '9', 'x', '8', 'g', 'f', '2', 't', 'v', 'd', 'w', '0', 's', '3', 'j', 'n', '5', '4', 'k',
    'h', 'c', 'e', '6', 'm', 'u', 'a', '7', 'l',
];

/// Inverse of `CHARSET`: base32 char → 5-bit value, or -1 if invalid.
const CHARSET_REV: [i8; 128] = {
    let mut t = [-1i8; 128];
    let mut i = 0;
    while i < 32 {
        t[CHARSET[i] as usize] = i as i8;
        // case-insensitive: also map uppercase
        let c = CHARSET[i] as u8;
        if c.is_ascii_lowercase() {
            t[(c - 32) as usize] = i as i8;
        }
        i += 1;
    }
    t
};

const BCH_GENERATORS: [u64; 5] = [0x98f2bc8e61, 0x79b76d99e2, 0xf33e5fb3c4, 0xae2eabe2a8, 0x1e4f43e470];

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum AddressType {
    P2PKH,
    P2SH,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NetworkPrefix {
    BitcoinCash,
    BchTest,
    BchReg,
    /// SLP on BCH mainnet
    SimpleLedger,
    /// SLP on BCH testnet
    SlpTest,
    Other(String),
}

impl fmt::Display for NetworkPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            NetworkPrefix::BitcoinCash => "bitcoincash",
            NetworkPrefix::BchTest => "bchtest",
            NetworkPrefix::BchReg => "bchreg",
            NetworkPrefix::SimpleLedger => "simpleledger",
            NetworkPrefix::SlpTest => "slptest",
            NetworkPrefix::Other(s) => s,
        })
    }
}

impl FromStr for NetworkPrefix {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        Ok(match lower.as_str() {
            "bitcoincash" => NetworkPrefix::BitcoinCash,
            "bchtest" => NetworkPrefix::BchTest,
            "bchreg" => NetworkPrefix::BchReg,
            "simpleledger" => NetworkPrefix::SimpleLedger,
            "slptest" => NetworkPrefix::SlpTest,
            _ => NetworkPrefix::Other(lower),
        })
    }
}

impl From<&'static str> for NetworkPrefix {
    fn from(s: &str) -> Self {
        s.parse().expect("infallible")
    }
}

impl NetworkPrefix {
    /// Per spec the prefix is contributed to the checksum as one 5-bit
    /// value per character (the low 5 bits of the ASCII byte) followed
    /// by a single zero terminator.
    fn checksum_seed(&self) -> Vec<u8> {
        let mut out: Vec<u8> = self.to_string().bytes().map(|b| b & 0x1f).collect();
        out.push(0);
        out
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CashAddress {
    pub prefix: NetworkPrefix,
    pub hash: Vec<u8>,
    pub address_type: AddressType,
}

impl CashAddress {
    pub fn new(prefix: &str, hash: Vec<u8>, address_type: AddressType) -> Result<Self, String> {
        match hash.len() {
            20 | 24 | 28 | 32 | 40 | 48 | 56 | 64 => {},
            n => return Err(format!("Unexpected hash size {}", n)),
        }
        Ok(CashAddress {
            prefix: prefix.parse()?,
            hash,
            address_type,
        })
    }

    pub fn decode(addr: &str) -> Result<Self, String> {
        let (prefix, payload_str) = match addr.find(':') {
            Some(i) => (addr[..i].parse()?, &addr[i + 1..]),
            None => (NetworkPrefix::BitcoinCash, addr),
        };

        if mixed_case(payload_str) {
            return Err("cashaddress contains mixed upper and lowercase characters".into());
        }

        let payload5 = decode_base32(payload_str)?;

        if poly_mod_with_prefix(&prefix, &payload5) != 0 {
            return Err("Checksum verification failed".into());
        }
        if payload5.len() < 9 {
            return Err("Insufficient packed data to decode".into());
        }
        let body5 = &payload5[..payload5.len() - 8];
        let mut body8 = repack_bits(body5, 5, 8, false).0;

        let version = body8.remove(0);
        if version & 0x80 != 0 {
            return Err("The version byte's most significant bit is reserved and must be 0".into());
        }
        let address_type = match version >> 3 {
            0 => AddressType::P2PKH,
            1 => AddressType::P2SH,
            _ => return Err("Unexpected address type".into()),
        };
        let expected = hash_size_from_version(version);
        if body8.len() != expected {
            return Err(format!(
                "Incorrect address hash len: expected={}, actual={}",
                expected,
                body8.len()
            ));
        }
        Ok(CashAddress {
            prefix,
            hash: body8,
            address_type,
        })
    }

    pub fn encode(&self) -> Result<String, String> {
        let version = self.version_byte()?;
        let mut body8 = Vec::with_capacity(1 + self.hash.len());
        body8.push(version);
        body8.extend_from_slice(&self.hash);

        let body5 = repack_bits(&body8, 8, 5, true).0;

        // Compute BCH checksum: append 8 zero "phantom" 5-bit symbols, take poly_mod, then
        // the resulting 40 bits are emitted big-endian as 8 base32 symbols.
        let mut buf = body5.clone();
        buf.extend_from_slice(&[0u8; 8]);
        let chk = poly_mod_with_prefix(&self.prefix, &buf);

        let mut payload5 = body5;
        for i in 0..8 {
            payload5.push(((chk >> (5 * (7 - i))) & 0x1f) as u8);
        }
        Ok(format!("{}:{}", self.prefix, encode_base32(&payload5)?))
    }

    fn version_byte(&self) -> Result<u8, String> {
        let typ: u8 = match self.address_type {
            AddressType::P2PKH => 0,
            AddressType::P2SH => 1,
        };
        let size: u8 = match self.hash.len() {
            20 => 0,
            24 => 1,
            28 => 2,
            32 => 3,
            40 => 4,
            48 => 5,
            56 => 6,
            64 => 7,
            n => return Err(format!("Unexpected hash size {}", n)),
        };
        Ok((typ << 3) | size)
    }
}

impl FromStr for CashAddress {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        CashAddress::decode(s)
    }
}

impl From<&'static str> for CashAddress {
    fn from(s: &'static str) -> Self {
        s.parse().expect("valid cashaddr literal")
    }
}

fn hash_size_from_version(v: u8) -> usize {
    match v & 0x07 {
        0 => 20,
        1 => 24,
        2 => 28,
        3 => 32,
        4 => 40,
        5 => 48,
        6 => 56,
        _ => 64,
    }
}

/// BCH checksum function: 5-bit-symbol BCH code over GF(2^5).
/// Returns 40 bits packed into a u64. Receiver should verify == 0.
fn poly_mod_with_prefix(prefix: &NetworkPrefix, payload: &[u8]) -> u64 {
    let mut data = prefix.checksum_seed();
    data.extend_from_slice(payload);
    let mut c: u64 = 1;
    for d in data {
        let c0 = (c >> 35) as u8;
        c = ((c & 0x07_ffff_ffff) << 5) ^ d as u64;
        for (i, g) in BCH_GENERATORS.iter().enumerate() {
            if c0 & (1 << i) != 0 {
                c ^= g;
            }
        }
    }
    c ^ 1
}

/// Repack a byte buffer between two bit-widths (used for the
/// 8↔5-bit conversion the spec calls for). Returns the new buffer
/// and a flag indicating whether output was complete (`true` if no
/// pending bits, or all pending bits were padded).
fn repack_bits(input: &[u8], from: u8, to: u8, pad: bool) -> (Vec<u8>, bool) {
    debug_assert!(from > 0 && from <= 8 && to > 0 && to <= 8);
    let mask: u64 = (1u64 << to) - 1;
    let max_acc: u64 = (1u64 << (from + to - 1)) - 1;
    let mut acc: u64 = 0;
    let mut bits: u64 = 0;
    let from = from as u64;
    let to = to as u64;
    let mut out = Vec::with_capacity(input.len() * from as usize / to as usize + 1);
    for &b in input {
        acc = ((acc << from) | b as u64) & max_acc;
        bits += from;
        while bits >= to {
            bits -= to;
            out.push(((acc >> bits) & mask) as u8);
        }
    }
    if bits > 0 {
        if pad {
            out.push(((acc << (to - bits)) & mask) as u8);
        } else {
            return (out, false);
        }
    }
    (out, true)
}

fn encode_base32(input: &[u8]) -> Result<String, String> {
    input
        .iter()
        .map(|&v| {
            CHARSET
                .get(v as usize)
                .copied()
                .ok_or_else(|| "Invalid byte in input array".to_string())
        })
        .collect()
}

fn decode_base32(input: &str) -> Result<Vec<u8>, String> {
    input
        .chars()
        .map(|c| {
            let i = c as usize;
            if i >= CHARSET_REV.len() || CHARSET_REV[i] < 0 {
                return Err("Invalid base32 input string".to_string());
            }
            Ok(CHARSET_REV[i] as u8)
        })
        .collect()
}

fn mixed_case(s: &str) -> bool {
    let mut seen_lower = false;
    let mut seen_upper = false;
    for c in s.chars() {
        if c.is_ascii_lowercase() {
            seen_lower = true;
        } else if c.is_ascii_uppercase() {
            seen_upper = true;
        }
        if seen_lower && seen_upper {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repack_bits_roundtrip() {
        let (five, ok) = repack_bits(&[0xFF], 8, 5, true);
        assert!(ok, "padded conversion completes cleanly");
        assert_eq!(five, vec![0x1F, 0x1C]);
        let (eight, ok) = repack_bits(&five, 5, 8, false);
        assert!(!ok, "unpadded conversion has trailing bits dropped");
        assert_eq!(eight, vec![0xFF]);
    }

    #[test]
    fn decode_known_vectors() {
        let vectors: &[(&str, NetworkPrefix, AddressType, Vec<u8>)] = &[
            (
                "bitcoincash:pq4ql3ph6738xuv2cycduvkpu4rdwqge5q2uxdfg6f",
                NetworkPrefix::BitcoinCash,
                AddressType::P2SH,
                vec![
                    42, 15, 196, 55, 215, 162, 115, 113, 138, 193, 48, 222, 50, 193, 229, 70, 215, 1, 25, 160,
                ],
            ),
            (
                "qrplwyx7kueqkrh6dmd3fclta6u32hafp5tnpkchx2",
                NetworkPrefix::BitcoinCash,
                AddressType::P2PKH,
                vec![
                    195, 247, 16, 222, 183, 50, 11, 14, 250, 110, 219, 20, 227, 235, 238, 185, 21, 95, 169, 13,
                ],
            ),
            (
                "BitCoinCash:QRPLWYX7KUEQKRH6DMD3FCLTA6U32HAFP5TNPKCHX2",
                NetworkPrefix::BitcoinCash,
                AddressType::P2PKH,
                vec![
                    195, 247, 16, 222, 183, 50, 11, 14, 250, 110, 219, 20, 227, 235, 238, 185, 21, 95, 169, 13,
                ],
            ),
            (
                "bchtest:qqjr7yu573z4faxw8ltgvjwpntwys08fysk07zmvce",
                NetworkPrefix::BchTest,
                AddressType::P2PKH,
                vec![
                    36, 63, 19, 148, 244, 69, 84, 244, 206, 63, 214, 134, 73, 193, 154, 220, 72, 60, 233, 36,
                ],
            ),
        ];
        for (s, p, t, h) in vectors {
            let a = CashAddress::decode(s).unwrap();
            assert_eq!(&a.prefix, p);
            assert_eq!(&a.address_type, t);
            assert_eq!(&a.hash, h);
            assert!(a.encode().unwrap().contains(&s.to_lowercase()));
        }
    }

    #[test]
    fn checksum_validation() {
        // Valid 20-byte hash addresses round-trip cleanly.
        let valid = [
            "bitcoincash:qzxqqt9lh4feptf0mplnk58gnajfepzwcq9f2rxk55",
            "bitcoincash:qr6m7j9njldwwzlg9v7v53unlr4jkmx6eylep8ekg2",
            "bitcoincash:pq4ql3ph6738xuv2cycduvkpu4rdwqge5q2uxdfg6f",
        ];
        for a in valid {
            CashAddress::decode(a).unwrap();
        }
        // Mutating any character invalidates the BCH checksum.
        let invalid = [
            "bitcoincash:qzxqqt9lh4feptf0mplnk58gnajfepzwcq9f2rxk56",
            "bchtest:qqjr7yu573z4faxw8ltgvjwpntwys08fysk07zmvcf",
        ];
        for a in invalid {
            assert!(CashAddress::decode(a).is_err());
        }
    }
}
