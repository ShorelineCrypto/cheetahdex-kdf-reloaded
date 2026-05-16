//! Storage-error trait and shared outcome types.

use mm2_err_handle::mm_error::NotMmError;
use std::fmt::Debug;

/// Marker trait satisfied by the per-backend error types. The constraints
/// are kept minimal so that wrapping errors (database, JSON, IO) can be
/// surfaced through `MmError` without losing their `Send` bound.
pub trait NftStoreError: Debug + NotMmError + Send + Sync {}

/// Outcome of a token-removal request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoveOutcome {
    /// The cache entry existed and was removed.
    Removed,
    /// No cache entry matched the request.
    Absent,
}
