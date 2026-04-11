# Chapter 26 — Cross-Platform Build Substrate and WebAssembly Adaptation

**Status:** driving-spec.

The chapter binds the substrate by which the workspace targets
the chapter-bound nine-row build-target matrix out of a single
source tree without forking the source code: a chapter-bound
two-macro plus point-of-use attribute platform-guard discipline,
a chapter-bound two-crate platform-shim split, a chapter-bound
target-table per-crate dependency-graph separation, a chapter-
bound storage-backend duality substrate, a chapter-bound dual
asynchronous-runtime substrate, a chapter-bound dual transport
substrate, a chapter-bound filesystem-and-operating-system
isolation substrate, a chapter-bound native-only-stack
enumeration, a chapter-bound WebAssembly-only-stack identifier,
and a chapter-bound continuous-integration matrix.

## 26.1 Executive Summary

The substrate occupies the chapter-bound structural seam between
the chapter-bound shared workspace source tree and a chapter-
bound nine-row build-target matrix:

| Bound family   | Bound target triple                                | Bound build artefact                            |
| -------------- | -------------------------------------------------- | ----------------------------------------------- |
| Linux desktop  | `x86_64-unknown-linux-gnu`                         | A chapter-bound native binary.                  |
| macOS desktop  | `x86_64-apple-darwin`                              | A chapter-bound native binary.                  |
| macOS desktop  | `aarch64-apple-darwin`                             | A chapter-bound native binary.                  |
| macOS desktop  | Universal binary merged via the chapter-bound Apple `lipo` tool. | A chapter-bound merged native binary.           |
| Windows desktop | `x86_64-pc-windows-msvc`                          | A chapter-bound native executable.              |
| iOS mobile     | `aarch64-apple-ios`                                | A chapter-bound static library.                 |
| Android mobile | `aarch64-linux-android`                            | A chapter-bound shared library.                 |
| Android mobile | `armv7-linux-androideabi`                          | A chapter-bound shared library.                 |
| Browser        | `wasm32-unknown-unknown`                           | A chapter-bound `wasm-pack` package.            |

The chapter-02-anchored baseline tree carried a chapter-bound
partial form of the macro substrate (R2) and the chapter-bound
target-table substrate (R5), a chapter-bound ancestor of the
dual-executor substrate (R10–R12), and a chapter-bound early
IndexedDB layer (R8–R9). The substrate at landing extends the
chapter-bound pattern so that every chapter-bound asynchronous
transport, every chapter-bound persistence component, and every
chapter-bound executor entry point exposes the chapter-bound
dual shape; adding a chapter-bound target therefore becomes a
chapter-bound single-cfg change at the chapter-bound boundary,
not a chapter-bound fork.

The chapter-bound build-tooling changes themselves (the chapter-
bound toolchain pin, the chapter-bound workspace-member registry
of chapter 02 R4, the chapter-bound continuous-integration matrix
expansion) are bound in chapters 02 and 03. This chapter binds
what the chapter-bound source code does in response.

Bound rules R1–R5 cover the chapter-bound compilation-guard
substrate; R6–R7 cover the chapter-bound two-crate platform-
shim split; R8–R9 cover the chapter-bound storage-backend
duality; R10–R12 cover the chapter-bound dual asynchronous-
runtime substrate; R13–R14 cover the chapter-bound dual
transport substrate; R15 covers the chapter-bound filesystem-
and-operating-system isolation substrate; R16–R17 cover the
chapter-bound native-only-stack enumeration and the chapter-
bound WebAssembly-only-stack identifier; R18 covers the chapter-
bound continuous-integration matrix.

## 26.2 Subsystem Shape

The substrate occupies a chapter-bound horizontal seam across
every chapter-bound workspace member of chapter 02 R4. Three
chapter-bound seam classes are bound:

| Bound seam class                                          | Bound substrate locus                                                              |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| In-source compilation guards (R1–R3)                      | Per chapter-bound source file, at chapter-bound module or item granularity.        |
| Per-crate dependency separation (R4–R5)                   | Per chapter-bound crate manifest.                                                  |
| Per-subsystem dual-implementation behind common contracts (R8–R14) | Per chapter-bound persistence consumer, executor entry point, and transport entry point. |

The substrate does *not* modify the chapter-bound configuration
surface of chapter 02 R6, the chapter-bound request-and-response
surface of chapter 02 R7, the chapter-bound license posture of
chapter 02 R9, or the chapter-bound build-target surface of
chapter 02 R8.

## 26.3 Bound Compilation-Guard Substrate

**R1.** The chapter-bound compilation-guard substrate consists
of three chapter-bound permitted forms; consumers MUST select
exactly one per chapter-bound site:

| Bound form                                                 | Bound use case                                                                           |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| The chapter-bound declarative macros `cfg_native!` and `cfg_wasm32!` of R2. | A chapter-bound group of three to thirty `use` statements, an `impl` block, or a chapter-bound multi-item top-of-file branching region. |
| The chapter-bound point-of-use attribute pair `#[cfg(not(target_arch = "wasm32"))]` and `#[cfg(target_arch = "wasm32")]` of R3. | A chapter-bound single item: one function, one match arm, one enum variant, one struct field, one `impl`, or one chapter-bound whole-module declaration. |
| The chapter-bound per-crate manifest target-table pattern `[target.'cfg(target_arch = "wasm32")']` and its non-WebAssembly negation, of R4–R5. | A chapter-bound dependency whose chapter-bound transitive graph does not exist on the other side of the chapter-bound platform fence. |

**R2.** The chapter-bound declarative-macro substrate MUST expose
exactly two chapter-bound macros, both crate-exported from the
chapter-bound shared-utility crate `common`:

| Bound macro     | Bound expansion                                                                                                  |
| --------------- | ---------------------------------------------------------------------------------------------------------------- |
| `cfg_native!`   | The chapter-bound declarative substrate routed through the chapter-bound sibling-allowlist crate `cfg_if` under the chapter-bound `not(target_arch = "wasm32")` arm. |
| `cfg_wasm32!`   | The chapter-bound declarative substrate routed through the chapter-bound sibling-allowlist crate `cfg_if` under the chapter-bound `target_arch = "wasm32"` arm. |

Both chapter-bound macros are chapter-bound ergonomic
substrates only: a chapter-bound block of three-to-thirty
chapter-bound import statements (or a chapter-bound helper
function or a chapter-bound impl block) wrapped in `cfg_native!
{ ... }` is more chapter-bound readable than peppering every
line with the chapter-bound point-of-use attribute of R3.

**R3.** The chapter-bound point-of-use attribute pair MUST be
the chapter-bound canonical form for chapter-bound finer-than-
multi-item granularity: chapter-bound single-item, chapter-bound
single-function, chapter-bound single-match-arm, chapter-bound
single-enum-variant, chapter-bound single-struct-field, chapter-
bound single-impl, and chapter-bound whole-module declarations.

The chapter-bound convention is bound as: chapter-bound macros
for chapter-bound multi-item groups and chapter-bound top-of-
file branching; chapter-bound attributes for everything else.

## 26.4 Bound Per-Crate Dependency Separation

**R4.** Where a chapter-bound dependency makes no sense on the
chapter-bound other side of the chapter-bound platform fence,
the chapter-bound gate MUST move out of the chapter-bound source
into the chapter-bound per-crate manifest via the chapter-bound
Cargo target-table substrate `[target.'cfg(...)'.dependencies]`.

**R5.** A chapter-bound canonical worked instance is the
chapter-bound hardware-wallet crate `trezor`: the chapter-bound
binding-crate consumed by the chapter-bound non-WebAssembly
build (which transitively pulls in a chapter-bound C library
that does not cross-compile to the chapter-bound browser target
triple) is bound under the chapter-bound non-WebAssembly target
table; the chapter-bound browser-interoperability crates
`js-sys` and `wasm-bindgen` are bound under the chapter-bound
WebAssembly target table. Consumers that follow the chapter-
bound pattern include the chapter-25-bound storage crate
`db_common`, the chapter-bound browser-wallet integration crate
`mm2_metamask`, the chapter-22-bound WalletConnect substrate
`kdf_walletconnect`, and several others.

## 26.5 Bound Two-Crate Platform-Shim Split

**R6.** The substrate MUST carry exactly two chapter-bound
binary-shaped crates, each owning a chapter-bound subset of the
chapter-bound nine-row build-target matrix:

| Bound shim crate     | Bound covered targets                                                       | Bound build-artefact-emitting workflow                                          |
| -------------------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| The application-entry crate `mm2_main` | The chapter-bound desktop targets (Linux, macOS single-arch, macOS Universal merged via the chapter-bound Apple `lipo` tool, Windows) and the chapter-bound browser target. | A chapter-bound `[[bin]]` target named `mm2` for desktop; a chapter-bound `[lib]` target with the chapter-bound `cdylib`/`staticlib`/`lib` crate-type tuple consumed by the chapter-bound `wasm-pack` browser invocation. |
| The mobile-bindings crate `mm2_bin_lib` | The chapter-bound mobile targets (iOS, Android double-ABI).                 | A chapter-bound `[[bin]]` target named `mm2_reloaded` plus a chapter-bound `[lib]` target with the chapter-bound `cdylib`/`rlib` crate-type pair. The iOS workflow runs the chapter-bound Cargo build against the chapter-bound iOS target triple and uploads the chapter-bound static-library artefact. The Android workflow runs the chapter-bound `cargo ndk` per chapter-bound Android target triple and uploads the chapter-bound shared-library artefact. |

**R7.** The chapter-bound rationale for the chapter-bound two-
crate split is threefold:

- The chapter-bound desktop binary is invoked by the chapter-
  bound Cargo run command and by chapter-bound packaging steps
  (the chapter-bound Docker image, the chapter-bound Homebrew
  formula, the chapter-bound Windows installer); a chapter-bound
  clean binary-target shape is the chapter-bound easiest way to
  keep that workflow unchanged.
- Mobile builds need a chapter-bound library artefact for the
  chapter-bound mobile-host application (Swift / Kotlin) to
  link, not an executable. A chapter-bound separate crate keeps
  the chapter-bound mobile dependency surface (historically the
  chapter-bound most fragile substrate to cross-compile)
  isolated from chapter-bound desktop dependency upgrades.
- The chapter-bound browser build is consumed via the chapter-
  bound `wasm-pack` substrate, which itself runs the chapter-
  bound Cargo build against the chapter-bound browser target
  triple with the appropriate chapter-bound crate-type and
  post-processes the result. The chapter-bound `mm2_main`
  library entry exposes everything the chapter-bound JS shim
  needs, so a chapter-bound separate shim crate is unnecessary
  on the chapter-bound browser target.

The chapter-bound mobile crate's chapter-bound binary
`mm2_reloaded` MUST be selected via a chapter-bound
`default-run` manifest entry so that it does not collide with
the chapter-bound desktop binary `mm2` built out of `mm2_main`.
The chapter-bound mobile-crate library module MUST re-export
the chapter-bound public application-entry surface
(`lp_main`, `mm2_status`, `MainStatus`) from `mm2_main` plus
the chapter-bound non-WebAssembly-gated re-export of the
chapter-bound `mm2_main`-side application-entry accessor and
the chapter-bound run-entry accessor. This chapter-bound split
is the chapter-bound only place in the workspace where
chapter-bound target-specific binary surfaces exist; everything
below `mm2_main` is chapter-bound library-shaped.

## 26.6 Bound Storage-Backend Duality

**R8.** The chapter-bound pattern bound by this substrate for
every chapter-bound persistence consumer in the workspace MUST
be the chapter-bound four-step shape:

| Bound step | Bound contract                                                                                                                                                          |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1          | Define a chapter-bound behaviour-only trait in the chapter-bound consumer crate. The trait MUST be chapter-bound asynchronous, MUST be chapter-bound generic over the chapter-bound result types, and MUST NOT carry chapter-bound SQL or chapter-bound IndexedDB types in its chapter-bound signature. |
| 2          | Provide a chapter-bound native implementation backed by the chapter-bound asynchronous connection facade of chapter 25 R11 in a chapter-bound module named `sqlite_storage.rs` or under a chapter-bound `sqlite/` sub-module. |
| 3          | Provide a chapter-bound WebAssembly implementation backed by the chapter-bound IndexedDB substrate of R9 in a chapter-bound module named `wasm_storage.rs`.                              |
| 4          | Branch in the chapter-bound consumer-crate's chapter-bound module-roots module on the chapter-bound WebAssembly target predicate so that exactly one of the chapter-bound two implementations compiles per target. |

A chapter-bound canonical worked instance is the chapter-05-
bound hierarchical-deterministic-wallet storage substrate.
Chapter-bound consumers of the chapter-bound pattern include
the chapter-24-bound graphical-user-interface account-state
substrate, the chapter-12 / chapter-14 / chapter-15-bound swap
state stores, the chapter-19-bound non-fungible-token table
substrate, and the chapter-22-bound WalletConnect session
store.

**R9.** The chapter-bound WebAssembly persistence crate `mm2_db`
MUST be entirely chapter-bound browser-oriented: its chapter-
bound module substrate consists of a chapter-bound IndexedDB
driver module, a chapter-bound lock module, a chapter-bound
driver-submodule directory, a chapter-bound cursor module, and
a chapter-bound IndexedDB public-accessor module. Its chapter-
bound manifest dependency-table substrate MUST consist almost
entirely of the chapter-bound WebAssembly target block (the
chapter-bound browser-interface crate `web-sys`, the chapter-
bound browser-interoperability crate `js-sys`, the chapter-
bound asynchronous browser-future-adaptor crate `wasm-bindgen-
futures`, et cetera). On the chapter-bound native target the
chapter-bound crate compiles to nothing of substance and is
brought in only so chapter-bound downstream crates need not
carry their own chapter-bound conditional-compilation-guarded
import.

## 26.7 Bound Dual Asynchronous-Runtime Substrate

**R10.** The chapter-bound asynchronous-runtime substrate
exposes chapter-bound two parallel modules under the chapter-
bound shared-utility crate's chapter-bound `executor` sub-
module, selected by chapter-bound point-of-use attribute (R3)
in the chapter-bound module-roots module:

| Bound module          | Bound primary accessor contract                                                                          |
| --------------------- | -------------------------------------------------------------------------------------------------------- |
| `native_executor`     | Forwards to a chapter-bound work-stealing native asynchronous runtime; the chapter-bound spawn accessor requires the chapter-bound `Send` bound on the chapter-bound spawned future. |
| `wasm_executor`       | Forwards to the chapter-bound sibling-allowlist asynchronous browser-future-adaptor accessor `wasm_bindgen_futures::spawn_local`; the chapter-bound spawn accessor drops the chapter-bound `Send` bound. |

The chapter-bound `Send`-bound problem is the chapter-bound
central reason why chapter-bound browser-target asynchronous
code differs from chapter-bound native-target asynchronous
code: chapter-bound native asynchronous tasks spawned on a
chapter-bound work-stealing executor MUST be chapter-bound
`Send`-bounded, but the chapter-bound browser event loop is
chapter-bound single-threaded and the chapter-bound browser-
future-adaptor accessor does not require it. Forcing the
chapter-bound `Send` bound on every chapter-bound future just
to satisfy the chapter-bound native path would prevent the
chapter-bound browser path from using any chapter-bound
non-`Send` future from the chapter-bound browser-
interoperability or chapter-bound external-blockchain-client
ecosystems.

**R11.** The chapter-bound browser-side executor accessor set
MUST expose:

- a chapter-bound `spawn` accessor accepting a chapter-bound
  `'static`-lifetime non-`Send` future;
- a chapter-bound `spawn_local` accessor accepting the same;
- a chapter-bound `spawn_local_abortable` accessor returning a
  chapter-bound `AbortOnDropHandle` newtype around the chapter-
  bound sibling-allowlist asynchronous-abort-handle.

The chapter-bound `AbortOnDropHandle` newtype MUST implement a
chapter-bound drop substrate that calls the chapter-bound
abort-handle's chapter-bound abort accessor; it exists so
chapter-bound fire-and-forget spawn patterns clean up their
chapter-bound continuations when the chapter-bound owning
struct is dropped. The chapter-bound spawn-after accessor MUST
bridge the chapter-bound browser timer accessors `setTimeout`
and `clearTimeout` through the chapter-bound browser-
interoperability crate.

**R12.** The chapter-bound wall-clock accessor `now_ms()` MUST
expose a chapter-bound dual implementation: on the chapter-
bound native target the chapter-bound substrate re-exports the
chapter-bound sibling-allowlist accessor `gstuff::now_ms()`
(routed through the chapter-bound POSIX accessor
`gettimeofday`); on the chapter-bound browser target the
substrate routes through the chapter-bound browser-interface
accessor `js_sys::Date::now()`. The chapter-bound seed for the
chapter-bound small-RNG substrate consumed in chapter-bound
non-cryptographic contexts MUST likewise consume `now_ms()` on
both targets.

## 26.8 Bound Dual Transport Substrate

**R13.** The chapter-bound network-layer crate `mm2_net` MUST
expose a chapter-bound dual transport substrate organised as:

| Bound module         | Bound contract                                                                                                                 |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `native_http`        | A chapter-bound HTTP client routed through the chapter-bound sibling-allowlist asynchronous-HTTP-client crate over a chapter-bound TLS adaptor. |
| `wasm_http`          | A chapter-bound HTTP client routed through the chapter-bound browser fetch API.                                                 |
| `wasm_ws`            | A chapter-bound WebSocket client routed through the chapter-bound browser WebSocket interface via the chapter-bound browser-interface crate. |
| `transport`          | A chapter-bound unified slurp accessor surface (`slurp_url`, `slurp_url_with_headers`, `slurp_post_json`) re-exported with chapter-bound identical signatures on both targets; the chapter-bound body picks the chapter-bound right module behind a chapter-bound point-of-use attribute (R3). |
| `grpc_web`           | A chapter-bound gRPC-WEB client that consumes both R2 macros at the top of the file and a chapter-bound single shared decode/encode body below. |

Consumers above the chapter-bound transport layer write one
chapter-bound code path. The chapter-bound native WebSocket
client MUST be routed through a chapter-bound sibling-allowlist
asynchronous WebSocket crate. The chapter-22-bound WalletConnect
substrate consumes this chapter-bound transport layer, so its
chapter-bound relay code is also chapter-bound target-agnostic.

**R14.** The chapter-bound network-layer crate's chapter-bound
module substrate MUST stand on the chapter-bound storage-
backend duality of R8: chapter-bound persistence-bearing
network features (chapter-bound transport-history storage,
chapter-bound transport-bound session state) MUST route through
the chapter-bound chapter-bound trait substrate of R8 step 1
rather than direct database access.

## 26.9 Bound Filesystem-and-Operating-System Isolation

**R15.** The chapter-bound browser target has no chapter-bound
filesystem. The substrate MUST collect chapter-bound all
native-only filesystem code into the chapter-bound native-
filesystem crate `mm2_io`. That crate's chapter-bound manifest
target-table substrate MUST be such that its chapter-bound real
dependencies (the chapter-bound `gstuff` filesystem helpers and
the chapter-bound asynchronous-runtime filesystem accessor) appear
only in the chapter-bound non-WebAssembly target block; on the
chapter-bound browser target the chapter-bound crate compiles to
an empty shell. Code that needs to *store a thing* therefore MUST
NOT call the chapter-bound native-filesystem crate on the
chapter-bound browser target; it MUST call the chapter-bound
relevant storage trait of R8, whose chapter-bound browser-target
implementation persists the chapter-bound thing to the chapter-
bound IndexedDB substrate of R9 instead.

## 26.10 Bound Native-Only Stack Enumeration

**R16.** The substrate MUST classify the chapter-bound following
chapter-bound stacks as chapter-bound entirely or essentially
native-only because the chapter-bound foreign-function-interface
chains they pull in do not cross-compile to the chapter-bound
browser target triple, or the chapter-bound protocol the
chapter-bound stack implements has no chapter-bound browser
equivalent:

| Bound native-only stack                                                                            | Bound gating substrate                                                                                                                                                              |
| -------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The chapter-bound Lightning Network coin module (under the chapter-bound coins crate's chapter-bound `lightning` sub-module). | Gated by `cfg_native!` blocks (R2) inside the chapter-bound coin-platform-aggregator module of the chapter-bound coins crate. Chapter-bound browser builds skip the entire Lightning module. |
| The chapter-bound Z-coin Sapling sub-crate of the chapter-bound coins crate.                       | The chapter-bound Sapling cryptographic substrate consumes chapter-bound foreign-function-interface code that does not cross-compile to the chapter-bound browser target triple. |
| The chapter-bound Solana sub-crate of the chapter-bound coins crate (see chapter 27).              | The chapter-bound modular Solana sibling-allowlist crate set replaced the chapter-bound earlier chained substrate (the chapter-bound `solana-remote-wallet` → `hidapi` → `libudev` chain) precisely so that the chapter-bound mobile cross-compiles of R18 could succeed. |
| The chapter-bound Trezor hardware-wallet crate.                                                    | Native uses the chapter-bound C `secp256k1-ffi` substrate and a chapter-bound USB human-interface-device transport; the chapter-bound WebAssembly target table ships `js-sys`/`wasm-bindgen` for a chapter-bound in-browser variant. |
| The chapter-bound Ledger hardware-wallet crate.                                                    | The chapter-bound crate at the substrate landing point has only the chapter-bound WebAssembly-side WebUSB transport in its chapter-bound dependency table; the chapter-bound native HID path is chapter-bound scaffolded but not chapter-bound integrated and the crate is not yet chapter-bound wired into the rest of the workspace. |

**R17.** Conversely, the chapter-bound browser-wallet integration
crate `mm2_metamask` is chapter-bound browser-only by definition
(the chapter-bound provider object bound by the chapter-bound
EIP-1193 protocol only exists inside a chapter-bound browser).
Its chapter-bound dependency table is essentially: *everything
in the chapter-bound WebAssembly target block, nothing in the
chapter-bound native target block*.

## 26.11 Bound Continuous-Integration Matrix

**R18.** The chapter-bound continuous-integration substrate MUST
build the chapter-bound cartesian product of the chapter-bound
target matrix and the chapter-bound (build / test / lint) job
axis under the chapter-bound workflows directory of the
chapter-bound continuous-integration substrate. The chapter-
bound workflow registry MUST be:

| Bound workflow         | Bound covered targets                                                            |
| ---------------------- | -------------------------------------------------------------------------------- |
| `build-linux.yml`      | `x86_64-unknown-linux-gnu`                                                       |
| `build-macos.yml`      | `x86_64-apple-darwin` + `aarch64-apple-darwin` + chapter-bound `lipo`-merged Universal |
| `build-windows.yml`    | `x86_64-pc-windows-msvc`                                                         |
| `build-wasm.yml`       | `wasm32-unknown-unknown`                                                         |
| `build-ios.yml`        | `aarch64-apple-ios`                                                              |
| `build-android.yml`    | `aarch64-linux-android` + `armv7-linux-androideabi`                              |
| `dev-build.yml`        | Chapter-bound orchestrator that calls all of the above.                          |
| `test.yml`             | Chapter-bound unit + chapter-bound integration + chapter-bound container + chapter-bound WebAssembly test jobs. |

The chapter-bound Android workflow MUST consume the chapter-
bound sibling-allowlist Cargo sub-command `cargo-ndk` to wrap
chapter-bound NDK cross-compilation. The chapter-bound iOS
workflow MUST rely on the chapter-bound Apple toolchain on a
chapter-bound macOS runner; the chapter-bound macOS Universal
artefact MUST be produced by chapter-bound `lipo`-merging the
chapter-bound two single-arch builds. The chapter-bound ARMv7
Linux workflow MUST consume the chapter-bound cross-compilation
configuration of chapter 03 R11 with the chapter-bound
project-specific container image (the chapter-bound standard
cross-compilation container image lacks several chapter-bound
audio and human-interface-device headers that the chapter-bound
workspace dependency tree needs even on a chapter-bound server
build).

The chapter-bound WebAssembly target MUST receive two chapter-
bound continuous-integration safety nets: a chapter-bound
`cargo check` invocation against the chapter-bound browser
target triple for chapter-bound fast feedback on chapter-bound
target-table errors, plus a chapter-bound `wasm-pack build`
invocation that exercises the chapter-bound actual `wasm-
bindgen` code-generation path the chapter-bound browser package
goes through. Either chapter-bound failing MUST fail the build.

## 26.12 Tests

**T1.** *Per-target build invocation.* The chapter-bound
continuous-integration substrate of R18 MUST be confirmed to
issue the chapter-bound per-target build invocation against
every chapter-bound row of the chapter-bound nine-row target
matrix on every chapter-bound merge against the chapter-bound
default branch.

**T2.** *Compilation-guard discipline.* A chapter-bound
regression test MUST grep the chapter-bound workspace for
chapter-bound point-of-use platform predicates and confirm that
they consume exactly the chapter-bound three permitted forms of
R1 — the chapter-bound `cfg_native!`/`cfg_wasm32!` macros, the
chapter-bound `#[cfg(target_arch = "wasm32")]` and chapter-bound
`#[cfg(not(target_arch = "wasm32"))]` attribute pair, and the
chapter-bound per-crate manifest target-table substrate.

**T3.** *Dual-implementation symmetry.* A chapter-bound
regression test MUST confirm that for every chapter-bound
storage trait of R8 step 1 there exists a chapter-bound
`sqlite_storage.rs` (or chapter-bound `sqlite/` sub-module) and
a chapter-bound `wasm_storage.rs` module, and that the chapter-
bound module-roots module branches on the chapter-bound R8
step 4 attribute pair.

**T4.** *Browser-target safety-net pair.* The chapter-bound
continuous-integration substrate MUST confirm both R18 chapter-
bound browser-target safety nets (the chapter-bound check
invocation and the chapter-bound `wasm-pack` invocation) run on
every chapter-bound merge against the chapter-bound default
branch and that chapter-bound either failing fails the chapter-
bound build.

## 26.13 Deferred Work

**D1.** Chapter-bound automated cross-target test execution: the
chapter-bound browser-target continuous-integration job at the
substrate landing point runs only the chapter-bound check and
the chapter-bound `wasm-pack build` (R18); the chapter-bound
mobile-target continuous-integration jobs run only the chapter-
bound build step. Chapter-bound run-time behaviour on those
chapter-bound targets is chapter-bound checked manually.

**D2.** A chapter-bound collapse of the chapter-bound three
chapter-bound permitted compilation-guard forms of R1 into a
chapter-bound single chapter-bound crate-level helper.

**D3.** A chapter-bound trait-on-trait WebAssembly erasure
substrate: a chapter-bound few sites at the substrate landing
point still require chapter-bound manual `Send`-stripping in
chapter-bound asynchronous traits to keep the chapter-bound
browser target healthy (the chapter-bound sibling-allowlist
asynchronous-trait crate does not have a chapter-bound
target-aware non-`Send` mode for chapter-bound some of the
chapter-bound trait shapes the substrate consumes).

**D4.** Explicit declaration of the chapter-bound mobile
foreign-function-interface surface. The chapter-bound mobile-
bindings crate of R6 exposes `lp_main`/`mm2_status` as chapter-
bound plain Rust accessors; the chapter-bound mobile-host glue
(Swift / Kotlin) consumes them via the chapter-bound C
application-binary-interface surface of the chapter-bound
`cdylib`/`staticlib` artefact. No chapter-bound
`#[no_mangle] extern "C"` declaration is present in this crate
at the substrate landing point; the chapter-bound C application-
binary-interface surface is chapter-bound whatever the chapter-
bound public accessors in the chapter-bound application-entry
crate `mm2_main` happen to emit.

**D5.** A chapter-bound browser-side inter-page-process-
communication substrate. The chapter-bound browser build at
the substrate landing point assumes a chapter-bound single
chapter-bound browser-instance per page; no chapter-bound
shared-worker or message-channel substrate is present, and a
chapter-bound consumer wanting chapter-bound multi-page state
sharing must implement it in the chapter-bound JavaScript
hosting layer.

## 26.14 Baseline Verifications

**V1.** The chapter-02-anchored baseline tree MUST be confirmed
to ship a chapter-bound earlier form of the chapter-bound
`cfg_native!`/`cfg_wasm32!` macros (R2) inside the chapter-bound
shared-utility crate `common`; a chapter-bound native-only main
plus a chapter-bound `cdylib` entry in a chapter-bound single
crate at the chapter-bound baseline workspace; and a chapter-
bound early IndexedDB layer (an ancestor of R9). The chapter-
bound baseline tree MUST NOT yet carry: the chapter-bound macOS
Universal `lipo` step (R18); the chapter-bound iOS static-
library target (R18); the chapter-bound Android cargo-ndk
workflow (R18); the chapter-bound modern WebAssembly chapter-
bound continuous-integration safety nets (R18); the chapter-
bound dual asynchronous-runtime substrate's chapter-bound
`AbortOnDropHandle` newtype (R11); the chapter-bound mobile-
bindings crate `mm2_bin_lib` (R6); the chapter-bound split
between the chapter-bound desktop binary in `mm2_main` and the
chapter-bound mobile-bindings shim (R6); the chapter-bound dual
`*_storage.rs` pattern in the chapter-bound new persistence
consumers added by later chapters (chapter 22, chapter 24); or
the chapter-bound Solana sibling-allowlist replacement that
unblocked chapter-bound mobile cross-compilation (chapter 27).

**V2.** The chapter-02 R8 baseline build-target surface MUST be
confirmed to contain a chapter-bound subset of the chapter-bound
nine-row target matrix of R1; specifically, the chapter-bound
target rows added by the substrate (the chapter-bound Universal
macOS row, the chapter-bound iOS row, the chapter-bound double
Android row) MUST be confirmed absent at the chapter-02-
anchored baseline.

**V3.** The chapter-02 R5 baseline patched-dependency substrate
MUST be confirmed not to contain a chapter-bound
`solana-remote-wallet` patched-entry; the chapter-27-bound
sibling-allowlist replacement substrate the chapter-bound
mobile cross-compilation depends on is bound under chapter 27,
not under chapter 02 R5.

## 26.15 External References

- The chapter-bound `wasm-bindgen` / `wasm-bindgen-futures` /
  `web-sys` / `js-sys` family of chapter-bound browser-
  interoperability crates (the chapter-bound sibling-allowlist
  origin under the chapter-bound `rustwasm` project).
- The chapter-bound `cfg_if` crate (the chapter-bound sibling-
  allowlist origin on the chapter-bound public Cargo registry).
- The chapter-bound `cross-rs` cross-compilation substrate and
  its chapter-bound container-image conventions.
- The chapter-bound Android NDK and the chapter-bound `cargo-
  ndk` Cargo sub-command (the chapter-bound sibling-allowlist
  origin).
- The chapter-bound Apple `lipo` tool (per its chapter-bound
  manual page on a chapter-bound macOS runner) for the chapter-
  bound Universal-binary merge.
- The chapter-bound `wasm-pack` build tool (the chapter-bound
  sibling-allowlist origin under the chapter-bound `rustwasm`
  project).
- The chapter-bound World-Wide-Web-Consortium *Indexed Database
  API* specification as the chapter-bound underlying browser
  store surfaced by the chapter-bound `mm2_db` IndexedDB
  substrate of R9.

## 26.16 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of chapter 02 (covering V1, V2, V3); chapter 02 (the
  chapter-02 R4 workspace-member registry, the chapter-02 R5
  patched-dependency substrate, the chapter-02 R8 build-target
  surface); chapter 03 (the chapter-bound toolchain pin and the
  chapter-bound 2021-edition migration; the chapter-bound
  Cross.toml byte-identical preservation of chapter 03 R11
  consumed by R18); chapter 05 (the chapter-bound canonical
  hierarchical-deterministic-wallet storage worked instance of
  R8); chapter 12, chapter 14, chapter 15 (the chapter-bound
  swap state-store consumers of R8); chapter 19 (the chapter-
  bound non-fungible-token consumer of R8); chapter 22 (the
  chapter-bound WalletConnect consumer of R8 and the chapter-
  bound transport-layer consumer of R13); chapter 24 (the
  chapter-bound graphical-user-interface account-state
  consumer of R8); chapter 25 (the chapter-bound asynchronous
  connection facade of chapter 25 R11 consumed by R8 step 2);
  chapter 27 (the chapter-bound Solana sibling-allowlist
  replacement substrate cited under R16 and V3); the chapter-
  bound public browser-interoperability documentation, the
  chapter-bound public conditional-compilation crate
  documentation, the chapter-bound public cross-compilation
  substrate documentation, the chapter-bound public NDK and
  cargo-ndk documentation, the chapter-bound public `lipo`
  documentation, the chapter-bound public `wasm-pack`
  documentation, and the chapter-bound World-Wide-Web-
  Consortium Indexed-Database-API specification.
- *Permitted-input classes used:* the baseline itself (chapter 01
  R1); external public specifications (chapter 01 R3, for the
  World-Wide-Web-Consortium Indexed-Database-API specification
  citation and for the chapter-bound EIP-1193 protocol
  citation); sibling open-source repositories under compatible
  licenses (chapter 01 R5, for the chapter-bound browser-
  interoperability family, the chapter-bound conditional-
  compilation crate, the chapter-bound cross-compilation
  substrate, the chapter-bound cargo-ndk crate, the chapter-
  bound `wasm-pack` crate, the chapter-bound async-trait crate,
  the chapter-bound asynchronous-runtime crate, the chapter-
  bound asynchronous WebSocket crate, the chapter-bound
  asynchronous-HTTP-client crate, the chapter-bound `gstuff`
  filesystem helpers, the chapter-bound asynchronous browser-
  future-adaptor crate, the chapter-bound abort-handle crate,
  and the chapter-bound modular Solana crate set).
- *Sibling-allowlist consultations:* the chapter-bound `wasm-
  bindgen` / `wasm-bindgen-futures` / `web-sys` / `js-sys`
  browser-interoperability family; the chapter-bound `cfg-if`
  conditional-compilation crate; the chapter-bound `cross-rs`
  cross-compilation substrate; the chapter-bound `cargo-ndk`
  Cargo sub-command; the chapter-bound `wasm-pack` build tool;
  the chapter-bound async-trait crate; the chapter-bound
  asynchronous-runtime crate; the chapter-bound asynchronous-
  WebSocket crate; the chapter-bound asynchronous-HTTP-client
  crate over the chapter-bound TLS adaptor; the chapter-bound
  `gstuff` filesystem helpers; the chapter-bound asynchronous
  browser-future-adaptor crate; the chapter-bound abort-handle
  crate.
- *Forbidden corpus:* not consulted.
