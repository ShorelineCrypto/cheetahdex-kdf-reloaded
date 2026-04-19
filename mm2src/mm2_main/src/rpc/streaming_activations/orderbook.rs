//! RPC activation of the per-pair orderbook streamer.

use crate::mm2::lp_ordermatch::orderbook_events::OrderbookStreamer;
use super::{EnableStreamingResponse, StreamingError};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct EnableOrderbookRequest {
    pub client_id: u64,
    pub base: String,
    pub rel: String,
}

pub async fn enable_orderbook(
    ctx: MmArc,
    req: EnableOrderbookRequest,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    let streamer = OrderbookStreamer::new(ctx.clone(), req.base, req.rel);
    ctx.event_stream_manager
        .add(req.client_id, streamer)
        .await
        .map(|_| EnableStreamingResponse::new())
        .map_to_mm(StreamingError::InitFailed)
}
