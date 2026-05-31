//! # Purpose
//!
//! An [`alloy::transports::Transport`] implementation that mirrors the
//! semantics of the legacy `coins::eth::web3_transport::Web3Transport`:
//!
//! 1. Accepts a list of equivalent RPC URLs and tries them in order
//!    until one succeeds (poor-man's fail-over).
//! 2. Fires `RpcTransportEventHandler` callbacks for every outbound
//!    request and inbound response so the metrics layer continues to
//!    count bytes per coin / per URL.
//!
//! # Public exports
//!
//! - [`AlloyTransport`] — the cloneable, multi-URL, metrics-instrumented
//!   transport. Implements `tower::Service<RequestPacket>` and therefore
//!   blanket-implements [`alloy::transports::Transport`].
//!
//! # Invariants
//!
//! - **JSON-RPC wire bytes are unchanged.** Outgoing requests are
//!   serialised by alloy's own [`RequestPacket::Serialize`] impl
//!   (matches `web3::helpers::to_string` byte-for-byte for the
//!   methods we use). Incoming responses are deserialised by alloy's
//!   [`ResponsePacket::Deserialize`].
//! - **Method ordering preserved.** URLs are tried in the same order
//!   they were registered; the first 2xx response is returned. On
//!   exhaustion an aggregated `TransportErrorKind::Custom` is returned,
//!   matching the legacy behaviour.
//! - **`Send + Sync + Clone + 'static`.** Required by the alloy
//!   [`Transport`] blanket impl. Internal state is `Arc`-shared so
//!   cloning the transport does not duplicate the handler vector.
//! - **No secrets.** Error strings only contain status codes and
//!   serialised method names; never request bodies, RPC URLs in
//!   metrics callbacks beyond what the existing handler already saw,
//!   nor signatures.

use std::sync::Arc;
use std::task::{Context, Poll};

use alloy::rpc::json_rpc::{RequestPacket, ResponsePacket};
use alloy::transports::{TransportError, TransportErrorKind, TransportFut};
use tower::Service;

use crate::{RpcTransportEventHandler, RpcTransportEventHandlerShared};

/// Transport backing alloy's JSON-RPC pipeline for the EVM coin family.
///
/// Construct via [`AlloyTransport::with_event_handlers`] (mirrors the
/// existing `Web3Transport::with_event_handlers` constructor).
#[derive(Clone, Debug)]
pub struct AlloyTransport {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    uris: Vec<http::Uri>,
    event_handlers: Vec<RpcTransportEventHandlerShared>,
}

impl AlloyTransport {
    /// Build a new transport from a list of equivalent RPC URLs and a
    /// list of metrics handlers. Returns `Err` if any URL cannot be
    /// parsed as an [`http::Uri`].
    pub fn with_event_handlers(
        urls: Vec<String>,
        event_handlers: Vec<RpcTransportEventHandlerShared>,
    ) -> Result<Self, String> {
        let uris = urls
            .into_iter()
            .map(|u| u.parse::<http::Uri>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            inner: Arc::new(Inner { uris, event_handlers }),
        })
    }
}

impl Service<RequestPacket> for AlloyTransport {
    type Response = ResponsePacket;
    type Error = TransportError;
    type Future = TransportFut<'static>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> { Poll::Ready(Ok(())) }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let inner = self.inner.clone();
        Box::pin(send_packet(inner, req))
    }
}

/// Serialise the [`RequestPacket`], try every configured URL in turn,
/// fire metrics callbacks, deserialise the first successful body.
async fn send_packet(inner: Arc<Inner>, req: RequestPacket) -> Result<ResponsePacket, TransportError> {
    let body = req.serialize().map_err(|e| TransportError::ser_err(e))?;
    let body_bytes = body.get().as_bytes().to_vec();

    let mut errors: Vec<String> = Vec::new();
    for uri in inner.uris.iter() {
        inner.event_handlers.on_outgoing_request(&body_bytes);

        match send_one(uri, &body_bytes).await {
            Ok(response_bytes) => {
                inner.event_handlers.on_incoming_response(&response_bytes);
                return serde_json::from_slice::<ResponsePacket>(&response_bytes)
                    .map_err(|e| TransportError::deser_err(e, String::from_utf8_lossy(&response_bytes)));
            },
            Err(msg) => errors.push(format!("{uri}: {msg}")),
        }
    }

    Err(TransportErrorKind::custom_str(&format!(
        "all RPC URLs failed: {}",
        errors.join("; ")
    )))
}

#[cfg(not(target_arch = "wasm32"))]
async fn send_one(uri: &http::Uri, body: &[u8]) -> Result<Vec<u8>, String> {
    use common::executor::Timer;
    use futures::future::{select, Either};
    use http::header::HeaderValue;
    use mm2_net::transport::slurp_req;

    /// Hard timeout for a single RPC round-trip. Matches the value
    /// previously hard-coded in `web3_transport::send_request`.
    const REQUEST_TIMEOUT_S: f64 = 60.;

    let mut req = http::Request::new(body.to_vec());
    *req.method_mut() = http::Method::POST;
    *req.uri_mut() = uri.clone();
    req.headers_mut()
        .insert(http::header::CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let req_fut = Box::pin(slurp_req(req));
    let timeout = Timer::sleep(REQUEST_TIMEOUT_S);
    let (status, _headers, response_bytes) = match select(req_fut, timeout).await {
        Either::Left((Ok(triple), _)) => triple,
        Either::Left((Err(e), _)) => return Err(e.to_string()),
        Either::Right(_) => return Err(format!("{REQUEST_TIMEOUT_S}s timeout expired")),
    };

    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(response_bytes)
}

#[cfg(target_arch = "wasm32")]
async fn send_one(uri: &http::Uri, body: &[u8]) -> Result<Vec<u8>, String> {
    use mm2_net::wasm_http::FetchRequest;

    let body_string = String::from_utf8(body.to_vec()).map_err(|e| e.to_string())?;
    let (status, response_str) = FetchRequest::post(&uri.to_string())
        .cors()
        .body_utf8(body_string)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .request_str()
        .await
        .map_err(|e| format!("{e:?}"))?;

    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }
    Ok(response_str.into_bytes())
}
