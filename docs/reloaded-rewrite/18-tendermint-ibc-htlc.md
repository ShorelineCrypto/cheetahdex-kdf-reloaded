# Chapter 18 — Tendermint, IBC, and Cross-Chain HTLC Surfaces

**Status:** driving-spec

> **One-sentence claim:** the project supports atomic-swap and
> cross-chain transfer with Tendermint-based chains by speaking the
> Iris-mod and Nucleus HTLC protocols, the ICS-20 IBC transfer wire
> surface, and the V1 atomic-swap trait shape, against a platform
> coin and CW20-style token model that the baseline did not provide.

---

## 18.0 Executive Summary

Tendermint-based blockchains expose a JSON-RPC and gRPC interface
shape distinct from both UTXO chains and EVM chains: transactions
are signed Protobuf `SignDoc` bundles, accounts are bech32-encoded,
balances are multi-denom, and on-chain Hash-Time-Locked Contracts
are provided by a dedicated chain module rather than by a script
language or by a smart contract the wallet must deploy. This chapter
describes how the project speaks to that chain family for the
purposes of atomic swaps and IBC-routed transfers.

The design covers two HTLC dialects (Iris-mod and Nucleus), the
ICS-20 IBC transfer wire surface, a platform-coin type that holds
a single native denom, and a token type that rides on the same
platform-coin connection and holds a single additional denom
(native or CW20-encoded). The trait surface implemented on these
types is the project's existing V1 atomic-swap interface; an extra
branch is provided for the dex-fee burn variant introduced in
[Chapter 16](16-swap-v2-pre-burn-output.md).

Four sub-features are intentionally deferred from this chapter to
later work: a task-managed activation RPC for batching token
enablement, a stand-alone IBC transfer RPC handler that exposes the
wire surface to GUI callers, a balance-event Server-Sent-Events
producer wired into the streaming infrastructure of
[Chapter 10](10-sse-streaming.md), and an integration with the
project's v2 tx-history framework. They are listed in §18.7 with
the external inputs that fix their shape.

Permitted inputs that fix the shape of this chapter:

- The Cosmos SDK proto definitions for `cosmos.base.v1beta1.Coin`,
  `cosmos.bank.v1beta1.MsgSend`, and `cosmos.bank.v1beta1.MsgMultiSend`.
- The Iris-mod HTLC proto definitions published at
  `https://github.com/irismod/htlc`.
- The ICS-20 (`ibc.applications.transfer.v1.MsgTransfer`) wire
  definition published in the IBC specification family.
- BIP-173 bech32 encoding.
- The CometBFT ABCI-Query specification.
- The published JSON-RPC interface of CometBFT nodes for tx
  broadcast, block subscription, and tx search.

The chapter is structured as:

| Section | Topic |
|---------|-------|
| §18.1   | Tendermint chain landscape and dialect split (Iris vs Nucleus) |
| §18.2   | Platform-coin and CW20-style token types |
| §18.3   | HTLC protocol — wire types, ABCI query paths, lifecycle |
| §18.4   | IBC transfer (`MsgTransfer`) wire surface |
| §18.5   | V1 atomic-swap surface — payment, validation, secret extraction |
| §18.6   | Multi-denom and CW20-style token support |
| §18.7   | Deferred sub-features (activation RPC, IBC RPC, balance events, tx history) |
| §18.8   | External references |
| §18.9   | Baseline Verifications |
| §18.10  | Provenance Footer |

---

## 18.1 Tendermint chain landscape

The project supports two flavours of Tendermint-based chain:

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

## 18.2 Platform-coin and CW20-style token types

The chain family is represented in the project as two cooperating
types:

- A **platform-coin type** that owns the connection to a chain's
  CometBFT RPC endpoints, the per-chain configuration (bech32 HRP,
  chain id, gas price, native denom, decimals), the signing key,
  and the implementation of the project's general-purpose coin
  trait surface and the V1 atomic-swap trait surface.
- A **CW20-style token type** that holds a per-token denom (either
  a native denom exposed by the chain's bank module, or a
  `cw20:<contract-addr>` denom that the chain's CW20 module
  resolves to a contract-managed balance) and forwards all RPC and
  signing operations to a shared reference to the platform-coin
  type.

Neither type holds its own networking state. Multiple token
instances created against the same platform-coin instance share its
RPC client, its mempool view, and its signing primitives; spawning
a new token costs one allocation plus a per-token denom string.

### 18.2.1 Configuration

The per-coin configuration record carries:

- `account_prefix` — bech32 HRP (e.g. `"cosmos"`, `"iaa"`,
  `"nuc"`).
- `chain_id` — string id used in tx signing (`SignDoc.chain_id`).
- `gas_price` — base gas price (e.g. `0.025uatom`).
- `denom` — platform-coin base denom.
- `decimals` — display decimals (typically 6 or 18).
- `rpc_urls` — list of CometBFT RPC nodes for tx broadcast + query.

The per-token configuration record adds:

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
project's V1 swap surface, which expects every coin to be able to
broadcast a refund transaction, is satisfied here by returning a
sentinel from the refund-method family that the swap state machine
recognises as "no refund tx is needed; the chain will auto-refund".

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

The IBC transfer wire type `MsgTransfer` matches the Cosmos SDK
proto definition `ibc.applications.transfer.v1.MsgTransfer`:

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
  the deferred RPC handler in §18.7.2).
- Gas limit: `150_000` nanos.

The transfer wire surface is fully defined here; the stand-alone
RPC handler that exposes it to GUI callers is one of the deferred
sub-features (§18.7.2).

---

## 18.5 V1 atomic-swap surface

The project's V1 atomic-swap trait (the baseline `SwapOps` surface,
which predates the baseline commit) is implemented on the
platform-coin type from §18.2. The implementation carries sixteen
methods; the table below lists the eleven that carry behavioural
intent for this chapter (the remaining five — the two refund
methods, the swap-contract-address negotiator, the HTLC key-pair
accessor, and the platform-coin pubkey helper — are either
auto-refund sentinels or thin delegations to chain-wide
configuration):

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

The two refund-method entries return a sentinel that the swap
state machine recognises as "no refund tx is needed; the chain
will auto-refund" (§18.3.1). The swap-contract-address negotiator
returns `None` (Tendermint HTLC does not use a separate
swap-contract address) and the HTLC key-pair accessor returns
`None` (the HTLC key pair is the coin's primary signing key, so
there is no separate pair to surface).

### 18.5.1 Pre-burn (`DexFee::WithBurn`) is supported

Tendermint implements the `DexFee::WithBurn` branch (introduced by
[Chapter 16](16-swap-v2-pre-burn-output.md)) on the V1 fee-send
path. Implementation:

- `WithBurn { fee_amount, burn_amount, burn_destination:
  PreBurnAccount { burn_pubkey } }` → build a `MsgMultiSend` with:
  - `inputs[0] = { address: sender, coins: fee_amount + burn_amount }`.
  - `outputs[0] = { address: fee_address, coins: fee_amount }`.
  - `outputs[1] = { address: bech32(dhash160(burn_pubkey)), coins: burn_amount }`.
- `WithBurn { burn_destination: KmdOpReturn }` → rejected
  (`KmdOpReturn` is a UTXO concept; Tendermint has no OP_RETURN).
- `NoFee` and `Standard` → single `MsgSend`.

Tendermint is treated as a V1-only counterparty by the V2
state-machine driver. The V2 maker- and taker-side trait surfaces
are not implemented for this chain family in this chapter's scope;
treatment of a V2 path for Tendermint is out of scope here.

---

## 18.6 Multi-denom and CW20-style token support

The platform-coin type owns a single native denom; each token
instance attached to it adds a single extra denom. Tokens share
the platform's RPC client and signing primitives by reference, so
attaching a new token costs one allocation plus a per-token denom
string. The HTLC and IBC wire types take `Vec<Coin>` (a list of
`{denom, amount}` pairs), so a single transaction can move multiple
denoms; the V1 swap surface only ever uses a single denom per HTLC
(no multi-asset swaps in the V1 protocol).

CW20-style tokens are represented by denom strings of the form
`cw20:<contract_addr>`; the platform coin's CW20 module handles
these natively and no separate code path is needed beyond
constructing the right denom string.

---

## 18.7 Deferred sub-features

Four sub-features lie within the natural scope of Tendermint support
but are deferred from this chapter. Each is described here at the
level of the external interface it must produce; the design work
that fixes their internals belongs to a later chapter.

### 18.7.1 Task-managed activation RPC

The project's general-purpose `enable` RPC activates a single coin
synchronously and returns once activation has completed or failed.
A Tendermint platform-coin commonly arrives together with several
CW20-style tokens and a list of CometBFT RPC endpoints that must
each be probed before activation can return; the synchronous shape
produces a slow, GUI-blocking call.

The deferred shape is a task-managed RPC pair following the
project's existing init/status pattern (see the HD-wallet and
hardware-wallet activations for the same shape elsewhere in the
project): an `init` call returns a task id immediately and a
`status` call polls progress, with intermediate states for
endpoint probing, per-token enablement, and final readiness. The
input parameters are the chain configuration, the list of token
definitions to bring online in the same call, and the choice of
signing source (the project's HD key derivation, or an active
WalletConnect session per [Chapter 22](22-walletconnect-v2.md)).

### 18.7.2 IBC transfer RPC handler

The `MsgTransfer` wire type defined in §18.4 is already constructed,
signed, and broadcast in code paths that handle cross-chain HTLC
resolution. The deferred sub-feature is a dedicated RPC handler
that exposes the same construction path to GUI callers as a
stand-alone operation: arguments are the destination chain's
channel id, the recipient bech32 address, the source-denom amount,
and an optional timeout override.

### 18.7.3 Balance-event streaming

The project carries a Server-Sent-Events producer infrastructure
described in [Chapter 10](10-sse-streaming.md). A balance-event
producer for a Tendermint chain subscribes to the CometBFT
WebSocket endpoint's `tm.event='Tx'` and `tm.event='NewBlock'`
streams, filters for transfers whose sender or recipient equals the
active account, and emits a balance-update event through the SSE
framework. The deferred sub-feature is the producer; the framework
it plugs into already exists.

### 18.7.4 Tx-history v2 integration

The project's v2 tx-history framework defines a coin-side trait for
tx ingestion, classification, and storage. A Tendermint binding for
that framework queries the chain's tx-search RPC, classifies each
entry by its message type (`MsgSend`, `MsgMultiSend`, `MsgCreateHTLC`,
`MsgClaimHTLC`, `MsgTransfer`), and persists the result through the
framework's storage layer. The framework's own chapter is the
authoritative description of the trait shape; this section records
only that a Tendermint binding is in scope and not yet written.

---

## 18.8 External References

- Cosmos SDK `Coin` type: `cosmos.base.v1beta1.Coin`.
- Cosmos SDK bank module: `cosmos.bank.v1beta1.MsgSend`,
  `cosmos.bank.v1beta1.MsgMultiSend`.
- Iris-mod HTLC proto definitions:
  `https://github.com/irismod/htlc`.
- Nucleus HTLC: post-fork derivative of the Iris-mod HTLC proto.
- ICS-20 IBC transfer: `ibc.applications.transfer.v1.MsgTransfer`.
- bech32 encoding: BIP 173.
- ABCI query: CometBFT specification §"ABCI: Query".
- CometBFT JSON-RPC: tx broadcast, block subscription, and tx-search
  endpoints of the CometBFT node interface.

---

## 18.9 Baseline Verifications

The chapter relies on one baseline-state claim:

- *Claim.* The baseline tree contains no Tendermint-family coin
  support, no IBC wire types, and no on-chain HTLC support for any
  Tendermint-based chain.
- *Verification.* `git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f -- mm2src/coins/tendermint`
  returns the empty set; `git grep tendermint c1d46c0c1592faa0860f704008b2b2381bc3840f -- mm2src/coins/`
  matches nothing.
- *Cross-reference.* [Chapter 2 — Baseline State](02-baseline-state.md)
  enumerates the chain families present at the baseline; the
  Tendermint family is absent from that enumeration.

---

## 18.10 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; Cosmos SDK proto
  definitions for `Coin`, `MsgSend`, `MsgMultiSend`; ICS-20
  (`ibc.applications.transfer.v1.MsgTransfer`); the Iris-mod HTLC
  proto repository at `https://github.com/irismod/htlc`; BIP-173
  bech32; the CometBFT ABCI-Query and JSON-RPC specifications.
- *Forbidden corpus:* not consulted.
