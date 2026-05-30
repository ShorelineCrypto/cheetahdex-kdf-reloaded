// Transaction signature-hash computation.
//
// Implements three sighash modes:
//   * "original"  — pre-segwit Bitcoin (single-SHA256 over a
//     trimmed-down transaction; output is then DSHA256-folded
//     into the digest passed to ECDSA).
//   * "witness v0" — BIP-143 segwit / Bitcoin Cash (with optional
//     fork-id flag for replay protection).
//   * "overwintered" — ZIP-243 Zcash Sapling (BLAKE2b personalised
//     digest tree).
//
// Plus the legacy KDF "stake-input" extension (PoS chains with
// `nTime`) and KMD "Verus" version XOR — these don't change the
// sighash tree itself; they only change how the underlying transaction
// serializes, which is handled inside `kdf_chain`.

use crate::bytes::Bytes;
use crate::hash::{H256, H512};
use crate::{Builder, Script};
use blake2b_simd::Params as Blake2b;
use chain::{
    JoinSplit, OutPoint, ShieldedOutput, ShieldedSpend, Transaction, TransactionInput, TransactionOutput, TxHashAlgo,
};
use crypto::{dhash256, sha256};
use keys::KeyPair;
use serde::Deserialize;
use serialization::Stream;

// --- ZIP-243 personalisation tags (16 bytes each, last 4 bytes
// optionally carry the consensus branch id at runtime) -------------

const PERS_PREVOUTS: &[u8] = b"ZcashPrevoutHash";
const PERS_SEQUENCE: &[u8] = b"ZcashSequencHash";
const PERS_OUTPUTS: &[u8] = b"ZcashOutputsHash";
const PERS_JOINSPLITS: &[u8] = b"ZcashJSplitsHash";
const PERS_SAPLING_SPENDS: &[u8] = b"ZcashSSpendsHash";
const PERS_SAPLING_OUTPUTS: &[u8] = b"ZcashSOutputHash";
const PERS_SIGHASH: &[u8] = b"ZcashSigHash";

// --- Sighash flag bits (Bitcoin script reference) -----------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum SignatureVersion {
    #[serde(rename = "base")]
    Base,
    #[serde(rename = "witness_v0")]
    WitnessV0,
    #[serde(rename = "fork_id")]
    ForkId,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SighashBase {
    All = 1,
    None = 2,
    Single = 3,
}

impl From<SighashBase> for u32 {
    fn from(b: SighashBase) -> u32 {
        b as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sighash {
    pub base: SighashBase,
    pub anyone_can_pay: bool,
    pub fork_id: bool,
}

impl From<Sighash> for u32 {
    fn from(s: Sighash) -> u32 {
        let mut v = s.base as u32;
        if s.anyone_can_pay {
            v |= 0x80;
        }
        if s.fork_id {
            v |= 0x40;
        }
        v
    }
}

impl Sighash {
    pub fn new(base: SighashBase, anyone_can_pay: bool, fork_id: bool) -> Self {
        Sighash {
            base,
            anyone_can_pay,
            fork_id,
        }
    }

    /// `SCRIPT_VERIFY_STRICTENC` predicate: returns true iff the raw
    /// 32-bit value matches an exact (base | anyone_can_pay | fork_id)
    /// combination valid under the given signature version.
    pub fn is_defined(version: SignatureVersion, raw: u32) -> bool {
        let mask = match version {
            SignatureVersion::ForkId => !(0x40 | 0x80),
            _ => !0x80,
        };
        matches!(raw & mask, 1..=3)
    }

    /// Decode flag bits permissively (accepts non-canonical inputs;
    /// callers wanting strict checking should call `is_defined` first).
    pub fn from_u32(version: SignatureVersion, raw: u32) -> Self {
        let base = match raw & 0x1f {
            2 => SighashBase::None,
            3 => SighashBase::Single,
            _ => SighashBase::All,
        };
        let anyone_can_pay = (raw & 0x80) != 0;
        let fork_id = matches!(version, SignatureVersion::ForkId) && (raw & 0x40) != 0;
        Sighash {
            base,
            anyone_can_pay,
            fork_id,
        }
    }
}

// --- Signer-side input + transaction shape ------------------------

#[derive(Debug, Clone)]
pub struct UnsignedTransactionInput {
    pub previous_output: OutPoint,
    pub sequence: u32,
    pub amount: u64,
    pub witness: Vec<Vec<u8>>,
}

impl From<TransactionInput> for UnsignedTransactionInput {
    fn from(i: TransactionInput) -> Self {
        UnsignedTransactionInput {
            previous_output: i.previous_output,
            sequence: i.sequence,
            amount: 0,
            witness: i.script_witness.into_iter().map(Vec::from).collect(),
        }
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy)]
pub enum SignerHashAlgo {
    SHA256,
    DSHA256,
}

impl From<TxHashAlgo> for SignerHashAlgo {
    fn from(a: TxHashAlgo) -> Self {
        match a {
            TxHashAlgo::DSHA256 => SignerHashAlgo::DSHA256,
            TxHashAlgo::SHA256 => SignerHashAlgo::SHA256,
        }
    }
}
impl From<SignerHashAlgo> for TxHashAlgo {
    fn from(a: SignerHashAlgo) -> Self {
        match a {
            SignerHashAlgo::DSHA256 => TxHashAlgo::DSHA256,
            SignerHashAlgo::SHA256 => TxHashAlgo::SHA256,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TransactionInputSigner {
    pub version: i32,
    pub n_time: Option<u32>,
    pub overwintered: bool,
    pub version_group_id: u32,
    pub consensus_branch_id: u32,
    pub expiry_height: u32,
    pub value_balance: i64,
    pub inputs: Vec<UnsignedTransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub lock_time: u32,
    pub join_splits: Vec<JoinSplit>,
    pub shielded_spends: Vec<ShieldedSpend>,
    pub shielded_outputs: Vec<ShieldedOutput>,
    pub zcash: bool,
    pub str_d_zeel: Option<String>,
    pub hash_algo: SignerHashAlgo,
}

impl From<Transaction> for TransactionInputSigner {
    fn from(t: Transaction) -> Self {
        TransactionInputSigner {
            version: t.version,
            n_time: t.n_time,
            overwintered: t.overwintered,
            version_group_id: t.version_group_id,
            consensus_branch_id: 0,
            expiry_height: t.expiry_height,
            value_balance: t.value_balance,
            inputs: t.inputs.into_iter().map(Into::into).collect(),
            outputs: t.outputs,
            lock_time: t.lock_time,
            join_splits: t.join_splits,
            shielded_spends: t.shielded_spends,
            shielded_outputs: t.shielded_outputs,
            zcash: t.zcash,
            str_d_zeel: t.str_d_zeel,
            hash_algo: t.tx_hash_algo.into(),
        }
    }
}

impl From<TransactionInputSigner> for Transaction {
    fn from(s: TransactionInputSigner) -> Self {
        let inputs = s
            .inputs
            .iter()
            .map(|i| TransactionInput {
                previous_output: i.previous_output,
                script_sig: Bytes::default(),
                sequence: i.sequence,
                script_witness: vec![],
            })
            .collect();
        Transaction {
            version: s.version,
            n_time: s.n_time,
            overwintered: s.overwintered,
            version_group_id: s.version_group_id,
            expiry_height: s.expiry_height,
            value_balance: s.value_balance,
            inputs,
            outputs: s.outputs,
            lock_time: s.lock_time,
            join_splits: s.join_splits,
            shielded_spends: s.shielded_spends,
            shielded_outputs: s.shielded_outputs,
            zcash: s.zcash,
            binding_sig: H512::default(),
            join_split_pubkey: H256::default(),
            join_split_sig: H512::default(),
            str_d_zeel: s.str_d_zeel,
            tx_hash_algo: s.hash_algo.into(),
        }
    }
}

impl TransactionInputSigner {
    /// Compute the digest that ECDSA will sign for one input.
    pub fn signature_hash(
        &self,
        input_index: usize,
        input_amount: u64,
        script_pubkey: &Script,
        sigversion: SignatureVersion,
        sighashtype: u32,
    ) -> H256 {
        let sighash = Sighash::from_u32(sigversion, sighashtype);
        match sigversion {
            SignatureVersion::ForkId if sighash.fork_id => {
                self.sighash_fork_id(input_index, input_amount, script_pubkey, sighashtype, sighash)
            },
            SignatureVersion::Base | SignatureVersion::ForkId => {
                self.sighash_legacy(input_index, script_pubkey, sighashtype, sighash)
            },
            SignatureVersion::WitnessV0 => {
                self.sighash_witness_v0(input_index, input_amount, script_pubkey, sighashtype, sighash)
            },
        }
    }

    /// Sign one input; returns the signed `TransactionInput` ready to
    /// drop into the corresponding slot of the final transaction.
    pub fn signed_input(
        &self,
        keypair: &KeyPair,
        input_index: usize,
        input_amount: u64,
        script_pubkey: &Script,
        sigversion: SignatureVersion,
        sighash: u32,
    ) -> TransactionInput {
        let digest = self.signature_hash(input_index, input_amount, script_pubkey, sigversion, sighash);
        let mut sig: Vec<u8> = keypair.private().sign(&digest).expect("ECDSA signing").into();
        sig.push(sighash as u8);
        let script_sig = Builder::default().push_data(&sig).into_script();
        let unsigned = &self.inputs[input_index];
        TransactionInput {
            previous_output: unsigned.previous_output,
            sequence: unsigned.sequence,
            script_sig: script_sig.to_bytes(),
            script_witness: vec![],
        }
    }

    // --- Legacy (pre-segwit) sighash -----------------------------

    pub fn signature_hash_original(
        &self,
        input_index: usize,
        script_pubkey: &Script,
        sighashtype: u32,
        sighash: Sighash,
    ) -> H256 {
        self.sighash_legacy(input_index, script_pubkey, sighashtype, sighash)
    }

    fn sighash_legacy(&self, input_index: usize, script_pubkey: &Script, sighashtype: u32, sighash: Sighash) -> H256 {
        // Out-of-range input index → SIGHASH_ONE_BUG (digest is 0x01...01 little-endian) per Bitcoin Core.
        if input_index >= self.inputs.len() {
            return 1u8.into();
        }
        if sighash.base == SighashBase::Single && input_index >= self.outputs.len() {
            return 1u8.into();
        }
        // Zcash sapling diverts to a completely different algorithm.
        if self.version >= 3 && self.overwintered {
            return self
                .sighash_overwintered(input_index, script_pubkey, sighashtype, sighash)
                .expect("overwintered sighash");
        }

        let script_pubkey = script_pubkey.without_separators();

        // Build the trimmed transaction view that gets serialised + hashed.
        let inputs: Vec<TransactionInput> = if sighash.anyone_can_pay {
            // Only the input we are signing remains — its scriptSig is replaced with the prevout's scriptPubKey.
            let i = &self.inputs[input_index];
            vec![TransactionInput {
                previous_output: i.previous_output,
                script_sig: script_pubkey.to_bytes(),
                sequence: i.sequence,
                script_witness: vec![],
            }]
        } else {
            self.inputs
                .iter()
                .enumerate()
                .map(|(n, i)| TransactionInput {
                    previous_output: i.previous_output,
                    script_sig: if n == input_index {
                        script_pubkey.to_bytes()
                    } else {
                        Bytes::default()
                    },
                    sequence: match sighash.base {
                        SighashBase::Single | SighashBase::None if n != input_index => 0,
                        _ => i.sequence,
                    },
                    script_witness: vec![],
                })
                .collect()
        };

        let outputs: Vec<TransactionOutput> = match sighash.base {
            SighashBase::All => self.outputs.clone(),
            SighashBase::None => Vec::new(),
            SighashBase::Single => self
                .outputs
                .iter()
                .take(input_index + 1)
                .enumerate()
                .map(|(n, o)| {
                    if n == input_index {
                        o.clone()
                    } else {
                        TransactionOutput::default()
                    }
                })
                .collect(),
        };

        let tx = Transaction {
            version: self.version,
            n_time: self.n_time,
            inputs,
            outputs,
            lock_time: self.lock_time,
            zcash: self.zcash,
            str_d_zeel: self.str_d_zeel.clone(),
            tx_hash_algo: self.hash_algo.into(),
            // Sapling fields are zeroed for the legacy preimage.
            overwintered: false,
            version_group_id: 0,
            expiry_height: 0,
            value_balance: 0,
            join_splits: vec![],
            shielded_spends: vec![],
            shielded_outputs: vec![],
            binding_sig: H512::default(),
            join_split_pubkey: H256::default(),
            join_split_sig: H512::default(),
        };

        let mut s = Stream::default();
        s.append(&tx).append(&sighashtype);
        match self.hash_algo {
            SignerHashAlgo::DSHA256 => dhash256(&s.out()),
            SignerHashAlgo::SHA256 => sha256(&s.out()),
        }
    }

    // --- Segwit / BIP-143 sighash ---------------------------------

    fn sighash_witness_v0(
        &self,
        input_index: usize,
        input_amount: u64,
        script_pubkey: &Script,
        sighashtype: u32,
        sighash: Sighash,
    ) -> H256 {
        let prev = hash_prevouts(sighash, &self.inputs);
        let seq = hash_sequence(sighash, &self.inputs);
        let outs = hash_outputs(sighash, input_index, &self.outputs);
        let i = &self.inputs[input_index];

        let mut s = Stream::default();
        s.append(&self.version)
            .append(&prev)
            .append(&seq)
            .append(&i.previous_output)
            .append_list(script_pubkey)
            .append(&input_amount)
            .append(&i.sequence)
            .append(&outs)
            .append(&self.lock_time)
            .append(&sighashtype); // includes the high 24 bits as fork-id (zero for BCH 0-byte forkid).
        dhash256(&s.out())
    }

    // --- BCH / fork-id sighash (BIP-143 with the fork-id bit set) -

    fn sighash_fork_id(
        &self,
        input_index: usize,
        input_amount: u64,
        script_pubkey: &Script,
        sighashtype: u32,
        sighash: Sighash,
    ) -> H256 {
        if input_index >= self.inputs.len() {
            return 1u8.into();
        }
        if sighash.base == SighashBase::Single && input_index >= self.outputs.len() {
            return 1u8.into();
        }
        self.sighash_witness_v0(input_index, input_amount, script_pubkey, sighashtype, sighash)
    }

    // --- ZIP-243 Zcash Sapling sighash ----------------------------

    /// ZIP-243 (Sapling) — implemented for SIGHASH_ALL only, which is
    /// the only mode KDF ever uses.
    pub fn signature_hash_overwintered(
        &self,
        input_index: usize,
        script_pubkey: &Script,
        sighashtype: u32,
        sighash: Sighash,
    ) -> Result<H256, String> {
        self.sighash_overwintered(input_index, script_pubkey, sighashtype, sighash)
    }

    fn sighash_overwintered(
        &self,
        input_index: usize,
        script_pubkey: &Script,
        sighashtype: u32,
        _sighash: Sighash,
    ) -> Result<H256, String> {
        let mut s = Stream::new();

        // Build personalisation: "ZcashSigHash" + LE consensus_branch_id.
        let mut personal = PERS_SIGHASH.to_vec();
        if self.version >= 3 {
            personal.extend_from_slice(&self.consensus_branch_id.to_le_bytes());
        }

        // Block 1: header + version_group_id.
        let header = self.version | if self.overwintered { 1 << 31 } else { 0 };
        s.append(&header).append(&self.version_group_id);

        // Block 2: hashed prevouts.
        let mut prev_s = Stream::new();
        for i in &self.inputs {
            prev_s.append(&i.previous_output);
        }
        s.append(&blake2b_personal(&prev_s.out(), PERS_PREVOUTS));

        // Block 3: hashed sequences.
        let mut seq_s = Stream::new();
        for i in &self.inputs {
            seq_s.append(&i.sequence);
        }
        s.append(&blake2b_personal(&seq_s.out(), PERS_SEQUENCE));

        // Block 4: hashed outputs.
        let mut out_s = Stream::new();
        for o in &self.outputs {
            out_s.append(o);
        }
        s.append(&blake2b_personal(&out_s.out(), PERS_OUTPUTS));

        // Block 5: joinSplits (empty subtree → 32 zero bytes).
        if self.join_splits.is_empty() {
            s.append(&H256::default());
        } else {
            let mut js = Stream::new();
            for j in &self.join_splits {
                js.append(j);
            }
            s.append(&blake2b_personal(&js.out(), PERS_JOINSPLITS));
        }

        // Block 6: sapling spend descriptions (cv | anchor | nullifier | rk | zkproof, no spendAuthSig).
        if self.shielded_spends.is_empty() {
            s.append(&H256::default());
        } else {
            let mut sp = Stream::new();
            for spend in &self.shielded_spends {
                sp.append(&spend.cv)
                    .append(&spend.anchor)
                    .append(&spend.nullifier)
                    .append(&spend.rk)
                    .append(&spend.zkproof);
            }
            s.append(&blake2b_personal(&sp.out(), PERS_SAPLING_SPENDS));
        }

        // Block 7: sapling output descriptions.
        if self.shielded_outputs.is_empty() {
            s.append(&H256::default());
        } else {
            let mut so = Stream::new();
            for o in &self.shielded_outputs {
                so.append(o);
            }
            s.append(&blake2b_personal(&so.out(), PERS_SAPLING_OUTPUTS));
        }

        // Block 8: locktime, expiry, value_balance, sighashtype.
        s.append(&self.lock_time)
            .append(&self.expiry_height)
            .append(&self.value_balance)
            .append(&sighashtype);

        // Block 9: signed-input descriptor — prevout, scriptcode, value, sequence.
        let i = &self.inputs[input_index];
        s.append(&i.previous_output)
            .append(&script_pubkey.to_bytes())
            .append(&i.amount)
            .append(&i.sequence);

        Ok(blake2b_personal(&s.out(), &personal))
    }
}

// --- BIP-143 sub-hashes -------------------------------------------

fn hash_prevouts(sh: Sighash, inputs: &[UnsignedTransactionInput]) -> H256 {
    if sh.anyone_can_pay {
        return 0u8.into();
    }
    let mut s = Stream::default();
    for i in inputs {
        s.append(&i.previous_output);
    }
    dhash256(&s.out())
}

fn hash_sequence(sh: Sighash, inputs: &[UnsignedTransactionInput]) -> H256 {
    if sh.anyone_can_pay || sh.base != SighashBase::All {
        return 0u8.into();
    }
    let mut s = Stream::default();
    for i in inputs {
        s.append(&i.sequence);
    }
    dhash256(&s.out())
}

fn hash_outputs(sh: Sighash, input_index: usize, outputs: &[TransactionOutput]) -> H256 {
    match sh.base {
        SighashBase::All => {
            let mut s = Stream::default();
            for o in outputs {
                s.append(o);
            }
            dhash256(&s.out())
        },
        SighashBase::Single if input_index < outputs.len() => {
            let mut s = Stream::default();
            s.append(&outputs[input_index]);
            dhash256(&s.out())
        },
        _ => 0u8.into(),
    }
}

fn blake2b_personal(input: &[u8], personal: &[u8]) -> H256 {
    let bytes = Blake2b::new()
        .hash_length(32)
        .personal(personal)
        .to_state()
        .update(input)
        .finalize();
    H256::from(bytes.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{
        blake2b_personal, Sighash, SighashBase, SignatureVersion, SignerHashAlgo, TransactionInputSigner,
        UnsignedTransactionInput,
    };
    use crate::bytes::Bytes;
    use crate::hash::{H160, H256};
    use crate::script::Script;
    use chain::{OutPoint, Transaction, TransactionOutput};
    use keys::{Address, AddressHashEnum, Private};

    // Reference vectors lifted from public Bitcoin docs:
    //   * http://www.righto.com/2014/02/bitcoins-hard-way-using-raw-bitcoin.html
    //   * https://github.com/zcash/zips/blob/master/zip-0243.rst (Sapling test vectors)

    #[test]
    fn legacy_sighash_matches_bitcoin_reference() {
        let _priv: Private = "5HusYj2b2x4nroApgfvaSfKYZhRbKFH41bVyPooymbC6KfgSXdD".into();
        let prev_tx = H256::from_reversed_str("81b4c832d70cb56ff957589752eb4125a4cab78a25a8fc52d6a09e5bd4404d48");
        let to: Address = "1KKKK6N21XKo48zWKuQKXdvSsCf95ibHFa".into();
        let prev_script_pubkey: Script = "76a914df3bd30160e6c6145baaf2c88a8844c13a00d1d588ac".into();
        let cur_output: Bytes = "76a914c8e90996c7c6080ee06284600c684ed904d14c5c88ac".into();
        let value = 91_234u64;
        let want: H256 = "5fda68729a6312e17e641e9a49fac2a4a6a680126610af573caab270d232f850".into();

        let mut to_hash = H160::default();
        if let AddressHashEnum::AddressHash(h) = to.hash {
            to_hash = h;
        }
        assert_eq!(&cur_output[3..23], &*to_hash);

        let signer = TransactionInputSigner {
            version: 1,
            n_time: None,
            overwintered: false,
            version_group_id: 0,
            consensus_branch_id: 0,
            expiry_height: 0,
            value_balance: 0,
            lock_time: 0,
            inputs: vec![UnsignedTransactionInput {
                sequence: 0xffff_ffff,
                previous_output: OutPoint {
                    index: 0,
                    hash: prev_tx,
                },
                amount: 0,
                witness: vec![Vec::new()],
            }],
            outputs: vec![TransactionOutput {
                value,
                script_pubkey: cur_output,
            }],
            join_splits: vec![],
            shielded_spends: vec![],
            shielded_outputs: vec![],
            zcash: false,
            str_d_zeel: None,
            hash_algo: SignerHashAlgo::DSHA256,
        };
        let got = signer.signature_hash(
            0,
            0,
            &prev_script_pubkey,
            SignatureVersion::Base,
            SighashBase::All.into(),
        );
        assert_eq!(got, want);
    }

    #[test]
    fn sighash_flag_validation() {
        for (v, raw, ok) in [
            (SignatureVersion::Base, 0xFFFF_FF82, false),
            (SignatureVersion::Base, 0x0000_0182, false),
            (SignatureVersion::Base, 0x0000_0080, false),
            (SignatureVersion::Base, 0x0000_0001, true),
            (SignatureVersion::Base, 0x0000_0082, true),
            (SignatureVersion::Base, 0x0000_0003, true),
            (SignatureVersion::ForkId, 0xFFFF_FFC2, false),
            (SignatureVersion::ForkId, 0x0000_01C2, false),
            (SignatureVersion::ForkId, 0x0000_0081, true),
            (SignatureVersion::ForkId, 0x0000_00C2, true),
            (SignatureVersion::ForkId, 0x0000_0043, true),
        ] {
            assert_eq!(Sighash::is_defined(v, raw), ok, "v={:?} raw=0x{:08x}", v, raw);
        }
    }

    #[test]
    fn blake2b_personal_zip243_anchor() {
        let h = blake2b_personal(b"", b"ZcashPrevoutHash");
        assert_eq!(
            H256::from("d53a633bbecf82fe9e9484d8a0e727c73bb9e68c96e72dec30144f6a84afa136"),
            h
        );
    }

    #[test]
    fn sapling_sighash_zip243_vector_1() {
        let tx: Transaction = "0400008085202f8901a8c685478265f4c14dada651969c45a65e1aeb8cd6791f2f5bb6a1d9952104d9010000006b483045022100a61e5d557568c2ddc1d9b03a7173c6ce7c996c4daecab007ac8f34bee01e6b9702204d38fdc0bcf2728a69fde78462a10fb45a9baa27873e6a5fc45fb5c76764202a01210365ffea3efa3908918a8b8627724af852fc9b86d7375b103ab0543cf418bcaa7ffeffffff02005a6202000000001976a9148132712c3ff19f3a151234616777420a6d7ef22688ac8b959800000000001976a9145453e4698f02a38abdaa521cd1ff2dee6fac187188ac29b0040048b004000000000000000000000000".into();
        let mut signer = TransactionInputSigner::from(tx);
        signer.inputs[0].amount = 50_000_000;
        signer.consensus_branch_id = 0x76b8_09bb;

        let sh = Sighash::from_u32(SignatureVersion::Base, 1);
        let got = signer
            .signature_hash_overwintered(
                0,
                &Script::from("1976a914507173527b4c3318a2aecd793bf1cfed705950cf88ac"),
                1,
                sh,
            )
            .unwrap();
        assert_eq!(
            H256::from("f27411aa9bd02879181c763a80bdb6f9ea9158f0de71757e7e12ed17760ebe3f"),
            got
        );
    }

    #[test]
    fn sapling_sighash_dispatch_via_signature_hash() {
        let tx: Transaction = "0400008085202f89012c07a03638d9cf4d2cc837784b3b06aa9a5c8b819f7cb0d373bf711108f4c0f2010000006b483045022100fceec7ffa2686377fa2e13d43aa1d8836c3b5ace5292dd2f65a75befec2660bd02205dc000c13a89975bf3fe85aa9c891fcdea6eb25bd5459ad204fe2946d22e49c3012102031d4256c4bc9f99ac88bf3dba21773132281f65f9bf23a59928bce08961e2f3ffffffff0240420f00000000001976a91405aab5342166f8594baf17a7d9bef5d56744332788ac7c288800000000001976a91405aab5342166f8594baf17a7d9bef5d56744332788ac00000000000000000000000000000000000000".into();
        let mut signer = TransactionInputSigner::from(tx);
        signer.inputs[0].amount = 9_924_260;
        signer.consensus_branch_id = 0x76b8_09bb;
        let got = signer.signature_hash(
            0,
            0,
            &Script::from("76a91405aab5342166f8594baf17a7d9bef5d56744332788ac"),
            SignatureVersion::Base,
            1,
        );
        assert_eq!(
            H256::from("047da0d9932545770fc570122c4451b53fadad219650008e5026162e957a46f9"),
            got
        );
    }
}
