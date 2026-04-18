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
    SwapStatus(String),
    OrderStatus(String),
}

impl fmt::Display for StreamerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StreamerId::Heartbeat => write!(f, "HEARTBEAT"),
            StreamerId::Balance(coin) => write!(f, "BALANCE:{}", coin),
            StreamerId::Network => write!(f, "NETWORK"),
            StreamerId::SwapStatus(uuid) => write!(f, "SWAP_STATUS:{}", uuid),
            StreamerId::OrderStatus(uuid) => write!(f, "ORDER_STATUS:{}", uuid),
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
pub enum NoDataIn {}

/// Trait bound for the data input channel a streamer receives.
/// Streamers that need external pushes use `mpsc::Receiver<T>`;
/// self-driven streamers use `NoDataIn`.
pub trait StreamHandlerInput<T>: Send + 'static {}
impl<T: Send + 'static> StreamHandlerInput<T> for mpsc::Receiver<T> {}
impl StreamHandlerInput<NoDataIn> for () {}

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
    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    );
}
