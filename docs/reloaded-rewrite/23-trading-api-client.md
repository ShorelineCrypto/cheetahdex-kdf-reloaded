# Chapter 23 -- External Trading-API Client

**Status:** driving-spec

> **One-sentence claim:** the project carries a self-contained
> binding crate for external trading-API providers; its first
> provider binds the publicly-documented 1inch Swap API v6.0 and
> the matching portfolio price-history endpoint, exposes typed
> request and response shapes for both, and never embeds an
> API key, default base URL, or production rate-limit policy.

## 23.0 Executive Summary

A dedicated workspace crate carries bindings to external
trading-API providers (price oracles, swap aggregators, route
indexers). At the time of writing the crate contains exactly
one provider binding: a typed client for the publicly-
documented **1inch Swap API v6.0** plus the matching portfolio
price-history endpoint. The crate is structured so that a
second provider would live as a sibling module rather than
replace the first.

The crate is **a library only**. It does not register any
public JSON-RPC handler, does not depend on any coin support
module, and is not consumed by the daemon's runtime path at the
time of writing. The integration boundary (per-provider RPC
handlers, allowance-check and transaction-submission wiring
into EVM coin support) is named explicitly as deferred work in
§23.9.

The bound surface for the 1inch provider covers:

- A typed HTTP client that holds a caller-supplied base URL
  and routes requests to the correct path-template per
  endpoint and per chain id.
- Builders for the **classic-swap quote** and **classic-swap
  create** (transaction-build) requests, the **liquidity-
  sources** and **tokens** discovery requests, and the
  **portfolio cross-prices** OHLC request.
- Typed response records for each of the above, including a
  shared classic-swap response shape covering both quote and
  create returns.
- An error enum that distinguishes invalid-parameter, out-of-
  bounds, transport, body-parse, generic API, and the
  provider-specific `AllowanceNotEnough` failure (the last
  carrying the required allowance and the current allowance as
  256-bit integer values).

## 23.1 Subsystem Shape

The crate is organised by **provider**: each provider gets its
own submodule containing its client, error type, URL builder,
and typed request/response records. The crate's public surface
re-exports one provider submodule per binding.

| Region                  | Responsibility                                  |
|-------------------------|-------------------------------------------------|
| Crate-level public API  | Re-exports one provider submodule per binding   |
| Provider submodule      | Client, URL builder, errors, typed records      |
| Provider client         | Stateless HTTP entry point bound to a base URL  |
| Provider URL builder    | Endpoint path + query-parameter composition     |
| Provider error type     | Provider-specific error enum                    |
| Provider request types  | Builder structs for each endpoint               |
| Provider response types | Typed records for each endpoint                 |

The crate **does not** define a provider-agnostic trait. A
second provider added later would live as a sibling submodule
(`<provider>`) with its own client, its own error type, and
its own URL builder. A provider-agnostic abstraction is an open
question (§23.9 D1), not a binding rule at the time of writing.

## 23.2 1inch Provider -- Endpoints

The 1inch provider binds the following routes of the public
1inch v6.0 API:

| Path template                                                            | Purpose                       |
|--------------------------------------------------------------------------|-------------------------------|
| `GET /swap/v6.0/{chainId}/quote`                                         | Indicative swap quote         |
| `GET /swap/v6.0/{chainId}/swap`                                          | Build executable swap tx      |
| `GET /swap/v6.0/{chainId}/liquidity-sources`                             | Enumerate router protocols    |
| `GET /swap/v6.0/{chainId}/tokens`                                        | Enumerate supported tokens    |
| `GET /portfolio/integrations/prices/v1/time_range/cross_prices`          | OHLC token-pair price history |

Chain ids accepted on the four `/swap/v6.0/{chainId}/...`
endpoints correspond to the publicly-documented v6.0-supported
EVM chains:

| Chain id   | Chain ecosystem        |
|-----------:|------------------------|
| 1          | Ethereum               |
| 10         | Optimism               |
| 56         | BNB Smart Chain        |
| 100        | Gnosis                 |
| 137        | Polygon                |
| 250        | Fantom                 |
| 324        | zkSync Era             |
| 8217       | Klaytn                 |
| 8453       | Base                   |
| 42161      | Arbitrum               |
| 43114      | Avalanche              |
| 1313161554 | Aurora                 |

The chain id is interpolated into the path for the four swap
routes; any chain id outside the set above is rejected with
the `InvalidParam` variant of the provider error type.

The portfolio route does **not** carry the chain id in the
path; the chain id travels as a query parameter on the
portfolio request type.

## 23.3 Configuration and Authentication

R1. **No embedded base URL.** The 1inch base URL shall be
    sourced from the daemon's JSON configuration field
    `1inch_api`. If the field is absent, the client shall fail
    fast at construction with the provider's `InvalidParam`
    error variant. The crate shall not carry a compiled-in
    default URL.

R2. **No embedded API key.** Production builds shall not carry
    any API key for any provider. The public 1inch endpoints
    do not require authentication; the binding shall therefore
    issue unauthenticated requests on production builds.

R3. **Test-only authentication path.** A test build path,
    gated behind the **`test-ext-api`** Cargo feature, may
    additionally:
    - Read an authentication token from the environment
      variable `ONE_INCH_API_TEST_AUTH` and attach it as an
      `Authorization` header on every request.
    - Serialise outbound requests through a one-request-per-
      second lock so that test runs stay inside the test-tier
      rate limit of the provider.
    Neither behaviour shall be active in release builds.

R4. **Standard headers.** Every request carries the headers
    `Accept: application/json` and `Content-Type:
    application/json`. The conditional `Authorization` header
    of R3 is added only on the test-only build path.

## 23.4 Request and Response Shapes

The 1inch classic-swap **quote** request carries:

- *Required:* source token address, destination token address,
  amount (as a wei-denominated decimal string).
- *Optional:* fee, protocols filter, gas-price hint,
  complexity-level hint, parts count, main-route parts count,
  gas-limit hint, include-tokens-info flag, include-protocols
  flag, include-gas flag, connector-tokens list.

The 1inch classic-swap **create** request extends the quote
request with:

- *Required additionally:* sender address (`from`), slippage
  percentage in the range 0..=50.
- *Optional additionally:* excluded-protocols filter, permit
  payload, compatibility flag, alternative receiver address,
  referrer, disable-estimate flag, allow-partial-fill flag,
  use-permit2 flag.

Both endpoints return a shared classic-swap response shape
carrying:

| Field           | Type                            | Populated by             |
|-----------------|---------------------------------|--------------------------|
| `dst_amount`    | decimal string                  | quote and create         |
| `src_token`     | optional token-info record      | quote and create         |
| `dst_token`     | optional token-info record      | quote and create         |
| `protocols`     | optional triple-nested protocol-info list | quote and create |
| `tx`            | optional transaction-fields record | create only           |
| `gas`           | optional 128-bit gas estimate   | quote only               |

The transaction-fields record on a create response carries the
fields needed to sign and broadcast the swap transaction
(sender, recipient, calldata, value, gas price, gas limit).
Signing and broadcast are out of scope for this crate.

The portfolio cross-prices request carries chain id, token-0
address, token-1 address, optional granularity, and optional
limit. The response is a series of OHLC records keyed by
timestamp; numeric fields use a big-decimal type so the
response can be deserialised without precision loss.

R5. **Numeric precision.** Provider response shapes shall use
    big-decimal or 256-bit-integer types for any field
    representing an on-chain amount, an allowance, an OHLC
    price, or a wei-denominated value. Floating-point types
    shall not be used for any such field.

## 23.5 Error Model

The provider error enum distinguishes the following failure
modes:

| Variant               | Carries                                                    |
|-----------------------|------------------------------------------------------------|
| Invalid parameter     | Description of the violated invariant                      |
| Out-of-bounds         | Parameter name, value, declared minimum, declared maximum  |
| Transport             | Inner transport-layer error                                |
| Parse body            | Body-decode message                                        |
| General API           | Provider's `error` message, description, HTTP status code  |
| Allowance not enough  | The provider's error/description/status code **plus**     |
|                       | required allowance and current allowance, each as a       |
|                       | 256-bit unsigned integer                                   |

R6. **Allowance shortfall carries machine-actionable data.**
    The provider's 400-with-meta body for an
    `allowance is not enough` failure shall be parsed into the
    `AllowanceNotEnough` variant with the required and current
    allowance promoted to the typed 256-bit unsigned integer
    used by EVM coin support. The variant is the surface
    through which an allowance-approval flow (deferred D5
    below) reads the amounts it needs.

R7. **No HTTP-status mapping in the crate.** The crate shall
    not implement the codebase's
    `HttpStatusCode` mapping trait. Mapping provider errors
    to RPC-layer HTTP status codes is the responsibility of
    the future RPC handlers (D2).

## 23.6 Networking

R8. **Transport-layer indirection.** All outbound HTTP traffic
    shall flow through the workspace's cross-platform HTTP
    transport (see [Chapter 26](26-cross-platform-and-wasm.md))
    rather than calling a concrete HTTP-client crate directly.
    This is what allows the same client code to compile and
    run on both native and browser targets.

R9. **GET-only wire surface.** Every endpoint bound by this
    crate is a `GET` with query parameters. No `POST` body and
    no streaming endpoint is in scope.

The URL builder for each provider is responsible for:

- Prepending the configured base URL.
- Interpolating the chain id into the path component where the
  endpoint template requires it.
- Serialising typed request parameters into URL query
  parameters.
- Validating that numeric parameters lie within their declared
  bounds; out-of-bound values surface as the out-of-bounds
  error variant of §23.5 before any network call is made.

## 23.7 Binding Requirements

R1-R9 above are binding. In addition:

R10. **Provider isolation.** Each provider's client, error
     type, URL builder, and request/response types shall live
     in a single submodule of the crate. No provider's types
     shall depend on another provider's types.

R11. **Library-only.** The crate shall not register any
     JSON-RPC handler and shall not depend on any coin
     support module. RPC integration and coin wiring live
     outside the crate (D2, D5).

R12. **No vendored provider source.** The crate shall consume
     each provider's public HTTP API only. No provider's
     source code shall be vendored into the crate.

## 23.8 Tests

The crate ships unit tests colocated with each region. The
unit-test set at the time of writing covers:

- Anti-phishing URL validation on the provider client.

End-to-end tests against the live provider API are not in the
test set at the time of writing; they require both the
deferred RPC handlers (D2) and the test-only authentication
build path (R3) configured with a valid test-tier token.

## 23.9 Deferred Work

D1. **Provider-agnostic abstraction.** A trait covering the
    common client surface across providers is an open
    question. The current per-provider-submodule layout
    leaves room for one but does not bind one.

D2. **JSON-RPC handler registration.** The intended public
    RPC surface for the 1inch provider is a set of five
    handlers covering router-address resolution, classic-swap
    quote, classic-swap create, liquidity-sources discovery,
    and tokens discovery. None of these handlers exist at the
    time of writing; the public RPC dispatcher carries no
    entry for this provider.

D3. **1inch Fusion mode.** Only the classic-swap surface is
    bound at the time of writing. The intent-based, resolver-
    filled Fusion variant of the provider's API is not
    bound.

D4. **Portfolio endpoint integration.** The portfolio cross-
    prices request and response types are defined but no
    consumer in the codebase calls them.

D5. **Allowance-approval flow.** The `AllowanceNotEnough`
    error variant carries enough information (R6) for a
    consumer to issue an ERC-20 `approve` call before
    retrying. No such flow is wired at the time of writing;
    the variant is a parse target without a handler.

D6. **Production rate-limit policy.** Only the test-only
    build path of R3 serialises requests. A production rate-
    limit policy (per-provider, per-chain, or global) is a
    deferred decision; the crate does not currently impose
    one.

D7. **Transaction signing and broadcast.** The transaction-
    fields record returned by the classic-swap create
    endpoint is delivered to the caller. The crate does not
    sign or broadcast; that wiring belongs in the integrating
    RPC handler and the EVM coin support module.

## 23.10 External References

- The 1inch Swap API v6.0 specification (the public HTTP API
  bound by the first provider).
- The 1inch Portfolio Cross-Prices API specification (the
  public OHLC endpoint bound by the same provider).
- The publicly-documented EVM chain ids of the twelve chains
  enumerated in §23.2.
- The ERC-20 `approve`/`allowance` standard (the basis for
  the typed allowance amounts of §23.5 and the deferred
  approval flow of D5).
- The big-decimal and 256-bit-integer numeric types of R5
  (the workspace's standard numeric substrates for on-chain
  amounts and allowance values).

## 23.11 Baseline Verifications

The following are verifiable from the baseline state defined
in [Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains **no** trading-API binding
    crate. A directory listing of the baseline tree
    (`git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`)
    contains no `trading_api` entry; a tree-wide
    `git grep -l '1inch\|one_inch\|trading_api'` against the
    baseline returns no matches.

V2. The baseline tree contains no JSON-RPC handler for any
    1inch endpoint. A tree-wide `git grep` for
    `one_inch_v6_0` against the baseline returns no matches.
    R11 of §23.7 ("library-only") is therefore consistent
    with the baseline state and not a regression from it.

V3. The twelve chain ids enumerated in §23.2 correspond to
    the publicly-documented v6.0-supported EVM chains. The
    list is the provider's published support set; chain ids
    outside the list are out of scope by binding rule R1 of
    §23.2.

V4. The provider's HTTP API is a public specification.
    No provider source code is vendored anywhere in the
    workspace; the binding is via the public HTTP surface
    only (R12).

## 23.12 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  trading-API binding crate at baseline verified via
  `git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the provider keywords against
  the baseline; the public 1inch Swap API v6.0 specification;
  the public 1inch Portfolio Cross-Prices API specification;
  the publicly-documented EVM chain ids of the twelve chains
  enumerated in §23.2; the ERC-20 `approve`/`allowance`
  standard.
- *Forbidden corpus:* not consulted.
