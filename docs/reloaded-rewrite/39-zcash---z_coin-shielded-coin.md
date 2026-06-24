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

---

## Part C -- Shielded transaction-history RPC (T-DOC)

## 39.8 `z_coin_tx_history` method

> **Source-of-truth note.** The wire contract below (method string, request and
> response field names/types, error variants) is the externally dictated public
> RPC interface; the authoritative reference is the Komodo DeFi Framework API
> documentation for `z_coin_tx_history`. Behaviour is specified abstractly.

> **Implementation status (reloaded): BLOCKED ON SUBSTRATE.** This wire
> contract is specified ahead of implementation. The reloaded `ZCoin` is a
> native-full-node port whose local SQLite store holds only the shielded-tx
> *creation* cache; there is no shielded transaction-history store, no
> compact-block scanner, and no stable signed-64-bit `internal_id` keyspace to
> page over. Satisfying R39.8.9 / §39.7 first requires porting a shielded
> wallet-history store (zcash light-client `WalletDb` + scanner) and adding the
> shared `target` field to the v2 request envelope. Until that substrate exists,
> `z_coin_tx_history` is classified greenfield-dependent (Phase 4-adjacent), not
> a routing/aliasing gap.

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

### 39.8.5 Functional behaviour

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
- R39.6.1 and R39.6.2 remain pending ports.
- `z_coin_tx_history` (mmrpc 2.0, native-only) returns a paginated page of
  shielded transaction detail entries for an activated shielded coin, honours
  `limit` and both `PageNumber`/`FromId` paging modes, echoes paging metadata,
  reports `sync_status: Finished`, and rejects non-shielded coins
  (`NotSupportedFor`) and inactive coins (`CoinIsNotActive`) (§39.8).
