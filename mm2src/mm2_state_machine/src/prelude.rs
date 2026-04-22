pub use crate::state_machine::{ChangeStateExt, LastState, State, StateMachineTrait, StateResult};

/// Marker declaring which state can follow which. Implement on the *destination* state:
///
/// ```ignore
/// impl TransitionFrom<Parsing> for Authentication {}
/// ```
///
/// The compiler enforces that:
/// - A state cannot transition to itself (`impl<T> !TransitionFrom<T> for T`)  
/// - A terminal (`LastState`) cannot be a source for further transitions
pub trait TransitionFrom<Prev> {}

/// Marker for basic (non-storable) state machines.
/// Implementing this enables `ChangeStateExt::change_state` for all states of the machine.
/// `StorableStateMachine` impls are prevented from also implementing this.
pub trait StandardStateMachine {}
