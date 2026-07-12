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
> `protocol.protocol_data` payload:
>
> - **Consensus parameters (R39.6.4 §1).** The builder reads
>   `consensus_params` from `protocol_data` and uses its `coin_type`, `hrp_sapling_*`
>   prefixes, `b58_*` prefixes, and activation-height policy throughout the running
>   coin (address encoding/decoding, key derivation, scanning, transaction
>   construction, and commitment-tree sync). A ZHTLC coin with non-mainnet
>   parameters (e.g., different HRP or `coin_type`) now derives keys and
>   addresses under those declared parameters, not Zcash-mainnet defaults.
> - **Sync checkpoint (R39.6.4 §3).** The builder seeds the wallet's
>   commitment-tree cache at `check_point_block.height`, deserialized from the
>   checkpoint's `sapling_tree`, falling back to `sapling_activation_height` when
>   absent (native mode). Light-mode checkpoint-to-height tree seeding is deferred
>   (CRD 39.8.0b).
> - **HD derivation path (R39.6.4 §2).** The `z_derivation_path` field is parsed
>   from `protocol_data` and retained for use when the key
>   policy is HD-derived. The current implementation is limited to the single-key
>   policy, so `z_derivation_path` is not consulted; support for HD-derived key
>   policies is deferred to a future enhancement.
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

### 39.8.0 Shielded history is activation-owned wallet state

R39.8.0a A successfully activated shielded coin shall have an initialized
shielded-wallet history store. The store is part of ZCoin activation, not the
generic v2 UTXO history background fetcher. A terminal activation result for
ARRR / ZCoin in Light or Native mode means the wallet database exists, contains
the tracked account/viewing key, has scanned through the activation tip selected
by the sync start policy, and can be queried by `z_coin_tx_history`.

R39.8.0b Activation shall not report shielded sync as finished merely because a
Sapling commitment-tree or block-state cache has reached the backend tip. The
terminal state is valid only after compact blocks have also been validated and
applied to the shielded wallet database so received notes, note nullifiers,
spend links, witnesses, and wallet transactions are available to the history and
balance paths.

R39.8.0c The generic `my_tx_history` v2 method is not the shielded transaction
history interface. For an activated ZCoin it shall not be used to synthesize
shielded history from the generic UTXO history store; it shall reject the coin as
unsupported for that method. Shielded callers, including Desktop, shall use
`z_coin_tx_history` for ARRR/ZCoin transaction display.

R39.8.0d After a successful Light activation of ARRR, an LTC-to-ARRR swap that
pays the wallet's shielded address shall become visible through
`z_coin_tx_history` once the ARRR transaction is mined and scanned. Returning
`StorageIsNotInitialized` for that activated ARRR coin is non-conformant unless
the local wallet database is genuinely unavailable or corrupt and the coin should
not have reached a normal terminal activation state.

### 39.8.0.1 Activation trigger conditions

R39.8.0e On every `init_z_coin` activation for a ZCoin, the implementation shall
create or open two per-coin local stores before the coin is returned active:

- a compact-block cache keyed by block height, storing the serialized compact
  block data fetched from the configured shielded backend;
- a shielded wallet database keyed by coin/account state, storing scanned
  blocks, tracked account viewing keys, wallet transactions, received notes,
  sent-note metadata, and Sapling witnesses.

R39.8.0f In Light mode, activation shall use the configured Electrum servers for
the transparent backend and the configured `light_wallet_d_servers` for compact
block and shielded note scanning. In Native mode, activation shall use the native
Zcash-family backend as the compact-block source. Both modes shall feed the same
shielded wallet database contract and the same `z_coin_tx_history` data path.

R39.8.0g The sync start point is resolved before wallet scanning:

- an explicit height starts from that height, floored at
  `sapling_activation_height`;
- an explicit date starts from the backend block height resolved for that date,
  floored at `sapling_activation_height`;
- `earliest` starts from `sapling_activation_height`;
- an omitted start point may continue from existing local wallet/cache state
  when that state exists; otherwise it shall use the implementation's default
  recent-start policy, floored at `sapling_activation_height`.

R39.8.0h If a caller supplies a start point that differs from existing local
scan state, activation shall rewind or recreate the compact-block cache and
wallet database to a safe height before rescanning. If the caller explicitly
requests reuse of previous sync state and a valid previous state exists,
activation may continue from that state. The activation result shall expose the
requested start, whether the request was below Sapling activation, and the actual
start height used.

### 39.8.0.2 Storage and schema expectations

R39.8.0i The native wallet database shall be compatible with the public Zcash
light-client wallet schema used by `zcash_client_sqlite`, including at least the
logical tables `accounts`, `blocks`, `transactions`, `received_notes`,
`sent_notes`, and `sapling_witnesses`. The schema shall preserve the database's
monotonically increasing signed transaction row identifier, because
`z_coin_tx_history` exposes that identifier as `internal_id` and accepts it in
`FromId` paging.

R39.8.0j The WASM wallet database shall preserve the same logical data model in
IndexedDB. Its table names may be namespaced for the runtime, but it shall store
the same categories of data: accounts/viewing keys, scanned blocks, wallet
transactions, received notes with value and optional spent linkage, sent-note
metadata, and Sapling witnesses. It shall also provide stable per-transaction
integer identifiers for shielded history paging.

R39.8.0k The compact-block cache shall store compact blocks by height and support
querying the latest, earliest, and ranged block data, plus rewinding to a height.
It is not a replacement for the shielded wallet database: it is only the scanned
block source from which wallet notes, transactions, nullifiers, and witnesses are
derived.

R39.8.0l The wallet database shall be initialized with the wallet's extended full
viewing key and the configured checkpoint block when available. The checkpoint's
height, hash, timestamp, and Sapling tree seed the scanned-block state so the
wallet can start from the resolved sync point instead of replaying from Sapling
activation.

### 39.8.0.3 Scanner behavior

R39.8.0m The shielded scanner shall run during activation and continue after
activation as the coin's background shielded sync loop. During activation it
shall report observable progress for compact-block cache update and wallet-db
building, and activation shall wait until the wallet database is scanned through
the current activation tip before returning success.

R39.8.0n The scanner shall fetch compact blocks from the configured shielded
backend, cache them by height, validate that scanned heights are sequential and
that block hashes link to the previous scanned block, and rewind/rescan on chain
continuity failures instead of accepting inconsistent wallet history.

R39.8.0o The scanner shall trial-decrypt Sapling outputs with the wallet's
incoming viewing capability, record notes belonging to tracked accounts, advance
the Sapling commitment tree, maintain incremental witnesses, track note
nullifiers, and mark a received note as spent when a later scanned transaction
spends its nullifier. These records are the source of `received_by_me`,
`spent_by_me`, and `my_balance_change`.

R39.8.0p Generated shielded transactions shall coordinate with the scanner so the
same wallet note is not selected concurrently by multiple sends or swaps. After
broadcast, the scanner shall keep following the transaction until it is either
scanned into the wallet database or no longer available from the backend. Change
outputs and outgoing spends shall become reflected in balance and
`z_coin_tx_history` only through the wallet database scan state.

R39.8.0q Unconfirmed shielded outputs are not spendable merely because the local
process created or observed them. Spendable balance, transaction history, and
post-swap Desktop display shall reflect confirmed wallet-db scan results, with
the existing locked-note/change tracking used only to prevent unsafe local
double-spends while waiting for confirmation.

### 39.8.1 Envelope, method string & platform gate

R39.8.1 The project shall expose a dedicated shielded-coin transaction-history
method with the wire method string `z_coin_tx_history`, dispatched over the
**mmrpc 2.0** envelope (`{"mmrpc":"2.0","method":"z_coin_tx_history",
"params":{...}}`, with the usual `userpass`). The result is returned in the
standard v2 `{"mmrpc":"2.0","result":{...}}` success envelope; errors use the v2
error envelope (`error`, `error_path`, `error_trace`, `error_type`,
`error_data`).

R39.8.2 The method is available on every platform where an activated ZCoin has
the shielded wallet database required by §39.8.0. It is resolved against the
activated coin named by `coin` and shall succeed only when that coin is an
activated shielded (ZCoin) coin; any other activated coin type is rejected (see
R39.8.6).

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

> **Shared error surface (corpus-faithful).** `z_coin_tx_history` uses the same
> public v2 transaction-history error discriminants and HTTP status mapping as
> the generic `my_tx_history` (v2) method. The HTTP statuses in the table above
> are therefore part of the shared public error contract
> (`CoinIsNotActive` → 404; `NotSupportedFor` and `InvalidTarget` → 400;
> `StorageIsNotInitialized`, `StorageError`, `RpcError`, and `Internal` → 500).
> The **published wire contract is the `error_type` discriminant names**; the
> HTTP integers are the shared public status mapping rather than separate
> per-method values.

> **Reloaded alignment.** Reloaded's shared transaction-history HTTP status
> mapping is aligned to the upstream values recorded in the table above
> (`CoinIsNotActive` → 404; `NotSupportedFor` → 400; `StorageIsNotInitialized`,
> `StorageError`, `RpcError` → 500). The `error_type` discriminant names are the
> published contract and remain unchanged.

### 39.8.5 Functional behaviour

R39.8.9 For an activated shielded coin with an initialized wallet database,
`z_coin_tx_history` shall return one requested page of shielded transaction
details derived from wallet scan state and confirmed transaction data. Each
entry shall expose the address sets, wallet-owned spent and received amounts,
net wallet balance change, fee, block height, timestamp, confirmation count
relative to the current backend tip, and the stable internal paging identifier.
The response shall also report `skipped`, `total`, and `total_pages` metadata
consistent with the stored shielded history and the effective paging request.

R39.8.9a The history page order shall be newest mined transaction first. For
transactions in the same block, ordering shall be stable by the wallet
database's transaction identifier. `PageNumber` paging skips
`(page_number - 1) * limit` entries in that order. `FromId` paging starts after
the entry identified by the supplied `internal_id` according to that same order;
an unknown `FromId` is a storage/history error rather than a silent empty page.

R39.8.9b The `received_by_me` amount is the sum of wallet-owned received notes
whose transaction is the history entry's transaction. The `spent_by_me` amount
is the sum of wallet-owned received notes whose spent-link points to the history
entry's transaction. `my_balance_change` is `received_by_me - spent_by_me`.

R39.8.9c The `from` address set shall include transparent input addresses that
can be recovered from previous transaction outputs and shall include the wallet's
own shielded address when the wallet spent shielded notes in the transaction. A
fully shielded incoming transaction from another wallet may have an empty `from`
set because the sender's shielded address is not generally recoverable.

R39.8.9d The `to` address set shall include transparent output addresses that
can be decoded from the transaction outputs, the wallet's own shielded address
when the wallet received shielded notes in the transaction, and shielded
addresses recoverable from outgoing viewing information for wallet-created
outputs such as change, swap, fee, or burn outputs.

R39.8.9e `transaction_fee` shall be computed from the transparent input value,
transparent output value, and the Sapling value balance of the full transaction.
`confirmations` shall be zero when the transaction height is above the backend
tip; otherwise it is `current_block + 1 - block_height`.

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

R39.8.11 `StorageIsNotInitialized` remains a valid error discriminant for
history methods when the relevant local history store is genuinely absent. For
`z_coin_tx_history`, however, a normal successful ZCoin activation initializes
the shielded wallet database. Therefore an activated ARRR/ZCoin that reached the
terminal activation state after wallet-db scanning shall not return
`StorageIsNotInitialized` from `z_coin_tx_history`; if the store cannot be
opened, the activation path shall fail or the history path shall return a
storage error that reflects the store failure.

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
- Shielded history activation: a Light-mode ARRR activation initializes both the
  compact-block cache and shielded wallet database, reports compact-block and
  wallet-db scan progress, and reaches terminal activation only after wallet-db
  scanning is complete through the activation tip (§39.8.0).
- ARRR post-swap display: after an LTC-to-ARRR swap pays the activated wallet's
  shielded address and the ARRR transaction is mined and scanned,
  `z_coin_tx_history` returns a transaction entry with positive
  `received_by_me` and `my_balance_change`, includes the wallet shielded address
  in `to`, reports `sync_status: Finished`, and does not return
  `StorageIsNotInitialized` (§39.8.0d, §39.8.9).
- Generic history separation: for an activated ZCoin, generic `my_tx_history`
  v2 rejects the coin as unsupported for that method; Desktop uses
  `z_coin_tx_history` for shielded history (§39.8.0c).
- `z_coin_tx_history` returns a paginated page of shielded transaction detail
  entries for an activated shielded coin, honours `limit` and both
  `PageNumber`/`FromId` paging modes, echoes paging metadata, reports
  `sync_status: Finished`, and rejects non-shielded coins (`NotSupportedFor`)
  and inactive coins (`CoinIsNotActive`) (§39.8).
