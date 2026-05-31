//! Core state machine execution with compile-time transition validation.
//!
//! The async run loop drives states through `on_changed` until a terminal `LastState`
//! produces a result or an error short-circuits execution.

use crate::prelude::*;
use crate::NotSame;
use async_trait::async_trait;

/// The main trait every state machine must implement.
#[async_trait]
pub trait StateMachineTrait: Send + Sized + 'static {
    /// Value produced when the machine reaches a terminal state.
    type Result: Send;
    /// Error type that can abort execution from any state.
    type Error: Send;

    /// Called once before the first state is entered. Override for setup logic.
    async fn on_start(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Called after a terminal state produces its result. Override for teardown logic.
    async fn on_finished(&mut self) -> Result<(), Self::Error> { Ok(()) }

    /// Drives the state machine from `state` until completion or error.
    async fn run(&mut self, mut state: Box<dyn State<StateMachine = Self>>) -> Result<Self::Result, Self::Error> {
        self.on_start().await?;

        loop {
            match state.on_changed(self).await {
                StateResult::ChangeState(ChangeGuard { next }) => {
                    state = next;
                },
                StateResult::Finish(ResultGuard { result }) => {
                    self.on_finished().await?;
                    return Ok(result);
                },
                StateResult::Error(ErrorGuard { error }) => return Err(error),
            }
        }
    }
}

// Prevent transitions *from* a terminal state — once you reach a LastState, there is no next.
impl<T, Next> !TransitionFrom<T> for Next
where
    T: LastState,
    (T, Next): NotSame,
{
}

// Prevent self-transitions — a state cannot transition to itself.
impl<T> !TransitionFrom<T> for T {}

/// Implemented by each non-terminal state. `on_changed` runs the state's logic and
/// returns the next transition via `ChangeStateExt::change_state`.
#[async_trait]
pub trait State: Send + Sync + 'static {
    type StateMachine: StateMachineTrait;

    async fn on_changed(self: Box<Self>, state_machine: &mut Self::StateMachine) -> StateResult<Self::StateMachine>;
}

/// Provides `change_state` for basic (non-storable) FSMs. The compiler validates the
/// transition at call site via `TransitionFrom<Self>` bound.
pub trait ChangeStateExt {
    fn change_state<Next>(next_state: Next) -> StateResult<Next::StateMachine>
    where
        Self: Sized,
        Next: State + TransitionFrom<Self>,
    {
        StateResult::ChangeState(ChangeGuard::next(next_state))
    }
}

// Only states belonging to a StandardStateMachine get ChangeStateExt.
// StorableStateMachine states use ChangeStateOnNewExt instead (which persists events).
impl<S: StandardStateMachine, T: State<StateMachine = S>> ChangeStateExt for T {}

/// Marker for terminal states. Produces the machine's `Result` and ends execution.
#[async_trait]
pub trait LastState: Send + Sync + 'static {
    type StateMachine: StateMachineTrait;

    async fn on_changed(
        self: Box<Self>,
        ctx: &mut Self::StateMachine,
    ) -> <Self::StateMachine as StateMachineTrait>::Result;
}

// Every LastState is automatically a State that yields StateResult::Finish.
#[async_trait]
impl<T: LastState> State for T {
    type StateMachine = T::StateMachine;

    async fn on_changed(self: Box<Self>, ctx: &mut T::StateMachine) -> StateResult<T::StateMachine> {
        let result = LastState::on_changed(self, ctx).await;
        StateResult::Finish(ResultGuard::new(result))
    }
}

/// Outcome of a state's `on_changed` — transition, finish, or error.
pub enum StateResult<Machine: StateMachineTrait> {
    ChangeState(ChangeGuard<Machine>),
    Finish(ResultGuard<Machine::Result>),
    Error(ErrorGuard<Machine::Error>),
}

/// Wraps the next state box. Can only be constructed inside this crate.
pub struct ChangeGuard<Machine: StateMachineTrait> {
    next: Box<dyn State<StateMachine = Machine>>,
}

impl<Machine: StateMachineTrait + 'static> ChangeGuard<Machine> {
    pub(crate) fn next<Next: State<StateMachine = Machine>>(next_state: Next) -> Self {
        ChangeGuard {
            next: Box::new(next_state),
        }
    }
}

/// Wraps the terminal result. Can only be constructed inside this crate.
pub struct ResultGuard<T> {
    result: T,
}

impl<T> ResultGuard<T> {
    fn new(result: T) -> Self { ResultGuard { result } }
}

/// Wraps an error that short-circuits execution. Can only be constructed inside this crate.
pub struct ErrorGuard<E> {
    error: E,
}

impl<E> ErrorGuard<E> {
    pub(crate) fn new(error: E) -> Self { ErrorGuard { error } }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::block_on;
    use common::executor::spawn;
    use futures::channel::mpsc;
    use futures::{SinkExt, StreamExt};
    use std::collections::HashMap;
    use std::convert::Infallible;

    type UserId = usize;
    type Login = String;
    type Password = String;

    #[derive(Debug, PartialEq)]
    enum AuthError {
        BadCredentialFormat,
        UnknownUser,
    }

    struct AuthMachine {
        users: HashMap<(Login, Password), UserId>,
    }

    type AuthResult = Result<UserId, AuthError>;

    impl StateMachineTrait for AuthMachine {
        type Result = AuthResult;
        type Error = Infallible;
    }

    impl StandardStateMachine for AuthMachine {}

    // States ------------------------------------------------------------------

    struct Reading {
        rx: mpsc::Receiver<char>,
    }
    struct Parsing {
        line: String,
    }
    struct Authenticating {
        login: Login,
        password: Password,
    }
    struct Authenticated {
        user_id: UserId,
    }
    struct Failed {
        error: AuthError,
    }

    // Allowed transitions -----------------------------------------------------

    impl TransitionFrom<Reading> for Parsing {}
    impl TransitionFrom<Parsing> for Authenticating {}
    impl TransitionFrom<Parsing> for Failed {}
    impl TransitionFrom<Authenticating> for Authenticated {}
    impl TransitionFrom<Authenticating> for Failed {}

    // Terminal states ---------------------------------------------------------

    #[async_trait]
    impl LastState for Authenticated {
        type StateMachine = AuthMachine;
        async fn on_changed(self: Box<Self>, _ctx: &mut AuthMachine) -> AuthResult { Ok(self.user_id) }
    }

    #[async_trait]
    impl LastState for Failed {
        type StateMachine = AuthMachine;
        async fn on_changed(self: Box<Self>, _ctx: &mut AuthMachine) -> AuthResult { Err(self.error) }
    }

    // State logic -------------------------------------------------------------

    #[async_trait]
    impl State for Reading {
        type StateMachine = AuthMachine;

        async fn on_changed(mut self: Box<Self>, _ctx: &mut AuthMachine) -> StateResult<AuthMachine> {
            let mut line = String::with_capacity(80);
            while let Some(ch) = self.rx.next().await {
                line.push(ch);
            }
            Self::change_state(Parsing { line })
        }
    }

    #[async_trait]
    impl State for Parsing {
        type StateMachine = AuthMachine;

        async fn on_changed(self: Box<Self>, _ctx: &mut AuthMachine) -> StateResult<AuthMachine> {
            let chunks: Vec<_> = self.line.split(' ').collect();
            if chunks.len() == 2 {
                return Self::change_state(Authenticating {
                    login: chunks[0].to_owned(),
                    password: chunks[1].to_owned(),
                });
            }
            Self::change_state(Failed {
                error: AuthError::BadCredentialFormat,
            })
        }
    }

    #[async_trait]
    impl State for Authenticating {
        type StateMachine = AuthMachine;

        async fn on_changed(self: Box<Self>, ctx: &mut AuthMachine) -> StateResult<AuthMachine> {
            let key = (self.login, self.password);
            match ctx.users.get(&key) {
                Some(&uid) => Self::change_state(Authenticated { user_id: uid }),
                None => Self::change_state(Failed {
                    error: AuthError::UnknownUser,
                }),
            }
        }
    }

    // Helpers -----------------------------------------------------------------

    fn run_auth(credentials: &'static str) -> AuthResult {
        let (mut tx, rx) = mpsc::channel(80);

        let mut users = HashMap::new();
        users.insert(("alice".to_owned(), "pass_a".to_owned()), 1);
        users.insert(("bob".to_owned(), "pass_b".to_owned()), 2);

        spawn(async move {
            for ch in credentials.chars() {
                tx.send(ch).await.expect("send char");
            }
        });

        block_on(async move {
            let mut machine = AuthMachine { users };
            machine.run(Box::new(Reading { rx })).await.unwrap()
        })
    }

    #[test]
    fn test_basic_state_machine_success() {
        assert_eq!(run_auth("bob pass_b"), Ok(2));
    }

    #[test]
    fn test_basic_state_machine_bad_format() {
        assert_eq!(run_auth("no_spaces_here"), Err(AuthError::BadCredentialFormat));
    }

    #[test]
    fn test_basic_state_machine_unknown_user() {
        assert_eq!(run_auth("eve pass_x"), Err(AuthError::UnknownUser));
    }
}
