# Chapter 02 — The Baseline State at the June 2022 Commit

## Executive Summary

This chapter describes the state of the inherited codebase at the
baseline commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`
(3 June 2022). Every later chapter in this document set describes a
delta from this state; this chapter is the agreed vocabulary the
later chapters refer to.

The inherited project, hosted at the time as
`github.com/KomodoPlatform/atomicdex-api`, was an open-source
implementation of an atomic-swap exchange daemon. Its README at the
baseline gave it the user-facing name *AtomicDEX API*, and the
distributed binary was named `mm2`. The `LEGAL/LICENSE` file at the
baseline distributed the project under the GNU General Public
License version 2, with copyright attributed to "The SuperNET
Developers" for the years 2013–2018.

The daemon supported atomic swaps across multiple blockchain
protocol families (UTXO chains, EVM chains, QRC20, Solana, the
Lightning Network, and Z-coin–style shielded UTXO chains), exposed
those capabilities over a JSON-RPC interface listening on TCP
port 7783 by default, and participated in a peer-to-peer mesh for
order discovery and swap negotiation. It ran on 64-bit Linux,
macOS, and Windows, and could also be compiled to a WebAssembly
target for in-browser deployment. Configuration was carried in two
files: `MM2.json` (user secrets, network identifier, RPC password)
and `coins` (a JSON list of supported assets, sourced from a sister
repository `KomodoPlatform/coins`).

The codebase was a single Rust Cargo workspace under `mm2src/`,
containing 33 workspace members. The toolchain was pinned to Rust
nightly `nightly-2022-02-01` via `rust-toolchain.toml`. A small
amount of supporting material lived alongside the workspace:
`iguana/tools` and `etomic_build/` shell helpers, a `js/` directory
holding the WebAssembly build harness, a `docs/` directory with a
handful of developer notes, Azure Pipelines and Docker
infrastructure files, and the `LEGAL/` directory with the license,
copying, third-party-license, and developer-agreement texts.

The reader should leave this chapter knowing the names of the
workspace members and the broad areas of functionality each one
covers, because subsequent chapters refer to those names without
re-establishing them.

## Reproduction Detail

### 2.1 Obtaining the baseline tree

The baseline is a single Git commit. Anyone with a working clone of
the repository can reproduce the working tree of this chapter by:

```bash
git checkout c1d46c0c1592faa0860f704008b2b2381bc3840f
```

All file names, paths, and listings in the rest of this chapter
refer to that working tree. Readers verifying claims against later
revisions of the project should consult the same commit, not the
current branch tip.

### 2.2 Top-level layout

The repository root at the baseline contains:

| Path | Role |
|---|---|
| `Cargo.toml`, `Cargo.lock` | Workspace manifest and lockfile |
| `mm2src/` | The Rust workspace; all source code lives here |
| `rust-toolchain.toml` | Pins Rust to `nightly-2022-02-01` |
| `Cross.toml`, `deny.toml`, `rustfmt.toml` | Cross-compilation, dependency-policy, and formatting configuration |
| `Dockerfile`, `Dockerfile.release`, `Dockerfile.dev-release`, `Dockerfile.parity.dev`, `Dockerfile.ubuntu.ci`, `Dockerfile.armv7-unknown-linux-gnueabihf`, `.dockerignore` | Container build definitions |
| `azure-pipelines.yml` and four `azure-pipelines-*-stage-job.yml` files | CI pipeline definitions for build, lint, release, and WASM stages |
| `etomic_build/` | Shell scripts wrapping common RPC calls (`buy`, `enable`, `orderbook`, `seed`, `setpassphrase`, `stop`, `userpass`, `autoprice`, `client`) |
| `iguana/tools/` | Auxiliary tooling, retained from a predecessor project |
| `js/` | WebAssembly build harness (`Dockerfile`, `package.json`, `wasm-build.sh`) |
| `docs/` | Developer documentation: `DEV_ENVIRONMENT.md`, `GIT_FLOW_AND_WORKING_PROCESS.md`, `HEAPTRACK.md`, `PR_REVIEW_CHECKLIST.md`, `RASPBERRY_PI4_CROSS.md`, `WASM_BUILD.md` |
| `LEGAL/` | License materials: `AUTHORS`, `COPYING` (GPLv2 text), `LICENSE` (project-specific GPLv2 statement), `THIRDPARTY-LICENSES`, `DEVELOPER-AGREEMENT` |
| `wasm_build/` | WebAssembly build helpers complementary to `js/` |
| `start_ONE_ANOTHER_trade.sh`, `travis_cmake_linux.sh`, `travis_cmake_mac.sh` | Legacy CI/test scripts |
| `parity.dev.chain.json` | An Ethereum chain specification used by the Parity-backed development EVM node |
| `README.md`, `CONTRIBUTING.md` | Project description and contribution guide |
| `.github/`, `.vscode/`, `.cargo/`, `.editorconfig`, `.gitignore` | Tooling configuration |

### 2.3 The Cargo workspace

`Cargo.toml` at the root declares 33 workspace members under
`mm2src/`. They group naturally into the functional areas listed
below. Where a crate name in the manifest differs from a customary
short name used in the rest of this document set, the short name
is shown in parentheses.

**Application core.**

- `mm2src/mm2_main` — the binary's entry point and the home of the
  long-running daemon orchestration; see §2.4.
- `mm2src/mm2_core` — the central application context shared
  between subsystems (the object that other crates receive and
  consult to reach configuration, key material, the database, the
  network, and so on).
- `mm2src/mm2_rpc` — RPC data types and protocol-level definitions
  shared between the dispatcher and the handlers.
- `mm2src/mm2_err_handle` — the error-handling framework
  (`MmError<T>`-style typed errors) the rest of the codebase uses.
- `mm2src/mm2_io` — file-system input/output utilities, separated
  for portability so that the WebAssembly target can substitute
  its own implementation.
- `mm2src/mm2_db` — IndexedDB-backed storage abstractions for the
  WebAssembly target.
- `mm2src/db_common` — SQLite-backed storage abstractions for the
  native targets.
- `mm2src/mm2_net` — HTTP, WebSocket, and related networking
  primitives.
- `mm2src/rpc_task` — a task framework for long-running, multi-step
  RPC operations that report progress and accept cancellation.
- `mm2src/mm2_test_helpers` — helpers reused by integration tests
  across crates (declared in the workspace; not a published
  crate).

**Coin protocols.**

- `mm2src/coins` — the multi-protocol coin layer; see §2.5.
  Internal subdirectories at the baseline are `eth` (EVM),
  `for_tests`, `hd_wallet_storage`, `lightning`,
  `lightning_background_processor`, `lightning_persister`, `qrc20`,
  `rpc_command`, `solana`, `utxo`, `utxo_signer`, and `z_coin`.
- `mm2src/coins/utxo_signer` — UTXO transaction signing, factored
  out of the main `coins` crate so it can be reused.
- `mm2src/coins/lightning_persister` — persistent storage for
  Lightning Network channel data.
- `mm2src/coins/lightning_background_processor` — background-task
  processor for Lightning Network maintenance.
- `mm2src/coins_activation` — coin activation flows. A separate
  crate so that adding a new coin protocol is a matter of
  implementing the activation contract here, without touching the
  core daemon.

**Cryptography and key management.**

- `mm2src/crypto` — key management, hierarchical-deterministic
  derivation (in its baseline form; see chapter 05 for the
  post-baseline rework), passphrase handling, and the global key
  context.
- `mm2src/mm2_bitcoin` — UTXO-protocol primitives, organised into
  sub-crates (also workspace members):
  - `mm2_bitcoin/chain` — block and transaction structures.
  - `mm2_bitcoin/crypto` — hash functions used by Bitcoin-style
    chains (`bitcrypto`).
  - `mm2_bitcoin/keys` — Bitcoin-style address and key types.
  - `mm2_bitcoin/primitives` — `H160`, `H256`, `U256`, and
    arithmetic on them.
  - `mm2_bitcoin/script` — Bitcoin scripting primitives.
  - `mm2_bitcoin/serialization` — binary encoding for Bitcoin-style
    types.
  - `mm2_bitcoin/serialization_derive` — proc-macro support for
    `serialization`.
  - `mm2_bitcoin/rpc` — RPC response types for Bitcoin-style nodes.
  - `mm2_bitcoin/test_helpers` — testing utilities for the family.
- `mm2src/hw_common` — hardware-wallet abstractions shared between
  device-specific implementations.
- `mm2src/trezor` — Trezor-device protocol implementation.
- `mm2src/ledger` — present as a directory at the baseline but not
  a workspace member; scaffolding only.

**Peer-to-peer networking.**

- `mm2src/mm2_libp2p` (crate name `mm2-libp2p`) — the project's
  libp2p-based peer-to-peer behaviour, including transport setup,
  swarm wiring, and the project's gossip and request-response
  protocols.
- `mm2src/gossipsub` — a vendored copy of the gossipsub pub-sub
  protocol, brought in-tree to allow project-specific
  modifications.
- `mm2src/floodsub` — a vendored copy of floodsub, similarly
  in-tree.
- `mm2src/peers` — present at the baseline as a directory under
  `mm2src/`; not a workspace member at the baseline.

**Procedural-macro support.**

- `mm2src/derives/ser_error` — defines a trait used to mark error
  types as safe-to-serialise on RPC responses.
- `mm2src/derives/ser_error_derive` — proc-macro implementing the
  trait above.

**Shared utilities.**

- `mm2src/common` (crate name `common`) — shared utility code; not
  itself listed as a workspace member at the root manifest, but
  present as a directory under `mm2src/`.
- `mm2src/common/shared_ref_counter` — a debug-instrumented
  reference-counter wrapper.

The complete list of workspace members declared in the root
`Cargo.toml` at the baseline is reproduced verbatim below. Readers
verifying claims about later renamings, splits, or merges of these
crates should refer to this list as the canonical baseline:

```toml
[workspace]
members = [
    "mm2src/coins",
    "mm2src/common/shared_ref_counter",
    "mm2src/coins/lightning_persister",
    "mm2src/coins/lightning_background_processor",
    "mm2src/coins/utxo_signer",
    "mm2src/coins_activation",
    "mm2src/crypto",
    "mm2src/db_common",
    "mm2src/derives/ser_error",
    "mm2src/derives/ser_error_derive",
    "mm2src/floodsub",
    "mm2src/gossipsub",
    "mm2src/hw_common",
    "mm2src/mm2_bitcoin/crypto",
    "mm2src/mm2_bitcoin/chain",
    "mm2src/mm2_bitcoin/keys",
    "mm2src/mm2_bitcoin/rpc",
    "mm2src/mm2_bitcoin/primitives",
    "mm2src/mm2_bitcoin/script",
    "mm2src/mm2_bitcoin/serialization",
    "mm2src/mm2_bitcoin/serialization_derive",
    "mm2src/mm2_bitcoin/test_helpers",
    "mm2src/mm2_core",
    "mm2src/mm2_db",
    "mm2src/mm2_err_handle",
    "mm2src/mm2_test_helpers",
    "mm2src/mm2_libp2p",
    "mm2src/mm2_main",
    "mm2src/mm2_net",
    "mm2src/mm2_io",
    "mm2src/mm2_rpc",
    "mm2src/rpc_task",
    "mm2src/trezor",
]
resolver = "2"
```

The manifest also pins two patched dependencies:

```toml
[patch.crates-io]
backtrace = { git = "https://github.com/artemii235/backtrace-rs.git" }
backtrace-sys = { git = "https://github.com/artemii235/backtrace-rs.git" }
```

These patches address an Android-target backtrace issue
(`HAVE_DL_ITERATE_PHDR`) documented in the manifest comments and
unrelated to the post-baseline work this document set describes.

### 2.4 The application-core crate (`mm2_main`)

`mm2_main` houses the binary entry point and the long-running
orchestration that holds the project together. Top-level files
under `mm2src/mm2_main/src/` at the baseline are:

| File | Role |
|---|---|
| `mm2.rs`, `mm2_bin.rs`, `mm2_lib.rs` | The `mm2` binary entry point and the library shape used by the WebAssembly target |
| `lp_native_dex.rs` | Native-target startup: parsing configuration, initialising key material, launching the network, the database, the order-matching loop, the swap loop, and the RPC server |
| `lp_network.rs` | Wiring between the application core and the peer-to-peer behaviour in `mm2_libp2p` — message dispatch, peer reputation, network events |
| `lp_ordermatch.rs` plus `lp_ordermatch/` | Order book, order placement, order matching, cancellation |
| `lp_swap.rs` plus `lp_swap/` | Atomic-swap state machines (the v1 protocol at the baseline) |
| `lp_dispatcher.rs` | Cross-subsystem event dispatch |
| `lp_message_service.rs` | A small message-passing facility used by the order matcher and swap loop |
| `lp_stats.rs` | Network-wide statistics gathering |
| `database.rs` plus `database/` | Persistence schema and migrations |
| `rpc.rs` plus `rpc/` | RPC dispatcher and handler routing |
| `mm2_lib/` | Library-mode helpers for the WebAssembly target |
| `notification/`, `for_tests/`, `docker_tests/`, `mm2_tests/` | Notification helpers and test scaffolding |

The names of the `lp_*` modules — short for "long-poll", a naming
convention inherited from the project's predecessor — are part of
the baseline vocabulary that subsequent chapters refer to. When a
later chapter says "the order-matching code", it means
`lp_ordermatch.rs` and the directory of the same stem; when it
says "the swap state machine", it means `lp_swap.rs` and its
directory.

### 2.5 The coin layer (`coins`)

The `coins` crate is the project's plug-point for blockchain
protocols. At the baseline its sub-modules are:

| Sub-module | Protocol family |
|---|---|
| `utxo` | Bitcoin-derived UTXO chains |
| `utxo_signer` | UTXO transaction signing, factored into a sibling crate |
| `eth` | Ethereum and EVM-compatible chains |
| `qrc20` | Qtum's QRC-20 token standard (UTXO chain with EVM-style contract calls) |
| `solana` | Solana |
| `z_coin` | Zcash-style shielded UTXO chains |
| `lightning` | Bitcoin Lightning Network |
| `hd_wallet_storage` | Hierarchical-deterministic wallet persistence used across coin types |
| `rpc_command` | Coin-specific RPC handlers (withdraw, etc.) |
| `for_tests` | Test utilities for coin developers |

Tendermint, Cosmos, IBC, and TRON support are not present at the
baseline. NFT support is not present at the baseline. Siacoin
support is not present at the baseline. WalletConnect and MetaMask
integration are not present at the baseline. These protocols and
integrations are the subjects of later chapters (see chapters 18,
19, 20, 21).

### 2.6 The peer-to-peer layer (`mm2_libp2p`, `gossipsub`, `floodsub`)

The peer-to-peer layer at the baseline is built on a vendored copy
of `libp2p` extended with the project's own behaviour. The
project-specific behaviour wires together a set of sub-protocols:

- A pub-sub protocol for orderbook gossip, layered on `gossipsub`
  (with `floodsub` retained as a compatibility option).
- A request-response protocol for direct peer queries.
- A peer-discovery and -reputation layer.

Vendoring `gossipsub` and `floodsub` in-tree allowed protocol-level
modifications that the project required and that an unmodified
upstream `libp2p` did not offer at the baseline. Chapter 28
describes the post-baseline modernisation of this layer.

### 2.7 The configuration surface

A daemon at the baseline is configured by two files:

- `MM2.json` — user-facing runtime configuration. The example in
  the baseline `README.md` documents at minimum the fields `gui`,
  `netid`, `rpc_password`, and `passphrase`. The `netid` value
  selects the peer-to-peer mesh; the baseline README states
  `7777` as "the current main network".
- `coins` — a JSON list of supported assets, with one record per
  coin describing its protocol family, network parameters, and
  default servers. The baseline README points readers to the
  sister repository `github.com/KomodoPlatform/coins` as the
  authoritative source.

Both file formats are part of the externally-visible interface
this project must inter-operate with: the configuration files are
authored by users and by the GUIs that drive the daemon, and any
post-baseline change to either format is a change to a public
contract.

### 2.8 The RPC surface

The daemon exposes a JSON-RPC interface on a TCP port — `7783` by
default per the baseline README. The README documents the `enable`
method and a handful of related calls; the full RPC surface at the
baseline lives in `mm2src/mm2_main/src/rpc.rs` and the directory of
the same name. RPC method names, payload field names, and error
codes are part of the project's external contract and are part of
the vocabulary later chapters quote verbatim.

### 2.9 The build surface

The baseline supports five primary targets:

- **Native Linux x86-64**, via direct `cargo build`.
- **Native macOS** (Intel and Apple Silicon), via direct `cargo
  build`.
- **Native Windows x86-64**, via `cargo build` with MSVC.
- **WebAssembly**, via the `js/` and `wasm_build/` build harness.
- **Cross-compiled targets** (Android `aarch64`, ARM Linux), via
  the `Cross.toml` configuration.

The `azure-pipelines*.yml` files codify how each of these targets
is built, linted, tested, and released in the project's then-active
CI environment. The CI infrastructure itself was migrated to
GitHub Actions later in the post-baseline work; chapter 03 covers
that change.

### 2.10 The license posture

`LEGAL/COPYING` carries the verbatim text of the GNU General Public
License version 2. `LEGAL/LICENSE` carries the project's
project-specific GPLv2 statement: "Copyright © 2013-2018 The
SuperNET Developers. This program is free software; you can
redistribute it and/or modify it under the terms of the GNU
General Public License version 2, as published by the Free Software
Foundation." `LEGAL/AUTHORS` lists the contributors known to the
project at the time. `LEGAL/THIRDPARTY-LICENSES` accumulates
upstream license texts for vendored dependencies.
`LEGAL/DEVELOPER-AGREEMENT` documents the contribution terms then
in force.

The baseline tree is therefore distributed under GPLv2 in its
entirety, with the exception of in-tree third-party material whose
own licenses are recorded in `THIRDPARTY-LICENSES`. This document
set treats any artefact present at the baseline — code, comment,
documentation, identifier name, schema field, configuration key —
as available material in line with the inputs permitted by
[`01-clean-room-rules.md`](01-clean-room-rules.md) §2.

### 2.11 What this chapter explicitly does *not* establish

This chapter is a snapshot. It does not:

- describe any code, comment, or document produced upstream after
  the baseline; that material is forbidden input under
  [`01-clean-room-rules.md`](01-clean-room-rules.md) §3;
- claim that the listed crates are still organised as at the
  baseline in the present tree (they are not; chapters 03–30
  describe the changes);
- describe the protocols of the live peer-to-peer mesh
  (`netid` 7777 or otherwise); the v1 swap protocol is
  characterised by the baseline source itself, and where later
  chapters describe a v2 protocol they do so on their own terms.

## External References

- *GNU General Public License, version 2, June 1991.* Free Software
  Foundation, Inc. The full text is reproduced at the baseline as
  `LEGAL/COPYING`.
- *Komodo Platform coins repository.* Sister repository referenced
  by the baseline README as the authoritative source for the
  project's asset list. https://github.com/KomodoPlatform/coins
- *Rust toolchain channel `nightly-2022-02-01`.* Pinned at the
  baseline by `rust-toolchain.toml`.
- *Cargo Feature Resolver, version 2.* Documented at
  https://doc.rust-lang.org/beta/cargo/reference/features.html#feature-resolver-version-2
  and selected at the baseline by `resolver = "2"` in the root
  `Cargo.toml`.

## Provenance Footer

*This chapter v1; verified directly against the baseline tree at
commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` on 2026-05-31.
Reviewer #1 and reviewer #2 reports stored at
`local/clean-room-doc/reviews/02-baseline-state-r{1,2}.md`.*
