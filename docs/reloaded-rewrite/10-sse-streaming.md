# Chapter 10 — Server-Sent-Events Streaming Backbone

## Executive Summary

The baseline tree has no first-class real-time event channel: GUIs poll
JSON-RPC endpoints. The post-baseline tree adds a Server-Sent-Events (SSE)
backbone so a GUI can subscribe to specific event categories (heartbeat,
per-coin balance, swap status, order status, orderbook updates) and receive
push notifications over a single long-lived HTTP response.

The backbone is split into two layers:

- A reusable in-process pub/sub crate, `mm2_event_stream/`, that owns
  per-streamer task lifecycles and per-client bounded channels.
- A native-only HTTP handler at `GET /event-stream?id=<client_id>` that adapts
  the crate's per-client event stream into the
  `data: <json>\n\n` SSE wire format.

Activation is via a new RPC namespace, `stream::*`, with one
`<category>::enable` method per streamer. Disabling is implicit: when an SSE
client connection drops, its bookkeeping is removed and any streamer that
loses its last subscriber is shut down. Slow clients are individually
back-pressured (events dropped per slow client) and never block the
broadcaster or other clients.

This chapter documents the public crate API, the streamer trait contract,
the wire-stable `StreamerId` strings, the HTTP endpoint shape, the `stream::*`
RPC dispatcher, the concrete streamers shipped in this tree (heartbeat,
balance, swap status, order status, orderbook), and the runtime invariants
the design relies on.

## Reproduction Detail

### 10.1 Baseline shape (no SSE)

Commit `c1d46c0…` contains no `mm2_event_stream` crate, no
`/event-stream` HTTP route, no `stream::` RPC namespace, and no event-broker
field on `MmCtx`. All GUI updates are pull-mode via `my_balance`,
`my_swap_status`, `orderbook`, etc.

### 10.2 Crate layout

A new workspace member is added: `mm2src/mm2_event_stream/`.

```
mm2_event_stream/
├── Cargo.toml
└── src/
    ├── lib.rs       — re-exports + crate-level docs
    ├── event.rs     — `Event` payload
    ├── streamer.rs  — `EventStreamer` trait, `StreamerId`, `Broadcaster`, `NoDataIn`
    └── manager.rs   — `StreamingManager`, `ClientHandle`
```

Public re-exports from `lib.rs`:

```rust
pub use event::Event;
pub use manager::StreamingManager;
pub use streamer::{Broadcaster, EventStreamer, NoDataIn, StreamerId};
pub use tokio::sync::{mpsc, oneshot};
```

The `tokio::sync::{mpsc, oneshot}` re-export lets consumer crates use the
async channel primitives the `EventStreamer` trait surfaces without a direct
`tokio` dependency.

### 10.3 `Event` payload

```rust
pub struct Event {
    streamer_id: StreamerId,
    message:     serde_json::Value,
    error:       bool,
}
```

Constructors return `Arc<Self>` so fan-out cloning is cheap:

- `Event::new(streamer_id, message) -> Arc<Self>` — normal event.
- `Event::err(streamer_id, message) -> Arc<Self>` — error event.
- `is_error()`, `origin() -> String`, `get() -> (String, &Json)` are the
  reader-side helpers.

`origin()` returns the wire-stable string form of the originating `StreamerId`.

### 10.4 `StreamerId` — wire-stable origin tags

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamerId {
    Heartbeat,
    Balance(String),
    Network,
    SwapStatus,
    OrderStatus,
    OrderbookUpdate { topic: String },
}
```

Wire strings (from the `Display` impl, part of the SSE contract):

| Variant | Wire form |
| --- | --- |
| `Heartbeat` | `HEARTBEAT` |
| `Balance("KMD")` | `BALANCE:KMD` |
| `Network` | `NETWORK` |
| `SwapStatus` | `SWAP_STATUS` |
| `OrderStatus` | `ORDER_STATUS` |
| `OrderbookUpdate { topic: "KMD/BTC" }` | `ORDERBOOK:KMD/BTC` |

These strings are GUI-visible and must not be renamed without coordinating
consumers.

### 10.5 `EventStreamer` trait

```rust
#[async_trait]
pub trait EventStreamer: Sized + Send + 'static {
    type DataInType: Send;

    fn streamer_id(&self) -> StreamerId;

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        data_rx: mpsc::UnboundedReceiver<Self::DataInType>,
    );
}
```

Contract:

- `streamer_id()` is queried before `handle` to key the streamer in the
  manager registry.
- `handle` is the streamer's main task. It is spawned exactly once, on the
  first subscribe.
- The implementation **must** send exactly one value on `ready_tx`: `Ok(())`
  to signal it is initialised, or `Err(String)` to abort. The manager treats
  a dropped `ready_tx` as failure.
- `handle` should return when `shutdown_rx` resolves; the manager fires
  shutdown once the last subscriber leaves.
- For self-driven streamers (e.g. timers, polls) that do not consume external
  pushes, set `type DataInType = NoDataIn;`. `NoDataIn` is an uninhabited
  enum, so the `data_rx` will never yield a value, but the channel still
  exists for type-erasure uniformity inside the manager.

### 10.6 `Broadcaster`

Cheap-to-clone handle threaded into the running streamer:

```rust
pub struct Broadcaster {
    pub(crate) inner: Arc<parking_lot::RwLock<StreamingManagerInner>>,
}

impl Broadcaster {
    pub fn broadcast(&self, event: Arc<Event>) {
        let inner = self.inner.read();
        let origin = event.origin();
        for client in inner.clients.values() {
            if client.listening_to.contains(&origin) {
                let _ = client.tx.try_send(event.clone());
            }
        }
    }
}
```

Two invariants matter:

- `try_send` is best-effort. If a client's bounded channel is full, the event
  is dropped for that client; the broadcast does not block.
- The fan-out scans the full client map under a read lock; a client only
  receives the event if its `listening_to` set contains the origin string.

### 10.7 `StreamingManager`

```rust
#[derive(Clone, Default)]
pub struct StreamingManager { inner: Arc<RwLock<StreamingManagerInner>> }

pub struct ClientHandle { pub rx: mpsc::Receiver<Arc<Event>> }
```

The internal state holds two maps:

- `streamers: HashMap<StreamerId, StreamerInfo>` — per running streamer:
  `shutdown_tx: Option<oneshot::Sender<()>>`, `subscribers: HashSet<u64>`,
  and `data_in: Box<dyn Any + Send + Sync>` (type-erased `mpsc::UnboundedSender<T>`).
- `clients: HashMap<u64, ClientInfo>` — per SSE client: `listening_to:
  HashSet<String>`, `tx: mpsc::Sender<Arc<Event>>`.

Public methods:

| Method | Behaviour |
| --- | --- |
| `new_client(client_id: u64) -> ClientHandle` | Creates a bounded `mpsc::channel(256)` for the client, inserts a `ClientInfo` with empty `listening_to`, returns the `rx`. |
| `add<S: EventStreamer>(client_id, streamer) -> Result<(), String>` async | If the streamer is already running, subscribes `client_id` to it and adds the origin string to `client.listening_to`. Otherwise spawns the streamer task (`common::executor::spawn`), records it in the registry with a `oneshot` shutdown channel and a type-erased `mpsc::UnboundedSender<S::DataInType>`, and waits on `ready_rx`. Returns `Err` if the streamer's `ready_tx` reports failure or is dropped. |
| `stop(client_id, streamer_id)` | Unsubscribes `client_id` from the specific streamer. If subscribers becomes empty, sends the shutdown signal and removes the streamer's registry entry. |
| `remove_client(client_id)` | Removes the client, unsubscribes it from every streamer it was listening to, and shuts down any streamer whose subscriber set just became empty. |
| `send<T: Send + 'static>(streamer_id, data) -> Result<(), String>` | Looks up the streamer, downcasts `data_in` to `mpsc::UnboundedSender<T>`, sends. Returns `Err` if the streamer is not running or the type does not match. |
| `send_fn<T>(streamer_id, data_fn: impl FnOnce() -> T)` | Same as `send` but constructs the payload only after the streamer-running check. |
| `is_active(streamer_id) -> bool` | Pure registry lookup. |

Per-client receive channels are sized to a fixed buffer of 256 `Arc<Event>`
entries (`mpsc::channel(256)`). When a slow client fills its buffer, the
broadcaster's `try_send` returns `Err` and the event is dropped for that
client only.

The registry's `Default` impl is the only way to construct an instance; it
is stored on `MmCtx`:

```rust
pub event_stream_manager: StreamingManager,
```

### 10.8 Native HTTP endpoint

```rust
#[cfg(not(target_arch = "wasm32"))]
pub const SSE_ENDPOINT: &str = "/event-stream";

#[cfg(not(target_arch = "wasm32"))]
pub async fn handle_sse(req: http::request::Parts, ctx_h: u32)
    -> hyper::Response<hyper::Body>;
```

Wire shape:

- Method: `GET /event-stream?id=<u64>`. Missing or unparseable `id` defaults
  to `0`.
- Response status: `200`.
- Response headers:
  - `Content-Type: text/event-stream`
  - `Cache-Control: no-cache`
  - `Connection: keep-alive`
  - `Access-Control-Allow-Origin: <value of ctx.event_stream_access_control()>`
    (defaults from `event_stream_access_control` in `mm2_ctx.conf`).
- Response body: a chunked stream produced by `futures::stream::unfold` over
  the per-client `mpsc::Receiver<Arc<Event>>` returned by
  `StreamingManager::new_client`. Each chunk is the bytes of
  ```
  data: {"origin":"<StreamerId display>","payload":<event JSON>,"error":<bool>}\n\n
  ```

The endpoint is gated `#[cfg(not(target_arch = "wasm32"))]`; in WASM builds
the streaming manager still exists and accepts subscriptions, but the HTTP
endpoint is absent.

### 10.9 `stream::*` RPC namespace

In `mm2_main/src/rpc/dispatcher/dispatcher.rs`, the v2 dispatcher routes any
method whose name begins with `stream::` to a dedicated
`rpc_streaming_dispatcher`:

```rust
async fn rpc_streaming_dispatcher(
    request: MmRpcRequest,
    ctx: MmArc,
    streaming_method: &str,
) -> DispatcherResult<Response<Vec<u8>>> {
    match streaming_method {
        "balance::enable"       => handle_mmrpc(ctx, request, streaming_activations::balance::enable_balance).await,
        "heartbeat::enable"     => handle_mmrpc(ctx, request, streaming_activations::heartbeat::enable_heartbeat).await,
        "order_status::enable"  => handle_mmrpc(ctx, request, streaming_activations::orders::enable_order_status).await,
        "orderbook::enable"     => handle_mmrpc(ctx, request, streaming_activations::orderbook::enable_orderbook).await,
        "swap_status::enable"   => handle_mmrpc(ctx, request, streaming_activations::swaps::enable_swap_status).await,
        _ => MmError::err(DispatcherError::NoSuchMethod),
    }
}
```

All `enable_*` handlers share the request/response envelope and error type
in `streaming_activations/mod.rs`:

```rust
#[derive(Deserialize)]
pub struct EnableStreamingRequest<T> {
    pub client_id: u64,
    #[serde(flatten)]
    pub inner: T,
}

#[derive(Serialize)]
pub struct EnableStreamingResponse { pub active: bool }

#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum StreamingError {
    #[display(fmt = "Streamer initialization failed: {}", _0)]
    InitFailed(String),
}
```

`StreamingError::InitFailed` maps to HTTP 500 via `HttpStatusCode`.

### 10.10 Concrete streamers in this tree

`mm2_main/src/rpc/streaming_activations/` ships five streamers, each in its
own module:

| Module | RPC method | Streamer key |
| --- | --- | --- |
| `heartbeat.rs` | `stream::heartbeat::enable` | `StreamerId::Heartbeat` |
| `balance.rs` | `stream::balance::enable` | `StreamerId::Balance(ticker)` |
| `swaps.rs` | `stream::swap_status::enable` | `StreamerId::SwapStatus` |
| `orders.rs` | `stream::order_status::enable` | `StreamerId::OrderStatus` |
| `orderbook.rs` | `stream::orderbook::enable` | `StreamerId::OrderbookUpdate { topic }` |

The balance streamer is illustrative of the pattern (the others follow the
same shape). Its activation request is:

```rust
#[derive(Deserialize)]
pub struct EnableBalanceRequest {
    pub coin: String,
    #[serde(default = "default_interval")]
    pub interval_secs: u64, // default 30, floored to >= 10 at construction
}
```

Its `handle` body:

1. Look up the coin via `lp_coinfind`; report `Err` over `ready_tx` if not
   activated, otherwise send `Ok(())`.
2. Loop: race a `Timer::sleep(interval_secs)` against `shutdown_rx`.
3. On the timer branch, call `coin.my_balance()` and broadcast a normal
   `Event` only when the spendable or unspendable value differs from the
   last emission; broadcast an error `Event` on failure.
4. On the shutdown branch, return.

Each event payload includes the coin ticker, both balance components as
decimal strings, and `common::now_ms()` as a `timestamp` field. Errors are
broadcast with `Event::err`.

### 10.11 Runtime invariants

| Invariant | Where enforced |
| --- | --- |
| One streamer instance per `StreamerId`; first subscribe spawns it, last unsubscribe shuts it down. | `StreamingManager::add` / `remove_client` / `stop` |
| Per-client buffers are bounded; slow clients only drop their own events. | `new_client` (`mpsc::channel(256)`) + `Broadcaster::broadcast` (`try_send`) |
| `StreamerId::Display` strings are part of the SSE wire surface. | `streamer.rs` doc comments |
| The HTTP endpoint is native-only. | `#[cfg(not(target_arch = "wasm32"))]` |
| Streamer type-erased input must match `DataInType` exactly. | `StreamingManager::send` downcast |
| A streamer that drops its `ready_tx` is treated as failed and removed. | `add()` ready-handling branch |

### 10.12 Tests in the crate

`manager.rs` ships three tokio unit tests that exercise the lifecycle:

- `should_deliver_event_when_client_subscribes`
- `should_shut_down_streamer_when_last_client_removed`
- `should_share_streamer_when_multiple_clients_subscribe`

These also serve as the minimum acceptance criteria for the crate.

### 10.13 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. Add a new workspace member `mm2src/mm2_event_stream/` with the four
   source files listed in §10.2. Cargo dependencies: `async-trait`,
   `parking_lot`, `serde`, `serde_json`, `tokio` (with `sync` feature),
   `common` (for `executor::spawn`).
2. Implement `Event` per §10.3 (Arc-returning constructors, `is_error`,
   `origin`, `get`, manual `Debug`).
3. Implement `StreamerId` per §10.4 with the six variants and the `Display`
   wire strings exactly as listed.
4. Implement `Broadcaster` per §10.6 (read-locked iteration over clients,
   `try_send` per matching client).
5. Implement `NoDataIn` as `pub enum NoDataIn {}`.
6. Implement the `EventStreamer` trait per §10.5; document the
   `ready_tx`/`shutdown_rx` contract in doc comments.
7. Implement `StreamingManager` and `ClientHandle` per §10.7. Per-client
   buffer size is `256`. Use `parking_lot::RwLock`. Spawn via
   `common::executor::spawn`. Type-erase the per-streamer data sender via
   `Box<dyn Any + Send + Sync>` storing an `mpsc::UnboundedSender<T>`;
   downcast with `downcast_ref::<mpsc::UnboundedSender<T>>()` in `send`
   and `send_fn`.
8. Add the three lifecycle tests (§10.12) to `manager.rs`.
9. In `mm2_core::mm_ctx::MmCtx`, add a public field
   `event_stream_manager: StreamingManager` and initialise it to
   `StreamingManager::default()`. Also add the JSON accessor
   `event_stream_access_control()` that reads
   `conf["event_stream_access_control"]` and falls back to a sensible CORS
   default for local GUIs.
10. Add `mm2_main/src/rpc/sse_handler.rs` with `SSE_ENDPOINT =
    "/event-stream"` and `handle_sse(req, ctx_h)` per §10.8. Wire it into
    the native HTTP router so `GET /event-stream` is routed to `handle_sse`.
11. Add `mm2_main/src/rpc/streaming_activations/mod.rs` with
    `EnableStreamingRequest<T>`, `EnableStreamingResponse`, and
    `StreamingError` per §10.9, plus `pub mod` declarations for the five
    streamer modules.
12. Implement the five streamer modules per §10.10. Each module defines:
    (a) a per-streamer request struct deserialised inside
    `EnableStreamingRequest<…>::inner`; (b) a struct implementing
    `EventStreamer` with the assigned `StreamerId`; (c) an async RPC handler
    that constructs the streamer and calls
    `ctx.event_stream_manager.add(client_id, streamer)`, mapping `Err` into
    `StreamingError::InitFailed`.
13. In the v2 dispatcher, before the per-method match, add the prefix-strip
    branch that routes `stream::*` methods to `rpc_streaming_dispatcher`
    (§10.9). Add the match arms for the five `enable` methods.
14. Verify with the three crate tests and an end-to-end smoke: connect to
    `GET /event-stream?id=1`, call `stream::heartbeat::enable` with
    `{"client_id":1}`, observe `data: {…"origin":"HEARTBEAT"…}\n\n` lines
    on the long-lived response.

## External References

- HTML Living Standard, "Server-sent events",
  <https://html.spec.whatwg.org/multipage/server-sent-events.html>. Specifies
  the `text/event-stream` wire format used in §10.8.
- WHATWG Fetch — `Access-Control-Allow-Origin`,
  <https://fetch.spec.whatwg.org/#http-access-control-allow-origin>.
- IETF RFC 6585 — additional HTTP status codes (informational).
- `tokio::sync` channels (`mpsc`, `oneshot`),
  <https://docs.rs/tokio/latest/tokio/sync/index.html>.
- `parking_lot::RwLock`, <https://docs.rs/parking_lot/latest/parking_lot/>.
- `async-trait`, <https://crates.io/crates/async-trait>.
- `serde` and `serde_json`, <https://crates.io/crates/serde>.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline workspace at commit
  `c1d46c0…`; the post-baseline files
  `mm2src/mm2_event_stream/src/{lib,event,streamer,manager}.rs`,
  `mm2src/mm2_main/src/rpc/sse_handler.rs`,
  `mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs` (stream-dispatcher
  section), `mm2src/mm2_main/src/rpc/streaming_activations/{mod,balance,
  heartbeat,swaps,orders,orderbook}.rs`, and the
  `event_stream_manager`/`event_stream_access_control` additions in
  `mm2_core/src/mm_ctx.rs`.
- **Permitted-input classes used:** baseline source; first-party
  post-baseline identifiers introduced with in-chapter justification; public
  protocol documentation (WHATWG SSE, CORS); public Rust crates (tokio,
  parking_lot, serde, async-trait).
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree.
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
