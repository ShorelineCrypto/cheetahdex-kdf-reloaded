# Upstream Parity Gaps — TODO Tracker

**Status:** active backlog.

This document tracks confirmed functional gaps between the reloaded tree and
the upstream Komodo DeFi Framework wire surface, as established by the changelog
parity audit (Jan 2022 → present) cross-referenced against
[`rpc-method-census.md`](./rpc-method-census.md) (the upstream wire-method
parity reference) and the reloaded dispatcher routing.

Each gap below has been confirmed at **both** the CRD-specification level and the
reloaded-code level (the audit method and evidence are recorded inline). Items
are numbered to match the parity report delivered to the maintainer.

Branch hierarchy for remediation: each item gets its own feature branch cut from
`dev`; branches are left local (not pushed/merged) pending review.

---

## #2 — Event streaming: missing `stream::` streamers

**Confirmed real. CRD scope: deliberately narrowed; upstream has more.**

CRD reference: [`10-sse-streaming.md`](./10-sse-streaming.md). R6 binds the
`StreamerId` enum to exactly six variants (`Heartbeat`, `Balance`, `Network`,
`SwapStatus`, `OrderStatus`, `OrderbookUpdate`). R20/R23 bind exactly **five**
active `stream::<category>::enable` activation methods. The `Network` tag is
reserved by R6 with a fixed wire string (`NETWORK`) but **D2 explicitly defers
its activation handler** ("no activation handler is bound until the consumer
exists").

Code evidence:
- `mm2src/mm2_event_stream/src/streamer.rs` — `StreamerId` enum carries
  `Network` but has no `FeeEstimator` / `TxHistory` / `ShutdownSignal` variants.
- `mm2src/mm2_main/src/rpc/streaming_activations/` — modules present:
  `balance`, `heartbeat`, `orderbook`, `orders`, `swaps`. No `network` module.
- Dispatcher routes only the five `*::enable` arms.

Upstream `stream::` surface (census) vs reloaded:

| upstream method | reloaded | gap |
| --- | --- | --- |
| `stream::balance::enable` | yes | — |
| `stream::heartbeat::enable` | yes | — |
| `stream::order_status::enable` | yes | — |
| `stream::orderbook::enable` | yes | — |
| `stream::swap_status::enable` | yes | — |
| `stream::network::enable` | **no** | TODO — tag reserved (D2) |
| `stream::fee_estimator::enable` | **no** | see #3 |
| `stream::tx_history::enable` | **no** | TODO — needs new variant |
| `stream::shutdown_signal::enable` | **no** | TODO — needs new variant |
| `stream::disable` | **no** | TODO — generic unsubscribe |

- [ ] **`stream::network::enable`** — implement the reserved `Network` streamer
  (D2 completion). Lowest-risk: the tag + wire string are already bound. Requires
  a clean-room spec section for the activation request and emitted payload.
- [ ] `stream::tx_history::enable` — requires a new `StreamerId` variant + spec.
- [ ] `stream::shutdown_signal::enable` — requires a new `StreamerId` variant + spec.
- [ ] `stream::disable` — generic per-client unsubscribe RPC. Reloaded currently
  relies on implicit deactivation (connection drop → `remove_client`); upstream
  also exposes an explicit unsubscribe method.

## #3 — Fee estimator stream (`stream::fee_estimator::enable`)

**Confirmed real.** The EIP-1559 fee estimator is exposed upstream as a streamer
(`stream::fee_estimator::enable`), not a legacy method. The on-demand
single-shot estimator `get_eth_estimated_fee_per_gas` **is** present in reloaded
(arms=1), and the gas-policy pair `get_swap_gas_fee_policy` /
`set_swap_gas_fee_policy` are present (arms=2 each). Only the **continuous
streaming** variant is missing.

CRD reference: [`08-fee-routing-engine.md`](./08-fee-routing-engine.md) covers
the swap fee-routing engine but does not bind a fee-estimator streamer;
[`10-sse-streaming.md`](./10-sse-streaming.md) does not list one among its five.

- [ ] **`stream::fee_estimator::enable`** — add a `FeeEstimator` `StreamerId`
  variant + activation module that periodically broadcasts EIP-1559 fee
  estimates for a given EVM coin. Requires a clean-room spec section.

## #1 — NFT activation (`enable_nft`)

**Confirmed real. Design divergence.** Upstream activates the NFT subsystem via
`enable_nft`; reloaded has **no** such method anywhere in the workspace (0 hits).

CRD reference: [`19-nft-module-layout.md`](./19-nft-module-layout.md) binds the
NFT subsystem to **seven** JSON-RPC methods (inventory, metadata, transfer
history, withdrawal, wipe). Reloaded routes exactly those seven: `clear_nft_db`,
`get_nft_list`, `get_nft_metadata`, `get_nft_transfers`, `refresh_nft_metadata`,
`update_nft`, `withdraw_nft`. In reloaded's design, `update_nft` performs the
initialise/refresh role that upstream's `enable_nft` performs as a distinct
activation entry point.

Interop impact: the Komodo DeFi SDK (and the Gleec GUI built on it) calls
`enable_nft` directly; its absence is the likely cause of the NFT "Bad state: No
element" runtime error observed in the GUI.

- [ ] **`enable_nft`** — add the activation entry point so SDK NFT enablement
  succeeds. Requires a clean-room spec section reconciling upstream `enable_nft`
  semantics with reloaded's existing `update_nft` initialisation path.

## #4 — Experimental liquidity routing (DOCUMENTED ONLY — NOT to be implemented)

**Confirmed real. Out of scope by maintainer decision.** Upstream exposes an
experimental multi-quote liquidity-routing namespace; reloaded has 0 routes for
it.

Upstream methods (census, all `experimental::liquidity_routing::`):
- `experimental::liquidity_routing::execute_routed_trade`
- `experimental::liquidity_routing::find_best_quote`
- `experimental::liquidity_routing::get_quotes_for_tokens`

No CRD chapter binds this namespace. Per maintainer instruction, this experimental
feature is **deliberately not implemented**; it is recorded here for completeness
only.

- [x] Documented as a known, intentional omission. No implementation planned.

---

## Audit method (for reproducibility)

1. Extracted the 223 unique upstream wire-method strings from
   `rpc-method-census.md`.
2. Extracted reloaded's routed method strings from
   `mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs` and
   `dispatcher_legacy.rs`.
3. Normalised namespace prefixes on both sides and diffed.
4. Verified every candidate gap by direct source grep across `mm2src/`
   (eliminating routing-style false positives such as generic `::cancel` arms
   and task-lifecycle leaves).
