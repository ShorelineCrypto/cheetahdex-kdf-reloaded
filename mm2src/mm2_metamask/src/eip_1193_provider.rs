//! # Purpose
//!
//! A clean-room, wasm-bindgen-only EIP-1193 transport for the
//! browser-injected MetaMask provider (`window.ethereum`). Replaces
//! the previous wrapper around `web3::transports::eip_1193::Eip1193`
//! and removes the last upstream-fork rust-web3 dependency from the
//! project (LP-17 step closing §0 of RELOADED-CODING-STANDARDS.md).
//!
//! # Public exports
//!
//! - [`Eip1193Provider`] — cloneable, `Send + Sync` handle that
//!   forwards JSON-RPC method calls to the active browser provider
//!   via a dedicated command loop. Returns deserialised
//!   [`serde_json::Value`] payloads.
//!
//! # Invariants
//!
//! - **JSON-RPC wire shape unchanged.** Outbound requests are
//!   constructed exactly as `{ method, params }` and forwarded
//!   verbatim to `window.ethereum.request(args)`. The browser
//!   provider performs the JSON encoding; we never serialise method
//!   bodies ourselves, so the bytes that reach the wallet are
//!   byte-equal to what the previous web3 transport produced.
//! - **Error categories preserved.** Failures map onto the same five
//!   categories the legacy `From<web3::Error> for MetamaskError`
//!   matched on (Decoder \u2192 InvalidResponse, Transport, Rpc, Internal).
//! - **`!Send` JS provider isolation.** The raw `Provider` JsValue is
//!   `!Send`; we own it from a single async task on the wasm
//!   single-thread event loop and communicate with it via an
//!   `mpsc::unbounded` channel of `ProviderCommand`s. Cloning
//!   `Eip1193Provider` only clones the channel sender, so callers can
//!   freely move it across futures and tasks.
//! - **Single in-flight command per request.** The session-level
//!   serialisation that prevents chain-switch races is implemented
//!   one layer up in [`crate::MetamaskSession`]; this module makes no
//!   serialisation guarantees beyond FIFO command delivery.

use std::fmt;
use std::sync::Arc;

use common::executor::{spawn_local_abortable, AbortOnDropHandle};
use common::log::error;
use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use serde::de::DeserializeOwned;
use serde_json::Value as Json;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::metamask_error::Eip1193Error;

type CommandSender = mpsc::UnboundedSender<ProviderCommand>;
type CommandReceiver = mpsc::UnboundedReceiver<ProviderCommand>;
type ResultSender<T> = oneshot::Sender<Result<T, Eip1193Error>>;

/// Cross-thread wrapper over the `window.ethereum` EIP-1193 provider.
///
/// Drives the underlying `!Send` `Provider` JsValue from a dedicated
/// command loop spawned on the local async executor.
#[derive(Clone)]
pub struct Eip1193Provider {
    cmd_tx: CommandSender,
    /// Aborts the command loop when all clones are dropped.
    _abort: Arc<AbortOnDropHandle>,
}

impl Eip1193Provider {
    /// Detect the browser-injected EIP-1193 provider, if any.
    pub fn detect() -> Option<Self> {
        let raw = get_window_ethereum().ok().flatten()?;
        let (cmd_tx, cmd_rx) = mpsc::unbounded();
        let abort = spawn_local_abortable(Self::run_command_loop(raw, cmd_rx));

        Some(Eip1193Provider {
            cmd_tx,
            _abort: Arc::new(abort),
        })
    }

    /// Send a single JSON-RPC method call through the EIP-1193 channel
    /// and deserialise the response into `T`.
    pub async fn call_method<T>(&self, method: &str, params: Vec<Json>) -> Result<T, Eip1193Error>
    where
        T: DeserializeOwned,
    {
        let raw = self.call_raw(method, params).await?;
        serde_json::from_value(raw).map_err(|e| Eip1193Error::InvalidResponse(e.to_string()))
    }

    /// Send a single JSON-RPC method call and return the raw
    /// `serde_json::Value` payload, leaving deserialisation to the
    /// caller.
    pub async fn call_raw(&self, method: &str, params: Vec<Json>) -> Result<Json, Eip1193Error> {
        let (result_tx, result_rx) = oneshot::channel();
        let cmd = ProviderCommand::CallMethod {
            method: method.to_owned(),
            params,
            result_tx,
        };
        if self.cmd_tx.unbounded_send(cmd).is_err() {
            error!("EIP-1193 command channel closed");
            return Err(Eip1193Error::Internal);
        }
        match result_rx.await {
            Ok(result) => result,
            Err(_) => {
                error!("EIP-1193 result channel dropped");
                Err(Eip1193Error::Internal)
            },
        }
    }

    async fn run_command_loop(provider: BrowserProvider, mut cmd_rx: CommandReceiver) {
        while let Some(cmd) = cmd_rx.next().await {
            match cmd {
                ProviderCommand::CallMethod {
                    method,
                    params,
                    result_tx,
                } => {
                    let res = invoke_provider(&provider, &method, params).await;
                    result_tx.send(res).ok();
                },
            }
        }
    }
}

impl fmt::Debug for Eip1193Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("Eip1193Provider") }
}

enum ProviderCommand {
    CallMethod {
        method: String,
        params: Vec<Json>,
        result_tx: ResultSender<Json>,
    },
}

/// Serialise the `(method, params)` pair into a `RequestArguments` JS
/// object, await `provider.request(args)`, and translate the JS-side
/// outcome into the rich [`Eip1193Error`] taxonomy.
async fn invoke_provider(provider: &BrowserProvider, method: &str, params: Vec<Json>) -> Result<Json, Eip1193Error> {
    let params_js = serde_wasm_bindgen::to_value(&params).map_err(|e| Eip1193Error::InvalidResponse(e.to_string()))?;
    let params_array = js_sys::Array::from(&params_js);
    let args = RequestArguments::new(method.to_owned(), params_array);

    let promise = provider.request(args);
    match JsFuture::from(promise).await {
        Ok(js_val) => {
            // Successful responses are arbitrary JSON; deserialise and
            // hand back a `serde_json::Value` for the caller to refine.
            serde_wasm_bindgen::from_value(js_val).map_err(|e| Eip1193Error::InvalidResponse(e.to_string()))
        },
        Err(js_err) => Err(classify_js_error(js_err)),
    }
}

/// Map a `JsValue` thrown by `provider.request` into an [`Eip1193Error`].
///
/// EIP-1193 errors are documented as `{ code, message, data? }`. We
/// attempt to deserialise that shape first; anything that does not
/// parse falls back to a transport-category error carrying the
/// JS-side `toString()` rendering.
fn classify_js_error(js_err: JsValue) -> Eip1193Error {
    use jsonrpc_core::{Error as RpcError, ErrorCode as RpcErrorCode};

    #[derive(serde::Deserialize)]
    struct RpcErrorShape {
        code: i64,
        message: String,
        #[serde(default)]
        data: Option<Json>,
    }

    if let Ok(shape) = serde_wasm_bindgen::from_value::<RpcErrorShape>(js_err.clone()) {
        return Eip1193Error::Rpc(RpcError {
            code: RpcErrorCode::from(shape.code),
            message: shape.message,
            data: shape.data,
        });
    }

    let msg = js_err.as_string().unwrap_or_else(|| format!("{js_err:?}"));
    Eip1193Error::Transport(msg)
}

/// `BrowserProvider` is the typed JS handle for `window.ethereum`. The
/// `request(args)` method returns a `Promise` that we await via
/// [`wasm_bindgen_futures::JsFuture`].
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = "Object")]
    #[derive(Clone, Debug)]
    type BrowserProvider;

    #[wasm_bindgen(method, js_name = "request")]
    fn request(this: &BrowserProvider, args: RequestArguments) -> js_sys::Promise;
}

/// Plain JS object passed to `provider.request({ method, params })`.
#[wasm_bindgen]
struct RequestArguments {
    method: String,
    params: js_sys::Array,
}

#[wasm_bindgen]
impl RequestArguments {
    #[wasm_bindgen(constructor)]
    pub fn new(method: String, params: js_sys::Array) -> Self { Self { method, params } }

    #[wasm_bindgen(getter)]
    pub fn method(&self) -> String { self.method.clone() }

    #[wasm_bindgen(getter)]
    pub fn params(&self) -> js_sys::Array { self.params.clone() }
}

/// Inline JS shim that returns `window.ethereum` (or `undefined` when
/// no EIP-1193 provider is installed). Mirrors the helper that
/// previously lived in `web3::transports::eip_1193`.
#[wasm_bindgen(inline_js = "export function get_window_ethereum() { return window.ethereum; }")]
extern "C" {
    #[wasm_bindgen(catch)]
    fn get_window_ethereum() -> Result<Option<BrowserProvider>, JsValue>;
}
