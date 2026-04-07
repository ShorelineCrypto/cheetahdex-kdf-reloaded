# Chapter 08 — Atomic-Swap Fee-Routing Engine

## Executive Summary

In the baseline tree the taker fee on every atomic swap is a single amount
sent to a single address. Three short helpers in `lp_swap.rs` —
`dex_fee_threshold`, `dex_fee_rate`, `dex_fee_amount` — compute the number
from per-process hard-coded constants (one rate, one discount list, one floor).
Two coin-trait methods carry the fee through the swap: `send_taker_fee(fee_addr,
amount, uuid)` produces a single-output transaction, and `validate_fee` accepts
a flat parameter list ending in a single `amount`.

The post-baseline tree generalises the fee path along three orthogonal axes
without touching the on-chain HTLC protocol itself:

1. **A typed fee descriptor.** A new `DexFee` enum (`NoFee` / `Standard` /
   `WithBurn`) and an accompanying `DexFeeBurnDestination` enum replace the
   bare `MmNumber` / `BigDecimal` parameter at every boundary. Helper methods
   on `DexFee` (`total_spend_amount`, `fee_amount`, `burn_amount`) keep the
   per-component arithmetic in one place.

2. **A network-parameter source of truth.** All numerics (rate, discounted
   rate, discount-eligible tickers, floor, burn share) are moved out of the
   swap module into the `NetConfig` trait introduced in chapter 06. The fee
   module is a pure arithmetic consumer; adding a new network is a
   `NetConfig`-side concern.

3. **A struct-arguments boundary.** `validate_fee` now takes a single
   `ValidateFeeArgs<'a>` struct rather than a six-parameter positional list,
   which both removes call-site ambiguity (the `fee_addr`/`expected_sender`
   pair used to be easy to swap by accident) and gives the parameter
   `dex_fee: &DexFee` a place to live alongside the existing fields.

This chapter documents the shape of the new fee descriptor, the
arithmetic-only `compute_dex_fee` function that produces it, the trait-method
signature changes, and the safety fallbacks that protect callers from
degenerate split inputs. It does not document the specific numeric values used
on any production network — those are network-policy, not fee-engine, and live
in the per-netid configuration modules described in chapter 06.

## Reproduction Detail

### 8.1 Baseline shape

At commit `c1d46c0…` the fee engine consists of three private/public helpers
in `mm2_main/src/lp_swap.rs`:

```rust
fn dex_fee_threshold(min_tx_amount: MmNumber) -> MmNumber { … }   // private
fn dex_fee_rate(base: &str, rel: &str) -> MmNumber { … }          // private
pub fn dex_fee_amount(
    base: &str, rel: &str, trade_amount: &MmNumber, dex_fee_threshold: &MmNumber,
) -> MmNumber { … }
pub fn dex_fee_amount_from_taker_coin(
    taker_coin: &MmCoinEnum, maker_coin: &str, trade_amount: &MmNumber,
) -> MmNumber { … }
```

All numerics are inline in `dex_fee_threshold` (a single fraction literal) and
`dex_fee_rate` (two `BigRational::new(...)` literals plus a one-element
discount-ticker slice). Tests in the same file feed
`MmNumber::from("0.0001")` to verify the floor behaviour.

`SwapOps::send_taker_fee` is

```rust
fn send_taker_fee(&self, fee_addr: &[u8], amount: BigDecimal, uuid: &[u8])
    -> TransactionFut;
```

`SwapOps::validate_fee` is

```rust
fn validate_fee(
    &self,
    fee_tx: &TransactionEnum,
    expected_sender: &[u8],
    fee_addr: &[u8],
    amount: &BigDecimal,
    min_block_number: u64,
    uuid: &[u8],
) -> Box<dyn Future<Item = (), Error = String> + Send>;
```

There is no `DexFee` type, no concept of a burn output, and no struct
arguments anywhere in the fee path. The taker fee is one transaction with one
output to one address.

### 8.2 New module layout

A new module `mm2_main/src/lp_swap/dex_fee.rs` replaces the in-file helpers.
It is pure arithmetic: it produces a `DexFee` value but does not resolve a
destination address. Address resolution remains the coin layer's
responsibility (UTXO coins assemble the fee transaction, EVM coins encode the
ERC-20 transfer, Tendermint coins build the bank send, and so on).

Two new public types live in `mm2src/coins/lp_coins_types.rs`:

```rust
pub enum DexFeeBurnDestination {
    KmdOpReturn,                                    // KMD-only: OP_RETURN
    PreBurnAccount { burn_pubkey: Vec<u8> },        // non-KMD: address derived
                                                    //          from a pubkey
}

pub enum DexFee {
    NoFee,
    Standard(MmNumber),
    WithBurn {
        fee_amount:       MmNumber,
        burn_amount:      MmNumber,
        burn_destination: DexFeeBurnDestination,
    },
}
```

`DexFee` exposes three accessors:

| Method | Returns |
| --- | --- |
| `total_spend_amount()` | `fee_amount + burn_amount` for `WithBurn`; the value for `Standard`; `0` for `NoFee` |
| `fee_amount()` | the routed-to-fee-address portion |
| `burn_amount()` | the destroyed portion (`0` for `Standard`/`NoFee`) |

It also implements `Display`, which is the only thing that ever observes the
internal `(fee, burn)` decomposition outside the coin layer.

### 8.3 `compute_dex_fee` — the arithmetic core

`compute_dex_fee(net_cfg, taker_coin, maker_coin, trade_amount) -> DexFee` is
the only public producer of `DexFee` values for normal swaps. It runs the
following pipeline:

1. **Total.** `total = dex_fee_amount_from_taker_coin(net_cfg, taker_coin,
   maker_coin, trade_amount)`. That helper, in turn, reads
   `NetConfig::dex_fee_rate()` / `dex_fee_rate_discounted()` /
   `fee_discount_tickers()` for the multiplier, and `dex_fee_min_threshold()`
   for the floor.
2. **Single-output branch.** If `NetConfig::burn_enabled()` returns `false`,
   return `DexFee::Standard(total)` immediately. Networks that do not
   participate in a burn-split scheme stop here.
3. **Split.** Otherwise compute `share = NetConfig::dex_fee_share()`,
   `fee_amount = total * share`, `burn_amount = total - fee_amount`.
4. **Safety fallback A — zero/negative burn.** If `burn_amount <= 0` (which
   can happen if `share == 1` or as a rounding edge case), drop the split:
   return `DexFee::Standard(total)`.
5. **Safety fallback B — under-dust portion.** Let `dust =
   taker_coin.min_tx_amount()`. If either `fee_amount < dust` or
   `burn_amount < dust`, drop the split: return `DexFee::Standard(total)`.
   This avoids constructing a fee transaction one of whose outputs the coin
   layer would refuse to broadcast.
6. **Burn destination.** If the taker coin's ticker equals the literal
   `"KMD"`, use `DexFeeBurnDestination::KmdOpReturn`; otherwise use
   `PreBurnAccount { burn_pubkey: NetConfig::burn_addr_raw_pubkey().to_vec() }`.
7. **Return.** `DexFee::WithBurn { fee_amount, burn_amount, burn_destination }`.

The split versus single-output decision and all of its inputs are made at this
single call site. Coin-side code never re-derives the split.

### 8.4 `ValidateFeeArgs` — struct boundary on `validate_fee`

`SwapOps::validate_fee` becomes

```rust
fn validate_fee(&self, args: ValidateFeeArgs<'_>)
    -> Box<dyn Future<Item = (), Error = String> + Send>;
```

with

```rust
pub struct ValidateFeeArgs<'a> {
    pub fee_tx:          &'a TransactionEnum,
    pub expected_sender: &'a [u8],
    pub fee_addr:        &'a [u8],
    pub dex_fee:         &'a DexFee,
    pub min_block_number: u64,
    pub uuid:            &'a [u8],
}
```

Two effects:

- A coin implementor receiving `args.dex_fee` can branch on `Standard` vs
  `WithBurn` and validate the correct number of outputs (one or two) and the
  correct distribution of values, without an out-of-band parallel parameter.
- The argument-positional confusion possible at the baseline call site
  (`expected_sender: &[u8]` and `fee_addr: &[u8]` are the same type) is
  removed: each field is now named.

`SwapOps::send_taker_fee` is updated to

```rust
fn send_taker_fee(&self, dex_fee: &DexFee, fee_addr: &[u8], uuid: &[u8])
    -> TransactionFut;
```

The coin implementation reads `dex_fee.total_spend_amount()` to find the input
side of the transaction, then reads `dex_fee.fee_amount()` and (for `WithBurn`)
`dex_fee.burn_amount()` plus `burn_destination` to assemble the outputs.

### 8.5 Where the per-network numerics live

| Numeric | Source |
| --- | --- |
| Base rate (taker_amount → fee multiplier) | `NetConfig::dex_fee_rate()` |
| Discounted rate (when either side is in the discount list) | `NetConfig::dex_fee_rate_discounted()` |
| Discount-eligible ticker list | `NetConfig::fee_discount_tickers()` |
| Minimum fee floor (lower bound applied after multiplication) | `NetConfig::dex_fee_min_threshold()` |
| Burn enabled? | `NetConfig::burn_enabled()` (defaults to `false`) |
| Share to fee address (the rest is burned) | `NetConfig::dex_fee_share()` (defaults to `1`) |
| Public key of the burn address | `NetConfig::burn_addr_raw_pubkey()` |

None of these values appear in the fee module. A network whose burn policy
should change at runtime (rate, share, threshold, address) is changed in
`mm2_net_config`; no fee-engine recompile semantics change.

### 8.6 Coin-side validation contract

The change at the coin layer is that each coin's `validate_fee` and
`send_taker_fee` implementation must handle two structurally distinct cases:

- **`DexFee::Standard(amount)`**: the fee tx contains exactly one output to
  the address derived from `fee_addr`, in `amount`.
- **`DexFee::WithBurn { fee_amount, burn_amount, burn_destination }`**: the
  fee tx contains
  - one output to the address derived from `fee_addr`, in `fee_amount`, **and**
  - one further output of `burn_amount` going to the destination dictated by
    `burn_destination` (a literal `OP_RETURN` for KMD; an address derived from
    `burn_pubkey` for everything else).

A `DexFee::NoFee` value short-circuits both `send_taker_fee` and
`validate_fee` to a no-op.

The structural validation rules (output count, addresses match, values match)
are the coin layer's responsibility; `compute_dex_fee` only guarantees that
the numbers it returns are non-degenerate (no zero outputs, no under-dust
outputs) — when those guarantees would not hold, it returns `Standard`
instead.

### 8.7 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. Create `mm2src/mm2_main/src/lp_swap/dex_fee.rs`. Re-export it from
   `lp_swap.rs` (`pub mod dex_fee; pub use dex_fee::*;`).
2. In `mm2src/coins/lp_coins_types.rs`, define `DexFeeBurnDestination`
   (`KmdOpReturn` and `PreBurnAccount { burn_pubkey: Vec<u8> }`) and `DexFee`
   (`NoFee`, `Standard(MmNumber)`, `WithBurn { fee_amount, burn_amount,
   burn_destination }`). Derive `Clone`, `Debug`, `PartialEq`. Implement the
   three accessors and `Display` from §8.2 / §8.3.
3. Re-export `DexFee` and `DexFeeBurnDestination` from `coins/lp_coins.rs` so
   the swap layer can name them without depth-3 paths.
4. Move the baseline `dex_fee_threshold` / `dex_fee_rate` / `dex_fee_amount`
   / `dex_fee_amount_from_taker_coin` helpers from `lp_swap.rs` into the new
   `dex_fee.rs`. Add `net_cfg: &dyn NetConfig` as their leading parameter and
   replace every fraction/slice literal with the corresponding `NetConfig`
   accessor (table in §8.5). Make the threshold and rate helpers
   `pub(crate)`; keep the amount helpers `pub`.
5. Define `pub fn compute_dex_fee(net_cfg: &dyn NetConfig, taker_coin:
   &MmCoinEnum, maker_coin: &str, trade_amount: &MmNumber) -> DexFee`
   implementing the pipeline in §8.3 — including both safety fallbacks.
6. In `mm2src/coins/lp_coins_traits.rs` (or wherever `SwapOps` lives in your
   tree), define `ValidateFeeArgs<'a>` with the six fields listed in §8.4.
   Change `SwapOps::validate_fee` to take a single `ValidateFeeArgs<'_>`.
   Change `SwapOps::send_taker_fee` to take `dex_fee: &DexFee` in place of the
   bare amount.
7. Update every `impl SwapOps for …` block accordingly. UTXO, EVM, Tendermint
   and any other coin family will each need to read `args.dex_fee` and
   branch on `Standard` versus `WithBurn` to emit / verify the right number
   of outputs.
8. In the taker swap (`mm2_main/src/lp_swap/taker_swap.rs`), replace the bare
   `dex_fee_amount_from_taker_coin(...)` call with `compute_dex_fee(...)` at
   the site that constructs the fee, and thread the resulting `DexFee` down
   into `send_taker_fee` and `validate_fee`.
9. Run the baseline `test_dex_fee_amount` (porting it to take `&dyn
   NetConfig`) to confirm the floor / above-floor arithmetic still matches
   expectations against your chosen `NetConfig`.
10. Add new tests for `compute_dex_fee`:
    - `burn_enabled = false` → always `Standard`.
    - `burn_enabled = true, share = 1` → `Standard` via fallback A.
    - `burn_enabled = true, share < 1, total << dust` → `Standard` via
      fallback B.
    - `burn_enabled = true, share < 1, total >> dust, ticker = "KMD"` →
      `WithBurn { …, burn_destination: KmdOpReturn }`.
    - `burn_enabled = true, share < 1, total >> dust, ticker != "KMD"` →
      `WithBurn { …, burn_destination: PreBurnAccount { burn_pubkey } }`.

## External References

- *Atomic swap*, Wikipedia overview of HTLC-based cross-chain atomic swaps,
  <https://en.wikipedia.org/wiki/Atomic_swap>.
- `OP_RETURN` semantics in Bitcoin Script, *Bitcoin Wiki*,
  <https://en.bitcoin.it/wiki/Script#Provably_Unspendable/PrunableOutputs>.
- Bitcoin Core dust-threshold rationale, *Bitcoin Core PR description and
  policy header* (`policy/policy.h`), <https://github.com/bitcoin/bitcoin>.
  The "no output below dust" rule referenced in §8.3 fallback B is a relay
  policy in Bitcoin Core and analogous chains.
- ERC-20 token-transfer semantics (`transfer(address,uint256)`), *Ethereum
  Improvement Proposals*, <https://eips.ethereum.org/EIPS/eip-20>. Relevant to
  EVM-side fee delivery referenced in §8.6.
- *Cosmos SDK x/bank* documentation, <https://docs.cosmos.network/main/modules/bank>.
  Relevant to Tendermint-side fee delivery.
- `num-rational` crate, <https://crates.io/crates/num-rational>, used as the
  underlying rational type for `MmNumber`.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline `mm2_main/src/lp_swap.rs`
  and `coins/lp_coins.rs` at commit `c1d46c0…`; the post-baseline files
  `mm2_main/src/lp_swap/dex_fee.rs`, `coins/lp_coins_types.rs`,
  `coins/lp_coins_traits.rs`, the per-coin `*_swap.rs` modules that consume
  `DexFee`; chapter 06 (`NetConfig`); the external references listed above.
- **Permitted-input classes used:** baseline source; first-party post-baseline
  identifiers introduced with in-chapter justification; public protocol
  documentation (atomic swap, OP_RETURN, ERC-20, Cosmos SDK bank); a public
  Rust crate (`num-rational`).
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree (no kdf-analysis-2022 access).
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
