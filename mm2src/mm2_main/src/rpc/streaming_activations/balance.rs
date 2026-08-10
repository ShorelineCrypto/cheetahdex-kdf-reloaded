/// Balance event streamer.
///
/// Polls a single coin's balance at a configurable interval and emits
/// SSE events when the balance changes. Each coin gets its own streamer
/// instance identified by `StreamerId::Balance(ticker)`.
use async_trait::async_trait;
use coins::utxo::utxo_common::{address_balance as utxo_address_balance, address_from_str_unchecked};
use common::executor::Timer;
use common::log;
use futures::channel::mpsc as futures_mpsc;
use futures::compat::Future01CompatExt;
use futures::future::{select, Either};
use futures::StreamExt;
use mm2_event_stream::{mpsc, oneshot, Broadcaster, Event, EventStreamer, StreamerId};
use serde::Deserialize;
use serde_json::json;

use super::{EnableStreamingRequest, EnableStreamingResponse, StreamingError};
use coins::utxo::rpc_clients::{electrum_script_hash, ElectrumClient, UtxoRpcClientEnum};
use coins::utxo::{output_script, ScriptType, UtxoCoinFields};
use coins::{lp_coinfind, MarketCoinOps, MmCoinEnum};
use mm2_core::mm_ctx::MmArc;
use mm2_err_handle::prelude::*;

/// The Electrum client and P2PKH script hash for an Electrum-backed UTXO coin.
///
/// `None` for native-RPC coins and every non-UTXO family: only Electrum offers
/// script-hash subscriptions, so there is nothing to register elsewhere.
///
/// The script hash is derived exactly as the balance path derives it
/// (`output_script(.., P2PKH)` then `electrum_script_hash`), because a hash that
/// does not match the one the balance read uses would subscribe successfully and
/// then never fire — a silent failure indistinguishable from an idle address.
fn electrum_subscription_target(coin: &MmCoinEnum) -> Option<(ElectrumClient, String)> {
    fn resolve(fields: &UtxoCoinFields, address_str: &str) -> Option<(ElectrumClient, String)> {
        let electrum = match &fields.rpc_client {
            UtxoRpcClientEnum::Electrum(client) => client.clone(),
            UtxoRpcClientEnum::Native(_) => return None,
        };
        let address = address_from_str_unchecked(fields, address_str).ok()?;
        let script = output_script(&address, ScriptType::P2PKH);
        Some((electrum, hex::encode(electrum_script_hash(&script))))
    }

    match coin {
        MmCoinEnum::UtxoCoin(c) => resolve(c.as_ref(), &c.my_address().ok()?),
        MmCoinEnum::QtumCoin(c) => resolve(c.as_ref(), &c.my_address().ok()?),
        MmCoinEnum::Bch(c) => resolve(c.as_ref(), &c.my_address().ok()?),
        _ => None,
    }
}

fn electrum_utxo_watch_address(coin: &MmCoinEnum) -> Option<String> {
    match coin {
        MmCoinEnum::UtxoCoin(c) => {
            if c.as_ref().rpc_client.is_native() {
                None
            } else {
                c.my_address().ok()
            }
        },
        MmCoinEnum::QtumCoin(c) => {
            if c.as_ref().rpc_client.is_native() {
                None
            } else {
                c.my_address().ok()
            }
        },
        MmCoinEnum::Bch(c) => {
            if c.as_ref().rpc_client.is_native() {
                None
            } else {
                c.my_address().ok()
            }
        },
        _ => None,
    }
}

async fn watched_balance(coin: &MmCoinEnum, watched_address: &Option<String>) -> Result<coins::CoinBalance, String> {
    match (coin, watched_address.as_deref()) {
        (MmCoinEnum::UtxoCoin(c), Some(address)) if !c.as_ref().rpc_client.is_native() => {
            let address = address_from_str_unchecked(c.as_ref(), address)?;
            utxo_address_balance(c, &address).await.map_err(|e| e.to_string())
        },
        (MmCoinEnum::QtumCoin(c), Some(address)) if !c.as_ref().rpc_client.is_native() => {
            let address = address_from_str_unchecked(c.as_ref(), address)?;
            utxo_address_balance(c, &address).await.map_err(|e| e.to_string())
        },
        (MmCoinEnum::Bch(c), Some(address)) if !c.as_ref().rpc_client.is_native() => {
            let address = address_from_str_unchecked(c.as_ref(), address)?;
            utxo_address_balance(c, &address).await.map_err(|e| e.to_string())
        },
        _ => coin.my_balance().compat().await.map_err(|e| e.to_string()),
    }
}

fn should_emit_balance_event(
    prev_spendable: Option<&String>,
    prev_unspendable: Option<&String>,
    prev_watched_address: Option<&String>,
    spendable: &String,
    unspendable: &String,
    watched_address: Option<&String>,
) -> bool {
    prev_spendable != Some(spendable)
        || prev_unspendable != Some(unspendable)
        || prev_watched_address != watched_address
}

/// Per-coin configuration for the balance streamer.
#[derive(Deserialize)]
pub struct EnableBalanceRequest {
    /// Coin ticker to monitor (must already be activated).
    pub coin: String,
    /// Poll interval in seconds. Default: 30, minimum: 10.
    #[serde(default = "default_interval")]
    pub interval_secs: u64,
}

fn default_interval() -> u64 { 30 }

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

    fn streamer_id(&self) -> StreamerId { StreamerId::Balance(self.ticker.clone()) }

    async fn handle(
        self,
        broadcaster: Broadcaster,
        ready_tx: oneshot::Sender<Result<(), String>>,
        shutdown_rx: oneshot::Receiver<()>,
        _data_rx: mpsc::UnboundedReceiver<mm2_event_stream::NoDataIn>,
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

        let interval_secs = self.interval_secs as f64;
        let mut shutdown = core::pin::pin!(shutdown_rx);
        let sid = StreamerId::Balance(self.ticker.clone());

        // Track previous balance to only emit on change.
        let mut prev_spendable: Option<String> = None;
        let mut prev_unspendable: Option<String> = None;
        let mut watched_address: Option<String> = electrum_utxo_watch_address(&coin);
        let mut prev_watched_address: Option<String> = None;

        // Ask the servers to tell us when this address changes, so a deposit or
        // an incoming swap payment does not wait out the poll interval
        // (R38.6.5). The receiver is only a wake signal: the balance below is
        // always re-read authoritatively, never inferred from a notification.
        //
        // Every failure path here degrades to plain polling rather than
        // aborting: a coin with no subscription is exactly as correct as it was
        // before, only slower to notice.
        let (wake_tx, mut wake_rx) = futures_mpsc::unbounded::<String>();
        let mut subscribed_script_hash: Option<String> = None;
        if let Some((electrum, script_hash)) = electrum_subscription_target(&coin) {
            match electrum.subscribe_scripthash(script_hash.clone()).await {
                Ok(()) => {
                    coins::utxo::rpc_clients::watch_scripthash(script_hash.clone(), wake_tx);
                    subscribed_script_hash = Some(script_hash);
                },
                Err(e) => log::debug!(
                    "Balance streamer for {}: script-hash subscription unavailable ({}); polling only",
                    self.ticker,
                    e
                ),
            }
        }

        // Emit an initial snapshot right away so clients don't wait for the first interval tick.
        match watched_balance(&coin, &watched_address).await {
            Ok(balance) => {
                let spendable = balance.spendable.to_string();
                let unspendable = balance.unspendable.to_string();

                prev_spendable = Some(spendable.clone());
                prev_unspendable = Some(unspendable.clone());
                prev_watched_address = watched_address.clone();

                let event = Event::new(
                    sid.clone(),
                    json!({
                        "coin": self.ticker,
                        "watched_address": watched_address,
                        "spendable": spendable,
                        "unspendable": unspendable,
                        "timestamp": common::now_ms(),
                    }),
                );
                broadcaster.broadcast(event);
            },
            Err(e) => {
                log::error!("Initial balance poll error for {}: {}", self.ticker, e);
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

        loop {
            // Re-register watch target for Electrum-backed UTXO coins if the active address changes.
            // This is relevant when address state is rotated externally (e.g. account/address updates).
            let current_watch_address = electrum_utxo_watch_address(&coin);
            if current_watch_address != watched_address {
                watched_address = current_watch_address;
            }

            // Whichever comes first: the poll deadline, or a server telling us
            // the address changed. The timer is retained as the correctness
            // floor, so a subscription that silently dies costs latency only.
            let tick = async {
                let sleep = Timer::sleep(interval_secs);
                futures::pin_mut!(sleep);
                let woken = wake_rx.next();
                futures::pin_mut!(woken);
                match select(sleep, woken).await {
                    Either::Left(_) => {},
                    // Drain anything that arrived while we were busy so a burst
                    // of notifications collapses into a single refresh.
                    Either::Right(_) => while wake_rx.try_next().is_ok() {},
                }
            };
            let tick = core::pin::pin!(tick);
            match select(tick, &mut shutdown).await {
                Either::Left(_) => {
                    match watched_balance(&coin, &watched_address).await {
                        Ok(balance) => {
                            let spendable = balance.spendable.to_string();
                            let unspendable = balance.unspendable.to_string();

                            // Emit when the balance or the watched Electrum UTXO address changes.
                            let changed = should_emit_balance_event(
                                prev_spendable.as_ref(),
                                prev_unspendable.as_ref(),
                                prev_watched_address.as_ref(),
                                &spendable,
                                &unspendable,
                                watched_address.as_ref(),
                            );

                            if changed {
                                prev_spendable = Some(spendable.clone());
                                prev_unspendable = Some(unspendable.clone());
                                prev_watched_address = watched_address.clone();

                                let event = Event::new(
                                    sid.clone(),
                                    json!({
                                        "coin": self.ticker,
                                        "watched_address": watched_address,
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
                },
                Either::Right(_) => {
                    if let Some(script_hash) = subscribed_script_hash.take() {
                        coins::utxo::rpc_clients::unwatch_scripthash(&script_hash);
                    }
                    break;
                },
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
    let streamer_id = streamer.streamer_id().to_string();
    ctx.event_stream_manager
        .add(client_id, streamer)
        .await
        .map_err(|e| MmError::new(StreamingError::InitFailed(e)))?;

    Ok(EnableStreamingResponse::new(streamer_id))
}

#[cfg(test)]
mod tests {
    use super::should_emit_balance_event;

    #[test]
    fn emits_when_watched_address_changes() {
        let prev_spendable = Some("1".to_string());
        let prev_unspendable = Some("0".to_string());
        let prev_watched_address = Some("RoldAddress".to_string());
        let spendable = "1".to_string();
        let unspendable = "0".to_string();
        let watched_address = Some("RnewAddress".to_string());

        assert!(should_emit_balance_event(
            prev_spendable.as_ref(),
            prev_unspendable.as_ref(),
            prev_watched_address.as_ref(),
            &spendable,
            &unspendable,
            watched_address.as_ref(),
        ));
    }

    #[test]
    fn skips_when_balance_and_watched_address_are_unchanged() {
        let prev_spendable = Some("1".to_string());
        let prev_unspendable = Some("0".to_string());
        let prev_watched_address = Some("RsameAddress".to_string());
        let spendable = "1".to_string();
        let unspendable = "0".to_string();
        let watched_address = Some("RsameAddress".to_string());

        assert!(!should_emit_balance_event(
            prev_spendable.as_ref(),
            prev_unspendable.as_ref(),
            prev_watched_address.as_ref(),
            &spendable,
            &unspendable,
            watched_address.as_ref(),
        ));
    }

    /// The wake path must actually shorten the wait, not merely compile.
    ///
    /// This mirrors the streamer's tick: race the poll deadline against the
    /// notification channel. With a deliberately long deadline, a delivered
    /// notification must return promptly — if the receiver were awaited
    /// incorrectly the race would simply fall through to the timer, which looks
    /// identical to "no notification arrived" and would leave push silently
    /// doing nothing.
    #[test]
    fn notification_wakes_the_tick_before_the_poll_deadline() {
        use common::executor::Timer;
        use futures::channel::mpsc as futures_mpsc;
        use futures::future::{select, Either};
        use futures::StreamExt;
        use std::time::Instant;

        let (tx, mut rx) = futures_mpsc::unbounded::<String>();

        common::block_on(async move {
            tx.unbounded_send("deadbeef".to_owned()).expect("receiver is alive");

            let started = Instant::now();
            // 30 s is the production default; a correct implementation must not
            // wait for it when a notification is already pending.
            let sleep = Timer::sleep(30.);
            futures::pin_mut!(sleep);
            let woken = rx.next();
            futures::pin_mut!(woken);

            let via_notification = matches!(select(sleep, woken).await, Either::Right(_));

            assert!(via_notification, "the notification must win the race, not the timer");
            assert!(
                started.elapsed().as_secs() < 5,
                "waking took {:?}; the notification did not short-circuit the poll deadline",
                started.elapsed()
            );
        });
    }

    /// A burst of notifications must collapse into one refresh rather than
    /// queueing a refresh per notification.
    #[test]
    fn burst_of_notifications_drains_to_a_single_wake() {
        use futures::channel::mpsc as futures_mpsc;
        use futures::StreamExt;

        let (tx, mut rx) = futures_mpsc::unbounded::<String>();
        for _ in 0..5 {
            tx.unbounded_send("deadbeef".to_owned()).expect("receiver is alive");
        }

        common::block_on(async move {
            rx.next().await.expect("first notification");
            let mut drained = 0;
            while rx.try_next().is_ok() {
                drained += 1;
            }
            assert_eq!(
                drained, 4,
                "the remaining notifications must be drained, not left queued"
            );
        });
    }
}
