/// EIP-1559 fee-estimator event streamer.
///
/// Produces a continuous, timer-paced EIP-1559 fee-per-gas estimate for a
/// single EVM coin and emits it as an SSE event every cycle (timer-paced,
/// not emit-on-change). Each coin gets its own streamer instance identified
/// by `StreamerId::FeeEstimation(ticker)`.
use async_trait::async_trait;
use common::executor::Timer;
use common::log;
use futures::future::{select, Either};
use mm2_event_stream::{mpsc, oneshot, Broadcaster, Event, EventStreamer, StreamerId};
use serde::Deserialize;
use serde_json::json;
use std::convert::TryFrom;

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use coins::eth::fee_estimation::ser::FeePerGasEstimated;
use coins::{lp_coinfind, MmCoinEnum};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

/// Cadence floor (seconds): if the remaining wait after a cycle falls below
/// this, the next cycle begins immediately (R32).
const RESTART_FLOOR: f64 = 0.1;

/// Which estimation strategy the streamer uses.
///
/// `Simple` selects the internal historical estimator; `Provider` selects the
/// external gas-API provider configured on the coin itself.
#[derive(Clone, Copy, Default, Deserialize)]
pub enum FeeEstimatorType {
    #[default]
    Simple,
    Provider,
}

/// Estimator configuration object (the `config` field of the request).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeeEstimatorConfig {
    /// Target cadence in seconds between successive re-estimations. Default: 15.
    #[serde(default = "default_estimate_every")]
    pub estimate_every: f64,
    /// Estimation strategy. Default: `Simple`.
    #[serde(default)]
    pub estimator_type: FeeEstimatorType,
}

fn default_estimate_every() -> f64 { 15.0 }

/// Per-coin activation request for the fee-estimator streamer.
#[derive(Deserialize)]
pub struct EnableFeeEstimatorRequest {
    /// EVM coin ticker to estimate fees for (must already be activated).
    pub coin: String,
    /// Estimator configuration (minimal accepted form is the empty object `{}`).
    pub config: FeeEstimatorConfig,
}

/// The fee-estimator streamer for a single EVM coin.
pub struct FeeEstimatorStreamer {
    ticker: String,
    estimate_every: f64,
    use_simple: bool,
    ctx: MmArc,
}

impl FeeEstimatorStreamer {
    pub fn new(ticker: String, estimate_every: f64, estimator_type: FeeEstimatorType, ctx: MmArc) -> Self {
        Self {
            ticker,
            estimate_every,
            use_simple: matches!(estimator_type, FeeEstimatorType::Simple),
            ctx,
        }
    }
}

#[async_trait]
impl EventStreamer for FeeEstimatorStreamer {
    type DataInType = mm2_event_stream::NoDataIn;

    fn streamer_id(&self) -> StreamerId { StreamerId::FeeEstimation(self.ticker.clone()) }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: oneshot::Sender<Result<(), String>>,
        shutdown_rx: oneshot::Receiver<()>,
        _data_rx: mpsc::UnboundedReceiver<mm2_event_stream::NoDataIn>,
    ) {
        // Resolve the coin and require it to be an activated EVM coin.
        let coin = match lp_coinfind(&self.ctx, &self.ticker).await {
            Ok(Some(MmCoinEnum::EthCoin(coin))) => coin,
            Ok(Some(_)) => {
                let _ = ready_tx.send(Err(format!(
                    "Coin {} is not an EVM coin; fee estimation is unsupported",
                    self.ticker
                )));
                return;
            },
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

        let sid = StreamerId::FeeEstimation(self.ticker.clone());
        let mut shutdown = shutdown_rx;

        loop {
            let start = common::now_float();

            // Re-estimate and broadcast unconditionally (timer-paced, not emit-on-change).
            match coin.get_eip1559_gas_fee(self.use_simple).await {
                Ok(fee) => match FeePerGasEstimated::try_from(fee) {
                    Ok(estimate) => match serde_json::to_value(&estimate) {
                        Ok(payload) => broadcaster.broadcast(Event::new(sid.clone(), payload)),
                        Err(e) => {
                            log::error!("Fee estimate serialization error for {}: {}", self.ticker, e);
                            broadcaster.broadcast(Event::err(sid.clone(), json!({ "error": e.to_string() })));
                        },
                    },
                    Err(e) => {
                        log::error!("Fee estimate conversion error for {}: {}", self.ticker, e);
                        broadcaster.broadcast(Event::err(sid.clone(), json!({ "error": e.to_string() })));
                    },
                },
                Err(e) => {
                    log::error!("Fee estimation error for {}: {}", self.ticker, e);
                    broadcaster.broadcast(Event::err(sid.clone(), json!({ "error": e.to_string() })));
                },
            }

            // Wait `estimate_every` minus the elapsed estimation time of this cycle.
            let wait = self.estimate_every - (common::now_float() - start);
            if wait < RESTART_FLOOR {
                // Below the floor: begin the next cycle immediately, but still
                // honour an already-fired shutdown signal.
                match shutdown.try_recv() {
                    Ok(()) | Err(oneshot::error::TryRecvError::Closed) => break,
                    Err(oneshot::error::TryRecvError::Empty) => continue,
                }
            }

            let sleep = core::pin::pin!(Timer::sleep(wait));
            match select(sleep, &mut shutdown).await {
                Either::Left(_) => {},
                Either::Right(_) => break,
            }
        }
    }
}

/// RPC handler for `stream::fee_estimator::enable`.
pub async fn enable_fee_estimator(
    ctx: MmArc,
    req: EnableStreamingRequest<EnableFeeEstimatorRequest>,
) -> MmResult<EnableStreamingResponse, StreamingError> {
    let client_id = req.client_id;
    let ticker = req.inner.coin.clone();
    let estimate_every = req.inner.config.estimate_every;
    let estimator_type = req.inner.config.estimator_type;

    let streamer = FeeEstimatorStreamer::new(ticker, estimate_every, estimator_type, ctx.clone());
    let streamer_id = streamer.streamer_id().to_string();
    ctx.event_stream_manager
        .add(client_id, streamer)
        .await
        .map_err(|e| MmError::new(StreamingError::InitFailed(e)))?;

    Ok(EnableStreamingResponse::new(streamer_id))
}

#[cfg(test)]
mod tests {
    use super::{FeeEstimatorConfig, FeeEstimatorType};
    use mm2_event_stream::StreamerId;

    #[test]
    fn fee_estimation_wire_string() {
        assert_eq!(
            StreamerId::FeeEstimation("ETH".to_string()).to_string(),
            "FEE_ESTIMATION:ETH"
        );
    }

    #[test]
    fn config_defaults_from_empty_object() {
        let cfg: FeeEstimatorConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.estimate_every, 15.0);
        assert!(matches!(cfg.estimator_type, FeeEstimatorType::Simple));
    }

    #[test]
    fn config_rejects_unknown_fields() {
        let res: Result<FeeEstimatorConfig, _> = serde_json::from_str(r#"{"bogus": 1}"#);
        assert!(res.is_err());
    }

    #[test]
    fn config_parses_provider_and_estimate_every() {
        let cfg: FeeEstimatorConfig =
            serde_json::from_str(r#"{"estimate_every": 5.5, "estimator_type": "Provider"}"#).unwrap();
        assert_eq!(cfg.estimate_every, 5.5);
        assert!(matches!(cfg.estimator_type, FeeEstimatorType::Provider));
    }
}
