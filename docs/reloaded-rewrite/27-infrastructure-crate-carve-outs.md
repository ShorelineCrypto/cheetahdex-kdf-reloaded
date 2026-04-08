# Chapter 27 -- Infrastructure Crate Carve-Outs

> **Chapter type:** document existing. No IMPL marker.

## 27.0 Executive summary

The post-baseline workspace gained roughly two dozen new
crates plus a handful of substantial extensions to crates
that already existed at the baseline. They divide cleanly
into two categories:

1. **Feature-bearing crates** -- each owns a user-visible
   capability and is documented in its own chapter:
   `kdf_walletconnect/` (Ch 22), `trading_api/` (Ch 23),
   `mm2_gui_storage/` (Ch 24), `db_common/` (Ch 25),
   `mm2_p2p/` (Ch 28), the `kdf_*` Bitcoin-primitive replacements
   (covered alongside the UTXO / SPV chapters), and the EVM /
   MetaMask / mobile-binding crates introduced in their
   feature chapters.
2. **Cross-cutting infrastructure crates** -- they own no
   single user-visible feature but provide a uniform tool
   that every other crate depends on. This chapter is the
   register for that second group.

The canonical members of the second group are:

- error handling: [`mm2_err_handle/`](../../mm2src/mm2_err_handle/)
  + [`derives/ser_error/`](../../mm2src/derives/ser_error/)
  + [`derives/ser_error_derive/`](../../mm2src/derives/ser_error_derive/);
- event streaming: [`mm2_event_stream/`](../../mm2src/mm2_event_stream/);
- typed RPC protocol: [`mm2_rpc/`](../../mm2src/mm2_rpc/);
- long-running RPC task framework: [`rpc_task/`](../../mm2src/rpc_task/);
- state-machine runtime: [`mm2_state_machine/`](../../mm2src/mm2_state_machine/);
- metrics: [`mm2_metrics/`](../../mm2src/mm2_metrics/);
- exact numerics: [`mm2_number/`](../../mm2src/mm2_number/);
- filesystem and net I/O scaffolding:
  [`mm2_io/`](../../mm2src/mm2_io/) +
  [`mm2_net/`](../../mm2src/mm2_net/) +
  [`mm2_net_config/`](../../mm2src/mm2_net_config/);
- repository-driven configuration: [`mm2_git/`](../../mm2src/mm2_git/);
- libp2p proxy signing: [`proxy_signature/`](../../mm2src/proxy_signature/);
- hardware-wallet primitives: [`hw_common/`](../../mm2src/hw_common/),
  [`trezor/`](../../mm2src/trezor/), and
  [`ledger/`](../../mm2src/ledger/);
- shared-reference debug helper:
  [`common/shared_ref_counter/`](../../mm2src/common/shared_ref_counter/);
- procedural-macro derives: [`derives/enum_derives/`](../../mm2src/derives/enum_derives/).

Each section below is one paragraph: what the crate does,
its baseline status (entirely new vs pre-existing-extended),
its public re-export surface, and how consumers use it. The
intent is that every other chapter that mentions an
infrastructure crate by name links here for a single source
of truth.

## 27.1 Error handling -- `mm2_err_handle` + `ser_error{,_derive}`

The error-handling triplet defines the project's common
error envelope: a generic carrier that wraps a domain enum
plus optional structured payload, a result alias, a
`prelude` of conversion combinators (`map_to_mm`,
`or_mm_error`, `map_to_mm_fut`, `map_mm_error`), and a
JSON-error variant suitable for logging. The companion
proc-macro crate emits a sealed marker that gates the
derive of the wire-format adjacent-tagged JSON shape used by
the RPC layer (`error_type` / `error_data`). The result is
that any RPC handler that wants to return a structured
error has only to derive the marker on its enum and write a
single `HttpStatusCode` impl; the rest of the conversion
is automatic. Re-exports in this triplet's prelude include
the carrier type, the result alias, the conversion traits,
the JSON-error variant, and the proc-macro derive name.
The carrier crate already existed at the baseline, but the
prelude shape, the proc-macro split, and several conversion
traits are post-baseline; the trait-solver-driven derive
was rebuilt from scratch (Chapter
[4](04-error-aggregation-type-adaptation.md)). Every other
crate in this register depends on this triplet either
directly or transitively.

## 27.2 Event streaming -- `mm2_event_stream`

The event-streaming crate owns the SSE fan-out used by every
`stream::*` RPC method (Chapter
[10](10-sse-streaming.md)). It exposes:

- a small algebra for events (a typed enum with serde tags
  for the JSON shape, an origin identifier whose textual
  form is part of the wire contract, a payload trait);
- a `Broadcaster` handle producers hold to publish events;
- a behaviour trait that streamers implement (an async
  worker loop with shutdown handling);
- a manager that registers, spawns, and shuts down
  streamers, and that fans broadcast events out to
  subscribed clients with bounded per-client back-pressure
  channels.

The crate is entirely post-baseline. `MmCtx` carries the
manager handle on both native and WASM. Streamers covered
elsewhere in this document set include the heartbeat
streamer, the swap-status streamer, the order-book streamer,
the network-health streamer, the balance and tx-history
streamers, and the orderbook-depth streamer.

## 27.3 Typed RPC protocol -- `mm2_rpc`

The RPC-protocol crate owns the typed wrappers around the
JSON-RPC 2.0-style envelope the workspace exposes. Its
modules carry the shared request-and-response types (the
`mmrpc` version tag, the typed wrappers around
`params`/`result`/`error`), and a WASM-only sender used by
the JS-shim glue (Chapter [26](26-cross-platform-and-wasm.md))
to bridge cross-thread RPC. The crate already existed at
baseline; post-baseline changes added typed envelopes for
the `task::*` and `stream::*` namespaces and the
gRPC-WEB-related helpers consumed by `mm2_net::grpc_web`.
Consumer pattern: every dispatcher entry in
[`mm2_main::rpc::dispatcher`](../../mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs)
is typed against this crate's envelope.

## 27.4 Long-running RPC tasks -- `rpc_task`

`rpc_task` provides the four-call lifecycle for any RPC
operation that cannot complete inside a single request. The
current dispatcher exposes the lifecycle through unprefixed
method names (one set per task type); the AGENTS.md note
on a uniform `task::<op>::<verb>` namespace records the
direction of travel rather than the present wire shape.

| Lifecycle phase  | Effect                                             |
|------------------|----------------------------------------------------|
| init             | Schedules the task; returns a task identifier. Method form: `init_<op>` (e.g. `init_withdraw`, `init_create_new_account`). |
| status           | Returns one of `InProgress` / `UserActionRequired` / `Ok` / `Error` (with structured payload). Method form: `<op>_status`. |
| user_action      | Resumes a task that is awaiting input (PIN, passphrase, confirmation). Method form: `<op>_user_action`. |
| cancel           | Aborts an in-flight task. Currently exposed per task type (e.g. `cancel_init_lightning`); not yet a uniform method. |

Public surface: a behaviour trait the task implements
(generic over user-action input, awaitable item, error
type, and intermediate progress payload), a manager handle
held by `MmCtx`, a status enum used in the `status` reply,
and a typed handle held by call sites for the task they
spawned. The crate already existed at baseline; the
post-baseline extensions added user-action resumption (the
`AwaitingUserAction` arm), structured progress payloads,
and the typed-error machinery that lets each task type
expose its own structured error variant via the
[§27.1](#271-error-handling----mm2_err_handle---ser_error_derive)
triplet. Consumers in this document set are the wallet
lifecycle init flows (Chapter
[7](07-wallet-lifecycle-and-key-export.md)), the
hardware-wallet init flows, the long-running withdraw
operations, and the V2 swap initialisation paths
(Chapters [13](13-swap-version-negotiation.md)+).

## 27.5 State-machine runtime -- `mm2_state_machine`

The state-machine crate is the runtime the V2 swap state
machines (Chapters
[14](14-state-machine-runtime.md), [15](15-swap-v2-utxo-path.md),
[16](16-swap-v2-pre-burn-output.md), [17](17-swap-v2-evm-path.md))
sit on top of. Its essence is a behaviour trait that a
state implements once per (current state, next state) pair
plus an auto-trait-and-negative-impl pattern that makes
self-transitions and transitions out of terminal states
into compile errors rather than runtime errors. A second
half of the crate, the storable-state-machine variant,
adds event-sourced persistence so that an interrupted swap
can be reloaded from its persisted last-event log and
resumed at the boundary the interruption left it at. The
crate is entirely post-baseline. It depends on the error
triplet ([§27.1](#271-error-handling----mm2_err_handle---ser_error_derive))
for its result shape and on `db_common`
([Chapter 25](25-sql-query-builder.md)) for its persistence
backend. Consumer pattern: a swap path declares one type
per state, implements one transition trait per legal pair,
and hands the resulting object to the runtime, which drives
it to a terminal state and returns its final value.

## 27.6 Metrics -- `mm2_metrics`

`mm2_metrics` is the workspace metrics surface: a thin
abstraction over a Prometheus registry on native, a no-op
on WASM. Its public surface is a `Metrics` registry handle
held by `MmCtx`, a clock handle for periodic
counter/gauge/histogram updates, an `init`/
`init_with_dashboard` pair (the latter starts an HTTP
listener and a periodic-snapshot logger), a
`collect_json` snapshot reader, and a small set of macros
(`mm_counter!`, `mm_gauge!`, `mm_timing!`) that compile to
no-ops on WASM. Each macro supports both a bare form
(metric, name, value) and a labelled form that takes
`"label_key" => "label_val"` pairs. The crate is entirely
post-baseline; the baseline carried ad-hoc metric counters
scattered across files. Consumer pattern: code emits via
the macros; the registry is wired once at startup; the
dashboard endpoint is exposed only when the configuration
enables it.

## 27.7 Exact numerics -- `mm2_number`

`mm2_number` is the workspace's exact-arithmetic type for
prices, balances, and fee calculations. Its core is a
wrapper around the third-party `BigRational` (an arbitrary-
precision signed rational) plus serde-glue for several
on-the-wire shapes: a decimal-string representation, a
`{numer, denom}` rational pair, and a `BigDecimal`
fixed-precision form. Public surface is the wrapper type
itself, an associated zero/one constant, the standard
arithmetic operator impls (Add, Sub, Mul, Div, Neg, Pow),
ordering impls, and several `From`/`TryFrom` conversions
for the integer and decimal types. The crate is entirely
post-baseline. Consumer pattern: every fee, price, and
balance value crossing a wire boundary is this type;
floating-point arithmetic is reserved for non-financial
code paths (UI display, logging, etc.).

## 27.8 Filesystem and configuration scaffolding

Three small crates carry the I/O and configuration glue:

- [`mm2_io/`](../../mm2src/mm2_io/) -- native-only
  filesystem and file-lock helpers (Chapter
  [26 §26.6](26-cross-platform-and-wasm.md#266-filesystem-and-os-interactions)).
  Existed at baseline; post-baseline extensions are minor.
- [`mm2_net/`](../../mm2src/mm2_net/) -- the dual transport
  layer documented in Chapter
  [26 §26.5](26-cross-platform-and-wasm.md#265-the-dual-transport):
  native HTTP via hyper, WASM HTTP via fetch, native and
  WASM WebSocket clients, and a target-aware gRPC-WEB
  decoder that uses the `cfg_native!`/`cfg_wasm32!` macros
  from `common`. Existed at baseline; the WASM half and
  gRPC-WEB are post-baseline additions.
- [`mm2_net_config/`](../../mm2src/mm2_net_config/) --
  network-id and seed-node configuration (Chapter
  [6](06-network-id-seed-node.md)). Entirely post-baseline.

## 27.9 Repository-driven configuration -- `mm2_git`

`mm2_git` is the workspace's read-only Git client: an
abstraction over a remote repository (a Git host, an owner,
a repo, a branch, a path) that fetches files, lists
directories, and decodes JSON payloads on the way out. It
exists so that coin lists, electrum lists, faucet lists,
and a handful of other configuration tables can be served
from a public Git repository instead of from a centralised
HTTP service: a node operator changes a JSON file in the
config repo, pushes, and the workspace picks up the change
on its next refresh interval. Public surface is a
repository-operations trait, a GitHub-flavoured client that
implements it, and a typed file-metadata struct used in
listings. The crate is entirely post-baseline.

## 27.10 libp2p proxy signing -- `proxy_signature`

`proxy_signature` is the small crate libp2p proxy
endpoints (Chapter [28](28-libp2p-modernization.md)) use
to authenticate clients without a shared secret: a request
is wrapped in a typed envelope (a fixed magic prefix, the
target URI, a body-size hash, the libp2p-encoded public
key, and an expiry timestamp), signed with the client's
libp2p keypair, and validated on the proxy by re-deriving
the prefix and verifying the signature with the embedded
public key. Public surface is the envelope struct, the
sign-and-verify trait, and the magic-prefix constant. The
crate is entirely post-baseline.

## 27.11 Hardware-wallet primitives

Three crates carry the hardware-wallet stack:

- [`hw_common/`](../../mm2src/hw_common/) -- shared
  primitives and transport contracts (a derivation-path
  type, an ECDSA-curve enum, an abstract transport trait).
  Existed at baseline; post-baseline extensions track new
  curve and path conventions.
- [`trezor/`](../../mm2src/trezor/) -- the Trezor protobuf
  protocol client, a session abstraction, the user-
  interaction trait that the host UI implements to handle
  PIN and passphrase prompts, and the request/response
  types for the swap-relevant subset of Trezor messages.
  Existed at baseline; post-baseline extensions added
  EIP-712 and ERC-20 message handling and the dual native /
  WASM target tables documented in
  Chapter [26 §26.7](26-cross-platform-and-wasm.md#267-hardware-wallets-and-other-native-only-stacks).
- [`ledger/`](../../mm2src/ledger/) -- the present scaffold
  for an analogous Ledger client. Existed at baseline as a
  scaffold and remains a scaffold; the only target table in
  its `Cargo.toml` is the WASM one, and the crate is not
  yet wired into `mm2_main`'s coin or swap paths.

## 27.12 Shared-reference debug helper

[`common/shared_ref_counter/`](../../mm2src/common/shared_ref_counter/)
is a small `Arc`-shaped wrapper that, when its `enable`
feature is on, instruments every clone and drop with the
caller's source location and prints a leak summary on
process exit; when the feature is off it compiles down to
the standard `Arc`. Used by `MmCtx` and a handful of other
long-lived shared types so that lifetime issues in async
code paths can be diagnosed without changing the surface
type. Entirely post-baseline.

## 27.13 Procedural-macro derives

[`derives/enum_derives/`](../../mm2src/derives/enum_derives/)
is a small proc-macro crate carrying the boiler-plate-
reduction derives the workspace uses for its tagged
enums: a `From`-for-newtype derive, a `From`-via-
`to_string` for string-variant derives, a custom-trait
delegation derive, and a unit-variant listing derive. The
companion `derives/ser_error{,_derive}/` crates are
documented at [§27.1](#271-error-handling----mm2_err_handle---ser_error_derive)
above and serve the same boilerplate-reduction goal but
specifically for the wire-format error envelope. The
proc-macro split is post-baseline; the underlying derives
have been added gradually as each consumer's enum count
crossed the threshold where the macro paid for itself.

## 27.14 Limitations and known gaps

1. **No single dependency-graph diagram.** The infrastructure
   crates form a tree (each one depends on a subset of the
   others), but the tree is not drawn anywhere; it has to
   be reconstructed from `Cargo.toml` files.
2. **The boundary between feature crates and infrastructure
   crates is conventional.** `db_common` is documented in
   its own chapter (Ch 25) because its DSL surface is
   user-visible at the SQL level; `mm2_event_stream` is in
   this chapter because no consumer is named at the wire
   level. The decision is editorial, not enforced.
3. **`ledger` is on the register but inactive.** It appears
   here to make the inventory complete, not because the
   crate is reachable from a runtime entry point today.
4. **Some smaller crates are intentionally out of scope.**
   `kdf_test_helpers/`, `mm2_test_helpers/`,
   `ethabi-vendored/`, `testcontainers-vendored/`, and the
   `mm2_bitcoin_wire_tests/` integration-test crate are
   testing or vendoring artefacts and are documented (when
   relevant) in the chapter that owns the feature they
   support.

## 27.15 External references

- The `bigdecimal` and `num-rational` crates underlying
  `mm2_number`
  ([crates.io/crates/bigdecimal](https://crates.io/crates/bigdecimal),
  [crates.io/crates/num-rational](https://crates.io/crates/num-rational)).
- The Prometheus protocol and exposition format
  ([prometheus.io/docs/instrumenting/exposition_formats/](https://prometheus.io/docs/instrumenting/exposition_formats/)).
- The `tokio` async runtime
  ([tokio.rs](https://tokio.rs/)).
- The `serde` and `serde_json` ecosystem
  ([serde.rs](https://serde.rs/)).
- The libp2p signing primitives consumed by
  `proxy_signature`
  ([libp2p.io](https://libp2p.io/)).

## 27.16 Provenance

At the baseline (`c1d46c0`), `mm2src/` carried 22 crates,
of which the following are listed in this register:
`mm2_err_handle`, `mm2_net`, `mm2_rpc`, `mm2_io`, `mm2_db`,
`hw_common`, `trezor`, `ledger`, `rpc_task`, `mm2_core`,
`db_common`, and the `derives/` family. The rest of the
crates documented above are entirely post-baseline:
`mm2_event_stream`, `mm2_state_machine`, `mm2_metrics`,
`mm2_number`, `mm2_git`, `mm2_net_config`,
`proxy_signature`, and `common/shared_ref_counter`. Crate
existence at baseline was verified with
`git ls-tree c1d46c0 -- mm2src/`.

The chapter does not enumerate per-crate Cargo.toml fields
or per-crate test inventories; those belong in each crate's
own README or in the chapter that owns the crate's
user-visible behaviour.
