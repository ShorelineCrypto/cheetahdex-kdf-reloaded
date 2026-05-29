use primitives::hash::H256;

/// Convenience trait for types that expose a primary `H256` identity hash.
///
/// Implemented by `Block` (where it returns the block hash) and by indexed-block
/// wrappers (which expose the cached header hash). Kept in the crate root so that
/// downstream consumers can implement it for their own indexed types without
/// pulling in any internals of this crate.
pub trait RepresentH256 {
    fn h256(&self) -> H256;
}
