//! V2 swap-protocol redeem-script builders (chapter 15).
//!
//! Each function returns the **redeem script** that is wrapped as P2SH
//! on-chain. Branch selection is performed by `OP_IF`/`OP_ELSE` flags
//! pushed by the spender's script-sig.
//!
//! All `*_secret_hash` inputs are 32-byte `sha256(secret)` digests;
//! `ripemd160` is applied inside the builder before being embedded so
//! the script's `OP_HASH160 <push> OP_EQUALVERIFY` check matches
//! `OP_HASH160(secret) == ripemd160(sha256(secret))`.

use kdf_crypto::ripemd160;
use keys::Public;
use script::{Builder, Opcode, Script};

/// Convert 32-byte sha256(secret) into the 20-byte ripemd160 hash
/// embedded in the script.
fn h160_of(secret_hash_32: &[u8]) -> [u8; 20] {
    let r = ripemd160(secret_hash_32);
    let mut out = [0u8; 20];
    out.copy_from_slice(r.as_slice());
    out
}

/// Taker funding redeem script (§15.3.1).
///
/// Outer `OP_IF`:
/// - `OP_1`: timelock refund — `<locktime> CLTV DROP <taker_pub> CHECKSIG`
/// - `OP_0`: inner `OP_IF`:
///   - `OP_1`: cooperative co-signature — `<taker_pub> CHECKSIGVERIFY <maker_pub> CHECKSIG`
///   - `OP_0`: secret-reveal refund — `SIZE 32 EQUALVERIFY HASH160 <r160(taker_secret_hash)> EQUALVERIFY <taker_pub> CHECKSIG`
pub fn taker_funding_script(locktime: u32, taker_secret_hash: &[u8], taker_pub: &Public, maker_pub: &Public) -> Script {
    let secret_h160 = h160_of(taker_secret_hash);
    Builder::default()
        .push_opcode(Opcode::OP_IF)
        .push_bytes(&locktime.to_le_bytes())
        .push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
        .push_opcode(Opcode::OP_DROP)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_IF)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIGVERIFY)
        .push_bytes(maker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_SIZE)
        .push_bytes(&[32])
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_opcode(Opcode::OP_HASH160)
        .push_bytes(&secret_h160)
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ENDIF)
        .push_opcode(Opcode::OP_ENDIF)
        .into_script()
}

/// Taker payment redeem script (§15.3.2).
///
/// `OP_IF`:
/// - `OP_1`: timelock refund — `<locktime> CLTV DROP <taker_pub> CHECKSIG`
/// - `OP_0`: cooperative spend revealing maker secret —
///   `SIZE 32 EQUALVERIFY HASH160 <r160(maker_secret_hash)> EQUALVERIFY
///    <taker_pub> CHECKSIGVERIFY <maker_pub> CHECKSIG`
pub fn taker_payment_script(locktime: u32, maker_secret_hash: &[u8], taker_pub: &Public, maker_pub: &Public) -> Script {
    let secret_h160 = h160_of(maker_secret_hash);
    Builder::default()
        .push_opcode(Opcode::OP_IF)
        .push_bytes(&locktime.to_le_bytes())
        .push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
        .push_opcode(Opcode::OP_DROP)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_SIZE)
        .push_bytes(&[32])
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_opcode(Opcode::OP_HASH160)
        .push_bytes(&secret_h160)
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIGVERIFY)
        .push_bytes(maker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ENDIF)
        .into_script()
}

/// Maker payment redeem script (§15.3.3).
///
/// Outer `OP_IF`:
/// - `OP_1`: timelock refund — `<locktime> CLTV DROP <maker_pub> CHECKSIG`
/// - `OP_0`: inner `OP_IF`:
///   - `OP_1`: taker spends with maker secret —
///     `SIZE 32 EQUALVERIFY HASH160 <r160(maker_secret_hash)> EQUALVERIFY <taker_pub> CHECKSIG`
///   - `OP_0`: maker refunds with taker secret —
///     `SIZE 32 EQUALVERIFY HASH160 <r160(taker_secret_hash)> EQUALVERIFY <maker_pub> CHECKSIG`
pub fn maker_payment_script(
    locktime: u32,
    maker_secret_hash: &[u8],
    taker_secret_hash: &[u8],
    maker_pub: &Public,
    taker_pub: &Public,
) -> Script {
    let maker_h160 = h160_of(maker_secret_hash);
    let taker_h160 = h160_of(taker_secret_hash);
    Builder::default()
        .push_opcode(Opcode::OP_IF)
        .push_bytes(&locktime.to_le_bytes())
        .push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
        .push_opcode(Opcode::OP_DROP)
        .push_bytes(maker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_IF)
        .push_opcode(Opcode::OP_SIZE)
        .push_bytes(&[32])
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_opcode(Opcode::OP_HASH160)
        .push_bytes(&maker_h160)
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_bytes(taker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ELSE)
        .push_opcode(Opcode::OP_SIZE)
        .push_bytes(&[32])
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_opcode(Opcode::OP_HASH160)
        .push_bytes(&taker_h160)
        .push_opcode(Opcode::OP_EQUALVERIFY)
        .push_bytes(maker_pub)
        .push_opcode(Opcode::OP_CHECKSIG)
        .push_opcode(Opcode::OP_ENDIF)
        .push_opcode(Opcode::OP_ENDIF)
        .into_script()
}
