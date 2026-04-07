# Chapter 09 — Third-Party Swap-Watcher Infrastructure

## Executive Summary

The baseline tree's atomic-swap protocol is strictly two-party: a maker and a
taker exchange HTLC transactions, and if either side disappears mid-swap the
remaining party must wait out the timelock and broadcast the refund itself.
There is no concept of a third-party observer.

The post-baseline tree adds a **swap-watcher layer**: any other node on the
gossip overlay can volunteer to monitor a taker's in-flight swap, and if the
taker disappears, the watcher can broadcast precomputed spend or refund
transactions on the taker's behalf so the maker is paid and the taker is
refunded without the taker process being online. This chapter documents the
wire envelope, the gossipsub topic family, the watcher state machine, and the
small surface every coin family needs to declare to participate.

A separate, narrower change in the same direction is the **watcher-reward
opt-in field** that has been added to V2 swap-argument structs:

- A `watcher_reward: bool` field appears on `RefundMakerPaymentTimelockArgs`,
  `RefundTakerPaymentArgs`, and `RefundFundingSecretArgs` in
  `coins/lp_coins_types.rs`.
- That field is currently **always constructed as `false`** in the three
  call sites that populate V2 swap arguments (`maker_swap_v2.rs`,
  `taker_swap_v2.rs`).
- The downstream coin implementations therefore never see a `true`-valued
  reward in this tree.

This chapter documents the watcher-message protocol and the watcher state
machine — both of which are fully functional — and the `watcher_reward`
boolean as a coin-trait *interface* whose runtime activation is not currently
wired in our tree. Any reader expecting a per-swap economic reward to flow
to watcher operators in this tree will find none: the watcher service is
voluntary and unpaid.

## Reproduction Detail

### 9.1 Baseline shape (no watcher concept)

Commit `c1d46c0…` does not contain a watcher module, a watcher gossipsub
topic, a `is_supported_by_watchers` coin method, or a `watcher_reward` field
anywhere. The baseline `lp_swap/` directory contains ten files and none of
them mentions a watcher. Refunds are exclusively the responsibility of the
original transaction sender.

### 9.2 New module and re-exports

A new module `mm2_main/src/lp_swap/swap_watcher.rs` is added under the
existing `lp_swap` module tree (via `#[path = "lp_swap/swap_watcher.rs"]`).
`lp_swap.rs` re-exports the four public items the rest of the binary needs:

```rust
pub use swap_watcher::{
    process_watcher_msg, watcher_topic, SwapWatcherMsg, TakerSwapWatcherData,
    WATCHER_PREFIX,
};
```

The `SwapsContext` (also in `lp_swap.rs`) grows a `taker_swap_watchers:
PaMutex<WatcherEntryMap>` field, where `WatcherEntryMap = HashMap<Vec<u8>,
u64>` maps a per-swap deduplication key (the taker-fee transaction hash) to
an entry expiry timestamp in seconds.

### 9.3 Wire envelope and gossipsub topic

```rust
pub const WATCHER_PREFIX: TopicPrefix = "swpwtchr";

pub fn watcher_topic(coin_ticker: &str) -> String {
    mm2_p2p::pub_sub_topic(WATCHER_PREFIX, coin_ticker)
}
```

Topics are per-taker-coin (e.g. `swpwtchr/BTC`); nodes subscribe to the
watcher topic for every coin they have enabled and can therefore choose which
swap families they are willing to watch.

The message type is a versioned enum so additional watcher message shapes can
be added without a topic split:

```rust
pub enum SwapWatcherMsg {
    TakerSwapWatcherMsg(TakerSwapWatcherData),
}
```

Wire payloads are wrapped in the project's signed-envelope format
(`mm2_p2p::decode_signed::<SwapWatcherMsg>`), which authenticates the sender
via libp2p key. A watcher only acts on a message whose signature verifies and
whose embedded coin tickers it actually has enabled locally.

### 9.4 `TakerSwapWatcherData`

The payload the taker publishes after it has sent its taker payment on-chain:

```rust
pub struct TakerSwapWatcherData {
    pub uuid: Uuid,
    pub secret_hash: Vec<u8>,
    pub maker_payment_spend_preimage:  Vec<u8>,  // taker's success-path tx
    pub taker_payment_refund_preimage: Vec<u8>,  // taker's safety-net tx
    pub swap_started_at:  u64,
    pub lock_duration:    u64,
    pub taker_coin:       String,
    pub taker_fee_hash:   Vec<u8>,
    pub taker_payment_hash: Vec<u8>,
    pub taker_coin_start_block: u64,
    pub taker_payment_confirmations: u64,
    pub taker_payment_requires_nota: Option<bool>,
    pub maker_coin:       String,
    pub maker_pub:        Vec<u8>,                // 33-byte compressed secp256k1
    pub maker_payment_hash: Vec<u8>,
    pub maker_coin_start_block: u64,
}
```

The two "preimage" fields are the *unsigned-or-presigned* transaction blobs
the watcher will use to act on the taker's behalf:

- `maker_payment_spend_preimage` is the transaction that, once augmented with
  the secret revealed by the taker-payment spend, will let the maker claim
  the maker payment.
- `taker_payment_refund_preimage` is the transaction that, after the timelock,
  refunds the taker payment back to the taker.

Because both preimages are precomputed by the taker before its
own private key is gone, the watcher never needs a taker private key.

### 9.5 Watcher state machine

The watcher is implemented as a state machine using the generic state-machine
runtime (chapter 14):

| State | Role |
| --- | --- |
| `ValidateTakerFee` | Locate the taker fee tx on-chain and run the coin's `validate_fee` over the watcher's `TakerSwapWatcherData`. Retries up to a fixed number of times before stopping. |
| `ValidateTakerPayment` | Wait for the taker payment tx to appear on-chain with the configured confirmation count, then validate it. |
| `WaitForTakerPaymentSpend` | Poll for either (a) a spend of the taker payment (normal completion path) or (b) the refund deadline being reached. |
| `SpendMakerPayment { secret }` | Triggered by (a). Extract the secret from the taker-payment spend, plug it into `maker_payment_spend_preimage`, broadcast it. |
| `RefundTakerPayment` | Triggered by (b). Broadcast `taker_payment_refund_preimage` once the refund deadline has elapsed. |
| `Stopped { result: WatcherResult }` | Terminal. Logs the outcome. |

`WatcherResult` is the four-variant enum

```rust
pub enum WatcherResult {
    MakerPaymentSpent,
    TakerPaymentRefunded,
    CompletedNormally,
    StoppedOnError(String),
}
```

The transition table is fixed (the `TransitionFrom` implementations
explicitly list the legal moves) so the machine cannot enter an unintended
state. Per-coin tuning parameters are read once into a `WatcherConf` struct
at machine construction:

```rust
pub struct WatcherConf {
    pub wait_taker_payment:   f64,  // seconds
    pub search_interval:      f64,  // seconds
    pub refund_start_factor:  f64,  // multiplier on lock_duration
}
```

These have defaults — see §9.7 — that are deliberately conservative so the
watcher always loses the race to the original parties under normal latency.

### 9.6 Lock-out and de-duplication

`SwapWatcherLock` (RAII guard) prevents two parallel watcher state machines
from spawning for the same taker fee hash on a single node:

- `try_lock(swap_ctx, fee_hash)` returns `None` if the map currently contains
  an unexpired entry for `fee_hash`; otherwise inserts an entry with expiry
  `now + TAKER_SWAP_WATCHER_ENTRY_TIMEOUT_SECS` and returns the guard.
- `Drop for SwapWatcherLock` removes the entry, so a panicking or
  early-returning watcher releases its slot promptly.

The expiry timeout (six hours in our tree) protects against forever-stuck
entries if a process is killed before the `Drop` runs and the file-system
lock is lost.

### 9.7 Constants and defaults

The current values in `swap_watcher.rs`:

| Constant | Value | Purpose |
| --- | --- | --- |
| `WATCHER_PREFIX` | `"swpwtchr"` | gossipsub topic prefix |
| `WATCHER_MSG_INTERVAL` | `10.0 s` | how often the taker re-broadcasts its watcher data |
| `TAKER_FEE_VALIDATION_ATTEMPTS` | `6` | retries when validating the fee on-chain |
| `TAKER_FEE_VALIDATION_RETRY_SECS` | `10.0 s` | delay between retries |
| `WAIT_TAKER_PAYMENT_DEFAULT_SECS` | `60.0 s` | default for `WatcherConf::wait_taker_payment` |
| `SEARCH_INTERVAL_DEFAULT_SECS` | `300.0 s` | default poll interval |
| `REFUND_START_FACTOR` | `1.5` | refund window opens at `started_at + 1.5 * lock_duration` |
| `TAKER_SWAP_WATCHER_ENTRY_TIMEOUT_SECS` | `21600` (6 h) | watcher-lock expiry |

These are first-party tuning choices; nothing in the file is sourced from
upstream literature.

### 9.8 Coin-trait surface

`SwapOps` (in `coins/lp_coins_traits.rs`) gains one read-only declaration:

```rust
fn is_supported_by_watchers(&self) -> bool { false }
```

A coin implementer flips this to `true` once both the on-chain spend and
refund transactions can be reconstructed deterministically by a third party
given the watcher payload. Today the UTXO-standard, BCH and QTUM coins
declare `true`; everything else inherits the default `false`. The watcher
short-circuits on any coin pair where either side reports `false`.

### 9.9 The unactivated `watcher_reward` boolean

The following V2 argument structs in `coins/lp_coins_types.rs` carry a
`watcher_reward: bool` field:

- `RefundMakerPaymentTimelockArgs`
- `RefundTakerPaymentArgs`
- `RefundFundingSecretArgs`

The intent of this field is to signal to the coin implementation that the
refund transaction should reserve some value to a watcher's address as
compensation for having performed the broadcast. In our tree the field is
**always assigned `false`** at the three sites that populate these
structures (`maker_swap_v2.rs:2104`, `taker_swap_v2.rs:2430`,
`taker_swap_v2.rs:2495`). Coin implementations may inspect the field; they
will only ever see `false`.

The field is therefore documented here as an *interface* — a coin
implementor can rely on it being a stable parameter slot — without committing
this tree to any particular reward economy. Adding a reward economy would
require:

- a per-network policy parameter (e.g. a new `NetConfig::watcher_reward_*`
  method, parallel to the burn-related methods in chapter 06);
- code paths at the three construction sites above that derive the boolean
  from the policy;
- coin-side handling for the `true` branch in each refund implementation.

None of those exist in our tree, and this chapter does not document a design
for them.

### 9.10 Entry-point wiring

- The libp2p incoming-message handler in `mm2_main/src/lp_network.rs` calls
  `lp_swap::process_watcher_msg(ctx.clone(), &message.data).await` on every
  message whose topic matches the watcher prefix.
- A node subscribes to a watcher topic for a coin when the coin is enabled,
  via `subscribe_to_topic(&ctx, watcher_topic(coin.ticker()))` in the legacy
  `enable` / `electrum` RPCs (`rpc/lp_commands/lp_commands_legacy.rs`).
- A taker broadcasts to its watcher topic from `taker_swap.rs` after sending
  its payment (`watcher_topic(self.taker_coin.ticker())` + a published
  `SwapWatcherMsg::TakerSwapWatcherMsg(data)`).

### 9.11 Reproduction recipe

For an implementer holding only the baseline tree and this chapter:

1. Add `mm2_main/src/lp_swap/swap_watcher.rs`. Register it from
   `lp_swap.rs` with `#[path = "lp_swap/swap_watcher.rs"] pub mod
   swap_watcher;` and re-export the five public items in §9.2.
2. In `lp_swap.rs`, add `pub type WatcherEntryMap = HashMap<Vec<u8>, u64>;`
   and grow `SwapsContext` with `pub taker_swap_watchers:
   PaMutex<WatcherEntryMap>`, initialised to `PaMutex::new(HashMap::new())`.
3. Define `WATCHER_PREFIX: TopicPrefix = "swpwtchr"` and `pub fn
   watcher_topic(coin_ticker: &str) -> String { mm2_p2p::pub_sub_topic(
   WATCHER_PREFIX, coin_ticker) }`.
4. Define `SwapWatcherMsg` (single variant `TakerSwapWatcherMsg`) and
   `TakerSwapWatcherData` exactly as §9.4 lists. Derive `Clone`, `Debug`,
   `Serialize`, `Deserialize`.
5. Add `WatcherConf` (§9.5) with `serde(default = "…")` defaults pulling
   from named module-level functions returning the constants in §9.7.
6. Implement `WatcherStateMachineCtx` (§9.5) with helpers `taker_locktime`
   and `refund_start_time`. Use the generic state-machine runtime (chapter
   14) to define the six states and the eight legal transitions.
7. Implement each state per §9.5: `ValidateTakerFee` calls the coin's
   `validate_fee` (chapter 08 `ValidateFeeArgs`); `ValidateTakerPayment`
   waits and calls `validate_taker_payment`; `WaitForTakerPaymentSpend`
   polls; the two success/timeout branches broadcast the relevant preimage;
   `Stopped` logs.
8. Define `WatcherResult` (four variants per §9.5).
9. Implement `SwapWatcherLock` as a `Drop`-guarded RAII type (§9.6).
10. Add `pub async fn process_watcher_msg(ctx: MmArc, msg: &[u8])` that
    decodes a signed `SwapWatcherMsg`, looks up both coins via `lp_coinfind`,
    checks `is_supported_by_watchers()` on each, acquires the lock, and
    spawns the state machine.
11. In `coins/lp_coins_traits.rs`, add `fn is_supported_by_watchers(&self) ->
    bool { false }` to `SwapOps`. Flip the default to `true` only in coin
    implementations whose transaction format permits third-party rebroadcast
    (UTXO-standard, BCH, QTUM in our tree; gate others as their authors
    confirm).
12. In `coins/lp_coins_types.rs`, add the `watcher_reward: bool` field to
    the three V2 argument structs listed in §9.9. Initialise it as `false`
    at every construction site.
13. Hook `process_watcher_msg` from `lp_network.rs` on incoming gossipsub
    messages whose topic begins with `WATCHER_PREFIX`.
14. In `enable` / `electrum` RPCs, subscribe to `watcher_topic(ticker)` when
    a coin is activated.
15. In `taker_swap.rs`, after sending the taker payment, populate a
    `TakerSwapWatcherData` and publish it via the topic returned by
    `watcher_topic(taker_coin.ticker())` every `WATCHER_MSG_INTERVAL`
    seconds until the swap concludes.
16. Add a unit test `test_watcher_topic_format` asserting
    `watcher_topic("BTC") == "swpwtchr/BTC"`.

## External References

- IETF *Atomic-cross-chain swap* informal references and Bitcoin Wiki entry,
  <https://en.bitcoin.it/wiki/Atomic_swap>. Establishes the maker/taker /
  secret-reveal HTLC pattern the watcher operates within.
- libp2p Gossipsub v1.1 specification,
  <https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md>.
  Used for the watcher topic.
- libp2p PeerId / cryptographic identities,
  <https://github.com/libp2p/specs/blob/master/peer-ids/peer-ids.md>. Backs
  the signed-envelope authentication of `SwapWatcherMsg`.
- `uuid` crate, <https://crates.io/crates/uuid>.
- `serde` and `serde_derive` crates, <https://crates.io/crates/serde>.
- Rust `async-trait` crate, <https://crates.io/crates/async-trait>.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline `lp_swap/` directory at
  commit `c1d46c0…`; the post-baseline files
  `mm2_main/src/lp_swap/swap_watcher.rs`, `mm2_main/src/lp_swap.rs`,
  `mm2_main/src/lp_network.rs`, `mm2_main/src/lp_swap/taker_swap.rs`,
  `mm2_main/src/rpc/lp_commands/lp_commands_legacy.rs`,
  `coins/lp_coins_traits.rs`, `coins/lp_coins_types.rs`, the per-coin
  declarations of `is_supported_by_watchers`; chapter 06 (`NetConfig`),
  chapter 08 (`ValidateFeeArgs`), chapter 14 (state-machine runtime).
- **Permitted-input classes used:** baseline source; first-party post-baseline
  identifiers introduced with in-chapter justification; public protocol
  documentation (atomic swap, Gossipsub, libp2p peer IDs); public Rust
  crates.
- **Not used:** any private repository, any internal-only document, any
  upstream post-baseline source tree (no kdf-analysis-2022 access).
- **Sibling-allowlist consultations:** none.
- **Author of this chapter:** clean-room reimplementation working set,
  reviewed under the two-reviewer protocol defined in
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
