use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

use crate::event::Event;
use crate::streamer::{Broadcaster, EventStreamer, StreamerId};

/// Per-streamer bookkeeping.
struct StreamerInfo {
    /// Shutdown signal sender. Dropping this also triggers shutdown.
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// Client IDs subscribed to this streamer.
    subscribers: HashSet<u64>,
}

/// Per-client bookkeeping.
pub(crate) struct ClientInfo {
    /// Set of streamer origin strings this client is listening to.
    pub(crate) listening_to: HashSet<String>,
    /// Channel to push events to this client's SSE connection.
    pub(crate) tx: mpsc::Sender<Arc<Event>>,
}

/// Internal state behind the `RwLock`.
pub(crate) struct StreamingManagerInner {
    /// Active streamers keyed by their ID.
    streamers: HashMap<StreamerId, StreamerInfo>,
    /// Connected clients keyed by a unique client ID.
    pub(crate) clients: HashMap<u64, ClientInfo>,
}

/// Manages event streamers and client subscriptions.
///
/// Thread-safe, cheaply cloneable (wraps `Arc<RwLock<...>>`).
/// Add to `MmCtx` as a field; it starts with no streamers or clients.
#[derive(Clone)]
pub struct StreamingManager {
    inner: Arc<RwLock<StreamingManagerInner>>,
}

impl Default for StreamingManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StreamingManagerInner {
                streamers: HashMap::new(),
                clients: HashMap::new(),
            })),
        }
    }
}

/// Returned to new SSE clients so they can receive events.
pub struct ClientHandle {
    pub rx: mpsc::Receiver<Arc<Event>>,
}

impl StreamingManager {
    /// Register a new SSE client. Returns a `ClientHandle` whose `rx` field
    /// yields `Arc<Event>` items as they are broadcast.
    ///
    /// The caller should read from `rx` in a loop, formatting each event
    /// as `data: {json}\n\n` for the SSE protocol.
    pub fn new_client(&self, client_id: u64) -> ClientHandle {
        // Buffer up to 256 events per client before back-pressure kicks in.
        let (tx, rx) = mpsc::channel(256);
        let mut inner = self.inner.write();
        inner.clients.insert(
            client_id,
            ClientInfo {
                listening_to: HashSet::new(),
                tx,
            },
        );
        ClientHandle { rx }
    }

    /// Subscribe `client_id` to a streamer. If the streamer isn't running yet,
    /// it is spawned via `tokio::spawn`.
    ///
    /// Returns `Ok(())` when the streamer signals readiness, or `Err` if it
    /// failed to initialize.
    pub async fn add<S: EventStreamer>(&self, client_id: u64, streamer: S) -> Result<(), String> {
        let sid = streamer.streamer_id();
        let origin = sid.to_string();

        // Check if streamer is already running — just subscribe.
        {
            let mut inner = self.inner.write();
            if let Some(info) = inner.streamers.get_mut(&sid) {
                info.subscribers.insert(client_id);
                if let Some(client) = inner.clients.get_mut(&client_id) {
                    client.listening_to.insert(origin);
                }
                return Ok(());
            }
        }

        // Streamer not running — spawn it.
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();
        let broadcaster = Broadcaster {
            inner: self.inner.clone(),
        };

        {
            let mut inner = self.inner.write();
            inner.streamers.insert(
                sid.clone(),
                StreamerInfo {
                    shutdown_tx: Some(shutdown_tx),
                    subscribers: {
                        let mut s = HashSet::new();
                        s.insert(client_id);
                        s
                    },
                },
            );
            if let Some(client) = inner.clients.get_mut(&client_id) {
                client.listening_to.insert(origin);
            }
        }

        // Spawn the streamer task.
        let manager_inner = self.inner.clone();
        let sid_clone = sid.clone();
        tokio::spawn(async move {
            streamer.handle(broadcaster, ready_tx, shutdown_rx).await;
            // Cleanup when the streamer exits (for any reason).
            let mut inner = manager_inner.write();
            inner.streamers.remove(&sid_clone);
        });

        // Wait for the streamer to signal readiness.
        match ready_rx.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => {
                // Streamer reported an initialization error — clean up.
                let mut inner = self.inner.write();
                if let Some(info) = inner.streamers.remove(&sid) {
                    if let Some(tx) = info.shutdown_tx {
                        let _ = tx.send(());
                    }
                }
                Err(e)
            },
            Err(_) => {
                // ready_tx was dropped without sending — treat as failure.
                Err("Streamer dropped ready signal without responding".into())
            },
        }
    }

    /// Unsubscribe `client_id` from a streamer. If no subscribers remain,
    /// the streamer is shut down.
    pub fn stop(&self, client_id: u64, streamer_id: &StreamerId) {
        let origin = streamer_id.to_string();
        let mut inner = self.inner.write();

        // Remove from client's listening set.
        if let Some(client) = inner.clients.get_mut(&client_id) {
            client.listening_to.remove(&origin);
        }

        // Remove from streamer's subscriber set.
        if let Some(info) = inner.streamers.get_mut(streamer_id) {
            info.subscribers.remove(&client_id);
            if info.subscribers.is_empty() {
                // Last subscriber gone — shut down the streamer.
                if let Some(tx) = info.shutdown_tx.take() {
                    let _ = tx.send(());
                }
                inner.streamers.remove(streamer_id);
            }
        }
    }

    /// Remove a client entirely (e.g., SSE connection closed).
    /// Unsubscribes from all streamers and cleans up.
    pub fn remove_client(&self, client_id: u64) {
        let mut inner = self.inner.write();

        // Collect streamer IDs this client was subscribed to.
        let origins: Vec<String> = inner
            .clients
            .get(&client_id)
            .map(|c| c.listening_to.iter().cloned().collect())
            .unwrap_or_default();

        inner.clients.remove(&client_id);

        // For each streamer, remove this client from subscribers.
        let mut to_remove = Vec::new();
        for (sid, info) in inner.streamers.iter_mut() {
            if origins.contains(&sid.to_string()) {
                info.subscribers.remove(&client_id);
                if info.subscribers.is_empty() {
                    to_remove.push(sid.clone());
                }
            }
        }
        for sid in to_remove {
            if let Some(mut info) = inner.streamers.remove(&sid) {
                if let Some(tx) = info.shutdown_tx.take() {
                    let _ = tx.send(());
                }
            }
        }
    }

    /// Returns true if the given streamer is currently running.
    pub fn is_active(&self, streamer_id: &StreamerId) -> bool {
        let inner = self.inner.read();
        inner.streamers.contains_key(streamer_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::streamer::{Broadcaster, EventStreamer, StreamerId};
    use serde_json::json;

    struct TestStreamer;

    #[async_trait::async_trait]
    impl EventStreamer for TestStreamer {
        type DataInType = crate::NoDataIn;

        fn streamer_id(&self) -> StreamerId {
            StreamerId::Heartbeat
        }

        async fn handle(
            self,
            broadcaster: Broadcaster,
            ready_tx: oneshot::Sender<Result<(), String>>,
            shutdown_rx: oneshot::Receiver<()>,
        ) {
            let _ = ready_tx.send(Ok(()));
            // Emit one event, then wait for shutdown.
            let event = Event::new(StreamerId::Heartbeat, json!({"status": "alive"}));
            broadcaster.broadcast(event);
            let _ = shutdown_rx.await;
        }
    }

    #[tokio::test]
    async fn test_client_receives_event() {
        let mgr = StreamingManager::default();
        let mut handle = mgr.new_client(1);
        mgr.add(1, TestStreamer).await.unwrap();

        let event = tokio::time::timeout(std::time::Duration::from_secs(2), handle.rx.recv())
            .await
            .expect("timeout waiting for event")
            .expect("channel closed");

        assert_eq!(event.origin(), "HEARTBEAT");
        assert!(!event.is_error());
    }

    #[tokio::test]
    async fn test_remove_client_shuts_down_streamer() {
        let mgr = StreamingManager::default();
        let _handle = mgr.new_client(1);
        mgr.add(1, TestStreamer).await.unwrap();
        assert!(mgr.is_active(&StreamerId::Heartbeat));

        mgr.remove_client(1);
        // Give the spawned task a moment to clean up.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!mgr.is_active(&StreamerId::Heartbeat));
    }

    #[tokio::test]
    async fn test_multiple_clients_share_streamer() {
        let mgr = StreamingManager::default();
        let _h1 = mgr.new_client(1);
        let _h2 = mgr.new_client(2);
        mgr.add(1, TestStreamer).await.unwrap();
        // Second add should just subscribe, not spawn a new streamer.
        mgr.add::<TestStreamer>(2, TestStreamer).await.unwrap();

        // Remove first client — streamer should stay alive.
        mgr.stop(1, &StreamerId::Heartbeat);
        assert!(mgr.is_active(&StreamerId::Heartbeat));

        // Remove second client — streamer should shut down.
        mgr.stop(2, &StreamerId::Heartbeat);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!mgr.is_active(&StreamerId::Heartbeat));
    }
}
