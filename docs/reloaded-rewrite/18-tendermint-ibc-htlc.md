# Chapter 18 — Tendermint, IBC, and Cross-Chain HTLC Surfaces

<!-- AUDIT-FLAG: H-CH18-001 Front-matter status block uses wrong shape and 'document existing + framed gaps' voice. Replace with 'Status: driving-spec' per public rules §6.1 and rewrite the descriptive paragraph as a one-sentence chapter claim. -->
> **Status in reloaded:** the core Tendermint coin (`TendermintCoin`),
> CW20-style token support (`TendermintToken`), HTLC operations
> (Iris and Nucleus dialects), IBC transfer wire types
> (`MsgTransfer`), and the V1 atomic-swap surface (`SwapOps`
> implementation including `DexFee::WithBurn` support) are all
> present and complete. What is *missing* is the activation RPC
> subsystem (the `task::enable_tendermint::*` family), the IBC
> transfer RPC handler, balance-event streaming, and Tx-history
> indexing — those four sub-features are framed here as known gaps
> and tracked for Phase 3.
>
> **Chapter type:** document existing + framed gaps. No IMPL marker.
<!-- AUDIT-FLAG-END: H-CH18-001 -->

---

## 18.0 Executive Summary

<!-- AUDIT-FLAG: H-CH18-002 Direct admission of upstream carry-forward + diff-against-upstream framing. Rewrite as option-B driving spec: state the design produced by Cosmos SDK + ICS-20 + irismod proto + bech32 inputs; do not narrate provenance. Baseline-absence claim moves to a new Baseline Verifications section with a git ls-tree c1d46c0 -- mm2src/coins/tendermint verification. -->
The entire Tendermint family is a **post-baseline addition** to the
reloaded tree — the GPLv2 baseline at commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f` contained no Tendermint
or IBC code. Reloaded carries the post-baseline implementation
forward as-is for HTLC, swap, and IBC wire types; the missing
activation and RPC surfaces are explicitly enumerated in §18.7.
<!-- AUDIT-FLAG-END: H-CH18-002 -->

The chapter is structured as:

| Section | Topic |
|---------|-------|
| §18.1   | Tendermint chain landscape and dialect split (Iris vs Nucleus) |
| §18.2   | `TendermintCoin` and `TendermintToken` types |
| §18.3   | HTLC protocol — wire types, ABCI query paths, lifecycle |
| §18.4   | IBC transfer (`MsgTransfer`) wire surface |
| §18.5   | V1 `SwapOps` impl — payment, validation, secret extraction |
| §18.6   | Multi-denom and CW20-style token support |
| §18.7   | Known gaps (activation RPC, IBC RPC, balance events, tx history) |
| §18.8   | Provenance |

---

## 18.1 Tendermint chain landscape

The reloaded tree supports two flavours of Tendermint-based chains:

- **Iris** — chains running the Iris-mod HTLC module (e.g. IRIS,
  ATOM via IBC bridges using the Iris HTLC fork). Wire prefix:
  `irismod.htlc`.
- **Nucleus** — chains running the Nucleus HTLC module (a derivative
  of Iris with adjustments). Wire prefix: `nucleus.htlc`.

The two dialects share an identical *message shape* (create-HTLC
and claim-HTLC) but differ in:

- The Protobuf type URL on the wire.
- The ABCI query path used to fetch HTLC state by id
  (`/irismod.htlc.Query/HTLC` vs `/nucleus.htlc.Query/HTLC`).
- The Iris dialect carries extra optional fields
  (`transfer`, `receiver_on_other_chain`, `sender_on_other_chain`)
  that record cross-chain provenance; Nucleus omits them.

`mm2src/coins/tendermint/htlc/mod.rs` defines a thin abstraction
layer with two union types:

- `CreateHtlcMsg` — sum type over `IrisCreateHtlcMsg` and
  `NucleusCreateHtlcMsg`.
- `ClaimHtlcMsg` — sum type over `IrisClaimHtlcMsg` and
  `NucleusClaimHtlcMsg`.

Higher-level swap code holds a `Box<dyn Htlc>` (or a trait-object
equivalent) and dispatches without knowing the dialect.

---

## 18.2 `TendermintCoin` and `TendermintToken`

<!-- AUDIT-FLAG: M-CH18-004 Section heading and body reference internal Rust type names not on any allow-list. Replace with behaviour-oriented language: "the platform-coin type" and "the CW20-style token type". Where the chapter still needs to name files for the implementer's benefit, move them out of the spec body into the Baseline Verifications section and frame as 'expected source layout'. -->
Files of interest under
[`mm2src/coins/tendermint/`](../../mm2src/coins/tendermint/):

- `tendermint_coin.rs` — the `TendermintCoin` struct.
- `tendermint_token.rs` — `TendermintToken` (CW20-style fungible
  tokens hosted on a Tendermint platform coin).
- `tendermint_mm_coin.rs` — `MmCoin` trait impl for `TendermintCoin`.
- `tendermint_market_ops.rs` — market-side operations (orderbook
  hooks etc.).
- `tendermint_swap_ops.rs` — the V1 `SwapOps` impl.
- `tendermint_staking.rs` — staking helpers (out of scope here).
- `tendermint_helpers.rs` / `tendermint_types.rs` — shared
  utilities and type definitions.
- `ethermint_account.rs` — Ethermint-style account adapter for
  EVM-compatible Tendermint chains.
- `htlc/{mod, iris, nucleus}/*.rs` — HTLC wire types.
- `ibc/transfer_v1.rs` — IBC transfer wire types.
- `rpc/` — the small RPC handler surface that exists today (does
  not include the missing items in §18.7).

Note: `tendermint_balance_events.rs` and `tendermint_tx_history_v2.rs`
are *not yet ported* into reloaded; they live in the upstream
codebase and are framed under gaps (3) and (4) in §18.7.
<!-- AUDIT-FLAG-END: M-CH18-004 -->

### 18.2.1 Configuration

`TendermintConf` (per-coin config struct) carries:

- `account_prefix` — bech32 HRP (e.g. `"cosmos"`, `"iaa"`,
  `"nuc"`).
- `chain_id` — string id used in tx signing (`SignDoc.chain_id`).
- `gas_price` — base gas price (e.g. `0.025uatom`).
- `denom` — platform-coin base denom.
- `decimals` — display decimals (typically 6 or 18).
- `rpc_urls` — list of CometBFT RPC nodes for tx broadcast + query.

`TendermintTokenProtocolInfo` (per-token config) adds:

- `platform` — ticker of the platform coin this token rides on.
- `decimals` — token display decimals.
- `denom` — token denom (CW20 contract address encoded as a
  `cw20:{contract_addr}` denom, or a native denom for chains that
  expose native token modules).

### 18.2.2 Address derivation

Bech32: `bech32_encode(hrp = account_prefix, data = dhash160(pubkey))`.
The pubkey hash is the standard Cosmos `RIPEMD160(SHA256(pubkey))`
combination (i.e. the same `dhash160` used by Bitcoin); the HRP
disambiguates chains. Account id is the bech32 string in full,
case-sensitive.

---

## 18.3 HTLC protocol

### 18.3.1 Lifecycle

```
Send:    sender calls   MsgCreateHTLC(to, amount, hashLock, timeLock, ...)
              → chain stores HTLC under id = hash(sender || to || amount || hashLock || timeLock || timestamp)
Spend:   recipient calls MsgClaimHTLC(id, secret)
              → chain verifies sha256(secret) == hashLock, transfers amount to recipient
Refund:  *no broadcast required* — the chain auto-refunds on the block in which
              block.timestamp >= timestamp + timeLock (where timeLock is a
              block-count duration encoded relative to the create-tx's
              timestamp).
```

The auto-refund is the most important divergence from the UTXO and
EVM HTLC families: there is no `MsgRefundHTLC` message; the funds
return to the sender automatically once the timelock elapses. The
Rust side reflects this by returning a "no refund tx required —
auto-refund on chain" sentinel from the `SwapOps::*refund*`
methods.

### 18.3.2 Wire types

`MsgCreateHTLC` (Iris dialect; Nucleus omits the three
`*_on_other_chain` fields):

| Field                       | Type                  | Notes                                    |
|-----------------------------|-----------------------|------------------------------------------|
| `sender`                    | bech32 string         | Tx signer.                                |
| `to`                        | bech32 string         | Recipient when claimed.                  |
| `receiver_on_other_chain`   | string (Iris-only)    | Optional cross-chain provenance.         |
| `sender_on_other_chain`     | string (Iris-only)    | Optional cross-chain provenance.         |
| `amount`                    | `Vec<Coin>`           | Amount in `{denom, amount}` per denom.    |
| `hash_lock`                 | hex string            | `sha256(secret)`.                         |
| `timestamp`                 | uint64                | UNIX seconds; floor for timelock math.   |
| `time_lock`                 | uint64                | Block-count duration after `timestamp`.  |
| `transfer`                  | bool (Iris-only)      | If `true`, this HTLC is part of an IBC transfer. |

`MsgClaimHTLC`:

| Field    | Type          | Notes                            |
|----------|---------------|----------------------------------|
| `sender` | bech32 string | The claimer (must equal `to`).   |
| `id`     | hex string    | HTLC id returned by `create`.    |
| `secret` | hex string    | Preimage of `hash_lock`.         |

### 18.3.3 ABCI query

To check whether a previously-broadcast HTLC is still active, the
Rust side issues an ABCI query against the chain's HTLC module
(path varies by dialect):

```
path = "/irismod.htlc.Query/HTLC"  (or "/nucleus.htlc.Query/HTLC")
data = Protobuf-encoded { id: <hex> }
response = Protobuf-encoded HTLC state (state, balance, ...)
```

The Rust client decodes the response and inspects the state field
(`State::Open`, `State::Completed`, `State::Refunded`).

---

## 18.4 IBC transfer

`mm2src/coins/tendermint/ibc/transfer_v1.rs` carries the wire type
`MsgTransfer` matching the Cosmos SDK proto definition
`ibc.applications.transfer.v1.MsgTransfer`:

| Field               | Type                       | Default          |
|---------------------|----------------------------|------------------|
| `source_port`       | string                     | `"transfer"`     |
| `source_channel`    | string                     | per-route param  |
| `token`             | `Coin`                     | required         |
| `sender`            | bech32 string              | tx signer        |
| `receiver`          | bech32 string (other chain HRP) | required    |
| `timeout_height`    | `{revision_number, revision_height}` | unused (set to 0) |
| `timeout_timestamp` | uint64 nanoseconds         | `now + 15 min`   |

Defaults:

- `timeout_timestamp = block-time + 15 minutes` (configurable via
  the future RPC handler — see §18.7 gap (1)).
- Gas limit: `150_000` nanos.

The transfer wire surface is complete; the RPC handler that exposes
it to GUI code is one of the framed gaps.

---

## 18.5 V1 `SwapOps` impl

`mm2src/coins/tendermint/tendermint_swap_ops.rs` implements the
V1 atomic-swap trait surface on `TendermintCoin`. The full impl
block carries sixteen methods; the table below lists the eleven
that carry behavioural intent for this chapter (the remaining five
— `send_maker_refunds_payment`, `send_taker_refunds_payment`,
`negotiate_swap_contract_addr`, `get_htlc_key_pair`, and the
platform-coin pubkey helper — are either auto-refund sentinels or
thin delegations to chain-wide configuration):

| Method                                  | Behaviour                                                                            |
|----------------------------------------|--------------------------------------------------------------------------------------|
| `send_taker_fee`                       | Branches on `DexFee`: `Standard` → `MsgSend`; `WithBurn` → `MsgMultiSend` (split).   |
| `send_maker_payment`                   | Delegates to `send_htlc_for_denom`.                                                  |
| `send_taker_payment`                   | Delegates to `send_htlc_for_denom`.                                                  |
| `send_maker_spends_taker_payment`      | Delegates to `spend_htlc`.                                                           |
| `send_taker_spends_maker_payment`      | Delegates to `spend_htlc`.                                                           |
| `validate_fee`                         | Decodes the fee tx, asserts denoms and recipients per `DexFee`.                      |
| `validate_maker_payment`               | Delegates to `validate_payment_for_denom`.                                           |
| `validate_taker_payment`               | Delegates to `validate_payment_for_denom`.                                           |
| `check_if_my_payment_sent`             | ABCI-query HTLC state by id.                                                         |
| `search_for_swap_tx_spend_my/other`    | Cosmos-tx-search for `MsgClaimHTLC` referencing the HTLC id.                         |
| `extract_secret`                       | Decode `MsgClaimHTLC` from a spend tx and return the `secret` field.                 |

The `*refund*` methods (`send_maker_refunds_payment`,
`send_taker_refunds_payment`) return a sentinel
`"auto-refund-on-chain"` error per §18.3.1 — the caller is
expected to special-case Tendermint in the state machine and skip
the refund-broadcast step, relying on chain auto-refund instead.
The `negotiate_swap_contract_addr` and `get_htlc_key_pair` methods
return `None` and `None` respectively (Tendermint HTLC does not
use a separate swap contract address and the HTLC key pair comes
from the coin's primary signing key).

### 18.5.1 Pre-burn (`DexFee::WithBurn`) is supported

Tendermint is the only coin family that already implements the
`DexFee::WithBurn` branch on the V1 path (the UTXO V1 support
landed alongside ch.16's V2 work). Implementation:

- `WithBurn { fee_amount, burn_amount, burn_destination:
  PreBurnAccount { burn_pubkey } }` → build a `MsgMultiSend` with:
  - `inputs[0] = { address: sender, coins: fee_amount + burn_amount }`.
  - `outputs[0] = { address: fee_address, coins: fee_amount }`.
  - `outputs[1] = { address: bech32(dhash160(burn_pubkey)), coins: burn_amount }`.
- `WithBurn { burn_destination: KmdOpReturn }` → rejected
  (`KmdOpReturn` is a UTXO concept; Tendermint has no OP_RETURN).
- `NoFee` and `Standard` → single `MsgSend`.

<!-- AUDIT-FLAG: M-CH18-007 Meta-project framing ('reloaded's licence-rebase work'). Drop the meta-reference; describe only the technical design choice (Tendermint is V1-only in this chapter's scope; V2 surface is out of the chapter's scope, full stop). -->
There is no V2 atomic-swap path for Tendermint (today). The V2
state-machine driver therefore treats Tendermint as a V1-only
counterparty; the V2 trait impls (`MakerCoinSwapOpsV2`,
`TakerCoinSwapOpsV2`) are *not* implemented on `TendermintCoin` and
are explicitly out of scope for this chapter and for reloaded's
licence-rebase work.
<!-- AUDIT-FLAG-END: M-CH18-007 -->

---

## 18.6 Multi-denom and CW20-style token support

Each `TendermintCoin` owns a single platform denom; `TendermintToken`
instances ride on the same `TendermintCoin` (via a shared `Arc` to
the platform's RPC client + signing primitives) and add a single
extra denom. The HTLC and IBC wire types take `Vec<Coin>` (a list
of `{denom, amount}` pairs), so a single transaction can move
multiple denoms — but the V1 swap surface only ever uses a single
denom per HTLC (no multi-asset swaps in the V1 protocol).

CW20-style tokens are represented by denom strings of the form
`cw20:<contract_addr>`; the platform coin's CW20 module handles
these natively and the Rust side does not need a separate code path
beyond constructing the right denom string.

---

## 18.7 Known gaps

<!-- AUDIT-FLAG: H-CH18-008 The entire 'Known gaps' section frames missing functionality by reference to upstream existence ('present in the post-baseline upstream codebase but not yet ported'). This is a direct voice violation and a direct admission that the chapter consulted forbidden corpus. Rewrite as a deferred-spec subsection that specifies the four pieces purely from external inputs (Cosmos SDK task patterns, ICS-20 wire surface, SSE infrastructure from ch.10, the tx-history-v2 framework whose own chapter is the authority). Do not refer to upstream presence. -->
These four sub-features are *present in the post-baseline upstream
codebase* but not yet ported into the reloaded tree. They are
called out here so a future chapter author can pick them up
without re-discovering the gap.

### Gap 1 — Activation RPC subsystem

Missing:

- RPC method `task::enable_tendermint::init` (an async, task-managed
  activation flow following the `InitTaskMethod` pattern shared
  with HD wallets and Trezor).
- `InitPlatformCoinWithTokensTaskManager<TendermintCoin>` task-loop.
- Activation parameters struct: nodes list, tokens to enable, the
  pubkey-source choice (raw HD key vs WalletConnect session).
- `coins_activation/src/tendermint_with_assets_activation.rs`
  (platform-coin) and
  `coins_activation/src/tendermint_token_activation.rs` (per-token)
  modules.

Today the only way to bring a `TendermintCoin` online is via the
generic `enable` RPC, which lacks the token-batching and async
status-poll semantics the GUI needs.

### Gap 2 — IBC transfer RPC handler

Missing: an `ibc_transfer` (or `withdraw` with an IBC option) RPC
that takes a destination chain, channel id, recipient, amount,
optional timeout, and constructs + signs + broadcasts a
`MsgTransfer` (the wire type from §18.4 already exists).

### Gap 3 — Balance-event streaming

Missing: a SSE producer that watches Tendermint events (block
events + tx events) for balance-affecting transfers to the active
account and pushes updates through the SSE infrastructure
([Chapter 10](10-sse-streaming.md)).

The file `tendermint_balance_events.rs` is **not yet ported into
reloaded**; the upstream codebase carries a wired-up implementation
that needs to be brought across.

### Gap 4 — Tx history v2

Missing: integration with the v2 tx-history framework. The file
`tendermint_tx_history_v2.rs` is **not yet ported into reloaded**;
the upstream codebase carries the `CoinWithTxHistoryV2` impl that
needs to be brought across.

These four gaps form the natural batch for a Phase-3 Tendermint
chapter; this chapter does not commission their implementation.
<!-- AUDIT-FLAG-END: H-CH18-008 -->

---

## 18.8 Provenance

<!-- AUDIT-FLAG: H-CH18-010 Bullet 2 directly admits 'post-baseline contributor attribution intact' for files this chapter describes. This is the single highest-severity finding in the chapter set; the Phase-2A artefact gate showed the files in fact carry only this project's authorship, so the prose is also factually wrong. Delete the bullet entirely. -->
<!-- AUDIT-FLAG: H-CH18-011 Bullet 3 admits the gap list was scoped by 'direct grep ... against the forbidden-corpus reference'. This is an admission of forbidden-corpus consultation during chapter authoring. Delete the bullet entirely; rewrite the gap-list scope to refer to the design intent only. -->
<!-- AUDIT-FLAG: M-CH18-012 Section uses paragraph 'Provenance' shape instead of the locked bulleted 'Provenance Footer' shape with the mandatory 'Forbidden corpus: not consulted' line. Replace with the canonical footer from CHAPTER_TEMPLATE.md. Move the technical cross-references (Ch 2 baseline-absence, Ch 3/4 compile-fixes, Ch 16 burn-output) into the body or into Baseline Verifications where appropriate. -->
- The entire Tendermint surface is post-baseline. The baseline at
  `c1d46c0c1592faa0860f704008b2b2381bc3840f` contained no
  `mm2src/coins/tendermint/` directory; see
  [Chapter 2 — Baseline State §"Tendermint, Cosmos, IBC, and TRON
  support are not present at the baseline"](02-baseline-state.md).
- All HTLC, swap-ops, and IBC wire type code carries the
  post-baseline contributor attribution intact. No reloaded-side
  modifications to those files beyond compile-fixes documented in
  [Chapter 3 — Toolchain Modernization](03-toolchain-modernization.md)
  and [Chapter 4 — Error Aggregation & Type Adaptation](04-error-aggregation-type-adaptation.md).
- The four gaps in §18.7 are scoped by direct grep of the reloaded
  tree (placeholder files present, RPC handlers absent) against the
  forbidden-corpus reference (those handlers present in the
  upstream codebase the corpus snapshots).
- `tendermint_swap_ops.rs::send_taker_fee` is the V1 reference
  implementation for `DexFee::WithBurn` and informed the UTXO V2
  pre-burn design documented in
  [Chapter 16 §16.5.4](16-swap-v2-pre-burn-output.md#1654-burn-output-construction).
<!-- AUDIT-FLAG-END: M-CH18-012 -->
<!-- AUDIT-FLAG-END: H-CH18-011 -->
<!-- AUDIT-FLAG-END: H-CH18-010 -->
---

## 18.9 External references

- Cosmos SDK `Coin` type: `cosmos.base.v1beta1.Coin`.
- Iris-mod HTLC: https://github.com/irismod/htlc (proto definitions).
- Nucleus HTLC: post-fork derivative of Iris HTLC.
- IBC `MsgTransfer`: `ibc.applications.transfer.v1.MsgTransfer`
  (ICS-20).
- bech32: BIP 173.
- ABCI query: CometBFT spec § "ABCI: Query".
