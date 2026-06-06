# KDF Reloaded

> **GPLv2-only continuation of the Komodo DeFi Framework — peer-to-peer atomic swaps, no central authority.**

[![License: GPL v2](https://img.shields.io/badge/License-GPLv2-blue.svg)](LEGAL/LICENSE)
[![Status: Alpha](https://img.shields.io/badge/status-alpha-orange.svg)](#alpha-disclaimer)

KDF Reloaded is an open-source [atomic-swap](https://en.wikipedia.org/wiki/Atomic_swap) engine for trustless peer-to-peer trading across blockchains, derived from the Komodo DeFi Framework / AtomicDEX-API codebase as it stood under the GPLv2 license.

> **Release note:** This repository is published with a documented GPLv2-only continuation posture and an explicit pre-release checklist; see [`SECURITY.md`](SECURITY.md) and [`RELEASE_CHECKLIST.md`](RELEASE_CHECKLIST.md) for release controls and operator guidance.

## Heritage

This project is a **continuation**, not a fork-of-current-upstream. It is anchored to the last commit of the upstream Komodo DeFi Framework that was unambiguously distributed under GPLv2-only (commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`, dated 2022-06-03). Post-anchor development includes independently authored work and selected imported/adapted components from publicly available, license-compatible sources; provenance notes are tracked in [`docs/reloaded-rewrite/34-provenance-ledger.md`](docs/reloaded-rewrite/34-provenance-ledger.md). All material distributed in this repository is intended to remain under GPLv2-only-compatible terms.

For the relationship to other downstream projects (notably the GLEEC fork), see [`RELOADED_VS_GLEEC.md`](RELOADED_VS_GLEEC.md).

## Alpha disclaimer

KDF Reloaded is currently in **public alpha**. APIs, on-disk formats, and the network protocol may change between alpha releases. Use only with funds you can afford to lose.

- The `mm2` binary is provided for evaluation, testing, and review.
- Mainnet swaps function on netid `8762` (AtomicDEX network) and netid `6133` (GLEEC network), but you are running unaudited pre-release software.
- GPG/minisign signatures on release artifacts are planned for the alpha cycle; verify provenance from this repository directly until signatures are published. See [`SECURITY.md`](SECURITY.md).

## What it does

- **Atomic swaps** between supported chains via Hash Time Locked Contracts (HTLCs) — no custodian, no proxy tokens, you keep your keys.
- **Multi-protocol coin support**: UTXO chains (Bitcoin family), EVM chains, Tendermint/Cosmos, Zcash (sapling), Lightning Network, and others — see [`mm2src/coins/`](mm2src/coins/).
- **Distributed orderbook** propagated over [libp2p](https://libp2p.io/) gossipsub.
- **JSON-RPC API** consumable from CLI, scripts, or third-party GUIs.

## Networks

| netid | Network | Status in alpha |
|------:|---------|-----------------|
| 8762  | AtomicDEX (default upstream network) | Supported |
| 6133  | GLEEC                                | Supported |

Other netids (including 7777, 8100, 8999, 9000, 9998) are not part of the supported alpha surface. Test-only netids exist in the codebase under the `regtest-netid` Cargo feature, off by default. See [`docs/NETWORK_CONFIG.md`](docs/NETWORK_CONFIG.md).

## Building from source

Requirements:

- Stable Rust toolchain (see [`rust-toolchain.toml`](rust-toolchain.toml))
- CMake ≥ 3.12
- A C/C++ toolchain (build-essential / Xcode CLT / MSVC)

```sh
cargo build --release --bin mm2
```

The binary is placed at `target/release/mm2`. For a development environment with full test infrastructure (Docker-based integration tests, electrum mocks, etc.) see [`docs/DEV_ENVIRONMENT.md`](docs/DEV_ENVIRONMENT.md).

For WebAssembly builds, see [`docs/WASM_BUILD.md`](docs/WASM_BUILD.md).

## Configuration

Two files drive runtime configuration:

- `MM2.json` — RPC credentials, mnemonic, `netid`, optional toggles. See the upstream developer docs for the full schema.
- `coins` — list of activatable coin definitions. A community-maintained registry lives at [github.com/KomodoPlatform/coins](https://github.com/KomodoPlatform/coins).

Minimal example:

```json
{
  "gui": "kdf-reloaded",
  "netid": 8762,
  "rpc_password": "Ent3r_Un1Qu3_Pa$$w0rd",
  "passphrase": "ENTER_UNIQUE_SEED_PHRASE_DO_NOT_REUSE"
}
```

iOS builds are not currently part of the alpha release matrix.

## Usage

Launch the daemon:

```sh
./mm2
```

It exposes a JSON-RPC server on `127.0.0.1:7783` by default. The RPC catalogue is identical to the upstream Komodo DeFi Framework where unchanged; differences are tracked in [`RELOADED_VS_GLEEC.md`](RELOADED_VS_GLEEC.md). RPC namespaces include unprefixed stable methods, `task::*` for long-running operations, `stream::*` for SSE subscriptions, and others.

## Project layout

```
mm2src/         Workspace crates (Rust)
docs/           Developer documentation
LEGAL/          License, contributor agreement, third-party notices
.github/        CI workflows
```

Notable crates: [`mm2src/mm2_main/`](mm2src/mm2_main/) (entry, RPC, swaps, ordermatch), [`mm2src/coins/`](mm2src/coins/), [`mm2src/mm2_p2p/`](mm2src/mm2_p2p/), [`mm2src/crypto/`](mm2src/crypto/).

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md) and the [PR review checklist](docs/PR_REVIEW_CHECKLIST.md). All contributors must agree to the [Developer Agreement](LEGAL/DEVELOPER-AGREEMENT) and abide by the [Code of Conduct](CODE_OF_CONDUCT.md).

For the project roadmap beyond the alpha, see [`ROADMAP.md`](ROADMAP.md). For the change log, see [`CHANGELOG.md`](CHANGELOG.md).

## License

GPLv2-only. See [`LEGAL/LICENSE`](LEGAL/LICENSE), [`LEGAL/COPYING`](LEGAL/COPYING), and [`LEGAL/THIRDPARTY-LICENSES`](LEGAL/THIRDPARTY-LICENSES).
