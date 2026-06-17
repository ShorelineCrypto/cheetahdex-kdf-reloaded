use crate::transport::endpoints::SiaApiRequest;
use async_trait::async_trait;
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub use wasm::*;

pub(crate) mod helpers;
pub use helpers::ApiClientHelpers;

// Client implementation is generalized
// This allows for different client implementations (e.g., WebSocket, libp2p, etc.)
// Any client implementation must implement the ApiClient trait and optionally ApiClientHelpers
#[async_trait]
pub trait ApiClient: Clone {
    type Request;
    type Response;
    type Conf;
    type Error: std::error::Error + std::fmt::Debug;

    async fn new(conf: Self::Conf) -> Result<Self, Self::Error>
    where
        Self: Sized;

    fn process_schema(&self, schema: EndpointSchema) -> Result<Self::Request, Self::Error>;

    // TODO default implementation should be possible if Execute::Response is a serde deserializable type
    async fn dispatcher<R: SiaApiRequest>(&self, request: R) -> Result<R::Response, Self::Error>;
}

// Not all client implementations will have an exact equivalent of HTTP methods
// However, the client implementation should be able to map the HTTP methods to its own methods
#[derive(Clone, Debug)]
pub enum SchemaMethod {
    Get,
    Post,
    Put,
    Delete,
}

impl From<SchemaMethod> for http::Method {
    fn from(method: SchemaMethod) -> Self {
        match method {
            SchemaMethod::Get => http::Method::GET,
            SchemaMethod::Post => http::Method::POST,
            SchemaMethod::Put => http::Method::PUT,
            SchemaMethod::Delete => http::Method::DELETE,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EndpointSchema {
    pub path_schema: String, // The endpoint path template (e.g., /api/transactions/{id})
    pub path_params: Option<HashMap<String, String>>, // Optional parameters to replace in the path (e.g., /{key} becomes /value)
    pub query_params: Option<HashMap<String, String>>, // Optional query parameters to add to the URL (e.g., ?key=value)
    pub method: SchemaMethod,                         // The method (e.g., Get, Post, Put, Delete)
    pub body: Body,                                   // Optional body for POST and POST-like requests
}

pub struct EndpointSchemaBuilder {
    path_schema: String,
    path_params: Option<HashMap<String, String>>,
    query_params: Option<HashMap<String, String>>,
    method: SchemaMethod,
    body: Body,
}

impl EndpointSchemaBuilder {
    pub fn new(path_schema: String, method: SchemaMethod) -> Self {
        Self {
            path_schema,
            path_params: None,
            query_params: None,
            method,
            body: Body::None,
        }
    }

    pub fn path_params(mut self, path_params: HashMap<String, String>) -> Self {
        self.path_params = Some(path_params);
        self
    }

    pub fn query_params(mut self, query_params: Option<HashMap<String, String>>) -> Self {
        self.query_params = query_params;
        self
    }

    pub fn body(mut self, body: Body) -> Self {
        self.body = body;
        self
    }

    pub fn build(self) -> EndpointSchema {
        EndpointSchema {
            path_schema: self.path_schema,
            path_params: self.path_params,
            query_params: self.query_params,
            method: self.method,
            body: self.body,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Body {
    Utf8(String),
    Json(JsonValue),
    Bytes(Vec<u8>),
    None,
}

#[derive(Debug, Error)]
pub enum EndpointSchemaError {
    #[error("EndpointSchema::build_url: failed to parse Url from constructed path: {0}")]
    ParseUrl(#[from] url::ParseError),
}

impl EndpointSchema {
    // Safely build the URL using percent-encoding for path params
    pub fn build_url(&self, base_url: &Url) -> Result<Url, EndpointSchemaError> {
        let mut path = self.path_schema.to_string();

        // Replace placeholders in the path with encoded values if path_params are provided
        if let Some(params) = &self.path_params {
            for (key, value) in params {
                let encoded_value = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
                path = path.replace(&format!("{{{}}}", key), &encoded_value); // Use {} for parameters
            }
        }

        // Combine base_url with the constructed path
        let mut url = base_url.join(&path)?;

        // Add query parameters if any
        if let Some(query_params) = &self.query_params {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query_params {
                let encoded_value = utf8_percent_encode(value, NON_ALPHANUMERIC).to_string();
                pairs.append_pair(key, &encoded_value);
            }
        }

        Ok(url)
    }
}
