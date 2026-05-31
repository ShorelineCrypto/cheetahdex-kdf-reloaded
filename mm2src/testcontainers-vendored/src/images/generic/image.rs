use crate::{Container, Docker, Image, WaitError, WaitForMessage};
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum WaitFor {
    Nothing,
    LogMessage { message: String, stream: Stream },
}

#[derive(Debug, PartialEq, Clone)]
pub enum Stream {
    StdOut,
    StdErr,
}

impl Default for WaitFor {
    fn default() -> Self { WaitFor::Nothing }
}

impl WaitFor {
    pub fn message_on_stdout<S: Into<String>>(message: S) -> WaitFor {
        WaitFor::LogMessage {
            message: message.into(),
            stream: Stream::StdOut,
        }
    }

    pub fn message_on_stderr<S: Into<String>>(message: S) -> WaitFor {
        WaitFor::LogMessage {
            message: message.into(),
            stream: Stream::StdErr,
        }
    }

    fn wait<D: Docker, I: Image>(&self, container: &Container<D, I>) -> Result<(), WaitError> {
        match self {
            WaitFor::Nothing => Ok(()),
            WaitFor::LogMessage { message, stream } => match stream {
                Stream::StdOut => container.logs().stdout.wait_for_message(message),
                Stream::StdErr => container.logs().stderr.wait_for_message(message),
            },
        }
    }
}

#[derive(Debug, Default)]
pub struct GenericImage {
    descriptor: String,
    arguments: Vec<String>,
    env_vars: HashMap<String, String>,
    wait_for: WaitFor,
}

impl GenericImage {
    pub fn new<S: Into<String>>(descriptor: S) -> GenericImage {
        Self {
            descriptor: descriptor.into(),
            arguments: vec![],
            env_vars: HashMap::new(),
            wait_for: WaitFor::Nothing,
        }
    }

    pub fn with_env_var<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    pub fn with_wait_for(mut self, wait_for: WaitFor) -> Self {
        self.wait_for = wait_for;
        self
    }
}

impl Image for GenericImage {
    type Args = Vec<String>;
    type EnvVars = HashMap<String, String>;

    fn descriptor(&self) -> String { self.descriptor.clone() }
    fn wait_until_ready<D: Docker>(&self, container: &Container<D, Self>) { self.wait_for.wait(container).unwrap(); }
    fn args(&self) -> Self::Args { self.arguments.clone() }
    fn env_vars(&self) -> Self::EnvVars { self.env_vars.clone() }
    fn with_args(self, arguments: Self::Args) -> Self { Self { arguments, ..self } }
}
