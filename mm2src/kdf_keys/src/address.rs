// `Address` — KDF's flexible bitcoin-family address container.
//
// Wire shape (Standard / Legacy):
//   `[t_addr_prefix?][prefix][hash:20][checksum:4]` (25 or 26 bytes)
// Checksum kind is auto-detected from a list of known hash families
// (DSHA256 for Bitcoin/Komodo/Zcash, DGROESTL512 for Groestlcoin,
// KECCAK256 for SmartCash) — see `detect_checksum`.
//
// Two alternative encodings layer on top: BIP-173 SegWit (`bech32`) and
// BCH CashAddr.

use crate::cashaddr::{AddressType as CashAddrType, CashAddress};
use crate::{AddressHashEnum, DisplayLayout, Error, SegwitAddress};
use base58::{FromBase58, ToBase58};
use crypto::{checksum, dgroestl512, dhash256, keccak256, ChecksumType};
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Type {
    P2PKH,
    P2SH,
    P2WPKH,
    P2WSH,
}

#[derive(Clone, Debug, Default, Display, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "format")]
pub enum AddressFormat {
    #[serde(rename = "standard")]
    #[display(fmt = "Legacy")]
    #[default]
    Standard,
    #[serde(rename = "segwit")]
    Segwit,
    #[serde(rename = "cashaddress")]
    #[display(fmt = "CashAddress")]
    #[allow(dead_code)]
    CashAddress {
        network: String,
        #[serde(default)]
        pub_addr_prefix: u8,
        #[serde(default)]
        p2sh_addr_prefix: u8,
    },
}

impl AddressFormat {
    pub fn is_segwit(&self) -> bool {
        matches!(self, AddressFormat::Segwit)
    }
    pub fn is_cashaddress(&self) -> bool {
        matches!(self, AddressFormat::CashAddress { .. })
    }
    pub fn is_legacy(&self) -> bool {
        matches!(self, AddressFormat::Standard)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Address {
    pub prefix: u8,
    pub t_addr_prefix: u8,
    pub hrp: Option<String>,
    pub hash: AddressHashEnum,
    pub checksum_type: ChecksumType,
    pub addr_format: AddressFormat,
}

/// Tries DSHA256, then DGROESTL512, then KECCAK256 — first match wins.
/// SegWit programs carry their own bech32 checksum and don't reach here.
pub fn detect_checksum(data: &[u8], expected: &[u8]) -> Result<ChecksumType, Error> {
    if expected == &dhash256(data)[0..4] {
        return Ok(ChecksumType::DSHA256);
    }
    if expected == &dgroestl512(data)[0..4] {
        return Ok(ChecksumType::DGROESTL512);
    }
    if expected == &keccak256(data)[0..4] {
        return Ok(ChecksumType::KECCAK256);
    }
    Err(Error::InvalidChecksum)
}

pub struct AddressDisplayLayout(Vec<u8>);

impl Deref for AddressDisplayLayout {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DisplayLayout for Address {
    type Target = AddressDisplayLayout;

    fn layout(&self) -> Self::Target {
        let mut buf = Vec::with_capacity(26);
        if self.t_addr_prefix > 0 {
            buf.push(self.t_addr_prefix);
        }
        buf.push(self.prefix);
        buf.extend_from_slice(&self.hash.to_vec());
        buf.extend_from_slice(&*checksum(&buf, &self.checksum_type));
        AddressDisplayLayout(buf)
    }

    fn from_layout(data: &[u8]) -> Result<Self, Error> {
        let (t_addr_prefix, prefix, hash_bytes) = match data.len() {
            25 => (0u8, data[0], &data[1..21]),
            26 => (data[0], data[1], &data[2..22]),
            _ => return Err(Error::InvalidAddress),
        };
        let split = data.len() - 4;
        let checksum_type = detect_checksum(&data[..split], &data[split..])?;
        let mut hash = AddressHashEnum::default_address_hash();
        hash.copy_from_slice(hash_bytes);
        Ok(Address {
            prefix,
            t_addr_prefix,
            hash,
            checksum_type,
            hrp: None,
            addr_format: AddressFormat::Standard,
        })
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.addr_format {
            AddressFormat::Standard => self.layout().to_base58().fmt(f),
            AddressFormat::Segwit => {
                SegwitAddress::new(&self.hash, self.hrp.clone().expect("Segwit address requires hrp"))
                    .to_string()
                    .fmt(f)
            },
            AddressFormat::CashAddress {
                network,
                pub_addr_prefix,
                p2sh_addr_prefix,
            } => self
                .to_cashaddress(network, *pub_addr_prefix, *p2sh_addr_prefix)
                .expect("valid cashaddress")
                .encode()
                .expect("valid encoded cashaddress")
                .fmt(f),
        }
    }
}

impl FromStr for Address {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Error> {
        let raw = s.from_base58().map_err(|_| Error::InvalidAddress)?;
        Address::from_layout(&raw)
    }
}

impl From<&'static str> for Address {
    fn from(s: &'static str) -> Self {
        s.parse().expect("valid address literal")
    }
}

impl Address {
    pub fn display_address(&self) -> Result<String, String> {
        match &self.addr_format {
            AddressFormat::Standard => Ok(self.to_string()),
            AddressFormat::Segwit => self
                .hrp
                .as_ref()
                .map(|hrp| SegwitAddress::new(&self.hash, hrp.clone()).to_string())
                .ok_or_else(|| "Cannot display segwit address for a coin with no bech32_hrp in config".into()),
            AddressFormat::CashAddress {
                network,
                pub_addr_prefix,
                p2sh_addr_prefix,
            } => self
                .to_cashaddress(network, *pub_addr_prefix, *p2sh_addr_prefix)
                .and_then(|c| c.encode()),
        }
    }

    pub fn from_cashaddress(
        cashaddr: &str,
        checksum_type: ChecksumType,
        p2pkh_prefix: u8,
        p2sh_prefix: u8,
        t_addr_prefix: u8,
    ) -> Result<Address, String> {
        let decoded = CashAddress::decode(cashaddr)?;
        if decoded.hash.len() != 20 {
            return Err("Expect 20 bytes long hash".into());
        }
        let mut hash = AddressHashEnum::default_address_hash();
        hash.copy_from_slice(&decoded.hash);
        let prefix = match decoded.address_type {
            CashAddrType::P2PKH => p2pkh_prefix,
            CashAddrType::P2SH => p2sh_prefix,
        };
        Ok(Address {
            prefix,
            t_addr_prefix,
            hash,
            checksum_type,
            hrp: None,
            addr_format: AddressFormat::CashAddress {
                network: decoded.prefix.to_string(),
                pub_addr_prefix: p2pkh_prefix,
                p2sh_addr_prefix: p2sh_prefix,
            },
        })
    }

    pub fn to_cashaddress(
        &self,
        network_prefix: &str,
        p2pkh_prefix: u8,
        p2sh_prefix: u8,
    ) -> Result<CashAddress, String> {
        let address_type = if self.prefix == p2pkh_prefix {
            CashAddrType::P2PKH
        } else if self.prefix == p2sh_prefix {
            CashAddrType::P2SH
        } else {
            return Err(format!(
                "Unknown address prefix {}. Expect: {}, {}",
                self.prefix, p2pkh_prefix, p2sh_prefix
            ));
        };
        CashAddress::new(network_prefix, self.hash.to_vec(), address_type)
    }

    pub fn from_segwitaddress(
        segaddr: &str,
        checksum_type: ChecksumType,
        prefix: u8,
        t_addr_prefix: u8,
    ) -> Result<Address, String> {
        let s = SegwitAddress::from_str(segaddr).map_err(|e| e.to_string())?;
        let mut hash = match s.program.len() {
            20 => AddressHashEnum::default_address_hash(),
            32 => AddressHashEnum::default_witness_script_hash(),
            _ => return Err("Expect either 20 or 32 bytes long hash".into()),
        };
        hash.copy_from_slice(&s.program);
        Ok(Address {
            prefix,
            t_addr_prefix,
            hash,
            checksum_type,
            hrp: Some(s.hrp),
            addr_format: AddressFormat::Segwit,
        })
    }

    pub fn to_segwitaddress(&self) -> Result<SegwitAddress, String> {
        match &self.hrp {
            Some(hrp) => Ok(SegwitAddress::new(&self.hash, hrp.clone())),
            None => Err("hrp must be provided for segwit address".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashaddr::NetworkPrefix;

    fn legacy(prefix: u8, t: u8, hash_hex: &'static str, ct: ChecksumType) -> Address {
        Address {
            prefix,
            t_addr_prefix: t,
            hash: AddressHashEnum::AddressHash(hash_hex.into()),
            checksum_type: ct,
            hrp: None,
            addr_format: AddressFormat::Standard,
        }
    }

    #[test]
    fn btc_legacy_display() {
        let a = legacy(0, 0, "3f4aa1fedf1f54eeb03b759deadb36676b184911", ChecksumType::DSHA256);
        assert_eq!(a.to_string(), "16meyfSoQV6twkAAxPe51RtMVz7PGRmWna");
        assert_eq!(a, "16meyfSoQV6twkAAxPe51RtMVz7PGRmWna".into());
    }

    #[test]
    fn kmd_legacy_display() {
        let a = legacy(60, 0, "05aab5342166f8594baf17a7d9bef5d567443327", ChecksumType::DSHA256);
        assert_eq!(a.to_string(), "R9o9xTocqr6CeEDGDH6mEYpwLoMz6jNjMW");
        assert_eq!(a, "R9o9xTocqr6CeEDGDH6mEYpwLoMz6jNjMW".into());
    }

    #[test]
    fn zec_t_address_display() {
        let a = legacy(
            37,
            29,
            "05aab5342166f8594baf17a7d9bef5d567443327",
            ChecksumType::DSHA256,
        );
        assert_eq!(a.to_string(), "tmAEKD7psc1ajK76QMGEW8WGQSBBHf9SqCp");
        assert_eq!(a, "tmAEKD7psc1ajK76QMGEW8WGQSBBHf9SqCp".into());
    }

    #[test]
    fn kmd_p2sh_display() {
        let a = legacy(85, 0, "ca0c3786c96ff7dacd40fdb0f7c196528df35f85", ChecksumType::DSHA256);
        assert_eq!(a.to_string(), "bX9bppqdGvmCCAujd76Tq76zs1suuPnB9A");
    }

    #[test]
    fn grs_dgroestl512_roundtrip() {
        let a = legacy(
            36,
            0,
            "c3f710deb7320b0efa6edb14e3ebeeb9155fa90d",
            ChecksumType::DGROESTL512,
        );
        assert_eq!(a, "Fo2tBkpzaWQgtjFUkemsYnKyfvd2i8yTki".into());
        assert_eq!(a.to_string(), "Fo2tBkpzaWQgtjFUkemsYnKyfvd2i8yTki");
    }

    #[test]
    fn smartcash_keccak256_roundtrip() {
        let a = legacy(
            63,
            0,
            "56bb05aa20f5a80cf84e90e5dab05be331333e27",
            ChecksumType::KECCAK256,
        );
        assert_eq!(a, "SVCbBs6FvPYxJrYoJc4TdCe47QNCgmTabv".into());
        assert_eq!(a.to_string(), "SVCbBs6FvPYxJrYoJc4TdCe47QNCgmTabv");
    }

    #[test]
    fn cashaddress_roundtrip() {
        let cashaddrs = [
            "bitcoincash:qzxqqt9lh4feptf0mplnk58gnajfepzwcq9f2rxk55",
            "bitcoincash:qr6m7j9njldwwzlg9v7v53unlr4jkmx6eylep8ekg2",
            "bitcoincash:pq4ql3ph6738xuv2cycduvkpu4rdwqge5q2uxdfg6f",
        ];
        let legacy = [
            "1DmFp16U73RrVZtYUbo2Ectt8mAnYScpqM",
            "1PQPheJQSauxRPTxzNMUco1XmoCyPoEJCp",
            "35XRC5HRZjih1sML23UXv1Ry1SzTDKSmfQ",
        ];
        for i in 0..3 {
            let actual = Address::from_cashaddress(cashaddrs[i], ChecksumType::DSHA256, 0, 5, 0).unwrap();
            let expected: Address = legacy[i].into();
            assert_eq!(actual.hash, expected.hash);
            let encoded = actual.to_cashaddress("bitcoincash", 0, 5).unwrap().encode().unwrap();
            assert_eq!(encoded, cashaddrs[i]);
        }
    }

    #[test]
    fn cashaddress_hash_size_rejected() {
        assert_eq!(
            Address::from_cashaddress(
                "bitcoincash:qgagf7w02x4wnz3mkwnchut2vxphjzccwxgjvvjmlsxqwkcw59jxxuz",
                ChecksumType::DSHA256,
                0,
                5,
                0,
            ),
            Err("Expect 20 bytes long hash".into())
        );
    }

    #[test]
    fn to_cashaddress_unknown_prefix() {
        let a = Address {
            prefix: 2,
            t_addr_prefix: 0,
            hash: AddressHashEnum::AddressHash(
                [
                    140, 0, 44, 191, 189, 83, 144, 173, 47, 216, 127, 59, 80, 232, 159, 100, 156, 132, 78, 192,
                ]
                .into(),
            ),
            checksum_type: ChecksumType::DSHA256,
            hrp: None,
            addr_format: AddressFormat::CashAddress {
                network: "bitcoincash".into(),
                pub_addr_prefix: 0,
                p2sh_addr_prefix: 5,
            },
        };
        assert_eq!(
            a.to_cashaddress("bitcoincash", 0, 5),
            Err("Unknown address prefix 2. Expect: 0, 5".into())
        );
    }

    #[test]
    fn to_cashaddress_other_prefix_passthrough() {
        let expected = CashAddress {
            prefix: NetworkPrefix::Other("prefix".into()),
            hash: vec![
                140, 0, 44, 191, 189, 83, 144, 173, 47, 216, 127, 59, 80, 232, 159, 100, 156, 132, 78, 192,
            ],
            address_type: CashAddrType::P2PKH,
        };
        let a: Address = "1DmFp16U73RrVZtYUbo2Ectt8mAnYScpqM".into();
        assert_eq!(a.to_cashaddress("prefix", 0, 5).unwrap(), expected);
    }
}
