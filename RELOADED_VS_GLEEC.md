# KDF Reloaded v0.1.0-alpha.1 — Comparison to GLEEC KDF v\<X.Y.Z\>

> **Status: skeleton.** Detail sections will be filled in as v0.1.0-alpha.1 is finalised. The summary lists below are the authoritative public catalogue of differences once the alpha is tagged.

## What is KDF Reloaded?

KDF Reloaded is a GPLv2-only continuation of the Komodo DeFi Framework codebase, anchored to upstream commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` (2022-06-03) — the last commit unambiguously distributed under GPLv2-only by upstream. All work since that anchor in this repository is original to KDF Reloaded and licensed under GPLv2-only.

Credit for the original design and the bulk of the pre-anchor code is due to Komodo Platform and the AtomicDEX-API contributors, and for ongoing development on the parallel branch to GLEEC and the GLEEC KDF contributors. KDF Reloaded aims to remain **operationally compatible where practical** with GLEEC KDF: where we diverge, we expose per-feature compatibility switches so existing GLEEC-KDF integrations can retain the original behaviour with explicit configuration. We follow GLEEC's evolution and incorporate compatible changes wherever it is legally and technically feasible and where the change does not contradict our own goals (a free, open trading platform under GPLv2).

The GLEEC fork of the same upstream codebase diverged from the joint history at upstream commit `d36369980a6c08f8689b64df56fbccf0097a0a6f` and continues under a different licensing posture. KDF Reloaded and GLEEC KDF share the pre-`d3636998` history; everything since is independent.

For licensing details, see [`LEGAL/LICENSE`](LEGAL/LICENSE) and [`LEGAL/COPYING`](LEGAL/COPYING).

## Compatibility convention

KDF Reloaded does **not** define a single global compatibility mode, and does **not** introduce a common code construct (no `compatibility` JSON object, no `CompatMode` enum, no shared registry). Instead, every behavioural divergence from GLEEC KDF that could affect operators, users, or third-party API integrations is implemented in whatever shape fits the feature, and is then documented in two specific places so that any operator can reach the GLEEC-equivalent behaviour:

1. Next to the setting itself, a short "set this to `<value>` for GLEEC compatibility" note.
2. A row in the central admin chapter [`docs/GLEEC_COMPATIBILITY.md`](docs/GLEEC_COMPATIBILITY.md), which lists every such setting end-to-end.

The developer-facing rule (mandatory for AI assistants, strong recommendation for human contributors) lives in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md). For v0.1.0-alpha.1 the central chapter is empty — no behavioural divergences require operator configuration yet — but the convention, the central chapter scaffold, and the developer rule are in place.

## Summary

### Added in KDF Reloaded

- Compatibility convention and central admin chapter for GLEEC-equivalent operation; see [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md) and [`docs/GLEEC_COMPATIBILITY.md`](docs/GLEEC_COMPATIBILITY.md).
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

### Settings to set for GLEEC-compatible operation

See [`docs/GLEEC_COMPATIBILITY.md`](docs/GLEEC_COMPATIBILITY.md). For v0.1.0-alpha.1 the list is empty — no operator configuration is required to match GLEEC KDF behaviour.

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
- For changes we *cannot* incorporate verbatim (licensing, project-direction, or quality reasons), we still expose a per-setting opt-in so the GLEEC-equivalent behaviour remains reachable, and we list that setting in [`docs/GLEEC_COMPATIBILITY.md`](docs/GLEEC_COMPATIBILITY.md).
- The only exception is a behaviour that would either violate GPLv2 or directly contradict the project's principle of free, open trading at reasonable cost. In that case the divergent setting defaults to the KDF Reloaded behaviour and the original-compatible value is gated behind an explicit acknowledgement with a runtime warning. See the developer rule in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md).
