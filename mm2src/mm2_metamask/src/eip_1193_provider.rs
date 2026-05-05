use common::executor::{spawn_local_abortable, AbortOnDropHandle};
use common::log::error;
use futures::channel::{mpsc, oneshot};
use futures::future::BoxFuture;
use futures::StreamExt;
use jsonrpc_core::Call;
use serde::de::DeserializeOwned;
use serde_json::Value as Json;
use std::fmt;
use std::sync::Arc;
use web3::helpers::build_request;
use web3::transports::eip_1193::{Eip1193, Provider as RawProvider};
use web3::{Error, RequestId, Result, Transport};

type CommandSender = mpsc::UnboundedSender<ProviderCommand>;
type CommandReceiver = mpsc::UnboundedReceiver<ProviderCommand>;
type ResultSender<T> = oneshot::Sender<Result<T>>;

/// Cross-thread wrapper over an `Eip1193` transport.
///
/// Spawns a dedicated command loop so the underlying JS provider (which is
/// `!Send`) can be driven from any async context.
#[derive(Clone)]
pub struct Eip1193Provider {
    cmd_tx: CommandSender,
    /// Aborting the command loop when all clones are dropped.
    _abort: Arc<AbortOnDropHandle>,
}

impl Eip1193Provider {
    /// Attempts to detect a browser-injected EIP-1193 provider (e.g. MetaMask).
    pub fn detect() -> Option<Self> {
        let raw_provider = RawProvider::default().ok()?.map(Eip1193::new)?;
        let (cmd_tx, cmd_rx) = mpsc::unbounded();
        let abort = spawn_local_abortable(Self::run_command_loop(raw_provider, cmd_rx));

        Some(Eip1193Provider {
            cmd_tx,
            _abort: Arc::new(abort),
        })
    }

    /// Sends a single RPC call through the EIP-1193 channel and deserializes the response.
    pub async fn call_method<T>(&self, id: RequestId, request: Call) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let (result_tx, result_rx) = oneshot::channel();
        let cmd = ProviderCommand::CallMethod { id, request, result_tx };
        let raw = dispatch_and_await(&self.cmd_tx, cmd, result_rx).await?;
        serde_json::from_value(raw).map_err(|e| Error::InvalidResponse(e.to_string()))
    }

    async fn run_command_loop(transport: Eip1193, mut cmd_rx: CommandReceiver) {
        while let Some(cmd) = cmd_rx.next().await {
            match cmd {
                ProviderCommand::CallMethod { id, request, result_tx } => {
                    let res = transport.send(id, request).await;
                    result_tx.send(res).ok();
                },
            }
        }
    }
}

impl Transport for Eip1193Provider {
    type Out = BoxFuture<'static, Result<Json>>;

    fn prepare(&self, method: &str, params: Vec<Json>) -> (RequestId, Call) {
        // For EIP-1193 the request ID is unused, but the trait requires one.
        const FIXED_ID: RequestId = 0;
        let request = build_request(FIXED_ID, method, params);
        (FIXED_ID, request)
    }

    fn send(&self, id: RequestId, request: Call) -> Self::Out {
        let this = self.clone();
        Box::pin(async move { this.call_method(id, request).await })
    }
}

impl fmt::Debug for Eip1193Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Eip1193Provider")
    }
}

/// Sends a command over the channel and waits for the result.
async fn dispatch_and_await<T>(
    cmd_tx: &CommandSender,
    cmd: ProviderCommand,
    result_rx: oneshot::Receiver<Result<T>>,
) -> Result<T> {
    if let Err(e) = cmd_tx.unbounded_send(cmd) {
        error!("Failed to send EIP-1193 command: {}", e);
        return Err(Error::Internal);
    }
    match result_rx.await {
        Ok(result) => result,
        Err(e) => {
            error!("Failed to receive EIP-1193 result: {}", e);
            Err(Error::Internal)
        },
    }
}

enum ProviderCommand {
    CallMethod {
        id: RequestId,
        request: Call,
        result_tx: ResultSender<Json>,
    },
}
