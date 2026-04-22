//! RPC activation of the order status streamer.

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use crate::mm2::lp_ordermatch::order_events::OrderStatusStreamer;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

pub async fn enable_order_status(
    ctx: MmArc,
    req: EnableStreamingRequest<()>,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    ctx.event_stream_manager
        .add(req.client_id, OrderStatusStreamer)
        .await
        .map(|_| EnableStreamingResponse::new())
        .map_to_mm(StreamingError::InitFailed)
}
