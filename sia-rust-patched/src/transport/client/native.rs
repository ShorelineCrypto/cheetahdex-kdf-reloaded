use crate::transport::endpoints::{ConsensusTipRequest, SiaApiRequest, SiaApiRequestError};
use async_trait::async_trait;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use http::header::{HeaderMap, HeaderValue, InvalidHeaderValue, AUTHORIZATION};
use reqwest::Client as ReqwestClient;
use serde::Deserialize;
use thiserror::Error;
use url::Url;

use crate::transport::client::{ApiClient, ApiClientHelpers, Body as ClientBody, EndpointSchema, EndpointSchemaError};
use core::time::Duration;

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

    /// An error that may occur when using the `NativeClient`.
    /// Each variant is used exactly once and represents a unique logical path in the code.
    // TODO this can be broken into enum per method; Reqwest error handling also has significant updates
    // in newer versions that provide unique error types instead of a single "reqwest::Error"
    #[derive(Debug, Error)]
    pub enum ClientError {
        #[error("NativeClient::new: Failed to construct HTTP auth header: {0}")]
        InvalidHeader(#[from] InvalidHeaderValue),
        #[error("NativeClient::new: Failed to build reqwest::Client: {0}")]
        BuildClient(reqwest::Error),
        #[error("NativeClient::new: Failed to ping server with ConsensusTipRequest: {0}")]
        PingServer(Box<ClientError>),
        #[error("NativeClient::dispatcher: failed to convert request into schema: {0}")]
        RequestToSchema(#[from] SiaApiRequestError),
        #[error("NativeClient::process_schema: failed to build url: {0}")]
        SchemaBuildUrl(#[from] EndpointSchemaError),
        #[error("NativeClient::process_schema: Failed to build request: {0}")]
        SchemaBuildRequest(reqwest::Error),
        #[error("NativeClient::dispatcher: Failed to convert SiaApiRequest to reqwest::Request: {0}")]
        DispatcherBuildRequest(Box<ClientError>),
        #[error("NativeClient::dispatcher: Failed to execute reqwest::Request: {0}")]
        DispatcherExecuteRequest(reqwest::Error),
        #[error("NativeClient::dispatcher: Failed to deserialize response body:{body} reqwest error: {error} ")]
        DispatcherDeserializeJsonBody { error: serde_json::Error, body: String },
        #[error("NativeClient::dispatcher: Failed to deserialize response body as JSON or UTF-8: {0}")]
        DispatcherDeserializeUtf8Body(reqwest::Error),
        #[error("NativeClient::dispatcher: Expected:{expected_type} found 204 No Content")]
        DispatcherUnexpectedEmptyResponse { expected_type: String },
        #[error("NativeClient::dispatcher: unexpected HTTP status:{status} body:{body}")]
        DispatcherUnexpectedStatus { status: http::StatusCode, body: String },
    }
}

use error::*;

#[derive(Clone)]
pub struct Client {
    pub client: ReqwestClient,
    pub base_url: Url,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Conf {
    pub server_url: Url,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub timeout: Option<u64>,
}

#[async_trait]
impl ApiClient for Client {
    type Request = reqwest::Request;
    type Response = reqwest::Response;
    type Conf = Conf;

    type Error = ClientError;

    async fn new(conf: Self::Conf) -> Result<Self, Self::Error> {
        let mut headers = HeaderMap::new();
        if let Some(password) = &conf.password {
            let auth_value = format!("Basic {}", BASE64.encode(format!(":{}", password)));
            headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_value)?);
        }
        let timeout = conf.timeout.unwrap_or(30);
        let client = ReqwestClient::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(timeout))
            .build()
            .map_err(ClientError::BuildClient)?;

        let ret = Client {
            client,
            base_url: conf.server_url,
        };
        // Ping the server with ConsensusTipRequest to check if the client is working
        ret.dispatcher(ConsensusTipRequest)
            .await
            .map_err(|e| ClientError::PingServer(Box::new(e)))?;
        Ok(ret)
    }

    fn process_schema(&self, schema: EndpointSchema) -> Result<Self::Request, Self::Error> {
        let url = schema.build_url(&self.base_url)?;
        let req = match schema.body {
            ClientBody::None => self.client.request(schema.method.into(), url).build(),
            ClientBody::Utf8(body) => self.client.request(schema.method.into(), url).body(body).build(),
            ClientBody::Json(body) => self.client.request(schema.method.into(), url).json(&body).build(),
            ClientBody::Bytes(body) => self.client.request(schema.method.into(), url).body(body).build(),
        }
        .map_err(ClientError::SchemaBuildRequest)?;
        Ok(req)
    }

    async fn dispatcher<R: SiaApiRequest>(&self, request: R) -> Result<R::Response, Self::Error> {
        let request = self
            .process_schema(request.to_endpoint_schema()?)
            .map_err(|e| ClientError::DispatcherBuildRequest(Box::new(e)))?;

        let mut retries = 3;
        let response = loop {
            match self.client.execute(request.try_clone().unwrap()).await {
                Ok(resp) => break Ok(resp),
                Err(_) if retries > 0 => {
                    retries -= 1;
                    continue;
                },
                Err(e) => break Err(ClientError::DispatcherExecuteRequest(e)),
            }
        }?;

        // Check the response status and return the appropriate result
        match response.status() {
            // Attempt to deserialize the response body to the expected type if the status is OK
            reqwest::StatusCode::OK => {
                let utf8_body = response
                    .text()
                    .await
                    .map_err(ClientError::DispatcherDeserializeUtf8Body)?;
                serde_json::from_str(&utf8_body).map_err(|e| ClientError::DispatcherDeserializeJsonBody {
                    error: e,
                    body: utf8_body,
                })
            },
            // Handle empty responses; throw an error if the response is not expected to be empty
            reqwest::StatusCode::NO_CONTENT => {
                if let Some(resp_type) = R::is_empty_response() {
                    Ok(resp_type)
                } else {
                    Err(ClientError::DispatcherUnexpectedEmptyResponse {
                        expected_type: std::any::type_name::<R::Response>().to_string(),
                    })
                }
            },
            // Handle unexpected statuses eg, 400, 404, 500
            status => {
                // Extract the body, using map_err to format the error in case of failure
                let body = response
                    .text()
                    .await
                    .map_err(|e| format!("Failed to retrieve body: {}", e))
                    .unwrap_or_else(|e| e);

                Err(ClientError::DispatcherUnexpectedStatus { status, body })
            },
        }
    }
}

#[async_trait]
impl ApiClientHelpers for Client {}

// TODO these tests should not rely on the actual server - mock the server or use docker
#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::endpoints::AddressBalanceRequest;
    use crate::types::Address;

    use std::str::FromStr;
    use tokio;

    async fn init_client() -> Client {
        let conf = Conf {
            server_url: Url::parse("https://api.siascan.com/wallet/api").unwrap(),
            password: None,
            timeout: Some(10),
        };
        Client::new(conf).await.unwrap()
    }

    /// Helper function to setup the client and send a request
    async fn test_dispatch<R: SiaApiRequest>(request: R) -> R::Response {
        let api_client = init_client().await;
        api_client.dispatcher(request).await.unwrap()
    }

    #[tokio::test]
    async fn test_new_client() {
        let _api_client = init_client().await;
    }

    #[tokio::test]
    async fn test_api_consensus_tip() {
        // paranoid unit test - NativeClient::new already pings the server with ConsensusTipRequest
        let _response = test_dispatch(ConsensusTipRequest).await;
    }

    #[tokio::test]
    async fn test_api_address_balance() {
        let request = AddressBalanceRequest {
            address: Address::from_str("591fcf237f8854b5653d1ac84ae4c107b37f148c3c7b413f292d48db0c25a8840be0653e411f")
                .unwrap(),
        };
        let _response = test_dispatch(request).await;
    }

    #[tokio::test]
    async fn test_api_address_events() {
        use crate::transport::endpoints::AddressesEventsRequest;
        // This is an address provided by Nate that has various event types
        let request = AddressesEventsRequest {
            address: Address::from_str("4f28992465d1f993f68b389b5b4a097b0c14de5022110ae3efe198fc1c6c68fa96aaa49e4196")
                .unwrap(),
            limit: None,
            offset: None,
        };
        let _response = test_dispatch(request).await;
    }
}
