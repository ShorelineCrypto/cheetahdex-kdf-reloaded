# Chapter 15 — Atomic-Swap V2 UTXO Path

> **Status in reloaded:** *gap.* The V2 swap state-machine core
> ([Chapter 14 — state-machine runtime](14-state-machine-runtime.md))
> and the V2 coin-trait surface
> ([`mm2src/coins/lp_coins_traits.rs`](../../mm2src/coins/lp_coins_traits.rs))
> are present, but no concrete UTXO impl of those traits exists in this
> tree. EVM is currently the only consumer
> ([Chapter 17 — V2 EVM swap path](17-swap-v2-evm-path.md)). This
> chapter is the **driving specification**: a clean-room implementer
> who reads only this chapter, the V2 trait definitions, and the
> existing V1 UTXO swap code in [`mm2src/coins/utxo/utxo_common/utxo_common_swap.rs`](../../mm2src/coins/utxo/utxo_common/utxo_common_swap.rs)
> must be able to reproduce a working UTXO V2 implementation.

---

## 15.0 Executive Summary

The V2 atomic-swap protocol replaces the V1 single-payment HTLC with
a **two-stage payment flow** on the taker side and a **dual-secret
HTLC** on the maker side:

| Side  | V1                                          | V2                                                                                   |
|-------|---------------------------------------------|--------------------------------------------------------------------------------------|
| Maker | Single HTLC bound to one secret hash        | HTLC bound to **both** maker-secret-hash and taker-secret-hash                       |
| Taker | Single HTLC bound to maker-secret-hash      | **Funding** tx → **Payment** tx (cooperative funding-spend turns funding into payment) |
| Dex-fee delivery | Separate dex-fee tx                | Folded into the funding amount; the funding-spend output goes to the dex-fee address |
| Pre-burn output | n/a (V1)                          | Optional pre-burn output funded by the same trade — see [Chapter 16](16-swap-v2-pre-burn-output.md) |

For UTXO coins this requires three Bitcoin-script forms (taker
funding, taker payment, maker payment) and a set of trait-method
implementations on `UtxoStandardCoin` that
the V2 state machines drive. All of the heavy lifting (UTXO
selection, script signing, P2SH spend construction, SPV-aware
validation) is delegated to helper functions in
`mm2src/coins/utxo/utxo_common/utxo_common_swap.rs` (the V1 helpers
module, which V2 extends in-place),
which mirror the existing V1 helpers but are parameterised by the new
`SwapTxTypeWithSecretHash::{MakerPaymentV2, TakerPaymentV2,
TakerFunding}` variants and the new V2 script builders in
`mm2src/coins/utxo/swap_proto_v2_scripts.rs`.

---

## 15.1 Why this exists

The V2 protocol was designed to:

1. **Atomically deliver the dex-fee** in the same on-chain footprint
   as the trade itself, instead of a separate dex-fee transaction
   that a malicious taker could try to omit or under-pay.
2. **Bind both parties' secrets** into the maker payment, so neither
   side can grief the other by withholding a secret without consequence.
3. **Enable optional pre-burn outputs** (Chapter 16) on the same
   funding spend, so deflationary or proof-of-burn policies can be
   enforced atomically with the trade.
4. **Standardise the on-chain script shape across coin families** so
   the EVM contract surface
   ([`coins/eth/eth_swap_v2/*`](../../mm2src/coins/eth/eth_swap_v2/))
   and the UTXO script surface express the same state machine in
   their respective native idioms.

No reloaded post-baseline commit motivates this chapter's content
(the trait surface and SM core were carried forward from the V1→V2
upgrade work in the baseline tree's history); the motivation above is
a clean-room restatement of the protocol design intent, derived from
the trait shape, the state graph in
[`maker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs)
and [`taker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs),
and the EVM contract ABI in
[`coins/eth/maker_swap_v2_abi.json`](../../mm2src/coins/eth/maker_swap_v2_abi.json).

---

## 15.2 Surface to implement on `UtxoStandardCoin`

Four traits from
[`mm2src/coins/lp_coins_traits.rs`](../../mm2src/coins/lp_coins_traits.rs)
must be implemented on `UtxoStandardCoin`:

- `ParseCoinAssocTypes` (8 associated types + 6 parse methods)
- `CommonSwapOpsV2` (2 derivation methods)
- `MakerCoinSwapOpsV2` (5 async methods)
- `TakerCoinSwapOpsV2` (14 async methods + 1 sync flag)

The argument and result types
(`SendMakerPaymentArgs`, `ValidateMakerPaymentArgs`,
`SendTakerFundingArgs`, `GenTakerFundingSpendArgs`,
`GenTakerPaymentSpendArgs`, `TxPreimageWithSig`,
`FundingTxSpend`, `RefundMakerPaymentTimelockArgs`,
`RefundMakerPaymentSecretArgs`, `RefundTakerPaymentArgs`,
`RefundFundingSecretArgs`, `SpendMakerPaymentArgs`,
`GenPreimageResult`, `ValidateSwapV2TxResult`,
`ValidateTakerFundingSpendPreimageResult`,
`ValidateTakerPaymentSpendPreimageResult`,
`FindPaymentSpendError`, `SearchForFundingSpendErr`)
are defined alongside the traits in `lp_coins_traits.rs` and
`lp_coins_types.rs` and are already used by the EVM impl; they are
re-used unchanged.

### 15.2.1 Associated types for UTXO

| Associated type    | UTXO concrete type                                  |
|--------------------|-----------------------------------------------------|
| `Address`          | `keys::Address`                                     |
| `AddressParseError`| `keys::Error`                                       |
| `Pubkey`           | `keys::Public`                                      |
| `PubkeyParseError` | `keys::Error`                                       |
| `Tx`               | `chain::Transaction` (alias `UtxoTx`)               |
| `TxParseError`     | `serialization::Error`                              |
| `Preimage`         | `UtxoTxPreimage` — local newtype around `chain::TransactionInputSigner` |
| `PreimageParseError`| `serialization::Error`                             |
| `Sig`              | `keys::Signature`                                   |
| `SigParseError`    | `keys::Error`                                       |

**Preimage newtype.** The V2 trait surface in
[`mm2src/coins/lp_coins_traits.rs`](../../mm2src/coins/lp_coins_traits.rs)
requires `Preimage: ToBytes`, and `ToBytes` is declared with a blanket
`impl<T: AsRef<[u8]>> ToBytes for T`. Because Rust's orphan rules
forbid a direct `impl ToBytes for TransactionInputSigner` in this
crate (the trait and the type live in different crates and the blanket
already covers any `AsRef<[u8]>` shape the foreign type might one day
gain), the UTXO V2 impl introduces a local newtype
`pub struct UtxoTxPreimage(pub TransactionInputSigner);` and provides
its own `ToBytes` impl that serialises the contained signer as a
finalised `UtxoTx`. `parse_preimage` deserialises bytes into a
`UtxoTx`, then converts into `TransactionInputSigner` via the existing
`From<UtxoTx> for TransactionInputSigner` conversion.

**`AsRef<[u8]>` for `keys::Public` and `keys::Signature`.** Both types
already expose their bytes through `Deref<Target = [u8]>`. To let the
blanket `ToBytes` impl cover them — which the V2 trait surface assumes —
the UTXO V2 work adds plain `impl AsRef<[u8]>` to each in
[`mm2src/kdf_keys/src/public.rs`](../../mm2src/kdf_keys/src/public.rs)
and [`mm2src/kdf_keys/src/signature.rs`](../../mm2src/kdf_keys/src/signature.rs).
This is a coherence-mechanic adapter, not a behavioural change.

**`my_addr()`** returns the coin's current HTLC address. For Iguana
(single-keypair) policy this is the coin's primary address. For
HD-wallet and Trezor policies a per-swap HTLC address must be derived
— see [§15.6 Common Swap Ops V2](#156-commonswapopsv2--derivation)
for the keypair-derivation helper used (`get_htlc_key_pair_v2` in
[`mm2src/coins/utxo/utxo_common/utxo_common_swap.rs`](../../mm2src/coins/utxo/utxo_common/utxo_common_swap.rs)).

> **Phase 3 deferral.** The initial V2 UTXO implementation targets
> the Iguana (single-keypair) `PrivKeyPolicy`. The HD-wallet branch
> of `my_addr()` and the Trezor branch of `derive_htlc_pubkey_v2` are
> intentionally left as `unimplemented!("ch15 phase 2: ...")` stubs
> in `utxo_standard_swap_v2.rs`. Hardware-wallet and HD-aware V2 swap
> paths are tracked as future work.

`parse_pubkey` must accept the compressed 33-byte form
(SEC1 prefix `0x02`/`0x03`) via `Public::from_slice`. `parse_tx` uses
the `serialization` crate's `deserialize` directly. `parse_preimage`
deserialises a `UtxoTx` then wraps in `UtxoTxPreimage` as described
above. `parse_signature` accepts the raw byte form used by the
existing UTXO code (`Signature::from(bytes.to_vec())`).

---

## 15.3 Bitcoin scripts (V2 protocol)

All three scripts live in
[`mm2src/coins/utxo/swap_proto_v2_scripts.rs`](../../mm2src/coins/utxo/swap_proto_v2_scripts.rs).
Each is wrapped as **P2SH**: the on-chain output is
`OP_HASH160 <ripemd160(sha256(redeem))> OP_EQUAL`, and the redeem
script is supplied at spend time via the script-sig. Each redeem
script has multiple branches selected by the script-sig pushing
`OP_0`/`OP_1` flags onto the stack before the script runs.

**Secret-hash width.** Throughout V2, secret hashes carried in args
and encoded in scripts are 32-byte `sha256(secret)` digests. The
script builders apply `ripemd160` internally before pushing the
20-byte digest into the `OP_HASH160 <…> OP_EQUALVERIFY` check, so the
on-chain comparison is `OP_HASH160(secret) == ripemd160(sha256(secret))`
(the standard Bitcoin dhash160). The 32-byte width matches the V2
argument types in [`lp_coins_types.rs`](../../mm2src/coins/lp_coins_types.rs)
(`MakerPaymentV2`, `TakerPaymentV2`, `TakerFunding`).

**CLTV encoding.** Locktimes are encoded as 4-byte little-endian
pushes (`time_lock.to_le_bytes()`), matching the existing V1 UTXO
swap scripts in `utxo_common_swap.rs`.

### 15.3.1 Taker funding script

Two branches, selected by a single `OP_IF`:

- **Refund-by-timelock branch (`OP_1`)** — `<locktime> OP_CHECKLOCKTIMEVERIFY OP_DROP <taker_pub> OP_CHECKSIG`
- **Cooperative branches (`OP_0`)** — a nested `OP_IF`:
  - `OP_1`: cooperative co-signature path — `<taker_pub> OP_CHECKSIGVERIFY <maker_pub> OP_CHECKSIG`
  - `OP_0`: secret-reveal refund — `OP_SIZE <32> OP_EQUALVERIFY OP_HASH160 <ripemd160(taker_secret_hash)> OP_EQUALVERIFY <taker_pub> OP_CHECKSIG`

Builder signature:

```rust
pub fn taker_funding_script(
    locktime: u32,
    taker_secret_hash: &[u8],   // 32 bytes; ripemd160-of-sha256 is applied inside
    taker_pub: &Public,
    maker_pub: &Public,
) -> Script;
```

### 15.3.2 Taker payment script

Created from the funding output by `sign_and_send_taker_funding_spend`.
Two branches:

- **Refund-by-timelock (`OP_1`)** — taker can reclaim after locktime
- **Cooperative spend (`OP_0`)** — both sigs required and the **maker's**
  secret must be revealed: `OP_SIZE <32> OP_EQUALVERIFY OP_HASH160
  <ripemd160(maker_secret_hash)> OP_EQUALVERIFY <taker_pub>
  OP_CHECKSIGVERIFY <maker_pub> OP_CHECKSIG`

Builder signature:

```rust
pub fn taker_payment_script(
    locktime: u32,
    maker_secret_hash: &[u8],
    taker_pub: &Public,
    maker_pub: &Public,
) -> Script;
```

### 15.3.3 Maker payment script

Three logical branches, encoded with nested `OP_IF`:

- **Refund-by-timelock (`OP_1`)** — `<locktime> OP_CHECKLOCKTIMEVERIFY OP_DROP <maker_pub> OP_CHECKSIG`
- **Taker-spends-with-maker-secret (`OP_0`/`OP_1`)** — `OP_SIZE <32> OP_EQUALVERIFY OP_HASH160 <ripemd160(maker_secret_hash)> OP_EQUALVERIFY <taker_pub> OP_CHECKSIG`
- **Maker-refunds-with-taker-secret (`OP_0`/`OP_0`)** — `OP_SIZE <32> OP_EQUALVERIFY OP_HASH160 <ripemd160(taker_secret_hash)> OP_EQUALVERIFY <maker_pub> OP_CHECKSIG`

Builder signature:

```rust
pub fn maker_payment_script(
    locktime: u32,
    maker_secret_hash: &[u8],
    taker_secret_hash: &[u8],
    maker_pub: &Public,
    taker_pub: &Public,
) -> Script;
```

The "maker can refund by revealing taker's secret" branch is the key
shape difference from V1 and is what enforces atomicity of the
two-payment exchange: if the taker abandons the swap after sending
funding but before sending the funding-spend signature, the maker
cannot move forward — but if the maker has already revealed her
secret in a published spend, the taker can refund via the taker
funding script's secret-reveal branch and reclaim funds without
waiting for the timelock.

---

## 15.4 `MakerCoinSwapOpsV2` — method specs

All five methods return `Result<UtxoTx, TransactionErr>` or
`ValidateSwapV2TxResult` and delegate to corresponding `*_v2` helpers
in `utxo_common/utxo_common_swap.rs`. Algorithms below describe what each helper must
do; the trait impl on `UtxoStandardCoin` is a thin forwarder.

### 15.4.1 `send_maker_payment_v2`

1. Derive maker's HTLC keypair from `args.swap_unique_data` via
   `get_htlc_key_pair(self.as_ref(), args.swap_unique_data)`.
2. Parse `args.taker_pub` via `parse_pubkey`.
3. Build the maker-payment script (§15.3.3) with
   `time_lock = args.time_lock`,
   `maker_secret_hash = args.maker_secret_hash`,
   `taker_secret_hash = args.taker_secret_hash`,
   `maker_pub = htlc_keypair.public()`,
   `taker_pub = parsed taker pub`.
4. Wrap as P2SH; build an output of value
   `sat_from_big_decimal(&args.amount, self.as_ref().decimals)`.
5. Use `generate_swap_payment_outputs` (existing V1 helper, generalised
   to accept the V2 script) → `send_outputs_from_my_address_impl`
   → broadcast.
6. Return the `UtxoTx`.

Errors: `TransactionErr` for UTXO selection / fee estimation / sign /
broadcast failures; the helper wraps low-level errors with context.

### 15.4.2 `validate_maker_payment_v2`

1. Derive maker's HTLC pubkey from `args.maker_pub`.
2. Call `validate_payment` (V1+V2 shared helper) with:
   - `tx = args.maker_payment_tx`
   - `output_index = DEFAULT_SWAP_VOUT (0)`
   - `first_pub = maker_pub`, `second_pub = taker_pub`
   - `tx_type_with_secret_hash = SwapTxTypeWithSecretHash::MakerPaymentV2 { maker_secret_hash, taker_secret_hash }`
   - `amount = args.amount`
   - `watcher_reward = None` (V2 doesn't use watcher rewards on UTXO yet)
   - `time_lock = args.time_lock`
   - `try_spv_proof_until = args.try_spv_proof_until`
   - `confirmations = args.confirmations`
3. `validate_payment` reconstructs the expected redeem script
   from `tx_type_with_secret_hash.redeem_script()`, compares the
   output's `script_pubkey` to `P2SH(dhash160(redeem_script))`,
   verifies the amount, polls for confirmations, and (if Electrum +
   SPV-validated) verifies the SPV proof.

The `redeem_script()` method on `SwapTxTypeWithSecretHash` must be
extended to dispatch the `MakerPaymentV2` variant to
`maker_payment_script(time_lock, maker_secret_hash, taker_secret_hash, maker_pub, taker_pub)`
and the `TakerPaymentV2` variant to
`taker_payment_script(time_lock, maker_secret_hash, taker_pub, maker_pub)`.
The `TakerFunding` variant dispatches to
`taker_funding_script(time_lock, taker_secret_hash, taker_pub, maker_pub)`.

### 15.4.3 `refund_maker_payment_v2_timelock`

Delegates to `refund_htlc_payment` (the generic V1+V2 refund helper)
with:

- `tx = args.payment_tx`
- `tx_type_with_secret_hash = SwapTxTypeWithSecretHash::MakerPaymentV2 { maker_secret_hash, taker_secret_hash }`
- `other_pubkey = parsed taker_pub`
- `time_lock = args.time_lock`
- `script_data = [OP_1]` (selects the outer timelock branch)
- `sequence = SEQUENCE_FINAL - 1` (enables locktime check)

`refund_htlc_payment` builds a P2SH spending preimage (§15.6),
signs with the maker HTLC keypair, assembles the script-sig as
`[sig, OP_1, redeem_script]`, broadcasts and returns the tx.

### 15.4.4 `refund_maker_payment_v2_secret`

Immediate refund path: maker reveals **taker's** secret. Constructs:

- `script_data = [taker_secret (32 bytes), OP_0, OP_0]` — pushes the
  secret, then two `OP_0` flags to select the inner "maker refunds
  with taker secret" branch.
- `sequence = SEQUENCE_FINAL`
- `time_lock = 0` (no CLTV gate)
- Signs with maker keypair; broadcasts.

This path lets the maker reclaim her own funds the moment she learns
the taker's secret (typically by observing the taker's funding refund
on-chain), without waiting for the timelock.

### 15.4.5 `spend_maker_payment_v2`

Taker spending maker's payment to extract the agreed coin. The
witness must reveal the **maker's** secret. Construct:

- `script_data = [maker_secret (32 bytes), OP_1, OP_0]` — pushes
  secret, then `OP_1`/`OP_0` to select the inner "taker spends with
  maker secret" branch.
- `sequence = SEQUENCE_FINAL`
- `time_lock = 0`
- Signs with taker keypair, builds final script-sig as
  `[taker_sig, maker_secret, OP_1, OP_0, redeem_script]`, broadcasts.

(Note: the maker-payment script has only one signature required in
the secret-reveal branch — the taker's — because the cooperative
two-sig branch is the timelock-refund alternative for the maker.)

---

## 15.5 `TakerCoinSwapOpsV2` — method specs

### 15.5.1 `send_taker_funding`

1. Derive taker HTLC keypair from `args.swap_unique_data`.
2. Compute the funding amount as
   `args.trading_amount + args.premium_amount + args.dex_fee.fee_amount()`
   (all `BigDecimal`, converted to sats via `sat_from_big_decimal`).
3. Build the taker-funding script (§15.3.1) and the P2SH output.
4. Reuse `generate_swap_payment_outputs` + `send_outputs_from_my_address_impl`.
5. Broadcast. Return the funding `UtxoTx`.

### 15.5.2 `validate_taker_funding`

1. Parse `args.funding_tx`.
2. Reconstruct expected script via
   `taker_funding_script(args.funding_time_lock,
   args.taker_secret_hash, args.taker_pub, args.maker_pub)`.
3. Verify `tx.outputs[DEFAULT_SWAP_VOUT].script_pubkey == P2SH(dhash160(script))`.
4. Verify the output value equals
   `args.trading_amount + args.premium_amount + args.dex_fee.fee_amount()`
   (converted to sats).
5. On native mode, call `import_address` for the P2SH address so the
   node tracks spends.
6. Returns `Ok(())` on `ValidateSwapV2TxResult`.

### 15.5.3 `refund_taker_funding_timelock`

Same shape as §15.4.3 but on the funding tx, with
`SwapTxTypeWithSecretHash::TakerFunding { taker_secret_hash }` and
`script_data = [OP_1, OP_0]` (timelock branch is the outer `OP_IF`'s
true arm in the funding script).

### 15.5.4 `refund_taker_funding_secret`

Immediate refund: taker reveals her own secret on the funding script's
inner secret-reveal branch (`OP_0`, `OP_0`). `script_data =
[taker_secret, OP_0, OP_0]`. Signs and broadcasts.

### 15.5.5 `search_for_taker_funding_spend`

Given the funding `tx`, scan from `from_block` for any transaction
spending `tx.hash():DEFAULT_SWAP_VOUT`. When found, inspect the
spend's script-sig at instruction index `1` (after the signature(s)):

- `OP_1` → timelock refund → `FundingTxSpend::RefundedTimelock(spend_tx)`
- `OP_PUSHBYTES_32` (raw 32-byte push) → secret refund → extract the
  pushed bytes, return
  `FundingTxSpend::RefundedSecret { tx: spend_tx, secret }`
- Otherwise → assume cooperative spend → the funding has been
  converted to a taker-payment by the maker-signed funding-spend →
  `FundingTxSpend::TransferredToTakerPayment(spend_tx)`

On chains without per-output spend index (native UTXO without
electrum), use the existing V1 `search_for_swap_tx_spend` polling
loop pattern, adapted for the V2 funding script's branch flags.

### 15.5.6 `gen_taker_funding_spend_preimage`

Generates the unsigned tx that, once both parties sign, converts
the funding output into the taker-payment output:

1. Build the taker-payment script (§15.3.2) with
   `locktime = args.taker_payment_time_lock`,
   `maker_secret_hash = args.maker_secret_hash`,
   the two HTLC pubkeys.
2. Compute the funding-spend fee. The fee policy is
   `FundingSpendFeeSetting::EstimatedByCoin` — call
   `get_htlc_spend_fee(DEFAULT_SWAP_TX_SPEND_SIZE)`.
3. Build a `TransactionInputSigner` spending the funding output, with
   a single output: P2SH of the taker-payment script for value
   `funding_value - fee`.
4. Set `lock_time = 0`, `sequence = SEQUENCE_FINAL`.
5. Sign the input with the taker HTLC keypair using `SIGHASH_ALL`.
6. Return `TxPreimageWithSig { preimage: signer, signature: taker_sig }`.

### 15.5.7 `validate_taker_funding_spend_preimage`

Maker side. Re-derive the expected preimage as in §15.5.6, then:

1. Compare the preimage's input outpoint, output script, and output
   value (allowing ±10% fee tolerance — re-derive the fee both ways
   and check `|preimage_output_value - expected_value| ≤ 0.1 * expected_value`).
2. Verify the supplied taker signature against the preimage's input
   signature hash for the funding-script secret-reveal branch
   (cooperative co-sig path, not the timelock path), using the
   `SIGHASH_ALL` digest.
3. Return `Ok(())` or the appropriate
   `ValidateTakerFundingSpendPreimageResult::Err(...)` variant.

### 15.5.8 `sign_and_send_taker_funding_spend`

Taker side, having received the maker's signature on top of the
preimage. Build the final tx:

1. Re-derive the preimage tx exactly as in §15.5.6.
2. Sign the input with the taker HTLC keypair (`SIGHASH_ALL`).
3. Build the script-sig:
   `[OP_0, maker_sig (with sighash byte), taker_sig (with sighash byte), OP_1, OP_0, redeem_script]`
   — the leading `OP_0` satisfies the standard OP_CHECKMULTISIG bug
   if multisig is used; the two `OP_1, OP_0` flags select the
   cooperative branch of the funding script.
4. Set sequence/locktime as in §15.5.6.
5. Broadcast.

### 15.5.9 `refund_combined_taker_payment`

Timelock refund of the taker-payment tx (after funding was already
converted). Same machinery as §15.5.3, but using
`SwapTxTypeWithSecretHash::TakerPaymentV2 { maker_secret_hash,
taker_secret_hash }` and `script_data = [OP_1]` (the taker-payment
script's outer timelock branch).

### 15.5.10 `skip_taker_payment_spend_preimage`

UTXO returns `false` (the default). UTXO **does** need a preimage
exchange because the maker must add her signature to the
taker-payment-spend tx before it can be broadcast. (EVM returns
`true` because the EVM contract handles spend authorisation
on-chain without preimage exchange.)

### 15.5.11 `gen_taker_payment_spend_preimage`

Taker side. Generates the unsigned spend of the taker-payment
output that, once the maker signs and reveals her secret, transfers
the agreed amount to the maker and the dex-fee amount to the
dex-fee address. Build:

1. A `TransactionInputSigner` with one input (the taker-payment
   output) and one or two outputs depending on `args.dex_fee`:
   - **`DexFee::Standard(amount)`** — one output: maker's address
     receiving `taker_payment_value - dex_fee_amount - fee_estimate`.
     The dex-fee output is added later by the maker in
     `sign_and_broadcast_taker_payment_spend` (§15.5.13). Sign with
     `SIGHASH_SINGLE | SIGHASH_ANYONECANPAY`-style scheme is **not**
     used here — instead the taker signs with `SIGHASH_SINGLE` so
     the maker can append outputs without invalidating the
     signature.
   - **`DexFee::WithBurn { fee_amount, burn_amount }`** or other
     pre-burn variants — see [Chapter 16](16-swap-v2-pre-burn-output.md).
     Sign with `SIGHASH_ALL` because all outputs are fixed at this
     stage.
2. Sign with the taker HTLC keypair.
3. Return `TxPreimageWithSig { preimage, signature: taker_sig }`.

### 15.5.12 `validate_taker_payment_spend_preimage`

Maker side. Mirror of §15.5.7:

1. Re-derive the expected preimage tx.
2. Verify the taker's signature against the appropriate sighash
   (`SIGHASH_SINGLE` for `DexFee::Standard`, `SIGHASH_ALL` otherwise).
3. For `DexFee::Standard`, allow that the preimage has only the
   maker-bound output and that the dex-fee output will be appended.
4. Return `Ok(())` or
   `ValidateTakerPaymentSpendPreimageResult::Err(...)`.

### 15.5.13 `sign_and_broadcast_taker_payment_spend`

Maker side, finalising the spend with her secret. Build:

1. Start from the validated preimage.
2. For `DexFee::Standard`, append the dex-fee output (value
   `dex_fee.fee_amount()`, address = the dex-fee address from coin
   config). The fee for this added output is taken out of the maker's
   share, not re-computed.
3. Compute the maker's signature for the taker-payment input,
   matching the same sighash scheme the taker used.
4. Build the script-sig:
   `[OP_0, maker_sig, taker_sig, maker_secret, OP_0, redeem_script]`
   — `OP_0` selects the cooperative-with-secret branch of the
   taker-payment script.
5. Broadcast.

`preimage: Option<&TxPreimageWithSig<Self>>` is `Some` for UTXO.

### 15.5.14 `find_taker_payment_spend_tx`

Poll the chain from `from_block` for any transaction spending the
taker-payment output. Poll every 10 seconds until `wait_until`
(unix seconds). Return the spending tx, or
`FindPaymentSpendError::Timeout` if the deadline elapses.

### 15.5.15 `extract_secret_v2`

Walk the spend tx's input(s) at `vin = DEFAULT_SWAP_VIN (0)`. Parse
the script-sig instructions; for each `OP_PUSHBYTES_32` push, compute
`dhash160(push)` and compare against `secret_hash` (which is itself
the `dhash160` of the protocol secret). On match, return the 32 raw
bytes. Otherwise return `Err("Secret not found in spend transaction")`.

`dhash160` here is `ripemd160(sha256(x))`, matching the script
hashing used by `OP_HASH160`.

---

## 15.6 `CommonSwapOpsV2` — derivation

### 15.6.1 `derive_htlc_pubkey_v2`

```rust
fn derive_htlc_pubkey_v2(&self, swap_unique_data: &[u8]) -> Public {
    *get_htlc_key_pair_v2(self.as_ref(), swap_unique_data)
        .expect("htlc keypair derivation")
        .public()
}
```

`get_htlc_key_pair_v2` is a new V2-specific helper added to
`utxo_common/utxo_common_swap.rs` alongside (not replacing) the V1
`get_htlc_key_pair`. It is fallible (`Result<KeyPair, String>`):

- For `PrivKeyPolicy::KeyPair(kp)` (Iguana), returns `kp`.
- For `PrivKeyPolicy::HDWallet { activated_key, .. }`, returns the
  activated key. (Per-swap HD derivation off `swap_unique_data` is a
  Phase 3 refinement.)
- For `PrivKeyPolicy::Trezor`, returns `Err(...)` — the
  hardware-wallet path is a Phase 3 refinement and the trait method
  body above carries an `unimplemented!` under the Trezor branch.

The distinct `*_v2` name avoids mutating the semantics of V1's
`get_htlc_key_pair`, which has a different signature and a different
behavioural contract under HD/Trezor (it returns `Option`).

`swap_unique_data` is the swap's UUID bytes. The current
implementation does not yet thread it into derivation; the same coin
yields the same HTLC keypair for every swap. Per-swap key isolation
is Phase 3 work.

### 15.6.2 `derive_htlc_pubkey_v2_bytes`

```rust
fn derive_htlc_pubkey_v2_bytes(&self, swap_unique_data: &[u8]) -> Vec<u8> {
    self.derive_htlc_pubkey_v2(swap_unique_data).to_bytes().into()
}
```

Returns the compressed 33-byte SEC1 form for P2P transmission in the
V2 negotiation messages.

---

### 15.6.3 Known chapter rough edges (Phase 2 residual)

The initial V2 UTXO impl surfaced a small set of chapter
inconsistencies that did not block compilation or unit tests but
should be reconciled in the next chapter revision:

- The taker-payment / funding redeem scripts use sequential
  `CHECKSIGVERIFY` + `CHECKSIG`, not `OP_CHECKMULTISIG`, so the
  leading `OP_0` stuffer listed in §15.5.8 step 3 and §15.5.13 step 3
  is unnecessary and is omitted by the implementation.
- §15.5.6 prose specifies a single P2SH(taker_payment) output for
  the funding-spend preimage; the dex-fee delivery mechanism is
  intentionally deferred to [Chapter 16 — V2 pre-burn output](16-swap-v2-pre-burn-output.md).
  Implementations of `DexFee::WithBurn` / `DexFee::NoFee` in the
  taker-payment-spend helpers therefore return an explicit
  "deferred to ch16" error rather than panicking.
- `SwapTxTypeWithSecretHash::*` callers pass `time_lock: u32` (the
  4-byte LE form encoded into the redeem script). V2 trait args
  carry `time_lock: u64` for cross-protocol uniformity; the UTXO V2
  helpers cast `as u32` at the boundary. Locktimes outside the u32
  range are not a valid Bitcoin-script value and are rejected by
  the script builder.
- The secret bytes pushed onto the script-sig in cooperative-spend
  paths are 32 bytes (the `sha256` preimage). The secret-hash bytes
  embedded in the redeem script are 20 bytes (`HASH160(sha256(secret))`)
  for the cooperative-spend secret-reveal branch, matching V1 HTLC
  convention.
- §15.5.2 native-mode `import_address` step is performed best-effort
  with log-on-error; the analogous step is not currently mirrored
  in §15.4.2 maker-payment validation. A future revision should
  decide whether both paths should import or neither.
- `swap_unique_data` is not yet threaded into
  `GenTakerFundingSpendArgs` / `ValidateTakerFundingSpendPreimageArgs`;
  the helpers therefore pass an empty slice to `get_htlc_key_pair_v2`.
  This is harmless under the current single-keypair derivation but
  blocks per-swap key isolation when Phase 3 lands.

---

## 15.7 P2SH spend construction (shared helper)

A single `utxo_common::p2sh_spending_tx_preimage` helper underlies
every spend path (refund, cooperative spend, funding-spend
conversion). Signature:

```rust
async fn p2sh_spending_tx_preimage<T: UtxoCommonOps>(
    coin: &T,
    prev_tx: &UtxoTx,
    lock_time: LocktimeSetting,
    set_n_time: NTimeSetting,
    sequence: u32,
    outputs: Vec<TransactionOutput>,
) -> Result<TransactionInputSigner, String>;
```

`LocktimeSetting` is one of `Zero`, `FromCltv(u32)`. `NTimeSetting`
is `None` for non-PoS chains, `SetToNow` for PoS chains that use
`nTime`. The helper:

1. Spends `prev_tx.outputs[DEFAULT_SWAP_VOUT]` at vout 0.
2. Sets `lock_time` per `LocktimeSetting`.
3. Sets `n_time` per `NTimeSetting`.
4. Sets `consensus_branch_id` (Komodo/Zcash family).
5. Returns an unsigned `TransactionInputSigner` ready for the
   caller to sign in whatever sighash mode the script branch
   requires.

The signing step itself is performed by the caller using
`p2sh_spend` (existing V1 helper, which produces the final
script-sig given the redeem script, a signature, and the optional
script_data prefix bytes).

---

## 15.8 Helper inventory (additions to `utxo_common/utxo_common_swap.rs`)

| Helper                                  | Purpose                                                                            |
|-----------------------------------------|------------------------------------------------------------------------------------|
| `send_maker_payment_v2`                 | Build + broadcast maker-payment tx (§15.4.1)                                       |
| `spend_maker_payment_v2`                | Taker spends maker payment with maker-secret reveal (§15.4.5)                      |
| `refund_maker_payment_v2_secret`        | Maker immediate refund via taker-secret reveal (§15.4.4)                           |
| `send_taker_funding`                    | Build + broadcast taker funding tx (§15.5.1)                                       |
| `validate_taker_funding`                | Verify taker funding output structure + amount (§15.5.2)                           |
| `refund_taker_funding_secret`           | Taker immediate refund via taker-secret reveal (§15.5.4)                           |
| `gen_taker_funding_spend_preimage`      | Build unsigned funding→payment conversion + taker sig (§15.5.6)                    |
| `validate_taker_funding_spend_preimage` | Maker verifies taker's preimage signature (§15.5.7)                                |
| `sign_and_send_taker_funding_spend`     | Both-sigs cooperative funding-spend broadcast (§15.5.8)                            |
| `gen_taker_payment_spend_preimage`      | Build unsigned taker-payment spend + taker sig (§15.5.11)                          |
| `validate_taker_payment_spend_preimage` | Maker verifies taker's preimage signature (§15.5.12)                               |
| `sign_and_broadcast_taker_payment_spend`| Maker signs + adds dex-fee output + broadcasts (§15.5.13)                          |
| `extract_secret_v2`                     | Pull 32-byte secret from spend tx script-sig (§15.5.15)                            |
| `refund_htlc_payment`                   | Generic V1+V2 timelock refund (used by §15.4.3, §15.5.3, §15.5.9)                  |
| `validate_payment`                      | Generic V1+V2 payment-output validation (used by §15.4.2; pre-existing for V1)     |

`SwapTxTypeWithSecretHash::redeem_script()` must be implemented to
dispatch each variant to the correct script builder.

---

## 15.9 Constants

| Name                          | Value         | Purpose                                                              |
|-------------------------------|---------------|----------------------------------------------------------------------|
| `DEFAULT_SWAP_VOUT`           | `0`           | HTLC output index in every swap tx                                   |
| `DEFAULT_SWAP_VIN`            | `0`           | Input index that consumes a swap output in every spend tx            |
| `DEFAULT_SWAP_TX_SPEND_SIZE`  | `496` (bytes) | Estimated spend-tx size for fee calculation (P2SH spend with pre-burn capacity) |
| `SEQUENCE_FINAL`              | `0xffffffff`  | Disables CLTV/CSV checks; used for cooperative and secret-reveal branches |
| `SEQUENCE_FINAL - 1`          | `0xfffffffe`  | Enables CLTV check; used in timelock-refund spends                   |
| `SIGHASH_ALL`                 | `0x01`        | Standard sighash for fully-fixed-outputs spends                      |
| `SIGHASH_SINGLE`              | `0x03`        | Sighash for `DexFee::Standard` taker-payment-spend preimage          |

All values already exist in `mm2src/coins/utxo/utxo_common/utxo_common_swap.rs` or the
underlying `script` crate; the V2 implementation reuses them.

---

## 15.10 Tests

A working UTXO V2 implementation must include, at minimum:

1. **Unit tests** in `mm2src/coins/utxo/utxo_tests.rs`:
   - `should_build_taker_funding_script_with_expected_layout`
   - `should_build_taker_payment_script_with_expected_layout`
   - `should_build_maker_payment_script_with_expected_layout`
   - `should_validate_maker_payment_v2_against_known_good_tx`
   - `should_reject_maker_payment_v2_with_wrong_amount`
   - `should_extract_secret_from_taker_payment_spend`
   - `should_classify_funding_spend_as_timelock_refund`
   - `should_classify_funding_spend_as_secret_refund`
   - `should_classify_funding_spend_as_transferred_to_payment`
2. **Docker integration tests** in
   `mm2src/mm2_main/tests/docker_tests/` covering a full UTXO↔UTXO
   V2 swap (happy path, taker abort after funding, maker abort after
   payment) — reuse the V2 SM integration-test scaffolding added in
   commit `7ec1651b9` and extend the coin configuration to include
   UTXO V2 entries.

---

## 15.11 Wire / state-machine call sites

The state machines never call coin methods directly except through
the V2 trait surface. The relevant call sites in
`mm2src/mm2_main/src/lp_swap/`:

- `taker_swap_v2.rs`:
  - `SendTakerFunding::on_changed` → `send_taker_funding`
  - `WaitingForTakerFundingConfirmation::on_changed` → `wait_for_confirmations` (V1 helper) on the funding tx
  - `MakerPaymentAndFundingSpendPreimgReceived::on_changed` →
    `validate_taker_funding_spend_preimage` →
    `sign_and_send_taker_funding_spend`
  - `TakerPaymentSent::on_changed` → `gen_taker_payment_spend_preimage`
    → send preimage to maker over P2P
  - `MakerPaymentSpent::on_changed` → `find_taker_payment_spend_tx`
    → `extract_secret_v2` → `spend_maker_payment_v2`
  - Refund paths: `refund_taker_funding_timelock`,
    `refund_taker_funding_secret`, `refund_combined_taker_payment`
- `maker_swap_v2.rs`:
  - `TakerFundingReceived::on_changed` → `validate_taker_funding`
  - `MakerPaymentSentFundingSpendGenerated::on_changed` →
    `gen_taker_funding_spend_preimage` → `send_maker_payment_v2`
  - `TakerPaymentReceived::on_changed` →
    `validate_taker_payment_spend_preimage`
  - `TakerPaymentSpent::on_changed` →
    `sign_and_broadcast_taker_payment_spend`
  - Refund paths: `refund_maker_payment_v2_timelock`,
    `refund_maker_payment_v2_secret`

A UTXO impl that satisfies the trait signatures will work with these
call sites unchanged because the state machines are generic over
`MakerCoin: MakerCoinSwapOpsV2 + …` / `TakerCoin: TakerCoinSwapOpsV2 + …`.

---

## 15.12 Activation wiring

Once the trait impls land on `UtxoStandardCoin`, the kickstart
recovery handler must be extended. In
[`mm2src/mm2_main/src/lp_swap/swap_v2_common.rs`](../../mm2src/mm2_main/src/lp_swap/swap_v2_common.rs),
the functions `swap_kickstart_handler_for_maker` and
`swap_kickstart_handler_for_taker` currently match only the
`MmCoinEnum::EthCoin` variant. Both must be extended to also match
`MmCoinEnum::UtxoCoin(utxo)` and dispatch to the same generic
`swap_kickstart_handler::<UtxoStandardCoin, _>` (or
`<_, UtxoStandardCoin>` for the cross-coin case) so interrupted UTXO
V2 swaps can resume after a restart.

No new `SwapV2Type` variant is needed; the existing
`MakerSwapV2` / `TakerSwapV2` discriminants are coin-agnostic and
the stored DB repr (`MakerSwapDbRepr` / `TakerSwapDbRepr`) is too.

---

## 15.13 External references

- Bitcoin opcode semantics: <https://en.bitcoin.it/wiki/Script>
- BIP-65 (CHECKLOCKTIMEVERIFY): <https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki>
- SIGHASH flag semantics: <https://en.bitcoin.it/wiki/OP_CHECKSIG>
- P2SH (BIP-16): <https://github.com/bitcoin/bips/blob/master/bip-0016.mediawiki>

---

## 15.14 Provenance

- Spec authored from the V2 trait surface in
  [`mm2src/coins/lp_coins_traits.rs`](../../mm2src/coins/lp_coins_traits.rs)
  and [`mm2src/coins/lp_coins_types.rs`](../../mm2src/coins/lp_coins_types.rs),
  the V2 state machines in
  [`mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs)
  and [`mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs),
  and the EVM V2 impls in
  [`mm2src/coins/eth/eth_swap_v2/`](../../mm2src/coins/eth/eth_swap_v2/)
  as the analog reference for trait conformance.
- Bitcoin-script designs derived from the V1 UTXO swap script shapes
  in [`mm2src/coins/utxo/utxo_common/utxo_common_swap.rs`](../../mm2src/coins/utxo/utxo_common/utxo_common_swap.rs)
  extended for dual-secret + funding/payment-split semantics required
  by the V2 protocol.
- This chapter is the **driving spec** under the missing-functionality
  policy: the implementation in `mm2src/coins/utxo/` (script builders,
  helpers, and the four trait impls on `UtxoStandardCoin`) is to be
  produced clean-room from this chapter alone.
