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
indexers). The chapter-bound crate contains exactly
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
question (§23.9 D1), not a binding rule of the chapter-bound substrate.

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
    not implement the project's
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

## 23.7 Error Parsing and the Allowance-Shortfall Wire Envelope

The error model of §23.5 is produced by parsing the provider's
HTTP error bodies. The provider dictates the wire shapes below;
the field spellings are the provider's documented camelCase JSON
and are reproduced here as externally-dictated interop (R29 wire-
format / R33 third-party-api-bound), not as project expression.

**R10-A — Error wire envelope (R29/R33, externally dictated).**
A 1inch error response on an HTTP 400 carries a JSON object with
these fields:

| Wire field    | JSON type        | Meaning                       |
|---------------|------------------|-------------------------------|
| `error`       | string           | short error token             |
| `description` | string, optional | human-readable description    |
| `statusCode`  | integer          | echoed HTTP status            |
| `meta`        | array, optional  | typed metadata entries        |
| `requestId`   | string, optional | provider request id (ignored) |

Each `meta` entry is an object with a `type` string and a
`value` string. The binding consumes `requestId` only so that
decode does not reject the field; it is never surfaced to
callers.

**R10-B — Recognised `meta.type` tokens (R33).** The two
`meta.type` tokens the binding acts on are the provider's
documented values `allowance` and `amount`. Any other token is
treated as unknown, and the body falls through to the general-
API-error path.

**R10-C — Allowance-shortfall promotion.** When a 400 body's
`meta` array contains an entry of type `allowance`, the binding
MUST:

- read the `value` of that entry as the current allowance;
- read the `value` of a sibling entry of type `amount` as the
  required allowance;
- decode both decimal strings into the workspace's 256-bit
  unsigned integer type;
- produce the allowance-not-enough variant of §23.5 carrying the
  provider `error`, `description`, echoed status code, and the
  two decoded 256-bit values.

A 400 body with no `allowance` meta entry MUST instead produce
the general-API-error variant.

**R10-D — Other error bodies.** A non-400 error response MUST be
reported as the general-API-error variant, reading the top-level
`error` string from the body (empty when absent) together with
the echoed status code. A 400 body that fails to decode against
the envelope of R10-A MUST be reported as the body-parse-error
variant of §23.5.

**R10-E — Lenient amount decode.** An `allowance`/`amount` value
that does not parse as a decimal 256-bit integer MUST decode to
zero rather than aborting the parse. This is a deliberate
functional choice: the downstream allowance-approval consumer
(deferred D5) treats a zero current allowance as "no approval on
record" and is not broken by the substitution, whereas surfacing
a parse error here would mask the actionable allowance shortfall.

**Binding scope of §23.7 (R36).** The wire field spellings and
`meta.type` tokens above are dictated by the public 1inch API and
bind as interop (R29/R33). The error-variant *shapes* are the
§23.5 contract. Any Rust type names, private helper or
deserialisation structs, helper decomposition, field
identifiers, and the Display/diagnostic wording used to realise
this parsing are informative under R36: a re-derivation that
decodes the same wire envelope and produces the same §23.5
variants with different internal naming or decomposition is
conformant.

## 23.8 HTTP Client and URL Composition

The networking contract of §23.6 is realised by a stateless
client namespace plus a URL composer. This section states the URL
grammar, request behaviour, and dictated interop the client MUST
produce. The grammar, path tokens, provider constants, supported-
chain set, and header set are dictated by the public 1inch v6.0
API and bind as interop (R29/R33); the internal Rust shape used to
realise them is informative (R36).

### 23.8.1 Provider constants (R33, externally dictated)

R11-A. The binding carries the following 1inch v6.0 protocol
values — published provider constants, not project choices — and
MUST expose them to callers:

- **Aggregation router (v6.0) contract address:**
  `0x111111125421ca6dc452d289314280a0f8842a65`.
- **Native-asset sentinel address**, used by the provider to
  denote the chain's native coin in token positions:
  `0xeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee`.
- **Supported chain set:** the twelve `(ecosystem, chain id)`
  pairs enumerated in §23.2.

The binding MUST additionally expose a predicate that answers
whether a given chain id is in the supported set above (used to
reject unsupported chains per R1 of §23.2).

### 23.8.2 URL grammar (R33, externally dictated)

R11-B. The composed request URL MUST follow the 1inch path
grammar:

```
<base-url>/<endpoint-prefix>/<chain-id>/<method-token>?<query>
```

where, for the four swap routes, `<endpoint-prefix>` is
`swap/v6.0` and `<method-token>` is one of `quote`, `swap`,
`liquidity-sources`, or `tokens`; and for the portfolio route the
prefix is `portfolio/integrations/prices/v1`, the chain-id path
segment is omitted (the chain id travels as a query parameter per
§23.2), and the method token is `time_range/cross_prices`. The
path segments MUST be joined in the order prefix → chain id (swap
routes only) → method token, so the chain id is interpolated
between the version-pinned endpoint prefix and the method token;
query parameters are appended last. These prefixes and method
tokens are 1inch URL grammar, not project expression.

### 23.8.3 Base URL and headers

R11-C. **Base URL resolution.** On production builds the client
MUST resolve the base URL from the daemon configuration field
`1inch_api` (per R1); absence MUST surface as the invalid-
parameter error variant of §23.5 before any network call is
made. A test build path MAY substitute a fixed provider test
host. A base URL that fails to parse MUST surface as the invalid-
parameter variant.

R11-D. **Header set (R33 / standard content negotiation).** Every
request MUST carry `accept: application/json` and
`content-type: application/json`. On the `test-ext-api` build
path only, an `Authorization` header sourced from the
`ONE_INCH_API_TEST_AUTH` environment variable (per R3) MUST be
added. These are the standard JSON content-negotiation headers
and, for the authorization header, the provider's documented
test-tier authentication scheme; none shall be present in
release builds beyond the two content-negotiation headers.

### 23.8.4 Request execution

R11-E. **Call sequence.** A typed request call MUST:

1. on the `test-ext-api` build path only, acquire the test-tier
   rate-limit guard (R11-G) for the duration of the call;
2. issue a `GET` for the composed URL through the chapter-26
   cross-platform HTTP transport with the header set of R11-D,
   mapping any transport failure to the transport error variant
   of §23.5;
3. decode the response body once into a generic JSON value,
   mapping a decode failure to the body-parse-error variant;
4. on a non-`200` status, route the generic value through the
   error-parsing contract of §23.7 and return the resulting
   error variant;
5. on a `200` status, decode the same generic value into the
   caller's requested typed response shape, mapping a decode
   failure to the body-parse-error variant.

The decode-once-to-value-then-branch-on-status shape is a
functional requirement: success and error bodies share the
provider's JSON envelope at the transport layer but deserialise
to different typed shapes, so the status code selects which shape
the already-decoded value is interpreted as.

R11-F. **Diagnostics.** The binding MAY emit debug-level
diagnostics around the outbound URL and the response body.
Diagnostic wording is not part of the contract.

### 23.8.5 Test-tier rate limiting

R11-G. **One-request-per-second guard (test builds only).** On
the `test-ext-api` build path the client MUST serialise outbound
requests so that no two requests issue within one second of each
other, keeping test runs inside the provider's test-tier rate
limit (R3). The guard MUST NOT be present in release builds. The
mechanism used to realise it (a process-wide async lock plus a
one-second delay held across the request) is informative under
R36.

**Binding scope of §23.8 (R36).** The URL grammar, path tokens,
provider constants, supported-chain set, and header set above are
dictated by the public 1inch v6.0 API and bind as interop
(R29/R33). The error-routing and decode-branch behaviour are the
functional contract. All Rust type names, private struct and
field names, helper/marker types, local variables, control-flow
decomposition, and diagnostic wording used to realise this
section are informative: a re-derivation that emits the same
URLs, headers, and decode/error behaviour with different internal
naming or structure is conformant. Residual similarity of the
realisation to the historical lineage is governed by the R35
gate, under which a thin REST-path composer of this kind retains
little discretionary expression once the dictated grammar and
interface are excluded.

## 23.9 Binding Requirements

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

## 23.10 Tests

The crate ships unit tests colocated with each region. The
chapter-bound unit-test set covers:

- Anti-phishing URL validation on the provider client.

End-to-end tests against the live provider API are not in the
chapter-bound test set; they require both the
deferred RPC handlers (D2) and the test-only authentication
build path (R3) configured with a valid test-tier token.

## 23.11 Deferred Work

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
    bound in the chapter-bound substrate. The intent-based, resolver-
    filled Fusion variant of the provider's API is not
    bound.

D4. **Portfolio endpoint integration.** The portfolio cross-
    prices request and response types are defined but no
    consumer in the project calls them.

D5. **Allowance-approval flow.** The `AllowanceNotEnough`
    error variant carries enough information (R6) for a
    consumer to issue an ERC-20 `approve` call before
    retrying. No such flow is wired in the chapter-bound substrate;
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

## 23.12 External References

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

## 23.13 Baseline Verifications

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

## 23.14 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  trading-API binding crate at baseline verified via
  `git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the provider keywords against
  the baseline; the public 1inch Swap API v6.0 specification;
  the public 1inch Portfolio Cross-Prices API specification;
  the publicly-documented EVM chain ids of the twelve chains
  enumerated in §23.2; the ERC-20 `approve`/`allowance`
  standard.
- *Permitted-input classes used:* baseline source; external public
  specifications (1inch Swap API v6.0; 1inch Portfolio Cross-Prices
  API; ERC-20 `approve`/`allowance`); behavioural observation of
  public networks (the publicly-documented EVM chain ids);
  Interop / third-party-API-bound reuse (R29 wire-format / R33
  third-party-api-bound) for the dictated 1inch interop embedded in
  §23.7 and §23.8 — the error wire-envelope field spellings
  (`error`/`description`/`statusCode`/`meta`/`requestId` and the
  `meta` `type`/`value` keys), the `allowance`/`amount` `meta.type`
  tokens, the URL grammar and path tokens, the aggregation-router
  and native-asset-sentinel contract addresses, the supported-chain
  set, and the content-negotiation header names — whose authoritative
  source is the public 1inch v6.0 API, not the historical lineage.
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* not consulted for clean-room derivation. The
  dictated 1inch interop fragments enumerated above (R29/R33) are
  sourced from the public 1inch v6.0 API documentation; no
  discretionary expression — no function bodies, private
  identifiers, helper decomposition, control-flow transcription, or
  diagnostic/Display string literals — from the historical lineage
  crosses into this chapter. The realisation's residual similarity
  to that lineage for the thin REST-path composer is governed by the
  R35 gate (see §23.8 binding-scope note).
