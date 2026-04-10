# Chapter 02 — Baseline State

**Status:** driving-spec (anchor-chapter).

The chapter binds the pinned baseline anchor every other chapter
in the document set refers to: the commit identifier, the
inherited workspace shape, the toolchain pin, the configuration
surface, the request-and-response surface, the build-target set,
the license posture, and the explicit non-claims that scope what
this chapter is and is not authoritative on.

## 2.1 Executive Summary

This chapter is the *anchor*. Every later chapter describes a delta
from the state bound here; this chapter is the agreed vocabulary
those later chapters refer to. The anchor consists of a single
chapter-bound commit, the workspace shape at that commit, the
toolchain pin at that commit, the configuration-and-request-
and-response surface at that commit, the build-target set at that
commit, and the license posture at that commit.

The inherited project at the bound anchor was an open-source
implementation of an atomic-swap exchange daemon. Its read-me at
the anchor gave it a public name; the distributed native binary
carried the chapter-bound short name `mm2`. The chapter-bound
license-directory file `LEGAL/LICENSE` at the anchor distributed
the project under the GNU General Public License version 2 with
copyright attributed to the chapter-02-bound copyright holder for
the years 2013–2018.

The anchor codebase supported atomic swaps across multiple
chapter-02-bound blockchain protocol families
(unspent-transaction-output chains, Ethereum-virtual-machine
chains, the chapter-02-bound QRC20 surface, the chapter-02-bound
Solana surface, the Lightning Network, and the chapter-02-bound
Zcash-style shielded surface), exposed those capabilities over a
request-and-response interface listening on the chapter-bound
default port (R6), and participated in a peer-to-peer mesh for
order discovery and swap negotiation. It ran on the chapter-02-
bound native-target trio (Linux x86-64, macOS, Windows x86-64) and
compiled to WebAssembly for in-browser deployment.

Bound rules R1–R3 anchor the commit, the workspace shape, and the
toolchain pin; R4–R5 anchor the workspace-member registry and the
patched-dependency registry; R6–R8 anchor the configuration
surface, the request-and-response surface, and the build-target
surface; R9 anchors the license posture; R10–R12 bound the chapter's
explicit non-claims. The chapter's substrate is the anchor itself,
not a delta from it.

## 2.2 Subsystem Shape

The anchor is a single chapter-bound commit on the historical
record the source tree descends from. The shape of the anchor is
the shape of the tree at that commit. The chapter's substrate is
the *anchor*, which is consumed by every later chapter via the
chapter-bound *the baseline tree* / *the baseline workspace*
referent (chapter 01 R15).

## 2.3 Bound Anchor Commit

**R1.** The bound anchor is a single chapter-bound commit on the
historical record the project source tree descends from. The
chapter-bound textual referent for the anchor is *the baseline
commit* or *the baseline* in any later chapter that consumes it.
The chapter-bound human-readable date is 3 June 2022. Anyone with
a working clone of the historical record reproduces the anchor by
checking out the chapter-bound commit identifier directly; later
chapters that verify claims against the anchor MUST consult the
chapter-bound commit, not a current branch tip.

## 2.4 Bound Top-Level Layout

**R2.** The chapter-bound top-level layout at the anchor is exactly:

| Bound path                                                  | Bound role                                                                                                       |
| ----------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `Cargo.toml`, `Cargo.lock`                                  | The Cargo workspace manifest and the lockfile.                                                                    |
| The workspace source directory                              | The Rust workspace; all source code lives here. R4 binds the workspace-member registry.                            |
| `rust-toolchain.toml`                                       | Pins the Rust toolchain (R3).                                                                                    |
| `Cross.toml`, `deny.toml`, `rustfmt.toml`                   | Cross-compilation, dependency-policy, and formatting configuration.                                              |
| `Dockerfile`, the variant Dockerfiles, `.dockerignore`      | Container build definitions.                                                                                     |
| The Azure-Pipelines build-and-release configuration files   | Continuous-integration pipeline definitions for build, lint, release, and WebAssembly stages.                     |
| The wrapped-request-and-response shell-script directory `etomic_build/` | Shell scripts wrapping common request-and-response calls (`buy`, `enable`, `orderbook`, `seed`, `setpassphrase`, `stop`, `userpass`, `autoprice`, `client`). |
| The auxiliary-tooling directory `iguana/tools/`             | Auxiliary tooling, retained from a predecessor project.                                                          |
| The WebAssembly build-harness directory `js/`               | WebAssembly build harness (`Dockerfile`, `package.json`, `wasm-build.sh`).                                       |
| The chapter-bound developer-documentation directory `docs/` | Developer documentation: `DEV_ENVIRONMENT.md`, `GIT_FLOW_AND_WORKING_PROCESS.md`, `HEAPTRACK.md`, `PR_REVIEW_CHECKLIST.md`, `RASPBERRY_PI4_CROSS.md`, `WASM_BUILD.md`. |
| The license-materials directory `LEGAL/`                    | `AUTHORS`, `COPYING` (GPLv2 text), `LICENSE` (project-specific GPLv2 statement), `THIRDPARTY-LICENSES`, `DEVELOPER-AGREEMENT`. |
| The WebAssembly build-helpers directory `wasm_build/`       | WebAssembly build helpers complementary to `js/`.                                                                |
| The chapter-bound continuous-integration shell-script files `start_ONE_ANOTHER_trade.sh`, `travis_cmake_linux.sh`, `travis_cmake_mac.sh` | Continuous-integration test-and-build scripts.                                                                   |
| `parity.dev.chain.json`                                     | An Ethereum chain specification used by the development Ethereum-virtual-machine node implementation.            |
| `README.md`, `CONTRIBUTING.md`                              | Project description and contribution guide.                                                                       |
| `.github/`, `.vscode/`, `.cargo/`, `.editorconfig`, `.gitignore` | Tooling configuration.                                                                                       |

## 2.5 Bound Toolchain Pin

**R3.** The chapter-bound Rust toolchain at the anchor is pinned by
the chapter-02-bound `rust-toolchain.toml` to the Rust *nightly*
channel of the chapter-bound date 2022-02-01 (`nightly-2022-02-01`).
The pin choice is part of the anchor; chapter 03 binds the
methodology by which the project migrates off the chapter-bound
unstable channel onto the chapter-03-bound stable channel.

## 2.6 Bound Workspace-Member Registry

**R4.** The chapter-bound workspace at the anchor consists of
exactly the following 33 workspace members, grouped by functional
area for legibility. Crate names below are the chapter-bound names
the later chapters cite verbatim:

*Application-core area:*

- The application-entry crate `mm2_main` (binary entry point and
  long-running daemon orchestration; §2.7 binds its top-level
  module layout).
- The central-application-context crate `mm2_core` (the chapter-
  bound shared context the rest of the workspace consults to reach
  configuration, key material, the database, and the network).
- The request-and-response data-types crate `mm2_rpc` (data types
  and protocol-level definitions shared between the dispatcher
  and the handlers).
- The error-handling-framework crate `mm2_err_handle` (the
  chapter-bound `MmError<T>`-style typed-errors framework the rest
  of the workspace consumes).
- The file-system input/output crate `mm2_io` (separated for
  portability so the WebAssembly target can substitute its own
  implementation).
- The IndexedDB-backed-storage crate `mm2_db` (storage abstraction
  for the WebAssembly target).
- The SQLite-backed-storage crate `db_common` (storage abstraction
  for the native targets).
- The HTTP-and-WebSocket networking crate `mm2_net`.
- The long-running-task crate `rpc_task` (a task framework for
  multi-step request-and-response operations that report progress
  and accept cancellation).
- The integration-test helpers crate `mm2_test_helpers` (declared
  as a workspace member; not a published crate).

*Coin-protocols area:*

- The multi-protocol coin crate `coins`. The chapter-bound coin
  sub-modules at the anchor are: the unspent-transaction-output
  module, the chapter-02-bound transaction-signing sibling, the
  Ethereum-virtual-machine module, the chapter-02-bound test-
  utilities module, the hierarchical-deterministic-wallet storage
  module, the Lightning Network module, the chapter-02-bound
  Lightning-persister and Lightning-background-processor modules,
  the chapter-02-bound QRC20 module, the coin-specific request-
  handler module, the chapter-02-bound Solana module, the chapter-
  02-bound Zcash-style shielded surface module.
- The unspent-transaction-output transaction-signing crate
  `utxo_signer` (factored out of the main coin crate for reuse).
- The Lightning-persister crate `lightning_persister` (persistent
  storage for Lightning Network channel data).
- The Lightning-background-processor crate
  `lightning_background_processor` (background-task processor for
  Lightning Network maintenance).
- The coin-activation crate `coins_activation` (so that adding a
  new coin protocol is a matter of implementing the activation
  contract here, without touching the core daemon).

*Cryptography and key-management area:*

- The key-management crate `crypto` (key management, hierarchical-
  deterministic derivation in its chapter-bound anchor form,
  passphrase handling, and the chapter-bound global key context;
  chapter 05 binds the substrate redesign).
- The Bitcoin-style primitives crate `mm2_bitcoin`, organised into
  workspace-member sub-crates: the chain sub-crate (block and
  transaction structures); the crypto sub-crate (hash functions
  used by Bitcoin-style chains, also known as `bitcrypto`); the
  keys sub-crate (Bitcoin-style address and key types); the
  primitives sub-crate (the chapter-bound `H160` / `H256` / `U256`
  types and arithmetic on them); the script sub-crate (Bitcoin
  scripting primitives); the serialization sub-crate (binary
  encoding for Bitcoin-style types); the serialization-derive
  proc-macro support sub-crate; the request-and-response sub-crate
  (request-and-response response types for Bitcoin-style node
  implementations); the test-helpers sub-crate.
- The hardware-wallet abstractions crate `hw_common` (shared
  between device-specific implementations).
- The Trezor device-protocol crate `trezor`.
- The chapter-02-bound Ledger device directory (present at the
  anchor as a directory under the workspace source directory; not
  a workspace member at the anchor; scaffolding only).

*Peer-to-peer-networking area:*

- The chapter-02-bound peer-to-peer behaviour crate `mm2-libp2p`
  (declared at workspace path `mm2_libp2p`; the project's
  peer-to-peer behaviour including transport setup, swarm wiring,
  and the project's gossip and request-response protocols).
- The in-tree gossipsub crate `gossipsub` (a chapter-bound in-tree
  copy of the gossipsub publish-and-subscribe protocol, brought
  in-tree to allow project-specific modifications).
- The in-tree floodsub crate `floodsub` (a chapter-bound in-tree
  copy of floodsub, brought in-tree on the same rationale).
- The chapter-02-bound `peers` directory (present at the anchor as
  a directory under the workspace source directory; not a
  workspace member at the anchor).

*Procedural-macro support area:*

- The serialise-error-marker trait crate `ser_error` (defines a
  trait used to mark error types as safe to serialise on request-
  and-response responses).
- The serialise-error-marker proc-macro crate `ser_error_derive`
  (proc-macro implementing the marker trait above).

*Shared-utilities area:*

- The shared-utility crate `common` (shared utility code; not
  itself declared as a workspace member in the root manifest at the
  anchor, but present as a directory under the workspace source
  directory).
- The debug-instrumented reference-counter sub-crate
  `shared_ref_counter` under the shared-utility crate.

## 2.7 Bound Patched-Dependency Registry

**R5.** The chapter-bound root manifest at the anchor pins two
sibling-allowlist patched dependencies (R5-allowlist consultation;
sibling repository under a chapter-bound compatible license, cited
per chapter 01 R14). The patch addresses a chapter-bound Android-
target backtrace issue documented in the manifest comments and
unrelated to the substantive work the document set binds. The
patched dependencies and the patch source are:

- The `backtrace` crate and the `backtrace-sys` crate, both patched
  to a chapter-bound sibling-allowlist clone of the original public
  repository, with the chapter-bound clone retained as part of the
  baseline tree by the chapter-bound `[patch.crates-io]` table.

## 2.8 Bound Application-Entry Crate Layout

The bound application-entry crate `mm2_main` houses the binary
entry point and the long-running orchestration that holds the
workspace together. The chapter-bound top-level module layout at
the anchor is:

| Bound module                          | Bound role                                                                                                       |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `mm2`, `mm2_bin`, `mm2_lib`           | The `mm2` binary entry point and the library shape used by the WebAssembly target.                                |
| `lp_native_dex`                       | Native-target startup: parsing configuration, initialising key material, launching the network, the database, the order-matching loop, the swap loop, and the request-and-response server. |
| `lp_network`                          | Wiring between the application core and the peer-to-peer behaviour — message dispatch, peer reputation, network events. |
| `lp_ordermatch` plus its sub-modules  | Order book, order placement, order matching, cancellation.                                                       |
| `lp_swap` plus its sub-modules        | Atomic-swap state machines (the chapter-bound version-one protocol at the anchor).                                |
| `lp_dispatcher`                       | Cross-subsystem event dispatch.                                                                                  |
| `lp_message_service`                  | A small message-passing facility used by the order matcher and swap loop.                                         |
| `lp_stats`                            | Network-wide statistics gathering.                                                                                |
| `database` plus its sub-modules       | Persistence schema and migrations.                                                                                |
| `rpc` plus its sub-modules            | Request-and-response dispatcher and handler routing.                                                              |
| `mm2_lib` sub-directory               | Library-mode helpers for the WebAssembly target.                                                                  |
| `notification`, `for_tests`, `docker_tests`, `mm2_tests` | Notification helpers and test scaffolding.                                                                 |

The chapter-bound `lp_*` module-name prefix (short for the chapter-
bound long-poll naming convention inherited from the predecessor
project) is part of the anchor vocabulary later chapters refer to.
A later chapter that refers to *the order-matching code* means the
chapter-bound `lp_ordermatch` module and its sub-modules; a later
chapter that refers to *the swap state machine* means the chapter-
bound `lp_swap` module and its sub-modules.

## 2.9 Bound Coin-Layer Layout

The bound multi-protocol coin crate `coins` is the workspace's
plug-point for blockchain protocols. The chapter-02-bound coin sub-
module set is bound by R4 (coin-protocols area enumeration).
Tendermint, Cosmos, the Inter-Blockchain Communication standard
surface, the TRON surface, the non-fungible-token surface, the
Siacoin surface, the WalletConnect surface, and the chapter-bound
browser-extension wallet surface are *not* present at the anchor;
later chapters (chapters 18, 19, 20, 21, 22) bind the substrate by
which the project adds them.

## 2.10 Bound Peer-to-Peer Layer

The chapter-bound peer-to-peer layer at the anchor is built on a
sibling-allowlist in-tree copy of the libp2p stack extended with
the project's own behaviour. The chapter-bound substrate wires
together:

- a publish-and-subscribe protocol for orderbook gossip, layered
  on the in-tree gossipsub copy of R4, with the in-tree floodsub
  copy of R4 retained as a chapter-bound compatibility option;
- a request-and-response protocol for direct peer queries;
- a peer-discovery and peer-reputation layer.

Bringing the in-tree gossipsub and floodsub copies in-tree allowed
the chapter-bound protocol-level modifications the project required
and that the chapter-bound off-the-shelf libp2p stack did not offer
at the anchor. Chapter 28 binds the substrate by which the project
modernises this layer onto the chapter-28-bound consolidated
libp2p stack.

## 2.11 Bound Configuration Surface

**R6.** The chapter-bound configuration surface at the anchor is
exactly two files:

| Bound file | Bound role                                                                                                                                                                              |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MM2.json` | User-facing runtime configuration. The chapter-bound documented-by-anchor-read-me minimum field set is `gui`, `netid`, `rpc_password`, `passphrase`. The chapter-bound `netid` value selects the peer-to-peer mesh; the chapter-bound anchor read-me states `7777` as the chapter-bound main-network identifier. |
| `coins`    | A request-and-response-shaped list of supported assets, with one record per coin describing its protocol family, network parameters, and default servers. The chapter-bound anchor read-me points readers to a chapter-bound sister repository as the authoritative source for the asset list. |

Both file formats are part of the chapter-bound externally-visible
inter-operability surface (chapter 01 R4): the configuration files
are authored by users and by the chapter-bound graphical-user-
interface frontends that drive the daemon, and any substantive
change to either format is a change to a public contract.

## 2.12 Bound Request-and-Response Surface

**R7.** The chapter-bound daemon at the anchor exposes a request-
and-response interface on a chapter-bound TCP port — `7783` by
default per the chapter-bound anchor read-me. The chapter-bound
read-me documents the `enable` method and a handful of related
calls; the full request-and-response surface at the anchor lives
in the chapter-bound dispatcher module of the application-entry
crate (R8 of §2.8). Request-and-response method names, payload
field names, and error codes are part of the chapter-bound
externally-visible inter-operability surface (chapter 01 R4) and
are part of the vocabulary later chapters quote verbatim.

## 2.13 Bound Build-Target Surface

**R8.** The chapter-bound build-target surface at the anchor is
exactly five primary targets:

| Bound target | Bound build path |
| ------------ | ---------------- |
| Native Linux x86-64    | Direct `cargo build`.                                              |
| Native macOS (Intel and the chapter-bound Apple-Silicon architecture) | Direct `cargo build`.                                              |
| Native Windows x86-64  | `cargo build` with the chapter-bound Microsoft C-runtime toolchain. |
| WebAssembly            | The chapter-bound WebAssembly build-harness directory `js/` and the chapter-bound complementary build-helpers directory `wasm_build/`. |
| Cross-compiled targets (Android `aarch64`, ARM Linux)              | The chapter-bound `Cross.toml` configuration.                       |

The chapter-bound Azure-Pipelines build-and-release configuration
files codify how each of these targets is built, linted, tested,
and released in the chapter-bound then-active continuous-
integration environment. The continuous-integration infrastructure
itself was migrated off Azure Pipelines onto a chapter-bound
sibling-allowlist hosted-continuous-integration provider later;
chapter 03 binds the substrate by which the project performs that
migration.

## 2.14 Bound License Posture

**R9.** The chapter-bound license-materials directory `LEGAL/` at
the anchor carries:

| Bound file | Bound role |
| ---------- | ---------- |
| `COPYING`  | The verbatim text of the GNU General Public License version 2. |
| `LICENSE`  | The chapter-bound project-specific GNU-General-Public-License-version-2 statement attributing copyright to the chapter-02-bound copyright holder for the years 2013–2018, with the chapter-bound permission-to-redistribute-and-modify clause. |
| `AUTHORS`  | The contributor list known to the project at the anchor. |
| `THIRDPARTY-LICENSES` | The chapter-bound accumulated license texts for in-tree third-party material. |
| `DEVELOPER-AGREEMENT` | The chapter-bound contribution-terms statement then in force. |

The chapter-bound anchor tree is therefore distributed under the
GNU General Public License version 2 in its entirety, with the
exception of in-tree third-party material whose own licenses are
recorded in the chapter-bound `THIRDPARTY-LICENSES` file. The
document set treats any artefact present at the anchor — code,
comment, documentation, identifier name, schema field,
configuration key — as a chapter-01 R1 permitted-input class
(*the baseline itself*).

## 2.15 Bound Non-Claims

**R10.** This chapter is a *snapshot*; it MUST NOT describe any
code, comment, or document produced under the chapter-01-bound
relicensed terms of R8 of chapter 01. Any such material is a
chapter-01 R8 forbidden-input class and MUST NOT be carried here.

**R11.** This chapter MUST NOT claim that the bound workspace-
member registry of R4 is still organised as at the anchor in the
present source tree (it is not; chapters 03 through 28 bind the
delta).

**R12.** This chapter MUST NOT describe the chapter-bound live
peer-to-peer-mesh protocol (the chapter-bound `netid 7777` shape
or otherwise). The chapter-bound version-one swap protocol is
characterised by the chapter-bound anchor source itself; later
chapters that bind the substrate of a chapter-bound version-two
swap protocol do so on their own terms (chapters 13 / 15 / 16 /
17).

## 2.16 Tests

This chapter binds an anchor; it has no test surface of its own.
The chapter-bound verification discipline that the anchor is
correctly stated is carried by V1–V3 of §2.18.

## 2.17 Deferred Work

**D1.** A chapter-bound mechanical verification harness that
re-derives the bound workspace-member registry of R4, the bound
patched-dependency registry of R5, and the bound module layout of
§2.8 from the anchor commit is deferred to chapter 30 D1 (the
audit-tooling-gap binding).

## 2.18 Baseline Verifications

**V1.** The chapter-bound anchor commit identifier MUST be
confirmed to exist on the historical record the source tree
descends from, and MUST be confirmed to carry the chapter-bound
anchor date.

**V2.** The chapter-bound workspace-member registry of R4 MUST be
confirmed against the chapter-bound root manifest of R2 at the
anchor: the chapter-bound 33-member count, the chapter-bound
member names, and the chapter-bound functional-area groupings of
R4 MUST all match.

**V3.** The chapter-bound license-materials directory `LEGAL/` of
R9 MUST be confirmed to carry the chapter-bound files and the
chapter-bound license-statement text at the anchor.

## 2.19 External References

- The GNU General Public License version 2 of June 1991 (Free
  Software Foundation, Inc.). The full text is reproduced at the
  anchor as the chapter-02-bound `LEGAL/COPYING` file.
- The chapter-bound Komodo Platform sibling-repository for the
  asset list referenced by the anchor read-me (chapter 01 R14
  sibling-allowlist citation).
- The Rust toolchain nightly channel of 2022-02-01
  (`nightly-2022-02-01`), pinned at the anchor by the chapter-
  bound `rust-toolchain.toml` of R3.
- The chapter-bound Cargo feature-resolver version two
  documentation page, and the chapter-bound `resolver = "2"`
  selection in the root manifest of R2.

## 2.20 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of R1; chapter 01 (the methodology this chapter is shaped
  by, the chapter-01 R15 baseline-citation rule, the chapter-01
  R1–R5 permitted-input classes the chapter consumes); chapter 30
  (the audit-tooling-gap this chapter's D1 hands off to); the
  chapter-bound anchor read-me, the chapter-bound anchor root
  manifest, the chapter-bound license-materials directory of R9 —
  all consulted directly at the chapter-bound anchor commit per
  R1.
- *Permitted-input classes used:* the baseline itself
  (chapter 01 R1).
- *Sibling-allowlist consultations:* the chapter-bound Komodo
  Platform sibling-repository asset-list citation of §2.19; the
  chapter-bound sibling-allowlist clone of the public backtrace
  repository cited by R5.
- *Forbidden corpus:* not consulted.
