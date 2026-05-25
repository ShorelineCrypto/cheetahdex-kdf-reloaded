//! # Purpose
//! Defines the [`Event`] payload broadcast from streamers to subscribed
//! clients.
//!
//! # Public exports
//! - [`Event`] — JSON payload tagged with origin [`StreamerId`] and an
//!   error flag.
//!
//! # Invariants
//! - Events are wrapped in `Arc` so cloning is cheap during fan-out.
//! - The textual form returned by [`Event::origin`] matches the
//!   [`StreamerId`] `Display` implementation and is part of the SSE
//!   wire contract.

use serde_json::Value as Json;
use std::fmt;
use std::sync::Arc;

use crate::StreamerId;

/// A single event emitted by a streamer, ready to be sent to subscribed clients.
#[derive(Clone)]
pub struct Event {
    /// Which streamer produced this event.
    streamer_id: StreamerId,
    /// JSON payload.
    message: Json,
    /// Whether this event represents an error condition.
    error: bool,
}

impl Event {
    /// Create a normal (non-error) event.
    pub fn new(streamer_id: StreamerId, message: Json) -> Arc<Self> {
        Arc::new(Self {
            streamer_id,
            message,
            error: false,
        })
    }

    /// Create an error event.
    pub fn err(streamer_id: StreamerId, message: Json) -> Arc<Self> {
        Arc::new(Self {
            streamer_id,
            message,
            error: true,
        })
    }

    /// Returns true if this event was constructed via [`Event::err`].
    pub fn is_error(&self) -> bool {
        self.error
    }

    /// Returns the origin streamer identifier in its wire-string form.
    pub fn origin(&self) -> String {
        self.streamer_id.to_string()
    }

    /// Returns the origin string paired with a borrow of the JSON payload.
    pub fn get(&self) -> (String, &Json) {
        (self.origin(), &self.message)
    }
}

impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Event")
            .field("origin", &self.origin())
            .field("error", &self.error)
            .finish()
    }
}
