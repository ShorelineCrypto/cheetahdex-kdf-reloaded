# Chapter 10 — Server-Sent-Events Streaming Substrate

**Status:** driving-spec.

A reusable in-process event-broker substrate plus a native-only HTTP
transport adapter, together exposing six Server-Sent-Events streamers
under a dedicated RPC namespace, structurally replacing the polling-only
read model the baseline tree carried for live GUI updates.

## 10.1 Executive Summary

The baseline tree carries no first-class real-time event channel:
graphical consumers must poll the JSON-RPC surface for balances, swap
status, order status, and orderbook updates. The substrate bound by this
chapter introduces a structural split into two layers:

1. An in-process publish/subscribe broker exposed as a standalone crate
   substrate (chapter-bound identifier: `mm2_event_stream`), owning
   per-streamer task lifecycles and per-client bounded delivery channels.
2. A native-only HTTP handler at the bound path `GET /event-stream`,
   adapting the broker's per-client receiver into the
   `text/event-stream` wire format.

Activation is bound to a new RPC namespace prefix (`stream::`) with
exactly six `<category>::enable` methods. Deactivation is bound to be
implicit: when an HTTP client connection drops, its bookkeeping is
removed and any streamer that loses its last subscriber is shut down.
Slow clients are bound to be individually back-pressured (events dropped
per slow client) and never block the broadcaster or other clients.

This chapter binds the broker's public crate surface, the streamer trait
contract, the wire-stable streamer origin tags, the HTTP endpoint shape,
the `stream::*` dispatcher routing, the six concrete streamer identities
shipped by the substrate, and the runtime invariants the design relies
on.

## 10.2 Subsystem Shape

The substrate occupies a structural seam between three subsystems:

- the central-context substrate (the broker handle is owned as a field
  on the context and reachable from any subsystem holding the context);
- the JSON-RPC dispatcher (the `stream::*` namespace prefix is bound as
  a dispatcher branch routing to a streamer-activation table);
- the native HTTP server (the `/event-stream` route is bound as an
  additional handler beside the JSON-RPC handler, gated to native-only
  targets).

The substrate is *not* a redesign of the JSON-RPC surface. The six
streamers add a push channel beside the existing pull surface; no
pre-existing read RPC is removed, renamed, or repurposed. Bound rules
(R1–R6) constrain the broker; (R7–R14) constrain the streamer trait
contract and origin tags; (R15–R20) constrain the HTTP endpoint and
namespace; (R21–R25) constrain the originally-bound five concrete
streamers; (R28–R31) bind the sixth (Network) streamer.

## 10.3 Bound Crate Surface

**R1.** The substrate MUST be a single crate. The chapter-bound
identifier is `mm2_event_stream`. Its public surface MUST be exactly the
following five names (any wider or narrower re-export set is a
substrate-shape violation):

- `Event`
- `StreamingManager`
- `Broadcaster`
- `EventStreamer` (trait)
- `NoDataIn`
- `StreamerId`

Plus a pass-through re-export of the asynchronous-channel primitives the
trait surfaces (`mpsc` and `oneshot`), so consumers can implement the
trait without a direct asynchronous-runtime dependency.

**R2.** The crate MUST be structured into four source modules:

| Module    | Bound responsibility                                   |
| --------- | ------------------------------------------------------ |
| `lib`     | Re-export surface only.                                |
| `event`   | The `Event` payload type and its constructors.        |
| `streamer`| The `EventStreamer` trait, `StreamerId`, `Broadcaster`, `NoDataIn`. |
| `manager` | `StreamingManager`, `ClientHandle`, lifecycle tests. |

The four-module split is itself contract: an implementation that places
the trait and the manager in the same module collapses the seam the
substrate relies on for fan-out under a read lock.

**R3.** The broker MUST be cheap-to-clone (an inner shared handle behind
an interior-mutable lock). Cloning the broker MUST NOT copy its
registry; all clones MUST observe the same set of running streamers and
the same client map.

## 10.4 Bound Event Payload

**R4.** The `Event` payload MUST carry exactly three fields: an origin
tag (typed `StreamerId`), a JSON message body, and a boolean error
indicator. All other on-the-wire data (timestamps, ticker, payload
shape) MUST live inside the JSON message body.

**R5.** `Event` constructors MUST return a reference-counted handle
(`Arc<Event>`), so fan-out cloning across many clients is reference-count
increment only. Two constructor names are bound: `Event::new` (normal
event) and `Event::err` (error event). Three reader-side helpers are
bound: `is_error()`, `origin()` returning the wire-stable origin string,
`get()` returning the `(origin, message)` pair.

## 10.5 Bound Streamer Origin Tags

**R6.** The streamer origin tag (`StreamerId`) MUST be an enumeration
with exactly six variants and the following wire-stable display strings
(GUI-visible, treated as part of the SSE contract surface):

| Variant                                | Wire string             |
| -------------------------------------- | ----------------------- |
| Heartbeat                              | `HEARTBEAT`             |
| Balance(ticker)                        | `BALANCE:<ticker>`      |
| Network                                | `NETWORK`               |
| SwapStatus                             | `SWAP_STATUS`           |
| OrderStatus                            | `ORDER_STATUS`          |
| OrderbookUpdate { topic }              | `ORDERBOOK:<topic>`     |

These wire strings MUST be exact byte-for-byte: uppercase, colon
separator before the dynamic component, no whitespace, no padding. They
appear inside every SSE frame's JSON envelope as the `origin` field
(R17).

**R7.** The tag MUST derive equality, hashing, debug, serde, and clone.
The hashable property is load-bearing: it is the registry key under
which the broker deduplicates running streamers (R10).

## 10.6 Bound Streamer Trait Contract

**R8.** The `EventStreamer` trait MUST have the following shape (only
two associated items: an associated input-data type and a
`streamer_id()` accessor; one asynchronous method `handle`):

- An associated `DataInType` constrained `Send`. For self-driven
  streamers (timers, polls) this is the bound uninhabited type
  `NoDataIn`.
- An accessor `streamer_id()` returning `StreamerId`. It MUST be callable
  before `handle` runs; it is the registry key the broker looks up to
  decide spawn-vs-attach (R10).
- An asynchronous method `handle(self, broadcaster, ready_tx,
  shutdown_rx, data_rx)` consuming the streamer by value, taking a
  `Broadcaster` handle, a single-shot `ready_tx` returning
  `Result<(), String>`, a single-shot `shutdown_rx`, and the typed
  data-input receiver.

**R9.** Implementations MUST send exactly one value on `ready_tx`:
`Ok(())` after initialisation succeeds, or `Err(reason)` to abort. A
dropped `ready_tx` MUST be treated as failure by the broker.
Implementations MUST return when `shutdown_rx` resolves; the broker
fires shutdown once the last subscriber leaves.

**R10.** The `NoDataIn` type MUST be an uninhabited enumeration (zero
variants). The data-input receiver always exists for type-erasure
uniformity inside the broker, but for self-driven streamers it can
never yield a value.

## 10.7 Bound Broker State and Lifecycle

**R11.** The `StreamingManager` MUST maintain exactly two internal maps:

- A streamer-registry map keyed by `StreamerId`. Each entry holds the
  single-shot shutdown sender, the set of subscriber client identifiers,
  and a type-erased asynchronous-sender (boxed as
  `dyn Any + Send + Sync`) wrapping the streamer's `DataInType`
  unbounded sender.
- A client-registry map keyed by client identifier (an unsigned 64-bit
  integer). Each entry holds the set of wire-stable origin strings the
  client is subscribed to, plus a bounded asynchronous sender into the
  per-client delivery channel.

**R12.** The broker MUST expose the following methods with these exact
contracts:

| Method                           | Bound behaviour                                                                                                                                                                                                                                                  |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `new_client(client_id)`          | Allocates a bounded asynchronous channel with capacity 256, inserts a client entry with empty subscription set, returns the receiver wrapped as `ClientHandle`.                                                                                                  |
| `add(client_id, streamer)` async | If the streamer is already registered, subscribes the client and adds the origin to the client's subscription set. Otherwise spawns the streamer's `handle` task, records the registry entry with shutdown channel and type-erased data sender, and awaits `ready_rx`. Returns the streamer's ready-reported error verbatim, or maps a dropped `ready_tx` to failure. |
| `stop(client_id, streamer_id)`   | Unsubscribes the client from the named streamer. If the streamer's subscriber set becomes empty, fires the shutdown signal and removes the registry entry.                                                                                                       |
| `remove_client(client_id)`       | Removes the client entirely, calling `stop` for every streamer the client was subscribed to.                                                                                                                                                                     |
| `send<T>(streamer_id, data)`     | Looks up the streamer, downcasts the type-erased data sender to the concrete `T` sender, forwards. Errors if the streamer is not running or the type does not match.                                                                                            |
| `send_fn<T>(streamer_id, fn)`    | As `send`, but constructs the payload only after the running-streamer check.                                                                                                                                                                                    |
| `is_active(streamer_id)`         | Pure registry lookup.                                                                                                                                                                                                                                          |

**R13.** The per-client delivery channel capacity MUST be exactly 256
entries. The fan-out path MUST use the non-blocking try-send variant: a
full client buffer MUST cause the event to be dropped for that client
only, with no effect on the broadcaster or any other client.

**R14.** Fan-out MUST iterate the client map under a read-only lock and
deliver the event to every client whose subscription set contains the
event's origin string. The broker MUST use a non-asynchronous lock (one
that does not yield across acquisition): every critical section that
touches the registry maps is short and must not be held across
asynchronous suspension points.

## 10.8 Bound HTTP Endpoint and Wire Frame

**R15.** The HTTP transport MUST be gated to native targets only.
WebAssembly builds MUST instantiate the broker and accept subscriptions
(so the same streamer activations can drive a WebAssembly-native
delivery channel exposed elsewhere), but MUST NOT carry the HTTP
endpoint.

**R16.** The endpoint MUST be exactly `GET /event-stream` with a single
query parameter `id`, an unsigned 64-bit integer. Missing or
unparseable `id` MUST default to zero. The endpoint MUST respond with
HTTP status 200 and the following header set:

| Header                          | Bound value                                                                                       |
| ------------------------------- | ------------------------------------------------------------------------------------------------- |
| `Content-Type`                  | `text/event-stream`                                                                              |
| `Cache-Control`                 | `no-cache`                                                                                       |
| `Connection`                    | `keep-alive`                                                                                     |
| `Access-Control-Allow-Origin`   | Value read from a chapter-bound central-context accessor `event_stream_access_control()`. |

**R17.** Each event frame MUST be the byte sequence:

```
data: {"origin":"<wire string>","payload":<message JSON>,"error":<bool>}\n\n
```

where `<wire string>` is exactly the R6 display string of the event's
origin tag, `<message JSON>` is the event's JSON message body verbatim,
and `<bool>` is the event's error indicator. No SSE event identifiers,
no named SSE events, no retry directives, and no comment lines are part
of the bound surface.

**R18.** The response body MUST be a chunked stream produced by
unfolding over the per-client receiver returned by `new_client`. When
the underlying connection drops, the substrate MUST call
`remove_client` for the disconnecting identifier (this is the only
deactivation path; there is no explicit unsubscribe RPC).

## 10.9 Bound RPC Namespace

**R19.** A dedicated dispatcher branch MUST be added for the `stream::`
namespace prefix. Methods whose name begins with the four-byte prefix
`stream::` MUST be routed to a streamer-activation table; methods
without the prefix MUST be routed unchanged through the existing v2
dispatcher.

**R20.** The streamer-activation table MUST contain exactly the
following six entries (no aliases, no deprecated names, no additional
methods):

| Method name                  | Streamer key                              |
| ---------------------------- | ----------------------------------------- |
| `stream::heartbeat::enable`  | `Heartbeat`                              |
| `stream::balance::enable`    | `Balance(<request.coin>)`                |
| `stream::network::enable`    | `Network`                                |
| `stream::swap_status::enable`| `SwapStatus`                             |
| `stream::order_status::enable`| `OrderStatus`                            |
| `stream::orderbook::enable`  | `OrderbookUpdate { topic: <request.topic> }` |

The `Network` origin tag reserved in R6 is now bound to its activation
method; its request shape, payload shape, cadence, and platform gate
are bound in §10.16 (R28–R31). This resolves D2.

**R21.** All activation handlers (the six bound in R20/R28) MUST share a
common request and response envelope:

- Request: a generic envelope carrying a `client_id` field (the same
  unsigned 64-bit integer the HTTP endpoint accepts) plus an
  inner per-streamer request flattened beside it.
- Response: the standard mmrpc-2.0 envelope (`mmrpc`, `result`, `id`)
  whose `result` is an object carrying a single string field
  `streamer_id` — the wire-stable identifier of the activated streamer
  (the same `StreamerId` display string bound in R6, e.g. `HEARTBEAT`,
  `BALANCE:<ticker>`, `SWAP_STATUS`, `ORDER_STATUS`,
  `ORDERBOOK:<topic>`). The client MUST retain this string to later
  deactivate the streamer via `stream::disable`. Success is conveyed by
  the mmrpc `result` envelope itself; there is NO boolean field in the
  response.

**R22.** The activation error type MUST be a single-variant enumeration
with display string `Streamer initialization failed: <reason>` and HTTP
status mapping 500. Any failure reported by the streamer's `ready_tx`
MUST be wrapped into this variant verbatim.

## 10.10 Bound Concrete Streamers

**R23.** The substrate MUST ship exactly the following six concrete
streamers, each with its own activation module under a single
streamer-activation directory:

| Streamer module | Activation method            | Streamer key                          |
| --------------- | ---------------------------- | ------------------------------------- |
| `heartbeat`     | `stream::heartbeat::enable`  | `Heartbeat`                          |
| `balance`       | `stream::balance::enable`    | `Balance(<ticker>)`                  |
| `network`       | `stream::network::enable`    | `Network`                            |
| `swaps`         | `stream::swap_status::enable`| `SwapStatus`                         |
| `orders`        | `stream::order_status::enable`| `OrderStatus`                        |
| `orderbook`     | `stream::orderbook::enable`  | `OrderbookUpdate { topic }`          |

The `network` streamer's request, payload, cadence, and placement are
bound in §10.16 (R28–R31); its streamer struct is placed in the
peer-discovery / p2p crate rather than under this directory (R31).

**R24.** The balance streamer's activation request MUST carry exactly
two fields: a coin ticker, and an interval in seconds defaulting to 30,
floored at construction time to a minimum of 10. The streamer's
`handle` MUST:

1. Resolve the coin via the central coin-registry accessor; report
   failure on `ready_tx` if not activated, otherwise report ready.
2. Loop, racing a timer against `shutdown_rx`.
3. On the timer branch, call the coin's balance accessor and broadcast
   a normal event only when the spendable or unspendable balance
   differs from the previous emission; broadcast an error event on
   failure.
4. On the shutdown branch, return.

The bound emit-on-change semantics MUST be observed: an unchanged
balance MUST NOT emit. Each emission's JSON message body MUST carry the
ticker, both balance components as decimal strings, and a timestamp in
milliseconds.

**R25.** The four non-balance streamers MUST follow the same activation
shape (a per-streamer request struct, a streamer struct implementing
the trait, an activation handler that constructs the streamer and
forwards into `add`, mapping any error into the bound activation error
variant). The substrate MUST NOT expose any streamer that is not on
the R23 list.

## 10.11 Bound Central-Context Wiring

**R26.** The central context MUST carry exactly one new public field of
type `StreamingManager`, initialised to the broker's default. Cloning
the central context (which is itself a cheap shared handle) MUST
observe the same broker instance.

**R27.** The central context MUST expose one new accessor
`event_stream_access_control()` returning the configured CORS origin
string used in R16. The accessor MUST read a single named key
(`event_stream_access_control`) from the central configuration and fall
back to a substrate-defined default suitable for locally-hosted
graphical consumers.

## 10.12 Tests

**T1.** *Single-client delivery.* A test client subscribes to a
streamer that emits a single event; the test asserts the event is
delivered to the client's receiver with the bound origin string.

**T2.** *Last-unsubscribe shutdown.* Two clients subscribe to the same
streamer (registry deduplication path); both unsubscribe in sequence;
the test asserts the streamer task observed its `shutdown_rx` resolve
exactly once, after the second unsubscribe.

**T3.** *Multi-client fan-out.* Three clients subscribe; the streamer
emits one event; the test asserts every client's receiver yields the
same `Arc<Event>` (reference-count fan-out, not payload copy).

**T4.** *Slow-client back-pressure.* One client subscribes but does not
drain its receiver; a second client subscribes and drains. The streamer
emits more than 256 events. The test asserts that the slow client's
receiver caps at 256 and that the fast client receives every event.

**T5.** *Wire-frame literalness.* An end-to-end test connects to
`GET /event-stream?id=1`, activates the heartbeat streamer, and
asserts the response body matches the regular expression
`^data: \{"origin":"HEARTBEAT","payload":.*,"error":(true|false)\}\n\n`
for at least one frame.

**T6.** *Namespace routing.* A dispatcher unit test asserts that
`stream::heartbeat::enable` is routed through the streamer-activation
table and not through the v2 method table, and that a method
`stream::nonsense::enable` returns the dispatcher's "no such method"
error.

## 10.13 Deferred Work

**D1.** A WebAssembly-native delivery transport (the WebAssembly target
currently has the broker but no transport adapter; a future substrate
chapter is expected to bind an in-process callback adapter for
embedded WebAssembly consumers).

**D2.** *(Resolved by §10.16, R28–R31.)* Activation of the reserved
`Network` origin tag. Originally deferred (the tag was bound to fix the
wire string while no consumer existed); the consumer now exists, so the
activation method `stream::network::enable`, its request and payload
shapes, its timer-driven emit-on-change cadence, and its ALL-targets
platform gate are bound in §10.16.

**D3.** Per-client authentication and per-client rate limits. The
substrate currently relies on the bound CORS origin (R16) and the
existing JSON-RPC password gate for the activation methods; per-stream
gating is out of scope.

**D4.** Streamer-specific event-history replay (a reconnecting client
currently observes only events that occur after reconnect; replay would
require a per-streamer bounded backlog and a "last-event-id" handshake,
neither of which is bound).

## 10.14 Baseline Verifications

**V1.** The baseline tree MUST be confirmed to contain no
`mm2_event_stream` crate, no `/event-stream` HTTP route, no
`stream::` dispatcher prefix, and no broker field on the central
context. All graphical synchronisation in the baseline tree is
pull-mode through the existing JSON-RPC read surface.

**V2.** The six activation method names bound in R20 MUST be confirmed
absent from the baseline's v2 dispatcher method table. Adding them in
the substrate is a pure surface addition; no baseline method is
renamed or repurposed.

**V3.** The six `StreamerId` wire strings bound in R6 MUST be confirmed
absent from the baseline tree. They are introduced by the substrate
and become part of the GUI-visible contract surface on first release.

## 10.15 External References

- HTML Living Standard, *Server-sent events*,
  <https://html.spec.whatwg.org/multipage/server-sent-events.html> —
  the `text/event-stream` wire format bound in R17.
- WHATWG Fetch, *HTTP Access-Control-Allow-Origin*,
  <https://fetch.spec.whatwg.org/#http-access-control-allow-origin> —
  CORS header bound in R16.
- `tokio::sync` channels (`mpsc`, `oneshot`),
  <https://docs.rs/tokio/latest/tokio/sync/index.html> — the
  asynchronous-channel primitives the trait surfaces in R8 and R10.
- `parking_lot::RwLock`,
  <https://docs.rs/parking_lot/latest/parking_lot/> — the
  non-asynchronous lock bound in R14.
- `async-trait`, <https://crates.io/crates/async-trait> — used by the
  trait definition in R8.
- libp2p gossipsub, <https://docs.rs/libp2p-gossipsub/latest/libp2p_gossipsub/>
  — the peer/topic/mesh introspection surface that dictates the
  `NETWORK` event payload field set bound in R29.

## 10.16 Bound Network Streamer Activation

This section binds activation of the sixth concrete streamer, whose
origin tag (`Network`, wire string `NETWORK`) is already reserved in
R6. It resolves D2: a consumer for the tag now exists (a graphical
peer-connectivity view), so the previously-deferred activation handler
is bound here. The exact wire method name is `stream::network::enable`.

**R28.** A sixth entry MUST be added to the streamer-activation table
(R20) and routed through the `stream::` dispatcher branch (R19):

| Method name                | Streamer key |
| -------------------------- | ------------ |
| `stream::network::enable`  | `Network`    |

The activation handler MUST follow the shared R21/R25 contract:

- Request: the shared envelope carrying `client_id` (unsigned 64-bit,
  defaulting to 0) flattened beside a single per-streamer
  configuration object named `config`. The `config` object MUST carry
  exactly two optional fields, both supplying a default when omitted:

  | Field                     | Type                   | Default | Meaning                                                                                            |
  | ------------------------- | ---------------------- | ------- | -------------------------------------------------------------------------------------------------- |
  | `stream_interval_seconds` | number (float seconds) | `5.0`   | Delay between successive network-snapshot emissions.                                               |
  | `always_send`             | boolean                | `false` | When `true`, emit every cycle even if the snapshot is unchanged; when `false`, emit only on change. |

  Unknown fields inside `config` MUST be rejected. There is no minimum
  floor on `stream_interval_seconds` (unlike the balance streamer's
  10-second floor in R24).
- Response: the single boolean `active` field of R21, returned `true`
  on successful activation. Any streamer initialisation failure MUST be
  surfaced through the bound activation error variant (R22).

**R29.** The `NETWORK` event message body MUST be a JSON object with
exactly the following five fields, describing the node's current
gossipsub / peer-connectivity snapshot. The field names are wire-stable
(GUI-visible interop), byte-for-byte:

| Field                      | JSON value                                             | Semantics                                                                            |
| -------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------ |
| `directly_connected_peers` | object: peer-id string → array of multiaddress strings | Peers the node currently holds live transport connections to, with reachable addresses. |
| `gossip_mesh`              | object: topic string → array of peer-id strings        | Per-topic gossipsub mesh membership.                                                 |
| `gossip_peer_topics`       | object: peer-id string → array of topic strings        | Topics each known peer is subscribed to.                                             |
| `gossip_topic_peers`       | object: topic string → array of peer-id strings        | Peers subscribed to each known topic.                                                |
| `relay_mesh`               | array of peer-id strings                               | Peers in the relay mesh.                                                             |

These five values are dictated by the gossipsub introspection surface
of the peer-discovery substrate; this section binds their presence,
names, and JSON shape, not the internal traversal that produces them.

**R30.** The network streamer MUST be self-driven (input type
`NoDataIn`, R8/R10) and timer-paced:

1. On activation it MUST report ready (R9) after attaching to the
   peer-discovery substrate, then begin its emission loop.
2. Each cycle it MUST assemble the R29 snapshot from the peer-discovery
   substrate, then wait `stream_interval_seconds` before the next cycle.
3. Emit semantics MUST be emit-on-change by default: a cycle whose
   snapshot equals the previously broadcast snapshot MUST NOT emit. The
   first cycle always emits (there is no prior snapshot). When
   `always_send` is `true`, every cycle MUST emit regardless of change.
4. The streamer MUST return when its shutdown signal resolves (R9),
   i.e. when its last subscriber leaves (R12 `stop`).

**R31.** Platform gate: the network streamer activation MUST be bound on
ALL targets (native and WebAssembly). It carries no native-only `cfg`
gate, because the peer-discovery substrate it introspects is present on
every target. Placement: the activation handler module is bound as
`network` under the streamer-activation directory (R23); the streamer
struct itself is bound to live in the peer-discovery / p2p networking
crate (it introspects that crate's gossipsub state), not in
`mm2_event_stream`.

## 10.17 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit; chapter 01 (clean-room rules); chapter 31 (the central
  application-context substrate the broker handle of R26 and the
  `event_stream_access_control()` accessor are bound on); the
  chapter-bound identifier set for the broker substrate, the HTTP
  endpoint, the RPC namespace, and the six concrete streamers;
  public protocol documentation (HTML Living Standard SSE, WHATWG
  CORS); the libp2p gossipsub introspection surface (peer/topic/mesh
  enumeration) that dictates the `NETWORK` payload field set (R29);
  public documentation for the asynchronous-runtime and
  lock crates listed in 10.15.
- *Permitted-input classes used:* baseline source; bound substrate
  identifiers introduced with in-chapter justification; public
  protocol documentation; public crate documentation;
  dictated-interop wire facts (the `stream::network::enable` method
  string, its `config` request fields, and the `NETWORK` event field
  names — all GUI/third-party-visible contract surface).
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* consulted (via the spec-author channel) ONLY for
  §10.16's dictated-interop facts — the network streamer's public wire
  method name, its activation request field names/defaults, the
  `NETWORK` event payload field names and value shapes, and its
  timer-driven emit-on-change cadence and ALL-targets gate. No private
  identifiers, function bodies, control-flow, or string literals were
  carried across; the behaviour is restated as the public contract.
