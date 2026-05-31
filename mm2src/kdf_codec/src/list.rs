//! Length-prefixed homogeneous list. Useful where the type system needs a
//! distinct `List<T>` (rather than a bare `Vec<T>`) for trait dispatch.
//!
//! KDF-original.

use crate::{Deserializable, Error, Reader, Serializable, Stream};
use std::io;

#[derive(Debug, Clone)]
pub struct List<T>(Vec<T>);

impl<T> List<T>
where
    T: Serializable + Deserializable,
{
    pub fn from(items: Vec<T>) -> Self { List(items) }

    pub fn into(self) -> Vec<T> { self.0 }
}

impl<S> Serializable for List<S>
where
    S: Serializable,
{
    fn serialize(&self, s: &mut Stream) { s.append_list(&self.0); }
}

impl<D> Deserializable for List<D>
where
    D: Deserializable,
{
    fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error>
    where
        T: io::Read,
    {
        reader.read_list().map(List)
    }
}
