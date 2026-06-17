//! Relay connection handler.
//!
//! The relay client drives this handler on its background task; every inbound
//! relay message is forwarded onto an unbounded channel that the subsystem's
//! own loop drains and routes by message id.

use relay_client::websocket::{ConnectionHandler, PublishedMessage};
use tokio::sync::mpsc::UnboundedSender;

/// Forwards relay events to the subsystem's processing loop.
pub struct WcConnectionHandler {
    inbound_tx: UnboundedSender<PublishedMessage>,
}

impl WcConnectionHandler {
    /// Creates a handler that forwards inbound messages onto `inbound_tx`.
    pub fn new(inbound_tx: UnboundedSender<PublishedMessage>) -> Self {
        WcConnectionHandler { inbound_tx }
    }
}

impl ConnectionHandler for WcConnectionHandler {
    fn message_received(&mut self, message: PublishedMessage) {
        // If the receiver has been dropped there is nothing left to route to.
        let _ = self.inbound_tx.send(message);
    }
}
