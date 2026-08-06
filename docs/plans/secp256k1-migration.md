# Plan: secp256k1 0.20 → 0.29.x migration

> **Status:** in progress — scoping refined and root cause empirically
> confirmed 2026-08-06 (see "Confirmed root cause" below). Not yet started.
> This plan scopes a workspace-wide migration from `secp256k1 0.20` to a
> `secp256k1` line that shares the `secp256k1-sys 0.10.x` native build with
> the Zcash/alloy stacks already in the tree. It is separate from the current
> wasm CI workaround (`.github/workflows/build-wasm.yml` was downgraded from
> a `wasm-pack build` link step to per-crate `cargo check` in commit
> `4a54d4415`, "ci: replace wasm-pack with wasm build check" — that commit is
> the *symptom-hiding* workaround; this plan is the actual fix) and should
> only be executed when the project is ready to accept the API churn and the
> wider dependency ripple.

## Goal

Move the repository off the legacy `secp256k1 0.20` family without changing
signing semantics, HD derivation results, public key encodings, or swap wire
behavior.

## Confirmed root cause (2026-08-06)

`Cargo.lock` currently carries three `secp256k1` lines resolving to two
distinct native `secp256k1-sys` builds:

| `secp256k1` | `secp256k1-sys` | Pulled by |
|---|---|---|
| 0.20.3 | **0.4.0** | `bip32 0.2.2` (direct HD path used by `crypto`/`kdf_keys`/`hw_common`/`mm2_p2p`/`mm2_main`/`mm2_eth`/`kdf_walletconnect`, all of which also depend on `secp256k1 = "0.20"` directly) **and** `bitcoin 0.27.1` (a **direct, unconditional dependency of `coins` itself** for UTXO tx/script handling — line 40 of its `Cargo.toml` — *not* just something pulled in via the vendored `rust-lightning-patched` LDK fork, which shares the same pin; see "Scope reduction" below) |
| 0.29.1 | 0.10.1 | `bip32 0.6.0-pre.1` → the Zcash stack (`zcash_client_backend`/`zcash_keys`/`zcash_script`/`zcash_primitives`, all in `vendor-patches/`) |
| 0.30.0 | 0.10.1 | `alloy-consensus` (transitively via the workspace `alloy = "2.0"` EVM dep) |

Native and `cargo check --target wasm32-unknown-unknown` both tolerate this
fine — `secp256k1-sys` 0.4.0 and 0.10.1 happily coexist as separate rlibs.
The break is specifically at the **final WASM link** (`cargo build --target
wasm32-unknown-unknown -p mm2_bin_lib --lib`, i.e. what `wasm-pack build`
also does), reproduced 2026-08-06:

```
rust-lld: error: duplicate symbol: WASM32_INT_SIZE
>>> defined in .../libsecp256k1_sys-68dd10659d789ce0.rlib(...-wasm.o)
>>> defined in .../libsecp256k1_sys-7956dd88569eef46.rlib(...-secp256k1.o)
```
(same for `WASM32_INT_ALIGN`, `WASM32_UNSIGNED_INT_SIZE/_ALIGN`,
`WASM32_SIZE_T_SIZE/_ALIGN`, `WASM32_UNSIGNED_CHAR_SIZE/_ALIGN`,
`WASM32_PTR_SIZE/_ALIGN`). Each `secp256k1-sys` version's bundled C build
defines these as non-static globals for its own wasm32 size/alignment
probing; wasm-ld's flat symbol namespace has no per-rlib scoping for C
globals the way native linkers tolerate, so **any two distinct
`secp256k1-sys` versions** landing in the same cdylib will hit this,
independent of which two versions they are. Eliminating the older
(0.4.0-paired) `secp256k1 0.20` line is necessary *and* sufficient — the
0.29.1 / 0.30.0 pair already coexists today without conflict because they
share the same `secp256k1-sys 0.10.1`.

## Scope reduction: Lightning wasm-gating — landed, smaller win than first scoped

**Done** (`1c94c2196`, on `dep-hygiene`): `mm2src/coins/lp_coins.rs:225`
already gates the entire `lightning` module behind
`#[cfg(not(target_arch = "wasm32"))]`, but `coins/Cargo.toml`'s `lightning`,
`lightning-invoice`, and `lightning-background-processor` (plus
`common/Cargo.toml`'s own unconditional `lightning` dep, used only by
`log.rs`'s already-`not(wasm32)`-gated `impl LightningLogger`) were
unconditional. Moved all of them to match their actual (already-gated) Rust
usage — `lightning-invoice` needed one extra step, gating a dead
`impl From<BlockchainNetwork> for LightningCurrency` in `utxo.rs` that had
no call sites anywhere in the tree. Verified: native build/clippy clean,
`cargo check --target wasm32-unknown-unknown -p coins -p common` clean,
`cargo test -p coins --lib` — 653 passed, 57 failed (all confirmed
pre-existing/environment: `FailedToConnectToElectrums` — this sandbox has no
outbound network to the real electrum servers those tests dial; unrelated to
this change).

**This does NOT clear the WASM link blocker**, and the original version of
this section overclaimed that it would — corrected 2026-08-06 after actually
implementing it and re-running `cargo tree -i secp256k1-sys@0.4.0 --target
wasm32-unknown-unknown -p coins`. `coins/Cargo.toml` has its own **direct**
`bitcoin = "0.27.1"` dependency (line 40, unconditional — for the crate's own
UTXO transaction/script handling, entirely independent of Lightning), and
*that* pulls `secp256k1 0.20.3` / `secp256k1-sys 0.4.0` into the wasm32 graph
regardless of anything Lightning-related. So `bitcoin 0.27.1` was never
purely "the Lightning fork's dependency" — it's a first-class, direct
dependency of `coins` itself, shared by (not owned by) the Lightning module.

**What this changes for the migration plan:** the same sharing is actually
useful. `lightning`, `lightning-invoice`, and `coins` all pin the *exact
same* `bitcoin = "0.27.1"`. A vendor-patch of that one shared dependency
(same pattern as `vendor-patches/zcash_client_backend-0.23.0`'s
`KDF-PATCH.md`, but source-level, not manifest-only — `bitcoin 0.27.1`'s own
Rust source calls secp256k1 0.20 APIs directly and needs real porting, not
just a version-constraint edit) would fix `coins`'s UTXO secp256k1 usage
**and** Lightning's simultaneously, without requiring a full LDK version
uplift (LDK 0.0.106's own code only touches `bitcoin`'s stable public types,
not `secp256k1` directly in most places — needs verification during
inventory). That's a materially better strategy than the original framing of
this section ("avoid touching bitcoin/LDK entirely") since avoiding
`bitcoin 0.27.1` isn't actually possible — `coins` needs *a* `bitcoin` crate
regardless. Add "vendor-patch `bitcoin 0.27.1`'s secp256k1 usage forward to
0.29/0.30" as an explicit candidate approach to evaluate in phase 1 below,
alongside a straight `coins`-side `bitcoin` crate version bump.

## Target version

Recommend `secp256k1 = "0.29"` (currently resolves to `0.29.1`) for every
migrated crate, rather than `0.30`. Rationale: 0.29.1 is already
integration-tested in this tree via the Zcash `bip32 0.6.0-pre.1` path
(HD derivation, signing), so migrated crates converge onto a version with
already-proven behavior in-repo instead of an unproven-here 0.30.0 (currently
only reached via `alloy-consensus`'s internal EVM-signing usage, which this
repo's own code never calls directly). Cargo does **not** need this to match
`alloy`'s 0.30.0 or vice versa — as the table above shows, 0.29.1 and 0.30.0
already coexist today because they share `secp256k1-sys 0.10.1`; only the
0.20-line needs to disappear. If a future dependency (e.g. a `bitcoin 0.32`
adoption for the Phase B parity-bitcoin work — see the workspace
`Cargo.toml` comment — pulls in secp256k1 ^0.29/^0.30 itself) forces a
different exact version, that's a compatible, low-friction bump from 0.29.1,
not a re-migration.

## Scope

Direct `secp256k1 = "0.20"` consumers (verified 2026-08-06, `grep -rn
"^secp256k1" mm2src/*/Cargo.toml`):

- `mm2src/coins/` (`features = ["recovery"]` — no, that's mm2_eth; coins
  pulls it via `crypto`/HD chain, confirm exact feature set during inventory)
- `mm2src/crypto/`
- `mm2src/kdf_keys/` (`features = ["rand", "recovery"]`)
- `mm2src/hw_common/` (`features = ["rand"]`)
- `mm2src/mm2_p2p/` (`features = ["rand"]`)
- `mm2src/mm2_main/` (`features = ["rand"]`)
- `mm2src/mm2_eth/` (`features = ["recovery"]`)
- `mm2src/kdf_walletconnect/` (plain `"0.20"`, **missing from the original
  version of this plan** — found during the 2026-08-06 re-scope)
- `mm2src/trezor/` — no direct `secp256k1` line found in its `Cargo.toml`;
  confirm during inventory whether it only consumes re-exported types from
  `hw_common`/`crypto` (in which case it needs no direct bump, just a
  transitive re-check)
- **`bitcoin = "0.27.1"` itself** (`mm2src/coins/Cargo.toml`, direct,
  unconditional) — **in scope**, not excludable. This is the piece the
  original version of this plan got wrong (see "Scope reduction" above):
  it's not just a Lightning artifact, `coins` needs *a* `bitcoin` crate for
  its own UTXO code regardless. Two live options, first inventory task:
  (a) vendor-patch `bitcoin 0.27.1`'s own secp256k1 usage forward to
  0.29/0.30 (source-level port, not manifest-only — see "Scope reduction"),
  which also fixes `rust-lightning-patched`'s pin for free since it shares
  the exact same `bitcoin = "0.27.1"` requirement; or (b) bump `coins`'s
  `bitcoin` dependency to a newer major version outright (larger API surface
  change: `Address`, `Script`, `Transaction` types have all moved across
  0.27 → 0.30+, on top of the secp256k1 churn).
- `rust-lightning-patched` (`lightning`, `lightning-invoice`,
  `lightning-background-processor`): **not separately in scope** as a
  version uplift (that's `docs/plans/lightning-ldk-upgrade.md`, deferred) —
  but its shared `bitcoin 0.27.1` pin gets fixed as a side effect of
  whichever option above is chosen for `coins`, since it's the same
  dependency edge. No LDK API/behavior changes needed for *this* plan.

## Planned order

0. **Lightning wasm-gating fix** — **done** (`1c94c2196`, `dep-hygiene`
   branch). Shrank the wasm32 build graph (drops the `lightning`/
   `lightning-background-processor` crates entirely) and fixed a
   Cargo.toml/cfg-gate mismatch, but does **not** reduce this plan's
   remaining scope the way originally claimed — see "Scope reduction" above.
1. Inventory all remaining `secp256k1` call sites (including `bitcoin
   0.27.1`'s own, now explicitly in scope) and classify them:
   - mechanical API rename only;
   - type-shape change;
   - recovery/signature-sensitive;
   - HD/key-export/swap-sensitive.
   Also resolve the `bitcoin 0.27.1` vendor-patch-vs-version-bump decision
   (Scope, above) during this phase — it changes how big phase 4 is.
2. Migrate leaf crates that only consume the API internally.
3. Migrate shared crypto/key crates (`crypto`, `kdf_keys`, `hw_common`).
4. Migrate coin, swap, transport, and WalletConnect crates (`coins`,
   `mm2_p2p`, `mm2_main`, `mm2_eth`, `kdf_walletconnect`), including the
   `bitcoin 0.27.1` work from step 1.
5. Re-run native, Windows GNU, and wasm32 verification after each phase.
6. Restore the WASM CI gate: re-add the `wasm-pack build` (or equivalent
   `cargo build --target wasm32-unknown-unknown -p mm2_bin_lib --lib`) link
   step to `.github/workflows/build-wasm.yml`, since that's the only check
   that actually exercises this failure mode — the current `cargo check`
   jobs will stay green throughout this entire migration without proving
   anything about the link step.

## Risk areas

- HD derivation and key export
- signature recovery and verification
- swap key handling and order signing
- public or persisted key encodings
- wasm packaging and `secp256k1-sys` linkage

## Required verification

- native build and tests for affected crates
- `cargo build --target wasm32-unknown-unknown -p mm2_bin_lib --lib`
  (the actual link step — `cargo check` alone does not prove this migration
  worked, per the root-cause section above)
- Windows GNU release build
- focused regression tests for key derivation, address generation, and swap
  signing
- `cargo tree -i secp256k1@0.20.3` returning nothing (confirms the old line
  is fully gone); `cargo tree -d secp256k1-sys` (or `cargo deny check bans`)
  confirming only one `secp256k1-sys` family remains

## Release posture

Do not start this migration as part of a normal bug fix. Treat it as a staged
dependency project with its own branch (see the branch plan in
`docs/plans/v0.2.0-dependency-hygiene.md`), its own regression pass, and its
own documentation update. The Lightning wasm-gating fix (step 0) may land
ahead of the rest as an ordinary dep-hygiene commit since it has no
behavioral surface.
