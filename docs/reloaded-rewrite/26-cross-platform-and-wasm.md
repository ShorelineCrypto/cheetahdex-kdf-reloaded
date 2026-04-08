# Chapter 26 -- Cross-Platform Build & WASM Adaptation

> **Chapter type:** document existing. No IMPL marker.

## 26.0 Executive summary

The reloaded workspace targets seven first-class platforms
out of one source tree:

| Family    | Triple                          | Artefact                         |
|-----------|---------------------------------|----------------------------------|
| Linux     | `x86_64-unknown-linux-gnu`      | `mm2` binary                     |
| macOS     | `x86_64-apple-darwin`           | `mm2` binary                     |
| macOS     | `aarch64-apple-darwin`          | `mm2` binary                     |
| macOS     | universal (`lipo`-merged)       | `mm2` binary                     |
| Windows   | `x86_64-pc-windows-msvc`        | `mm2.exe` binary                 |
| iOS       | `aarch64-apple-ios`             | `libmm2_bin_lib.a` static lib    |
| Android   | `aarch64-linux-android`         | `libmm2_bin_lib.so` shared lib   |
| Android   | `armv7-linux-androideabi`       | `libmm2_bin_lib.so` shared lib   |
| Browser   | `wasm32-unknown-unknown`        | `wasm-pack` package              |

This chapter documents the **structural pattern** the post-
baseline tree uses to share one workspace across all of
those targets without forking the code. The build-tooling
changes themselves (toolchain pin, Cargo workspace member
lists, CI matrix expansion) are covered in
[Chapter 3](03-toolchain-modernization.md); this chapter
describes what the source code does in response, namely:

1. A pair of in-workspace macros (`cfg_native!` and
   `cfg_wasm32!`) plus the standard
   `#[cfg(target_arch = "wasm32")]` attribute used at
   point-of-use for everything finer-grained than a whole
   module.
2. A one-crate platform shim
   ([`mm2src/mm2_bin_lib/`](../../mm2src/mm2_bin_lib/))
   that fans out into a native binary, a `cdylib`/`staticlib`
   for mobile FFI, and a WASM JS shim.
3. Per-crate target tables in `Cargo.toml`, so that
   native-only and WASM-only dependency graphs never collide.
4. Behaviour-equivalent storage backends
   (SQLite on native, IndexedDB on WASM) and
   behaviour-equivalent transports (hyper on native, browser
   `fetch`/`WebSocket` on WASM) sitting behind common traits
   so that consumer crates above the boundary stay
   target-agnostic.
5. A WASM-aware executor and time module in
   [`mm2src/common/executor/`](../../mm2src/common/executor/)
   that drops the `Send` bound on WASM (single-threaded JS
   event loop) and forwards to
   [`wasm_bindgen_futures`](https://crates.io/crates/wasm-bindgen-futures).

The baseline carried a partial form of (1) and (3), an
ancestor of (5), and an early IndexedDB layer. The reloaded
tree extends the pattern to every new feature added in this
document set: every async transport, every persistence
component, every executor entry point exposes the same dual
shape so that adding a target is a one-`cfg` change at the
boundary, not a fork.

## 26.1 Compilation guards

### 26.1.1 Two macros for the common case

The lowest layer is two declarative macros in
[`mm2src/common/common.rs`](../../mm2src/common/common.rs):

```rust
#[macro_export]
macro_rules! cfg_wasm32 {
    ($($tokens:tt)*) => {
        cfg_if::cfg_if! {
            if #[cfg(target_arch = "wasm32")] { $($tokens)* }
        }
    };
}

#[macro_export]
macro_rules! cfg_native {
    ($($tokens:tt)*) => {
        cfg_if::cfg_if! {
            if #[cfg(not(target_arch = "wasm32"))] { $($tokens)* }
        }
    };
}
```

Both macros expand to a `cfg_if::cfg_if!` block. Their
purpose is purely ergonomic: in source files that need to
gate a block of three to thirty `use` statements (or a
helper function, or an `impl` block), wrapping the block
in `cfg_native! { ... }` is more readable than peppering
every line with `#[cfg(not(target_arch = "wasm32"))]`.

A typical use, from
[`mm2src/mm2_net/src/grpc_web.rs`](../../mm2src/mm2_net/src/grpc_web.rs):

```rust
use common::{cfg_native, cfg_wasm32};

cfg_native! {
    use crate::transport::slurp_req;
    use http::header::{ACCEPT, CONTENT_TYPE};
}

cfg_wasm32! {
    use crate::wasm_http::FetchRequest;
}
```

Both branches are present in the source; one is selected by
the compiler.

### 26.1.2 Attribute guards everywhere else

For finer granularity (one item, one function, one match
arm, one enum variant, one struct field, one `impl`), the
canonical pattern is the plain attribute form:

```rust
#[cfg(not(target_arch = "wasm32"))]   // native only
#[cfg(target_arch = "wasm32")]        // WASM only
```

This is the form used inside
[`mm2src/db_common/`](../../mm2src/db_common/) (whole
modules native-gated, see
[Chapter 25](25-sql-query-builder.md)),
inside
[`mm2src/mm2_gui_storage/`](../../mm2src/mm2_gui_storage/)
(separate `sqlite_storage.rs` vs `wasm_storage.rs` modules,
see [Chapter 24](24-gui-account-state.md)), and at
function-level granularity inside
[`mm2src/coins/`](../../mm2src/coins/).

The convention is: macros for groups of imports and
top-of-file branching, attributes for everything else.

### 26.1.3 Target tables in `Cargo.toml`

Where a dependency makes no sense on the other side of the
fence, the gate moves out of the source and into
`Cargo.toml` via Cargo's `[target.'cfg(...)']` tables. The
pattern, from
[`mm2src/trezor/Cargo.toml`](../../mm2src/trezor/Cargo.toml):

```toml
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
bip32 = { version = "0.2.2", default-features = false,
          features = ["alloc", "secp256k1-ffi"] }

[target.'cfg(target_arch = "wasm32")'.dependencies]
js-sys      = { version = "0.3.27" }
wasm-bindgen = { version = "0.2.50" }
```

This keeps `bip32`+`secp256k1-ffi` out of the WASM build
(it pulls in a C library that does not cross-compile to
`wasm32-unknown-unknown`) and keeps `js-sys`/`wasm-bindgen`
out of the native build (they are no-ops there).

`db_common`, `mm2_metamask`, `kdf_walletconnect`, and several
others use the same shape.

## 26.2 The platform shim crates

There are two crates with binary-shaped surfaces, not one,
and each owns a subset of the target matrix.

### 26.2.1 mm2_main: desktop and WASM

[`mm2src/mm2_main/`](../../mm2src/mm2_main/) carries the
actual application logic. Its `Cargo.toml` declares both a
binary target and a library target:

```toml
[[bin]]
name = "mm2"
path = "src/mm2_bin.rs"

[lib]
name = "mm2"
path = "src/mm2_lib.rs"
crate-type = ["lib", "cdylib", "staticlib"]
```

The desktop builds (Linux, macOS, Windows -- including the
universal macOS artefact built by `lipo`-merging the two
single-arch outputs) compile this `[[bin]]` directly with
`cargo build --bin mm2`. The WASM build calls
`wasm-pack build mm2src/mm2_main --target web`, which
consumes the `cdylib` crate-type from the same `[lib]`.

### 26.2.2 mm2_bin_lib: mobile bindings

[`mm2src/mm2_bin_lib/`](../../mm2src/mm2_bin_lib/) is a
thin wrapper used only by the mobile workflows:

```
mm2_bin_lib/
|-- Cargo.toml      [[bin]] name = "mm2_reloaded"
|                   [lib]   crate-type = ["cdylib", "rlib"]
`-- src/
    |-- lib.rs      Re-exports from mm2_main
    `-- main.rs     Native pass-through to mm2::mm2_main()
```

`lib.rs` is essentially:

```rust
pub use mm2::lp_main;
pub use mm2::mm2_status;
pub use mm2::MainStatus;

#[cfg(not(target_arch = "wasm32"))]
pub use mm2::{mm2_main, run_lp_main};
```

The iOS workflow runs `cargo build -p mm2_bin_lib --target
aarch64-apple-ios` and uploads the resulting
`libmm2_bin_lib.a`. The Android workflow runs `cargo ndk
--target aarch64-linux-android -- build -p mm2_bin_lib`
(once per ABI) and uploads `libmm2_bin_lib.so`.

`main.rs` is two lines of body:

```rust
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    { mm2::mm2_main() }
}
```

The binary is named `mm2_reloaded` (via
`default-run = "mm2_reloaded"`) so that it does not collide
with the desktop `mm2` binary built out of `mm2_main`.

### 26.2.3 Why two shims

The split exists because the desktop and mobile targets
want different things from the same code:

- The desktop binary is invoked by `cargo run` and by
  packaging steps (Docker, Homebrew, MSI), and a clean
  `[[bin]]` target is the easiest way to keep that
  workflow unchanged.
- Mobile builds need a library artefact (`.a` / `.so`) for
  the host Swift/Kotlin app to link, not an executable. A
  separate crate keeps the mobile dependency surface
  (which has historically been the most fragile thing to
  cross-compile) isolated from desktop dependency upgrades.
- The browser build is consumed via `wasm-pack`, which
  itself runs `cargo build --target wasm32-unknown-unknown`
  with the appropriate crate-type and then post-processes
  the result; the `mm2_main` library entry exposes
  everything the JS shim needs, so a separate `mm2_bin_lib`
  is unnecessary on this target.

This is the only place in the workspace where
target-specific binary surfaces exist; everything below
`mm2_main` is library-shaped.

## 26.3 Storage-backend duality

The pattern used in every persistence consumer is:

1. Define a behaviour-only trait in the consumer crate
   (async, generic over `Result` types, no SQL or
   IndexedDB types in the signature).
2. Provide a native implementation backed by
   [`db_common::AsyncConnection`](../../mm2src/db_common/src/async_sql_conn.rs)
   (see [Chapter 25](25-sql-query-builder.md)) in a file
   named `sqlite_storage.rs` (or under a `sqlite/`
   submodule).
3. Provide a WASM implementation backed by
   [`mm2_db::indexed_db`](../../mm2src/mm2_db/src/indexed_db/)
   in a file named `wasm_storage.rs`.
4. Branch in the consumer's `mod.rs` on
   `#[cfg(target_arch = "wasm32")]` so that only one of the
   two compiles per target.

A canonical worked example is
[`mm2src/coins/hd_wallet_storage/`](../../mm2src/coins/hd_wallet_storage/):

```
hd_wallet_storage/
|-- mod.rs               trait definition + cfg branching
|-- mock_storage.rs      in-memory test double
|-- sqlite_storage.rs    native, on db_common
`-- wasm_storage.rs      WASM, on mm2_db::indexed_db
```

The same pattern is used by `mm2_gui_storage` (see
[Chapter 24 §24.5](24-gui-account-state.md)), by the swap
state stores (Chapters 12, 14, 15), by the NFT module
(Chapter 19), and by the WalletConnect session store
(Chapter 22).

[`mm2_db`](../../mm2src/mm2_db/) is itself entirely WASM-
oriented: its source tree is

```
mm2_db/src/
|-- lib.rs
`-- indexed_db/
    |-- db_driver.rs
    |-- db_lock.rs
    |-- drivers/
    |-- indexed_cursor.rs
    `-- indexed_db.rs
```

and its dependency table consists almost entirely of the
WASM target block (`web-sys`, `js-sys`,
`wasm-bindgen-futures`, etc.). On native it compiles to
nothing of substance and is brought in only so that
downstream crates do not need their own `#[cfg]`-guarded
import of it.

## 26.4 The dual executor

The `Send`-bound problem is the central reason WASM Rust
code looks different from native Rust code: native futures
spawned on a worker-stealing executor must be `Send`, but
the browser event loop is single-threaded and the
ecosystem's `wasm_bindgen_futures::spawn_local` does not
require it. Forcing `Send` on every future just to satisfy
the native path would prevent the WASM path from using any
`!Send` future from the `wasm-bindgen` or `alloy` ecosystem.

The reloaded tree solves this by exposing two parallel
modules in
[`mm2src/common/executor/`](../../mm2src/common/executor/),
selected by `#[cfg]` in `executor/mod.rs`:

```
common/executor/
|-- native_executor.rs    spawn() = tokio task; Send required
`-- wasm_executor.rs      spawn() = spawn_local(); no Send
```

The WASM-side signature drops the `Send` bound on `spawn`,
returns an `AbortOnDropHandle` wrapper around
`futures::future::AbortHandle`, and bridges `setTimeout`
/`clearTimeout` through `wasm_bindgen` for `spawn_after`.
The relevant excerpt (paraphrased structurally):

```rust
pub fn spawn(future: impl Future<Output = ()> + 'static) {
    spawn_local(future)
}

pub fn spawn_local(future: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(future)
}

pub fn spawn_local_abortable(
    future: impl Future<Output = ()> + 'static,
) -> AbortOnDropHandle {
    let (abortable_fut, abort_handle) = abortable(future);
    spawn_local(abortable_fut.then(|_| futures::future::ready(())));
    AbortOnDropHandle::from(abort_handle)
}
```

`AbortOnDropHandle` is a small `AbortHandle` newtype whose
`Drop` impl calls `abort()`; it exists so that fire-and-
forget spawn patterns clean up their continuations when the
owning struct is dropped.

The native side keeps the standard `Send`-bounded spawn
surface and forwards to a tokio runtime; consumers depend
on the unqualified `common::executor::*` re-exports and the
correct binding is picked automatically.

Closely related is the wall-clock module: on native,
`gstuff::now_ms()` is re-exported (libc `gettimeofday`); on
WASM the source replaces it with `js_sys::Date::now() as
u64`. The seed for the small-RNG used in non-cryptographic
contexts likewise uses `now_ms()` on both sides.

## 26.5 The dual transport

[`mm2src/mm2_net/`](../../mm2src/mm2_net/) is the workspace's
network layer. Its `lib.rs` declares both worlds:

```
mm2_net/src/
|-- lib.rs
|-- native_http.rs       hyper 0.14 + rustls
|-- wasm_http.rs         browser Fetch API
|-- wasm_ws.rs           browser WebSocket
|-- transport.rs         unified slurp_* surface
|-- grpc_web.rs          gRPC-WEB (uses cfg_native!/cfg_wasm32!)
`-- ...
```

The slurp surface (`slurp_url`, `slurp_url_with_headers`,
`slurp_post_json`) is re-exported from `transport.rs` with
identical signatures on both targets; the body picks the
right module behind a `cfg`. Consumers above this layer
write one code path.

WebSockets follow the same shape: `wasm_ws.rs` uses the
browser `WebSocket` interface (via `web-sys`), and the
native equivalent uses `tungstenite`. `kdf_walletconnect`
(see [Chapter 22](22-walletconnect-v2.md)) sits on top of
this layer, so its relay code is also target-agnostic.

gRPC-WEB is the third example, and it is the most explicit
about the dual: the file shown in [§26.1.1](#2611-two-macros-for-the-common-case)
uses both `cfg_native!` and `cfg_wasm32!` macros at the top
of the file and a single shared `decode`/`encode` body
below.

## 26.6 Filesystem and OS interactions

There is no browser filesystem. The reloaded tree handles
this by collecting all native-only filesystem code into
[`mm2src/mm2_io/`](../../mm2src/mm2_io/), which is itself
declared with target tables in its `Cargo.toml` such that
its real dependencies (the `gstuff` filesystem helpers,
`tokio::fs`) appear only in the native target block. On
WASM the crate compiles to an empty shell.

Code that needs to "store a thing" therefore does not call
`mm2_io` on WASM; it calls the relevant `*_storage` trait
described in [§26.3](#263-storage-backend-duality), whose
WASM implementation persists the thing to IndexedDB instead.

## 26.7 Hardware wallets and other native-only stacks

Several stacks are native-only by definition: the FFI
chains they pull in do not cross-compile to
`wasm32-unknown-unknown`, or the protocol the stack
implements has no browser equivalent.

The following are entirely or essentially native-only in the
reloaded tree:

- Lightning Network support inside
  [`mm2src/coins/lightning/`](../../mm2src/coins/lightning/)
  -- gated by `cfg_native!` blocks in
  `mm2src/coins/lp_coins.rs`. Browser builds skip the entire
  Lightning module.
- The Z-coin (Zcash) sapling stack
  ([`coins/z_coin/`](../../mm2src/coins/z_coin/)) -- same
  rationale.
- The Solana stack ([`coins/solana_coin/`](../../mm2src/coins/solana_coin/),
  see [Chapter 21 §21.x](21-tron-integration.md) and the
  dedicated provenance note in
  [Chapter 27](27-infrastructure-crate-carve-outs.md)) --
  the modular `solana-*` 2.x crates replaced the legacy
  `solana-remote-wallet -> hidapi -> libudev` chain
  precisely so that the mobile cross-compiles in
  [§26.8](#268-build-infrastructure) could succeed.
- Trezor USB transport
  ([`mm2src/trezor/`](../../mm2src/trezor/)) -- native uses
  the C `secp256k1-ffi` and a USB HID transport; the WASM
  target table ships `js-sys`/`wasm-bindgen` for an
  in-browser variant.
- Ledger USB transport
  ([`mm2src/ledger/`](../../mm2src/ledger/)) -- the present
  crate has only the WASM-side WebUSB transport in its
  dependency table; the native HID path is scaffolded but
  not integrated and the crate is not yet wired into the
  rest of the workspace.

Conversely, [`mm2src/mm2_metamask/`](../../mm2src/mm2_metamask/)
is WASM-only by definition (the EIP-1193 provider object
only exists inside a browser). Its dependency table is
essentially "everything in the WASM target block, nothing in
native".

## 26.8 Build infrastructure

CI builds the cartesian product of (targets) X (test/build/
lint) under [.github/workflows/](../../.github/workflows/):

| Workflow              | Targets covered                        |
|-----------------------|----------------------------------------|
| `build-linux.yml`     | `x86_64-unknown-linux-gnu`             |
| `build-macos.yml`     | x86_64 + ARM64 + Universal (`lipo`)    |
| `build-windows.yml`   | `x86_64-pc-windows-msvc`               |
| `build-wasm.yml`      | `wasm32-unknown-unknown`               |
| `build-ios.yml`       | `aarch64-apple-ios` (`staticlib`)      |
| `build-android.yml`   | `aarch64-linux-android`, `armv7-...`   |
| `dev-build.yml`       | orchestrator, calls all of the above   |
| `test.yml`            | unit + integration + docker + WASM     |

The Android target uses `cargo-ndk` to wrap NDK
cross-compilation; iOS builds rely on the Apple toolchain
on a macOS runner; the macOS universal artefact is produced
by `lipo`-merging the two single-arch builds. ARMv7 Linux
builds use [Cross.toml](../../Cross.toml) with a project-
specific Docker image (the standard `cross` image lacks
some of the audio/USB headers that the workspace's
dependency tree needs even on a server build).

WASM gets two layers of safety net in CI: `cargo check
--target wasm32-unknown-unknown` for fast feedback on
target-table errors, and a `wasm-pack build` step that
exercises the actual `wasm-bindgen` codegen path the
browser package goes through. Either failing fails the
build.

The toolchain itself is pinned via
[`rust-toolchain.toml`](../../rust-toolchain.toml) to a
single stable channel with `rustfmt` and `clippy` added;
see [Chapter 3](03-toolchain-modernization.md) for the
rationale and the upgrade history.

## 26.9 Limitations and known gaps

1. **No automated cross-target test execution.** WASM CI runs
   `cargo check` and `wasm-pack build`, not the unit-test
   binary; mobile CI runs only the build step. Runtime
   behaviour on those targets is checked manually.
2. **Three patterns, not one.** The workspace currently uses
   `cfg_native!`/`cfg_wasm32!` macros, plain `#[cfg(...)]`
   attributes, and `[target.'cfg(...)']` tables. The choice
   between them is conventional rather than enforced; a
   future cleanup could collapse the macros into a single
   crate-level helper.
3. **Trait-on-trait WASM erasure.** A few places still
   require manual `Send`-stripping in async traits to keep
   WASM happy (the
   [`async-trait`](https://crates.io/crates/async-trait) crate
   does not have a target-aware `?Send` mode for some of the
   shapes the workspace uses).
4. **Mobile FFI surface is implicit.** `mm2_bin_lib` exposes
   `lp_main`/`mm2_status` as plain Rust functions; the
   mobile glue (Swift/Kotlin) consumes them via the
   `cdylib`/`staticlib` C ABI. There is no `#[no_mangle]
   extern "C"` declaration in this crate today; the C ABI
   surface is whatever the public Rust functions in
   `mm2_main` happen to emit.
5. **No Wasm-side IPC.** The browser build assumes a single
   `wasm-bindgen` instance per page; there is no shared
   `SharedWorker`/`MessageChannel` story, and a user wanting
   multi-page state sharing has to use the JS hosting layer
   for it.

## 26.10 External references

- The `wasm-bindgen` /  `wasm-bindgen-futures` /  `web-sys` /
  `js-sys` family
  ([rustwasm.github.io/wasm-bindgen](https://rustwasm.github.io/wasm-bindgen/)).
- The `cfg_if` crate
  ([crates.io/crates/cfg-if](https://crates.io/crates/cfg-if)).
- The `cross-rs` cross-compile tool and its image conventions
  ([github.com/cross-rs/cross](https://github.com/cross-rs/cross)).
- The Android NDK and the `cargo-ndk` Cargo subcommand
  ([github.com/bbqsrc/cargo-ndk](https://github.com/bbqsrc/cargo-ndk)).
- Apple `lipo` (`man lipo` on a macOS runner) for the
  universal-binary merge.
- The `wasm-pack` build tool
  ([github.com/rustwasm/wasm-pack](https://github.com/rustwasm/wasm-pack)).
- IndexedDB (W3C
  [Indexed Database API](https://www.w3.org/TR/IndexedDB/))
  as the underlying browser store surfaced by
  [`mm2_db::indexed_db`](../../mm2src/mm2_db/src/indexed_db/).

## 26.11 Provenance

The baseline (`c1d46c0`) shipped:

- `cfg_native!` / `cfg_wasm32!` macros in `common` (an
  earlier form),
- a native-only main and a `cdylib` entry in `mm2src/mm2/`,
- an early IndexedDB layer.

It did not yet ship: the universal macOS lipo step, the iOS
`staticlib` target, the Android `cargo-ndk` workflow, the
modern WASM CI safety net, the WASM-aware executor with
`AbortOnDropHandle`, the `mm2_bin_lib` mobile-binding crate,
the split between the desktop `mm2` binary in `mm2_main`
and the mobile `mm2_bin_lib` shim, the dual `*_storage.rs`
pattern in the new post-baseline persistence consumers
(Chapters 22, 24), or the `solana-*` 2.x replacement that
unblocked mobile cross-compilation. Each of those items is
documented in its own chapter; this chapter records the
overall pattern they all follow.
