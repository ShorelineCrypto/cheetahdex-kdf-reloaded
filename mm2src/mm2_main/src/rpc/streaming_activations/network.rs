/// Network event streamer activation.
///
/// Activates the `Network` streamer (wire id `NETWORK`), which periodically
/// snapshots the node's gossipsub / peer-connectivity state and emits it as an
/// SSE event. The streamer struct itself lives in the `mm2_p2p` crate, beside
/// the gossipsub introspection accessors it consumes.
use mm2_p2p::network_streamer::NetworkStreamer;
use serde::Deserialize;

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use crate::mm2::lp_network::P2PContext;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use mm2_event_stream::EventStreamer;

/// Per-client configuration for the network streamer.
#[derive(Deserialize)]
pub struct EnableNetworkRequest {
    /// The per-streamer configuration object. Optional: when omitted, every
    /// inner field falls back to its default.
    #[serde(default)]
    pub config: NetworkStreamingConfig,
}

/// The `config` object carried by the network streamer activation request.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkStreamingConfig {
    /// Delay between successive network-snapshot emissions. Default: 5.0s.
    /// There is no minimum floor.
    #[serde(default = "default_stream_interval_seconds")]
    pub stream_interval_seconds: f64,
    /// When `true`, emit every cycle even if the snapshot is unchanged.
    /// When `false` (default), emit only on change.
    #[serde(default)]
    pub always_send: bool,
}

impl Default for NetworkStreamingConfig {
    fn default() -> Self {
        Self {
            stream_interval_seconds: default_stream_interval_seconds(),
            always_send: false,
        }
    }
}

fn default_stream_interval_seconds() -> f64 { 5.0 }

/// RPC handler for `stream::network::enable`.
pub async fn enable_network(
    ctx: MmArc,
    req: EnableStreamingRequest<EnableNetworkRequest>,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    let client_id = req.client_id;
    let config = req.inner.config;

    let cmd_tx = P2PContext::fetch_from_mm_arc(&ctx).cmd_tx.lock().clone();
    let streamer = NetworkStreamer::new(Some(config.stream_interval_seconds), config.always_send, cmd_tx);
    let streamer_id = streamer.streamer_id().to_string();
    ctx.event_stream_manager
        .add(client_id, streamer)
        .await
        .map_err(|e| MmError::new(StreamingError::InitFailed(e)))?;

    Ok(EnableStreamingResponse::new(streamer_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn config_defaults_when_omitted() {
        let req: EnableStreamingRequest<EnableNetworkRequest> =
            serde_json::from_value(json!({ "client_id": 7 })).unwrap();
        assert_eq!(req.client_id, 7);
        assert_eq!(req.inner.config.stream_interval_seconds, 5.0);
        assert!(!req.inner.config.always_send);
    }

    #[test]
    fn config_parses_explicit_values() {
        let req: EnableStreamingRequest<EnableNetworkRequest> = serde_json::from_value(json!({
            "client_id": 0,
            "config": { "stream_interval_seconds": 0.5, "always_send": true }
        }))
        .unwrap();
        // No floor on the interval (unlike the balance streamer's 10s floor).
        assert_eq!(req.inner.config.stream_interval_seconds, 0.5);
        assert!(req.inner.config.always_send);
    }

    #[test]
    fn config_rejects_unknown_fields() {
        let res: Result<EnableStreamingRequest<EnableNetworkRequest>, _> = serde_json::from_value(json!({
            "client_id": 0,
            "config": { "bogus": 1 }
        }));
        assert!(res.is_err());
    }

    #[test]
    fn streamer_id_is_network() {
        let (cmd_tx, _rx) = futures::channel::mpsc::channel(1);
        let streamer = NetworkStreamer::new(None, false, cmd_tx);
        assert_eq!(streamer.streamer_id().to_string(), "NETWORK");
    }
}
