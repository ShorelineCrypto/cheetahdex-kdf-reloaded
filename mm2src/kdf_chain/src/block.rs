//! Block container and merkle helpers.

use hex::FromHex;
use primitives::hash::H256;
use serialization::deserialize;

use crate::header::BlockHeader;
use crate::merkle::merkle_root;
use crate::repr::RepresentH256;
use crate::transaction::Transaction;

/// Bitcoin-style block: a header followed by a list of transactions.
///
/// The wire layout (header || varint count || txs) is provided by the
/// `Serializable` derive — the same format used by every UTXO chain KDF
/// supports.
#[derive(Debug, Clone, PartialEq, Serializable, Deserializable)]
pub struct Block {
    pub block_header: BlockHeader,
    pub transactions: Vec<Transaction>,
}

impl Block {
    pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
        Block {
            block_header: header,
            transactions,
        }
    }

    /// Standard merkle root over all transaction txids.
    pub fn merkle_root(&self) -> H256 {
        let txids: Vec<H256> = self.transactions.iter().map(Transaction::hash).collect();
        merkle_root(&txids)
    }

    /// SegWit witness merkle root. The coinbase entry is replaced with a
    /// zero hash per BIP-141; all other entries use `witness_hash`.
    pub fn witness_merkle_root(&self) -> H256 {
        let hashes = match self.transactions.split_first() {
            None => Vec::new(),
            Some((_, rest)) => {
                let mut acc = Vec::with_capacity(self.transactions.len());
                acc.push(H256::from(0));
                acc.extend(rest.iter().map(Transaction::witness_hash));
                acc
            },
        };
        merkle_root(&hashes)
    }

    pub fn transactions(&self) -> &[Transaction] {
        &self.transactions
    }

    pub fn header(&self) -> &BlockHeader {
        &self.block_header
    }

    pub fn hash(&self) -> H256 {
        self.block_header.hash()
    }
}

impl RepresentH256 for Block {
    fn h256(&self) -> H256 {
        self.hash()
    }
}

impl From<&'static str> for Block {
    fn from(s: &'static str) -> Self {
        let bytes: Vec<u8> = s.from_hex().expect("valid hex");
        deserialize(bytes.as_slice()).expect("valid block hex")
    }
}
