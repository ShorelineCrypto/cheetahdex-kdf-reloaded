//! Orderbook SSE event streamer.
//!
//! Per-pair streamer that subscribes to the P2P orderbook topic and
//! broadcasts item changes (additions, updates, removals) to SSE clients.

use super::{orderbook_topic_from_base_rel, subscribe_to_orderbook_topic, OrderbookP2PItem};
use async_trait::async_trait;
use coins::{is_wallet_only_ticker, lp_coinfind};
use mm2_core::mm_ctx::MmArc;
use mm2_event_stream::{mpsc, oneshot, Broadcaster, Event, EventStreamer, StreamerId};
use serde::Serialize;
use uuid::Uuid;

/// Per-pair orderbook streamer.
pub struct OrderbookStreamer {
    ctx: MmArc,
    base: String,
    rel: String,
}

impl OrderbookStreamer {
    pub fn new(ctx: MmArc, base: String, rel: String) -> Self { Self { ctx, base, rel } }
}

/// Events emitted when orderbook items change.
#[derive(Serialize)]
#[serde(tag = "order_type", content = "order_data")]
pub enum OrderbookItemChangeEvent {
    /// New or updated orderbook item.
    NewOrUpdatedItem(Box<OrderbookP2PItem>),
    /// Removed orderbook item (only UUID is relevant).
    RemovedItem(Uuid),
}

#[async_trait]
impl EventStreamer for OrderbookStreamer {
    type DataInType = OrderbookItemChangeEvent;

    fn streamer_id(&self) -> StreamerId {
        StreamerId::OrderbookUpdate {
            topic: orderbook_topic_from_base_rel(&self.base, &self.rel),
        }
    }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: oneshot::Sender<Result<(), String>>,
        _shutdown_rx: oneshot::Receiver<()>,
        mut data_rx: mpsc::UnboundedReceiver<Self::DataInType>,
    ) {
        if let Err(err) = sanity_checks(&self.ctx, &self.base, &self.rel).await {
            let _ = ready_tx.send(Err(err));
            return;
        }
        // Subscribe to the P2P orderbook topic so updates arrive.
        if let Err(err) = subscribe_to_orderbook_topic(&self.ctx, &self.base, &self.rel, false).await {
            let err = format!("Subscribing to orderbook topic failed: {:?}", err);
            let _ = ready_tx.send(Err(err));
            return;
        }
        let _ = ready_tx.send(Ok(()));

        while let Some(orderbook_update) = data_rx.recv().await {
            let event_data = serde_json::to_value(orderbook_update).expect("Serialization shouldn't fail.");
            let event = Event::new(self.streamer_id(), event_data);
            broadcaster.broadcast(event);
        }
    }
}

async fn sanity_checks(ctx: &MmArc, base: &str, rel: &str) -> Result<(), String> {
    lp_coinfind(ctx, base)
        .await
        .map_err(|e| format!("Coin {} not found: {}", base, e))?;
    if is_wallet_only_ticker(ctx, base) {
        return Err(format!("Coin {} is wallet-only.", base));
    }
    lp_coinfind(ctx, rel)
        .await
        .map_err(|e| format!("Coin {} not found: {}", rel, e))?;
    if is_wallet_only_ticker(ctx, rel) {
        return Err(format!("Coin {} is wallet-only.", rel));
    }
    Ok(())
}
