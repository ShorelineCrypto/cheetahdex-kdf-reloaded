# Chapter 31 — Central Application-Context Substrate

**Status:** driving-spec.

The chapter binds the substrate by which the chapter-bound
application-context crate `mm2_core` provides every running
workspace process with a chapter-bound single shared central
context: the chapter-bound reference-counted handle `MmArc`, the
chapter-bound owned-state record `MmCtx`, the chapter-bound
once-set lazy-initialisation field substrate, the chapter-bound
asynchronous-mutex-guarded field substrate, the chapter-bound
construction-and-lifecycle discipline (builder, registration,
ready-signal, stop-signal), and the chapter-bound consumer-
routing pattern by which every other workspace crate fetches its
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
asynchronous-mutex-guarded field substrate; R10–R13 cover the
chapter-bound construction-and-lifecycle discipline; R14–R16
cover the chapter-bound consumer-routing pattern; R17 covers the
chapter-bound platform-gate discipline on chapter-bound platform-
specific fields.

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
| `Constructible<T>::pin(value)`              | Set the chapter-bound field exactly once; chapter-bound subsequent calls return a chapter-bound double-initialisation error.    |
| `Constructible<T>::get_or_initialize(closure)` | Set-if-unset substrate; chapter-bound idempotent under chapter-bound concurrent callers.                                     |
| `Constructible<T>::or_err(closure)`         | Read-only accessor returning the chapter-bound stored value or the chapter-bound closure's error if unset.                      |
| `Constructible<T>::or_else(closure)`        | Read-only accessor returning the chapter-bound stored value or the chapter-bound closure's fallback if unset.                   |

**R5.** The chapter-bound once-set lazy-initialisation substrate
MUST carry a chapter-bound thread-safe primitive (a chapter-
bound asynchronous-once-cell pattern) so chapter-bound
concurrent initialisation attempts converge on chapter-bound
exactly one initialisation.

**R6.** Chapter-bound field categories carried as chapter-bound
once-set lazy-initialisation fields MUST include:

| Bound field category                                                                                                                                  | Bound substrate origin                                                            |
| ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| The chapter-bound process-identifier accessor and the chapter-bound public-key registered at chapter 07 R-substrate startup.                          | Chapter 07 wallet-lifecycle substrate.                                            |
| The chapter-bound synchronous SQLite connection handle `SqliteConnShared` (R4 of chapter 25).                                                          | Chapter 25 SQLite gateway substrate.                                              |
| The chapter-bound peer-identifier registered at chapter 28 R-substrate startup.                                                                       | Chapter 28 P2P substrate.                                                         |
| The chapter-bound P2P-substrate command-channel sender.                                                                                                | Chapter 28 P2P substrate.                                                         |
| The chapter-bound event-stream-manager substrate handle.                                                                                              | Chapter 10 SSE streaming substrate.                                               |
| The chapter-bound metrics-substrate handle.                                                                                                            | The chapter-bound metrics crate `mm2_metrics` of chapter 27.                      |
| The chapter-bound configuration record loaded from the chapter-bound JSON configuration file of chapter 06.                                            | Chapter 06 net-config registry substrate.                                          |
| The chapter-bound database root directory path on the chapter-bound native target.                                                                     | Chapter 26 R15 native-filesystem isolation substrate.                              |
| The chapter-bound stop-signal sender (R12).                                                                                                            | This chapter R12.                                                                  |

## 31.5 Bound Asynchronous-Mutex-Guarded Field Substrate

**R7.** The chapter-bound owned-state record MUST carry chapter-
bound shared fields that are chapter-bound mutated through the
chapter-bound process lifetime — chapter-bound active swaps,
chapter-bound order books, chapter-bound coin handles, chapter-
bound WalletConnect session store, chapter-bound graphical-
user-interface account state — behind a chapter-bound
asynchronous-mutex primitive so chapter-bound concurrent
consumers serialise their access without blocking the chapter-
bound asynchronous runtime.

| Bound field                                            | Bound owner chapter                          | Bound contract                                                                                                                                                              |
| ------------------------------------------------------ | -------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The chapter-bound asynchronous SQLite connection handle `AsyncMutex<AsyncConnection>`. | Chapter 25.                                  | Awaitable wrapper around the chapter-25 R11 dedicated worker thread.                                                                                                       |
| The chapter-bound coins-context registry handle.        | The chapter-bound coins-activation substrate. | The chapter-bound per-coin handle map keyed by chapter-bound ticker; consumers fetch through a chapter-bound asynchronous-mutex-guarded accessor.                          |
| The chapter-bound order-book substrate handle.          | Chapter 11 / chapter 12 order-match substrate. | The chapter-bound my-order / chapter-bound peer-order book pair behind asynchronous mutexes.                                                                                |
| The chapter-bound active-swap registry handle.          | Chapter 13 / chapter 15 swap substrate.       | The chapter-bound running-swap map keyed by chapter-bound swap-UUID.                                                                                                        |
| The chapter-bound graphical-user-interface account-state handle. | Chapter 24.                                  | Routed through the chapter-24 storage trait of chapter 26 R8.                                                                                                              |
| The chapter-bound WalletConnect session-store handle.   | Chapter 22.                                  | Routed through the chapter-22 storage trait of chapter 26 R8.                                                                                                              |
| The chapter-bound non-fungible-token registry handle.   | Chapter 19.                                  | Routed through the chapter-19 storage trait of chapter 26 R8.                                                                                                              |

**R8.** Chapter-bound consumers MUST acquire the chapter-bound
asynchronous-mutex guard via a chapter-bound `lock().await`-
shaped accessor; chapter-bound consumers MUST NOT hold the
chapter-bound guard across a chapter-bound await point on a
chapter-bound long-running future (the chapter-bound
asynchronous network round-trip, the chapter-bound asynchronous
database round-trip) so they do not chapter-bound starve other
chapter-bound consumers of the chapter-bound same field.

**R9.** Chapter-bound consumers that need chapter-bound read-
only access to a chapter-bound asynchronous-mutex-guarded field
MAY take the chapter-bound guard, chapter-bound clone the
chapter-bound relevant data out under the guard, chapter-bound
release the guard, and chapter-bound operate on the chapter-
bound cloned data.

## 31.6 Bound Construction-and-Lifecycle Discipline

**R10.** The chapter-bound application-context crate `mm2_core`
MUST expose a chapter-bound builder `MmCtxBuilder` that performs
the chapter-bound construction substrate as exactly one
chapter-bound consume-self call sequence:

| Bound builder step              | Bound contract                                                                                                                                          |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `MmCtxBuilder::new()`           | Allocate an empty chapter-bound owned-state record with chapter-bound once-set fields uninitialised.                                                    |
| `with_conf(value)`              | Pin the chapter-bound configuration record (R6).                                                                                                        |
| `with_log_level(level)`         | Pin the chapter-bound logging-substrate verbosity threshold.                                                                                            |
| `with_version(...)`             | Pin the chapter-bound process-version substrate consumed by the chapter-bound version-handshake substrate of chapter 13.                                |
| `with_datetime(...)`            | Pin the chapter-bound build-time datetime consumed by the chapter-bound version-handshake substrate of chapter 13.                                      |
| `into_mm_arc()`                 | Consume self, register the chapter-bound resulting owned-state record on the chapter-bound process-wide context-registry of R11, and return the chapter-bound `MmArc` clone. |

The chapter-bound builder MUST be the chapter-bound *only*
permitted construction substrate; chapter-bound consumers MUST
NOT construct chapter-bound owned-state records directly.

**R11.** The chapter-bound application-context crate MUST
maintain a chapter-bound process-wide registry of chapter-bound
in-flight owned-state records keyed by chapter-bound process-
context identifier so that:

- chapter-bound foreign-function-interface accessors (the
  chapter-bound mobile-bindings entry points of chapter 26 R6)
  may fetch a chapter-bound clone of the chapter-bound handle
  by chapter-bound integer identifier without a chapter-bound
  pass-the-pointer-across-the-FFI-boundary discipline;
- chapter-bound stop-signal handling (R12) finds the chapter-
  bound right owned-state record by chapter-bound identifier;
- chapter-bound multi-context test substrates run several
  chapter-bound owned-state records side by side in chapter-
  bound the same process.

**R12.** The chapter-bound owned-state record MUST carry a
chapter-bound stop-signal substrate exposing exactly two
chapter-bound accessors:

| Bound accessor       | Bound contract                                                                                                                                                              |
| -------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `is_stopping()`      | Synchronous predicate returning whether the chapter-bound stop signal has been raised; chapter-bound long-running loops poll this predicate at chapter-bound iteration boundaries. |
| `stop()`             | Idempotent setter that raises the chapter-bound stop signal; chapter-bound subsequent calls are chapter-bound no-ops.                                                       |

Chapter-bound consumers that hold chapter-bound long-running
asynchronous tasks (the chapter-bound P2P substrate of chapter
28; the chapter-bound swap substrate of chapter 15; the
chapter-bound order-match substrate of chapter 11 / chapter 12;
the chapter-bound event-stream substrate of chapter 10) MUST
consult `is_stopping()` at chapter-bound iteration boundaries
and chapter-bound exit cleanly when set.

**R13.** The chapter-bound application-context crate MUST expose
a chapter-bound *ready* helper that resolves to chapter-bound
true once every chapter-bound once-set lazy-initialisation field
of R6 enumerated as chapter-bound startup-mandatory has been
chapter-bound pinned. Chapter-bound asynchronous consumers that
need to wait for chapter-bound startup completion before
beginning their chapter-bound work MUST await the chapter-bound
ready helper.

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
chapter-bound `MmArc::event_stream_manager()` accessor for
chapter 10; the chapter-bound `MmArc::sqlite_connection()`
accessor for chapter 25; the chapter-bound coins-context
accessor for the chapter-bound coins-activation substrate; the
chapter-bound peer-identifier accessor for chapter 28; et
cetera) rather than chapter-bound reaching directly into the
chapter-bound owned-state record's chapter-bound field.

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

**T2.** *Concurrent-initialisation convergence.* A chapter-bound
regression test MUST confirm that two chapter-bound concurrent
callers of `Constructible<T>::get_or_initialize(closure)` of R4
converge on chapter-bound exactly one initialisation.

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
  asynchronous-mutex-guarded field substrate of R7 and the
  chapter-bound await-point discipline of R8.

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
