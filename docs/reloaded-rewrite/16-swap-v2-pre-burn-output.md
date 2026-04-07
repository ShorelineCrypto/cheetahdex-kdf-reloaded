# Chapter 16 — Atomic-Swap V2 Pre-Burn Output

> **Status in reloaded:** *gap.* The data-structure layer is in place
> (`DexFee` enum and `DexFeeBurnDestination` in
> [`mm2src/coins/lp_coins_types.rs`](../../mm2src/coins/lp_coins_types.rs);
> `BURN_ADDR_PUBKEY` constants and `NetConfig::burn_enabled() /
> dex_fee_share() / burn_addr_raw_pubkey()` accessors in
> [`mm2src/mm2_net_config/`](../../mm2src/mm2_net_config/)). What is
> missing is the **factory layer** that turns a trade into a
> `DexFee::WithBurn` instance, the **`MmCoin` trait surface** that
> tells the factory whether a given coin should burn, and the
> **V2 UTXO swap-helper branches** that build, validate, and broadcast
> a taker-payment-spend transaction with an explicit burn output.
>
> [Chapter 15](15-swap-v2-utxo-path.md) deferred all `DexFee::WithBurn`
> and `DexFee::NoFee` paths to this chapter and shipped explicit
> `"DexFee variant deferred to ch16 pre-burn batch"` rejections in
> the three taker-payment-spend helpers. This chapter removes those
> rejections.

---

## 16.0 Executive Summary

The pre-burn output is an optional second leg of the V2 atomic-swap
dex-fee delivery: instead of paying 100% of the dex fee to a
fee-collection address, a configurable share is **burned** — either
by sending it to a designated burn address (P2PKH on most coin
families) or by attaching it to an `OP_RETURN` output (KMD-only,
provably unspendable). The split ratio is governed by the
network-level constant `DEX_FEE_SHARE`; when `DEX_FEE_SHARE = 0.75`,
75% of the fee goes to the fee address and 25% is burned.

Adding pre-burn requires:

1. Three `MmCoin` trait methods (`burn_pubkey`, `should_burn_directly`,
   `should_burn_dex_fee`) so the dex-fee factory can ask each coin
   whether and how to burn.
2. Two `DexFee` factory functions (`new_from_taker_coin` and
   `new_with_taker_pubkey`) plus dust-aware split helpers.
3. Three new branches in the V2 UTXO taker-payment-spend helpers
   (`gen_taker_payment_spend_preimage`, `validate_taker_payment_spend_preimage`,
   `sign_and_broadcast_taker_payment_spend`) that handle
   `DexFee::WithBurn` under `SIGHASH_ALL` (vs the existing
   `DexFee::Standard` path under `SIGHASH_SINGLE`).
4. A `NoFee` branch that short-circuits the dex-fee output when the
   taker is the burn account.

EVM and Tendermint variants of pre-burn are out of scope here:

- EVM is covered in [Chapter 17 — V2 EVM swap path](17-swap-v2-evm-path.md).
- Tendermint already implements `WithBurn` in
  [`mm2src/coins/tendermint/tendermint_swap_ops.rs`](../../mm2src/coins/tendermint/tendermint_swap_ops.rs)
  and serves as the reference behavioural pattern (split-output
  bank message with separate burn-recipient).

---

## 16.1 Why this exists

The motivation is twofold:

1. **Deflationary policy** — burning a portion of every dex fee
   removes value from circulating supply, mirroring the proof-of-burn
   convention used by some coin communities.
2. **Atomic delivery** — the burn is part of the same
   taker-payment-spend transaction that delivers the maker payout
   and the fee, so neither party can settle the trade without also
   settling the burn. (V1's "separate dex-fee tx" model could not
   guarantee this.)

The split ratio is *network*-level (not coin-level): a network may
choose to burn 25% of every dex fee, irrespective of which coin pair
trades. Per-coin opt-in is done via two `MmCoin` boolean methods
(see §16.2), so coin families that cannot or should not burn (e.g.
because the chain has no usable burn address, or because the network
operator has not designated one) return `false` and fall back to the
`Standard` single-output path.

---

## 16.2 `MmCoin` trait additions

Three methods on the `MmCoin` trait carry per-coin burn policy:

```rust
/// The compressed public key of the network-designated burn address
/// for this coin family. Empty `Vec` if burn is not configured.
fn burn_pubkey(&self) -> Vec<u8>;

/// True iff the burn portion should be attached as an
/// `OP_RETURN(burn_amount)` output rather than sent to a P2PKH
/// burn address. Currently only KMD returns true here.
fn should_burn_directly(&self) -> bool;

/// True iff this coin participates in the pre-burn split at all.
/// When false, the dex-fee factory always emits `DexFee::Standard`.
fn should_burn_dex_fee(&self) -> bool;
```

### 16.2.1 Per-coin-family default impls

| Coin family       | `burn_pubkey()`                | `should_burn_directly()` | `should_burn_dex_fee()` |
|-------------------|--------------------------------|--------------------------|--------------------------|
| UTXO standard     | `NetConfig::burn_addr_raw_pubkey().to_vec()` if `NetConfig::burn_enabled()` else `vec![]` | `true` for KMD ticker, else `false` | matches `NetConfig::burn_enabled()` and not `should_burn_directly()` |
| UTXO BCH/SLP/QRC20| inherits from UTXO standard    | always `false`           | inherits                 |
| EVM (ETH and ERC-20) | `vec![]` (Phase 3, see ch.17) | `false`                  | `false`                  |
| Tendermint        | already implemented; do not regress | `false`             | already returns `true` when configured |
| Lightning / NFT / SLP-token / Sia / Solana / Z-coin | `vec![]` | `false` | `false` |

The default implementation on the `MmCoin` trait returns the
fall-through values (`vec![]` / `false` / `false`); coin families
that participate in pre-burn override.

### 16.2.2 NetConfig surface (already present)

The trait `NetConfig` in
[`mm2src/mm2_net_config/src/lib.rs`](../../mm2src/mm2_net_config/src/lib.rs)
already exposes:

- `fn burn_enabled(&self) -> bool` — true when the network has a
  configured burn address.
- `fn dex_fee_share(&self) -> MmNumber` — the fraction kept by the
  fee-collection address (e.g. `0.75` for a 75/25 split).
- `fn burn_addr_pubkey(&self) -> &'static str` — hex form for logs.
- `fn burn_addr_raw_pubkey(&self) -> &'static [u8]` — raw 33-byte
  compressed pub-key for output construction.

This chapter does not modify `NetConfig`; the existing surface is
sufficient.

---

## 16.3 `DexFee` factory functions

Two associated factory functions on `DexFee` produce the right
variant for a given trade:

```rust
impl DexFee {
    /// Used at swap initiation, before the taker's pubkey is known.
    /// Decides between `Standard`, `WithBurn{KmdOpReturn}`, and
    /// `WithBurn{PreBurnAccount}` based on the taker coin's burn flags
    /// and dust-tolerance.
    pub fn new_from_taker_coin<T: MmCoin>(
        taker_coin: &T,
        net_cfg: &dyn NetConfig,
        maker_ticker: &str,
        trade_amount: &MmNumber,
    ) -> DexFee;

    /// Used during validation when the taker's pubkey is known.
    /// Returns `NoFee` iff the taker pubkey is the burn pubkey
    /// (the burn address itself is not charged a fee on its own
    /// trades). Otherwise delegates to `new_from_taker_coin`.
    pub fn new_with_taker_pubkey<T: MmCoin>(
        taker_coin: &T,
        net_cfg: &dyn NetConfig,
        maker_ticker: &str,
        trade_amount: &MmNumber,
        taker_pubkey: &[u8],
    ) -> DexFee;
}
```

### 16.3.1 Decision tree

```
new_from_taker_coin(taker, cfg, maker_ticker, trade_amount):
    base_fee = compute_base_fee(trade_amount, taker.ticker(), maker_ticker)
    if not cfg.burn_enabled() or not taker.should_burn_dex_fee():
        return DexFee::Standard(base_fee)
    if taker.should_burn_directly():
        return calc_dex_fee_for_op_return(base_fee, taker.min_tx_amount())
    else:
        return calc_dex_fee_for_burn_account(
            base_fee,
            taker.min_tx_amount(),
            cfg.dex_fee_share(),
            taker.burn_pubkey(),
        )

new_with_taker_pubkey(taker, cfg, maker_ticker, trade_amount, taker_pubkey):
    if taker.burn_pubkey() == taker_pubkey:
        return DexFee::NoFee
    return new_from_taker_coin(taker, cfg, maker_ticker, trade_amount)
```

`compute_base_fee` is the existing fee computation (see
[Chapter 8 — Atomic-Swap Fee-Routing Engine](08-fee-routing-engine.md));
this chapter does not change it.

### 16.3.2 Dust-aware split helpers

```rust
/// KMD path: total fee goes into an OP_RETURN output. The `min_tx_amount`
/// applies only to the *fee* leg; the OP_RETURN leg has zero value
/// and no dust check.
pub fn calc_dex_fee_for_op_return(
    fee: MmNumber,
    min_tx_amount: MmNumber,
) -> DexFee {
    if fee < min_tx_amount {
        // Whole fee would be dust — skip burn metadata, emit Standard.
        return DexFee::Standard(fee);
    }
    DexFee::WithBurn {
        fee_amount: MmNumber::from(0),
        burn_amount: fee,
        burn_destination: DexFeeBurnDestination::KmdOpReturn,
    }
}

/// Non-KMD path: split into two P2PKH outputs.
pub fn calc_dex_fee_for_burn_account(
    fee: MmNumber,
    min_tx_amount: MmNumber,
    fee_share: MmNumber,         // e.g. 0.75
    burn_pubkey: Vec<u8>,
) -> DexFee {
    let fee_part = &fee * &fee_share;
    let burn_part = &fee - &fee_part;
    if burn_part < min_tx_amount || fee_part < min_tx_amount {
        // Either leg would be dust — fall back to Standard.
        return DexFee::Standard(fee);
    }
    DexFee::WithBurn {
        fee_amount: fee_part,
        burn_amount: burn_part,
        burn_destination: DexFeeBurnDestination::PreBurnAccount { burn_pubkey },
    }
}
```

The dust threshold is the coin's `min_tx_amount()` (an existing
`MmCoin` method); this avoids per-network dust hardcoding.

---

## 16.4 SIGHASH strategy for the V2 UTXO taker-payment-spend

`DexFee::Standard` and `DexFee::WithBurn` use **different** sighash
schemes, deliberately:

| Variant     | Outputs at preimage time | Outputs added by maker     | Taker sighash      |
|-------------|--------------------------|----------------------------|--------------------|
| `Standard`  | 1 (maker payout)         | +1 (fee P2PKH)             | `SIGHASH_SINGLE`   |
| `WithBurn`  | 3 (maker, fee, burn)     | 0 — locked at preimage     | `SIGHASH_ALL`      |
| `NoFee`     | 1 (maker payout)         | 0                          | `SIGHASH_ALL`      |

`SIGHASH_SINGLE` lets the maker append the dex-fee output without
invalidating the taker's signature — the taker only commits to its
own input and to output 0 (the maker payout). `SIGHASH_ALL` requires
all outputs to be present and frozen at preimage time, which is
correct for `WithBurn` because the burn output must not be
modifiable by the maker.

This is the central reason `WithBurn` cannot be implemented as
"Standard plus extra output": the signature flag is part of the
preimage, and the validator must check the signature under the
correct flag.

The constant `DEFAULT_SWAP_TX_SPEND_SIZE` (introduced in
[§15.9](15-swap-v2-utxo-path.md#159-constants)) is sized for the
3-output (`WithBurn`) case; the `Standard` case under-uses some
fee-budget bytes and produces a slightly higher per-byte fee than
strictly necessary, which is acceptable.

---

## 16.5 V2 UTXO helper updates

The three taker-payment-spend helpers in
[`mm2src/coins/utxo/utxo_common/utxo_common_swap.rs`](../../mm2src/coins/utxo/utxo_common/utxo_common_swap.rs)
each carry a single `match args.dex_fee { DexFee::Standard => ... ;
_ => return MmError::err(...) }` rejection today. This chapter
replaces the catch-all arm with three explicit arms.

### 16.5.1 `gen_taker_payment_spend_preimage`

For `DexFee::WithBurn { fee_amount, burn_amount, burn_destination }`:

1. Compute `dex_fee_sat = sat_from_big_decimal(fee_amount, decimals)`.
2. Compute `burn_sat = sat_from_big_decimal(burn_amount, decimals)`.
3. Compute `maker_value = taker_output.value - dex_fee_sat -
   burn_sat - htlc_spend_fee`. If this underflows, return
   `TxGenError::PrevOutputTooLow`.
4. Build three `TransactionOutput`s:
   - Output 0: P2PKH(`maker_address`) for `maker_value`.
   - Output 1: P2PKH(`fee_address`) for `dex_fee_sat`. The
     `fee_address` is derived from `NetConfig::dex_fee_addr_raw_pubkey()`
     using the same chain conf as the taker coin (existing helper:
     `dex_fee_standard_output`).
   - Output 2: see §16.5.4.
5. Use sighash `SIGHASH_ALL_BASE | conf.fork_id` (not
   `SIGHASH_SINGLE_BASE`).
6. Sign and package as before.

For `DexFee::NoFee`:

1. Compute `maker_value = taker_output.value - htlc_spend_fee`.
2. Build a single output (P2PKH maker payout).
3. Sighash: `SIGHASH_ALL_BASE | conf.fork_id`.
4. Sign and package.

### 16.5.2 `validate_taker_payment_spend_preimage`

Mirror the preimage construction. For `WithBurn`:

1. Expected output count: `3`.
2. Output 0: maker P2PKH, value within ±10% of expected.
3. Output 1: fee-address P2PKH, value within ±10% of expected
   `dex_fee_sat`.
4. Output 2: see §16.5.4 — its expected shape depends on
   `burn_destination`.
5. Sighash for signature verification: `SIGHASH_ALL_BASE | fork_id`.

For `NoFee`: expected output count `1`, sighash `SIGHASH_ALL`.

The ±10% tolerance is the same fee-budget tolerance used by the
existing `Standard` branch (per
[§15.5.7](15-swap-v2-utxo-path.md#1557-validate_taker_funding_spend_preimage)
/ §15.5.12).

### 16.5.3 `sign_and_broadcast_taker_payment_spend`

For `WithBurn` and `NoFee` the maker does **not** append outputs;
all outputs are already in the preimage. The maker:

1. Re-derives the same outputs (for fee-budget re-check).
2. Signs the input with her HTLC key under `SIGHASH_ALL`.
3. Assembles the cooperative-branch script_sig:
   `[push(taker_sig+sighash_byte), push(maker_sig+sighash_byte),
   push(maker_secret), OP_0, push(redeem_script)]`
   — same shape as the `Standard` path but with the `SIGHASH_ALL`
   byte (`0x01 | fork_id`) on both sigs.
4. Broadcasts.

### 16.5.4 Burn output construction

```rust
fn build_burn_output(
    burn_amount_sat: u64,
    destination: &DexFeeBurnDestination,
    coin_conf: &UtxoCoinConf,
) -> Result<TransactionOutput, TxGenError> {
    match destination {
        DexFeeBurnDestination::KmdOpReturn => Ok(TransactionOutput {
            value: 0,
            script_pubkey: Builder::default()
                .push_opcode(Opcode::OP_RETURN)
                .push_bytes(&burn_amount_sat.to_le_bytes())
                .into_bytes(),
        }),
        DexFeeBurnDestination::PreBurnAccount { burn_pubkey } => {
            let burn_addr = address_from_pubkey(
                &Public::from_slice(burn_pubkey)?,
                coin_conf,
            )?;
            Ok(TransactionOutput {
                value: burn_amount_sat,
                script_pubkey: output_script(&burn_addr, ScriptType::P2PKH).to_bytes(),
            })
        },
    }
}
```

For `KmdOpReturn` the burn value is encoded into the OP_RETURN
payload (8-byte LE) rather than as the output value, because
OP_RETURN outputs are conventionally zero-value. The chain still
records the destruction because the value is locked in the OP_RETURN
script that no one can spend.

For `PreBurnAccount` the value is sent to a normal P2PKH; whether
that address is *truly* unspendable is a key-management property
(the network operator does not retain the private key), not a
script-level guarantee.

### 16.5.5 Removing the rejection arms

The three `_ => return MmError::err("DexFee variant deferred to
ch16 ...")` arms in the helpers are removed. The compile-time
exhaustiveness check ensures all `DexFee` variants are handled.

---

## 16.6 Tests

Add a new `mod swap_v2_pre_burn_tests` next to the existing
`swap_v2_taker_payment_spend_tests` in
[`mm2src/coins/utxo/utxo_tests.rs`](../../mm2src/coins/utxo/utxo_tests.rs):

- `should_compute_dex_fee_with_burn_split_for_burn_enabled_coin`
  — call `DexFee::new_from_taker_coin` with a mock coin returning
  `should_burn_dex_fee = true`, assert variant is
  `WithBurn { burn_destination: PreBurnAccount { .. }, .. }` with
  fee_amount = 75% and burn_amount = 25% of total.
- `should_fall_back_to_standard_when_burn_share_is_dust`
  — same factory call but with a trade amount small enough that
  the 25% burn would be below `min_tx_amount`; assert
  `DexFee::Standard(full)` is returned.
- `should_emit_kmd_op_return_for_should_burn_directly_coin`
  — mock coin with `should_burn_directly = true`; assert
  `WithBurn { burn_destination: KmdOpReturn, .. }`.
- `should_emit_no_fee_when_taker_pubkey_is_burn_pubkey`
  — `DexFee::new_with_taker_pubkey` with `taker_pubkey ==
  burn_pubkey`; assert `NoFee`.
- `should_build_taker_payment_spend_preimage_with_three_outputs_for_with_burn`
  — synthesise `args.dex_fee = WithBurn{..PreBurnAccount}`, call
  `gen_taker_payment_spend_preimage`, assert
  `signer.outputs.len() == 3` and the burn output is P2PKH(burn_addr)
  for `burn_amount_sat`.
- `should_build_taker_payment_spend_preimage_with_op_return_for_kmd_burn`
  — same with `KmdOpReturn`; assert output 2 starts with
  `OP_RETURN` opcode and value is zero.
- `should_recover_partial_signature_from_with_burn_preimage_under_sighash_all`
  — verify the taker partial sig parses and verifies under
  `SIGHASH_ALL_BASE | fork_id` against the cooperative branch.
- `should_reject_with_burn_preimage_with_wrong_burn_value`
  — mutate the burn output's value, call
  `validate_taker_payment_spend_preimage`, assert
  `InvalidPreimage` with a "burn output value" message.

Network-dependent end-to-end tests (actual broadcast on a regtest
chain) belong to `docker_tests`; they are out of scope here.

---

## 16.7 Constants

No new constants. `DEFAULT_SWAP_TX_SPEND_SIZE` is already sized for
the 3-output `WithBurn` case; its current value (305) is the
ch.15 Phase-2 baseline. Re-evaluate if the actual on-chain bytes
exceed the budget plus 10% tolerance — the unit tests above will
flag this.

---

## 16.8 Wire / state-machine integration

The trait surface (`MakerCoinSwapOpsV2`, `TakerCoinSwapOpsV2`,
`GenTakerPaymentSpendArgs.dex_fee: &DexFee`) is unchanged; the
state-machine call sites in
[`mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs)
and
[`mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs)
already pass a `&DexFee` of arbitrary variant. After this chapter,
that variant is `WithBurn` whenever the taker coin opts into burn
(per §16.2.1 and §16.3.1) instead of being forced to `Standard`.

The dex-fee construction call site (where `compute_base_fee` was
called and `DexFee::Standard(base_fee)` was returned unconditionally)
is replaced with `DexFee::new_from_taker_coin(taker_coin, net_cfg,
maker_ticker, trade_amount)`, and at the validation step
`new_with_taker_pubkey` is used.

---

## 16.9 Activation wiring

The default `MmCoin` impls of the three new methods on every coin
that does not override (returning `vec![]` / `false` / `false`)
ensure that activation behaviour for non-burn coins is unchanged.
The UTXO-standard, BCH, SLP, QRC20, Z-coin, Lightning, NFT, Sia,
Solana, Tendermint, EVM, and TRON coin impls each get a one-line
override (or no change, for those returning the defaults) per
§16.2.1.

---

## 16.10 EVM and Tendermint

- **EVM**: the V2 EVM contract call surface accepts a single
  `dex_fee` `Uint256` today and does not split. Adding pre-burn to
  EVM requires the contract ABI to grow a `burn_amount` and
  `burn_address` parameter, which is part of
  [Chapter 17 — V2 EVM swap path](17-swap-v2-evm-path.md). This
  chapter does not modify the EVM helpers; the EVM `MmCoin` impl
  returns `should_burn_dex_fee = false` per §16.2.1, so the
  factory always emits `Standard` for EVM-side dex-fee delivery.
- **Tendermint**: the existing `tendermint_swap_ops.rs` already
  branches on `WithBurn` and routes the burn portion through a
  separate `MsgMultiSendProto` recipient. This chapter does not
  modify Tendermint code; it is the reference behavioural pattern
  that informed the UTXO design above (split-output with explicit
  burn-recipient, single-transaction atomic delivery).

---

## 16.11 Tests entry-point

The new test module is registered in
[`mm2src/coins/utxo/utxo_tests.rs`](../../mm2src/coins/utxo/utxo_tests.rs)
under the existing `mod swap_v2_taker_payment_spend_tests` block.
The mock coin used by the factory tests is a small struct
implementing the three new `MmCoin` methods plus the existing
`min_tx_amount()` and `ticker()` accessors; it does not need to
implement the full `MmCoin` trait — a focused trait alias
(`trait BurnPolicy: MmCoin {}`) keeps the test surface narrow.

---

## 16.12 External references

- Bitcoin script `OP_RETURN`: BIP 11 / standard tx-relay rules.
- 75/25 split convention: network operator constant
  `DEX_FEE_SHARE = 0.75` documented inline in
  [`mm2src/mm2_net_config/src/netid_6133.rs`](../../mm2src/mm2_net_config/src/netid_6133.rs).
- KMD OP_RETURN burn convention: documented behaviour of the KMD
  block-explorer's "burn" classification — provably-unspendable
  outputs.

---

## 16.13 Known chapter rough edges

The implementation surfaced three small clarifications that the
chapter does not pin down precisely. They are recorded here so a
future author can tighten the spec without re-reading the diff:

1. **Coin-level vs network-level burn opt-in.** §16.2.1's table reads
   `should_burn_dex_fee() = NetConfig::burn_enabled()` for UTXO
   standard. In practice, `UtxoStandardCoin` does not hold a
   `NetConfig` handle, so the two-layer split is: the *coin* returns
   an unconditional `true` (opt-in at the coin layer), and the
   network gate is enforced inside `DexFee::new_from_taker_coin` by
   calling `net_cfg.burn_enabled()`. End-to-end behaviour matches
   the spec on both burn-enabled (e.g. netid 6133) and burn-disabled
   (e.g. netid 8762) networks.

2. **Burn pubkey fallback.** §16.2.1 implies the coin owns its own
   `burn_pubkey()`. UTXO standard does not have one without a
   `NetConfig` handle, so the factory falls back to
   `net_cfg.burn_addr_raw_pubkey()` when the coin's `burn_pubkey()`
   returns empty. Coin families that *do* know their burn pubkey
   directly (e.g. a hypothetical chain with a baked-in burn address
   in its conf JSON) can override `burn_pubkey()` and the factory
   will prefer the coin's value.

3. **"Maker appends" branch is `Standard`-only.** §16.5.3 says the
   maker re-signs under `SIGHASH_ALL`. The implementation builds
   the full output set in the *preimage* for `WithBurn` and `NoFee`,
   so the maker simply re-signs the existing outputs without
   appending — only the legacy `Standard` (`SIGHASH_SINGLE`) branch
   appends a fee output at the maker side. This is consistent with
   §16.5.1 (which enumerates three outputs in the preimage for
   `WithBurn`) but the "append" verb in §16.5.3 should be read as
   "append-or-keep, depending on variant".

4. **Factory base-fee boundary.** §16.3 sketches
   `DexFee::new_from_taker_coin(taker_coin, net_cfg, maker_ticker,
   trade_amount)` as if the factory computes the base fee
   internally via `compute_base_fee`. The implementation instead
   takes the already-computed base fee as a parameter:
   `DexFee::new_from_taker_coin(taker_coin, net_cfg, base_fee:
   MmNumber)`. The base-fee computation is centralised at the
   call site in
   [`mm2src/mm2_main/src/lp_swap/dex_fee.rs`](../../mm2src/mm2_main/src/lp_swap/dex_fee.rs)
   so a single coding path computes it; the factory's job is only
   to decide split vs no-split. The decision tree in §16.3.1 is
   unchanged in either form.

These are stylistic / wording polish; behavioural correctness is
covered by the §16.6 tests.

---

## 16.14 Provenance

- Type definitions (`DexFee`, `DexFeeBurnDestination`, accessors)
  predate the V2 swap work in the GPLv2 baseline at commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; reloaded ships them
  unchanged.
- `NetConfig::burn_enabled() / dex_fee_share() / burn_addr_raw_pubkey()`
  were added in reloaded as part of the network-id-decoupling
  work documented in
  [Chapter 6 — Network-ID & Seed-Node Decoupling](06-network-id-seed-node.md).
- The split-output sighash strategy (`SIGHASH_SINGLE` for `Standard`,
  `SIGHASH_ALL` for `WithBurn`) is derived from the V2 UTXO
  preimage exchange documented in
  [§15.5.11 — §15.5.13](15-swap-v2-utxo-path.md#15511-gen_taker_payment_spend_preimage).
- The OP_RETURN-with-LE-amount encoding for `KmdOpReturn` is the
  smallest representation that records the burned value on-chain
  while keeping the output value at zero (so it does not pull
  spendable satoshis out of the taker's input).
