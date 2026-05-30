// `DisplayLayout` — common trait for types that have a wire-level
// byte serialization separate from their `Display` representation.

use crate::Error;
use std::ops::Deref;

pub trait DisplayLayout {
    type Target: Deref<Target = [u8]>;

    fn layout(&self) -> Self::Target;

    fn from_layout(data: &[u8]) -> Result<Self, Error>
    where
        Self: Sized;
}
