# Chapter 08 — Atomic-Swap Fee-Routing Engine

**Status:** driving-spec.

This chapter binds the typed fee-descriptor substrate and the arithmetic-only
fee-computation function that produces it, along with the two adjusted swap
trait-method signatures (`send_taker_fee` and `validate_fee`) that carry the
new descriptor through the swap.

## 8.1 Executive Summary

In the baseline tree the taker fee on every atomic swap is a single amount
sent to a single address. Three short helpers in the swap module compute the
number from per-process hard-coded constants (one rate, one discount list,
one floor). Two swap-trait methods carry the fee through the protocol:
`send_taker_fee` produces a single-output transaction, and `validate_fee`
accepts a flat parameter list ending in a single bare amount.

This chapter binds a generalisation of the fee path along three orthogonal
axes that do *not* change the on-chain HTLC protocol:

- **A typed fee descriptor.** A three-variant `DexFee` enum
  (`NoFee` / `Standard` / `WithBurn`) plus a companion
  `DexFeeBurnDestination` enum replace the bare numeric parameter at every
  boundary. Per-component accessors (`total_spend_amount`, `fee_amount`,
  `burn_amount`) keep the arithmetic in one place.
- **A network-parameter source of truth.** All numerics — base rate,
  discounted rate, discount-eligible tickers, floor, burn-enabled flag,
  fee/burn share, burn-pubkey — flow exclusively through the network-config
  accessor surface bound in Chapter 06. The fee module is a pure arithmetic
  consumer.
- **A struct-arguments boundary.** `validate_fee` takes a single
  `ValidateFeeArgs<'_>` value rather than a positional list, eliminating a
  same-type-confusion class (`expected_sender` vs `fee_addr`) and giving the
  new `dex_fee` field a named place to live.

The chapter is intentionally silent on every specific numeric: rate, share,
floor, burn destination, etc. are network policy bound in Chapter 06, not
substrate.

## 8.2 Subsystem Shape

The fee substrate is a one-way pipeline. The arithmetic core consumes a
network-config handle, a taker-coin handle, a maker-coin ticker and a trade
amount; it produces a `DexFee` value. From there the descriptor flows
unchanged through the swap state machine to the two coin-trait methods that
actually touch chain.

The arithmetic core does not resolve any destination address. Per-coin layers
are responsible for deriving the fee-address output, the burn-address output
(for `WithBurn`), and any chain-specific encoding (script, ERC-20 transfer,
bank send). The arithmetic core is purely numeric and contributes only the
shape (`Standard` vs `WithBurn`) and the per-component values.

Two safety fallbacks live inside the core (R10–R11 below). Their purpose is
to guarantee that no `WithBurn` descriptor ever crosses the substrate
boundary with a zero, negative, or below-dust component — when those
guarantees would not hold, the core degrades to `Standard` for the entire
total.

## 8.3 Bound Type Surface

**R1.** The substrate exposes a public enum named `DexFee` with exactly
three variants: `NoFee`, `Standard(MmNumber)`, and
`WithBurn { fee_amount: MmNumber, burn_amount: MmNumber, burn_destination:
DexFeeBurnDestination }`. The variant set and field set are bound: clients
are entitled to exhaustive `match` against this shape.

**R2.** The substrate exposes a public enum named `DexFeeBurnDestination`
with exactly two variants:

- `KmdOpReturn` — payload-free, signalling that the burn output is encoded
  as a Bitcoin-style `OP_RETURN` output.
- `PreBurnAccount { burn_pubkey: Vec<u8> }` — carrying the raw public-key
  bytes the coin layer derives the destination address from.

**R3.** `DexFee` exposes three bound accessors with bound semantics:

| Accessor               | `NoFee`        | `Standard(a)` | `WithBurn { fee, burn, .. }` |
| ---------------------- | -------------- | ------------- | ---------------------------- |
| `total_spend_amount()` | zero           | `a`           | `fee + burn`                 |
| `fee_amount()`         | zero           | `a`           | `fee`                        |
| `burn_amount()`        | zero           | zero          | `burn`                       |

Per-component arithmetic outside these three accessors is forbidden in the
substrate; callers MUST NOT recompute `fee + burn` themselves at any other
site.

**R4.** `DexFee` implements `Display`. `Display` is the *only* surface
outside the coin layer that is permitted to observe the
`(fee_amount, burn_amount)` decomposition for diagnostic / logging purposes.

## 8.4 Bound Arithmetic Pipeline

**R5.** A single public function `compute_dex_fee` is the only producer of
`DexFee` values for normal taker-fee construction. Its parameter list is
bound to four items: a network-config handle (Chapter 06), a taker-coin
handle, a maker-coin ticker, and a trade amount. Its return type is `DexFee`.

**R6.** The pipeline computes the *total* fee by reading the network-config
multiplier (base rate or discounted rate depending on whether either side's
ticker is in the discount-eligible list) and applying the network-config
floor. The arithmetic is bound to be deterministic and side-effect-free.

**R7.** When the network-config burn-enabled flag is *false*, the pipeline
MUST return `DexFee::Standard(total)` and MUST NOT consult the share, the
burn destination, or the dust threshold. Networks that do not participate in
a burn-split scheme stop here.

**R8.** When the network-config burn-enabled flag is *true*, the pipeline
computes `fee_amount = total * share` and `burn_amount = total - fee_amount`,
where `share` is read from the network-config share accessor.

**R9.** The burn destination is bound by taker-coin ticker:

- taker-coin ticker equal to the literal `"KMD"` (case-sensitive) →
  `DexFeeBurnDestination::KmdOpReturn`.
- any other ticker → `DexFeeBurnDestination::PreBurnAccount { burn_pubkey:
  <bytes read from the network-config burn-pubkey accessor> }`.

**R10.** *Safety fallback A — non-positive burn share.* If `burn_amount` is
zero or negative (which can arise when `share` rounds to 1 against the
operating numeric precision), the pipeline MUST return
`DexFee::Standard(total)` and MUST NOT emit a `WithBurn` descriptor with a
zero or negative second component.

**R11.** *Safety fallback B — under-dust component.* Let `dust` be the
taker coin's minimum-transferable-amount accessor. If either `fee_amount`
or `burn_amount` is strictly less than `dust`, the pipeline MUST return
`DexFee::Standard(total)`. The substrate MUST NEVER produce a `WithBurn`
descriptor whose components a coin layer would refuse to broadcast.

**R12.** The `Standard` versus `WithBurn` decision MUST be taken at the
single `compute_dex_fee` site. Coin-layer code MUST NOT re-derive or
re-split the descriptor after the fact.

## 8.5 Bound Trait-Surface Change

**R13.** The swap-side trait method that builds the taker fee transaction
is bound to the signature shape

```text
send_taker_fee(dex_fee: &DexFee, fee_addr: &[u8], uuid: &[u8]) -> TransactionFut
```

The bare-amount parameter present in the baseline is removed; the
`DexFee` reference replaces it.

**R14.** The swap-side trait method that validates a peer-built taker fee
transaction is bound to take a single struct argument:

```text
validate_fee(args: ValidateFeeArgs<'_>) -> <future of unit / error>
```

with `ValidateFeeArgs<'a>` bound to exactly six named fields:

- `fee_tx: &'a TransactionEnum`
- `expected_sender: &'a [u8]`
- `fee_addr: &'a [u8]`
- `dex_fee: &'a DexFee`
- `min_block_number: u64`
- `uuid: &'a [u8]`

Renaming, reordering by position-significance, or collapsing
`expected_sender` and `fee_addr` is forbidden — the struct-arguments
boundary exists specifically to make the two byte-slice fields
non-confusable.

**R15.** Coin-layer implementations of these two methods MUST exhaustively
handle all three `DexFee` variants:

- `DexFee::NoFee` MUST short-circuit both methods to a no-op success.
- `DexFee::Standard(amount)` MUST produce / validate a fee transaction with
  exactly one output to the fee address in `amount`.
- `DexFee::WithBurn { fee_amount, burn_amount, burn_destination }` MUST
  produce / validate a fee transaction with two outputs: one to the fee
  address in `fee_amount`, and one to the destination dictated by
  `burn_destination` in `burn_amount` (`OP_RETURN`-style burn for
  `KmdOpReturn`; address derived from `burn_pubkey` for `PreBurnAccount`).

Structural validation — output count match, address match, value match — is
the coin layer's responsibility; the substrate guarantees only that the
numbers returned by `compute_dex_fee` are non-degenerate per R10–R11.

## 8.6 Bound Network-Parameter Surface (cross-link to Chapter 06)

**R16.** The fee substrate MUST source the following parameters
exclusively from the network-config accessor surface bound in Chapter 06,
and MUST NOT define any of them as compile-time constants:

| Bound semantic                                  | Network-config accessor                  |
| ----------------------------------------------- | ---------------------------------------- |
| Base rate (trade amount → fee multiplier)       | base-rate accessor                       |
| Discounted rate (when discount list applies)    | discounted-rate accessor                 |
| Discount-eligible ticker list                   | discount-ticker-list accessor            |
| Minimum fee floor                               | min-threshold accessor                   |
| Burn-enabled flag                               | burn-enabled accessor (default false)    |
| Fee/total share (remainder is burned)           | share accessor (default one)             |
| Burn-destination raw public-key bytes           | burn-pubkey accessor                     |

**R17.** A new network is added by extending the network-config
implementation only. No fee-substrate code change is permitted to add or
adjust a network's numeric policy.

## 8.7 Tests (test invariants)

**T1.** *Floor.* For a trade amount that, after multiplication by the
network-config base rate, falls strictly below the network-config minimum
threshold, `compute_dex_fee` MUST return a descriptor whose
`total_spend_amount()` equals exactly the minimum threshold.

**T2.** *Burn-disabled passthrough.* With the network-config burn-enabled
flag set false, `compute_dex_fee` MUST return `Standard(_)` for every
non-zero trade amount; the share, burn-pubkey and dust accessors MUST NOT be
consulted (verifiable with a tracking mock).

**T3.** *Safety fallback A.* With burn-enabled true and a share value that
rounds to one against the operating numeric precision, `compute_dex_fee`
MUST return `Standard(total)`. The returned value MUST NOT be a `WithBurn`
descriptor with a zero or negative burn component.

**T4.** *Safety fallback B.* With burn-enabled true, a share strictly less
than one, and a trade amount small enough that either component computed by
R8 falls below the taker coin's dust accessor, `compute_dex_fee` MUST return
`Standard(total)`.

**T5.** *Burn destination by ticker.* With burn-enabled true, a share
strictly less than one, and a trade amount large enough that both R10 and
R11 are satisfied: for a taker-coin ticker `"KMD"`, the returned descriptor
MUST be `WithBurn { burn_destination: KmdOpReturn, .. }`. For any other
ticker, the returned descriptor MUST be
`WithBurn { burn_destination: PreBurnAccount { burn_pubkey: P }, .. }`
where `P` equals the bytes returned by the network-config burn-pubkey
accessor.

**T6.** *No-arithmetic-outside-accessors guard (linter or audit).* A
substrate-internal audit (test or lint) MUST confirm that no call site
outside the accessor implementations performs the `fee_amount + burn_amount`
addition on a `DexFee` value. This protects R3.

## 8.8 Deferred Work

**D1.** A configurable per-pair share (rather than a single network-wide
share) is deferred. The current substrate routes a single share through
the network-config accessor.

**D2.** An ERC-20-side burn that is not a transfer-to-pubkey (for example,
an explicit `burn(uint256)` extension on a token that supports it) is
deferred. The current substrate models burn exclusively as a
destination-address output.

**D3.** A separate "no-burn-for-this-pair" override is deferred; the
substrate's only no-burn modes are the network-wide flag (R7) and the
`NoFee` variant.

**D4.** Per-coin dust accessors that are state-dependent (for example,
varying with fee-rate estimation) are deferred; R11 reads a single
deterministic dust value per coin per evaluation.

## 8.9 External References

- *Atomic swap* — HTLC-based cross-chain atomic-swap overview.
- *Bitcoin Script `OP_RETURN`* — semantics of provably-unspendable outputs
  used for the `KmdOpReturn` burn destination.
- *Bitcoin Core dust-threshold policy* — relay-policy rationale informing
  the under-dust safety fallback bound in R11.
- *ERC-20 token-transfer semantics* (EIP-20) — relevant to coin-layer fee
  delivery on EVM coins under R15.
- *Cosmos SDK `x/bank`* — relevant to coin-layer fee delivery on Tendermint
  coins under R15.
- Chapter 06 (Network identifier and parameter substrate) — bound source
  of every numeric input to R6–R11 and the burn destination's pubkey bytes.
- Chapter 04 (error-aggregation type adaptation) — bound shape of the
  errors any future fallibility extension to `compute_dex_fee` would use.

## 8.10 Baseline Verifications

**V1.** The baseline tree MUST be confirmed to define exactly the
three pre-substrate helpers (`dex_fee_threshold`, `dex_fee_rate`,
`dex_fee_amount`) inside the monolithic swap module, with no dedicated
fee submodule:

```
git -C <baseline> grep -nE 'fn (dex_fee_threshold|dex_fee_rate|dex_fee_amount)\b'
```

**V2.** The baseline tree MUST be confirmed to lack any `DexFee`,
`DexFeeBurnDestination`, or `ValidateFeeArgs` type:

```
git -C <baseline> grep -nE '\b(DexFee|DexFeeBurnDestination|ValidateFeeArgs)\b'
```

**V3.** The baseline `validate_fee` and `send_taker_fee` swap-trait methods
MUST be confirmed to use positional bare-amount parameters as described in
the executive summary — i.e. the baseline shape this substrate replaces is
the documented one:

```
git -C <baseline> grep -nE 'fn (validate_fee|send_taker_fee)\b'
```

## 8.11 Provenance Footer

- *Inputs consulted for this chapter:* the baseline tree at project
  baseline commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`, Chapter 04
  (error envelope), Chapter 06 (network-id and parameter substrate), and
  the external specifications listed in §8.9.
- *Permitted-input classes used:* baseline source; chapter-bound type
  identifiers introduced here as substrate-contract surface (`DexFee`,
  `DexFeeBurnDestination`, `ValidateFeeArgs`, `compute_dex_fee`,
  `send_taker_fee`, `validate_fee`); standard chain-protocol terminology
  (`OP_RETURN`, ERC-20, Cosmos SDK `x/bank`).
- *Sibling chapters cross-referenced:* Chapter 04, Chapter 06.
- *Author of this chapter:* clean-room round-2 driving-spec working set.
- *Forbidden corpus:* not consulted.
