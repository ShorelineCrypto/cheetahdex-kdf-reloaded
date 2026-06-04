# KDF Reloaded v0.1.0-alpha.1 — Comparison to GLEEC KDF v\<X.Y.Z\>

> **Status: skeleton.** Detail sections will be filled in as v0.1.0-alpha.1 is finalised. The summary lists below are the authoritative public catalogue of differences once the alpha is tagged.

## What is KDF Reloaded?

KDF Reloaded is a GPLv2-only continuation of the Komodo DeFi Framework codebase, anchored to upstream commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` (2022-06-03) — the last commit unambiguously distributed under GPLv2-only by upstream. All work since that anchor in this repository is original to KDF Reloaded and licensed under GPLv2-only.

Credit for the original design and the bulk of the pre-anchor code is due to Komodo Platform and the AtomicDEX-API contributors, and for ongoing development on the parallel branch to GLEEC and the GLEEC KDF contributors. KDF Reloaded aims to remain a **drop-in compatible** counterpart to GLEEC KDF wherever practical: where we diverge, we expose a per-feature compatibility switch so anyone with an existing GLEEC-KDF integration can keep the original behaviour. We follow GLEEC's evolution and incorporate compatible changes wherever it is legally and technically feasible and where the change does not contradict our own goals (a free, open trading platform under GPLv2).

The GLEEC fork of the same upstream codebase diverged from the joint history at upstream commit `d36369980a6c08f8689b64df56fbccf0097a0a6f` and continues under a different licensing posture. KDF Reloaded and GLEEC KDF share the pre-`d3636998` history; everything since is independent.

For licensing details, see [`LEGAL/LICENSE`](LEGAL/LICENSE) and [`LEGAL/COPYING`](LEGAL/COPYING).

## Compatibility framework

KDF Reloaded does **not** use a single global compatibility mode. Instead, every behavioural divergence from GLEEC KDF that could affect operators, users, or third-party API integrations ships with a **dedicated per-feature switch** under the `compatibility` object in `MM2.json`.

```json
{
  "netid": 8762,
  "compatibility": {
    "<switch_name>": "<value>"
  }
}
```

Each switch documents its own default and its own compatibility value. A maintained drop-in template, [`MM2_classic.json`](MM2_classic.json), pins every switch to the GLEEC-compatible value.

The full catalogue of switches, their defaults, and the rationale behind each one lives in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md). For v0.1.0-alpha.1 the catalogue is empty — no switches are active yet — but the framework, the template, and the developer rule ("any divergent change must ship with a switch") are in place.

## Summary

### Added in KDF Reloaded

- Per-feature compatibility switch framework (`compatibility` object in `MM2.json`); see [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md).
- [`MM2_classic.json`](MM2_classic.json) drop-in template for GLEEC-KDF replacement deployments.
- `regtest-netid` Cargo feature exposing test-only netids 8100, 8999, 9000, 9998 (off by default in production builds).
- On-demand `Build Linux` GitHub Actions workflow for release-profile binaries.
- Self-hosted CI runner support ([`docs/CI_RUNNERS.md`](docs/CI_RUNNERS.md)).
- Split CI: format → matrix unit-tests → docker-tests, with cancel-in-progress concurrency.
- Specific mismatch-kind reporting in UTXO maker-payment validation (better operator diagnostics).

### Removed / disabled in KDF Reloaded

- iOS build target removed from the alpha release matrix.
- *(further entries to be enumerated as the alpha is finalised.)*

### Work in progress

- Hardware wallet — Ledger transport (scaffolding only; crate not built into any artifact).
- Siacoin atomic-swap operations (HD-wallet stubs only; not wired to any default activation flow).
- HD wallet dispatch in swap/ordermatch paths (legacy iguana-key fallback retained for parity with upstream and GLEEC; documented in code).
- *(further entries to be enumerated.)*

### Compatibility switches

| Switch | Default | Compatibility value | Introduced |
|--------|---------|---------------------|------------|
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

## Forward-compatibility commitment

KDF Reloaded commits to following GLEEC KDF's evolution:

- We monitor GLEEC KDF releases and incorporate compatible changes when they are legally and technically feasible.
- For changes we *cannot* incorporate verbatim (licensing, project-direction, or quality reasons), we still expose a compatibility switch so the GLEEC behaviour remains reachable.
- The only exception is a behaviour that would either violate GPLv2 or directly contradict the project's principle of free, open trading at reasonable cost. In that case the switch defaults to the KDF Reloaded behaviour and the original-compatible value is gated behind an explicit acknowledgement key with a runtime warning. See the developer rule in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md).
