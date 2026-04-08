# Chapter 23 — External Trading-API Client

> **Chapter type:** document existing. No IMPL marker.

## 23.0 Executive summary

The reloaded tree carries an external trading-API binding crate
at [`mm2src/trading_api/`](../../mm2src/trading_api/). It
currently holds a single provider: a typed HTTP client for the
**1inch Swap API v6.0** and a small portfolio price-history
endpoint. The crate is post-baseline (the entire directory is
absent at `c1d46c0`) and is built as a self-contained library:
no coin crate depends on it, and no RPC handler in
[`mm2_main`](../../mm2src/mm2_main/) routes to it yet.

Concretely, the crate provides:

- a typed `ApiClient` that owns the 1inch base URL (taken from
  `MmCtx.conf["1inch_api"]`),
- a URL builder + chain-id router covering every
  v6.0-supported EVM chain,
- request/response structs for classic-swap **quote**, classic-
  swap **create** (build-tx), **liquidity-sources**, **tokens**,
  and **portfolio cross-prices** endpoints, and
- an `ApiClientError` enum that distinguishes transport,
  parse, generic API, and `AllowanceNotEnough` failures.

What is **not** yet wired in reloaded:

- per-coin RPC handlers (the `one_inch_v6_0_*` family),
- dispatcher entries in
  [`dispatcher.rs`](../../mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs)
  (grep is empty for `one_inch` / `trading_api`),
- any cross-talk with `coins::eth` (allowance checks, tx
  submission),
- Fusion-mode swap types,
- production-side rate limiting.

The crate is therefore best described as a clean API binding
with the integration surface (RPC + coin wiring) deliberately
deferred.

## 23.1 Crate layout

```
mm2src/trading_api/
|-- Cargo.toml
`-- src/
    |-- lib.rs                        (re-exports one_inch_api)
    |-- one_inch_api.rs               (module aggregator)
    `-- one_inch_api/
        |-- client.rs                 ApiClient, UrlBuilder, base URL
        |-- errors.rs                 ApiClientError, NativeError
        |-- classic_swap_types.rs     quote/create params + response types
        `-- portfolio_types.rs        cross-prices types
```

Roughly 950 lines of Rust, all of it 1inch-specific.

## 23.2 Public API

[`lib.rs`](../../mm2src/trading_api/src/lib.rs) re-exports a
single module:

```rust
pub mod one_inch_api;
```

Consumers therefore import via
`trading_api::one_inch_api::{ApiClient, ApiClientError, ...}`.
There is no provider-agnostic facade crate trait; the
expectation is that a future second provider would live as a
sibling module (`trading_api::<provider>`) with its own client.

The headline types are:

| Type                          | Role                                          |
|-------------------------------|-----------------------------------------------|
| `ApiClient`                   | Stateless HTTP client bound to the 1inch base |
| `UrlBuilder`                  | Chain-id + query-param URL composer           |
| `ClassicSwapQuoteParams`      | Builder for `/quote` requests                 |
| `ClassicSwapCreateParams`     | Builder for `/swap` (build-tx) requests       |
| `ClassicSwapData`             | Response shared by quote and create           |
| `TxFields`                    | Embedded `tx` payload on create responses     |
| `TokenInfo`, `ProtocolInfo`   | Sub-records on swap responses                 |
| `CrossPriceParams`            | Portfolio price-history request               |
| `CrossPricesData` / `Series`  | OHLC response                                 |
| `ApiClientError`              | Crate error enum (see §23.6)                  |

`ApiClient` exposes a single generic call site (a free associated
function; no `self`):

```rust
pub async fn call_api<T: DeserializeOwned>(
    api_url: Url,
) -> MmResult<T, ApiClientError>;
```

There are no per-endpoint helper methods on `ApiClient` today.
Callers compose a request manually: build typed params, lower
them to query parameters via the params' `build_query_params()`,
then construct a `Url` through `SwapUrlBuilder` (for
classic-swap routes) or `PortfolioUrlBuilder` (for the
price-history route) and pass that `Url` to `call_api::<R>`.
Whether to grow per-endpoint convenience methods is an explicit
open question -- see §23.10.

## 23.3 1inch endpoints covered

The crate binds to the following v6.0 routes:

| Path                                                                    | Purpose                       |
|-------------------------------------------------------------------------|-------------------------------|
| `GET /swap/v6.0/{chainId}/quote`                                        | Indicative swap quote         |
| `GET /swap/v6.0/{chainId}/swap`                                         | Build executable swap tx      |
| `GET /swap/v6.0/{chainId}/liquidity-sources`                            | Enumerate router protocols    |
| `GET /swap/v6.0/{chainId}/tokens`                                       | Enumerate supported tokens    |
| `GET /portfolio/integrations/prices/v1/time_range/cross_prices`         | OHLC token-pair price history |

Supported chain ids (per the chain table in
[`client.rs`](../../mm2src/trading_api/src/one_inch_api/client.rs)):
Ethereum (1), Optimism (10), BSC (56), Gnosis (100), Polygon
(137), Fantom (250), ZkSync (324), Klaytn (8217), Base
(8453), Arbitrum (42161), Avalanche (43114), Aurora
(1313161554). The chain id is injected into the path by
`SwapUrlBuilder`; unsupported ids surface as
`ApiClientError::InvalidParam`. The portfolio route does *not*
take the chain id in the path; `PortfolioUrlBuilder` instead
forwards it as a query parameter sourced from
`CrossPriceParams`.

## 23.4 Configuration and authentication

The base URL is sourced from `MmCtx.conf["1inch_api"]` -- there
is no compiled-in default. If the field is missing the client
fails fast with `ApiClientError::InvalidParam` at construction.

Production endpoints used here are the public 1inch endpoints
and do not require an API key. Test builds gain a key path via
the `test-ext-api` cargo feature:

```rust
#[cfg(feature = "test-ext-api")]
lazy_static! {
    static ref ONE_INCH_API_TEST_AUTH: String =
        std::env::var("ONE_INCH_API_TEST_AUTH").unwrap_or_default();
}
```

When that feature is active, the client adds an
`Authorization: <ONE_INCH_API_TEST_AUTH>` header and serialises
requests through a `one_req_per_sec()` async lock to stay
inside 1inch's test-tier rate limit. Outside of tests there is
no throttle.

Standard headers on every request are `Accept: application/json`
and `Content-Type: application/json`. When the `test-ext-api`
feature is active an `Authorization: <ONE_INCH_API_TEST_AUTH>`
header is *also* sent; production builds (no feature) omit it
entirely, since the public 1inch endpoints do not require it.

## 23.5 Request and response shapes

`ClassicSwapQuoteParams`
([`classic_swap_types.rs`](../../mm2src/trading_api/src/one_inch_api/classic_swap_types.rs))
is a builder with:

- required: `src`, `dst` (token addresses), `amount` (wei as
  decimal string);
- optional: `fee`, `protocols`, `gas_price`, `complexity_level`,
  `parts`, `main_route_parts`, `gas_limit`, `include_tokens_info`,
  `include_protocols`, `include_gas`, `connector_tokens`.

`ClassicSwapCreateParams` extends quote params with:

- required: `from` (sender address) and `slippage` (0--50%);
- optional: `excluded_protocols`, `permit`, `compatibility`,
  `receiver`, `referrer`, `disable_estimate`,
  `allow_partial_fill`, `use_permit2`.

The shared response is `ClassicSwapData`:

```rust
pub struct ClassicSwapData {
    pub dst_amount: String,
    pub src_token: Option<TokenInfo>,
    pub dst_token: Option<TokenInfo>,
    pub protocols: Option<Vec<Vec<Vec<ProtocolInfo>>>>,
    pub tx: Option<TxFields>,   // populated by /swap only
    pub gas: Option<u128>,       // populated by /quote only
}
```

`TxFields` carries the on-chain transaction the caller is meant
to sign and broadcast (`from`, `to`, `data`, `value`,
`gas_price`, `gas`); `coins::eth` is the expected signer, but
that wiring is not present in reloaded today.

Portfolio cross-prices use `CrossPriceParams { chain_id,
token0_address, token1_address, granularity?, limit? }` and
return a `CrossPricesSeries` of OHLC records keyed by
timestamp; numerics are `BigDecimal`.

## 23.6 Error model

[`errors.rs`](../../mm2src/trading_api/src/one_inch_api/errors.rs)
defines:

```rust
pub enum ApiClientError {
    InvalidParam(String),                       // bad URL / config
    OutOfBounds { param, value, min, max },      // builder validation
    TransportError(SlurpError),                  // HTTP layer
    ParseBodyError { error_msg },                // JSON decode
    GeneralApiError { error_msg, description, status_code },
    AllowanceNotEnough { error_msg, description, status_code,
                         amount, allowance },
}
```

The crate does **not** implement `HttpStatusCode`; mapping API
errors to HTTP status codes is left to the (future) RPC
handlers. The 1inch-specific 400-with-meta body for
`allowance is not enough` is parsed by `NativeError` and lifted
into the strongly typed `AllowanceNotEnough` variant so the
caller can read the required `amount` and current `allowance`
as `U256`.

## 23.7 Networking

HTTP transport goes through
`mm2_net::transport::slurp_url_with_headers`
([Chapter 4](04-rust-and-tooling-baseline.md)), not
`reqwest` directly. The `mm2_net` indirection lets the same
client code run on both native and WASM without changes (see
[Chapter 26](26-cross-platform-and-wasm.md)). On the wire it is
plain GET requests with query parameters; there is no
WebSocket / streaming path in 1inch v6.0.

The `UrlBuilder` is responsible for:

- prepending the configured base URL,
- injecting the chain id into the path component,
- serialising the typed params into URL query parameters,
- validating that numeric params are within their declared
  bounds (which surfaces as `OutOfBounds`).

## 23.8 RPC dispatcher status

Grepping `mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs` for
`one_inch` / `oneinch` / `trading_api` returns no matches.
There are no `one_inch_v6_0_classic_swap_*` handlers in
`mm2src/mm2_main/src/rpc/lp_commands/` in reloaded.

The intended RPC surface (mirroring the upstream KDF layout)
would be five handlers under a `one_inch_v6_0_` prefix:

- `one_inch_v6_0_classic_swap_contract_rpc` (router address)
- `one_inch_v6_0_classic_swap_quote_rpc`
- `one_inch_v6_0_classic_swap_create_rpc`
- `one_inch_v6_0_classic_swap_liquidity_sources_rpc`
- `one_inch_v6_0_classic_swap_tokens_rpc`

These are explicitly future work; they require both the
handler module and a coin resolver that bridges `ApiClient`
results to `coins::eth` for allowance + tx submission.

## 23.9 Tests

The crate ships a single unit test in
[`classic_swap_types.rs`](../../mm2src/trading_api/src/one_inch_api/classic_swap_types.rs)
covering anti-phishing URL validation:

```rust
#[test]
fn test_validate_one_inch_link() { ... }
```

There are no end-to-end tests in reloaded; integration tests
that call the live API would require the RPC handlers (see
§23.8) and the `test-ext-api` feature with
`ONE_INCH_API_TEST_AUTH` set.

## 23.10 Known limitations and deferred work

1. **Provider count is one.** The module shape
   (`trading_api::<provider>`) anticipates more providers; only
   1inch is implemented today.
2. **No Fusion-mode types.** Only classic-swap is modeled;
   1inch Fusion (intent-based, resolver-filled) is absent.
3. **Portfolio endpoint unused.** `CrossPrice*` types are
   defined but no consumer in reloaded calls them.
4. **No RPC handlers.** See §23.8.
5. **No coin / allowance / submission wiring.** The
   `TxFields` payload is returned to the caller; nothing in
   reloaded signs and broadcasts it through `coins::eth`.
6. **Production rate limiting absent.** Only the
   `test-ext-api` build path serialises requests; production
   builds can hammer 1inch and be rate-limited.
7. **`AllowanceNotEnough` not threaded into an approval flow.**
   The error is parsed but no handler reacts to it with an
   automatic `approve` step.

## 23.11 Provenance

`mm2src/trading_api/` is post-baseline (`git ls-tree c1d46c0
-- mm2src/trading_api` returns empty); the entire directory is
new code in reloaded. The implementation binds to the public
1inch Swap API v6.0 (an external HTTP API specification); no
1inch source code is vendored. All transport goes through the
in-tree [`mm2_net`](../../mm2src/mm2_net/) crate documented in
[Chapter 4](04-rust-and-tooling-baseline.md).
