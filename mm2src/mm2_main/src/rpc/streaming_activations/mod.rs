/// Streaming activation/deactivation RPC handlers.
///
/// These handlers respond to `stream::*` RPC namespace methods,
/// enabling or disabling real-time event streams per client.
pub mod balance;
pub mod heartbeat;
pub mod orderbook;
pub mod orders;
pub mod swaps;

use common::HttpStatusCode;
use derive_more::Display;
use http::StatusCode;
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
