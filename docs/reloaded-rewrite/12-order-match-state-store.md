# Chapter 12 — Maker-Order State Store

## Executive Summary

The baseline tree stores live maker orders in a flat
`HashMap<Uuid, Arc<AsyncMutex<MakerOrder>>>` held inside the
`OrdermatchContext`. Two consequences follow:

- **No per-order TTL.** A maker order lives forever unless the operator
  explicitly cancels it or the process exits. There is no in-process
  facility to express *"keep this offer up for the next hour, then drop
  it."* GUIs and the simple-market-maker bot work around this by polling
  and re-issuing cancels on a wall-clock schedule from outside the
  ordermatch loop.
- **Linear queries by coin ticker.** Common predicates such as *"does this
  wallet have any active maker orders for KMD?"* require iterating the
  whole map and locking each `Arc<AsyncMutex<MakerOrder>>` to read its
  `base` field. As wallet RPCs perform this check on every coin
  enable/disable and on several internal codepaths, the cost is paid
  often even when most coins have no orders.

**Why this changed.** The project's own commit log (`P4.6 MakerOrdersContext
with TimedMap + order timeout`) records the explicit motivation:

> *"Replace the flat `HashMap<Uuid, Arc<AsyncMutex<MakerOrder>>>` with a new
> `MakerOrdersContext` wrapper that brings TimedMap-backed order storage,
> ticker tracking, expiry handling in `lp_ordermatch_loop`, and a
> `SetPriceReq.timeout_in_minutes` option."*

The change is therefore a focused refactor of a single field
(`OrdermatchContext::maker_orders_ctx`) plus an opt-in user-facing
parameter on the order-creation RPC.

The post-baseline tree:

1. Adds a thin `MakerOrdersContext` struct that wraps a `TimedMap` keyed
   the same way the previous `HashMap` was, and maintains two
   reverse-lookup indices: `order_tickers` (uuid → ticker) and
   `count_by_tickers` (ticker → live-count).
2. Plumbs an optional `timeout_in_minutes: Option<u16>` through the
   user-facing order-creation request, the `MakerOrderBuilder`, and the
   `MakerOrder` struct itself.
3. Hooks a `drop_expired()` call into the existing `lp_ordermatch_loop`
   tick so expired orders are cancelled with the same P2P notification
   and persistence treatment as any other cancellation.
4. Adds a new crate dependency, `timed-map 1.6` (with the `rustc-hash`
   and, on WASM, the `wasm` feature), used solely as the backing store
   for the new context.

## Reproduction Detail

### 12.1 Baseline shape

At commit `c1d46c0…`, `OrdermatchContext` holds:

```rust
pub my_maker_orders: AsyncMutex<HashMap<Uuid, Arc<AsyncMutex<MakerOrder>>>>,
```

Reads use `my_maker_orders.lock().await.clone()` and then iterate; writes
take the same lock and mutate the map directly. There is no TTL machinery
and no ticker index. `MakerOrder` has no `timeout_in_minutes` field.

### 12.2 Why the change

From the project's own `P4.6` commit message (verbatim points):

- *TimedMap-backed order storage — orders with `timeout_in_minutes` get
  automatic TTL via `insert_expirable_unchecked`; orders without it live
  until explicitly cancelled.*
- *Ticker tracking (`order_tickers`, `count_by_tickers`) — enables O(1)
  `coin_has_active_maker_orders()` queries.*
- *Expiry handling in `lp_ordermatch_loop` — `drop_expired()` collects
  stale orders each iteration, sends P2P cancel notification and persists
  `MakerOrderCancellationReason::Expired`.*
- *`SetPriceReq.timeout_in_minutes` — optional per-order timeout plumbed
  through `MakerOrderBuilder` to `MakerOrder`.*
- *Dependencies: adds `timed-map 1.6` (`rustc-hash` feature; `wasm`
  feature for `wasm32` target).*

Each of these is a concrete, narrow goal. The chapter follows the same
order in §§12.3 – 12.7.

### 12.3 New container: `MakerOrdersContext`

The new struct lives in `mm2_main/src/lp_ordermatch.rs` (its callers were
later moved to `lp_ordermatch/ordermatch_trading.rs` by the chapter-13
file-split refactor, but the container itself stays in the hub module):

```rust
use timed_map::TimedMap;

pub struct MakerOrdersContext {
    orders: TimedMap<Uuid, Arc<AsyncMutex<MakerOrder>>>,
    /// uuid → base ticker, for reverse lookup on removal.
    order_tickers:    HashMap<Uuid, String>,
    /// base ticker → count of active orders for that ticker.
    count_by_tickers: HashMap<String, usize>,
}
```

The container replaces the old `my_maker_orders` field on
`OrdermatchContext`. The new field is:

```rust
pub maker_orders_ctx: PaMutex<MakerOrdersContext>,
```

Two design notes:

- The lock is changed from `AsyncMutex` to `parking_lot::Mutex`
  (`PaMutex`) because every operation on the new container completes
  without I/O and without holding the lock across `.await`. Holding a
  contention-prone shared map across `.await` is exactly what the new
  layout avoids.
- The container is *not* `Clone`; callers who need a snapshot of the live
  orders use the explicit `clone_orders()` method (§12.4) which returns a
  plain `HashMap`, leaving the container free to mutate independently.

### 12.4 Public method surface

```rust
impl MakerOrdersContext {
    fn new() -> Self;                              // private constructor

    /// Insert from a `MakerOrder`; chooses expirable vs constant based on
    /// `order.timeout_in_minutes`.
    pub fn add_order(&mut self,
                     order: &MakerOrder,
                     order_arc: Arc<AsyncMutex<MakerOrder>>);

    /// Insert with caller-supplied ticker and optional timeout — used by
    /// taker→maker conversion and kick-start where the Arc already exists.
    pub fn insert_raw(&mut self,
                      uuid: Uuid,
                      ticker: String,
                      order_arc: Arc<AsyncMutex<MakerOrder>>,
                      timeout_in_minutes: Option<u16>);

    pub fn remove_order(&mut self, uuid: &Uuid)
        -> Option<Arc<AsyncMutex<MakerOrder>>>;

    pub fn get_order(&self, uuid: &Uuid)
        -> Option<Arc<AsyncMutex<MakerOrder>>>;

    pub fn contains_key(&self, uuid: &Uuid) -> bool;

    /// O(1) by-coin live-presence test backed by `count_by_tickers`.
    pub fn coin_has_active_maker_orders(&self, ticker: &str) -> bool;

    /// Snapshot for lock-free iteration by callers.
    pub fn clone_orders(&self)
        -> HashMap<Uuid, Arc<AsyncMutex<MakerOrder>>>;

    /// Collect (and remove) all expired entries; the caller is expected to
    /// run P2P cancellation and persistence on each.
    pub fn drop_expired(&mut self)
        -> Vec<(Uuid, Arc<AsyncMutex<MakerOrder>>)>;

    pub fn keys(&self) -> Vec<Uuid>;
    pub fn len(&self) -> usize;
    pub fn iter(&self)
        -> impl Iterator<Item = (&Uuid, &Arc<AsyncMutex<MakerOrder>>)>;
}

impl Default for MakerOrdersContext {
    fn default() -> Self { Self::new() }
}
```

The TTL branch is uniform across `add_order` and `insert_raw`:

```rust
if let Some(t) = timeout_in_minutes {
    self.orders.insert_expirable_unchecked(
        uuid,
        order_arc,
        Duration::from_secs(u64::from(t) * 60),
    );
} else {
    self.orders.insert_constant_unchecked(uuid, order_arc);
}
```

`add_order` derives `timeout_in_minutes` from `order.timeout_in_minutes`;
`insert_raw` takes it as a parameter so callers (kick-start, taker→maker
conversion) can preserve the original timeout when reconstructing an Arc.

Both indices are updated atomically in the same `&mut self` method, so
`order_tickers` and `count_by_tickers` cannot drift relative to `orders`
under normal use. `remove_order` and the expiry path symmetrically
decrement using `saturating_sub` to defend against accidental
double-decrements during refactors.

### 12.5 User-facing field: `SetPriceReq.timeout_in_minutes`

The order-creation legacy RPC `setprice` (and its v2 equivalent) gains a
single new optional field on its request struct (defined in
`lp_ordermatch/ordermatch_types.rs::SetPriceReq`):

```rust
pub struct SetPriceReq {
    /* …existing fields… */
    pub timeout_in_minutes: Option<u16>,
}
```

The value flows verbatim through `MakerOrderBuilder::with_timeout` into
`MakerOrder::timeout_in_minutes: Option<u16>` (also new), which is what
`MakerOrdersContext::add_order` reads. Unit `u16` (max 65535 minutes ≈ 45
days) is more than sufficient for any realistic on-screen offer; missing
or `None` preserves the baseline "never auto-expire" behaviour, so
existing RPC clients are unaffected.

The order persistence layer (`my_orders_storage`) defaults
`timeout_in_minutes` to `None` when reconstructing legacy on-disk orders
that pre-date this field, so persisted orders also keep their baseline
semantics.

### 12.6 Expiry tick in `lp_ordermatch_loop`

`lp_ordermatch_loop` is the existing periodic worker that already handles
order maintenance (e.g. keep-alive broadcasts). The expiry hook is a
single new block executed each iteration before any other per-order
work:

```rust
let expired = ordermatch_ctx
    .maker_orders_ctx
    .lock()
    .drop_expired();

for (uuid, order_mutex) in expired {
    let order = order_mutex.lock().await.clone();
    delete_my_maker_order(
        ctx.clone(),
        order,
        MakerOrderCancellationReason::Expired,
    )
    .compat()
    .await
    .ok();
}
```

Each expired entry:

- has already been removed from `orders`, `order_tickers`, and
  `count_by_tickers` inside `drop_expired()`;
- is then sent through the existing `delete_my_maker_order` cancellation
  pipeline, which (a) broadcasts the P2P `MakerOrderCancelled` message,
  (b) records the order in the maker-orders history with cancellation
  reason `Expired`, and (c) removes the on-disk active-order JSON.

`MakerOrderCancellationReason` is the existing enum (baseline); a new
variant `Expired` is added alongside the pre-existing variants such as
`InsufficientBalance`, `Fulfilled`, `Cancelled`.

### 12.7 New dependency: `timed-map 1.6`

Added to `mm2_main/Cargo.toml`:

```toml
timed-map = { version = "1.6", features = ["rustc-hash"] }
```

For WASM builds the same crate is enabled with the `"wasm"` feature so
its time source uses `js_sys::Date::now()` rather than `std::time`. The
crate is single-purpose (a `HashMap`-shaped store with optional per-entry
TTLs) and exposes exactly the four methods used by
`MakerOrdersContext`: `insert_expirable_unchecked`,
`insert_constant_unchecked`, `drop_expired_entries`, and the standard
`remove`/`get`/`contains_key`/`iter`/`keys`/`len` set. The chapter
relies only on this documented surface; no internal `timed-map` types are
referenced.

The dependency is justified by the alternative implementations that were
not chosen:

- Hand-rolled "uuid → (Arc, expiry)" map plus a min-heap of expiries:
  doable but reintroduces the ordering machinery that `timed-map`
  already provides and tests.
- Per-order spawned tokio task with a `tokio::time::sleep`: cheap to write
  but multiplies wakeups and complicates shutdown.
- Wall-clock scan over the live map every tick: equivalent in coverage to
  `drop_expired_entries`, but quadratic against an N-way grow as the
  count of expirable orders rises.

Using `timed-map` keeps the active-order data structure single-purpose and
its semantics testable in isolation.

### 12.8 Runtime invariants

| Invariant | Where enforced |
| --- | --- |
| `order_tickers` and `count_by_tickers` stay consistent with `orders`. | All mutators are `&mut self` on the same struct; index updates colocated with map updates. |
| `coin_has_active_maker_orders` is O(1). | Implemented as `count_by_tickers.get(ticker).copied() > Some(0)`. |
| Expired orders go through the existing cancellation pipeline (P2P + persistence). | `lp_ordermatch_loop` block in §12.6. |
| Lock is non-async (`PaMutex`). | Field declaration on `OrdermatchContext`. |
| Baseline RPC clients keep baseline semantics (no auto-expiry). | `SetPriceReq.timeout_in_minutes: Option<u16>` defaults to `None`; `add_order`'s `None` branch uses `insert_constant_unchecked`. |
| Old on-disk orders deserialise without a `timeout_in_minutes` field. | `my_orders_storage` reconstructs with `timeout_in_minutes: None`. |

### 12.9 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. Add `timed-map = { version = "1.6", features = ["rustc-hash"] }` to
   `mm2src/mm2_main/Cargo.toml`. Gate the `wasm` feature on the
   `wasm32` target via Cargo's target-specific feature syntax.
2. Add an optional `timeout_in_minutes: Option<u16>` field to the
   `MakerOrder` struct (in `lp_ordermatch.rs` per the baseline layout).
   Default it to `None` in every existing constructor and in serde
   deserialisation.
3. Add the same field to the user-facing `SetPriceReq` (the JSON request
   for `setprice` and its v2 equivalent). Document it as "optional;
   minutes; if absent the order has no automatic expiry."
4. Add `MakerOrderBuilder::with_timeout(self, Option<u16>) -> Self` and
   plumb the value through to the `MakerOrder` it builds.
5. Add `MakerOrderCancellationReason::Expired` variant alongside the
   existing variants of that enum.
6. Define `pub struct MakerOrdersContext { … }` per §12.3, the method
   surface per §12.4, and the `Default` impl.
7. Replace `my_maker_orders: AsyncMutex<HashMap<…>>` on
   `OrdermatchContext` with `maker_orders_ctx:
   PaMutex<MakerOrdersContext>`. Initialise to
   `PaMutex::new(MakerOrdersContext::default())` in every construction
   site.
8. Update every existing call site (insert/remove/iterate/snapshot) to
   use the new methods. The lock changes from
   `.lock().await` to `.lock()`; iteration sites that used to hold the
   lock across `.await` must capture a `clone_orders()` snapshot first
   and drop the lock immediately.
9. In `lp_ordermatch_loop`, add the expiry block from §12.6 near the top
   of the per-tick body, before any other per-order work.
10. In `my_orders_storage` native and WASM impls, ensure
    `timeout_in_minutes: None` is supplied when reconstructing a
    `MakerOrder` from an old persisted record that has no such field
    (use serde's `#[serde(default)]` on the new struct field).
11. Update the existing ordermatch test suite to cover: insert with
    timeout, expiry happens after the configured duration, index counts
    decrement on remove and on expiry, `coin_has_active_maker_orders`
    matches the live count, kick-start via `insert_raw` preserves
    `timeout_in_minutes`.

No public RPC method is added or removed; the only externally-visible
change is the new optional `timeout_in_minutes` request field, which is
backward-compatible.

## External References

- `timed-map` crate, <https://crates.io/crates/timed-map>. Used at
  major version 1, with the `rustc-hash` feature and (for WASM
  builds) the `wasm` feature.
- `rustc-hash` crate, <https://crates.io/crates/rustc-hash>.
- `parking_lot` crate (`Mutex`), <https://crates.io/crates/parking_lot>.
- `uuid` crate, <https://crates.io/crates/uuid>.
- `serde` and `serde_json`, <https://crates.io/crates/serde>.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline workspace at commit
  `c1d46c0…`; the post-baseline files
  `mm2src/mm2_main/src/lp_ordermatch.rs`,
  `mm2src/mm2_main/src/lp_ordermatch/ordermatch_types.rs`,
  `mm2src/mm2_main/src/lp_ordermatch/ordermatch_trading.rs`,
  `mm2src/mm2_main/src/lp_ordermatch/my_orders_storage.rs`,
  `mm2src/mm2_main/Cargo.toml`; the project's own commit
  `41b041d37` ("P4.6 MakerOrdersContext with TimedMap + order
  timeout") as authoritative source for the stated motivation; chapter
  11 (cross-reference to the orderbook-side cancellation guard).
- **Permitted-input classes used:** baseline source; first-party
  post-baseline identifiers introduced with in-chapter justification;
  the project's own commit messages on the post-baseline branch; public
  Rust crates (`timed-map`, `parking_lot`, `uuid`, `serde`).
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree.
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
