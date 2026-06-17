// Script builder. Concatenates opcode bytes and length-prefixed data
// pushes into a `Script`.

use crate::bytes::Bytes;
use crate::{Num, Opcode, Script};
use keys::{AddressHashEnum, Public};

#[derive(Default)]
pub struct Builder {
    data: Bytes,
}

impl Builder {
    // --- Standard script templates ----------------------------------

    /// `OP_DUP OP_HASH160 <hash> OP_EQUALVERIFY OP_CHECKSIG`
    pub fn build_p2pkh(address: &AddressHashEnum) -> Script {
        Builder::default()
            .push_opcode(Opcode::OP_DUP)
            .push_opcode(Opcode::OP_HASH160)
            .push_bytes(&address.to_vec())
            .push_opcode(Opcode::OP_EQUALVERIFY)
            .push_opcode(Opcode::OP_CHECKSIG)
            .into_script()
    }

    /// `<pubkey> OP_CHECKSIG`
    pub fn build_p2pk(pubkey: &Public) -> Script {
        Builder::default()
            .push_bytes(pubkey)
            .push_opcode(Opcode::OP_CHECKSIG)
            .into_script()
    }

    /// `OP_HASH160 <hash> OP_EQUAL`
    pub fn build_p2sh(address: &AddressHashEnum) -> Script {
        Builder::default()
            .push_opcode(Opcode::OP_HASH160)
            .push_bytes(&address.to_vec())
            .push_opcode(Opcode::OP_EQUAL)
            .into_script()
    }

    /// `OP_0 <hash>` — works for both P2WPKH (20-byte hash) and P2WSH
    /// (32-byte hash); the hash size disambiguates.
    pub fn build_witness_script(address: &AddressHashEnum) -> Script {
        Builder::default()
            .push_opcode(Opcode::OP_0)
            .push_bytes(&address.to_vec())
            .into_script()
    }

    /// `OP_RETURN <data>` — standard nulldata output.
    pub fn build_nulldata(payload: &[u8]) -> Script {
        Builder::default()
            .push_opcode(Opcode::OP_RETURN)
            .push_bytes(payload)
            .into_script()
    }

    // --- Append primitives ------------------------------------------

    pub fn push_opcode(mut self, op: Opcode) -> Self {
        self.data.push(op as u8);
        self
    }

    pub fn push_bool(self, b: bool) -> Self {
        self.push_opcode(if b { Opcode::OP_1 } else { Opcode::OP_0 })
    }

    pub fn push_num(self, n: Num) -> Self {
        self.push_data(&n.to_bytes())
    }

    /// Append a single OP_PUSHBYTES_N push. Length must be in 1..=75.
    pub fn push_bytes(mut self, bytes: &[u8]) -> Self {
        let len = bytes.len();
        assert!(
            (1..=75).contains(&len),
            "push_bytes called with len {}, must be 1..=75",
            len
        );
        self.data.push(len as u8);
        self.data.extend_from_slice(bytes);
        self
    }

    /// Append a data push, choosing the smallest of OP_PUSHBYTES_N /
    /// OP_PUSHDATA1 / OP_PUSHDATA2 / OP_PUSHDATA4 that fits.
    pub fn push_data(mut self, data: &[u8]) -> Self {
        let len = data.len();
        match len {
            l if l < Opcode::OP_PUSHDATA1 as usize => {
                self.data.push(l as u8);
            },
            l if l <= u8::MAX as usize => {
                self.data.push(Opcode::OP_PUSHDATA1 as u8);
                self.data.push(l as u8);
            },
            l if l <= u16::MAX as usize => {
                self.data.push(Opcode::OP_PUSHDATA2 as u8);
                self.data.extend_from_slice(&(l as u16).to_le_bytes());
            },
            l if (l as u64) <= u32::MAX as u64 => {
                self.data.push(Opcode::OP_PUSHDATA4 as u8);
                self.data.extend_from_slice(&(l as u32).to_le_bytes());
            },
            _ => panic!("push_data called with payload over 4GiB"),
        }
        self.data.extend_from_slice(data);
        self
    }

    /// `OP_RETURN <bytes>` packed inline. Length must be in 1..=75.
    pub fn return_bytes(mut self, bytes: &[u8]) -> Self {
        let len = bytes.len();
        assert!(
            (1..=75).contains(&len),
            "return_bytes called with len {}, must be 1..=75",
            len
        );
        self.data.push(Opcode::OP_RETURN as u8);
        self.data.push(len as u8);
        self.data.extend_from_slice(bytes);
        self
    }

    /// Append a single 0xff byte (an unassigned opcode), used by
    /// fuzzing / negative test paths.
    pub fn push_invalid_opcode(mut self) -> Self {
        self.data.push(0xff);
        self
    }

    pub fn into_script(self) -> Script {
        Script::new(self.data)
    }
    pub fn into_bytes(self) -> Bytes {
        self.data
    }
}
