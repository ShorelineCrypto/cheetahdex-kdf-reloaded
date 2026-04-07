# Chapter 11 — Order-Match Cancellation Race Mitigation

## Executive Summary

The baseline orderbook treats every inbound P2P maker-order message as
authoritative: a `MakerOrderCreated` always inserts/refreshes the order, a
`MakerOrderCancelled` always removes it. Because the gossipsub mesh delivers
messages without ordering guarantees, two scenarios can race in a way the
baseline cannot recover from:

1. A peer cancels an order and immediately republishes (e.g. price update). A
   third-party node may receive **(a) old create → (b) cancel → (c) new
   create** or **(a) cancel → (b) old create that was queued for retransmit**.
2. A network partition heals after a cancellation, replaying buffered create
   messages from peers that did not learn of the cancellation in time.

In both cases the baseline orderbook re-creates a maker order that the owner
has explicitly removed, where it lingers until its keep-alive timer expires
(tens of seconds during which a taker can match against an order the maker
will refuse to honour).

**Why this changed.** The project's own commit log (`P4.1 recently_cancelled
race fix`) records the explicit motivation: *"Add `recently_cancelled`
TimeCache to Orderbook to prevent re-creation of cancelled orders from
out-of-order P2P messages."* This is the post-baseline change documented
here.

The post-baseline tree adds a short-lived (120-second) per-orderbook cache
of recently-cancelled UUIDs, keyed by the cancelling pubkey. The
order-insert path consults the cache and silently drops any create message
that re-uses a UUID this node observed being cancelled by the same pubkey.
Different pubkeys are unaffected, so the mechanism does not interfere with
UUID re-use by other peers (which is already collision-free by construction
— UUIDs are v4 random).

## Reproduction Detail

### 11.1 Baseline shape

At commit `c1d46c0…`, `mm2_main/src/lp_ordermatch.rs::Orderbook` derives
`Default` and holds only the live-state collections (`ordered`, `unordered`,
`order_set`, `pubkeys_state`, `memory_db`, `topics_subscribed_to`,
`pairs_existing_for_*`). `delete_order` simply removes from `order_set` and
the indices; `insert_or_update_order_update_trie` performs no cancellation
check.

### 11.2 Why the change

From the project's own `P4.1` commit message:

> *"Add `recently_cancelled` TimeCache to Orderbook to prevent re-creation
> of cancelled orders from out-of-order P2P messages."*

The change targets a specific gossipsub-mesh failure mode (out-of-order
delivery of create/cancel pairs for the same UUID by the same pubkey). It is
not a redesign of the orderbook; it is a single sentinel collection plus two
small call-site additions, sized to cover the realistic worst-case mesh
delivery skew without permanently blocking legitimate UUID re-use.

### 11.3 New data on `Orderbook`

A single new field is added to the `Orderbook` struct, together with one new
module-level constant:

```rust
/// How long to remember a cancelled order UUID to guard against
/// out-of-order P2P messages.
const RECENTLY_CANCELLED_TIMEOUT: Duration = Duration::from_secs(120);

struct Orderbook {
    /* …existing fields unchanged… */

    /// Recently cancelled order UUIDs mapped to the cancelling pubkey.
    /// Guards against re-creation when a P2P cancel arrives before a
    /// late-delivered create message.
    recently_cancelled: TimeCache<Uuid, String>,
}
```

`TimeCache<K, V>` is the project's existing `common::time_cache::TimeCache`
(introduced in the baseline tree; reused here unchanged). It is a TTL-keyed
map that returns `None` for entries whose insertion time exceeds the
configured timeout.

Because `Orderbook` now has a non-`Default` constructible field (the
`TimeCache` needs the timeout), the previously-derived `Default` is replaced
by a hand-written `impl Default for Orderbook` that initialises every
existing field exactly as before plus:

```rust
recently_cancelled: TimeCache::new(RECENTLY_CANCELLED_TIMEOUT),
```

### 11.4 Cancellation recording

`delete_order(ctx, pubkey, uuid)` records the cancellation **before**
performing any removal, so the record is present even on early-return paths
where the order is not actually present in the local order set:

```rust
fn delete_order(ctx: &MmArc, pubkey: &str, uuid: Uuid) {
    let ordermatch_ctx = OrdermatchContext::from_ctx(ctx)
        .expect("from_ctx failed");
    let mut orderbook = ordermatch_ctx.orderbook.lock();

    // Record this UUID so that a late-arriving create message
    // won't resurrect the order.
    orderbook
        .recently_cancelled
        .insert(uuid, pubkey.to_string());

    if let Some(order) = orderbook.order_set.get(&uuid) {
        if order.pubkey == pubkey {
            orderbook.remove_order_trie_update(uuid);
        }
    }
}
```

Two invariants matter:

- The cache key is the `Uuid`; the value is the `pubkey` (as `String`) that
  originated the cancellation. Storing the pubkey lets the insert-guard
  distinguish "this UUID was cancelled by *this* publisher" from "this UUID
  was cancelled by some other publisher" — only the former is suppressed.
- The insert happens unconditionally. Even if the local node had never seen
  the order, recording the cancellation prevents a subsequent
  late-delivered create from resurrecting it.

### 11.5 Insert-time guard

`insert_or_update_order_update_trie(order)` adds a single early-return at
the top of the function:

```rust
fn insert_or_update_order_update_trie(&mut self, order: OrderbookItem) {
    // Ignore orders that were recently cancelled by their own pubkey.
    if self.recently_cancelled.get(&order.uuid) == Some(&order.pubkey) {
        log::warn!(
            "Order {} was recently cancelled, ignoring insert",
            order.uuid,
        );
        return;
    }

    /* …existing validation and trie-update logic unchanged… */
}
```

The match condition is strict equality of both UUID and pubkey:

- Same UUID, **same** pubkey, within 120 s of a cancellation → drop, log at
  warn level.
- Same UUID, **different** pubkey → proceed normally (UUID collision across
  publishers is treated as it would be without the guard — under v4 UUIDs
  the probability of collision is negligible, and the original publisher's
  cancellation is not authoritative over another publisher's create).
- Same UUID, same pubkey, more than 120 s after the cancellation → cache
  miss → proceed normally (allows legitimate UUID re-use after the mesh has
  certainly converged).

### 11.6 The `TimeCache` contract

`common::time_cache::TimeCache<K, V>` is consumed in this chapter via a tiny
surface:

| Method | Use here |
| --- | --- |
| `TimeCache::new(timeout: Duration) -> Self` | Construct with a fixed per-entry TTL. |
| `insert(key: K, value: V)` | Record a cancellation; resets TTL on repeat. |
| `get(key: &K) -> Option<&V>` | Read; returns `None` if the entry's age exceeds the constructor TTL. |

No other operations are required for the cancellation-race mitigation.

### 11.7 Tests

Two unit tests in `mm2_main/src/ordermatch_tests.rs` exercise the guard:

- **Blocks insert for same pubkey.** Construct an `Orderbook`, insert an
  order, call `delete_order` for the same pubkey/UUID, attempt to re-insert
  an `OrderbookItem` with the same pubkey/UUID; assert the order is **not**
  present afterwards.
- **Allows insert for different pubkey.** Same setup, but the re-insert uses
  a different pubkey; assert the order **is** present afterwards.

Both tests run within the existing in-process orderbook setup; no extra
fixtures are required.

### 11.8 Runtime invariants

| Invariant | Where enforced |
| --- | --- |
| TTL is exactly 120 s. | `RECENTLY_CANCELLED_TIMEOUT` constant. |
| Cancellation is recorded before removal, on the lock held by the caller. | `delete_order` |
| Guard fires only on UUID + pubkey match. | `insert_or_update_order_update_trie` early return |
| Existing UUID-collision behaviour across publishers is preserved. | Guard scoped to `Some(&order.pubkey)` equality |
| `Orderbook::default()` produces a usable instance. | Explicit `impl Default for Orderbook` |

### 11.9 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. In `mm2_main/src/lp_ordermatch.rs` (or, equivalently, whichever submodule
   currently owns `Orderbook` after later structural refactors — see
   chapter 12), import `common::time_cache::TimeCache` and
   `std::time::Duration`.
2. Add a module-level constant `RECENTLY_CANCELLED_TIMEOUT:
   Duration = Duration::from_secs(120)`.
3. Add a new field `recently_cancelled: TimeCache<Uuid, String>` to the
   `Orderbook` struct.
4. Remove `#[derive(Default)]` from `Orderbook` and add an explicit
   `impl Default for Orderbook { fn default() -> Self { … } }` that
   initialises every previously-default field plus
   `recently_cancelled: TimeCache::new(RECENTLY_CANCELLED_TIMEOUT)`.
5. In `delete_order(ctx, pubkey, uuid)`, **before** the existing
   removal logic, insert `orderbook.recently_cancelled.insert(uuid,
   pubkey.to_string())`.
6. At the top of `Orderbook::insert_or_update_order_update_trie(order)`, add
   the early-return guard from §11.5 (UUID + pubkey equality, warn-level
   log, return).
7. Add the two unit tests from §11.7 to the existing orderbook test
   suite.

No other call sites are modified; the guard is invisible to RPC consumers
and to coin implementations.

## External References

- libp2p Gossipsub v1.1 specification,
  <https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md>.
  The "no delivery ordering" property motivates the guard.
- RFC 4122 — UUID v4 random,
  <https://datatracker.ietf.org/doc/html/rfc4122>. Backs the
  negligible-collision assumption that lets the guard scope to UUID +
  pubkey equality.
- `uuid` crate, <https://crates.io/crates/uuid>.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline workspace at commit
  `c1d46c0…`; the post-baseline file
  `mm2src/mm2_main/src/lp_ordermatch.rs` (and its successor
  `mm2src/mm2_main/src/lp_ordermatch/ordermatch_orderbook.rs` after the
  later split documented in chapter 12); the post-baseline test additions
  in `mm2src/mm2_main/src/ordermatch_tests.rs`; the project's own commit
  `96d94bda5` ("P2.3 swap tests + P4.1 recently_cancelled race fix") as
  authoritative source for the stated motivation.
- **Permitted-input classes used:** baseline source; first-party
  post-baseline identifiers introduced with in-chapter justification; the
  project's own commit messages on the post-baseline branch; public
  protocol/RFC documentation.
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree.
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
