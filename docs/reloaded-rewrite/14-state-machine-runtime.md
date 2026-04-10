# Chapter 14 — Generic State-Machine Runtime

**Status:** driving-spec.

A reusable in-process finite-state-machine substrate exposed as a
standalone crate, with a layered storable variant that binds event
sourcing, persistence, reentrancy locking, and recovery — replacing
the in-baseline pattern-helper finite-state-machine module that is
not consumable by the atomic-swap subsystem.

## 14.1 Executive Summary

The baseline tree contains a single in-process finite-state-machine
helper module living under the shared-utilities crate as one of
several pattern-style helpers (chapter-bound identifier:
`common::patterns::state_machine`). The baseline helper provides only
a basic asynchronous run loop with compile-time transition validation
through negative trait implementations; it carries no persistence,
no event sourcing, no reentrancy story, no recovery surface, and is
not consumed by the atomic-swap subsystem.

The substrate bound by this chapter introduces a dedicated workspace
crate (chapter-bound identifier: `mm2_state_machine`) which:

- promotes the basic finite-state-machine contract to a public
  reusable framework with stable trait surface;
- layers a *storable* variant on top that binds an event-sourced
  transition model, a per-machine reentrancy lock, a recovery
  surface, and a marker-trait separation between standard and
  storable machines.

The storable variant is the substrate the atomic-swap version-two
state machines depend on (chapters 15 and 17). The framework is
also the only crate-scope consumer of the unstable Rust auto-trait
and negative-implementation language features in the substrate;
the chapter-04 compiler-bootstrap allowlist extends to it (V3).

This chapter binds the crate boundary, the basic finite-state-machine
contract, the compile-time transition validator, the storable
extension, the recovery surface, and the runtime invariants the
design relies on. Bound rules R1–R5 cover the crate boundary;
R6–R12 cover the basic finite-state machine; R13–R16 cover the
transition validator; R17–R26 cover the storable layer; R27–R30
cover the recovery surface and invariants.

## 14.2 Subsystem Shape

The substrate occupies a structural seam between three subsystems:

- the shared-utilities subsystem (the basic helper that the
  substrate supersedes lives in the shared-utilities crate and is
  retained by the substrate for use by callers that do not need
  persistence);
- the atomic-swap subsystem (the version-two state machines depend
  on the storable variant);
- the persistence subsystem (the storable variant defines an
  opaque storage-backend trait the persistence subsystem
  implements per target — native SQLite, browser IndexedDB,
  in-memory test mocks).

The substrate does *not* mandate any specific storage backend.
The persistence subsystem implements the backend trait; the
substrate consumes it through bounded trait methods only.

## 14.3 Bound Crate Boundary

**R1.** The substrate MUST be a single crate. The chapter-bound
identifier is `mm2_state_machine`. The crate MUST be structured
into exactly two source modules:

| Module                    | Bound responsibility                           |
| ------------------------- | ---------------------------------------------- |
| `state_machine`           | Basic finite-state-machine contract and run loop. |
| `storable_state_machine`  | Storable extension with event sourcing and recovery. |

A library-root module MUST host the auto-trait used by the
transition validator (R13) and re-export both submodules. A
prelude module MUST re-export the basic trait surface for
ergonomic consumption.

**R2.** The crate MUST declare exactly one runtime dependency: the
asynchronous-trait procedural-macro crate. All other behaviour
MUST be expressed with the standard library and the substrate's
own types. Tests MAY use the shared-utilities crate as a
development dependency for an asynchronous-runtime adapter.

**R3.** The crate MUST enable the unstable Rust language features
`auto_traits` and `negative_impls` at its library root. These
features are needed by exactly two constructs (the auto-trait of
R13 and the negative implementations of R14). The crate MUST be
listed in the compiler-bootstrap allowlist documented in chapter
03 so the substrate compiles on the bound stable toolchain
without a nightly compiler.

**R4.** The crate's documentation-test target MUST be disabled.
Trait surfaces in this substrate are not directly invocable from
documentation snippets without consumer-provided concrete
implementations; enabling documentation tests would force
substrate-internal mock types to be public.

**R5.** The baseline-shared in-process helper under
`common::patterns::state_machine` MUST be retained in place after
the substrate lands. Its consumers — small in-process flows
distinct from the swap subsystem — MUST NOT be migrated as part
of the substrate's introduction. Migration is deferred (D1).

## 14.4 Bound Basic Finite-State-Machine Contract

**R6.** The basic contract MUST consist of exactly the following
six items (no additional public types in the basic module):

| Item                              | Bound role                                      |
| --------------------------------- | ----------------------------------------------- |
| Trait `StateMachineTrait`         | Run-loop owner with associated `Result` and `Error`. |
| Trait `State`                     | Non-terminal state; produces a `StateResult` on entry. |
| Trait `LastState`                 | Terminal state; produces the machine's `Result` on entry. |
| Marker trait `StandardStateMachine` | Tag distinguishing basic machines from storable ones (R26). |
| Enumeration `StateResult`         | Three-arm transition outcome (R7).               |
| Guard structs `ChangeGuard`, `ResultGuard`, `ErrorGuard` | Crate-private constructors gating the outcome arms (R8). |

**R7.** The `StateResult` enumeration MUST have exactly three
variants, each carrying one of the bound guard structs:

- a change-state variant, carrying a boxed next-state guard;
- a finish variant, carrying a result-guard wrapping the
  machine's associated `Result` type;
- an error variant, carrying an error-guard wrapping the
  machine's associated `Error` type.

**R8.** All three guard structs MUST be publicly visible but MUST
have private or crate-private constructors. External states MUST
NOT be able to hand-construct a change-state guard; they MUST go
through the transition helpers (R10, R23, R24) so the
compile-time transition validator (R13–R16) is enforced at every
call site.

**R9.** The bound run loop on `StateMachineTrait` MUST:

1. call `on_start` once and propagate its error;
2. loop: invoke the current state's entry method, then match the
   returned `StateResult` arm — change-state replaces the current
   state with the guard's next-state and continues; finish calls
   `on_finished`, propagates its error, then returns the wrapped
   result; error returns the wrapped error verbatim.

The current-state slot MUST be a boxed trait object so a single
monomorphisation of the run loop drives every state graph in the
workspace.

**R10.** The basic transition helper (chapter-bound identifier
`ChangeStateExt`) MUST be a trait with one associated method
`change_state(next)` that:

- requires `Next: State + TransitionFrom<Self>` (R13–R16);
- wraps the next state in a change-state guard and returns the
  change-state arm of `StateResult`.

The helper MUST be blanket-implemented for every state whose
owning machine implements the standard marker (R6) but MUST NOT
be implementable for storable machines (R26).

**R11.** A blanket implementation MUST make every `LastState`
automatically a `State` whose entry method returns the finish
arm wrapping the terminal value. Consumers MUST NOT have to
implement both traits.

**R12.** `on_start` and `on_finished` on `StateMachineTrait` MUST
have default implementations returning success without side
effects. Consumers override them only when needed (the storable
variant of R26 overrides both).

## 14.5 Bound Compile-Time Transition Validator

**R13.** The library root MUST declare an auto-trait
`NotSame`, with one bound impossibility implementation `!NotSame
for (X, X)`. This is the only place the substrate consumes the
auto-trait language feature.

**R14.** The basic module MUST declare a marker trait
`TransitionFrom<Prev>` with exactly two negative implementations:

- *self-transition forbidden:* `!TransitionFrom<T> for T` for all
  `T`;
- *transition out of terminal forbidden:* `!TransitionFrom<T>
  for Next` where `T: LastState` and `(T, Next): NotSame`. The
  not-same bound is required so this impl does not overlap the
  self-transition rule when `T = Next`.

**R15.** State authors MUST declare allowed transitions by
writing explicit positive implementations of `TransitionFrom`
between concrete state types. The compiler MUST reject every
unauthorised transition at the call site of the helper from R10
(or its storable equivalents from R23, R24).

**R16.** The substrate MUST NOT expose any escape hatch that lets
a transition be enacted without satisfying the validator. The
crate-private guard constructors (R8) are the only path through
which a change-state arm of `StateResult` can be produced.

## 14.6 Bound Storable Extension

**R17.** The storable extension MUST be entirely contained in a
single source module. It MUST define exactly the following public
items (no additional public types in the module):

| Item                                | Bound role                                   |
| ----------------------------------- | -------------------------------------------- |
| Trait `StateMachineDbRepr`          | The on-storage representation; carries the typed event log via `add_event`. |
| Trait `StateMachineStorage`         | The opaque backend contract (R19).            |
| Trait `StorableStateMachine`        | The storable machine contract (R20).         |
| Trait `InitialState`                | Marker for the run's first state (R22).      |
| Trait `StorableState`               | Non-initial state that produces an event on entry (R22). |
| Trait `RestoredState`               | Resumable state recovered from the event log (R27). |
| Trait `OnNewState<S>`               | Bridge between a machine and its next state; carries the per-transition persistence step (R25). |
| Trait `ChangeStateOnNewExt`         | Storable transition helper (R23).            |
| Trait `ChangeInitialStateExt`       | Initial-state transition helper (R24).       |
| Struct `RestoredMachine<M>`         | Recovery handle (R28).                      |

**R18.** The on-storage representation trait MUST expose exactly
one method `add_event(event)` plus an associated `Event` type.
The substrate makes no further demand on the representation; its
shape is consumer-defined.

**R19.** The backend trait MUST expose exactly six asynchronous
methods, each parameterised by the storable machine's associated
identifier and representation types:

| Method                    | Bound semantics                                        |
| ------------------------- | ------------------------------------------------------ |
| `store_repr(id, repr)`    | Persist the initial representation for a new machine.   |
| `get_repr(id)`            | Read back the representation for a known machine.       |
| `has_record_for(id)`      | Existence check used by the storable run-start path (R26). |
| `store_event(id, event)`  | Append one event to the named machine's log.            |
| `get_unfinished()`        | Enumerate machine identifiers whose runs are not marked finished. |
| `mark_finished(id)`       | Mark a machine's run as terminated.                     |

The substrate MUST NOT add side channels: every persistence
effect MUST go through one of these six methods.

**R20.** The storable-machine trait MUST expose exactly the
following associated items: an associated `Storage` (R19),
`Result`, `Error` (bound to be constructible from the storage
error type via the `From` trait), `ReentrancyLock`, `RecreateCtx`,
and `RecreateError`. The trait MUST expose four synchronous
accessors (`to_db_repr`, `storage`, `id`, plus the two
context-lifecycle hooks `init_additional_context` /
`clean_up_context`), four asynchronous control methods
(`recreate_machine`, `store_event`, `mark_finished`,
`acquire_reentrancy_lock`), one synchronous spawn hook
(`spawn_reentrancy_lock_renew`), and two notification hooks
(`on_event` for live transitions, `on_kickstart_event` for
recovery — these MUST be distinct methods so consumers can
differentiate first-time entry from resume).

**R21.** The storable-machine trait's associated error type MUST
satisfy the bound `From<<Storage as StateMachineStorage>::Error>`.
Any storage-layer failure MUST surface unmodified to the machine's
error path; the substrate MUST NOT wrap or transform storage
errors.

**R22.** The substrate MUST define two non-overlapping markers
on storable states:

- `InitialState`, implemented by the very first state of a run
  (which has not yet produced an event);
- `StorableState`, implemented by every non-initial storable
  state, exposing `get_event()` that produces the typed event for
  the state's entry.

Non-overlap MUST be enforced with a negative implementation
`!InitialState for T where T: StorableState`. Removing this
negative implementation would make the helpers of R23 and R24
overlapping and break method resolution.

## 14.7 Bound Storable Transition Helpers and Auto-Impl

**R23.** The storable transition helper for non-initial transitions
(chapter-bound identifier `ChangeStateOnNewExt`) MUST be a trait
with one asynchronous method `change_state(next, machine)`. The
method MUST:

1. require `Next: State + TransitionFrom<Self> + ChangeStateOnNewExt`
   and `Next::StateMachine: OnNewState<Next> + Sync`;
2. call `machine.on_new_state(&next).await` *before* wrapping the
   next state;
3. on success, wrap the next state in a change-state guard and
   return the change-state arm of `StateResult`;
4. on failure, short-circuit into the error arm of `StateResult`,
   propagating the storage error through the machine's `Error`
   type.

The helper MUST be blanket-implemented for every storable state.

**R24.** The initial-state transition helper (chapter-bound
identifier `ChangeInitialStateExt`) MUST have the same shape as
R23 but bind `Self: InitialState`. It MUST be blanket-implemented
for every initial state.

**R25.** The `OnNewState<S>` trait MUST be blanket-implemented for
every `(StorableStateMachine, StorableState)` pair. The blanket
implementation MUST:

1. call `state.get_event()` to obtain the per-transition event;
2. invoke the machine's `on_event(&event)` synchronous
   notification hook;
3. invoke the asynchronous `store_event(event)` for persistence;
4. propagate the storage error through the machine's `Error`
   type.

This is the only place per-transition events are written to
storage during a live run. Bypassing this auto-impl is the
defect that R26 exists to prevent.

**R26.** The substrate MUST contain a single negative
implementation `!StandardStateMachine for T where T:
StorableStateMachine`. This forbids the basic transition helper
(R10) from being callable inside a storable machine's state. A
storable machine's `StateMachineTrait` implementation MUST be
provided automatically by a blanket impl that wraps the storable
contract: `on_start` MUST acquire the reentrancy lock, store the
initial representation if no prior record exists, spawn the
reentrancy renew hook, and call the additional-context init;
`on_finished` MUST call `mark_finished` and then clean up
additional context.

## 14.8 Bound Recovery Surface

**R27.** The `RestoredState` trait MUST extend `StorableState +
Send` and add exactly one method `into_state` that converts the
boxed restored state into a boxed `State` over the same machine.
The substrate MUST NOT expose any other path for converting a
restored state into a runnable state.

**R28.** The recovery handle `RestoredMachine<M>` MUST expose
exactly two methods:

- a public constructor wrapping a fully-built machine;
- an asynchronous `kickstart(from_state)` method that:
  1. calls `from_state.get_event()` to obtain the entry-event for
     the resumed state;
  2. invokes the machine's `on_kickstart_event(event)` hook *and
     not* `on_event`;
  3. does *not* call `store_event` (the event is already in the
     log from the original execution);
  4. invokes the machine's run loop with the converted state.

**R29.** The bound recovery contract MUST satisfy three structural
properties:

| Property                                    | Bound consequence                                |
| ------------------------------------------- | ------------------------------------------------ |
| Notification distinction                    | `on_kickstart_event` MUST be a separate method from `on_event` so consumers can distinguish first-time entry from resume (the version-two swap path uses this distinction to suppress message re-broadcasts). |
| No persistence on resume entry              | The resume entry MUST NOT be re-persisted; a duplicate in the log would corrupt downstream consumers that count entries. |
| Symmetric run-loop continuation             | Once `kickstart` has invoked `run`, all subsequent transitions MUST follow the live path (R23 / R25) — there is no further special-case behaviour after the entry hook. |

**R30.** The substrate MUST expose `recreate_machine` on the
storable-machine trait (R20) so consumers materialise a
`RestoredMachine` plus the boxed restored state from a stored
representation plus the caller-supplied recreate-context. The
recreate-context carries runtime handles that cannot be
serialised (coin handles, peer-to-peer channels). The
recreate-error is separate from the machine's run-time error
type, since recreation can fail before the machine is alive.

## 14.9 Tests

**T1.** *Standard run.* A basic standard finite-state-machine
runs through a four-state authentication graph
(`Reading → Parsing → Authenticating → Authenticated`) over
well-formed credentials; the test asserts the run loop returns
the success result.

**T2.** *Standard bad-format termination.* The same machine runs
on credentials missing the required separator; the test asserts
the run loop terminates in a bad-format failure variant.

**T3.** *Standard unknown-credential termination.* The same
machine runs on credentials whose `(login, password)` pair is
not in the in-memory credential table; the test asserts the run
loop terminates in an unknown-credential failure variant.

**T4.** *Storable full run.* A storable machine traverses a
linear four-state graph against a test storage mock; the test
asserts the resulting event log is exactly the three transition
events (one per non-initial state) and that the machine is
marked finished.

**T5.** *Storable restore and resume.* The storage mock is
pre-populated with the first transition event; `recreate_machine`
plus `kickstart` is invoked; the test asserts the final event log
contains exactly three entries (the pre-existing entry plus the
two emitted on resume) — i.e. the resume entry is *not*
duplicated.

## 14.10 Deferred Work

**D1.** Migration of the baseline-shared in-process helper's
consumers (small non-swap flows) to the new crate. The helper
remains in the shared-utilities crate at substrate landing time
(R5); migration is a follow-on refactor without surface change.

**D2.** Stabilisation of the unstable Rust language features the
substrate depends on. When `auto_traits` and `negative_impls`
stabilise, the bootstrap allowlist entry for the crate (R3) can
be dropped without source changes.

**D3.** A built-in reentrancy-lock implementation. The substrate
currently exposes the lock as an associated type the consumer
chooses (R20). Standard lock implementations against the bound
storage backends are deferred.

**D4.** A bounded built-in event-log retention policy. The
substrate persists every transition event indefinitely; pruning
of completed runs is deferred to consumers.

## 14.11 Baseline Verifications

**V1.** The baseline tree MUST be confirmed to contain no crate
named `mm2_state_machine`. The only finite-state-machine helper
in the baseline lives under the shared-utilities crate as
`common::patterns::state_machine` and is structurally inadequate
for the bound storable surface (no persistence, no event
sourcing, no reentrancy, no recovery).

**V2.** The baseline tree MUST be confirmed to expose no
event-sourced or kickstart-capable finite-state-machine
infrastructure under any other crate. The atomic-swap subsystem
at baseline drives swaps via hand-rolled control flow with no
generic resume contract.

**V3.** The compiler-bootstrap allowlist bound in chapter 03 MUST
include the new substrate-crate identifier exactly so the bound
auto-trait and negative-implementation usage (R13, R14, R22, R26)
compile on the bound stable toolchain.

## 14.12 External References

- Rust language reference, *auto traits* (unstable feature
  documentation),
  <https://doc.rust-lang.org/unstable-book/language-features/auto-traits.html>.
- Rust language reference, *negative implementations* (unstable
  feature documentation),
  <https://doc.rust-lang.org/unstable-book/language-features/negative-impls.html>.
- `async-trait` procedural-macro crate,
  <https://crates.io/crates/async-trait> — the substrate's sole
  runtime dependency (R2).

## 14.13 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit; chapter 01 (clean-room rules); chapter 03 (compiler
  bootstrap allowlist that gates V3); chapter 13 (the swap
  version-negotiation predicate that routes into the storable
  substrate); chapters 15 and 17 (the consumers of the storable
  variant); public Rust language documentation for the two
  unstable language features the substrate consumes; public
  documentation for the asynchronous-trait procedural-macro
  crate.
- *Permitted-input classes used:* baseline source; bound
  substrate identifiers introduced with in-chapter justification;
  public language documentation; public crate documentation.
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* not consulted.
