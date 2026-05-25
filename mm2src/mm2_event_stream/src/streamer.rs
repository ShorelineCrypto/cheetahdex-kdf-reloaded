//! # Purpose
//! Defines the [`EventStreamer`] trait, the wire-stable [`StreamerId`]
//! enum, and the [`Broadcaster`] handle a running streamer uses to fan
//! events out to subscribers.
//!
//! # Public exports
//! - [`StreamerId`] — origin identifier; its `Display` form is on-the-wire.
//! - [`Broadcaster`] — cheap-to-clone handle that pushes events into the
//!   manager's per-client channels.
//! - [`NoDataIn`] — uninhabited marker for streamers with no external
//!   input.
//! - [`EventStreamer`] — async trait every streamer implements.
//!
//! # Invariants
//! - [`StreamerId::Display`] strings (`HEARTBEAT`, `BALANCE:<COIN>`, …)
//!   are part of the SSE wire surface — do not rename.
//! - Per-client send channels are bounded; broadcasts use `try_send` so a
//!   slow client never blocks the broadcaster.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::event::Event;
use crate::manager::StreamingManagerInner;

/// Identifies a specific event streamer type.
///
/// Each variant corresponds to one category of real-time events.
/// String payloads allow per-coin or per-entity disambiguation
/// (e.g., `Balance("KMD")` vs `Balance("BTC")`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamerId {
    Heartbeat,
    Balance(String),
    Network,
    SwapStatus,
    OrderStatus,
    OrderbookUpdate { topic: String },
}

impl fmt::Display for StreamerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamerId::Heartbeat => write!(f, "HEARTBEAT"),
            StreamerId::Balance(coin) => write!(f, "BALANCE:{}", coin),
            StreamerId::Network => write!(f, "NETWORK"),
            StreamerId::SwapStatus => write!(f, "SWAP_STATUS"),
            StreamerId::OrderStatus => write!(f, "ORDER_STATUS"),
            StreamerId::OrderbookUpdate { topic } => write!(f, "ORDERBOOK:{}", topic),
        }
    }
}

/// Broadcaster handle given to each streamer for emitting events.
#[derive(Clone)]
pub struct Broadcaster {
    pub(crate) inner: Arc<parking_lot::RwLock<StreamingManagerInner>>,
}

impl Broadcaster {
    /// Broadcast an event to all clients subscribed to its origin streamer.
    pub fn broadcast(&self, event: Arc<Event>) {
        let inner = self.inner.read();
        let origin = event.origin();
        for client in inner.clients.values() {
            if client.listening_to.contains(&origin) {
                // Best-effort: if the channel is full, skip this client for this event.
                let _ = client.tx.try_send(event.clone());
            }
        }
    }
}

/// Marker type for streamers that don't receive external data.
/// Since this enum has no variants, `mpsc::UnboundedReceiver<NoDataIn>`
/// will never yield a value, which is exactly what self-driven streamers need.
pub enum NoDataIn {}

/// Core trait for all event streamers.
///
/// Implementors define how to produce events. The streaming manager
/// spawns the `handle` method when the first client subscribes and
/// shuts it down when the last client unsubscribes.
#[async_trait]
pub trait EventStreamer: Sized + Send + 'static {
    /// The type of data this streamer can receive from external sources.
    /// Use `NoDataIn` if the streamer is self-driven (e.g., polling).
    type DataInType: Send;

    /// Unique identifier for this streamer instance.
    fn streamer_id(&self) -> StreamerId;

    /// Main event loop. Called once when the first client subscribes.
    ///
    /// * `broadcaster` — use to emit events to subscribed clients
    /// * `ready_tx` — send `Ok(())` when initialization is done, or `Err` to abort
    /// * `shutdown_rx` — resolves when the streamer should stop
    /// * `data_rx` — channel for receiving external data pushes (empty for `NoDataIn`)
    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        data_rx: mpsc::UnboundedReceiver<Self::DataInType>,
    );
}
