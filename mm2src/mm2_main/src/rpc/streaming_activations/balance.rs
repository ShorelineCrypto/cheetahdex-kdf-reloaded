/// Balance event streamer.
///
/// Polls a single coin's balance at a configurable interval and emits
/// SSE events when the balance changes. Each coin gets its own streamer
/// instance identified by `StreamerId::Balance(ticker)`.
use async_trait::async_trait;
use common::log;
use futures::compat::Future01CompatExt;
use mm2_event_stream::{Broadcaster, Event, EventStreamer, StreamerId};
use serde::Deserialize;
use serde_json::json;

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use coins::lp_coinfind;
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

/// Per-coin configuration for the balance streamer.
#[derive(Deserialize)]
pub struct EnableBalanceRequest {
    /// Coin ticker to monitor (must already be activated).
    pub coin: String,
    /// Poll interval in seconds. Default: 30, minimum: 10.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 {
    30
}

/// The balance streamer for a single coin.
pub struct BalanceEventStreamer {
    ticker: String,
    interval_secs: u64,
    ctx: MmArc,
}

impl BalanceEventStreamer {
    pub fn new(ticker: String, interval_secs: u64, ctx: MmArc) -> Self {
        Self {
            ticker,
            interval_secs: interval_secs.max(10), // floor at 10s
            ctx,
        }
    }
}

#[async_trait]
impl EventStreamer for BalanceEventStreamer {
    type DataInType = mm2_event_stream::NoDataIn;

    fn streamer_id(&self) -> StreamerId {
        StreamerId::Balance(self.ticker.clone())
    }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
        shutdown_rx: tokio::sync::oneshot::Receiver<()>,
        _data_rx: tokio::sync::mpsc::UnboundedReceiver<mm2_event_stream::NoDataIn>,
    ) {
        // Verify the coin exists before signalling readiness.
        let coin = match lp_coinfind(&self.ctx, &self.ticker).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                let _ = ready_tx.send(Err(format!("Coin {} is not activated", self.ticker)));
                return;
            },
            Err(e) => {
                let _ = ready_tx.send(Err(format!("Error finding coin {}: {}", self.ticker, e)));
                return;
            },
        };

        let _ = ready_tx.send(Ok(()));

        let interval = std::time::Duration::from_secs(self.interval_secs);
        let mut shutdown = shutdown_rx;
        let sid = StreamerId::Balance(self.ticker.clone());

        // Track previous balance to only emit on change.
        let mut prev_spendable: Option<String> = None;
        let mut prev_unspendable: Option<String> = None;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    match coin.my_balance().compat().await {
                        Ok(balance) => {
                            let spendable = balance.spendable.to_string();
                            let unspendable = balance.unspendable.to_string();

                            // Emit only when balance has changed.
                            let changed = prev_spendable.as_ref() != Some(&spendable)
                                || prev_unspendable.as_ref() != Some(&unspendable);

                            if changed {
                                prev_spendable = Some(spendable.clone());
                                prev_unspendable = Some(unspendable.clone());

                                let event = Event::new(
                                    sid.clone(),
                                    json!({
                                        "coin": self.ticker,
                                        "spendable": spendable,
                                        "unspendable": unspendable,
                                        "timestamp": common::now_ms(),
                                    }),
                                );
                                broadcaster.broadcast(event);
                            }
                        },
                        Err(e) => {
                            log::error!("Balance poll error for {}: {}", self.ticker, e);
                            let event = Event::err(
                                sid.clone(),
                                json!({
                                    "coin": self.ticker,
                                    "error": e.to_string(),
                                    "timestamp": common::now_ms(),
                                }),
                            );
                            broadcaster.broadcast(event);
                        },
                    }
                }
                _ = &mut shutdown => {
                    break;
                }
            }
        }
    }
}

/// RPC handler for `stream::balance::enable`.
pub async fn enable_balance(
    ctx: MmArc,
    req: EnableStreamingRequest<EnableBalanceRequest>,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    let client_id = req.client_id;
    let ticker = req.inner.coin.clone();
    let interval = req.inner.interval_secs;

    let streamer = BalanceEventStreamer::new(ticker, interval, ctx.clone());
    ctx.event_stream_manager
        .add(client_id, streamer)
        .await
        .map_err(|e| MmError::new(StreamingError::InitFailed(e)))?;

    Ok(EnableStreamingResponse::new())
}
