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
| 0.20.3 | **0.4.0** | `bip32 0.2.2` (direct HD path used by `crypto`/`kdf_keys`/`hw_common`/`mm2_p2p`/`mm2_main`/`mm2_eth`/`kdf_walletconnect`, all of which also depend on `secp256k1 = "0.20"` directly) **and** `bitcoin 0.27.1` (pulled in by the vendored `rust-lightning-patched` LDK fork) |
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

## Scope reduction: decouple Lightning first (cheap, do this regardless)

`mm2src/coins/lp_coins.rs:225` already gates the entire `lightning` module
behind `#[cfg(not(target_arch = "wasm32"))]` (the module's own `mod.rs`
comment says as much). But `coins/Cargo.toml`'s `lightning`,
`lightning-invoice`, and `lightning-background-processor` dependencies (lines
91–93) are declared **unconditionally**, unlike `lightning-persister` /
`lightning-net-tokio` which are already correctly scoped under
`[target.'cfg(not(target_arch = "wasm32"))'.dependencies]` (lines 163–164).
That mismatch is why `bitcoin 0.27.1` (and its `secp256k1 0.20`/
`secp256k1-sys 0.4.0`) shows up in the wasm32 dependency graph at all today
for a module that never runs there.

**Action (independent, low-risk, do before or alongside phase 1 below):**
move `lightning`, `lightning-invoice`, and `lightning-background-processor`
into the existing `not(wasm32)` target block in `mm2src/coins/Cargo.toml`.
Zero functional change (the consuming code is already excluded), but it
removes the entire `rust-lightning-patched` → `bitcoin 0.27.1` subtree from
the WASM build, which means **this migration does not need to also touch or
upgrade the vendored LDK fork** (see `docs/plans/lightning-ldk-upgrade.md` —
that's a separate, much larger, funds-sensitive project with its own
timeline). It does *not* by itself clear the link error, since `crypto`,
`kdf_keys`, `hw_common`, `mm2_p2p`, `mm2_main`, `mm2_eth`, and
`kdf_walletconnect` all still depend on `secp256k1 = "0.20"` directly — the
full migration below is still required — but it removes the hardest,
riskiest sub-dependency (a heavily patched, ancient LDK fork) from this
migration's critical path for a 3-line change.

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
- Lightning-related crates: **excluded from this migration's WASM-blocking
  scope** per the decoupling above; leave `rust-lightning-patched`'s own
  `bitcoin 0.27.1`/`secp256k1 0.20` alone unless/until
  `docs/plans/lightning-ldk-upgrade.md` is executed. Native (non-wasm)
  builds already tolerate the dual `secp256k1-sys` family today, so there is
  no native-side reason to touch Lightning's pin as part of this plan.

## Planned order

0. **Lightning wasm-gating fix** (above) — land first, independently, as a
   trivial dep-hygiene commit; it shrinks this plan's blast radius before
   the real migration starts.
1. Inventory all remaining `secp256k1` call sites and classify them:
   - mechanical API rename only;
   - type-shape change;
   - recovery/signature-sensitive;
   - HD/key-export/swap-sensitive.
2. Migrate leaf crates that only consume the API internally.
3. Migrate shared crypto/key crates (`crypto`, `kdf_keys`, `hw_common`).
4. Migrate coin, swap, transport, and WalletConnect crates (`coins`,
   `mm2_p2p`, `mm2_main`, `mm2_eth`, `kdf_walletconnect`).
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
