//! # `ser_error` — adjacent-tagged error serialization contract
//!
//! This tiny crate defines the type-level contract every error type
//! serialized over the `mmrpc` wire must obey:
//!
//! 1. The error type is an `enum` (struct support is not yet wired in
//!    the companion derive).
//! 2. The enum is annotated `#[serde(tag = "error_type", content =
//!    "error_data")]` so it produces the *adjacent-tagged* JSON shape.
//! 3. The enum carries `#[derive(SerializeErrorType)]` from the
//!    `ser_error_derive` companion crate.
//!
//! Step 3 emits a sealed marker impl
//! ([`__private::SerializeErrorTypeImpl`]) for the enum, which the
//! blanket impl below picks up to grant it [`SerializeErrorType`]. The
//! whole machinery exists to make `MmError<E>: Serialize` available
//! only when `E` provably produces the right wire shape.
//!
//! # Wire constants
//!
//! [`TAG`] (`"error_type"`) and [`CONTENT`] (`"error_data"`) are part
//! of the JSON-RPC contract — clients destructure their error envelopes
//! by these exact field names. They MUST NOT be changed.

use serde::Serialize;

/// JSON object key carrying the `serde` enum-variant discriminator.
pub const TAG: &str = "error_type";

/// JSON object key carrying the `serde` enum-variant payload.
pub const CONTENT: &str = "error_data";

/// Type-level proof that an error type serializes into exactly the
/// `{ error_type, error_data }` wire shape.
///
/// You don't implement this trait directly — derive
/// [`ser_error_derive::SerializeErrorType`] on your error enum and the
/// blanket impl below picks it up automatically. The sealing mechanism
/// uses [`__private::SerializeErrorTypeImpl`] so that no third-party
/// crate can claim to implement `SerializeErrorType` without also
/// going through the derive (which validates the `#[serde(...)]`
/// attributes at compile time).
pub trait SerializeErrorType: Serialize + __private::SerializeErrorTypeImpl {
    /// `serde(tag = "...")` value the derive expects. Always [`TAG`].
    fn tag() -> &'static str {
        TAG
    }

    /// `serde(content = "...")` value the derive expects. Always [`CONTENT`].
    fn content() -> &'static str {
        CONTENT
    }
}

impl<T> SerializeErrorType for T where T: Serialize + __private::SerializeErrorTypeImpl {}

/// Sealing module for [`SerializeErrorType`].
///
/// The contained [`SerializeErrorTypeImpl`] trait is `pub`, but only
/// implementable through the `ser_error_derive` proc-macro because
/// downstream code does not write `impl ser_error::__private::...` by
/// hand.
pub mod __private {
    /// Sealed marker. Implementations are emitted only by
    /// `#[derive(SerializeErrorType)]` from the `ser_error_derive` crate.
    pub trait SerializeErrorTypeImpl {}
}
