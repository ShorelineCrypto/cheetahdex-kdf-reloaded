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
- **§39.6 (T-PORT, mixed):** three items remain absent (WASM support,
  activation-time sync tuning / sync-from-date, and sourcing consensus
  parameters/checkpoint/HD path from `protocol_data`); Sapling parameter
  integrity verification is implemented in reloaded.

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

R39.1.2 A shielded coin's `coins`-config `protocol` field is a tagged object
with `type` = `"ZHTLC"` **and a required `protocol_data` object**. The
coin-protocol type is an *adjacently tagged* union (the `type` string is the
tag; the `protocol_data` object is the content). The `ZHTLC` arm is a
**payload-carrying** variant — not a unit variant — whose content deserializes
into a shielded protocol-info structure with three members:

- `consensus_params` (object, **required**) — the Zcash consensus parameters
  for the coin (schema in R39.1.3);
- `check_point_block` (object, **optional**) — a sync-anchor block descriptor
  (schema in R39.1.4);
- `z_derivation_path` (string, **optional**) — a coin-level BIP32/ZIP32 HD path
  (e.g. `m/32'/133'`) used for shielded key derivation (R39.1.4).

Because the variant carries a required payload whose required member is
`consensus_params`, a **bare `{"type":"ZHTLC"}` with no `protocol_data` is
non-conformant** and shall fail coin-config deserialization (adjacently-tagged
serde has no content to populate the required fields). Every real config carries
`protocol_data`: production coins (ARRR/PIRATE) and the ZOMBIE test coin all
ship a full `protocol_data` block. See R39.6.4 for the required *consumption* of
these values by the shielded-coin builder.

R39.1.3 The `consensus_params` object is the Zcash network-parameter set and has
the following members (this is dictated config/wire interop — the field names
and JSON shapes are fixed):

| Field | JSON type | Required | Notes |
|-------|-----------|----------|-------|
| `overwinter_activation_height` | integer (unsigned 32-bit) | yes | Overwinter network-upgrade activation height. |
| `sapling_activation_height` | integer (unsigned 32-bit) | yes | Sapling activation height; also the lower floor for any sync start point. |
| `blossom_activation_height` | integer or `null` | no (nullable) | Blossom activation height, or `null` if not applicable. |
| `heartwood_activation_height` | integer or `null` | no (nullable) | Heartwood activation height, or `null`. |
| `canopy_activation_height` | integer or `null` | no (nullable) | Canopy activation height, or `null`. |
| `coin_type` | integer (unsigned 32-bit) | yes | SLIP-44 coin type used in shielded HD derivation. |
| `hrp_sapling_extended_spending_key` | string | yes | Bech32 human-readable prefix for extended spending keys. |
| `hrp_sapling_extended_full_viewing_key` | string | yes | Bech32 HRP for extended full-viewing keys. |
| `hrp_sapling_payment_address` | string | yes | Bech32 HRP for shielded payment addresses. |
| `b58_pubkey_address_prefix` | array of exactly 2 integers (each 0–255) | yes | Base58Check version prefix for transparent p2pkh addresses. |
| `b58_script_address_prefix` | array of exactly 2 integers (each 0–255) | yes | Base58Check version prefix for transparent p2sh addresses. |

R39.1.4 The optional `check_point_block` object, when present, is a sync-anchor
descriptor with all of the following members:

| Field | JSON type | Notes |
|-------|-----------|-------|
| `height` | integer (unsigned 32-bit) | Block height of the checkpoint. |
| `hash` | string | 32-byte block hash, hex-encoded. |
| `time` | integer (unsigned 32-bit) | Block timestamp (Unix seconds). |
| `sapling_tree` | string | Hex-encoded Sapling commitment-tree state as of this block. |

The optional `z_derivation_path` is a coin-level HD path string (ZIP32/BIP32
form, e.g. `m/32'/133'`). When the active key policy is HD-derived, the shielded
spending key is derived along this path with the activation `account` appended
as a hardened child (i.e. `m/<z_derivation_path>/account'`); it is required only
for the HD key policy (its absence is an error only in that policy). Both
`check_point_block` and `z_derivation_path` are consumed by the builder per
R39.6.4.

> **Upstream divergence (informative).** Reloaded currently defines the `ZHTLC`
> coin-protocol arm as a **unit** variant (no payload) and its shielded builder
> hardcodes Zcash-mainnet constants (see R39.6.4). Consequently reloaded's
> current behaviour is the inverse of upstream: it accepts a bare
> `{"type":"ZHTLC"}` and would *reject* a config that carries a `protocol_data`
> map (an adjacently-tagged unit variant cannot absorb content). Conforming to
> R39.1.2–R39.1.4 requires making the arm payload-carrying and updating the
> ZOMBIE test fixtures to ship full `protocol_data` (see R39.6.4 status note).

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

> **Status of Part B:** R39.6.1, R39.6.2, R39.6.3, and R39.6.4 are all
> implemented. Note: R39.6.4 has `z_derivation_path` parsed and stored,
> but HD-derived key policy support is deferred to a future enhancement.

## 39.6 Required shielded-coin ports

### 39.6.1 WASM support
### 39.6.1 WASM support
R39.6.1 The shielded coin shall be buildable and activatable on the WASM target,
with its shielded note/witness storage backed by IndexedDB (mirroring the
native storage contract). Acceptance: a light-mode shielded coin activates in a
WASM build and reports a shielded balance.

> **Status update (reloaded).** Implemented (commits 5514802de, f1a0a338e,
> 6cad05692). `zcash_primitives` and `zcash_client_backend` are now available
> on the WASM target. The sapling state cache is backed by a new
> `SaplingStateCacheOps` trait; `ZCoinSqliteSaplingCache` serves native builds
> and `ZCoinIdbSaplingCache` (mm2_db IndexedDB backend) serves WASM. The
> `MmCoinEnum::ZCoin` variant and the `z_coin` module are available on all
> targets. Transaction building (`gen_tx` / `send_outputs`) remains native-only
> because `LocalTxProver` (sapling parameter files) is absent in WASM.

### 39.6.2 Activation-time sync tuning / sync-from-date
R39.6.2 The activation request shall optionally accept sync-control parameters --
at minimum a **sync starting point** expressed either as a block height or as a
calendar **date** (so a fresh wallet need not scan from Sapling activation), and
scan-throughput tuning (blocks-per-iteration and/or inter-iteration interval).
Acceptance: activating with a sync-from-date begins scanning at the block
corresponding to that date, materially reducing initial scan time.

> **Status update (reloaded).** Implemented (commit 7976367e2). The activation
> request now accepts `blocks_per_iteration` (u32, default 1) and
> `inter_iteration_interval_ms` (u64, default 0) to control sync throughput and
> pacing. A `SyncStartpoint` enum (`Height(u32)` | `Date(String)`) is accepted;
> height-based start is wired through; date-to-height resolution is deferred
> pending an RPC lookup API.

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

### 39.6.4 Consume `protocol_data` consensus parameters
R39.6.4 The shielded-coin builder shall source **all** of its Zcash
network parameters, its shielded HD derivation path, and its sync checkpoint
from the coin config's `protocol.protocol_data` (R39.1.2–R39.1.4), rather than
from hardcoded Zcash-mainnet constants. Concretely:

- **Consensus parameters.** The `consensus_params` object shall be the single
  authority for the coin's network-parameter lookups: activation heights for
  each supported network upgrade (Overwinter/Sapling required; Blossom /
  Heartwood / Canopy optional), the `coin_type`, the three `hrp_sapling_*`
  human-readable prefixes, and the two `b58_*` transparent-address version
  prefixes. These values shall feed every place the builder and the running coin
  need network parameters: encoding/decoding the wallet's own shielded payment
  address and the DEX fee/burn shielded addresses (via
  `hrp_sapling_payment_address`), Sapling note trial-decryption and output
  recovery during scanning, address handling, and shielded-swap construction.
  A coin whose `consensus_params` differs from Zcash mainnet shall derive and
  scan accordingly.
- **HD derivation.** When the key policy is HD-derived, the shielded spending
  key shall be derived along `z_derivation_path` with the activation `account`
  appended as a hardened child (R39.1.4); with a raw/iguana key policy the path
  is not required.
- **Sync checkpoint.** `check_point_block` shall be the sync-start anchor. In
  native mode the wallet database is anchored at `check_point_block.height`,
  falling back to `sapling_activation_height` when the checkpoint is absent. In
  light mode the anchor is the checkpoint corresponding to the resolved sync
  start height (a height/date sync parameter, or the earliest/default), floored
  at `sapling_activation_height`; the checkpoint's `sapling_tree` seeds the
  wallet's initial commitment-tree state so scanning need not replay from
  Sapling activation.

Acceptance: a ZHTLC coin whose `protocol_data` declares non-Zcash-mainnet
parameters (different HRP/b58 prefixes, `coin_type`, or activation heights)
produces addresses and derives keys under those declared parameters and begins
its shielded sync from the declared `check_point_block` (or from
`sapling_activation_height` when no checkpoint is given) rather than from
mainnet constants.

> **Status update (reloaded).** Implemented (commit f1a0a338e). The shielded
> builder now sources all Zcash network parameters from the coin config's
> `protocol.protocol_data` via the typed `ZcoinProtocolInfo` payload:
>
> - **Consensus parameters (R39.6.4 §1).** The builder reads
>   `consensus_params` from `protocol_data` and uses its `coin_type`, `hrp_sapling_*`
>   prefixes, `b58_*` prefixes, and activation-height policy throughout the running
>   coin (address encoding/decoding, key derivation, scanning, transaction
>   construction, and commitment-tree sync). The `ZcashConsensusParams` struct
>   implements `zcash_primitives::consensus::Parameters` to drive consensus
>   validation. A ZHTLC coin with non-mainnet parameters (e.g., different HRP or
>   `coin_type`) now derives keys and addresses under those declared parameters,
>   not Zcash-mainnet defaults.
> - **Sync checkpoint (R39.6.4 §3).** The builder seeds the wallet's
>   commitment-tree cache at `check_point_block.height`, deserialized from the
>   checkpoint's `sapling_tree`, falling back to `sapling_activation_height` when
>   absent (native mode). Light-mode checkpoint-to-height tree seeding is deferred
>   (CRD 39.8.0b).
> - **HD derivation path (R39.6.4 §2).** The `z_derivation_path` field is parsed
>   from `protocol_data` and stored in `ZcoinProtocolInfo` for use when the key
>   policy is HD-derived. The current implementation enforces
>   `PrivKeyActivationPolicy::IguanaPrivKey` (single-key mode), so `z_derivation_path`
>   is not consulted; support for HD-derived key policies is deferred to a future
>   enhancement.
> - **ZOMBIE fixtures.** Test fixtures updated to carry full `protocol_data` with
>   Zcash-mainnet parameters; bare `{"type":"ZHTLC"}` is now non-conformant per
>   R39.1.2.

---

## Part C -- Shielded transaction-history RPC (T-DOC)

## 39.8 `z_coin_tx_history` method

> **Source-of-truth note.** The wire contract below (method string, request and
> response field names/types, error variants) is the externally dictated public
> RPC interface; the authoritative reference is the Komodo DeFi Framework API
> documentation for `z_coin_tx_history`. Behaviour is specified abstractly.

> **Feasibility verdict (reloaded): (B) SUBSTRATE-BLOCKED — ship the clean
> published failure now; full data path is a forward port.** The wire contract
> below is dictated and stable, but the *data* it serves cannot be produced on
> reloaded's current substrate. See §39.8.0 for the verdict, the role-level
> upstream behaviour it is measured against, the clean-failure contract the
> reloaded build must honour today (R39.8.0a), and the missing-substrate list a
> real implementation would require (R39.8.0b).

### 39.8.0 Feasibility verdict, upstream behaviour & clean-failure contract

**Role-level behaviour this method is measured against (informative).** A
"finished" shielded transaction history is a *wallet-derived* history: for every
shielded transaction it must state which outputs the wallet received
(`received_by_me`), which of the wallet's notes were spent (`spent_by_me`), the
participating shielded address sets (`from`/`to`), the net balance change, the
fee, and the mining height. Producing those values for a privacy-preserving
(Sapling) coin requires trial-decrypting the chain's shielded outputs with the
wallet's incoming-viewing key and tracking note nullifiers to detect spends —
i.e. a **light-client shielded wallet**. In the reference behaviour this is
realised by a light-client shielded-wallet database (the public `zcash`
light-client wallet-DB schema: scanned blocks, wallet transactions, and
received/spent shielded notes) that is filled by a background **compact-block
scanner** consuming the lightwalletd gRPC stream during and after activation.
`z_coin_tx_history` then reads one page straight out of that wallet database; the
`internal_id` used for `FromId` paging is that database's monotonically
increasing **signed-integer transaction row identifier**, which is why the
shielded paging key is an integer (R39.8.4, R39.8.10) rather than the opaque
byte-string identifier of the generic v2 method.

**Verdict: (B) SUBSTRATE-BLOCKED.** Reloaded's `ZCoin` is a native-full-node
port. Its only local shielded store is a commitment-tree **creation cache** used
to build/witness outgoing shielded transactions; it has no wallet-history store,
no incoming-viewing-key compact-block scanner, no per-note received/spent
tracking, and therefore no signed-integer `internal_id` keyspace to page over. A
native full node does not expose this project's shielded note ownership (the
project manages its own shielded keys, not the node's wallet), so the
`received_by_me` / `spent_by_me` / `from` / `to` values cannot be derived from
the current substrate by any bounded change. Fabricating, omitting, or
partially guessing those values would be a correctness and privacy hazard.
Accordingly the method is **not** deliverable as a real data path now; the honest
finish is a published, documented failure plus this forward-spec.

R39.8.0a **Clean-failure contract (current substrate).** On the current
native-only substrate the build shall still **dispatch** `z_coin_tx_history`
over the mmrpc 2.0 envelope and apply the boundary validation of §39.8.1–§39.8.2
so that genuine input errors return their documented discriminants — an
unactivated `coin` returns `CoinIsNotActive` (404) and an activated non-shielded
coin returns `NotSupportedFor` (400). For an activated shielded coin, because no
wallet-history store exists, the method shall return the documented
`StorageIsNotInitialized` discriminant (HTTP 500) via the standard v2 error
envelope. It shall **never** panic, never fabricate or partially synthesize
history entries, and never emit shielded amounts/addresses it cannot derive. The
failure is stable and documented, so a caller receives a well-formed,
discriminated response rather than an unknown-method error or a crash.

R39.8.0b **Missing substrate for the real implementation (forward port).**
Delivering the full R39.8.9 data path requires porting, in order:
1. a **light-client shielded-wallet database** (the public `zcash` light-client
   wallet-DB schema — scanned blocks, wallet transactions, received notes with
   value and spent-linkage) as a per-coin local store;
2. an **incoming-viewing-key compact-block scanner** that consumes the
   lightwalletd gRPC compact-block stream, trial-decrypts shielded outputs,
   records received notes, and tracks nullifiers to mark spends — running as a
   background sync loop during/after activation (this is the §39.2 light mode and
   §39.4.2 note-scanning substrate, which reloaded also lacks; see open
   question);
3. a stable, monotonically increasing **signed-integer transaction identifier**
   keyspace for `internal_id` / `FromId` paging, sourced from that wallet DB;
4. the shared v2 history-request **`target`** field on the request envelope so
   the dictated wire shape (R39.8.3, R39.8.5) is accepted and echoed.

Once that substrate exists, R39.8.9 specifies the handler behaviour unchanged and
this chapter becomes a driving-spec for the real data path. Until then R39.8.0a
is the binding behaviour.

### 39.8.1 Envelope, method string & platform gate

R39.8.1 The project shall expose a dedicated shielded-coin transaction-history
method with the wire method string `z_coin_tx_history`, dispatched over the
**mmrpc 2.0** envelope (`{"mmrpc":"2.0","method":"z_coin_tx_history",
"params":{...}}`, with the usual `userpass`). The result is returned in the
standard v2 `{"mmrpc":"2.0","result":{...}}` success envelope; errors use the v2
error envelope (`error`, `error_path`, `error_trace`, `error_type`,
`error_data`).

R39.8.2 The method is **native-only**, consistent with the ZCoin platform gate
(§39.1). It is resolved against the activated coin named by `coin` and shall
succeed only when that coin is an activated shielded (ZCoin) coin; any other
activated coin type is rejected (see R39.8.6).

### 39.8.2 Request parameters

R39.8.3 The request `params` object shall accept the following fields (this is
the shared v2 transaction-history request envelope, specialized to an
**integer** paging identifier for the shielded coin):

| Field | JSON type | Required | Default | Notes / bounds |
|-------|-----------|----------|---------|----------------|
| `coin` | string | yes | — | Ticker of an activated shielded coin. |
| `limit` | integer (unsigned) | no | `10` | Maximum number of transaction entries to return for the page. |
| `paging_options` | object (tagged union) | no | `{ "PageNumber": 1 }` | Selects the page; see R39.8.4. |
| `target` | object (tagged union) | no | `{ "type": "iguana" }` | Shared-envelope address-scope selector; accepted and echoed back in the response. Not used to scope shielded history results. |

R39.8.4 `paging_options` is a tagged union with exactly one of two shapes:
- `{ "PageNumber": <n> }` -- 1-based page number; `<n>` is a non-zero positive
  integer. This is the default when `paging_options` is omitted (page `1`).
- `{ "FromId": <id> }` -- continue paging from the entry whose internal
  identifier is `<id>` (a signed 64-bit integer matching the `internal_id`
  field of response entries; see R39.8.5). When `FromId` is supplied the page
  begins at the entries that follow that identifier in history order.

R39.8.5 `target` is a tagged union on field `type` with values `iguana`
(default), `account_id` (carrying an `account_id` integer), and `address_id`
(carrying an HD account/address path selector). It is part of the shared
request envelope; for the shielded method it is accepted for envelope
compatibility and reflected in the response unchanged, and does not alter which
shielded transactions are returned.

### 39.8.3 Success response

R39.8.6 On success the `result` object shall carry:

| Field | JSON type | Description |
|-------|-----------|-------------|
| `coin` | string | Echo of the requested ticker. |
| `target` | object | Echo of the request `target`. |
| `current_block` | integer | Current tip height known to the coin's backend at query time. |
| `transactions` | array of objects | The page of shielded transaction detail entries (see R39.8.7). |
| `sync_status` | object | History-sync state, tagged on field `state` with optional `additional_info`. For the shielded coin this is always the terminal `Finished` state, because a shielded coin is only active after its initial scan completes (§39.3). |
| `limit` | integer | Echo of the effective page limit. |
| `skipped` | integer | Number of entries skipped ahead of this page. |
| `total` | integer | Total number of known shielded transactions. |
| `total_pages` | integer | Total page count for `total` at the effective `limit`. |
| `paging_options` | object | Echo of the effective paging selector. |

R39.8.7 Each entry in `transactions` is a **shielded-coin transaction detail**
object whose shape differs from the generic v2 history entry. Its fields are:

| Field | JSON type | Description |
|-------|-----------|-------------|
| `tx_hash` | string | Transaction hash, hexadecimal. |
| `from` | array of strings | Source address set the coins were sent from. |
| `to` | array of strings | Destination address set the coins were sent to. |
| `spent_by_me` | decimal (string/number) | Amount spent from the wallet's own address. |
| `received_by_me` | decimal | Amount received by the wallet's own address. |
| `my_balance_change` | decimal | Net balance change for the wallet (received minus spent). |
| `block_height` | integer | Block height the transaction was mined at. |
| `confirmations` | integer | Confirmation count derived from `current_block` versus `block_height`. |
| `timestamp` | integer | Transaction timestamp (Unix seconds). |
| `transaction_fee` | decimal | Fee paid by the transaction. |
| `coin` | string | Ticker the transaction belongs to. |
| `internal_id` | integer (signed 64-bit) | Stable internal identifier used for `FromId` paging (R39.8.4). |

### 39.8.4 Error conditions

R39.8.8 The method shall report failures using the v2 error envelope with an
`error_type` drawn from the following set (functional descriptions; literal
operator-facing wording is not normative):

| `error_type` | HTTP status | Condition |
|--------------|-------------|-----------|
| `CoinIsNotActive` | 404 | The named `coin` is not an activated coin. |
| `NotSupportedFor` | 400 | The named coin is activated but is not a shielded (ZCoin) coin, so shielded history is unavailable for it. |
| `InvalidTarget` | 400 | The supplied `target` selector is invalid for the coin's wallet (e.g. an HD path/chain that does not apply). |
| `StorageIsNotInitialized` | 500 | The local transaction-history store for the coin has not been initialized. |
| `StorageError` | 500 | A failure occurred reading or building the local history store. |
| `RpcError` | 500 | A backend RPC error occurred while resolving tip height or fetching verbose transaction data. |
| `Internal` | 500 | An otherwise-unclassified internal error (e.g. address resolution). |

> **Shared error surface (corpus-faithful).** `z_coin_tx_history` returns the
> **same shared v2 transaction-history error type (`MyTxHistoryErrorV2`)** as the
> generic `my_tx_history` (v2) method — upstream reuses that one type for both
> methods. The HTTP statuses in the table above are therefore *inherited* from
> that shared type's status mapping exactly as upstream defines it
> (`CoinIsNotActive` → 404; `NotSupportedFor` and `InvalidTarget` → 400;
> `StorageIsNotInitialized`, `StorageError`, `RpcError`, and `Internal` → 500).
> The **published wire contract is the `error_type` discriminant names**; the
> HTTP integers are the shared type's inherited mapping rather than a
> per-method-published value.

> **Reloaded alignment.** Reloaded's shared `MyTxHistoryErrorV2` HTTP status
> mapping is aligned to the upstream values recorded in the table above
> (`CoinIsNotActive` → 404; `NotSupportedFor` → 400; `StorageIsNotInitialized`,
> `StorageError`, `RpcError` → 500). Because this is a single shared enum
> consumed by both `my_tx_history` and `z_coin_tx_history`, the alignment applies
> to both methods; the `error_type` discriminant names — the published contract —
> are identical and unaffected. (Reloaded's enum is a faithful subset of
> upstream's: it does not carry the `InvalidTarget` / `Internal` variants, and it
> adds a reloaded-only wasm-guard discriminant.)

### 39.8.5 Functional behaviour

> **Gated by §39.8.0 (verdict B).** R39.8.9 is the **forward-spec data path**;
> it becomes binding only once the §39.8.0b substrate is ported. On the current
> reloaded substrate the binding behaviour is the clean failure of R39.8.0a.

R39.8.9 The handler shall: resolve the activated coin by `coin` and confirm it
is a shielded coin; determine the current tip height from the coin's backend;
read one page of stored shielded transaction records from the coin's local
wallet history store honouring `limit` and `paging_options`; resolve the full
(verbose) transaction data for those records (and for their referenced previous
transactions) from a local cache or, on a miss, the backend; and assemble each
record into a shielded transaction detail entry (R39.8.7) -- deriving the
`from`/`to` address sets, the own-wallet spent/received amounts and net balance
change, the fee, and the confirmation count relative to the tip. The response
also reports paging metadata (`skipped`, `total`, `total_pages`) computed from
the stored history.

### 39.8.6 Relationship to the generic `my_tx_history` v2 method

R39.8.10 `z_coin_tx_history` reuses the **same v2 request/response envelope
types** as the generic v2 `my_tx_history` method (the same `coin`, `limit`,
`paging_options`, `target` request fields and the same `current_block`,
`transactions`, `sync_status`, `limit`, `skipped`, `total`, `total_pages`,
`paging_options` response framing). It differs in two contract-visible ways and
is therefore a distinct method rather than a branch of `my_tx_history`:
- **Paging identifier type.** The shielded method's paging identifier
  (`FromId` and the entry `internal_id`) is a **signed 64-bit integer**, whereas
  the generic v2 method keys paging on an opaque byte-string identifier.
- **Transaction entry shape.** The shielded method returns the shielded-specific
  detail object of R39.8.7 (shielded `from`/`to` address sets, integer
  `internal_id`), rather than the generic transaction-details entry returned by
  `my_tx_history`.

> **Upstream divergence (informative).** The `target` field is part of the
> shared v2 history-request envelope; for the shielded method it is accepted and
> echoed but does not scope the returned shielded transactions. Reloaded keeps
> this envelope-compatible acceptance to preserve the dictated wire contract.

## 39.7 Acceptance criteria (chapter)

- Baseline: a light-mode shielded coin activates via `init_z_coin`, advances
  through the documented progress states, accepts multiple lightwalletd
  endpoints, and returns `current_block` + `wallet_balance` (§39.2--§39.3).
- A shielded HTLC swap completes maker and taker legs and a refund path (§39.5).
- R39.6.3 is implemented with integrity-check behaviour enforced before prover
  initialization.
- Config-loading (R39.1.2–R39.1.4): a `{"type":"ZHTLC","protocol_data":{...}}`
  config with a well-formed `consensus_params` (plus optional `check_point_block`
  and `z_derivation_path`) deserializes successfully; a bare
  `{"type":"ZHTLC"}` (no `protocol_data`) is rejected at coin-config parse time.
- Parameter sourcing (R39.6.4, pending port): a ZHTLC coin whose `protocol_data`
  declares non-mainnet HRP/b58 prefixes, `coin_type`, or activation heights
  produces addresses/keys under those declared values and begins its shielded
  sync from the declared `check_point_block` (or `sapling_activation_height`
  when absent), not from hardcoded mainnet constants.
- R39.6.4 remains pending completion (HD-derived key policy support deferred).
- `z_coin_tx_history` (mmrpc 2.0, native-only) is **substrate-blocked** (§39.8.0,
  verdict B). On the current substrate it is dispatched and validates input:
  inactive coins return `CoinIsNotActive` (404), activated non-shielded coins
  return `NotSupportedFor` (400), and an activated shielded coin returns the
  documented `StorageIsNotInitialized` (500) — no panic, no fabricated history
  (R39.8.0a).
- Forward-spec acceptance (once the §39.8.0b substrate is ported):
  `z_coin_tx_history` returns a paginated page of shielded transaction detail
  entries for an activated shielded coin, honours `limit` and both
  `PageNumber`/`FromId` paging modes, echoes paging metadata, reports
  `sync_status: Finished`, and rejects non-shielded coins (`NotSupportedFor`)
  and inactive coins (`CoinIsNotActive`) (§39.8).
