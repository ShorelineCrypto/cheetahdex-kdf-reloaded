# Chapter 13 — Atomic-Swap Version Negotiation Layer

## Executive Summary

At the baseline tree there is exactly one atomic-swap protocol. Every
maker and every taker speaks the same wire format, runs the same
five-stage HTLC dance (taker fee → maker payment → taker payment →
maker spend → taker spend), and the on-the-wire order, reservation
and connection messages carry no protocol-version tag at all. Any
future protocol change would therefore be a hard fork of the gossip
overlay — old and new nodes would simply fail to deserialise each
other's messages.

The post-baseline tree introduces an **atomic-swap version
negotiation layer**. A new module `mm2_main::lp_swap::swap_versioning`
defines a single typed tag, `SwapVersion { version: u8 }`, exposes
three named constants (`LEGACY_SWAP_VERSION = 1`,
`TPU_SWAP_VERSION = 2`, `NFT_SWAP_V2_VERSION = 3`), and supplies
four predicate/utility methods (`is_legacy`, `is_v2_or_higher`,
`is_nft_v2`, `negotiate`). The tag is carried on two of the gossip
order-protocol messages (the taker's `TakerRequest` and the
maker's `MakerReserved`) as an optional, default-on-missing field,
mirrored on the persisted `MakerOrder` / `TakerOrder` types, and
the negotiation function picks the element-wise minimum of the two
advertised versions so that a peer that only knows version `n` can
still complete a swap with a peer that knows `n + 1`.

The negotiation surface is deliberately a single `u8` with a typed
wrapper; richer feature-vector negotiation was rejected because (a)
the only consumers in this tree are the V2 state-machine swap path
(`TPU`) and its NFT extension, (b) a `u8` keeps the wire envelope
size identical for legacy peers thanks to
`skip_serializing_if = "SwapVersion::is_legacy"`, and (c) the
predicate methods give the dispatcher a single semantic gate
(`is_v2_or_higher`) without exposing the numeric value to the rest
of the daemon.

The chapter documents the new module, the wire integration on
order-protocol messages, the negotiation semantics, the tests that
lock the behaviour, and the backward-compatibility guarantees that
make a versioned node and a baseline node interoperate without code
changes on the baseline side.

### Why this changed

The version-tag mechanism was introduced by the project's own commit
`bbfb96ace` (*feat(P5.3+P5.5): SwapVersion on orders + clippy
fixes*). The commit message states the design verbatim:

> *Add SwapVersion struct (u8 wrapper: legacy=1, TPU=2) in
> swap_versioning.rs. Add swap_version field to MakerOrder,
> TakerRequest, MakerReserved. Wire through MakerOrderBuilder,
> TakerOrderBuilder, P2P conversions. Backward-compatible: legacy
> version omitted via skip_serializing_if. Default to V1 (legacy);
> V2 dispatch requires P5.2 state machines.*

The NFT extension and the `negotiate()` helper were added later by
commit `5b965f3e9` (*P10.3.7.d NFT swap V2 state-machine wiring +
SwapVersion negotiation*):

> *Add NFT_SWAP_V2_VERSION = 3 plus SwapVersion::{is_v2_or_higher,
> is_nft_v2, negotiate} helpers (negotiation = element-wise min).*

A subsequent clean-room polish landed as `8f1d2e68b` (*lp4: T7 doc
rewrite + test rename in swap_versioning.rs*), which rewrote the
module documentation and renamed the unit tests to the project's
`should_<behaviour>_when_<condition>` standard without touching the
public API.

In clean-room voice: the post-baseline project chose to add a
narrow, typed version tag to the order-protocol messages so that
the V2 state-machine swap path (and, later, its NFT extension)
could be dispatched at order-match time without breaking the
existing wire format. The tag is wire-omitted when legacy, so old
nodes deserialise new messages unchanged; the dispatch decision
collapses to a single predicate call (`is_v2_or_higher`); and
peers running different upgrade levels still complete a swap by
falling back to the lower advertised version.

## Reproduction Detail

### 13.1 Baseline shape (no version tag)

At commit `c1d46c0…` the swap protocol is implicit:

- No `swap_versioning.rs` module exists under `lp_swap/`.
- The order-protocol message structs (`TakerRequest`, `MakerReserved`,
  `MakerConnect`, `MakerConnected`, `TakerConnect` in
  `lp_ordermatch/new_protocol.rs`) carry only the fields needed to
  match orders; no `swap_version` field is present.
- The persisted `MakerOrder` struct in
  `lp_ordermatch/ordermatch_types.rs` similarly has no
  `swap_version` field.
- The dispatcher in `lp_ordermatch.rs` calls a single pair of
  functions, `run_maker_swap` and `run_taker_swap`, after a match;
  there is no branching on a protocol-version value.

A future protocol upgrade in this shape requires either (a) breaking
the wire format and partitioning the network into "old" and "new"
nodes, or (b) carrying a version tag out-of-band (for example in
`base_protocol_info`/`rel_protocol_info` byte blobs), which would
conflate a network-wide capability flag with per-coin protocol
metadata. Both options are unattractive.

### 13.2 The `swap_versioning` module

The post-baseline tree adds the file
`mm2src/mm2_main/src/lp_swap/swap_versioning.rs` (161 lines,
including doc comments and tests) and registers it in
`mm2src/mm2_main/src/lp_swap.rs` as:

```rust
#[path = "lp_swap/swap_versioning.rs"] pub mod swap_versioning;
```

The module exports:

| Item | Kind | Wire-stable | Notes |
|---|---|---|---|
| `LEGACY_SWAP_VERSION` | `pub const u8 = 1` | yes | V1 protocol identifier |
| `TPU_SWAP_VERSION` | `pub const u8 = 2` | yes | Trading-Protocol-Upgrade (V2) identifier |
| `NFT_SWAP_V2_VERSION` | `pub const u8 = 3` | yes | NFT-extended V2 identifier |
| `SwapVersion` | `pub struct` | yes | `{ version: u8 }` |
| `SwapVersion::is_legacy` | `pub fn(&self) -> bool` | n/a | predicate |
| `SwapVersion::is_v2_or_higher` | `pub fn(&self) -> bool` | n/a | dispatch gate |
| `SwapVersion::is_nft_v2` | `pub fn(&self) -> bool` | n/a | NFT-only gate |
| `SwapVersion::negotiate` | `pub fn(SwapVersion, SwapVersion) -> SwapVersion` | n/a | pair-min |
| `impl Default for SwapVersion` | `fn default() -> Self` returning `LEGACY_SWAP_VERSION` | yes | default-on-missing |

The struct itself is the smallest possible wrapper:

```rust
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SwapVersion {
    pub version: u8,
}
```

Wire shape: `{"version": N}`. The wrapper exists rather than a
bare `u8` so that future fields (e.g. capability flags) could be
added inside the struct without breaking peers that already know
the JSON object shape.

### 13.3 Negotiation semantics

The pair-negotiation function is element-wise min:

```rust
pub fn negotiate(maker: SwapVersion, taker: SwapVersion) -> SwapVersion {
    SwapVersion { version: maker.version.min(taker.version) }
}
```

Consequences in this tree:

- A pair of legacy peers (`1, 1`) settles on legacy: no change to
  the historical wire format.
- A `TPU` peer matched with a legacy peer (`2, 1`) settles on
  legacy: the new peer falls back to the old protocol rather than
  failing the match. This preserves the gossip overlay across the
  upgrade.
- A `TPU` peer matched with another `TPU` peer (`2, 2`) settles on
  `TPU`: both sides enter the V2 state-machine swap path.
- An `NFT_SWAP_V2` peer matched with a `TPU` peer (`3, 2`) settles
  on `TPU`: the maker that knows the NFT extension still completes
  a fungible swap with a non-NFT peer.

The `is_v2_or_higher` predicate is the dispatch gate consumed by
the ordermatch loop after negotiation: when it returns `true`, the
state-machine swap path (chapters 14–17) is invoked; otherwise the
V1 path runs unchanged.

### 13.4 Wire integration on order-protocol messages

The version tag is carried on two message types in
`lp_ordermatch/new_protocol.rs`:

| Message | Field | Serde attribute |
|---|---|---|
| `TakerRequest` | `pub swap_version: SwapVersion` | `#[serde(default, skip_serializing_if = "SwapVersion::is_legacy")]` |
| `MakerReserved` | `pub swap_version: SwapVersion` | `#[serde(default, skip_serializing_if = "SwapVersion::is_legacy")]` |

The handshake-completion messages (`TakerConnect`, `MakerConnected`)
do **not** carry the field — by the time those are exchanged, the
dispatched version is already fixed by the prior `TakerRequest` /
`MakerReserved` exchange.

The `skip_serializing_if = "SwapVersion::is_legacy"` attribute is
the critical backward-compatibility hinge:

- A new node that runs only legacy swaps emits messages with the
  `swap_version` field **omitted**, byte-for-byte identical to a
  baseline node's messages.
- A new node deserialising a baseline node's message has no
  `swap_version` key; `#[serde(default)]` causes the field to
  resolve to `SwapVersion::default()` → `LEGACY_SWAP_VERSION`, and
  negotiation correctly falls back to V1.

The persisted-order types in `ordermatch_types.rs`
(`MakerOrder`, `TakerOrder`, the `TakerRequest`-equivalent
in-process type, and the maker-match envelope) carry the field
with the same serde attributes, so saved orders authored before
the upgrade deserialise to legacy without operator intervention.

### 13.5 Builder and dispatch plumbing

The integration in `lp_ordermatch/ordermatch_types.rs` provides:

- A `swap_version: SwapVersion` field on `MakerOrder` (line 1086 in
  the post-baseline tree) and on the in-process `TakerOrder` /
  `TakerRequest` family (lines 60, 154, 582, 597) with
  `#[serde(default, skip_serializing_if = "SwapVersion::is_legacy")]`
  where the type is serialised.
- A `with_swap_version(self, SwapVersion) -> Self` builder method
  on `TakerOrderBuilder` (line 789) so the RPC entry can plumb the
  caller's chosen version into the broadcast `TakerRequest`.
- Default-construction sites (e.g. lines 234, 745, 875) use
  `SwapVersion::default()` to keep legacy behaviour as the zero
  state.

Match resolution in the same file (lines 82, 128, 1007, 1031,
1114, 1131) propagates the negotiated version through maker-match
and connection envelopes, and the persistence layer
(`my_orders_storage.rs`, lines 754, 774) round-trips the field
with the order JSON.

### 13.6 Tests

The module ships with seven unit tests covering:

- `should_default_to_legacy_when_constructed_via_default`
- `should_report_not_legacy_when_version_is_v2`
- `should_roundtrip_through_serde_json`
- `should_default_to_legacy_when_field_missing_from_payload` — the
  critical backward-compatibility test; deserialises `"{}"` into a
  wrapper struct that contains an optional `swap_version: SwapVersion`
  with `#[serde(default)]` and asserts the resulting value is
  `LEGACY_SWAP_VERSION`.
- `should_recognise_nft_v2_tag`
- `should_classify_tpu_and_nft_as_v2_or_higher`
- `should_negotiate_minimum_when_versions_differ` — exercises the
  `(1,1)`, `(3,2)`, `(3,1)`, and `(3,3)` corners.

These tests are the contract: any reimplementation must keep them
passing without modification.

### 13.7 Integration boundaries (cross-references)

The negotiation result is consumed downstream by:

- The state-machine runtime (chapter 14), which is dispatched when
  `is_v2_or_higher()` returns `true` on the negotiated version.
- The V2 UTXO swap path (chapter 15) and V2 EVM swap path (chapter
  17), which are the two coin-family implementations of the state
  machine in this tree.
- The pre-burn output engine (chapter 16), which is only invoked
  on the V2 path; legacy swaps use the chapter-08 fee-routing
  engine unchanged.
- The V2 swap RPCs (`swap_v2_rpcs.rs`), which expose the
  negotiated `swap_version` as a `u8` column in DB rows (line 158)
  and as part of `MyRecentSwap` reprs (lines 196, 267, 297).
- The V2 P2P-message persistence in `swap_v2_common.rs` (lines
  390, 400, 428), which writes the version into the
  `other_p2p_pub`-indexed row alongside the swap UUID.

The NFT extension layer (`nft_maker_swap_v2.rs`) defines
`NftSwapV2NegotiationOutcome { Use | VersionMismatch { maker, taker }
| NoNftContract }` and a pure-function dispatcher,
`should_use_nft_swap_v2(maker, taker, has_contract)`, that wraps
the `SwapVersion::is_nft_v2()` predicate and the maker coin's
NFT-contract configuration check.

### 13.8 Invariants the design relies on

1. **Single negotiation point.** The version is fixed at the
   `TakerRequest` / `MakerReserved` exchange and never re-negotiated
   later in the swap. Any change to `swap_version` after the maker-
   match envelope is built must be treated as an attempted protocol
   downgrade attack and rejected; in this tree the field is
   `pub` but no code path mutates it after order construction.
2. **Wire-omitted legacy.** The `skip_serializing_if` attribute
   must remain on every `swap_version` field on every wire-typed
   struct, on pain of breaking peers that have not upgraded.
3. **Element-wise-min negotiation.** Any introduction of a version
   that is *not* a strict superset of its predecessor (i.e. cannot
   be cleanly downgraded) would invalidate the negotiation
   semantics and require a richer capability-vector approach.
4. **`Default = legacy`.** Every new code path that constructs a
   `SwapVersion` without a specific version in mind must rely on
   `Default` resolving to `LEGACY_SWAP_VERSION` so an
   unspecified-version code path can never accidentally upgrade
   the wire format.

## External References

- *libp2p gossipsub specification* — the transport over which
  versioned order-protocol messages are exchanged (see chapter
  06 for the netid registry that scopes the overlay).
- *serde-json crate documentation* — the
  `skip_serializing_if`/`default` attribute semantics on which
  backward compatibility depends.
- *Rust language reference* — `u8::min` is the underlying
  operation in `SwapVersion::negotiate`.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline workspace at
  commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`; the public
  serde-json documentation; the project's own commit messages
  `bbfb96ace`, `5b965f3e9`, and `8f1d2e68b`.
- **Sibling references:** chapter 06 (network-id registry), chapter
  10 (SSE swap-status streamer that reports `swap_version`),
  chapter 12 (maker-order state store that round-trips the field),
  chapters 14–17 (downstream consumers).
- **Forbidden corpus:** not consulted.
- **Author of this chapter:** clean-room reimplementation working
  set, see
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
- **Reviewers:** two-pass review at
  `local/clean-room-doc/reviews/13-swap-version-negotiation-r{1,2}.md`.
