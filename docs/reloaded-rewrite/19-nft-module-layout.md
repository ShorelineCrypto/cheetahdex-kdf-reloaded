# Chapter 19 -- NFT Module Layout

**Status:** driving-spec

> **One-sentence claim:** the project provides an in-tree NFT
> subsystem that covers five EVM chains, exposes seven JSON-RPC
> methods (inventory, metadata, transfer history, withdrawal,
> wipe), embeds no third-party indexer hostnames, and abstracts
> both its outbound HTTP surface and its on-device storage
> behind narrow traits with native and browser implementations.

## 19.0 Executive Summary

An NFT subsystem provides on-device caching of ERC-721 and
ERC-1155 token inventories, transfer histories, and per-token
metadata across five EVM-compatible chain ecosystems
(Ethereum, BNB Smart Chain, Polygon, Avalanche, Fantom). The
subsystem is bounded by three architectural principles:

1. **No hardcoded indexer endpoints.** Every HTTP base URL used
   to crawl chain history or refresh per-token metadata is
   supplied by the caller at RPC time. The subsystem embeds no
   third-party indexer hostnames, vendor names, or default
   provider URLs of any kind.
2. **Trait-abstracted persistence with two independent
   implementations.** Two narrow async traits cover the
   inventory and history surfaces; each is implemented twice
   (once for the native SQL backend, once for the browser
   IndexedDB backend) with no shared concrete type.
3. **Pluggable provider traits.** Two narrow async traits
   isolate the crawling and metadata-refresh logic from any
   specific HTTP wire shape, so additional providers (self-
   hosted indexer, signed-proxy indexer, mock test indexer)
   can be wired in without touching either the RPC handlers or
   the persistence layer.

The subsystem at the time of writing supports keypair-derived
withdrawal signing only; HD-wallet and hardware-wallet
withdrawal, browser-side incremental crawling, and live
confirmation counts are explicitly named as deferred work in
§19.9.

## 19.1 Subsystem Shape

The subsystem is grouped into the following functional regions:

| Region                  | Responsibility                                  |
|-------------------------|-------------------------------------------------|
| Public surface          | Per-context handle, re-exports                  |
| Errors                  | Per-operation error enums                       |
| Models                  | Chain enum, NFT object, transfer record,        |
|                         | metadata, RPC request payloads, withdraw payload|
| Storage trait surface   | Inventory trait, history trait, error trait     |
| Storage -- native       | SQL backend, per-chain table pair               |
| Storage -- browser      | IndexedDB backend, per-chain object-store pair  |
| Providers -- crawl      | Trait + HTTP implementation                     |
| Providers -- metadata   | Trait + HTTP implementation                     |
| Providers -- spam       | Caller-supplied domain-list filtering           |
| RPC handlers            | Seven JSON-RPC entry points (native; selected   |
|                         | are stubbed on the browser target)              |
| Withdraw                | ERC-721 / ERC-1155 calldata encoding & signing  |

The subsystem must not contain a separate sibling persistence
crate; both backend implementations live as submodules of the
storage region within the subsystem itself. This is a binding
layout rule.

## 19.2 Public Handle

The subsystem exposes a single handle type obtained via the
per-context lazy-init pattern of
[Chapter 8](08-mm-ctx-and-state-layering.md). Call sites
acquire the handle from the central context; they do not
construct it directly. The handle is the only path through
which the RPC handlers, the withdrawal flow, and any future
external consumer reach the persistence layer.

The browser target compiles out the RPC handlers and the
withdrawal flow at the module level via target-architecture
guards; the rest of the public surface is identical across
targets.

## 19.3 Chains and Tickers

The chain set is modelled as a closed enum with five variants,
serialised as upper-case strings:

| Variant value (serialised) | Platform-coin ticker | Chain ecosystem            |
|----------------------------|----------------------|-----------------------------|
| `AVALANCHE`                | `AVAX`               | Avalanche C-Chain           |
| `BSC`                      | `BNB`                | BNB Smart Chain             |
| `ETH`                      | `ETH`                | Ethereum mainnet            |
| `FANTOM`                   | `FTM`                | Fantom Opera                |
| `POLYGON`                  | `MATIC`              | Polygon PoS                 |

A small chain-ticker trait maps each variant to:

- The platform-coin ticker used elsewhere in the codebase.
- A table-name prefix used by the native backend
  (`NFT_AVAX_`, `NFT_BNB_`, `NFT_ETH_`, `NFT_FTM_`, `NFT_MATIC_`).

Per-chain prefixes are a deliberate choice: a chain-scoped wipe
is two `DROP TABLE` statements and one row delete from the
chain-progress table.

Adding a new EVM chain is a closed additive operation:
appending an enum variant and a chain-ticker mapping is
sufficient; everything downstream (RPC dispatch, withdrawal
path, storage table layout) flows from the enum.

## 19.4 Storage Layer

### 19.4.1 Trait Surface

Two object-safe async traits abstract the backend:

- An **inventory** trait covering the set of tokens currently
  owned by the active address per chain. Its surface includes
  per-chain ready-state ensure/check, bulk register of newly
  observed tokens, paginated listing with optional filters,
  per-token fetch, per-token drop, per-chain purge, full purge,
  balance bookkeeping, last-scanned-block bookkeeping,
  per-contract enumeration, contract-level spam marking,
  external-domain enumeration, and domain-level phishing
  marking.
- A **history** trait covering every observed transfer touching
  the active address per chain. Its surface includes bulk
  append, paginated listing with filters, last-transfer-block
  lookup, since-block filtering, per-token transfer listing,
  per-log lookup, metadata-attachment bulk update,
  missing-metadata enumeration, per-contract transfer
  enumeration, contract-level spam marking, contract-address
  enumeration, domain enumeration, domain-level phishing
  marking, per-chain purge, and full purge.

Both traits carry an associated error type bounded by a shared
storage-error trait so that calling code can be backend-agnostic.

Inventory and history are deliberately on independent code
paths: a write to one cannot corrupt the other, and a clear of
one does not affect the other. The split is a binding rule.

### 19.4.2 Native Backend Schema

Per chain, the native backend maintains two tables. Schema
illustrated for the Ethereum variant; the other four chains
have identical column layouts under the chain-specific prefix.

```sql
CREATE TABLE IF NOT EXISTS NFT_ETH_inventory (
    token_address     TEXT    NOT NULL,
    token_id_str      TEXT    NOT NULL,
    block_number      INTEGER NOT NULL,
    possible_spam     INTEGER NOT NULL DEFAULT 0,
    possible_phishing INTEGER NOT NULL DEFAULT 0,
    contract_type     TEXT    NOT NULL,
    image_domain      TEXT,
    animation_domain  TEXT,
    external_domain   TEXT,
    payload           TEXT    NOT NULL,
    PRIMARY KEY (token_address, token_id_str)
);

CREATE TABLE IF NOT EXISTS NFT_ETH_transfers (
    transaction_hash  TEXT    NOT NULL,
    log_index         INTEGER NOT NULL,
    token_id_str      TEXT    NOT NULL,
    token_address     TEXT    NOT NULL,
    block_number      INTEGER NOT NULL,
    block_timestamp   INTEGER NOT NULL,
    possible_spam     INTEGER NOT NULL DEFAULT 0,
    possible_phishing INTEGER NOT NULL DEFAULT 0,
    status            TEXT    NOT NULL,
    token_domain      TEXT,
    image_domain      TEXT,
    payload           TEXT    NOT NULL,
    PRIMARY KEY (transaction_hash, log_index, token_id_str)
);
```

A single global table tracks per-chain crawl progress:

```sql
CREATE TABLE IF NOT EXISTS nft_chain_progress (
    chain              TEXT PRIMARY KEY,
    last_scanned_block INTEGER NOT NULL DEFAULT 0
);
```

Schema rationale:

- The `payload` column stores the full token or transfer object
  as JSON. Scalar columns are extracted only as far as is
  needed to drive pagination, filtering, and spam masks. New
  optional fields in the model can be added without a schema
  migration.
- The `*_domain` columns store the parsed host of the relevant
  URL field, allowing domain-list lookups to run as a single
  SQL filter rather than a per-row check.
- Per-chain table prefixes make a chain-scoped wipe two
  `DROP TABLE` statements plus a row delete from the progress
  table.

### 19.4.3 Browser Backend Schema

The browser backend mirrors the native schema in IndexedDB
object stores: each chain receives its own pair of stores
(inventory and transfers) carrying the same fields, with
indexes that mirror the native backend's scalar columns so the
same pagination and filtering queries can be expressed.

The two backends do **not** share a base implementation: the
abstraction lives entirely above them through the async traits.

## 19.5 Provider Layer

### 19.5.1 Crawl Provider

A **crawl provider** trait abstracts the inventory and
transfer-history HTTP surface. Its three operations are:

- **Latest block** for a given chain.
- **Transfers** for a given owner since a given block on a
  given chain, returning a list of transfer records.
- **Token detail** for a given owner / contract / token-id
  triple on a given chain, returning a single inventory entry.

The bundled HTTP implementation is parameterised by a base URL
supplied at construction time and a boolean flag reserved for
signed-proxy operation (not yet wired). Its endpoint shapes are:

| Operation        | Path template                                       |
|------------------|-----------------------------------------------------|
| Latest block     | `GET {base}/<chain>/block/latest`                   |
| Transfers since  | `GET {base}/<chain>/<owner>/transfers?from_block=<n>` |
| Token detail     | `GET {base}/<chain>/<owner>/<contract>/<token_id>`  |

Latest-block responses are objects shaped `{ "block": <u64> }`.

### 19.5.2 Metadata Provider

A **metadata provider** trait abstracts the per-token
metadata-refresh HTTP surface. Its single operation refreshes
the metadata for a given chain / contract / token-id triple,
returning the freshly fetched inventory entry. The bundled HTTP
implementation issues
`GET {base}/<chain>/<contract>/<token_id>` and is again
parameterised by a base URL supplied at construction time plus
the same reserved signed-proxy flag.

The RPC handler that drives a metadata refresh merges the
returned object into persistence through the inventory trait's
bulk register operation plus an in-place merge of the URL
fields.

### 19.5.3 Spam and Phishing

The subsystem applies spam and phishing flags to inventory
entries by checking the token's image, animation, and external
domain fields against **caller-supplied** URL lists. The lists
themselves are passed at RPC time on a per-call basis; the
subsystem embeds none and ships none. The flags are stored as
scalar columns on each inventory row and are used purely for
client-side filtering and display masking.

## 19.6 RPC Wire Surface

The subsystem registers seven JSON-RPC methods in the public
dispatcher. The native target supports all seven; the browser
target supports five (with two stubbed as not-yet-supported).

| Method                  | Native | Browser | Returns                  |
|-------------------------|--------|---------|--------------------------|
| `get_nft_list`          | yes    | yes     | Paginated inventory list |
| `get_nft_metadata`      | yes    | yes     | Single inventory entry   |
| `get_nft_transfers`     | yes    | yes     | Paginated transfer list  |
| `refresh_nft_metadata`  | yes    | yes     | empty success            |
| `clear_nft_db`          | yes    | yes     | empty success            |
| `update_nft`            | yes    | stub    | empty success (crawl)    |
| `withdraw_nft`          | yes    | -       | Transaction details      |

Request payloads:

- `get_nft_list` -- chains, max-flag, page size, page number,
  spam-protection flag, optional filters.
- `get_nft_metadata` -- chain, token address, token id,
  spam-protection flag.
- `get_nft_transfers` -- chains, filters, max-flag, page size,
  page number, spam-protection flag.
- `refresh_nft_metadata` -- chain, token address, token id,
  metadata-provider base URL, spam-list base URL, signed-proxy
  flag.
- `update_nft` -- chains, crawl-provider base URL, spam-list
  base URL, signed-proxy flag.
- `clear_nft_db` -- chain set plus a `clear_all` flag; when
  `clear_all` is true the chain set is ignored and the wipe
  spans all chains.
- `withdraw_nft` -- a tagged union over the two NFT standards:
  - ERC-721 variant carries chain, recipient, token address,
    token id, optional fee override.
  - ERC-1155 variant additionally carries an optional amount
    and a max-balance drain flag.

The withdraw request is the only payload whose shape varies per
token standard.

## 19.7 EVM Withdrawal Path

The withdrawal module embeds minimal copies of the two on-chain
interface definitions (ABI fragments) needed to encode calldata:

```jsonc
// ERC-721 transferFrom
[{"inputs":[{"name":"from","type":"address"},
            {"name":"to","type":"address"},
            {"name":"tokenId","type":"uint256"}],
  "name":"transferFrom","outputs":[],
  "stateMutability":"nonpayable","type":"function"}]

// ERC-1155 safeTransferFrom + balanceOf
[{"inputs":[{"name":"from","type":"address"},
            {"name":"to","type":"address"},
            {"name":"id","type":"uint256"},
            {"name":"value","type":"uint256"},
            {"name":"data","type":"bytes"}],
  "name":"safeTransferFrom","outputs":[],
  "stateMutability":"nonpayable","type":"function"},
 {"inputs":[{"name":"account","type":"address"},
            {"name":"id","type":"uint256"}],
  "name":"balanceOf","outputs":[{"name":"","type":"uint256"}],
  "stateMutability":"view","type":"function"}]
```

These fragments are the public on-chain interface definitions
of ERC-721 and ERC-1155; they are deployed-contract
identifiers, not authorial material.

The flow is:

1. Resolve the EVM platform coin handle for the requested
   chain's ticker. Anything that is not a plain EVM platform
   coin is rejected; the codebase does not implement EVM-style
   NFT withdrawal for TRON-family chains and returns an
   explicit error to that effect.
2. Encode the calldata using a generic Ethereum ABI encoder
   over the embedded fragment for the selected method.
3. For the ERC-1155 variant, if the max-balance drain flag is
   set, query `balanceOf(owner, id)` via an `eth_call` and use
   the returned balance as the transfer amount.
4. Resolve gas using either the caller's explicit fee override
   or an estimate against the encoded calldata.
5. Resolve nonce using the platform coin's nonce-resolution
   path.
6. Build and sign with the keypair-derived signing path
   appropriate to the platform coin's fee policy (legacy or
   EIP-1559).
7. Return transaction details with fee values denominated in
   the platform coin (`ETH`, `BNB`, `MATIC`, `AVAX`, `FTM`).

The withdrawal path supports keypair-derived signing only.
HD-wallet and hardware-wallet signing for NFT withdrawal are
deliberately deferred (§19.9).

## 19.8 Tests

Unit tests are colocated with each region. The unit-test set at
the time of writing covers:

- Native storage: per-chain ensure, bulk register, pagination,
  filtering, chain-scoped clear.
- Browser storage: smoke tests over the IndexedDB backend.
- Pagination helper: boundary conditions on the shared
  pagination utility.
- Withdraw payload: serde round-trip over the tagged union,
  including optional fee and amount shapes.
- Metadata model: in-place merge semantics for URL fields.
- HTTP error classification.

End-to-end integration tests against a live indexer are not in
the test set at the time of writing; they are named as
follow-on work once a deterministic local test fixture for the
EVM activation surface is available.

## 19.9 Binding Requirements and Deferred Work

The following are **binding rules** for this subsystem:

R1. **No embedded indexer endpoints.** The subsystem shall
    embed no third-party indexer hostnames, vendor names, or
    default provider URLs of any kind. All HTTP base URLs used
    by the crawl and metadata providers are caller-supplied at
    RPC time.

R2. **No embedded domain lists.** The subsystem shall embed no
    spam-domain or phishing-domain lists. The lists are
    caller-supplied at RPC time and apply per call.

R3. **Trait-abstracted storage with two independent
    implementations.** The two persistence backends (native SQL
    and browser IndexedDB) shall not share a base
    implementation; the abstraction shall live entirely above
    them via the inventory and history traits.

R4. **Inventory / history independence.** The inventory and
    history paths shall remain on independent code paths and
    independent schemas so that a write to one cannot corrupt
    the other and a clear of one does not affect the other.

R5. **Closed chain enum.** The five-element chain enum is the
    single registration point for an EVM chain to gain NFT
    support; everything downstream (RPC dispatch, table
    prefixes, withdrawal path) shall flow from the enum.

R6. **Provider pluggability.** The crawl and metadata HTTP
    surfaces shall remain behind their respective traits so
    that alternative providers (signed-proxy, self-hosted,
    mock) can be substituted without changes to the RPC
    handlers or the persistence layer.

R7. **Standards-only ABI fragments.** The withdrawal path's
    embedded ABI fragments shall be the public ERC-721 and
    ERC-1155 on-chain interface definitions and nothing more.

The following are **deferred work** named explicitly in scope
of this chapter:

D1. **Browser-side incremental crawl.** The crawl operation is
    currently stubbed on the browser target. The crawl logic
    itself is target-agnostic; the missing piece is a
    long-running task harness on the browser target that does
    not block the host event loop.

D2. **HD-wallet and hardware-wallet NFT withdrawal.** The
    withdrawal path supports keypair-derived signing only.
    Adding HD-wallet and hardware-wallet signing requires
    threading the derivation path and address index through
    the withdraw request payload and the signing flow.

D3. **Live confirmation count.** The inventory stores the
    block at which each token was last observed; surfacing a
    live confirmation count requires either periodic
    re-observation or a streaming subscription
    ([Chapter 10](10-sse-streaming.md)).

D4. **Signed-proxy provider operation.** Both bundled HTTP
    providers carry a reserved signed-proxy flag intended to
    sign outbound HTTP with the codebase's signed-proxy scheme
    (see [Chapter 23](23-signed-proxy.md) for the scheme and
    [Chapter 24](24-libp2p-handshake.md) for the keying);
    wiring is deferred to that subsystem's integration step.

D5. **End-to-end integration tests** against a deterministic
    indexer fixture (§19.8).

## 19.10 External References

- The ERC-721 standard (the on-chain interface used by the
  withdrawal path's `transferFrom`).
- The ERC-1155 standard (the on-chain interface used by the
  withdrawal path's `safeTransferFrom` and `balanceOf`).
- The Ethereum JSON-RPC `eth_call` method (used for the
  `balanceOf` query on the ERC-1155 max-balance drain path).
- The EIP-1559 transaction format (one of the two signing
  policies selected by the platform coin's fee policy).
- The EVM chain ecosystems named in §19.3 (Ethereum, BNB Smart
  Chain, Polygon, Avalanche, Fantom) and their respective
  platform-coin tickers.

## 19.11 Baseline Verifications

The following are verifiable from the baseline state defined in
[Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains **no** NFT subsystem of the
    shape described in this chapter. A tree-wide
    `git grep -l '^pub.*nft\|NftListStore\|NftHistoryStore\|NftCrawlProvider'`
    against the baseline returns no matches; a directory
    listing of the baseline tree
    (`git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f`)
    contains no path containing `nft` as a directory component.

V2. The baseline tree contains **no** sibling NFT storage
    crate. The single-subsystem layout rule in §19.1 is
    consistent with the baseline state: no NFT material exists
    at baseline at all.

V3. The five chain variants in §19.3 correspond to the
    publicly-documented EVM ecosystems of the same names. The
    platform-coin tickers (`AVAX`, `BNB`, `ETH`, `FTM`,
    `MATIC`) are the publicly-deployed mainnet ticker symbols
    of the respective ecosystems.

V4. The ABI fragments embedded in §19.7 are byte-identical to
    the publicly-published ERC-721 and ERC-1155 on-chain
    interface definitions for the methods named. They are
    on-chain interface identifiers, not authorial material.

## 19.12 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  NFT subsystem at baseline verified via
  `git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the storage-trait names against
  the baseline; ERC-721 and ERC-1155 standards (the on-chain
  interface definitions used by the withdrawal path); the
  Ethereum JSON-RPC method `eth_call` (used for the ERC-1155
  balance query); EIP-1559 (one of the two signing policies);
  publicly-documented mainnet ticker symbols of the five EVM
  ecosystems named in §19.3.
- *Forbidden corpus:* not consulted.
