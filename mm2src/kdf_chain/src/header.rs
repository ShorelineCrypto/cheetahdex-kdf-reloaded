//! Block header model spanning every coin variant supported by KDF.
//!
//! The base layout is the 80-byte Bitcoin header (version, prev-hash, merkle
//! root, time, bits, nonce). Every other field is an optional extension
//! conditioned on `version` and — for Qtum — on the runtime coin variant
//! threaded through the `Reader`.

use crypto::dhash256;
use hex::FromHex;
use primitives::bytes::Bytes;
use primitives::compact::Compact;
use primitives::hash::H256;
use primitives::U256;
use serialization::{deserialize, serialize, Deserializable, Reader, Serializable, Stream};
use std::io;

use crate::transaction::{deserialize_tx, OutPoint, Transaction, TxType};

// Multi-coin block-version magic numbers. Each value is a published consensus
// constant of the upstream coin's reference client.
const AUX_POW_VERSION_DOGE: u32 = 6_422_788;
const AUX_POW_VERSION_SYS: u32 = 537_919_744;
const MTP_POW_VERSION: u32 = 0x2000_1000;
const PROG_POW_SWITCH_TIME: u32 = 1_635_228_000;
const QTUM_BLOCK_HEADER_VERSION: u32 = 536_870_912;
/// Ravencoin KAWPOW.
const KAWPOW_VERSION: u32 = 805_306_368;
/// Verus uses bit 16 of the version field to flag verus-blocks.
const VERUS_VERSION_BIT: u32 = 0x0001_0000;

/// Wire-level union for the nonce field — Bitcoin uses `u32`, Equihash uses
/// `H256`.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockHeaderNonce {
    U32(u32),
    H256(H256),
}

impl Serializable for BlockHeaderNonce {
    fn serialize(&self, s: &mut Stream) {
        match self {
            BlockHeaderNonce::U32(n) => s.append(n),
            BlockHeaderNonce::H256(h) => s.append(h),
        };
    }
}

/// Wire-level union for the difficulty target — most chains use the compact
/// 32-bit packed form; Equihash chains use a raw `u32` target.
#[derive(Clone, Debug, PartialEq)]
pub enum BlockHeaderBits {
    Compact(Compact),
    U32(u32),
}

impl Serializable for BlockHeaderBits {
    fn serialize(&self, s: &mut Stream) {
        match self {
            BlockHeaderBits::Compact(c) => s.append(c),
            BlockHeaderBits::U32(n) => s.append(n),
        };
    }
}

/// AuxPoW merkle proof branch (used by Doge/Syscoin merged mining).
#[derive(Clone, Debug, PartialEq, Deserializable, Serializable)]
pub struct MerkleBranch {
    branch_hashes: Vec<H256>,
    branch_side_mask: i32,
}

/// AuxPoW container — wraps the parent-chain coinbase plus merkle proofs
/// linking it back into the auxiliary chain header.
/// <https://en.bitcoin.it/wiki/Merged_mining_specification#Merged_mining_coinbase>
#[derive(Clone, Debug, PartialEq)]
pub struct AuxPow {
    coinbase_tx: Transaction,
    parent_block_hash: H256,
    coinbase_branch: MerkleBranch,
    blockchain_branch: MerkleBranch,
    parent_block_header: Box<BlockHeader>,
}

impl Serializable for AuxPow {
    fn serialize(&self, s: &mut Stream) {
        s.append(&self.coinbase_tx);
        s.append(&self.parent_block_hash);
        s.append(&self.coinbase_branch);
        s.append(&self.blockchain_branch);
        s.append(self.parent_block_header.as_ref());
    }
}

/// Firo ProgPoW header tail.
/// <https://github.com/firoorg/firo/blob/904984eecdc15df5de19bd34c70fd989847a2f08/src/primitives/block.h#L197>
#[derive(Clone, Debug, Deserializable, PartialEq, Serializable)]
pub struct ProgPow {
    n_height: u32,
    n_nonce_64: u64,
    mix_hash: H256,
}

/// Firo MTP header tail.
/// <https://github.com/firoorg/firo/blob/75f72d061eb793c39148c6d3f3eb5595159fdca0/src/primitives/block.h#L159>
#[derive(Clone, Debug, Deserializable, PartialEq, Serializable)]
pub struct MtpPow {
    n_version_mtp: i32,
    mtp_hash_value: H256,
    reserved_0: H256,
    reserved_1: H256,
}

/// Universal block header. Only the first six fields are present on every
/// chain; the rest are coin-specific extensions populated by the
/// `Deserializable` impl based on `version` (and, for Qtum, the
/// `Reader::coin_variant()` hint).
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHeader {
    pub version: u32,
    pub previous_header_hash: H256,
    pub merkle_root_hash: H256,
    pub hash_final_sapling_root: Option<H256>,
    pub time: u32,
    pub bits: BlockHeaderBits,
    pub nonce: BlockHeaderNonce,
    pub solution: Option<Vec<u8>>,
    pub aux_pow: Option<AuxPow>,
    pub prog_pow: Option<ProgPow>,
    pub mtp_pow: Option<MtpPow>,
    pub is_verus: bool,
    /// Qtum: <https://github.com/qtumproject/qtum/blob/2792457a6a7b7922bc33ba934c3ed47a3ff66bf9/src/primitives/block.h#L30>
    pub hash_state_root: Option<H256>,
    pub hash_utxo_root: Option<H256>,
    pub prevout_stake: Option<OutPoint>,
    pub vch_block_sig_dlgt: Option<Vec<u8>>,
    /// Ravencoin KAWPOW: <https://github.com/RavenProject/Ravencoin/blob/61c790447a5afe150d9892705ac421d595a2df60/src/primitives/block.h#L49>
    pub n_height: Option<u32>,
    pub n_nonce_u64: Option<u64>,
    pub mix_hash: Option<H256>,
}

impl BlockHeader {
    /// Block hash = DSHA-256 over the canonical serialization.
    pub fn hash(&self) -> H256 { dhash256(&serialize(self)) }

    pub fn is_prog_pow(&self) -> bool { self.version == MTP_POW_VERSION && self.time >= PROG_POW_SWITCH_TIME }

    pub fn raw(&self) -> Bytes { serialize(self) }

    /// Decodes the difficulty target. The compact form may contain a value
    /// that overflows 256 bits; in that case `Err(target)` carries the raw
    /// (truncated) value to let consumers report a meaningful error.
    pub fn target(&self) -> Result<U256, U256> {
        match self.bits {
            BlockHeaderBits::Compact(compact) => compact.to_u256(),
            BlockHeaderBits::U32(nb) => Ok(U256::from(nb)),
        }
    }
}

impl From<&'static str> for BlockHeader {
    fn from(s: &'static str) -> Self {
        let bytes: Vec<u8> = s.from_hex().expect("valid hex");
        deserialize(bytes.as_slice()).expect("valid header hex")
    }
}

impl Serializable for BlockHeader {
    fn serialize(&self, s: &mut Stream) {
        // Verus stores the version with bit 16 toggled on the wire.
        if self.is_verus {
            s.append(&(self.version ^ VERUS_VERSION_BIT));
        } else {
            s.append(&self.version);
        }
        s.append(&self.previous_header_hash);
        s.append(&self.merkle_root_hash);
        if let Some(h) = &self.hash_final_sapling_root {
            s.append(h);
        }
        s.append(&self.time);
        s.append(&self.bits);
        // KAWPOW and ProgPoW emit the nonce in their dedicated trailers.
        if !self.is_prog_pow() && self.version != KAWPOW_VERSION {
            s.append(&self.nonce);
        }
        if let Some(sol) = &self.solution {
            s.append_list(sol);
        }
        if let Some(pow) = &self.aux_pow {
            s.append(pow);
        }
        if let Some(pow) = &self.prog_pow {
            s.append(pow);
        }
        if let Some(pow) = &self.mtp_pow {
            s.append(pow);
        }
        if let Some(root) = &self.hash_state_root {
            s.append(root);
        }
        if let Some(root) = &self.hash_utxo_root {
            s.append(root);
        }
        if let Some(prevout) = &self.prevout_stake {
            s.append(prevout);
        }
        if let Some(vec) = &self.vch_block_sig_dlgt {
            s.append_list(vec);
        }
        if let Some(n_height) = &self.n_height {
            s.append(n_height);
        }
        if let Some(n_nonce_u64) = &self.n_nonce_u64 {
            s.append(n_nonce_u64);
        }
        if let Some(mix_hash) = &self.mix_hash {
            s.append(mix_hash);
        }
    }
}

impl Deserializable for BlockHeader {
    fn deserialize<T: io::Read>(reader: &mut Reader<T>) -> Result<Self, serialization::Error>
    where
        Self: Sized,
    {
        let mut version: u32 = reader.read()?;
        // Verus quirk: real version after toggling bit 16 is exactly 4.
        let is_verus = (version ^ VERUS_VERSION_BIT) == 4;
        if is_verus {
            version ^= VERUS_VERSION_BIT;
        }

        let previous_header_hash = reader.read()?;
        let merkle_root_hash = reader.read()?;

        // Sapling chains (Equihash family) inject the final sapling root for v4.
        let hash_final_sapling_root = if version == 4 { Some(reader.read()?) } else { None };
        let time = reader.read()?;
        let bits = if version == 4 {
            BlockHeaderBits::U32(reader.read()?)
        } else {
            BlockHeaderBits::Compact(reader.read()?)
        };
        let nonce = if version == 4 {
            BlockHeaderNonce::H256(reader.read()?)
        } else if version == KAWPOW_VERSION || (version == MTP_POW_VERSION && time >= PROG_POW_SWITCH_TIME) {
            // KAWPOW and ProgPoW carry the nonce in their dedicated trailer
            // sections instead of the standard 4-byte slot.
            BlockHeaderNonce::U32(0)
        } else {
            BlockHeaderNonce::U32(reader.read()?)
        };
        let solution = if version == 4 { Some(reader.read_list()?) } else { None };

        let aux_pow = if version == AUX_POW_VERSION_DOGE || version == AUX_POW_VERSION_SYS {
            // Parent coinbase is always a standard Bitcoin (witness) tx.
            let coinbase_tx = deserialize_tx(reader, TxType::StandardWithWitness)?;
            let parent_block_hash = reader.read()?;
            let coinbase_branch = reader.read()?;
            let blockchain_branch = reader.read()?;
            let parent_block_header = Box::new(reader.read()?);
            Some(AuxPow {
                coinbase_tx,
                parent_block_hash,
                coinbase_branch,
                blockchain_branch,
                parent_block_header,
            })
        } else {
            None
        };

        let prog_pow = if version == MTP_POW_VERSION && time >= PROG_POW_SWITCH_TIME {
            Some(reader.read()?)
        } else {
            None
        };
        let mtp_pow = if version == MTP_POW_VERSION && time < PROG_POW_SWITCH_TIME && prog_pow.is_none() {
            Some(reader.read()?)
        } else {
            None
        };

        // Qtum's PoS-stake fields share their version magic with several other
        // chains, so they are gated on the runtime coin-variant hint.
        let (hash_state_root, hash_utxo_root, prevout_stake, vch_block_sig_dlgt) =
            if version == QTUM_BLOCK_HEADER_VERSION && reader.coin_variant().is_qtum() {
                (
                    Some(reader.read()?),
                    Some(reader.read()?),
                    Some(reader.read()?),
                    Some(reader.read_list()?),
                )
            } else {
                (None, None, None, None)
            };

        let (n_height, n_nonce_u64, mix_hash) = if version == KAWPOW_VERSION {
            (Some(reader.read()?), Some(reader.read()?), Some(reader.read()?))
        } else {
            (None, None, None)
        };

        Ok(BlockHeader {
            version,
            previous_header_hash,
            merkle_root_hash,
            hash_final_sapling_root,
            time,
            bits,
            nonce,
            solution,
            aux_pow,
            prog_pow,
            mtp_pow,
            is_verus,
            hash_state_root,
            hash_utxo_root,
            prevout_stake,
            vch_block_sig_dlgt,
            n_height,
            n_nonce_u64,
            mix_hash,
        })
    }
}
