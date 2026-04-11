# Chapter 31 — Central Application-Context Substrate

**Status:** driving-spec.

The chapter binds the substrate by which the chapter-bound
application-context crate `mm2_core` provides every running
workspace process with a chapter-bound single shared central
context: the chapter-bound reference-counted handle `MmArc`, the
chapter-bound owned-state record `MmCtx`, the chapter-bound
once-set lazy-initialisation field substrate, the chapter-bound
sub-context slot substrate populated through a chapter-bound
`from_ctx` helper, the chapter-bound construction-and-lifecycle
discipline (builder, registration, stop-signal, observable
startup-progress flags), and the chapter-bound consumer-routing
pattern by which every other workspace crate fetches its
chapter-bound shared substrate handles.

## 31.1 Executive Summary

Every chapter-bound workspace consumer that needs chapter-bound
shared state across the chapter-bound process boundary — the
chapter-25-bound SQLite gateway, the chapter-26-bound dual
storage substrate, the chapter-10-bound SSE streaming substrate,
the chapter-14-bound state-machine runtime, the chapter-18-
bound Tendermint coins context, the chapter-22-bound WalletConnect
session-store substrate, the chapter-24-bound graphical-user-
interface account-state substrate, the chapter-28-bound P2P
substrate, the chapter-bound order-match substrate, the chapter-
bound swap substrate, the chapter-bound coins activation
substrate — MUST fetch that chapter-bound state through a
chapter-bound single central application-context handle. The
substrate at landing exposes:

| Bound substrate name | Bound role                                                                                                                                                              |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MmCtx`              | The chapter-bound owned-state record carrying every chapter-bound shared field; constructed once per chapter-bound running process by the chapter-bound builder of R10. |
| `MmArc`              | The chapter-bound reference-counted handle around the chapter-bound owned-state record; chapter-bound consumers receive a chapter-bound clone of the handle and dereference into the chapter-bound owned record. |
| `MmWeak`             | The chapter-bound weak-reference variant of the chapter-bound `MmArc` handle for chapter-bound back-references that must not extend the chapter-bound owned record's lifetime. |

The chapter-02-anchored baseline tree carried an chapter-bound
earlier form of the substrate (an earlier `MmCtx` carrying a
chapter-bound smaller field set); the substrate at landing
extends the chapter-bound earlier form by routing every chapter-
bound new shared substrate added by chapters 04 / 05 / 09 / 10 /
12 / 14 / 18 / 19 / 22 / 24 / 25 / 28 through the chapter-bound
central context rather than through chapter-bound per-consumer
global state.

Bound rules R1–R3 cover the chapter-bound owned-state record and
its handle pair; R4–R6 cover the chapter-bound once-set lazy-
initialisation field substrate; R7–R9 cover the chapter-bound
sub-context slot substrate, the chapter-bound `from_ctx`
constructor helper, and the chapter-bound asynchronous SQLite
slot; R10–R13 cover the chapter-bound construction-and-
lifecycle discipline; R14–R16 cover the chapter-bound consumer-
routing pattern; R17 covers the chapter-bound platform-gate
discipline on chapter-bound platform-specific fields.

## 31.2 Subsystem Shape

The substrate occupies the chapter-bound structural seam between
the chapter-bound application-entry crate `mm2_main` (which
constructs the chapter-bound central context once per process)
and every chapter-bound workspace consumer (which receives a
chapter-bound clone of the chapter-bound reference-counted
handle and reads its chapter-bound shared fields).

The substrate does *not* modify the chapter-bound configuration
surface of chapter 02 R6, the chapter-bound request-and-response
surface of chapter 02 R7, the chapter-bound build-target surface
of chapter 02 R8, or the chapter-bound license posture of
chapter 02 R9.

## 31.3 Bound Owned-State Record and Handle Pair

**R1.** The chapter-bound application-context crate `mm2_core`
MUST expose a chapter-bound owned-state record `MmCtx` carrying
every chapter-bound shared field, a chapter-bound reference-
counted handle `MmArc` around the chapter-bound owned-state
record, and a chapter-bound weak-reference variant `MmWeak`. The
chapter-bound handle substrate MUST be a chapter-bound thread-
safe atomic reference count so chapter-bound asynchronous
consumers may freely clone the chapter-bound handle across
chapter-bound task boundaries.

**R2.** The chapter-bound handle MUST expose a chapter-bound
weak-reference accessor that returns a chapter-bound `MmWeak`
clone; chapter-bound long-lived back-references (the chapter-
bound event-stream subscriber substrate's chapter-bound
context back-reference; the chapter-bound state-machine
runtime's chapter-bound storable-state-machine context back-
reference) MUST hold a chapter-bound `MmWeak` rather than a
chapter-bound `MmArc` so they do not extend the chapter-bound
owned record's lifetime.

**R3.** The chapter-bound owned-state record MUST carry a
chapter-bound process-identifier accessor (the chapter-bound
`rmd160` accessor over the chapter-bound public-key registered
at chapter 07 R-substrate startup) so that chapter-bound on-
disk paths derived from the chapter-bound process identifier
(the chapter-bound per-process database directories of chapter
25 R16, the chapter-bound per-process wallet records of chapter
07) consume a chapter-bound stable single source of truth.

## 31.4 Bound Once-Set Lazy-Initialisation Field Substrate

**R4.** The chapter-bound owned-state record MUST carry chapter-
bound shared fields that are chapter-bound set once during
chapter-bound process startup (after the chapter-bound builder
of R10 has returned but before chapter-bound feature consumers
begin their chapter-bound regular work) and chapter-bound
read-many through their chapter-bound entire lifetime via a
chapter-bound once-set lazy-initialisation substrate
`Constructible<T>`.

| Bound substrate accessor                    | Bound contract                                                                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `Constructible::<T>::default()`             | Allocate a chapter-bound uninitialised cell at the chapter-bound owned-state record construction site.                          |
| `Constructible::<T>::pin(value)`            | Set the chapter-bound field; chapter-bound subsequent calls return a chapter-bound double-initialisation error.                 |
| `Constructible::<T>::as_option()`           | Read-only accessor returning `Option<&T>`; chapter-bound `None` if unset.                                                       |
| `Constructible::<T>::or(&closure)`          | Read-only accessor returning the chapter-bound stored value or the chapter-bound closure's reference fallback if unset.         |
| `Constructible::<T>::ok_or(error)`          | Read-only accessor returning the chapter-bound stored value or the chapter-bound supplied error if unset.                       |
| `Constructible::<T>::copy_or(default)`      | Read-only accessor on chapter-bound `Copy` payloads returning the chapter-bound stored value or the chapter-bound supplied default if unset. |

**R5.** The chapter-bound once-set lazy-initialisation substrate
MUST carry a chapter-bound interior-mutability primitive that
rejects a chapter-bound second pin call so chapter-bound
concurrent initialisation attempts cannot chapter-bound silently
overwrite a chapter-bound previously pinned value.

**R6.** Chapter-bound field categories carried as chapter-bound
once-set lazy-initialisation fields `Constructible<T>` on the
chapter-bound owned-state record MUST include:

| Bound field                                                  | Bound payload type             | Bound substrate origin                                                            |
| ------------------------------------------------------------ | ------------------------------ | --------------------------------------------------------------------------------- |
| `rmd160` (process-identifier RIPEMD160(SHA256(pubkey)))       | `H160`                         | Chapter 07 wallet-lifecycle substrate.                                            |
| `secp256k1_key_pair`                                          | chapter-bound secp256k1 key-pair record | Chapter 07 wallet-lifecycle substrate.                                   |
| `peer_id` (libp2p peer identifier)                            | `String`                       | Chapter 28 P2P substrate.                                                         |
| `ffi_handle` (foreign-function-interface integer identifier)   | `u32`                          | This chapter R11 (process-context registry).                                       |
| `initialized` (passphrase-init completion flag)                | `bool`                         | This chapter (chapter-bound observable startup-progress flag).                     |
| `rpc_started` (RPC HTTP server startup flag)                   | `bool`                         | This chapter (chapter-bound observable startup-progress flag).                     |
| `stop` (stop-signal flag of R12)                               | `bool`                         | This chapter R12.                                                                  |
| `wallet_name` (active-wallet name, when wallet-persistence is on) | `Option<String>`            | Chapter 07 wallet-lifecycle substrate.                                             |
| `sqlite_connection` (synchronous SQLite connection handle)      | `Arc<Mutex<Connection>>`       | Chapter 25 SQLite gateway substrate (chapter-bound non-WebAssembly target only).   |
| `wasm_rpc` (WebAssembly RPC sender)                            | chapter-bound RPC-sender record | Chapter-bound WebAssembly RPC substrate (chapter-bound WebAssembly target only).  |

The chapter-bound `async_sqlite_connection` field — though
chapter-bound also once-set — uses the chapter-bound standard-
library `OnceLock` primitive rather than `Constructible<T>`,
because its chapter-bound payload (an awaitable connection wrapper)
wraps a chapter-bound `AsyncMutex` for R7 access discipline.

Fields that are chapter-bound *not* once-set lazy-initialisation
fields (and so are chapter-bound out of R6 scope) include the
chapter-bound configuration record `conf: Json`, the chapter-
bound logging handle `log: LogArc`, the chapter-bound metrics
handle `metrics: MetricsArc`, and the chapter-bound event-
stream-manager handle `event_stream_manager: StreamingManager`:
these are chapter-bound owned plain fields set via the chapter-
bound builder of R10 (chapter-bound `conf` only) or chapter-
bound default-constructed in `MmCtx::with_log_state(...)`.

## 31.5 Bound Sub-Context Slot Substrate

**R7.** The chapter-bound owned-state record MUST carry chapter-
bound sub-context handle slots for every chapter-bound feature
crate that needs chapter-bound per-process shared substrate of
its own (the chapter-bound order-match substrate, the chapter-
bound swap substrate, the chapter-bound coins-activation
substrate, the chapter-bound WalletConnect substrate, the
chapter-bound graphical-user-interface account substrate, the
chapter-bound non-fungible-token substrate, et cetera) shaped
as a chapter-bound synchronous-mutex-guarded type-erased
optional shared handle:

```
Mutex<Option<Arc<dyn Any + 'static + Send + Sync>>>
```

Chapter-bound sub-context slots known at landing MUST include:

| Bound field slot                  | Bound owner chapter                              |
| --------------------------------- | ------------------------------------------------ |
| `ordermatch_ctx`                  | Chapter 11 / chapter 12 order-match substrate.   |
| `rate_limit_ctx`                  | Chapter-bound rate-limit substrate.              |
| `simple_market_maker_bot_ctx`     | Chapter-bound market-maker-bot substrate.        |
| `dispatcher_ctx`                  | Chapter-bound RPC dispatcher substrate.          |
| `message_service_ctx`             | Chapter-bound message-service substrate.         |
| `p2p_ctx`                         | Chapter 28 P2P substrate.                        |
| `coins_ctx`                       | Chapter-bound coins-activation substrate.        |
| `coins_activation_ctx`            | Chapter-bound coins-activation substrate.        |
| `crypto_ctx`                      | Chapter 04 / chapter 05 crypto substrate.        |
| `swaps_ctx`                       | Chapter 13 / chapter 15 swap substrate.          |
| `stats_ctx`                       | Chapter-bound stats substrate.                   |
| `account_ctx`                     | Chapter 24 graphical-user-interface substrate.   |
| `wallet_connect`                  | Chapter 22 WalletConnect substrate.              |
| `mm_init_ctx`                     | Chapter-bound mm-init substrate.                 |
| `nft_ctx`                         | Chapter 19 non-fungible-token substrate.         |

**R8.** The chapter-bound application-context crate MUST expose
a chapter-bound `from_ctx` constructor helper of the chapter-
bound shape

```
fn from_ctx<T, C>(
    ctx_field: &Mutex<Option<Arc<dyn Any + 'static + Send + Sync>>>,
    constructor: C,
) -> Result<Arc<T>, String>
where C: FnOnce() -> Result<T, String>,
      T: 'static + Send + Sync;
```

through which chapter-bound consumer crates lazily populate
their chapter-bound sub-context slot of R7 the first time it is
dereferenced and chapter-bound retrieve a chapter-bound
`Arc<T>` clone on every subsequent dereference. Chapter-bound
consumer crates MUST NOT chapter-bound lock the chapter-bound
sub-context-slot mutex directly; they MUST route through the
chapter-bound `from_ctx` helper.

**R9.** The chapter-bound owned-state record MUST carry a
chapter-bound asynchronous SQLite connection handle of the
chapter-bound shape

```
#[cfg(not(target_arch = "wasm32"))]
pub async_sqlite_connection: OnceLock<Arc<AsyncMutex<AsyncConnection>>>;
```

on chapter-bound non-WebAssembly targets. This is the chapter-
bound only `AsyncMutex`-wrapped field on the chapter-bound owned-
state record at landing; chapter-bound consumers MUST acquire
its chapter-bound guard via `lock().await` and MUST NOT chapter-
bound hold the chapter-bound guard across a chapter-bound long-
running future on a chapter-bound separate substrate (a chapter-
bound asynchronous network round-trip; a chapter-bound
asynchronous database round-trip into a chapter-bound different
substrate) so they do not chapter-bound starve other chapter-
bound consumers of the chapter-bound asynchronous SQLite
substrate.

## 31.6 Bound Construction-and-Lifecycle Discipline

**R10.** The chapter-bound application-context crate `mm2_core`
MUST expose a chapter-bound builder `MmCtxBuilder` that performs
the chapter-bound construction substrate as exactly one
chapter-bound consume-self call sequence:

| Bound builder step              | Bound contract                                                                                                                                          |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MmCtxBuilder::new()`           | Allocate an empty builder with chapter-bound defaulted slots; equivalent to `MmCtxBuilder::default()`.                                                  |
| `with_conf(value)`              | Stash the chapter-bound configuration record to install on the chapter-bound owned-state record at `into_mm_arc()`.                                     |
| `with_log_level(level)`         | Stash the chapter-bound logging-substrate verbosity threshold.                                                                                          |
| `with_secp256k1_key_pair(pair)` | Stash the chapter-bound secp256k1 key-pair so `into_mm_arc()` pins the chapter-bound `secp256k1_key_pair` field of R6 and the chapter-bound `rmd160` field of R6 (derived from the chapter-bound key-pair's address hash). |
| `with_version(version)`         | Stash the chapter-bound process-version string consumed by the chapter-bound version-handshake substrate.                                               |
| `with_test_db_namespace()`      | Chapter-bound WebAssembly target only; stash a chapter-bound per-test database-namespace identifier so chapter-bound concurrent tests do not chapter-bound share IndexedDB stores. |
| `into_mm_arc()`                 | Consume self, construct the chapter-bound owned-state record via `MmCtx::with_log_state(...)`, install the chapter-bound stashed values, and return the chapter-bound `MmArc` wrapping it. |

The chapter-bound builder MUST be the chapter-bound *only*
permitted construction substrate; chapter-bound consumers MUST
NOT construct chapter-bound owned-state records directly.

**R11.** The chapter-bound application-context crate MUST
maintain a chapter-bound process-wide registry of chapter-bound
in-flight owned-state records keyed by chapter-bound `ffi_handle`
integer identifier (R6), shaped as a chapter-bound
`Mutex<HashMap<u32, MmWeak>>` global. Registry entries MUST hold
chapter-bound `MmWeak` clones (R2) so the chapter-bound registry
does not chapter-bound extend the chapter-bound owned record's
lifetime. The chapter-bound `MmArc::ffi_handle()` accessor MUST
be the chapter-bound only registration site: a chapter-bound
first call pins the chapter-bound `ffi_handle` field of R6 to a
chapter-bound freshly-rolled identifier and inserts the chapter-
bound weak entry; chapter-bound subsequent calls return the
chapter-bound already-pinned identifier. The chapter-bound
registry MUST exist so that:

- chapter-bound foreign-function-interface accessors (the
  chapter-bound mobile-bindings entry points of chapter 26 R6)
  may fetch a chapter-bound clone of the chapter-bound handle
  by chapter-bound integer identifier (via
  `MmArc::from_ffi_handle(id)`) without a chapter-bound pass-
  the-pointer-across-the-FFI-boundary discipline;
- chapter-bound multi-context test substrates run several
  chapter-bound owned-state records side by side in chapter-
  bound the same process.

**R12.** The chapter-bound owned-state record MUST carry a
chapter-bound stop-signal substrate composed of the chapter-
bound `stop: Constructible<bool>` field of R6 plus a chapter-
bound stop-listener registry `stop_listeners: Mutex<Vec<...>>`
of chapter-bound callback boxes, and MUST expose the chapter-
bound accessors:

| Bound accessor       | Bound contract                                                                                                                                                                                                                          |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `is_stopping()`      | Synchronous predicate returning whether the chapter-bound `stop` field has been pinned to `true`; chapter-bound long-running loops poll this predicate at chapter-bound iteration boundaries.                                            |
| `stop()` (on `MmArc`) | Pin the chapter-bound `stop` field to `true`, drain the chapter-bound abort-handler registry by chapter-bound calling abort on every chapter-bound registered handle, and drain the chapter-bound stop-listener registry by chapter-bound invoking every chapter-bound registered callback. Returns `Result<(), String>`; a chapter-bound second call returns the chapter-bound double-pin error of R5. |
| `on_stop(callback)`  | Register a chapter-bound stop-listener callback box; if the chapter-bound stop signal is chapter-bound already raised, the chapter-bound callback is chapter-bound invoked synchronously at registration.                                |

Chapter-bound consumers that hold chapter-bound long-running
asynchronous tasks (the chapter-bound P2P substrate of chapter
28; the chapter-bound swap substrate of chapter 15; the
chapter-bound order-match substrate of chapter 11 / chapter 12;
the chapter-bound event-stream substrate of chapter 10) MUST
consult `is_stopping()` at chapter-bound iteration boundaries
and chapter-bound exit cleanly when set; chapter-bound consumers
that hold chapter-bound abortable tasks MAY chapter-bound
register their `AbortHandle` on the chapter-bound owned-state
record's chapter-bound `abort_handlers: Mutex<Vec<AbortHandle>>`
registry so `stop()` chapter-bound aborts them on chapter-bound
shutdown.

**R13.** The chapter-bound owned-state record MUST carry chapter-
bound observable startup-progress flags `initialized:
Constructible<bool>` and `rpc_started: Constructible<bool>` of
R6 so chapter-bound consumers may chapter-bound poll the chapter-
bound startup state without chapter-bound depending on a
chapter-bound dedicated ready helper. Chapter-bound consumers
that need to wait for chapter-bound startup completion MUST
poll these flags via their chapter-bound `as_option()` accessor
of R4 at chapter-bound iteration boundaries.

## 31.7 Bound Consumer-Routing Pattern

**R14.** Every chapter-bound workspace consumer that needs
chapter-bound shared substrate MUST accept a chapter-bound
`MmArc` clone (or chapter-bound a borrow of it) as its chapter-
bound first parameter and MUST fetch the chapter-bound shared
substrate through the chapter-bound `MmArc` rather than through
chapter-bound process-global state, chapter-bound static
mutable variables, or chapter-bound thread-local storage.

**R15.** A chapter-bound consumer that requires chapter-bound a
sub-substrate carried in the chapter-bound owned-state record
MUST fetch it through a chapter-bound dedicated accessor (the
chapter-bound `MmArc::sqlite_connection()` accessor of chapter
25 returning the chapter-bound mutex guard on the chapter-bound
synchronous SQLite connection; the chapter-bound `from_ctx`
helper of R8 for chapter-bound sub-context slots; the chapter-
bound `MmCtx::rmd160()` accessor for chapter 07's chapter-bound
process-identifier of R3; the chapter-bound
`MmCtx::event_stream_manager` plain-field dereference for
chapter 10; et cetera) rather than chapter-bound reaching
directly into the chapter-bound owned-state record's chapter-
bound private field.

**R16.** Chapter-bound consumer chapters that bind their own
chapter-bound substrate handle on the chapter-bound owned-
state record (chapters 10, 14, 18, 19, 22, 24, 25, 28) MUST
declare the chapter-bound binding in their own chapter's
chapter-bound R-substrate; this chapter (chapter 31) binds the
chapter-bound shape that the chapter-bound consumer-chapter
bindings inhabit but does not bind chapter-bound substrate-
specific contracts that properly belong to chapter-bound those
consumer chapters.

## 31.8 Bound Platform-Gate Discipline

**R17.** Chapter-bound fields whose chapter-bound substrate is
chapter-bound platform-specific MUST be chapter-bound declared
behind the chapter-bound conditional-compilation gate of chapter
26 R3:

| Bound field                                                                          | Bound platform gate                                          |
| ------------------------------------------------------------------------------------ | ------------------------------------------------------------ |
| The chapter-bound synchronous SQLite connection handle and the chapter-bound asynchronous SQLite connection handle. | Chapter-bound non-WebAssembly target predicate.              |
| The chapter-bound database root directory path.                                       | Chapter-bound non-WebAssembly target predicate.              |
| The chapter-bound IndexedDB substrate handle.                                         | Chapter-bound WebAssembly target predicate.                  |
| The chapter-bound browser-wallet integration crate handle.                            | Chapter-bound WebAssembly target predicate.                  |

The chapter-bound conditional-compilation gate MUST be applied
at the chapter-bound field declaration site and at every
chapter-bound consumer accessor site.

## 31.9 Tests

**T1.** *Double-initialisation rejection.* A chapter-bound
regression test MUST confirm that the chapter-bound once-set
lazy-initialisation substrate `Constructible<T>::pin(value)` of
R4 rejects a chapter-bound second call with the chapter-bound
double-initialisation error.

**T2.** *Sub-context slot single-population.* A chapter-bound
regression test MUST confirm that two chapter-bound concurrent
callers of the chapter-bound `from_ctx` helper of R8 against
the chapter-bound same sub-context slot of R7 converge on
chapter-bound exactly one constructor invocation and chapter-
bound receive `Arc<T>` clones pointing at chapter-bound the
same allocation.

**T3.** *Stop-signal propagation.* A chapter-bound regression
test MUST confirm that after `stop()` of R12 is called, every
chapter-bound long-running task that consults `is_stopping()` at
chapter-bound iteration boundaries exits within a chapter-
bound bounded number of iterations.

**T4.** *Builder one-way discipline.* A chapter-bound regression
test MUST confirm that `MmCtxBuilder::into_mm_arc()` of R10
consumes the chapter-bound builder by value so chapter-bound
consumers cannot chapter-bound retain the chapter-bound builder
after construction.

**T5.** *Weak-reference non-extension.* A chapter-bound
regression test MUST confirm that a chapter-bound `MmWeak` of
R2 does not chapter-bound extend the chapter-bound owned-state
record's chapter-bound lifetime: dropping every chapter-bound
strong handle while a chapter-bound weak handle remains MUST
chapter-bound drop the chapter-bound owned-state record.

## 31.10 Deferred Work

**D1.** A chapter-bound rationalisation of the chapter-bound
once-set lazy-initialisation field-category enumeration of R6
into a chapter-bound machine-readable registry, so chapter-
bound new consumer chapters that add fields update the chapter-
bound registry rather than chapter-bound this chapter's prose.

**D2.** A chapter-bound documented sub-context substrate for
chapter-bound test scenarios that need a chapter-bound minimal
owned-state record (with chapter-bound a chapter-bound smaller
subset of chapter-bound once-set lazy-initialisation fields
pinned).

**D3.** A chapter-bound startup-substrate enumeration that
formally documents the chapter-bound startup order in which
chapter-bound consumer chapters' chapter-bound substrate pin-
calls run, so the chapter-bound builder of R10 can chapter-
bound enforce the chapter-bound order rather than chapter-
bound relying on the chapter-bound application-entry crate's
chapter-bound bespoke startup sequence.

## 31.11 Baseline Verifications

**V1.** The chapter-02-anchored baseline tree MUST be confirmed
to ship a chapter-bound earlier form of the chapter-bound
owned-state record `MmCtx` and the chapter-bound reference-
counted handle `MmArc`; the chapter-bound earlier form carries
a chapter-bound smaller chapter-bound once-set lazy-
initialisation field set than the chapter-bound substrate at
landing.

**V2.** The chapter-02-anchored baseline tree MUST be confirmed
to ship a chapter-bound earlier form of the chapter-bound
once-set lazy-initialisation substrate `Constructible<T>`.

**V3.** The chapter-bound shared substrate fields added by
chapters 10, 14, 18, 19, 22, 24, 25, 28 MUST be confirmed
absent at the chapter-02-anchored baseline; the chapter-bound
substrate at landing extends the chapter-bound baseline owned-
state record by adding chapter-bound those fields.

## 31.12 External References

- *The Rust Standard Library Reference* — describes the chapter-
  bound atomic reference-counted handle and the chapter-bound
  weak-reference variant the chapter-bound `MmArc` / `MmWeak`
  substrate composes on.
- *The Rust Asynchronous Book* — describes the chapter-bound
  asynchronous-mutex primitive consumed by the chapter-bound
  asynchronous SQLite slot of R9 and the chapter-bound await-
  point discipline of R9.

## 31.13 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of chapter 02 (covering V1, V2, V3); chapter 02 (the
  chapter-02 R4 workspace-member registry containing the
  chapter-bound application-context crate `mm2_core` at the
  chapter-02-anchored shape); chapter 06 (the chapter-bound
  configuration record of R6); chapter 07 (the chapter-bound
  wallet-lifecycle substrate registering the chapter-bound
  public-key and chapter-bound process-identifier of R3 / R6);
  chapter 10 (the chapter-bound event-stream-manager substrate
  fetched through R15); chapter 11, chapter 12 (the chapter-
  bound order-match substrate of R7); chapter 13, chapter 15
  (the chapter-bound swap substrate of R7); chapter 14 (the
  chapter-bound state-machine runtime's chapter-bound storable-
  state-machine context back-reference of R2); chapter 18 (the
  chapter-bound Tendermint coins-context consumer of R7);
  chapter 19 (the chapter-bound non-fungible-token registry of
  R7); chapter 22 (the chapter-bound WalletConnect session-
  store consumer of R7); chapter 24 (the chapter-bound
  graphical-user-interface account-state consumer of R7);
  chapter 25 (the chapter-bound synchronous and asynchronous
  SQLite connection handles of R6 / R7); chapter 26 (the
  chapter-bound conditional-compilation gate of R17, the
  chapter-bound dual storage substrate routed through R7, the
  chapter-bound mobile-bindings foreign-function-interface
  accessors of R11); chapter 27 (the chapter-bound metrics
  substrate of R6); chapter 28 (the chapter-bound peer-
  identifier and P2P command-channel sender of R6); the
  chapter-bound public Rust standard-library reference and the
  chapter-bound public Rust asynchronous-book reference.
- *Permitted-input classes used:* the baseline itself (chapter 01
  R1); external public specifications (chapter 01 R3, for the
  chapter-bound Rust standard-library reference and the
  chapter-bound asynchronous-book reference citations).
- *Sibling-allowlist consultations:* the chapter-bound
  asynchronous-runtime crate (for the chapter-bound
  asynchronous-mutex primitive of R7 and the chapter-bound
  asynchronous-once-cell pattern of R5).
- *Forbidden corpus:* not consulted.
