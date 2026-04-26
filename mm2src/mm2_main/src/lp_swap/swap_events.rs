//! Swap status SSE streamer.
//!
//! Provides a single global `SwapStatusStreamer` that broadcasts swap state
//! changes (both V1 and V2) to all subscribed SSE clients.

use async_trait::async_trait;
use mm2_event_stream::{Broadcaster, Event, EventStreamer, StreamerId};
use serde::Serialize;
use uuid::Uuid;

use super::maker_swap::MakerSwapEvent as MakerV1Event;
use super::maker_swap_v2::MakerSwapEvent as MakerV2Event;
use super::taker_swap::TakerSwapEvent as TakerV1Event;
use super::taker_swap_v2::TakerSwapEvent as TakerV2Event;

/// A single swap-status event, tagged by swap type.
#[derive(Serialize)]
#[serde(tag = "swap_type", content = "swap_data")]
pub enum SwapStatusEvent {
    MakerV1 { uuid: Uuid, event: MakerV1Event },
    TakerV1 { uuid: Uuid, event: TakerV1Event },
    MakerV2 { uuid: Uuid, event: MakerV2Event },
    TakerV2 { uuid: Uuid, event: TakerV2Event },
}

/// Global streamer that relays swap-status events to SSE clients.
pub struct SwapStatusStreamer;

#[async_trait]
impl EventStreamer for SwapStatusStreamer {
    type DataInType = SwapStatusEvent;

    fn streamer_id(&self) -> StreamerId { StreamerId::SwapStatus }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        _shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        mut data_rx: tokio::sync::mpsc::UnboundedReceiver<Self::DataInType>,
    ) {
        let _ = ready_tx.send(Ok(()));

        while let Some(swap_data) = data_rx.recv().await {
            let event_data = serde_json::to_value(&swap_data).expect("SwapStatusEvent serialization shouldn't fail");
            broadcaster.broadcast(Event::new(self.streamer_id(), event_data));
        }
    }
}
