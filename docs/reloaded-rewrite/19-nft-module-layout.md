# Chapter 19 — NFT Module Layout

> **Chapter type:** document existing. No IMPL marker.

## 19.0 Executive summary

The NFT subsystem under
[`mm2src/coins/nft/`](../../mm2src/coins/nft/) is a clean-room
addition to reloaded; the GPLv2 baseline at `c1d46c0` did not ship
any NFT code. It supports five EVM-compatible chains
(Ethereum, BSC, Polygon, Avalanche, Fantom) and exposes seven
JSON-RPC methods covering inventory listing, metadata refresh,
transfer history, and ERC-721 / ERC-1155 withdrawal.

The design follows three principles that distinguish it from the
upstream codebase:

1. **No hardcoded third-party services.** All HTTP endpoints
   used to crawl chain history or refresh per-token metadata are
   supplied by the caller at RPC time as base URLs. The reloaded
   tree contains no embedded references to Moralis, OpenSea, or
   any other commercial NFT API.
2. **Trait-abstracted storage with native + WASM backends.**
   Two narrow async traits (`NftListStore`, `NftHistoryStore`)
   are implemented by SQLite (native) and IndexedDB (WASM) with
   no shared concrete type.
3. **Pluggable provider traits** (`NftCrawlProvider`,
   `MetadataProvider`) decouple the inventory/refresh logic from
   any specific HTTP shape, so future providers (self-hosted
   indexer, Komodo proxy-signed indexer, etc.) can drop in
   without touching the RPC handlers or the storage layer.

This chapter documents the module layout, public surface,
storage schema, provider interfaces, RPC wire surface, and
current limitations.

## 19.1 Directory layout

```
mm2src/coins/nft/
|-- mod.rs                     Public re-exports
|-- context.rs                 NftCtx (lazy per-MmArc init)
|-- errors.rs                  GetNftInfoError, UpdateNftError, ...
|-- rpc.rs                     7 RPC handlers (native; WASM stubs)
|-- withdraw.rs                ERC-721/1155 calldata + signing
|-- serde_helpers.rs           BigUint / token-id serialisers
|-- model/
|   |-- mod.rs                 Re-exports
|   |-- chain.rs               Chain enum, ChainTicker trait
|   |-- nft.rs                 Nft, NftCommon, NftList
|   |-- transfer.rs            NftTransfer, TransferStatus
|   |-- metadata.rs            UriMeta
|   |-- request.rs             RPC request payloads
|   `-- withdraw.rs            WithdrawErc721 / WithdrawErc1155
|-- providers/
|   |-- mod.rs
|   |-- crawler.rs             NftCrawlProvider trait + HttpCrawlProvider
|   |-- http.rs                FetchError, fetch_json
|   |-- refresh.rs             MetadataProvider trait + HttpMetadataProvider
|   |-- spam.rs                Spam / phishing heuristics
|   `-- url_helpers.rs         domain_of, normalise_metadata_urls
`-- store/
    |-- mod.rs                 Public traits, paginate helper
    |-- errors.rs              NftStoreError, RemoveOutcome
    |-- list.rs                NftListStore trait
    |-- history.rs             NftHistoryStore trait
    |-- sqlite/                Native impl
    |   |-- mod.rs             SqliteNftStore
    |   |-- list.rs
    |   |-- history.rs
    |   |-- schema.rs          CREATE TABLE statements
    |   `-- tests.rs
    `-- idb/                   WASM impl
        |-- mod.rs             IndexedDbNftStore
        |-- list.rs
        |-- history.rs
        |-- schema.rs
        `-- tests.rs
```

No separate `mm2src/coins/nft_storage/` crate exists in reloaded;
both backends live as sub-modules of `nft::store`.

## 19.2 Public surface (`nft/mod.rs`)

The module is gated only by `target_arch` for the RPC and
withdraw sub-modules:

```rust
pub mod context;
pub mod errors;
pub mod model;
pub mod providers;
#[cfg(not(target_arch = "wasm32"))]
pub mod rpc;
pub mod serde_helpers;
pub mod store;
#[cfg(not(target_arch = "wasm32"))]
pub mod withdraw;

pub use context::NftCtx;
pub use errors::{ClearNftDbError, GetNftInfoError, /* ... */};
pub use model::{Chain, Nft, NftInfo, NftList, NftTransfer, /* ... */};
pub use providers::{apply_spam_protection_to_nft, /* ... */};
pub use store::{NftHistoryStore, NftListStore, /* ... */};
```

`NftCtx` is the single entry point a caller needs to reach the
storage layer. It is lazy-initialised per `MmArc` via the
`from_ctx` helper pattern used throughout the codebase (see
[Chapter 8](08-mm-ctx-and-state-layering.md)).

## 19.3 Chains and tickers

[`nft/model/chain.rs`](../../mm2src/coins/nft/model/chain.rs)
defines:

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Chain {
    Avalanche,
    Bsc,
    Eth,
    Fantom,
    Polygon,
}
```

The `ChainTicker` trait maps the enum to:

- `coin_ticker()` -- platform-coin ticker on the kdf side
  (e.g. `"AVAX"`, `"BNB"`, `"ETH"`, `"FTM"`, `"MATIC"`).
- `nft_table_prefix()` -- SQLite table prefix
  (e.g. `"NFT_AVAX_"`, `"NFT_BNB_"`, ...). The native backend
  creates a separate inventory + transfers table per chain so
  per-chain wipes are simple `DROP TABLE` operations.

The chain list is the only place new EVM chains need to be
registered to gain NFT support; everything downstream
(`Chain` -> ticker, table prefix, RPC dispatch, withdraw)
flows from the enum.

## 19.4 Storage layer

### 19.4.1 Public traits

Two async object-safe traits abstract the backend
([`store/list.rs`](../../mm2src/coins/nft/store/list.rs),
[`store/history.rs`](../../mm2src/coins/nft/store/history.rs)).
Both carry an associated `Error: NftStoreError` and use
`MmResult<_, Self::Error>` throughout. A representative cut of
`NftListStore` (full trait is wider):

```rust
#[async_trait]
pub trait NftListStore {
    type Error: NftStoreError;

    async fn ensure_chain(&self, chain: &Chain)
        -> MmResult<(), Self::Error>;
    async fn chain_ready(&self, chain: &Chain)
        -> MmResult<bool, Self::Error>;
    async fn register_owned(
        &self,
        chain: Chain,
        items: Vec<Nft>,
        last_scanned_block: u64,
    ) -> MmResult<(), Self::Error>;
    async fn list_owned(
        &self,
        chains: Vec<Chain>,
        take_all: bool,
        page_size: usize,
        page: Option<NonZeroUsize>,
        filters: Option<NftListFilters>,
    ) -> MmResult<NftList, Self::Error>;
    async fn fetch_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
    ) -> MmResult<Option<Nft>, Self::Error>;
    async fn drop_token(
        &self,
        chain: &Chain,
        token_address: String,
        token_id: BigUint,
        scanned_block: u64,
    ) -> MmResult<(), Self::Error>;
    async fn purge_chain(&self, chain: &Chain)
        -> MmResult<(), Self::Error>;
    async fn purge_all(&self) -> MmResult<(), Self::Error>;
    /* plus: token_balance, merge_metadata, latest_block_in_cache,
       latest_scanned_block, set_token_amount(_and_block),
       tokens_for_contract, mark_contract_spam,
       list_external_domains, mark_domain_phishing */
}
```

`NftHistoryStore` follows the same shape with
`append_transfers(chain, Vec<NftTransfer>)`,
`list_transfers(...)`,
`latest_transfer_block(chain) -> Option<u64>`,
`transfers_since`, `transfers_for_token`,
`transfer_by_log`, `attach_metadata_to_transfers`,
`transfers_missing_metadata`, `transfers_for_contract`,
`mark_contract_spam`, `contract_addresses`, `domain_set`,
`mark_domain_phishing`, `purge_chain`, `purge_all`.

The split keeps the inventory (what tokens are currently owned)
and the history (every transfer touching the account) on
independent code paths -- a write to one cannot corrupt the
other and a clear of one does not affect the other. Both traits
are implemented twice, once for SQLite and once for IndexedDB,
with no shared concrete base.

### 19.4.2 SQLite backend

Per chain, two tables (illustrated for ETH):

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

Plus one global table:

```sql
CREATE TABLE IF NOT EXISTS nft_chain_progress (
    chain              TEXT PRIMARY KEY,
    last_scanned_block INTEGER NOT NULL DEFAULT 0
);
```

Design notes:

- `payload` holds the full serialised `Nft` / `NftTransfer` as
  JSON. Indexed scalars (block height, spam flags, status,
  domain) are extracted only as far as needed to drive
  pagination, filtering, and spam masks. New optional fields can
  be added to the model without a schema migration.
- The `*_domain` columns store the parsed host of the relevant
  URL field; spam-list lookups can run as a single SQL filter
  rather than a per-row Rust check.
- Per-chain table prefixes mean `clear_nft_db(chain = Eth)` is
  two `DROP TABLE` statements and a row delete from
  `nft_chain_progress`.

### 19.4.3 IndexedDB backend

The WASM backend
([`store/idb/`](../../mm2src/coins/nft/store/idb/)) mirrors the
SQLite schema in IndexedDB object stores, built on top of
[`mm2_db::indexed_db::ConstructibleDb`](../../mm2src/mm2_db/src/indexed_db/).
Each chain gets its own pair of stores; indexes mirror the
SQLite scalar columns so the same pagination and filtering
queries can be expressed.

The two backends do *not* share a base implementation; the
abstraction lives entirely above them via the async traits.

## 19.5 Provider layer

Two small traits in
[`nft/providers/`](../../mm2src/coins/nft/providers/) shape the
outward HTTP surface.

### 19.5.1 `NftCrawlProvider`

```rust
#[async_trait]
pub trait NftCrawlProvider: Send + Sync {
    async fn latest_block(&self, chain: Chain) -> Result<u64>;
    async fn transfers(
        &self,
        chain: Chain,
        owner: &str,
        from_block: u64,
    ) -> Result<Vec<NftTransfer>>;
    async fn token(
        &self,
        chain: Chain,
        owner: &str,
        contract: &str,
        token_id: &str,
    ) -> Result<Nft>;
}
```

The bundled `HttpCrawlProvider` is parameterised by a base URL
and a `komodo_proxy: bool` flag (reserved; not yet wired).
Endpoints follow a simple path shape:

- `GET {base}/<chain>/block/latest` -> `{ "block": <u64> }`
- `GET {base}/<chain>/<owner>/transfers?from_block=<n>` -> list
- `GET {base}/<chain>/<owner>/<contract>/<token_id>` -> single
  token

The implementation is in
[`providers/crawler.rs`](../../mm2src/coins/nft/providers/crawler.rs);
HTTP plumbing and error classification live in
[`providers/http.rs`](../../mm2src/coins/nft/providers/http.rs).

### 19.5.2 `MetadataProvider`

```rust
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    async fn refresh(
        &self,
        chain: Chain,
        contract: &str,
        token_id: &str,
    ) -> Result<Nft>;
}
```

`HttpMetadataProvider { base_url, komodo_proxy }` issues
`GET {base}/<chain>/<contract>/<token_id>` and returns the
freshly fetched `Nft`. The RPC handler then merges the metadata
into storage via `register_owned` + `UriMeta::merge_in`.

### 19.5.3 Spam protection

`apply_spam_protection_to_nft(nft, spam_list, phishing_list)`
([`providers/spam.rs`](../../mm2src/coins/nft/providers/spam.rs))
mutates the `possible_spam` / `possible_phishing` flags by
checking the token's `image_domain` / `animation_domain` /
`external_domain` against caller-supplied URL lists. The lists
themselves are supplied at RPC time; the module does not embed
any.

## 19.6 RPC wire surface

Dispatcher wiring
([`mm2_main/src/rpc/dispatcher/dispatcher.rs`](../../mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs)):

```rust
use coins::nft::rpc::{
    clear_nft_db, get_nft_list, get_nft_metadata,
    get_nft_transfers, refresh_nft_metadata, update_nft,
    withdraw_nft,
};
```

Dispatch arms:

| Method                  | Native | WASM | Returns                  |
|-------------------------|--------|------|--------------------------|
| `get_nft_list`          | y      | y    | `NftList` (paginated)    |
| `get_nft_metadata`      | y      | y    | `Nft`                    |
| `get_nft_transfers`     | y      | y    | `NftTransferList`        |
| `refresh_nft_metadata`  | y      | y    | `()`                     |
| `clear_nft_db`          | y      | y    | `()`                     |
| `update_nft`            | y      | stub | `()` (incremental crawl) |
| `withdraw_nft`          | y      | -    | `TransactionDetails`     |

Request payloads live in
[`nft/model/request.rs`](../../mm2src/coins/nft/model/request.rs)
and
[`nft/model/withdraw.rs`](../../mm2src/coins/nft/model/withdraw.rs):

- `NftListReq { chains, max, limit, page_number, protect_from_spam, filters }`
- `NftMetadataReq { chain, token_address, token_id, protect_from_spam }`
- `NftTransfersReq { chains, filters, max, limit, page_number, protect_from_spam }`
- `RefreshMetadataReq { chain, token_address, token_id, url, url_antispam, komodo_proxy }`
- `UpdateNftReq { chains, url, url_antispam, komodo_proxy }`
- `ClearNftDbReq { chains: Vec<Chain>, clear_all: bool }`
  -- when `clear_all` is true, `chains` is ignored
- `WithdrawNftReq` -- tagged union:
  - `WithdrawErc721 { chain, to, token_address, token_id, fee? }`
  - `WithdrawErc1155 { chain, to, token_address, token_id, amount?, max, fee? }`

The `WithdrawNftReq` enum is the only RPC payload that varies
its shape per token standard; ERC-1155 carries the extra
`amount` and `max` (drain-balance) fields.

## 19.7 EVM withdrawal path

[`nft/withdraw.rs`](../../mm2src/coins/nft/withdraw.rs) ships
minimal vendored ABI strings for the two methods needed:

```rust
const ERC721_ABI: &str = r#"[
    {"inputs":[{"name":"from","type":"address"},
               {"name":"to","type":"address"},
               {"name":"tokenId","type":"uint256"}],
     "name":"transferFrom","outputs":[],
     "stateMutability":"nonpayable","type":"function"}
]"#;

const ERC1155_ABI: &str = r#"[
    {"inputs":[{"name":"from","type":"address"},
               {"name":"to","type":"address"},
               {"name":"id","type":"uint256"},
               {"name":"value","type":"uint256"},
               {"name":"data","type":"bytes"}],
     "name":"safeTransferFrom","outputs":[],
     "stateMutability":"nonpayable","type":"function"},
    {"inputs":[{"name":"account","type":"address"},
               {"name":"id","type":"uint256"}],
     "name":"balanceOf","outputs":[{"name":"","type":"uint256"}],
     "stateMutability":"view","type":"function"}
]"#;
```

The flow is:

1. `resolve_eth_coin(ctx, chain_ticker)` -> `EthCoin` via
   `lp_coinfind`. Anything that is not a plain `EthCoin` is
   rejected (TRON family chains return
   `"TRON family chains do not implement EVM NFT withdraws"`).
2. Encode calldata using `ethabi::Contract::load` +
   `function(...).encode_input(args)`.
3. For ERC-1155, if `max == true`, query `balanceOf(owner, id)`
   via `eth_call` and use the full balance.
4. Resolve gas: explicit fee override -> `estimate_gas` against
   the encoded calldata.
5. Resolve `nonce` via `EthCoin::get_addr_nonce`.
6. Build and sign with `UnSignedEthTx::sign(secret_key, chain_id)`
   (legacy or EIP-1559 path per the platform coin's policy).
7. Return `TransactionDetails` with fee values denominated in
   the platform coin (ETH / BNB / MATIC / AVAX / FTM).

Today the path supports only keypair-derived signing. HD-wallet
and hardware-wallet (Trezor / Ledger) signing for NFT withdraw
are deliberately deferred.

## 19.8 Tests

Unit tests are colocated with each module:

- [`store/sqlite/tests.rs`](../../mm2src/coins/nft/store/sqlite/tests.rs)
  -- tokio tests for `ensure_chain`, register, pagination,
  filtering, clear.
- [`store/idb/tests.rs`](../../mm2src/coins/nft/store/idb/tests.rs)
  -- `wasm-bindgen-test` smoke tests for the IndexedDB backend.
- [`store/mod.rs`](../../mm2src/coins/nft/store/mod.rs) -- tests
  for the `paginate` helper.
- [`model/withdraw.rs`](../../mm2src/coins/nft/model/withdraw.rs)
  -- serde round-trip tests for the tagged union, including
  optional `fee` and `amount` shapes.
- [`model/metadata.rs`](../../mm2src/coins/nft/model/metadata.rs)
  -- `UriMeta::merge_in` semantics.
- [`providers/http.rs`](../../mm2src/coins/nft/providers/http.rs)
  -- `FetchError` classification tests.

End-to-end integration tests against a live indexer are not
present today; the test plan calls them out as a follow-up once
the activation layer for EVM platform coins exposes a
deterministic local test fixture.

## 19.9 Known limitations and deferred work

The module is feature-complete for its current scope (five EVM
chains, ERC-721 / ERC-1155, caller-supplied indexer). Items
explicitly deferred:

1. **WASM `update_nft`.** Returns "not yet supported on the
   WASM target"
   ([`nft/rpc.rs`](../../mm2src/coins/nft/rpc.rs)). The crawler
   logic is platform-agnostic; the missing piece is wiring a
   long-running task harness on WASM that does not block the JS
   event loop.
2. **HD-wallet and hardware-wallet NFT withdraw.** Only
   keypair-derived signing is supported in
   [`nft/withdraw.rs`](../../mm2src/coins/nft/withdraw.rs).
   Reusing the HD / Trezor signing path from `coins::eth` is
   straightforward but requires plumbing the path account /
   address index through `WithdrawNftReq`.
3. **Confirmation count for transfers.** Today the inventory
   simply stores the block at which the token was last observed;
   surfacing a live confirmation count would require either
   periodic re-checks or a streaming subscription
   ([Chapter 10](10-sse-streaming.md)).
4. **Komodo proxy signing.** Both `HttpCrawlProvider` and
   `HttpMetadataProvider` carry a `komodo_proxy: bool` field
   (marked `#[allow(dead_code)]`) ready to sign outbound HTTP
   with the libp2p proxy-signature scheme from
   [`mm2src/proxy_signature/`](../../mm2src/proxy_signature/),
   once the upstream proxy endpoints are stood up under the
   reloaded operational model.

These items are framed deliberately; none are blockers for the
RPC surface described above, and none belong in this chapter's
scope.

## 19.10 Provenance

The entire `mm2src/coins/nft/` tree is post-baseline. None of
the files exist at commit `c1d46c0`. All authorship is recorded
in the reloaded git history; the design is original to the
reloaded effort and does not derive from the upstream NFT module
beyond conforming to the wire-level ERC-721 / ERC-1155 standards
(which are public Ethereum specifications).
