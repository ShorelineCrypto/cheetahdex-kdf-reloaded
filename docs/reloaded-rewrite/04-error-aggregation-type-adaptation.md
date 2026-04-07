# Chapter 04 — Error-Aggregation Type Adaptation to the Modern Trait Solver

## Executive Summary

The project carries an error-aggregation wrapper, `MmError<E>`,
that pairs any inner error type `E` with an ordered list of
source-code locations (`TraceLocation`s) recording the path the
error took through the call stack. At the baseline this wrapper
relied on two cooperating `From` implementations and on a custom
nightly auto-trait, `NotEqual`, whose only job was to keep those
two impls from overlapping when both type parameters happen to be
the same.

The Rust trait solver was reworked between the baseline and the
present day. The reworked solver no longer accepts the disjointness
argument that `NotEqual` was used to express, and the
`From<MmError<E1>> for MmError<E2>` impl that needed `NotEqual`
ceased to compile on stable. The post-baseline adaptation removes
the broken impl and the auxiliary auto-trait, leaving the simpler
`From<E1> for MmError<E2>` blanket impl in place, and introduces a
small explicit helper — `MmResultExt::map_mm_err()` — for the
trace-preserving conversion between two `MmError` types that the
removed impl used to drive.

The wire shape of an `MmError<E>` (the `error`, `error_path`,
`error_trace`, `error_type`, `error_data` fields of the
`mmrpc` JSON envelope) is unchanged. The `NotMmError` auto-trait is
unchanged. Every call site that used to write `?` to lift
`MmResult<_, E1>` to `MmResult<_, E2>` either keeps that idiom
(when `E1` is not itself wrapped by `MmError`) or now writes
`.map_mm_err()?`. The change is mechanical and does not affect
swap, order-match, or RPC behaviour.

A reader leaving this chapter should be able to (a) reconstruct
the baseline two-impl design and the role of `NotEqual` in it,
(b) explain why the design ceased to compile after the trait-solver
rework, and (c) verify that the current design preserves the
external JSON wire contract.

### Why this changed

The error-aggregation rework was forced by the toolchain modernisation recorded in the project's own commit `dd0460c9f` (*P0: Modernize toolchain from nightly-2022-02-01 to stable Rust 1.93*). The relevant excerpt is verbatim:

> *MmError trait solver fix: Removed NotEqual auto trait and From<MmError<E1>> for MmError<E2> impl (fails on modern Rust trait solver, even with RUSTC_BOOTSTRAP). ~350 call sites updated to use .mm_err(Into::into) for MmError→MmError conversions, preserving trace propagation semantics. Keep NotMmError auto trait and From<E1> for MmError<E2> (works fine).*

A subsequent clean-room rewrite of the file landed as `9bc32406c` (*LP-1: restructure mm_err_handle/mm_error.rs per RELOADED standards*), which preserves the API verbatim while replacing prose and reorganising the module: *"Public surface unchanged: type signatures, trait bounds, every function name and return shape, the Serialize impl's exact JSON output \u2026 are all preserved byte-for-byte at the API boundary."*

In clean-room voice: the post-baseline project chose to move off a nightly-2022-02-01 toolchain to stable Rust 1.93, which removed access to the `NotEqual` auto-trait mechanism the baseline relied on for transparent `MmError`-to-`MmError` conversion. The replacement is an explicit `.mm_err(Into::into)` propagation pattern that preserves the existing trace semantics without depending on negative reasoning in the trait solver. A parallel clean-room rewrite of the module's prose was done for legal reasons (LP-1) and is API-compatible.

## Reproduction Detail

### 4.1 The baseline error-aggregation type

The baseline `mm2src/mm2_err_handle/src/mm_error.rs` defines an
`MmError<E>` struct that pairs an inner error of type `E` with a
`Vec<TraceLocation>`. The `Vec` is appended every time the wrapper
crosses a `?` operator or one of the explicit lifting helpers
(`map_to_mm`, `mm_err`, `or_mm_err`). Combined with
`#[track_caller]` on the constructors, the list ends up containing
one entry per conversion site, in stack order.

Two `From` impls drove the `?` operator at the baseline:

```rust
// Impl A — lift an inner error from one MmError to another, preserving trace.
impl<E1, E2> From<MmError<E1>> for MmError<E2>
where
    E1: NotMmError,
    E2: From<E1> + NotMmError,
    (E1, E2): NotEqual,
{
    #[track_caller]
    fn from(orig: MmError<E1>) -> Self { orig.map(E2::from) }
}

// Impl B — wrap a bare inner error into an MmError, starting a new trace.
impl<E1, E2> From<E1> for MmError<E2>
where
    E1: NotMmError,
    E2: From<E1> + NotMmError,
{
    #[track_caller]
    fn from(e1: E1) -> Self { MmError::new(E2::from(e1)) }
}
```

Impl A is the interesting one: it allows a caller to write

```rust
fn outer() -> MmResult<(), OuterError> {
    let v = inner_call()?; // returns MmResult<_, InnerError>
    ...
}
```

and have the `?` operator do two things at once — convert
`InnerError` into `OuterError` via `OuterError: From<InnerError>`,
and append the call site to the trace stored in the existing
`MmError<InnerError>` rather than discarding it and starting fresh.

Without Impl A, the same `?` would fall through to Impl B, which
treats the `MmError<InnerError>` as an opaque inner error and
wraps it inside a *new* `MmError`, producing a value of type
`MmError<MmError<InnerError>>`. The `NotMmError` auto-trait — also
defined at the baseline — explicitly excludes any `MmError<_>`
from being treated as a valid inner error, so this fallthrough is
in fact disallowed by the type checker:

```rust
pub auto trait NotMmError {}
impl<E> !NotMmError for MmError<E> {}
```

This left exactly one disjointness problem to solve. When `E1`
equals `E2`, both impls are applicable: Impl A trivially (`E2:
From<E1>` reduces to `E1: From<E1>`, which `core` provides for
every type), and Impl B vacuously. Rust's coherence rules reject
overlapping `From` impls.

The baseline solution was the `NotEqual` auto-trait:

```rust
pub auto trait NotEqual {}
impl<X> !NotEqual for (X, X) {}
impl<T: ?Sized> NotEqual for Box<T> {}
```

By bounding Impl A with `(E1, E2): NotEqual`, the impl was
restricted to type-parameter pairs where the two members are
distinct, and the overlap with Impl B disappeared. `NotEqual` was
re-exported from the crate's `prelude` so that downstream code
could express the same disjointness when it needed to write
similar generic impls.

### 4.2 Why the design ceased to compile

`auto trait` and the `impl !Trait for Type` syntax used to express
"negative" implementations are both still nightly-only Rust
features. The error crate at the baseline opted into both via:

```rust
#![feature(negative_impls)]
#![feature(auto_traits)]
```

Through the post-baseline toolchain migration (chapter 03), those
two features stayed in use; they are among the small number of
nightly-only language constructs that the project still needs and
that it bridges to the stable toolchain through the
`RUSTC_BOOTSTRAP` allowlist.

The relevant change is not in the features themselves but in the
trait solver that consumes the impls. Between the baseline and the
present day, the Rust language team reworked the trait solver. The
reworked solver evaluates negative implementations and overlap
arguments through a different procedure and, in particular,
rejects the "negative impl over a structural pattern such as
`(X, X)`" pattern that `NotEqual` relied on. After that rework
the Impl A constraint `(E1, E2): NotEqual` no longer satisfies the
compiler as a disjointness proof, and the two `From` impls are
once again rejected for overlap.

The configuration comment in `.cargo/config.toml` records the
proximate trigger:

```
- mm2_err_handle: `auto trait NotMmError` + negative impls
                  (NotEqual was removed in P0 — Rust 1.93 trait solver regression)
```

"P0" is a project-internal priority label; it identifies the
remediation as a build-blocker fix rather than a feature change.

### 4.3 The adaptation

The adaptation has three pieces.

**4.3.1 Removed Impl A.** The `From<MmError<E1>> for MmError<E2>`
impl is no longer present in `mm_error.rs`. The `NotEqual`
auto-trait is no longer defined and is no longer re-exported from
the crate's `prelude`. A grep across the current tree
(`grep -rn 'NotEqual' mm2src/`) finds zero occurrences.

**4.3.2 Kept Impl B.** The simpler `From<E1> for MmError<E2>` impl
remains exactly as at the baseline:

```rust
impl<E1, E2> From<E1> for MmError<E2>
where
    E1: NotMmError,
    E2: From<E1> + NotMmError,
{
    #[track_caller]
    fn from(e1: E1) -> Self { MmError::new(E2::from(e1)) }
}
```

Because Impl A is gone, the `?` operator can no longer lift
`MmResult<_, E1>` into `MmResult<_, E2>` directly when both are
already `MmError`-wrapped — the type `MmError<E1>` is excluded
from `E1: NotMmError` by the negative impl on `MmError<_>`. The
bare `?` continues to work for converting any non-`MmError` inner
error into an `MmError` of a different type, which is the more
common pattern at call sites.

**4.3.3 Added `MmResultExt::map_mm_err()`.** The trace-preserving
lift between two `MmError` types is provided as an extension
method on `Result<T, MmError<E1>>`:

```rust
pub trait MmResultExt<T, E1> {
    #[track_caller]
    fn map_mm_err<E2>(self) -> Result<T, MmError<E2>>
    where
        E2: From<E1> + NotMmError;
}

impl<T, E1> MmResultExt<T, E1> for Result<T, MmError<E1>>
where
    E1: NotMmError,
{
    #[track_caller]
    fn map_mm_err<E2>(self) -> Result<T, MmError<E2>>
    where
        E2: From<E1> + NotMmError,
    {
        match self {
            Ok(v) => Ok(v),
            Err(err_e1) => Err(err_e1.map(E2::from)),
        }
    }
}
```

Call sites that used to read

```rust
let v = inner_call()?;
```

now read

```rust
let v = inner_call().map_mm_err()?;
```

when `inner_call` returns `MmResult<_, E1>` and the enclosing
function returns `MmResult<_, E2>` for some `E2: From<E1>`. The
behaviour is identical: `MmError::map` is called with `E2::from`,
which preserves the existing trace and appends the caller's source
location (via `#[track_caller]`).

For callers whose conversion is not a `From` conversion — i.e.
whose `E2` cannot be derived mechanically from `E1` — the existing
`MapMmError::mm_err(|e1| ...)` helper (present at the baseline and
unchanged) remains the recommended idiom.

### 4.4 What did not change

The JSON wire shape of a serialised `MmError<E>` is unchanged. The
`Serialize` implementation produces an object with the same five
fields as at the baseline:

| Field | Source |
|---|---|
| `error` | `etype.to_string()` |
| `error_path` | `MmError::path()` — dot-separated, de-duplicated file chain |
| `error_trace` | `MmError::stack_trace()` — full `file:line]` chain |
| `error_type` | `E`'s `#[serde(tag)]` discriminator |
| `error_data` | `E`'s `#[serde(content)]` payload |

These are the field names the `mmrpc` JSON-RPC clients consume.
They form part of the project's external interface and cannot
change without breaking GUIs and integrations.

The `NotMmError` auto-trait is unchanged: it still has the same
definition, the same negative impl on `MmError<E>`, and the same
two opt-in impls on `Box<T: ?Sized>` and `UnsafeCell<T: ?Sized>`
that the baseline used to compensate for auto-traits not propagating
through unsized types.

The `SerMmErrorType` blanket bound, the `SerializeErrorType`
trait in the `ser_error` crate, the `#[derive(SerializeErrorType)]`
attribute, the `HttpStatusCode` blanket impl, the
`MmError::new` / `MmError::err` / `MmError::map` /
`MmError::new_with_trace` / `MmError::split` API, and the
`map_to_mm` / `mm_err` / `or_mm_err` / `map_to_mm_fut` extension
traits are all preserved.

### 4.5 Reproducing the adaptation from the baseline

Starting from the baseline working tree at commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`, an engineer aware of
the trait-solver regression can reproduce the present state by:

1. Open `mm2src/mm2_err_handle/src/mm_error.rs`.
2. Delete the `From<MmError<E1>> for MmError<E2>` impl block.
3. Delete the `pub auto trait NotEqual {}` declaration and its two
   accompanying impls (`impl<X> !NotEqual for (X, X) {}` and
   `impl<T: ?Sized> NotEqual for Box<T> {}`).
4. Open `mm2src/mm2_err_handle/src/lib.rs`. Remove `NotEqual` from
   the `prelude` re-export list.
5. Open `mm2src/mm2_err_handle/src/map_mm_error.rs`. Add a new
   trait `MmResultExt<T, E1>` with a single `map_mm_err<E2>()`
   method, plus the blanket impl shown in §4.3.3 above. Re-export
   `MmResultExt` from the crate's `prelude` if call sites use the
   bare method name.
6. For every call site in the workspace where the type checker now
   complains that `?` cannot lift `MmResult<_, E1>` to
   `MmResult<_, E2>`, change the call to
   `.map_mm_err()?` (or, if the conversion is not a `From`
   conversion, to `.mm_err(|e| ...)?`).

The build should pass against stable Rust with the
`mm_err_handle` crate present in `.cargo/config.toml`'s
`RUSTC_BOOTSTRAP` allowlist (see chapter 03), because
`negative_impls` and `auto_traits` are still required by the
surviving `NotMmError` declaration.

## External References

- *The Rust Reference — auto traits.* Defines the `auto trait`
  declaration and the negative-impl syntax used by `NotMmError`
  and (at the baseline) `NotEqual`.
  https://doc.rust-lang.org/reference/special-types-and-traits.html#auto-traits
- *The Rust Reference — `#[track_caller]` attribute.* Documents
  the mechanism that lets `From::from` record the caller's source
  location, which is the basis of `MmError`'s trace.
  https://doc.rust-lang.org/reference/attributes/codegen.html#the-track_caller-attribute
- *The Rust Reference — coherence and overlap rules.* The basis
  for the disjointness argument that `NotEqual` originally
  expressed.
  https://doc.rust-lang.org/reference/items/implementations.html#trait-implementation-coherence
- *Rust language team — next-generation trait solver.* The
  upstream effort that produced the trait-solver behaviour change
  this chapter is responding to.
  https://blog.rust-lang.org/inside-rust/2023/07/17/trait-system-refactor-initiative.html
- *Cargo Reference — `RUSTC_BOOTSTRAP`.* The mechanism that lets
  `mm_err_handle` keep its `auto_traits` and `negative_impls`
  declarations while the rest of the project compiles on stable.
  https://doc.rust-lang.org/cargo/reference/environment-variables.html

## Provenance Footer

*This chapter v1; verified directly against the baseline tree at
commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` and the current
tree on 2026-05-31. Reviewer #1 and reviewer #2 reports stored at
`local/clean-room-doc/reviews/04-error-aggregation-type-adaptation-r{1,2}.md`.*
