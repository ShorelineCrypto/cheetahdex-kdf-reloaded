use futures::channel::oneshot;
use http::{HeaderMap, StatusCode};
use js_sys::Uint8Array;
use serde_json::Value as JsonValue;
use serde_wasm_bindgen;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;
use url::Url;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{spawn_local, JsFuture};
use web_sys::{Request as JsRequest, RequestInit, Response as JsResponse, Window, WorkerGlobalScope};

/// This is loosely based on the "mm2_net" crate found within Komodo DeFi Framework.
/// There is some extra work involved here because `Send` is required for Komodo DeFi Framework.

/// Get only the first line of the error.
/// Generally, the `JsValue` error contains the stack trace of an error.
/// This function cuts off the stack trace.
pub fn stringify_js_error(error: &JsValue) -> String {
    format!("{:?}", error)
        .lines()
        .next()
        .map(|e| e.to_owned())
        .unwrap_or_default()
}

#[derive(Debug, Error)]
pub enum FetchError {
    #[error("Error deserializing '{uri}' response: {error}")]
    ErrorDeserializing { uri: String, error: String },

    #[error("Transport '{uri}' error: {error}")]
    Transport { uri: String, error: String },

    #[error("Invalid status code in response")]
    InvalidStatusCode(#[from] http::status::InvalidStatusCode),

    #[error("Invalid headers in response: {0}")]
    InvalidHeadersInResponse(String),

    #[error("Invalid body: {0}")]
    InvalidBody(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

pub enum FetchMethod {
    Get,
    Post,
}

impl FetchMethod {
    fn as_str(&self) -> &'static str {
        match self {
            FetchMethod::Get => "GET",
            FetchMethod::Post => "POST",
        }
    }
}

#[derive(Clone, Debug)]
pub enum Body {
    Utf8(String),
    Json(JsonValue),
    Bytes(Vec<u8>),
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Body::Utf8(text) => write!(f, "Utf8: {}", text),
            Body::Json(json) => write!(f, "Json: {}", json),
            Body::Bytes(bytes) => write!(f, "Bytes: {:?}", bytes), // Use Debug formatting for Vec<u8>
        }
    }
}

impl Body {
    fn into_js_value(self) -> Result<JsValue, FetchError> {
        match self {
            Body::Utf8(string) => Ok(JsValue::from_str(&string)),
            Body::Bytes(bytes) => {
                let js_array = Uint8Array::from(bytes.as_slice());
                Ok(js_array.into())
            },
            Body::Json(json) => serde_wasm_bindgen::to_value(&json)
                .map_err(|e| FetchError::InvalidBody(format!("Failed to serialize body to Json. err: {}", e))),
        }
    }
}

pub type FetchResult = Result<FetchResponse, FetchError>;

pub struct FetchResponse {
    pub status: StatusCode,
    pub headers: HashMap<String, String>,
    pub body: Option<Body>,
}

impl FetchResponse {
    pub async fn from_js_response(response: JsResponse) -> Result<Self, FetchError> {
        let status = StatusCode::from_u16(response.status()).map_err(FetchError::InvalidStatusCode)?;

        // TODO newer versions of js_sys allow direct iter over response.headers().entries()
        let header_js_map = js_sys::Map::from(JsValue::from(response.headers()));
        let mut header_map = HashMap::new();

        for header in header_js_map.entries() {
            let header = header.map_err(|e| FetchError::InvalidHeadersInResponse(stringify_js_error(&e)))?;
            let kv = js_sys::Array::from(&header);
            let key = kv.get(0).as_string().ok_or(FetchError::InvalidHeadersInResponse(
                "Key is not utf-8 string".to_string(),
            ))?;
            let value = kv.get(1).as_string().ok_or(FetchError::InvalidHeadersInResponse(
                "Value is not utf-8 string".to_string(),
            ))?;
            header_map.insert(key, value);
        }

        let content_type = header_map.get("content-type").map(|v| v.as_str()).unwrap_or("");

        let body = if content_type.contains("application/json") || content_type.contains("text/") {
            let text_promise = response
                .text()
                .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;
            let text_js_value = JsFuture::from(text_promise)
                .await
                .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;
            let text = text_js_value
                .as_string()
                .ok_or_else(|| FetchError::InvalidBody("Failed to convert body to string".to_string()))?;
            Some(Body::Utf8(text))
        } else if content_type.contains("application/octet-stream") {
            let buffer_promise = response
                .array_buffer()
                .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;
            let buffer_js_value = JsFuture::from(buffer_promise)
                .await
                .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;
            let array = js_sys::Uint8Array::new(&buffer_js_value);
            Some(Body::Bytes(array.to_vec()))
        } else {
            // No body or unsupported content-type
            None
        };
        Ok(FetchResponse {
            status,
            headers: header_map,
            body,
        })
    }
}

pub struct FetchRequest {
    pub uri: Url,
    pub method: FetchMethod,
    pub headers: HashMap<String, String>,
    pub body: Option<Body>,
}

impl FetchRequest {
    pub fn get(uri: Url) -> FetchRequest {
        FetchRequest {
            uri,
            method: FetchMethod::Get,
            headers: HashMap::new(),
            body: None,
        }
    }

    pub fn post(uri: Url) -> FetchRequest {
        FetchRequest {
            uri,
            method: FetchMethod::Post,
            headers: HashMap::new(),
            body: None,
        }
    }

    pub fn body_utf8(mut self, body: String) -> FetchRequest {
        self.body = Some(Body::Utf8(body));
        self
    }

    pub fn header_map(mut self, header_map: HeaderMap) -> FetchRequest {
        for (key, value) in header_map.iter() {
            if let Ok(val) = value.to_str() {
                self.headers.insert(key.as_str().to_owned(), val.to_owned());
            }
        }
        self
    }

    async fn fetch(request: Self) -> FetchResult {
        let uri = request.uri.to_string();

        let mut req_init = RequestInit::new();
        req_init.method(request.method.as_str());

        let body = request.body.map(|b| b.into_js_value()).transpose()?;
        req_init.body(body.as_ref());

        let js_request = JsRequest::new_with_str_and_init(&uri, &req_init)
            .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;

        for (hkey, hval) in request.headers {
            js_request
                .headers()
                .set(&hkey, &hval)
                .map_err(|e| FetchError::Internal(stringify_js_error(&e)))?;
        }

        let request_promise = compatible_fetch_with_request(&js_request)?;

        let future = JsFuture::from(request_promise);
        let resp_value = future.await.map_err(|e| FetchError::Transport {
            uri: uri.clone(),
            error: stringify_js_error(&e),
        })?;
        let js_response: JsResponse = match resp_value.dyn_into() {
            Ok(res) => res,
            Err(origin_val) => {
                let error = format!("Error casting {:?} to 'JsResponse'", origin_val);
                return Err(FetchError::Internal(error));
            },
        };

        let fetch_response = FetchResponse::from_js_response(js_response).await?;
        Ok(fetch_response)
    }

    pub async fn execute(self) -> FetchResult {
        let (tx, rx) = oneshot::channel();
        Self::spawn_fetch_request(self, tx);
        match rx.await {
            Ok(res) => res,
            Err(_e) => Err(FetchError::Internal("Spawned future has been canceled".to_owned())),
        }
    }

    fn spawn_fetch_request(request: Self, tx: oneshot::Sender<FetchResult>) {
        let fut = async move {
            let result = Self::fetch(request).await;
            tx.send(result).ok();
        };

        spawn_local(fut);
    }
}

/// This function is a wrapper around the `fetch_with_request`, providing compatibility across
/// different execution environments, such as window and worker.
fn compatible_fetch_with_request(js_request: &web_sys::Request) -> Result<js_sys::Promise, FetchError> {
    let global = js_sys::global();

    if let Some(scope) = global.dyn_ref::<Window>() {
        return Ok(scope.fetch_with_request(js_request));
    }

    if let Some(scope) = global.dyn_ref::<WorkerGlobalScope>() {
        return Ok(scope.fetch_with_request(js_request));
    }

    Err(FetchError::Internal("Unknown WASM environment.".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    #[wasm_bindgen_test]
    async fn test_build_js_request_ok() {
        let uri = "http://example.com";
        let mut req_init = RequestInit::new();
        req_init.method("GET");
        let _js_request = JsRequest::new_with_str_and_init(&uri, &req_init).unwrap();
    }

    // further unit tests could be implemented for the Err case based on the spec
    // https://fetch.spec.whatwg.org/#dom-request
    #[wasm_bindgen_test]
    async fn test_build_js_request_err_invalid_uri() {
        let uri = "http://user:password@example.com";
        let mut req_init = RequestInit::new();
        req_init.method("GET");
        let err = JsRequest::new_with_str_and_init(&uri, &req_init)
            .map_err(|e| FetchError::Internal(stringify_js_error(&e)))
            .unwrap_err();
        assert!(err.to_string().contains("is an url with embedded credentials"));
    }
}
