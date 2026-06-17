use crate::transport::client::{ApiClient, ApiClientHelpers, Body, EndpointSchema, EndpointSchemaError, SchemaMethod};
use crate::transport::endpoints::{ConsensusTipRequest, SiaApiRequest, SiaApiRequestError};

use async_trait::async_trait;
use http::StatusCode;
use serde::Deserialize;
use std::collections::HashMap;
use thiserror::Error;
use url::Url;

pub mod wasm_fetch;
use wasm_fetch::{Body as FetchBody, FetchError, FetchMethod, FetchRequest, FetchResponse};

pub mod error {
    use super::*;
    use crate::transport::client::helpers::generic_errors::*;

    pub type BroadcastTransactionError = BroadcastTransactionErrorGeneric<ClientError>;
    pub type UtxoFromTxidError = UtxoFromTxidErrorGeneric<ClientError>;
    pub type GetUnconfirmedTransactionError = GetUnconfirmedTransactionErrorGeneric<ClientError>;
    pub type GetMedianTimestampError = GetMedianTimestampErrorGeneric<ClientError>;
    pub type FindWhereUtxoSpentError = FindWhereUtxoSpentErrorGeneric<ClientError>;
    pub type FundTxSingleSourceError = FundTxSingleSourceErrorGeneric<ClientError>;
    pub type GetConsensusUpdatesError = GetConsensusUpdatesErrorGeneric<ClientError>;
    pub type GetUnspentOutputsError = GetUnspentOutputsErrorGeneric<ClientError>;
    pub type CurrentHeightError = CurrentHeightErrorGeneric<ClientError>;
    pub type SelectUtxosError = SelectUtxosErrorGeneric<ClientError>;
    pub type GetTransactionError = GetTransactionErrorGeneric<ClientError>;

    /// An error that may occur when using the `WasmClient`.
    /// Each variant is used exactly once and represents a unique logical path in the code.
    #[derive(Debug, Error)]
    pub enum ClientError {
        #[error("WasmClient::new: Failed to ping server with ConsensusTipRequest: {0}")]
        PingServer(Box<ClientError>),
        #[error("WasmClient::process_schema: failed to build url: {0}")]
        SchemaBuildUrl(#[from] EndpointSchemaError),
        #[error("WasmClient::process_schema: unsupported EndpointSchema.method: {0:?}")]
        SchemaUnsupportedMethod(EndpointSchema),
        #[error("WasmClient::dispatcher: Failed to generate EndpointSchema from SiaApiRequest: {0}")]
        DispatcherGenerateSchema(#[from] SiaApiRequestError),
        #[error("WasmClient::dispatcher: process_schema failed: {0}")]
        DispatcherProcessSchema(Box<ClientError>),
        #[error("WasmClient::dispatcher: Failed to execute request: {0}")]
        DispatcherExecuteRequest(#[from] FetchError),
        #[error("WasmClient::dispatcher: expected utf-8 or JSON in response body, found octet-stream: {0:?}")]
        DispatcherUnexpectedBodyBytes(Vec<u8>),
        #[error("WasmClient::dispatcher: expected utf-8 or JSON in response body, found empty body")]
        DispatcherUnexpectedBodyEmpty,
        #[error("WasmClient::dispatcher: failed to deserialize response body from JSON: {0}")]
        DispatcherDeserializeBodyJson(serde_json::Error),
        #[error("WasmClient::dispatcher: failed to deserialize response body from string: {0}")]
        DispatcherDeserializeBodyUtf8(serde_json::Error),
        #[error("WasmClient::dispatcher: unexpected HTTP status:{status} body:{body:?}")]
        DispatcherUnexpectedHttpStatus {
            status: StatusCode,
            body: Option<FetchBody>,
        },
        #[error("WasmClient::dispatcher: Expected:{expected_type} found 204 No Content")]
        DispatcherUnexpectedEmptyResponse { expected_type: String },
    }
}

use error::*;

#[derive(Clone)]
pub struct Client {
    pub base_url: Url,
    pub headers: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Conf {
    pub server_url: Url,
    #[serde(default)]
    pub headers: HashMap<String, String>,
}

#[async_trait]
impl ApiClient for Client {
    type Request = FetchRequest;
    type Response = FetchResponse;
    type Conf = Conf;
    type Error = ClientError;

    async fn new(conf: Self::Conf) -> Result<Self, Self::Error> {
        let client = Client {
            base_url: conf.server_url,
            headers: conf.headers,
        };
        // Ping the server with ConsensusTipRequest to check if the client is working
        client
            .dispatcher(ConsensusTipRequest)
            .await
            .map_err(|e| ClientError::PingServer(Box::new(e)))?;
        Ok(client)
    }

    fn process_schema(&self, schema: EndpointSchema) -> Result<Self::Request, Self::Error> {
        let url = schema.build_url(&self.base_url)?;
        let method = match schema.method {
            SchemaMethod::Get => FetchMethod::Get,
            SchemaMethod::Post => FetchMethod::Post,
            _ => return Err(ClientError::SchemaUnsupportedMethod(schema.clone())),
        };
        let body = match schema.body {
            Body::Utf8(body) => Some(FetchBody::Utf8(body)),
            Body::Json(body) => Some(FetchBody::Json(body)),
            Body::Bytes(body) => Some(FetchBody::Bytes(body)),
            Body::None => None,
        };
        Ok(FetchRequest {
            uri: url,
            method,
            headers: self.headers.clone(),
            body,
        })
    }

    // Dispatcher function that converts the request and handles execution
    async fn dispatcher<R: SiaApiRequest>(&self, request: R) -> Result<R::Response, Self::Error> {
        // Generate EndpointSchema from the SiaApiRequest
        let schema = request.to_endpoint_schema()?;

        // Convert the SiaApiRequest to FetchRequest
        let request = self
            .process_schema(schema)
            .map_err(|e| ClientError::DispatcherProcessSchema(Box::new(e)))?;

        // Execute the FetchRequest
        let response = request.execute().await?;

        match response.status {
            // Deserialize the response body if 200 OK
            StatusCode::OK => {
                let response_body = match response.body {
                    Some(FetchBody::Json(body)) => {
                        serde_json::from_value(body).map_err(ClientError::DispatcherDeserializeBodyJson)?
                    },
                    Some(FetchBody::Utf8(body)) => {
                        serde_json::from_str(&body).map_err(ClientError::DispatcherDeserializeBodyUtf8)?
                    },
                    Some(FetchBody::Bytes(bytes)) => return Err(ClientError::DispatcherUnexpectedBodyBytes(bytes)),
                    None => return Err(ClientError::DispatcherUnexpectedBodyEmpty),
                };
                Ok(response_body)
            },
            // Return an EmptyResponse if 204 NO CONTENT
            StatusCode::NO_CONTENT => {
                if let Some(resp_type) = R::is_empty_response() {
                    Ok(resp_type)
                } else {
                    Err(ClientError::DispatcherUnexpectedEmptyResponse {
                        expected_type: std::any::type_name::<R::Response>().to_string(),
                    })
                }
            },
            // Handle unexpected HTTP statuses eg, 400, 404, 500
            status => Err(ClientError::DispatcherUnexpectedHttpStatus {
                status,
                body: response.body,
            }),
        }
    }
}

// Just this is needed to implement the `ApiClientHelpers` trait
// unless custom implementations for the traits methods are needed
#[async_trait]
impl ApiClientHelpers for Client {}

#[cfg(all(target_arch = "wasm32", test))]
mod wasm_tests {
    use super::*;
    use std::str::FromStr;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_browser);

    async fn init_client() -> Client {
        let conf = Conf {
            server_url: Url::parse("https://api.siascan.com/wallet/api").unwrap(),
            headers: HashMap::new(),
        };
        Client::new(conf).await.unwrap()
    }

    #[wasm_bindgen_test]
    async fn test_new_client() {
        let _api_client = init_client().await;
    }

    #[wasm_bindgen_test]
    async fn test_address_balance() {
        use crate::transport::endpoints::AddressBalanceRequest;
        use crate::types::Address;

        let request = AddressBalanceRequest {
            address: Address::from_str("591fcf237f8854b5653d1ac84ae4c107b37f148c3c7b413f292d48db0c25a8840be0653e411f")
                .unwrap(),
        };
        let client = init_client().await;
        let _response = client.dispatcher(request).await;
    }
}
