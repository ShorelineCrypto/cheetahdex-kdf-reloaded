# Chapter 03 — Toolchain Modernization

## Executive Summary

At the June 2022 baseline the project was pinned to a specific
Rust nightly toolchain (`nightly-2022-02-01`) and used a number of
nightly-only language features sprinkled across several crates. It
also pinned a forked `backtrace` crate to work around an Android
`dl_iterate_phdr` issue that was specific to the nightly compiler
of the era, and it spelled its release-profile dependency overrides
in a syntax that Cargo subsequently renamed.

The post-baseline modernisation moved the project to the **stable**
Rust toolchain. The four crates that still depend on a nightly-only
language feature (negative trait implementations, custom test
frameworks) compile on stable through Cargo's
`RUSTC_BOOTSTRAP` allowlist, scoped to exactly those crate names,
documented in-tree, and earmarked for removal once the upstream
features stabilise. The Android `backtrace` patch is gone. The
release-profile override block uses the current Cargo spelling
(`[profile.release.package."*"]`). Workspace members migrated en
masse from Rust edition 2018 to edition 2021. Cross-compilation
configuration was untouched.

A separate concern — code formatting — still relies on nightly
features of `rustfmt`. That is handled at the CI layer by
installing a nightly compiler *only for the format check job*;
the actual build and test toolchain remains stable.

A reader leaving this chapter should be able to (a) reproduce the
toolchain switch from the baseline, (b) explain why a small set of
crates retains `#![feature(...)]` annotations, and (c) verify that
the removed patches and renamed sections are no longer needed for
any current build target.

## Reproduction Detail

### 3.1 The baseline toolchain pin

The baseline `rust-toolchain.toml` reads, verbatim:

```toml
[toolchain]
channel = "nightly-2022-02-01"
components = ["rustfmt", "clippy"]
```

Every developer build, CI build, and release build at the baseline
therefore used the same nightly compiler snapshot. This was a hard
dependency on the nightly channel: any source file with a
`#![feature(...)]` attribute requires the nightly compiler to
compile at all.

A scan of the baseline tree (`git grep '#!\[feature'` against the
commit) shows the following distinct feature attributes in use,
with the number of files in which each appeared:

| Nightly feature attribute | Files |
|---|---|
| `async_closure` | 5 |
| `auto_traits` | 2 |
| `custom_test_frameworks` | 1 |
| `drain_filter` | 4 |
| `hash_raw_entry` | 5 |
| `integer_atomics` | 2 |
| `integer_atomics, panic_info_message` | 1 |
| `io_error_more` | 1 |
| `ip` | 1 |
| `map_first_last` | 2 |
| `negative_impls` | 3 |
| `stmt_expr_attributes` | 1 |
| `test` | 2 |

Most of these features were either subsequently stabilised by the
Rust language team (for example `drain_filter` under the new name
`extract_if`, `map_first_last` as `first_last_iterator`-related
methods, `hash_raw_entry` via accessor methods on `HashMap`,
`io_error_more` as concrete `ErrorKind` variants, `panic_info_message`
as `PanicInfo::message()`) or were avoidable through small-scale
rewrites against stable APIs.

### 3.2 The new toolchain pin

The current `rust-toolchain.toml` reads:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

The components list is unchanged. Every routine build — developer,
CI, release — runs against the latest stable Rust at the time the
build is performed.

### 3.3 What remained on nightly, and why

A scan of the current tree shows the surviving `#![feature(...)]`
attributes are concentrated in four source files:

| File | Features used |
|---|---|
| `mm2src/common/common.rs` | `negative_impls`, `auto_traits` |
| `mm2src/mm2_err_handle/src/lib.rs` | `negative_impls`, `auto_traits` |
| `mm2src/mm2_state_machine/src/lib.rs` | `negative_impls`, `auto_traits` |
| `mm2src/mm2_main/src/docker_tests.rs` | `custom_test_frameworks`, `test` |

These four files (plus two vendored test-mocking crates,
`mocktopus` and `mocktopus_macros`) need nightly-only behaviour
that has no equivalent on stable:

- `auto_traits` plus `negative_impls` express an
  "if-T-is-not-already-X-then-treat-it-as-Y" pattern that the
  project's typed-error framework uses to make `MmError<E>`-style
  conversions compose without conflicting blanket implementations.
  There is no equivalent on stable Rust today; rewriting the
  framework to avoid the pattern would touch dozens of error
  types across the workspace.
- `custom_test_frameworks` plus `test` are used by an in-tree
  test harness for the Dockerised integration tests, which need
  to enumerate and dispatch tests in a non-standard way.

To compile these crates on the stable toolchain, the project uses
Cargo's `RUSTC_BOOTSTRAP` mechanism, scoped to exactly the affected
crate names. The relevant excerpt from `.cargo/config.toml` reads:

```toml
[env]
# Enables unstable features on stable toolchain for specific crates only.
# ...
RUSTC_BOOTSTRAP = "common,mm2_err_handle,mm2_state_machine,docker_tests,mocktopus,mocktopus_macros"
```

This is an opt-in: it enables nightly-only features only in the
listed crates and only during builds run with this configuration.
Crates outside the list cannot use `#![feature(...)]` annotations
and the compiler will reject them as it would on any stable build.

This arrangement is documented in-tree as a temporary bridge: when
the upstream Rust project stabilises `auto_traits`-equivalent
behaviour or when the codebase migrates off the affected pattern,
the corresponding crate is to be removed from the `RUSTC_BOOTSTRAP`
list.

### 3.4 The edition migration

Rust crates declare a language-edition pin in their per-crate
`Cargo.toml`. The baseline distribution of editions across the
workspace was:

| Edition | Crates |
|---|---|
| 2018 | 24 |
| 2021 | 3 |

The current distribution is:

| Edition | Crates |
|---|---|
| 2021 | 48 |
| 2018 | 2 |
| 2015 | 1 |

The two surviving 2018 crates and the single 2015 crate are
vendored upstream code preserved at the edition of their origin.
Every first-party workspace crate is now on edition 2021.

The edition bump is a per-crate operation: change
`edition = "2018"` to `edition = "2021"` in the crate's
`Cargo.toml`, then run `cargo fix --edition` against that crate to
mechanically rewrite any source patterns that the new edition
treats differently (for the 2018→2021 hop, the principal change is
that array-into-iterator conversions follow the
`IntoIterator for [T; N]` implementation, which can require small
turn-of-phrase changes to call sites that were relying on the
previous behaviour).

### 3.5 The removed `backtrace` patch

The baseline root `Cargo.toml` carried:

```toml
[patch.crates-io]
backtrace = { git = "https://github.com/artemii235/backtrace-rs.git" }
backtrace-sys = { git = "https://github.com/artemii235/backtrace-rs.git" }
```

The accompanying comment in the baseline manifest explains the
patch: the upstream `backtrace` crate at the time, when built with
the project's nightly toolchain for the Android target, did not
define `HAVE_DL_ITERATE_PHDR`, which led to unreadable backtraces
on Android binaries. The fork enabled the macro for Android
toolchain levels that supported it.

The current root `Cargo.toml` does not carry the patch. With the
move to stable Rust and current versions of `backtrace` from
crates.io, the workaround is no longer needed on any supported
target.

### 3.6 The renamed `[profile.release]` override block

The baseline root `Cargo.toml` carried:

```toml
[profile.release.overrides."*"]
debug = false
```

`overrides` was Cargo's then-name for per-package profile
adjustments. Cargo later renamed it to `package` to match the
broader `[package.metadata.*]` convention. The current
`Cargo.toml` carries the equivalent block under the new name:

```toml
[profile.release.package."*"]
debug = false
```

The semantics are unchanged: dependencies outside the workspace
build without debug symbols even when the release profile carries
`debug = true` for the workspace's own crates.

### 3.7 Adoption of `[workspace.dependencies]`

A second change to the root `Cargo.toml` is the introduction of a
`[workspace.dependencies]` block. This Cargo feature (stabilised in
Cargo 1.64) allows the workspace root to declare a single version
of each shared third-party dependency and lets each crate inherit
that version with `serde = { workspace = true }` or similar in its
per-crate `Cargo.toml`.

The baseline manifest did not use this feature — each crate
declared its own dependency versions independently. The current
manifest centralises shared versions, which removes a long-running
risk of two workspace crates accidentally compiling against two
different minor versions of the same library and pulling both into
the resulting binary.

### 3.8 What did *not* change

Two pieces of the toolchain surface were deliberately left as they
were at the baseline:

- The Cargo feature resolver remains at v2 (`resolver = "2"` in
  the workspace root). This is the resolver that the baseline
  already selected.
- `Cross.toml` is byte-identical to the baseline. The
  cross-compilation configuration for the `armv7-unknown-linux-gnueabihf`
  target (custom image name, dynamic-linker `rustflags`) continues
  to be the only entry, with the same image name and flag.

### 3.9 The rustfmt nightly carve-out

The project's `rustfmt.toml` (present at the baseline and retained
since) uses formatting options that are nightly-only as of this
writing (`unstable_features = true`, `inline_attribute_width`,
`overflow_delimited_expr`). Stable `rustfmt` rejects these options
with an error rather than ignoring them, which means a stable
toolchain cannot run the configured formatting check.

The post-baseline handling installs a nightly toolchain *only* for
the format-check step of the CI workflow and runs `cargo +nightly
fmt --all -- --check` from there. The build and test jobs continue
to use stable. The format check therefore has its own toolchain
dependency that is independent of the compilation toolchain.

This carve-out is the only nightly toolchain installation that
runs in routine CI; it does not produce any compiled artefact.

### 3.10 Reproducing the migration from the baseline

Any reader can reproduce the migration from the baseline working
tree by performing, in order:

1. Edit `rust-toolchain.toml` to set `channel = "stable"`.
2. For every workspace crate whose `Cargo.toml` declares
   `edition = "2018"`, change it to `edition = "2021"` and run
   `cargo fix --edition -p <crate>` against that crate.
3. For every `#![feature(...)]` attribute in the tree, either
   rewrite the affected code against the stable equivalent that
   the Rust language team subsequently shipped (most of the
   features listed in §3.1 had stable equivalents by 2024), or
   add the owning crate to a `RUSTC_BOOTSTRAP` list in
   `.cargo/config.toml`.
4. Delete the `[patch.crates-io]` block that pins the forked
   `backtrace` and `backtrace-sys`.
5. Rename the `[profile.release.overrides."*"]` table to
   `[profile.release.package."*"]`.
6. Optionally, add a `[workspace.dependencies]` block and migrate
   per-crate dependency declarations to `workspace = true`.
7. Add a CI step that installs nightly `rustfmt` only for format
   checking, leaving the build and test toolchain at stable.

At the end of these steps the project should build and pass tests
against stable Rust on every target the baseline supported.

## External References

- *Rust Edition Guide.* The official guide describes the 2018→2021
  edition migration and the `cargo fix --edition` workflow.
  https://doc.rust-lang.org/edition-guide/
- *The Rust Reference — Conditional compilation: the `feature`
  attribute.* Describes the `#![feature(...)]` mechanism and its
  restriction to the nightly toolchain.
  https://doc.rust-lang.org/reference/attributes/codegen.html
- *Cargo Reference — `RUSTC_BOOTSTRAP`.* Documents the environment
  variable that the Rust project provides as an escape hatch for
  using unstable features on stable toolchains during build
  bootstraps.
  https://doc.rust-lang.org/cargo/reference/environment-variables.html
- *Cargo Reference — Profile overrides.* Documents the
  `[profile.<name>.package.<spec>]` syntax (the successor to the
  earlier `overrides` spelling).
  https://doc.rust-lang.org/cargo/reference/profiles.html#overrides
- *Cargo Reference — Workspace inheritance.* Documents the
  `[workspace.dependencies]` block stabilised in Cargo 1.64.
  https://doc.rust-lang.org/cargo/reference/workspaces.html#the-dependencies-table
- *backtrace-rs issue #227.* The upstream tracking issue for the
  Android `dl_iterate_phdr` situation that the baseline `backtrace`
  patch worked around.
  https://github.com/rust-lang/backtrace-rs/issues/227

## Provenance Footer

*This chapter v1; verified directly against the baseline tree at
commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` and the current
tree on 2026-05-31. Reviewer #1 and reviewer #2 reports stored at
`local/clean-room-doc/reviews/03-toolchain-modernization-r{1,2}.md`.*
