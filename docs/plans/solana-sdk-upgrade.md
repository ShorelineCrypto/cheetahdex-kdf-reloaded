# Plan: Solana SDK ed25519-dalek 1.x → 2.x

> **Status:** not started — scoping only, 2026-08-06.

## Goal

Clear `RUSTSEC-2022-0093` (ed25519-dalek 1.x double-public-key signing
oracle) and `RUSTSEC-2024-0344` (curve25519-dalek timing variability) from
the Solana integration without changing Solana address derivation, signing,
or wire behavior.

## Confirmed root cause (2026-08-06)

This is **not** a lazy caret-range pin reloaded hasn't bumped — it's an
**exact upstream pin**. Both `solana-keypair 2.2.3` and `solana-signature
2.3.0` (the current Solana SDK 2.x line, already the newest major, resolved
via the workspace's `solana-* = { version = "2", ... }` pins in
`mm2src/coins/Cargo.toml`) declare:

```toml
[dependencies.ed25519-dalek]
version = "=1.0.1"
```

(verified directly against the registry-cached manifests). `solana-signature`
pulls the same exact pin behind its `verify` feature, which `coins/Cargo.toml`
explicitly enables. So there is no version selector on reloaded's side that
gets a newer `ed25519-dalek` here — every current `anza-xyz/solana-sdk` 2.x
release is hard-pinned. This is what deny.toml's section B ("Solana SDK —
upstream-blocked") already correctly labels it, but the pin is now confirmed
exact rather than assumed loose.

The same `curve25519-dalek 3.2.0` (pulled transitively by `ed25519-dalek
1.0.1`) previously also arrived via the libp2p 0.45 fork's Noise stack; that
path is gone now that the libp2p 0.52 fork migration is merged (confirmed via
`cargo tree -i curve25519-dalek@3.2.0` — the only remaining path is through
`solana-keypair`/`solana-signature`/`ed25519-dalek 1.0.1`). So clearing this
advisory no longer needs to wait on libp2p fork modernization (section A) —
it's purely a Solana-side fix now. Update `v0.2.0-dependency-hygiene.md`'s
section B note about the libp2p coupling once this lands; it's stale as of
the libp2p merge.

## Options

**A. Upstream tracking (lowest effort, indefinite timeline).** Watch
`anza-xyz/solana-sdk` for a release that relaxes the `=1.0.1` pin to
`ed25519-dalek 2.x`. No reloaded-side code work, but no control over timing;
Solana's SDK has historically been slow to move this pin (it was already
old when this repo's `solana-*` deps were first added at "version 2").

**B. Fork `solana-keypair`/`solana-signature` (moderate effort, full
control).** Vendor-patch (KDF-PATCH.md style, like the zcash `time-core`
precedent) or fork these two small crates, bump their `ed25519-dalek` pin to
`2.x`, and port their (small — these are thin wrapper crates, not the full
SDK) internal usage to the dalek 2 API (`Keypair` → `SigningKey`, merged
`PublicKey`/`SecretKey`, `Signature` repr change). Lower blast radius than
it sounds since these are leaf crates with a narrow surface, but it's still
new maintenance burden reloaded would own indefinitely (re-diff on every
future `solana-keypair` bump).

**C. Replace with a reloaded-owned thin wrapper (highest effort, cleanest
result).** `mm2src/coins/solana/` (~3000 lines total; check exact call sites
during inventory) likely only needs Ed25519 keypair generation, signing, and
`solana-pubkey`-compatible address formatting from `solana-keypair`/
`solana-signature` — not their full API surface. A small reloaded-owned
`Signer` implementation directly on `ed25519-dalek 2` + `solana-pubkey`
(which is *not* pinned to old dalek — only `solana-keypair` and
`solana-signature`'s `verify` feature are) would drop the old dalek line
entirely without waiting on upstream or forking a whole crate. This is the
same class of decision as the `sia-rust` dalek 1→2 migration already done in
this repo (see `v0.2.0-dependency-hygiene.md`'s "Done" list) — that precedent
is worth reviewing first since it's the closest prior art in this exact
codebase.

**Recommendation:** start with a real inventory of `mm2src/coins/solana/`'s
actual `solana-keypair`/`solana-signature` call sites (step 1 below) before
picking between B and C — if usage is as narrow as it looks, C is likely
less total work than it sounds and permanently removes the upstream
dependency, matching how `sia-rust` was already handled. Option A costs
nothing today; keep it as the fallback if B/C prove larger than expected.

## Scope

- `mm2src/coins/solana/` (`mod.rs`, `solana_common.rs`, `solana_types.rs`,
  `rpc_client.rs`, `rpc_pool.rs`, `spl.rs`, plus their test files)
- `mm2src/coins/Cargo.toml` `solana-*` dependency block (lines ~186–196)

## Planned order

1. Inventory every `solana_keypair::`/`solana_signature::` (and any
   re-exported `ed25519_dalek::`) call site in `mm2src/coins/solana/`;
   classify as keygen, signing, verification, or serialization-only.
2. Decide B vs. C based on that inventory (see Recommendation above).
3. If C: implement the reloaded-owned signer against `ed25519-dalek 2`,
   matching `solana-keypair`'s exact byte-level keypair/signature encoding
   (this must produce byte-identical signatures/addresses to today's
   `solana-keypair 2.2.3` output — treat this as a wire-compatibility
   constraint, not just an API-compatibility one).
4. If B: fork, patch, port call sites, same wire-compatibility constraint.
5. Regression-test against known-good vectors: existing wallet addresses and
   at least one previously-broadcast signed transaction, to confirm
   byte-identical output before/after.

## Risk areas

- Address derivation must stay byte-identical (existing users' Solana
  addresses must not change)
- Signature encoding must stay byte-identical (network/RPC compatibility)
- Any accidental widening of the dependency surface if forking pulls in
  more of `ed25519-dalek 2`'s API than intended

## Required verification

- `cargo tree -i ed25519-dalek@1.0.1` / `-i curve25519-dalek@3.2.0` return
  nothing after the change
- Regression test: known keypair seed → same derived pubkey, same signature
  over a fixed test message, before and after
- `coins` unit/integration tests covering Solana activation, balance, and
  transaction signing/broadcast
- `cargo deny check advisories` green with RUSTSEC-2022-0093 and
  RUSTSEC-2024-0344 removed from `deny.toml`'s `ignore` list

## Release posture

Independent of `secp256k1-migration.md` and `walletconnect-fork-upgrade.md`
— can run in parallel with either, on its own branch
(`dep/solana-sdk-dalek2`) off `dev`. Not funds-custody-adjacent in the same
way as Lightning, but wallet-address/signature correctness still warrants
the byte-identical regression check above before merge.
