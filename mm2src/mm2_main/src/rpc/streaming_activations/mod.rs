/// Streaming activation/deactivation RPC handlers.
///
/// These handlers respond to `stream::*` RPC namespace methods,
/// enabling or disabling real-time event streams per client.
pub mod balance;
pub mod fee_estimator;
pub mod heartbeat;
pub mod network;
pub mod orderbook;
pub mod orders;
pub mod swaps;

use common::HttpStatusCode;
use derive_more::Display;
use http::StatusCode;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use ser_error_derive::SerializeErrorType;
use serde::{Deserialize, Serialize};

/// Common request wrapper for streaming activation RPCs.
#[derive(Deserialize)]
pub struct EnableStreamingRequest<T> {
    pub client_id: u64,
    #[serde(flatten)]
    pub inner: T,
}

/// Response returned when a streamer is successfully enabled.
#[derive(Serialize)]
pub struct EnableStreamingResponse {
    /// Identifier of the enabled streamer. Clients use it to correlate the
    /// subscription with the SSE events it emits.
    pub streamer_id: String,
}

impl EnableStreamingResponse {
    pub fn new(streamer_id: String) -> Self { Self { streamer_id } }
}

/// Errors that can occur during streaming operations.
#[derive(Display, Serialize, SerializeErrorType)]
#[serde(tag = "error_type", content = "error_data")]
pub enum StreamingError {
    #[display(fmt = "Streamer initialization failed: {}", _0)]
    InitFailed(String),
}

impl HttpStatusCode for StreamingError {
    fn status_code(&self) -> StatusCode {
        match self {
            StreamingError::InitFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Request body for `stream::disable` — stops all active streams for the given client.
#[derive(Deserialize)]
pub struct DisableStreamingRequest {
    pub client_id: u64,
}

/// Response returned when streams are successfully disabled.
#[derive(Serialize)]
pub struct DisableStreamingResponse {
    pub stopped: bool,
}

/// Handler for `stream::disable`.
///
/// Removes the client from the event-stream manager, which stops all active
/// streamers for which this is the last subscriber.
pub async fn disable_streaming(
    ctx: MmArc,
    req: DisableStreamingRequest,
) -> MmResult<DisableStreamingResponse, StreamingError> {
    ctx.event_stream_manager.remove_client(req.client_id);
    Ok(DisableStreamingResponse { stopped: true })
}
