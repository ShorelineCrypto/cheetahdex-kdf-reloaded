# Chapter 14 — Generic State-Machine Runtime

## Executive Summary

At the baseline tree there is one in-tree state-machine helper —
`common::patterns::state_machine` (a ~290-line module under
`mm2src/common/patterns/`) — which provides a basic `StateMachineTrait`
+ `State` + `LastState` async driver with compile-time
transition-validation via negative trait impls on `TransitionFrom`.
There is no persistence, no event sourcing, no recovery story, and
no marker separating "stateless" from "storable" machines. The
atomic-swap protocol does not use this helper at the baseline at
all; swaps are driven by hand-rolled control flow in
`maker_swap.rs` / `taker_swap.rs`.

The post-baseline tree introduces a **dedicated workspace crate**,
`mm2_state_machine/`, that promotes the basic FSM contract to a
public, reusable framework and layers a **storable** variant on top
of it. The storable variant adds:

- An **event-sourced** transition model — every state change
  produces a typed event that is appended to a per-machine
  event log in the storage backend.
- A **reentrancy lock** that prevents the same machine from being
  driven by two tasks concurrently (e.g. a kickstart loop and a
  live RPC both deciding to advance the same swap).
- A **recovery surface** — `RestoredMachine::kickstart(from_state)`
  — that rebuilds a live machine from its persisted events and
  resumes execution at the recovered state.
- A **marker-trait separation** between "standard" (`StandardStateMachine`)
  and "storable" (`StorableStateMachine`) machines, with a negative
  impl that forbids any single type from being both, so the two
  transition helpers (`ChangeStateExt` vs `ChangeStateOnNewExt`)
  cannot be mixed at a call site.

The chapter documents the new crate's two source files
(`state_machine.rs`, `storable_state_machine.rs`), the public
trait surface (six core traits plus three transition-helper
traits), the compile-time transition-validation rules, the
storage-backend contract, the reentrancy model, the recovery
surface, and the five unit tests that lock the behaviour.

### Why this changed

The state-machine framework was introduced by the project's own
commit `a703da93b` (*feat: P5.1 mm2_state_machine generic async
FSM framework*). The commit message states the design verbatim:

> *Basic FSM: StateMachineTrait + State + LastState with async run
> loop. Storable FSM: StorableStateMachine with event sourcing, DB
> persistence, reentrancy locks, and recovery via
> RestoredMachine::kickstart(). Compile-time transition validation
> via negative trait impls on TransitionFrom (self-transitions
> blocked, transitions from terminal states blocked).
> StandardStateMachine vs StorableStateMachine marker separation
> prevents mixing basic ChangeStateExt with storable
> ChangeStateOnNewExt. 5 tests: basic auth FSM (success/error/bad-
> format), storable full-run, storable restore-and-resume.*

In clean-room voice: the post-baseline project chose to extract the
existing `common::patterns::state_machine` shape into a dedicated
crate, leave that crate as a standalone reusable framework, and
layer a persistence-aware variant on top so the V2 atomic-swap
state machines (chapters 15 and 17) could be expressed as
straightforward `State` impls with the event-sourcing, reentrancy,
and recovery responsibilities handled once by the framework rather
than copy-pasted across maker and taker code. The crate is the
only place in the post-baseline tree that depends on the unstable
`auto_traits` + `negative_impls` Rust features (enabled via the
workspace-wide `RUSTC_BOOTSTRAP` allowlist documented in chapter 04),
because those features are what make compile-time transition
validation possible without macro magic.

## Reproduction Detail

### 14.1 Baseline shape (one in-process FSM helper, no persistence)

At commit `c1d46c0…` the only FSM helper in the workspace is
`mm2src/common/patterns/state_machine.rs` (~290 lines), which
provides:

- A `StateMachineTrait` with associated `Result` / `Error` types
  and an async `run(initial_state)` loop.
- A `State` trait whose `on_changed` returns a `StateResult`
  variant (`ChangeState` / `Finish` / `Error`).
- A `LastState` trait for terminal states.
- A `TransitionFrom<Prev>` marker trait + the two negative impls
  (`!TransitionFrom<T> for T`, `!TransitionFrom<LastState>`).
- A `ChangeStateExt::change_state(next)` helper.

The helper has **no** persistence, **no** event log, **no**
reentrancy concept, and **no** kickstart/recovery surface. It is
not consumed by the swap path at the baseline; it is used only by
small in-process flows (the file lives under `common/patterns/`
alongside other pattern-style helpers).

### 14.2 The new `mm2_state_machine` crate

The post-baseline tree adds a workspace member
`mm2src/mm2_state_machine/` with this layout:

```
mm2src/mm2_state_machine/
├── Cargo.toml                    # async-trait dep only; common is dev-dep
├── src/
│   ├── lib.rs                    # NotSame auto-trait + module roots
│   ├── prelude.rs                # TransitionFrom + StandardStateMachine markers
│   ├── state_machine.rs          # 296 lines — basic FSM
│   └── storable_state_machine.rs # 454 lines — storable FSM extension
```

`Cargo.toml` declares one runtime dependency, `async-trait = "0.1"`,
and one dev-dependency, `common` (for `block_on` / `executor::spawn`
in tests). `doctest = false` is set on the library.

`lib.rs` enables two unstable Rust features that the framework
depends on:

```rust
#![feature(negative_impls, auto_traits)]
```

These are needed for the `NotSame` auto-trait and the negative
`!TransitionFrom` impls (§14.4). Compilation on stable Rust is
made possible by the workspace's `.cargo/config.toml`
`RUSTC_BOOTSTRAP` allowlist, which includes `mm2_state_machine`
alongside `common`, `mm2_err_handle`, `docker_tests`, `mocktopus`,
and `mocktopus_macros`. The allowlist is documented in chapter 04
as the same mechanism that keeps a small number of pre-stabilisation
auto-trait users compiling on the modern toolchain; the
state-machine crate is the newest member of that allowlist.

`prelude.rs` re-exports the basic trait surface for ergonomic
consumption (`use mm2_state_machine::prelude::*;`).

### 14.3 The basic FSM (`state_machine.rs`)

The basic FSM contract is six items plus three guard types.

#### Core traits

```rust
#[async_trait]
pub trait StateMachineTrait: Send + Sized + 'static {
    type Result: Send;
    type Error: Send;

    async fn on_start(&mut self) -> Result<(), Self::Error> { Ok(()) }
    async fn on_finished(&mut self) -> Result<(), Self::Error> { Ok(()) }

    async fn run(&mut self,
                 mut state: Box<dyn State<StateMachine = Self>>)
        -> Result<Self::Result, Self::Error>;
}

#[async_trait]
pub trait State: Send + Sync + 'static {
    type StateMachine: StateMachineTrait;
    async fn on_changed(self: Box<Self>,
                        sm: &mut Self::StateMachine)
        -> StateResult<Self::StateMachine>;
}

#[async_trait]
pub trait LastState: Send + Sync + 'static {
    type StateMachine: StateMachineTrait;
    async fn on_changed(self: Box<Self>,
                        ctx: &mut Self::StateMachine)
        -> <Self::StateMachine as StateMachineTrait>::Result;
}

pub trait StandardStateMachine {}
```

The `LastState` blanket impl makes every terminal state
automatically a `State` whose `on_changed` returns
`StateResult::Finish`:

```rust
#[async_trait]
impl<T: LastState> State for T {
    type StateMachine = T::StateMachine;
    async fn on_changed(self: Box<Self>, ctx: &mut T::StateMachine)
        -> StateResult<T::StateMachine>
    {
        let result = LastState::on_changed(self, ctx).await;
        StateResult::Finish(ResultGuard::new(result))
    }
}
```

#### The run loop

```rust
async fn run(&mut self, mut state: Box<dyn State<StateMachine = Self>>) -> Result<Self::Result, Self::Error> {
    self.on_start().await?;
    loop {
        match state.on_changed(self).await {
            StateResult::ChangeState(ChangeGuard { next }) => state = next,
            StateResult::Finish(ResultGuard { result })    => {
                self.on_finished().await?;
                return Ok(result);
            }
            StateResult::Error(ErrorGuard { error })       => return Err(error),
        }
    }
}
```

The loop is monomorphisation-free over states: the dynamic
dispatch through `Box<dyn State<…>>` is what makes a single `run`
implementation drive every state-graph in the workspace.

#### Guard types

```rust
pub enum StateResult<Machine: StateMachineTrait> {
    ChangeState(ChangeGuard<Machine>),
    Finish(ResultGuard<Machine::Result>),
    Error(ErrorGuard<Machine::Error>),
}

pub struct ChangeGuard<Machine: StateMachineTrait> {
    next: Box<dyn State<StateMachine = Machine>>,
}
pub struct ResultGuard<T> { result: T }
pub struct ErrorGuard<E>  { error: E }
```

The three guard types are deliberately constructed only inside
the crate (`pub(crate)` constructors on `ChangeGuard` and
`ErrorGuard`, private `fn new` on `ResultGuard`). External states
cannot bypass the transition checker by hand-constructing a
`ChangeGuard`; they must go through `ChangeStateExt::change_state`
(basic) or `ChangeStateOnNewExt::change_state` (storable), both of
which carry the `TransitionFrom` bound that gives compile-time
validation.

#### Transition helper

```rust
pub trait ChangeStateExt {
    fn change_state<Next>(next_state: Next) -> StateResult<Next::StateMachine>
    where
        Self: Sized,
        Next: State + TransitionFrom<Self>,
    { StateResult::ChangeState(ChangeGuard::next(next_state)) }
}

impl<S: StandardStateMachine, T: State<StateMachine = S>> ChangeStateExt for T {}
```

The `StandardStateMachine` bound on the blanket impl is what
prevents `ChangeStateExt::change_state` from being callable in a
storable-machine state; for storable machines the analogous
helper is `ChangeStateOnNewExt::change_state` (§14.5).

### 14.4 Compile-time transition validation

The validator is built from three pieces in `lib.rs` and
`prelude.rs`:

```rust
// lib.rs
pub auto trait NotSame {}
impl<X> !NotSame for (X, X) {}

// prelude.rs
pub trait TransitionFrom<Prev> {}

// state_machine.rs
impl<T, Next> !TransitionFrom<T> for Next
where
    T: LastState,
    (T, Next): NotSame,
{}

impl<T> !TransitionFrom<T> for T {}
```

Effect:

- A state author declares allowed transitions by writing
  `impl TransitionFrom<Source> for Destination {}`.
- The compiler rejects `impl TransitionFrom<X> for X {}` (self-
  transition) via the second negative impl.
- The compiler rejects any `impl TransitionFrom<Terminal> for X`
  where `Terminal: LastState` (transition out of a terminal state)
  via the first negative impl. The `(T, Next): NotSame` bound is
  required so the impl does not overlap the self-transition rule
  for the case `T = Next`.
- A `change_state::<Next>()` call fails to compile when
  `TransitionFrom<Self>` is not implemented for `Next`.

The `NotSame` auto-trait is the only place in the framework that
needs `auto_traits`; the negative `!TransitionFrom` impls are the
only place that needs `negative_impls`. Both features are nightly-
only, hence the per-crate `RUSTC_BOOTSTRAP` enable in
`.cargo/config.toml`.

### 14.5 The storable FSM (`storable_state_machine.rs`)

The storable layer extends the basic one along three axes:
**event sourcing**, **persistence**, and **recovery**.

#### Storage contract

```rust
pub trait StateMachineDbRepr: Send {
    type Event: Send;
    fn add_event(&mut self, event: Self::Event);
}

#[async_trait]
pub trait StateMachineStorage: Send + Sync {
    type MachineId: Send + Sync;
    type DbRepr: StateMachineDbRepr;
    type Error: Send;

    async fn store_repr(&mut self, id: Self::MachineId, repr: Self::DbRepr)         -> Result<(), Self::Error>;
    async fn get_repr(&self, id: Self::MachineId)                                   -> Result<Self::DbRepr, Self::Error>;
    async fn has_record_for(&mut self, id: &Self::MachineId)                        -> Result<bool, Self::Error>;
    async fn store_event(&mut self,
                         id: Self::MachineId,
                         event: <Self::DbRepr as StateMachineDbRepr>::Event)        -> Result<(), Self::Error>;
    async fn get_unfinished(&self)                                                  -> Result<Vec<Self::MachineId>, Self::Error>;
    async fn mark_finished(&mut self, id: Self::MachineId)                          -> Result<(), Self::Error>;
}
```

The framework imposes no opinion on the backend; SQLite, IndexedDB,
and in-memory test mocks are all implementations the consumer
supplies. The chapter-15 (UTXO V2) and chapter-17 (EVM V2) paths
provide concrete backends.

#### State contract

```rust
pub trait InitialState {
    type StateMachine: StorableStateMachine;
}

pub trait StorableState {
    type StateMachine: StorableStateMachine;
    fn get_event(&self)
        -> <<<Self::StateMachine as StorableStateMachine>::Storage
                as StateMachineStorage>::DbRepr
                as StateMachineDbRepr>::Event;
}

pub trait RestoredState: StorableState + Send {
    fn into_state(self: Box<Self>)
        -> Box<dyn State<StateMachine = Self::StateMachine>>;
}
```

A `StorableState` declares the event it produces upon entry. The
framework's `OnNewState` auto-impl (below) calls `get_event()`
just before the transition is enacted, persists it via
`store_event`, and notifies the machine via `on_event`. The
`InitialState` marker is for the very first state of the run,
which has not yet produced an event and therefore does not
implement `StorableState`; a negative impl
(`impl<T: StorableState> !InitialState for T`) makes the
distinction non-overlapping.

#### Machine contract

```rust
#[async_trait]
pub trait StorableStateMachine: Send + Sync + Sized + 'static {
    type Storage: StateMachineStorage;
    type Result: Send;
    type Error: From<<Self::Storage as StateMachineStorage>::Error> + Send;
    type ReentrancyLock: Send;
    type RecreateCtx: Send;
    type RecreateError: Send;

    fn to_db_repr(&self) -> <Self::Storage as StateMachineStorage>::DbRepr;
    fn storage(&mut self) -> &mut Self::Storage;
    fn id(&self) -> <Self::Storage as StateMachineStorage>::MachineId;

    async fn recreate_machine(
        id: <Self::Storage as StateMachineStorage>::MachineId,
        storage: Self::Storage,
        repr: <Self::Storage as StateMachineStorage>::DbRepr,
        from_repr_ctx: Self::RecreateCtx,
    ) -> Result<(RestoredMachine<Self>,
                 Box<dyn RestoredState<StateMachine = Self>>),
                Self::RecreateError>;

    async fn store_event(&mut self, event: …) -> Result<…, …>;
    async fn mark_finished(&mut self)         -> Result<…, …>;
    async fn acquire_reentrancy_lock(&self)   -> Result<Self::ReentrancyLock, Self::Error>;
    fn spawn_reentrancy_lock_renew(&mut self, guard: Self::ReentrancyLock);

    fn init_additional_context(&mut self);
    fn clean_up_context(&mut self);

    fn on_event(&mut self,
                event: &<<Self::Storage as StateMachineStorage>::DbRepr as StateMachineDbRepr>::Event);
    fn on_kickstart_event(&mut self,
                event: <<Self::Storage as StateMachineStorage>::DbRepr as StateMachineDbRepr>::Event);
}
```

#### Marker separation

```rust
impl<T: StorableStateMachine> !StandardStateMachine for T {}
```

This is the negative impl that prevents the basic
`ChangeStateExt::change_state` helper from being callable inside a
storable machine's state — if it were, transitions would skip
event persistence and the recovery surface would silently drift
from the live machine.

#### StateMachineTrait blanket impl

The crate provides `StateMachineTrait` for every
`StorableStateMachine` automatically:

```rust
#[async_trait]
impl<T: StorableStateMachine> StateMachineTrait for T {
    type Result = T::Result;
    type Error  = T::Error;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        let reentrancy_lock = self.acquire_reentrancy_lock().await?;
        let id = self.id();
        if !self.storage().has_record_for(&id).await? {
            let repr = self.to_db_repr();
            self.storage().store_repr(id, repr).await?;
        }
        self.spawn_reentrancy_lock_renew(reentrancy_lock);
        self.init_additional_context();
        Ok(())
    }

    async fn on_finished(&mut self) -> Result<(), T::Error> {
        self.mark_finished().await?;
        self.clean_up_context();
        Ok(())
    }
}
```

So a `StorableStateMachine` author writes only the storable-
specific methods; the basic `on_start`/`on_finished` hooks are
filled in by the framework with the persistence and reentrancy
sequencing baked in.

#### Transition helpers

```rust
#[async_trait]
pub trait ChangeStateOnNewExt {
    async fn change_state<Next>(next_state: Next, machine: &mut Next::StateMachine) -> StateResult<Next::StateMachine>
    where
        Self: Sized,
        Next: State + TransitionFrom<Self> + ChangeStateOnNewExt,
        Next::StateMachine: OnNewState<Next> + Sync;
}

impl<M: StorableStateMachine, T: StorableState<StateMachine = M>> ChangeStateOnNewExt for T {}

#[async_trait]
pub trait ChangeInitialStateExt: InitialState { /* same shape as above */ }

impl<M: StorableStateMachine, T: InitialState<StateMachine = M>> ChangeInitialStateExt for T {}
```

Both helpers funnel through a private `change_state_impl` that
performs `machine.on_new_state(&next_state).await` (which calls
`on_event` and persists via `store_event`) before wrapping the
state in a `ChangeGuard`. Failure to persist short-circuits to
`StateResult::Error`, which surfaces to the caller as the swap's
own error type via `T::Error: From<…StorageError>`.

The auto-impl of `OnNewState` for any
`(StorableStateMachine, StorableState)` pair ties everything
together:

```rust
#[async_trait]
impl<T: StorableStateMachine + Sync, S: StorableState<StateMachine = T> + Sync>
     OnNewState<S> for T
{
    async fn on_new_state(&mut self, state: &S) -> Result<(), T::Error> {
        let event = state.get_event();
        self.on_event(&event);
        Ok(self.store_event(event).await?)
    }
}
```

### 14.6 Recovery surface (`RestoredMachine::kickstart`)

```rust
pub struct RestoredMachine<M: StorableStateMachine> { machine: M }

impl<M: StorableStateMachine> RestoredMachine<M> {
    pub fn new(machine: M) -> Self { RestoredMachine { machine } }

    pub async fn kickstart(
        &mut self,
        from_state: Box<dyn RestoredState<StateMachine = M>>,
    ) -> Result<M::Result, M::Error> {
        let event = from_state.get_event();
        self.machine.on_kickstart_event(event);
        self.machine.run(from_state.into_state()).await
    }
}
```

The semantics differ from a normal transition:

- `on_kickstart_event` (not `on_event`) is called for the resumed
  state's event. This lets the machine distinguish between
  "advancing through this state for the first time" and
  "resuming at this state" — for example, the V2 swap path uses
  the distinction to skip re-broadcasting messages that were
  already broadcast before the crash.
- `store_event` is **not** called for the resume event — it is
  already in the log from the original execution.
- Execution then continues through the normal `run` loop.

The `recreate_machine` method on `StorableStateMachine` is what
the consumer implements to materialise a `RestoredMachine` plus
the `Box<dyn RestoredState>` from a stored `DbRepr` plus the
caller-supplied `RecreateCtx` (which carries any runtime handles
the machine needs but cannot be serialised — coin handles, P2P
channels, etc.).

### 14.7 Tests

The crate ships with five unit tests, all enumerated in the
introducing commit:

1. `test_basic_state_machine_success` — runs an auth FSM
   (`Reading → Parsing → Authenticating → Authenticated`) over
   well-formed credentials.
2. `test_basic_state_machine_bad_format` — same FSM, credentials
   without a space; ends in `Failed { BadCredentialFormat }`.
3. `test_basic_state_machine_unknown_user` — same FSM,
   credentials whose `(login, password)` pair is not in the
   in-memory user table; ends in `Failed { UnknownUser }`.
4. `test_storable_machine_full_run` — drives `A → B → C → D`
   with a `MockStorage` and asserts the event log contains
   `[EnteredB, EnteredC, EnteredD]` after `mark_finished`.
5. `test_storable_machine_restore_and_resume` — pre-populates
   `MockStorage::unfinished` with `[EnteredB]`, calls
   `recreate_machine` + `kickstart`, and asserts the final
   `finished` log is `[EnteredB, EnteredC, EnteredD]` — i.e.
   the resumed events are appended to, not duplicated over, the
   pre-existing log entry.

These are the contract; any reimplementation must keep them
passing without modification.

### 14.8 Invariants the design relies on

1. **Single transition helper per machine variant.** A given
   state's transition call must go through exactly one of
   `ChangeStateExt` (standard) or `ChangeStateOnNewExt` (storable).
   The negative impl `!StandardStateMachine for StorableStateMachine`
   guarantees this; do not weaken it.
2. **Initial state is non-storable.** The `!InitialState for StorableState`
   negative impl is what makes the `ChangeInitialStateExt` and
   `ChangeStateOnNewExt` blanket impls non-overlapping. Removing
   the negative impl would let both apply to the same state
   and break method resolution.
3. **`kickstart` does not persist.** The recovery surface
   intentionally bypasses `store_event` for the entry state.
   A caller that wants the resume to be visible in the event log
   must use a higher-level operation (e.g. emit a separate "resumed"
   event on the next storable transition).
4. **Reentrancy is the consumer's responsibility.** The framework
   acquires and renews a `ReentrancyLock` but does not enforce
   what "lock" means; the consumer (V2 UTXO/EVM swap paths) must
   make the lock collision-detectable across process restarts —
   typically via a row in the same storage backend with a TTL
   and a renew loop driven by `spawn_reentrancy_lock_renew`.

## External References

- *Rust language reference — `auto_traits`* (unstable). The
  `NotSame` auto-trait depends on it.
- *Rust language reference — `negative_impls`* (unstable). The
  `!TransitionFrom` impls depend on it.
- *async-trait crate v0.1* — the only runtime dependency of the
  crate.

## Provenance Footer

- **Inputs:** `01-clean-room-rules.md`; the baseline workspace at
  commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`; the public
  `async-trait` crate documentation; the Rust language reference
  for `auto_traits` and `negative_impls`; the project's own
  commit message `a703da93b`.
- **Sibling references:** chapter 04 (the
  `RUSTC_BOOTSTRAP` allowlist mechanism that lets the crate
  compile on stable Rust), chapter 13 (the `is_v2_or_higher`
  predicate that gates dispatch into the storable swap state
  machines), chapters 15 and 17 (the V2 UTXO and EVM swap
  implementations on top of this runtime).
- **Forbidden corpus:** not consulted.
- **Author of this chapter:** clean-room reimplementation working
  set, see
  `local/clean-room-doc/IMPLEMENTER_RULES.md`.
- **Reviewers:** two-pass review at
  `local/clean-room-doc/reviews/14-state-machine-runtime-r{1,2}.md`.
