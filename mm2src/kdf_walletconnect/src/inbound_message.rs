//! Inbound request/response correlation.
//!
//! Outbound JSON-RPC requests register a one-shot waiter keyed by their numeric
//! message id. When the inbound loop decodes a response, it resolves the waiter
//! with the raw result value, waking the caller.

use parking_lot::Mutex;
use relay_rpc::domain::MessageId;
use std::collections::HashMap;
use tokio::sync::oneshot;

/// A registry of pending outbound requests awaiting a wallet response.
#[derive(Default)]
pub struct PendingRequests {
    waiters: Mutex<HashMap<MessageId, oneshot::Sender<serde_json::Value>>>,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a waiter for `id`, returning the receiver the caller awaits.
    pub fn register(&self, id: MessageId) -> oneshot::Receiver<serde_json::Value> {
        let (tx, rx) = oneshot::channel();
        self.waiters.lock().insert(id, tx);
        rx
    }

    /// Resolves the waiter registered for `id`, if any. Returns `true` when a
    /// waiter was present and notified.
    pub fn resolve(&self, id: MessageId, value: serde_json::Value) -> bool {
        if let Some(tx) = self.waiters.lock().remove(&id) {
            tx.send(value).is_ok()
        } else {
            false
        }
    }

    /// Drops the waiter for `id` without resolving it (e.g. on timeout).
    pub fn cancel(&self, id: MessageId) {
        self.waiters.lock().remove(&id);
    }
}
