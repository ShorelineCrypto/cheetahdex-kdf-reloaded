# Plan: V2 swap engine dedup, error-path hardening, and V1 driver consolidation

> **Status:** analysis and implementation plan only; not started.
>
> This plan is adapted from an external code review of `lp_swap/` (2026-08-25),
> supplemented with independent verification against this repository. Findings
> below are marked **CONFIRMED** where re-checked directly against source,
> **CONFIRMED (severity revised)** where the underlying fact is real but the
> reviewer's framing overstated it, or **TAKEN ON TRUST** where the reviewer's
> demonstrated precision on adjacent, checked claims made independent
> re-verification a poor use of time for that specific item. Two items in
> Phase 2 turned out to be new, previously-undocumented gaps rather than pure
> style cleanup; those are now also recorded in CRD chapters directly (see
> "New CRD entries" below) so they survive independently of this plan's
> lifecycle.

## Goal

Reduce `lp_swap/`'s maintenance burden (parallel V1+V2 implementations,
~21K lines across five files >2K lines each) through mechanical,
behavior-preserving deduplication first, then a small set of deliberate,
individually-justified error-path hardening changes, then (lowest priority,
highest risk) V1 driver consolidation. Every phase is independently
shippable and reversible; later phases are not blocked on earlier ones
landing, only on the review discipline this project already applies (see
`AGENTS.md` §2, the Coder/dispatcher review loop) being followed for each.

## Verification baseline (re-run, not assumed, 2026-08-25)

- `cargo check --workspace` — clean.
- `cargo test -p mm2_main --lib` — passes (450 tests at review time).
- `cargo clippy -p lightning --no-deps` — **fails**: 13 deny-level errors in
  the vendored, patched `rust-lightning` crate, confirmed independently.
  `cargo clippy --workspace` therefore cannot succeed today; any crate that
  depends on `lightning` (most of the workspace, transitively) never gets
  linted by a bare `cargo clippy --workspace` invocation. This is real and
  already the single highest-leverage process fix in this plan (Phase 0).

## Findings, most-to-least confirmed-severe

### CONFIRMED — D8-class: `MakerPaymentSpent`'s timeout-abort persists an
unusable, empty payment record (new; not in the original review's own
framing, found while verifying it)

`taker_swap_v2.rs`'s `MakerPaymentSpent::on_changed`, on the maker-payment-
spend-confirmation-timeout path, transitions to `TakerPaymentRefundRequired`
carrying `BytesJson::default()` for the taker's own payment transaction
(comment: "We don't have taker_payment bytes here; use empty as fallback").
That refund state hands the empty bytes straight to
`refund_combined_taker_payment` as `RefundTakerPaymentArgs::payment_tx`,
which cannot construct a valid refund from zero bytes. Root cause: the field
is dropped two transitions earlier than where it's needed —
`TakerPaymentSent` carries `taker_payment: BytesJson`, but `TakerPaymentSpent`
(the very next state) already drops it, keeping only `taker_payment_spend`
(what consumed it) and `maker_payment`. `StoredTakerNegotiationData` was
never scoped to carry it either. **This is a live, reachable bug on native
builds**, not a style nit — any delayed, dropped, or reorganized
confirmation of the maker's own payment spend triggers it. Now recorded as
[Chapter 52](../reloaded-rewrite/52-swap-v2-state-machine.md) D8, with the
two candidate fixes (thread the field through as an additive
`#[serde(default)]` field vs. make this one refund path locate-by-search
the way [Chapter 20](../reloaded-rewrite/20-siacoin-integration.md) D3
already does for Sia) recorded there. **Recommend prioritizing an actual
fix over the rest of this plan** — everything else here is maintainability;
this is correctness.

### CONFIRMED — F8: WASM V2 swap storage is a total no-op

`swap_v2_common.rs`'s `wasm` module's `StateMachineStorage` impls for both
maker and taker are stubs: writes are `Ok(())` no-ops, `has_record_for` is
always `false`, `get_unfinished` is always empty. A V2 swap run from a
WebAssembly build persists nothing; a reload during a live swap is
unrecoverable. This is also a confirmed violation of an existing binding
requirement — [Chapter 26](../reloaded-rewrite/26-cross-platform-and-wasm.md)
R8 names the V2 swap state stores explicitly among the consumers required
to have a real IndexedDB backend, not a stub. Now recorded as ch.52 D7 and
cross-referenced at ch.26 D6, since it's simultaneously "this subsystem has
a gap" and "this binding rule has a known violation."

### CONFIRMED: F9 — vendored `lightning` breaks `cargo clippy --workspace`

Re-run directly: `cargo clippy -p lightning --no-deps` exits with 13
deny-level errors. First-party clippy coverage silently stops at the
`lightning` dependency boundary for any bare `--workspace` invocation.
Highest-leverage, lowest-risk fix in this whole plan (Phase 0).

### CONFIRMED: F1 — 47 copy-paste refund-transition blocks

Recounted directly: `TakerFundingRefundRequired::new` appears 24 times in
`taker_swap_v2.rs`, `MakerPaymentRefundRequired::new` 23 times in
`maker_swap_v2.rs` — the reviewer's "47" is exact for the two constructors
they named. Not counted in their figure, found while verifying:
`TakerPaymentRefundRequired::new` appears a further 12 times in the same
file — the real copy-paste surface for refund-transition blocks is larger
than the headline number, not smaller.

### CONFIRMED (severity revised): F3's other three sub-findings

- The `net_config_or_panic` and recovery-path `.expect()` sites are real
  or-panic calls, confirmed present at the cited files. Whether each is
  truly reachable (vs. a "this config value is registered at startup and
  cannot be absent by construction" case, the way the zero-secret fallback
  below turned out to be) was not individually re-verified for all seven
  `net_config_or_panic` sites — worth confirming per-site before deciding
  each one warrants a behavior change, not assuming uniformly.
- **The zero-secret fallback is not reachable.** `sm.taker_secret` is typed
  `primitives::hash::H256` — a fixed 32-byte array type (`define_hash!(H256,
  32)`) — so `.as_slice().try_into()` into a `&[u8; 32]` can never fail;
  `unwrap_or(&[0u8; 32])` is dead code, not a live silent-corruption path.
  It should still be cleaned up (a fallback that implies fallibility where
  none exists is misleading to a future reader, and would become a real bug
  if `H256`'s definition ever changed to a variable-length type), but it
  does not belong in the same severity class as the `MakerPaymentSpent`
  empty-bytes bug above, which the original review's F3 grouped it with.

### CONFIRMED: F4 — `append_swap_v2_event`'s read-modify-write has no
transaction

`swap_v2_common.rs`'s `append_swap_v2_event` does `SELECT` → parse → push →
`UPDATE` with no `BEGIN`/`COMMIT` wrapper. The reviewer's own caveat — "safe
today only because a per-uuid file lock serializes writers" — is also
confirmed: both `MakerSwapStateMachine`/`TakerSwapStateMachine` declare
`type ReentrancyLock = SwapLock`, acquired per swap uuid before the machine
runs, which does serialize `store_event` calls for a given uuid in
practice. Real gap, correctly self-caveated by the reviewer as low-urgency;
cheap to fix regardless (wrap in `conn.transaction()`).

### TAKEN ON TRUST (not independently re-verified line-by-line; reviewer's
other quantitative claims all checked out): F2, F5, F6, F7

Duplicated derivation, twin SQL/kickstart functions, V1 driver duplication,
event/mirror boilerplate. No reason found to doubt these; not re-counted
given the effort-to-value ratio of re-deriving numbers the reviewer already
demonstrated care in producing elsewhere.

## New CRD entries (already written; not gated on this plan being executed)

- [Chapter 52](../reloaded-rewrite/52-swap-v2-state-machine.md) §52.13 D7
  (WASM storage no-op) and D8 (`MakerPaymentSpent` empty-bytes bug).
- [Chapter 26](../reloaded-rewrite/26-cross-platform-and-wasm.md) §26.14 D6
  (R8 non-conformance cross-reference).

These are documentation of the gap, not a fix. D8 in particular should not
be treated as merely deferred-and-fine; see "Recommend prioritizing" above.

## Phased plan (adapted from the original review; sequencing unchanged,
annotated where this repository's own verification changed the picture)

### Phase 0 — Guardrails (½ day, no behavior change)

- P0.1 Document that bare `cargo clippy --workspace` fails on vendored
  `lightning` (confirmed); note the `SDKROOT` local-build requirement.
- P0.2 Adopt `RUSTFLAGS="--cap-lints=warn" cargo clippy --workspace
  --all-targets` for CI/dev so first-party code actually gets linted despite
  the vendored failure.
- P0.3 Capture the current first-party warning baseline so regressions are
  visible.

### Phase 1 — Mechanical dedup, no behavior change

- P1.1 Extract a shared refund-transition helper replacing the 47 (really:
  59, once `TakerPaymentRefundRequired` is included — recount before
  scoping) copy-paste blocks (F1). Est. −450…−550 lines.
- P1.2 Delete the duplicate `taker_taker_coin_pub_bytes` derivation (F2).
- P1.3 Merge `insert_swap_v2_{maker,taker}` and the two kickstart handlers
  (F5).
- P1.4 Collapse `*SwapEventDeser` mirrors onto the hand-written
  `Deserialize` hooks (F7), preserving legacy-compat branches byte-for-byte.
- Update `dex_fee.rs`'s `include_str!` source-text meta-tests alongside
  P1.1/P1.2 — they assert on `taker_swap_v2.rs`/`maker_swap_v2.rs`'s literal
  source text and will need updating for any refactor touching those files.

### Phase 2 — Error-path hardening (small, deliberate behavior changes)

- **P2.0 (new, higher priority than the rest of this phase): fix the
  `MakerPaymentSpent` empty-bytes bug (ch.52 D8) for real** — thread
  `taker_payment` through the two intervening states, or switch this one
  refund path to locate-by-search. Needs its own scoped Coder pass and CRD
  sign-off on which of the two directions to take, same process as every
  other deferred-work item this project closes.
- P2.1 Replace the `net_config_or_panic` V2 sites with propagated
  `AbortReason::InternalError` — verify per-site reachability first (see
  "severity revised" note above) rather than assuming all seven are
  equally live.
- P2.2 Remove the (confirmed-dead) zero-secret `unwrap_or(&[0u8; 32])` for
  clarity/future-proofing — cosmetic, not safety-critical, given it's
  unreachable today.
- P2.3 Recovery-path `.expect()`s → logged abort with reason.
- P2.4 Wrap `append_swap_v2_event`'s read-modify-write in a transaction
  (F4) — correct-by-construction even though the reentrancy lock already
  makes it safe in practice today.

### Phase 3 — V1 consolidation (medium risk; do after Phase 1 experience)

- P3.1 Factor `run_taker_swap`/`run_maker_swap`'s shared driver skeleton
  into one parameterized helper (F6).
- P3.2 Snapshot-style lock extraction in V1 swaps (pattern already exists
  at `taker_swap.rs:1515-1558`) to make the not-`Send`-across-`.await`
  discipline structural rather than tribal.
- P3.3 (Optional) Move V1 `Result<_, String>` internals to
  `MmError<SwapError>` at the seams P3.1 touches only — do not churn
  untouched signatures.

### Explicitly out of scope (recorded, not planned)

- Full WASM IndexedDB V2 storage implementation (ch.52 D7) — its own
  project; the CRD entry, not this plan, is where its scope belongs once
  someone picks it up.
- Any serde layout change to persisted events/negotiation structs beyond
  the additive, `#[serde(default)]`-guarded field P2.0 may need.
- Completing the V1→V2 product migration — product decision, would
  obsolete much of Phase 3.
- Reworking `MmError`'s nightly-auto-trait bootstrap — fragile but
  documented, pinned, revisit only on the next toolchain bump.

## Suggested execution order & sizing

| Step | Items | Risk | Est. |
|---|---|---|---|
| 1 | P0.1–P0.3 | none | ½ d |
| 2 | **P2.0 (moved up)** | correctness fix, needs design decision | 1–2 d |
| 3 | P1.2, P1.3 | trivial | ½ d |
| 4 | P1.1 + dex_fee meta-test update | low | 1–2 d |
| 5 | P1.4 | low | ½–1 d |
| 6 | P2.1–P2.4 (minus P2.0, done above) | medium (behavioral) | 1 d |
| 7 | P3.1–P3.2 | medium | 2–3 d |
| 8 | P3.3 | optional | 1 d+ |

Total mechanical-dedup yield unchanged from the original estimate: roughly
−700…−900 lines in `lp_swap/` with net-zero behavior change from Phase 1,
plus one real correctness fix (P2.0) promoted ahead of the pure hardening
items.
