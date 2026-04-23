//! Platform entry-point library for the Komodo DeFi Framework (Reloaded).
//!
//! This crate provides the top-level binary and C-library interfaces.
//! The actual application logic lives in [`mm2`] (the mm2_main crate);
//! this crate is a thin wrapper that wires up platform-specific concerns
//! (native main, WASM bindings, mobile FFI).

// Re-export public entry points from mm2_main.
pub use mm2::lp_main;
pub use mm2::mm2_status;
pub use mm2::MainStatus;

#[cfg(not(target_arch = "wasm32"))]
pub use mm2::{mm2_main, run_lp_main};
