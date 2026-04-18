/// Heartbeat event streamer.
///
/// Emits periodic `{"status": "alive", "timestamp": <ms>}` events.
/// Primarily useful for:
/// - Verifying SSE infrastructure works end-to-end
/// - Client connection keep-alive
/// - Monitoring server liveness
use async_trait::async_trait;
use mm2_event_stream::{Broadcaster, Event, EventStreamer, StreamerId};
use serde::Deserialize;
use serde_json::json;

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

/// Per-client configuration for the heartbeat streamer.
#[derive(Deserialize)]
pub struct EnableHeartbeatRequest {
    /// Interval in seconds between heartbeat events. Default: 30.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 {
    30
}

/// The heartbeat streamer itself.
pub struct HeartbeatStreamer {
    interval_secs: u64,
}

impl HeartbeatStreamer {
    pub fn new(interval_secs: u64) -> Self {
        Self {
            interval_secs: interval_secs.max(5), // floor at 5s to prevent abuse
        }
    }
}

#[async_trait]
impl EventStreamer for HeartbeatStreamer {
    type DataInType = mm2_event_stream::NoDataIn;

    fn streamer_id(&self) -> StreamerId {
        StreamerId::Heartbeat
    }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
    ) {
        // Signal readiness immediately.
        let _ = ready_tx.send(Ok(()));

        let interval = std::time::Duration::from_secs(self.interval_secs);
        let mut shutdown = shutdown_rx;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    let now_ms = common::now_ms();
                    let event = Event::new(
                        StreamerId::Heartbeat,
                        json!({
                            "status": "alive",
                            "timestamp": now_ms,
                        }),
                    );
                    broadcaster.broadcast(event);
                }
                _ = &mut shutdown => {
                    break;
                }
            }
        }
    }
}

/// RPC handler for `stream::heartbeat::enable`.
pub async fn enable_heartbeat(
    ctx: MmArc,
    req: EnableStreamingRequest<EnableHeartbeatRequest>,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    let client_id = req.client_id;
    let interval = req.inner.interval_secs;

    let streamer = HeartbeatStreamer::new(interval);
    ctx.event_stream_manager
        .add(client_id, streamer)
        .await
        .map_err(|e| MmError::new(StreamingError::InitFailed(e)))?;

    Ok(EnableStreamingResponse::new())
}
