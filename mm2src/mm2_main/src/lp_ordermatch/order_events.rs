//! Order status SSE event streamer.
//!
//! Broadcasts order lifecycle events (matches, connections) to subscribed
//! SSE clients via the streaming infrastructure.

use super::{MakerMatch, TakerMatch};
use async_trait::async_trait;
use mm2_event_stream::{mpsc, oneshot, Broadcaster, Event, EventStreamer, StreamerId};
use serde::Serialize;

/// Streamer that relays order status events to SSE clients.
pub struct OrderStatusStreamer;

/// Events emitted during order lifecycle.
#[derive(Serialize)]
#[serde(tag = "order_type", content = "order_data")]
pub enum OrderStatusEvent {
    MakerMatch(MakerMatch),
    TakerMatch(TakerMatch),
    MakerConnected(MakerMatch),
    TakerConnected(TakerMatch),
}

#[async_trait]
impl EventStreamer for OrderStatusStreamer {
    type DataInType = OrderStatusEvent;

    fn streamer_id(&self) -> StreamerId { StreamerId::OrderStatus }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: oneshot::Sender<Result<(), String>>,
        _shutdown_rx: oneshot::Receiver<()>,
        mut data_rx: mpsc::UnboundedReceiver<Self::DataInType>,
    ) {
        let _ = ready_tx.send(Ok(()));

        while let Some(order_data) = data_rx.recv().await {
            let event_data = serde_json::to_value(order_data).expect("Serialization shouldn't fail.");
            let event = Event::new(self.streamer_id(), event_data);
            broadcaster.broadcast(event);
        }
    }
}
