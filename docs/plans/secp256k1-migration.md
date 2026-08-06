# Plan: secp256k1 0.20 → 0.29.x migration

> **Status:** in progress — scoping refined and root cause empirically confirmed 2026-08-06 (see "Confirmed root cause" below). Not yet started. This plan scopes a workspace-wide migration from `secp256k1 0.20` to a `secp256k1` line that shares the `secp256k1-sys 0.10.x` native build with the Zcash/alloy stacks already in the tree. It is separate from the current wasm CI workaround (`.github/workflows/build-wasm.yml` was downgraded from a `wasm-pack build` link step to per-crate `cargo check` in commit `4a54d4415`, "ci: replace wasm-pack with wasm build check" — that commit is the *symptom-hiding* workaround; this plan is the actual fix) and should only be executed when the project is ready to accept the API churn and the wider dependency ripple.

## Goal

Move the repository off the legacy `secp256k1 0.20` family without changing signing semantics, HD derivation results, public key encodings, or swap wire behavior.

## Confirmed root cause (2026-08-06)

`Cargo.lock` currently carries three `secp256k1` lines resolving to two distinct native `secp256k1-sys` builds:

| `secp256k1` | `secp256k1-sys` | Pulled by |
|------------------------|------------------------|------------------------|
| 0.20.3 | **0.4.0** | `bip32 0.2.2` (direct HD path used by `crypto`/`kdf_keys`/`hw_common`/`mm2_p2p`/`mm2_main`/`mm2_eth`/`kdf_walletconnect`, all of which also depend on `secp256k1 = "0.20"` directly) **and** `bitcoin 0.27.1` (a **direct, unconditional dependency of `coins` itself** for UTXO tx/script handling — line 40 of its `Cargo.toml` — *not* just something pulled in via the vendored `rust-lightning-patched` LDK fork, which shares the same pin; see "Scope reduction" below) |
| 0.29.1 | 0.10.1 | `bip32 0.6.0-pre.1` → the Zcash stack (`zcash_client_backend`/`zcash_keys`/`zcash_script`/`zcash_primitives`, all in `vendor-patches/`) |
| 0.30.0 | 0.10.1 | `alloy-consensus` (transitively via the workspace `alloy = "2.0"` EVM dep) |

Native and `cargo check --target wasm32-unknown-unknown` both tolerate this fine — `secp256k1-sys` 0.4.0 and 0.10.1 happily coexist as separate rlibs. The break is specifically at the **final WASM link** (`cargo build --target wasm32-unknown-unknown -p mm2_bin_lib --lib`, i.e. what `wasm-pack build` also does), reproduced 2026-08-06:

```         
rust-lld: error: duplicate symbol: WASM32_INT_SIZE
>>> defined in .../libsecp256k1_sys-68dd10659d789ce0.rlib(...-wasm.o)
>>> defined in .../libsecp256k1_sys-7956dd88569eef46.rlib(...-secp256k1.o)
```

(same for `WASM32_INT_ALIGN`, `WASM32_UNSIGNED_INT_SIZE/_ALIGN`, `WASM32_SIZE_T_SIZE/_ALIGN`, `WASM32_UNSIGNED_CHAR_SIZE/_ALIGN`, `WASM32_PTR_SIZE/_ALIGN`). Each `secp256k1-sys` version's bundled C build defines these as non-static globals for its own wasm32 size/alignment probing; wasm-ld's flat symbol namespace has no per-rlib scoping for C globals the way native linkers tolerate, so **any two distinct `secp256k1-sys` versions** landing in the same cdylib will hit this, independent of which two versions they are. Eliminating the older (0.4.0-paired) `secp256k1 0.20` line is necessary *and* sufficient — the 0.29.1 / 0.30.0 pair already coexists today without conflict because they share the same `secp256k1-sys 0.10.1`.

## Scope reduction: Lightning + bitcoin wasm-gating — done, and this time it's the full win

**Done** (`1c94c2196`, `14b94eaa5`, on `dep-hygiene`): two passes were needed, and it's worth recording both because the middle one was wrong.

**Pass 1** (`1c94c2196`): `mm2src/coins/lp_coins.rs:225` already gates the entire `lightning` module behind `#[cfg(not(target_arch = "wasm32"))]`, but `coins/Cargo.toml`'s `lightning`, `lightning-invoice`, and `lightning-background-processor` (plus `common/Cargo.toml`'s own unconditional `lightning` dep, used only by `log.rs`'s already-`not(wasm32)`-gated `impl LightningLogger`) were unconditional. Moved them to match. At this point `bitcoin 0.27.1` *still* showed up in the wasm32 tree — because `coins/Cargo.toml` also had its own **direct, unconditional** `bitcoin = "0.27.1"` line (separate from anything Lightning pulled in), so this pass alone didn't clear it. That's what the previous version of this section said, and it was correct as far as it went.

**Pass 2** (`14b94eaa5`): traced that direct `bitcoin` dependency's real usage. Outside the (already-gated) `lightning/` module, `coins` had exactly two `bitcoin::` call sites, both in `utxo.rs`: `use bitcoin::network::constants::Network as BitcoinNetwork` and `impl From<BlockchainNetwork> for BitcoinNetwork` — and both were **dead code, zero callers anywhere in the tree** (same pattern as the `LightningCurrency` dead impl from pass 1). Gated both to `not(wasm32)` and moved `bitcoin` itself into the `not(wasm32)` block. Confirmed via `cargo tree -p coins --target wasm32-unknown-unknown -i secp256k1-sys@0.4.0`: `bitcoin 0.27.1` is now **completely gone** from the wasm32 graph — it was never a real requirement outside Lightning, just leftover wiring (this codebase has precedent for exactly that class of bug going unnoticed until a build actually exercises it: see "Related finding — GLEEC KDF PR #2722" below).

**Net effect on this plan's scope: `bitcoin 0.27.1` is no longer part of it at all.** `secp256k1-sys 0.4.0` still appears in the wasm32 graph (confirmed, same `cargo tree -i` command), but purely via `bip32 0.2.2` and the direct `secp256k1 = "0.20"` pins in `crypto`/`kdf_keys`/`hw_common`/`mm2_p2p`/`mm2_main`/`mm2_eth`/`kdf_walletconnect`/`coins` — none of which touch `bitcoin` or Lightning. The vendor-patch idea floated in the previous version of this section (patching `bitcoin 0.27.1`'s own secp256k1 usage) is **not needed for this plan** — it would only matter for `docs/plans/lightning-ldk-upgrade.md`, and only if that uplift's own scoping decides to keep `bitcoin 0.27.1` pinned rather than moving to a version LDK itself has already ported forward. Noted there, not here.

### Related finding — GLEEC KDF PR #2722

While auditing this, the user pointed at
[`GLEECBTC/komodo-defi-framework#2722`](https://github.com/GLEECBTC/komodo-defi-framework/pull/2722)
(their own PR, filed after independently hitting an analogous issue). GLEEC's
`mm2_bitcoin/chain` crate has an `ext-bitcoin` feature providing
`From<chain::Transaction/BlockHeader> for bitcoin::...` conversions that
Lightning's `ln_events.rs`/`ln_platform.rs` depend on, gated behind `coins`'
default `utxo-walletconnect` feature; a commit that switched `mm2_main` to
`coins = { default-features = false }` without re-adding it silently broke
`cargo build -p mm2_main` (E0277, missing `From` impls).

Reloaded doesn't have this exact exposure — `kdf_chain` (the clean-room
GPL-2.0 rewrite of `mm2_bitcoin/chain`) has **zero `[features]`** and
**doesn't depend on the `bitcoin` crate at all**; nothing in the workspace
depends on `coins` with `default-features = false` either (checked
2026-08-06). Reloaded's Lightning gets its `bitcoin::Transaction` a
different way entirely: `lightning/ln_platform.rs`'s `kdf_tx_to_bitcoin`
serializes with KDF's own wire codec and re-deserializes the bytes via
`bitcoin::consensus::encode::deserialize` — a local, always-compiled byte
round-trip with no `kdf_chain`-level trait and no feature flag, so there's
no equivalent "feature default silently dropped" failure mode. But the
underlying lesson is directly relevant: this is the second time in one week
(GLEEC's PR, and pass 1 above finding a dead `LightningCurrency` impl) that
the `bitcoin`-crate/Lightning boundary in this family of codebases has had
silently-stale wiring around it. Treat that as a standing reason to actually
build (not just `cargo check`) both wasm and native targets whenever
anything touching this boundary changes, not a one-off.

## Target version

Recommend `secp256k1 = "0.29"` (currently resolves to `0.29.1`) for every migrated crate, rather than `0.30`. Rationale: 0.29.1 is already integration-tested in this tree via the Zcash `bip32 0.6.0-pre.1` path (HD derivation, signing), so migrated crates converge onto a version with already-proven behavior in-repo instead of an unproven-here 0.30.0 (currently only reached via `alloy-consensus`'s internal EVM-signing usage, which this repo's own code never calls directly). Cargo does **not** need this to match `alloy`'s 0.30.0 or vice versa — as the table above shows, 0.29.1 and 0.30.0 already coexist today because they share `secp256k1-sys 0.10.1`; only the 0.20-line needs to disappear. If a future dependency (e.g. a `bitcoin 0.32` adoption for the Phase B parity-bitcoin work — see the workspace `Cargo.toml` comment — pulls in secp256k1 ^0.29/^0.30 itself) forces a different exact version, that's a compatible, low-friction bump from 0.29.1, not a re-migration.

## Scope

Direct `secp256k1 = "0.20"` consumers (verified 2026-08-06, `grep -rn "^secp256k1" mm2src/*/Cargo.toml`):

-   `mm2src/coins/` (`features = ["recovery"]` — no, that's mm2_eth; coins pulls it via `crypto`/HD chain, confirm exact feature set during inventory)
-   `mm2src/crypto/`
-   `mm2src/kdf_keys/` (`features = ["rand", "recovery"]`)
-   `mm2src/hw_common/` (`features = ["rand"]`)
-   `mm2src/mm2_p2p/` (`features = ["rand"]`)
-   `mm2src/mm2_main/` (`features = ["rand"]`)
-   `mm2src/mm2_eth/` (`features = ["recovery"]`)
-   `mm2src/kdf_walletconnect/` (plain `"0.20"`, **missing from the original version of this plan** — found during the 2026-08-06 re-scope)
-   `mm2src/trezor/` — no direct `secp256k1` line found in its `Cargo.toml`; confirm during inventory whether it only consumes re-exported types from `hw_common`/`crypto` (in which case it needs no direct bump, just a transitive re-check)

**`bitcoin = "0.27.1"` and `rust-lightning-patched`: confirmed OUT of scope**
(2026-08-06, see "Scope reduction" above) — `bitcoin` no longer appears in
the wasm32 dependency graph at all once its two dead call sites in `coins`
are gated, so this plan needs zero `bitcoin`-crate or LDK work. Don't
re-introduce this as scope without a new empirical reason.

**`bip32`: real, previously-missed coupling.** All the `crypto`/`kdf_keys`/
`hw_common`/`coins` consumers above reach `secp256k1` partly *through*
`bip32 0.2.2` (feature `secp256k1-ffi`) for HD derivation, and `bip32 0.2.2`
hard-pins `secp256k1-ffi = "0.20"` in its own manifest (exact 0.x-minor pin
— Cargo can't be told to substitute 0.29 for a `bip32 0.2.2` dependency
edge). The Zcash stack's `bip32 0.6.0-pre.1` already depends on
`secp256k1-ffi = "0.29"`, confirming `bip32` itself supports it — but that
means this migration is a **coupled double bump**: `secp256k1` 0.20→0.29
*and* `bip32` 0.2.2→~0.6.x together, not secp256k1 alone. `bip32` 0.2→0.6 is
itself a large version jump (0.6.0 is still a `-pre` release as of this
writing) and likely carries independent HD-derivation-type API changes on
top of the secp256k1 churn — budget real inventory time for `bip32`'s own
diff, not just secp256k1's. (Side note, not a fix: `bip32` also offers a
pure-Rust `k256`-backed feature as an alternative to `secp256k1-ffi`, which
reloaded doesn't currently use. Switching wouldn't remove `secp256k1-sys`
from the graph by itself, since `crypto`/`kdf_keys`/`hw_common`/etc. all use
the `secp256k1` crate directly too, not just through `bip32`.)

## Planned order

0.  **Lightning + bitcoin wasm-gating fix** — **done** (`1c94c2196`,
    `14b94eaa5`, `dep-hygiene` branch). Removed `lightning`,
    `lightning-invoice`, `lightning-background-processor`, and `bitcoin`
    from the wasm32 build graph entirely (two rounds — see "Scope
    reduction" above for why the first wasn't sufficient). This is a real,
    confirmed reduction in remaining scope this time: `bitcoin 0.27.1` and
    `rust-lightning-patched` are fully out of this plan.
1.  Inventory all remaining `secp256k1` call sites — `bip32`'s own version
    coupling (Scope, above) makes this a `secp256k1` + `bip32` joint
    inventory, not `secp256k1` alone — and classify them:
    -   mechanical API rename only;
    -   type-shape change;
    -   recovery/signature-sensitive;
    -   HD/key-export/swap-sensitive.
2.  Migrate leaf crates that only consume the API internally.
3.  Migrate shared crypto/key crates (`crypto`, `kdf_keys`, `hw_common`) —
    this is where the `bip32` 0.2.2→~0.6.x bump lands.
4.  Migrate coin, swap, transport, and WalletConnect crates (`coins`,
    `mm2_p2p`, `mm2_main`, `mm2_eth`, `kdf_walletconnect`).
5.  Re-run native, Windows GNU, and wasm32 verification after each phase.
6.  Restore the WASM CI gate: re-add the `wasm-pack build` (or equivalent `cargo build --target wasm32-unknown-unknown -p mm2_bin_lib --lib`) link step to `.github/workflows/build-wasm.yml`, since that's the only check that actually exercises this failure mode — the current `cargo check` jobs will stay green throughout this entire migration without proving anything about the link step.

## Risk areas

-   HD derivation and key export
-   signature recovery and verification
-   swap key handling and order signing
-   public or persisted key encodings
-   wasm packaging and `secp256k1-sys` linkage

## Required verification

-   native build and tests for affected crates
-   `cargo build --target wasm32-unknown-unknown -p mm2_bin_lib --lib` (the actual link step — `cargo check` alone does not prove this migration worked, per the root-cause section above)
-   Windows GNU release build
-   focused regression tests for key derivation, address generation, and swap signing
-   `cargo tree -i secp256k1@0.20.3` returning nothing (confirms the old line is fully gone); `cargo tree -d secp256k1-sys` (or `cargo deny check bans`) confirming only one `secp256k1-sys` family remains

## Release posture

Do not start this migration as part of a normal bug fix. Treat it as a staged dependency project with its own branch (see the branch plan in `docs/plans/v0.2.0-dependency-hygiene.md`), its own regression pass, and its own documentation update. The Lightning wasm-gating fix (step 0) may land ahead of the rest as an ordinary dep-hygiene commit since it has no behavioral surface.