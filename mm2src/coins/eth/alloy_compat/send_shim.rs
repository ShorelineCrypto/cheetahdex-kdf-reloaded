//! # Purpose
//!
//! WASM-only adapter that strips the missing `Send` bound from a
//! future returned by alloy. The desktop wallet runs entirely on a
//! single-threaded `wasm32-unknown-unknown` executor, so promoting a
//! `!Send` future to `Send` is sound at runtime; the assertion exists
//! purely to satisfy the `Send` requirement that `async_trait`
//! desugars onto cross-platform coin traits like
//! `HDAddressBalanceScanner` and `HDWalletBalanceOps`.
//!
//! # Public exports
//!
//! - [`assert_send_future`] — identity on native (where the future is
//!   already `Send`), promotes to `Send` via [`SendShim`] on WASM.
//!
//! # Invariants
//!
//! - **Single-threaded WASM only.** The unsafe `Send` impl below is
//!   sound *only* because the kdf-reloaded WASM build runs on
//!   `wasm-bindgen-futures::spawn_local`, which never moves a future
//!   across threads. Compiling the shim outside `target_arch = "wasm32"`
//!   is a hard error (the helper is gated behind `cfg`).
//! - **Identical mirror of legacy behaviour.** The same `unsafe impl
//!   Send` trick lives at `coins/eth/web3_transport.rs` (`SendFuture`),
//!   so this shim does not introduce any new soundness assumption
//!   beyond what the legacy `web3` transport already relied on.

#[cfg(not(target_arch = "wasm32"))]
pub fn assert_send_future<F>(fut: F) -> F
where
    F: std::future::Future + Send,
{
    fut
}

#[cfg(target_arch = "wasm32")]
pub fn assert_send_future<F>(fut: F) -> SendShim<F>
where
    F: std::future::Future,
{
    SendShim(fut)
}

#[cfg(target_arch = "wasm32")]
pub struct SendShim<F>(F);

// SAFETY: the kdf-reloaded WASM build runs on a single-threaded
// `wasm-bindgen-futures::spawn_local` executor, so a `!Send` future
// will never be polled from a different thread than the one that
// created it. The same trick is used by the legacy
// `coins::eth::web3_transport::SendFuture`.
#[cfg(target_arch = "wasm32")]
unsafe impl<F> Send for SendShim<F> {}

#[cfg(target_arch = "wasm32")]
impl<F: std::future::Future> std::future::Future for SendShim<F> {
    type Output = F::Output;

    fn poll(self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        // SAFETY: structural pinning — `self.0` is never moved out.
        unsafe { self.map_unchecked_mut(|s| &mut s.0) }.poll(cx)
    }
}
