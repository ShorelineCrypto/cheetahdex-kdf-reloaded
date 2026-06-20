# Chapter 39 -- Zcash / z_coin Shielded Coin

**Status:** driving-spec (as-built baseline **plus** remaining
required-but-unimplemented extensions). Mixed treatment -- see §39.0.

> **One-sentence claim:** the project shall support Zcash-Sapling shielded coins
> (ARRR / ZOMBIE-style) as a first-class coin type that activates in either a
> full-node ("native") mode or a light-client mode backed by Electrum servers
> plus one or more lightwalletd gRPC endpoints, drives shielded balance/scan
> through a long-running task RPC, performs atomic swaps via shielded HTLCs, and
> (by required port) gains WASM support, activation-time sync tuning, and
> integrity verification of the Sapling proving/verifying parameters.

## 39.0 Treatment & scope split

- **§39.1--§39.5 (T-DOC, as-built):** verified present in reloaded -- native-only
  ZCoin type, dual activation modes (Native / Light), multi-lightwalletd light
  mode, the `init_z_coin` task-RPC trio with its progress states, shielded HTLC
  swap operations, and the activation result shape.
- **§39.6 (T-PORT, mixed):** two items remain absent (WASM support and
  activation-time sync tuning / sync-from-date); Sapling parameter integrity
  verification is implemented in reloaded.

> **Binding scope (R36).** Requirements bind observable behaviour, the public
> activation/task RPC surface and its JSON field names, and externally *dictated*
> interop: the Zcash Sapling protocol (shielded note/commitment-tree semantics,
> Sapling spend/output proving system) and the **lightwalletd gRPC** service
> contract. Sapling cryptography and the lightwalletd protocol are the source of
> truth, not this project's code. Private types and helper structure are
> informative.

---

## Part A -- As-built baseline (T-DOC)

## 39.1 Coin type & platform

R39.1.1 The shielded coin (`ZCoin`) is a UTXO-derived coin type that adds a
Sapling shielded layer. In reloaded it is built on the **native** target only;
the WASM build excludes it (see §39.6.1 for the required port).

## 39.2 Activation modes

R39.2.1 Activation is a long-running task exposed as the public RPC trio
`init_z_coin` / `init_z_coin_status` / `init_z_coin_user_action`.

R39.2.2 The activation request carries a `mode` object (a tagged union with tag
field `rpc` and payload field `rpc_data`) selecting one of:
- **Native** -- talks to a full Zcash-family node; no extra fields.
- **Light** -- a light client carrying `electrum_servers` (the UTXO-side
  transparent backend) and `light_wallet_d_servers` (a **list** of lightwalletd
  gRPC endpoints) for the shielded side.

R39.2.3 The request also carries optional `required_confirmations` and
`requires_notarization` fields.

R39.2.4 The light mode shall accept **more than one** lightwalletd endpoint so a
deployment can list several servers.

## 39.3 Activation progress, Trezor & result

R39.3.1 The activation task shall report progress through observable in-progress
states covering at least: activating the coin, scanning the shielded chain,
requesting the wallet balance, and finishing.

R39.3.2 When the wallet is hardware-backed, the task shall additionally surface
states asking the user to connect the device and to confirm the pubkey, and shall
accept the confirmation via `init_z_coin_user_action`.

R39.3.3 On success the task result shall report `current_block` and a
`wallet_balance` carrying the shielded balance.

## 39.4 Sapling parameters & scanning (R31 externally dictated)

R39.4.1 Shielded proving requires the Sapling spend/output parameters; the
project shall load them from the local parameter location and use them to build
and verify Sapling proofs.

R39.4.2 In light mode the project shall fetch compact blocks / shielded note data
from the configured lightwalletd endpoint(s) over gRPC, scan them to detect
incoming and spent notes, and maintain the shielded note set and witness data in
local storage.

R39.4.3 Unconfirmed (mempool / not-yet-mined) shielded notes shall be tracked
correctly so that the spendable shielded balance does not double-count or omit
in-flight notes.

## 39.5 Shielded atomic swaps (R31 externally dictated)

R39.5.1 The project shall perform atomic swaps for shielded coins using a
Sapling-based HTLC construction, fulfilling the same maker/taker payment,
spend-with-secret, and refund-after-timelock semantics required of every coin in
the swap protocol.

---

## Part B -- Required ports (T-PORT)

> **Status of Part B:** partially implemented in reloaded. R39.6.3 is
> implemented; R39.6.1 and R39.6.2 remain required.

## 39.6 Required shielded-coin ports

### 39.6.1 WASM support
R39.6.1 The shielded coin shall be buildable and activatable on the WASM target,
with its shielded note/witness storage backed by IndexedDB (mirroring the
native storage contract). Acceptance: a light-mode shielded coin activates in a
WASM build and reports a shielded balance.

### 39.6.2 Activation-time sync tuning / sync-from-date
R39.6.2 The activation request shall optionally accept sync-control parameters --
at minimum a **sync starting point** expressed either as a block height or as a
calendar **date** (so a fresh wallet need not scan from Sapling activation), and
scan-throughput tuning (blocks-per-iteration and/or inter-iteration interval).
Acceptance: activating with a sync-from-date begins scanning at the block
corresponding to that date, materially reducing initial scan time.

### 39.6.3 Sapling-parameter integrity verification
R39.6.3 Before use, the loaded Sapling spend/output parameters shall be verified
against their known-good integrity digests; parameters that fail verification
shall be rejected (and, if a downloader is provided, re-fetched). Acceptance: a
corrupted parameter file is detected and refused rather than used to produce
invalid proofs.

> **Status update (reloaded).** This requirement is implemented: Sapling spend
> and output parameter files are integrity-checked against canonical digests
> before prover initialization, and mismatches are rejected with explicit
> read/hash-mismatch errors.

## 39.7 Acceptance criteria (chapter)

- Baseline: a light-mode shielded coin activates via `init_z_coin`, advances
  through the documented progress states, accepts multiple lightwalletd
  endpoints, and returns `current_block` + `wallet_balance` (§39.2--§39.3).
- A shielded HTLC swap completes maker and taker legs and a refund path (§39.5).
- R39.6.3 is implemented with integrity-check behaviour enforced before prover
  initialization.
- R39.6.1 and R39.6.2 remain pending ports.
