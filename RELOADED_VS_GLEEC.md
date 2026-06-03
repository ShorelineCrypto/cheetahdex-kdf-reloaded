# KDF Reloaded v0.1.0-alpha.1 — Comparison to GLEEC KDF v\<X.Y.Z\>

> **Status: skeleton.** Detail sections will be filled in as v0.1.0-alpha.1 is finalised. The summary lists below are the authoritative public catalogue of differences once the alpha is tagged.

## What is KDF Reloaded?

KDF Reloaded is a GPLv2-only continuation of the Komodo DeFi Framework codebase, anchored to upstream commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` (2022-06-03) — the last commit unambiguously distributed under GPLv2-only by upstream. All work since that anchor in this repository is original to KDF Reloaded and licensed under GPLv2-only.

The GLEEC fork of the same upstream codebase diverged from the joint history at upstream commit `d36369980a6c08f8689b64df56fbccf0097a0a6f` and continues under a different licensing posture (GPL-3.0 with additional conditions). KDF Reloaded and GLEEC KDF share the pre-`d3636998` history; everything since is independent.

For licensing details, see [`LEGAL/LICENSE`](LEGAL/LICENSE) and [`LEGAL/COPYING`](LEGAL/COPYING).

## Compatibility framework

KDF Reloaded introduces a single global compatibility switch, `kdf_compat_mode`, set in `MM2.json`:

```json
{ "kdf_compat_mode": "reloaded" }
```

Accepted values:

- `"reloaded"` (default) — KDF Reloaded native behaviour.
- `"gleec_legacy"` — opt-in to behaviours that match GLEEC KDF where they would otherwise diverge from `reloaded`.

The set of switches that respect this flag is enumerated in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md). For v0.1.0-alpha.1 the table is empty; the framework is what we ship.

## Summary

### Added in KDF Reloaded

- `kdf_compat_mode` configuration field (no behavioural switches active yet).
- `regtest-netid` Cargo feature exposing test-only netids 8100, 8999, 9000, 9998.
- On-demand `Build Linux` GitHub Actions workflow for release-profile binaries.
- Self-hosted CI runner support (`docs/CI_RUNNERS.md`).
- Split CI: format → matrix unit-tests → docker-tests, with cancel-in-progress concurrency.
- Specific mismatch-kind reporting in UTXO maker-payment validation (better operator diagnostics).

### Removed / disabled in KDF Reloaded

- iOS build target removed from the alpha release matrix.
- *(further entries to be enumerated as the alpha is finalised.)*

### Work in progress

- Hardware wallet — Ledger transport (scaffolding only; gated behind `experimental-ledger`).
- Siacoin atomic-swap operations (currently `unimplemented!`; coin activatable for non-swap use only).
- HD wallet dispatch in swap/ordermatch paths (legacy TODO; explicit error returned where unimplemented).
- *(further entries to be enumerated.)*

### Compatibility switches

| Switch | Default in `reloaded` | `gleec_legacy` behaviour | Introduced |
|--------|-----------------------|--------------------------|------------|
| *(none in v0.1.0-alpha.1)* | — | — | — |

## Detail sections

Detail subsections will be added as features land. Each entry above expands here with: rationale, code locations, test coverage, migration notes, and links to relevant pull requests.

### Anchor and divergence

- **Joint history ends at:** `c1d46c0c1592faa0860f704008b2b2381bc3840f` (2022-06-03), GPLv2-only.
- **GLEEC divergence point:** `d36369980a6c08f8689b64df56fbccf0097a0a6f` (GLEEC-side first commit under modified terms).
- **KDF Reloaded baseline:** `c1d46c0c` verbatim, then independent commits squashed by feature phase.

### Build and toolchain

- Stable Rust per `rust-toolchain.toml`. (Upstream historical README pinned `nightly-2022-02-01`; this is no longer required.)
- CMake ≥ 3.12, system C/C++ toolchain.

### Networks

- Production netids supported: **8762** (AtomicDEX), **6133** (GLEEC).
- Test netids (8100/8999/9000/9998): only available with `--features regtest-netid` and never compiled into production `mm2` binaries.
