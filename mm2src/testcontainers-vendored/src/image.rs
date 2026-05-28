use crate::{Container, Docker};

/// Represents a docker image.
pub trait Image
where
    Self: Sized + Default,
    Self::Args: Default + IntoIterator<Item = String>,
    Self::EnvVars: Default + IntoIterator<Item = (String, String)>,
{
    type Args;
    type EnvVars;

    fn descriptor(&self) -> String;
    fn wait_until_ready<D: Docker>(&self, container: &Container<D, Self>);
    fn args(&self) -> Self::Args;
    fn env_vars(&self) -> Self::EnvVars;
    fn with_args(self, arguments: Self::Args) -> Self;
}
