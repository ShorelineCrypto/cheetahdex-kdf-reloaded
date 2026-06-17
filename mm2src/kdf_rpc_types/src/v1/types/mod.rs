// SPDX-License-Identifier: GPL-2.0-only
//! Public surface for the v1 RPC JSON wire types.

pub mod address;
mod bytes;
mod hash;
mod script;
mod transaction;

pub use self::bytes::Bytes;
pub use self::hash::{H160, H256, H264};
pub use self::script::ScriptType;
pub use self::transaction::{
    CoinbaseTransactionInput, GetRawTransactionResponse, LelantusInput, RawTransaction, SigmaInput,
    SignedTransactionInput, SignedTransactionOutput, Transaction, TransactionInput, TransactionInputEnum,
    TransactionInputScript, TransactionOutput, TransactionOutputScript, TransactionOutputWithAddress,
    TransactionOutputWithScriptData, TransactionOutputs,
};

/// Canonical lowercase-hex tx-hash representation produced from a raw byte buffer.
pub trait ToTxHash {
    fn to_tx_hash(&self) -> String;
}

impl ToTxHash for Bytes {
    fn to_tx_hash(&self) -> String {
        encode_lowercase_hex(self.as_slice())
    }
}

impl ToTxHash for Vec<u8> {
    fn to_tx_hash(&self) -> String {
        encode_lowercase_hex(self.as_slice())
    }
}

#[inline]
fn encode_lowercase_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(nibble_to_hex(byte >> 4));
        out.push(nibble_to_hex(byte & 0x0f));
    }
    out
}

#[inline]
fn nibble_to_hex(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        10..=15 => (b'a' + (nibble - 10)) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::{encode_lowercase_hex, ToTxHash};

    #[test]
    fn to_tx_hash_vec_lowercase() {
        let bytes: Vec<u8> = vec![0xDE, 0xAD, 0xBE, 0xEF];
        assert_eq!(bytes.to_tx_hash(), "deadbeef");
    }

    #[test]
    fn encode_pads_each_byte() {
        assert_eq!(encode_lowercase_hex(&[0x00, 0x0f, 0xa0]), "000fa0");
    }
}
