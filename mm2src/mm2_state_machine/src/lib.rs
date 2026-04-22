//! Generic async state machine framework with compile-time transition validation.
//!
//! Provides two variants:
//! - **Basic** (`StateMachineTrait` + `StandardStateMachine`): stateless FSM execution
//! - **Storable** (`StorableStateMachine`): persistence-aware FSM with event sourcing,
//!   reentrancy locks, and recovery from stored state
//!
//! Compile-time safety: invalid transitions are rejected by the compiler via negative
//! trait impls on `TransitionFrom`. Self-transitions and transitions from terminal
//! states are both prevented.

#![feature(negative_impls, auto_traits)]

pub mod prelude;
pub mod state_machine;
pub mod storable_state_machine;

/// Auto trait used to distinguish distinct types at the type level.
/// Every type pair (A, B) where A != B satisfies `NotSame`.
/// The pair (X, X) does not, preventing self-referential bounds.
pub auto trait NotSame {}
impl<X> !NotSame for (X, X) {}
