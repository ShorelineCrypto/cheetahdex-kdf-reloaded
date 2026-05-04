use crate::transport::client::{Body, EndpointSchema, EndpointSchemaBuilder, SchemaMethod};
use crate::types::{Address, ApiApplyUpdate, BlockId, ChainIndex, Currency, Event, Hash256, SiacoinElement,
                   SiacoinOutputId, V1Transaction, V2Transaction};
use crate::utils::deserialize_null_as_empty_vec;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

const ENDPOINT_ADDRESSES_BALANCE: &str = "api/addresses/{address}/balance";
const ENDPOINT_ADDRESSES_EVENTS: &str = "api/addresses/{address}/events";
const ENDPOINT_ADDRESSES_UTXOS_SIACOIN: &str = "api/addresses/{address}/outputs/siacoin";

const ENDPOINT_CONSENSUS_TIP: &str = "api/consensus/tip";
const ENDPOINT_CONSENSUS_INDEX: &str = "api/consensus/index/{height}";
const ENDPOINT_CONSENSUS_TIPSTATE: &str = "api/consensus/tipstate";
const ENDPOINT_CONSENSUS_UPDATES: &str = "api/consensus/updates/{height}::{hash}";

const ENDPOINT_DEBUG_MINE: &str = "api/debug/mine";

const ENDPOINT_EVENTS: &str = "api/events/{txid}";

const ENDPOINT_OUTPUTS_SIACOIN_SPENT: &str = "api/outputs/siacoin/{output_id}/spent";

const ENDPOINT_TXPOOL_BROADCAST: &str = "api/txpool/broadcast";
const ENDPOINT_TXPOOL_FEE: &str = "api/txpool/fee";
const ENDPOINT_TXPOOL_TRANSACTIONS: &str = "api/txpool/transactions";

pub trait SiaApiRequest: Send {
    type Response: DeserializeOwned;

    // Applicable for requests that return HTTP 204 No Content
    fn is_empty_response() -> Option<Self::Response> { None }

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError>;
}

// TODO it's not ideal that all endpoints share this error type because we may have "endpoints" in the
// future that aren't a 1:1 mapping of walletd endpoints and may require different error handling per
// endpoint or per data source. That is why this holds a String, request, that does not follow the typical
// error handling pattern of the rest of the library.
#[derive(Debug, Error)]
pub enum SiaApiRequestError {
    #[error("SiaApiRequest::to_endpoint_schema: failed to serialize request:{request} to JSON: {error}")]
    Serde { error: serde_json::Error, request: String },
}

/// Represents the request-response pair for fetching the current consensus tip of the Sia network.
///
/// # Walletd Endpoint
/// `GET /consensus/tip`
///
/// # Description
/// Returns the current consensus tip of the Sia network. The consensus tip includes the current block's height
/// and its block ID, representing the latest state of the blockchain.
///
/// # Response
/// - The response is a `ChainIndex`, which contains the block's height and ID.
///   This corresponds to the `types.ChainIndex` type in Go.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/api/server.go#L158)
/// - [Go Source for the ChainIndex Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L194)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ConsensusTipRequest;

impl SiaApiRequest for ConsensusTipRequest {
    type Response = ChainIndex;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        Ok(EndpointSchemaBuilder::new(ENDPOINT_CONSENSUS_TIP.to_owned(), SchemaMethod::Get).build())
    }
}

/// Represents the request-response pair for fetching a BlockId from a provided height.
///
/// # Walletd Endpoint
/// `GET /consensus/index/{height}`
///
/// # Description
/// Returns the ChainIndex of the Block at the provided height. The consensus tip includes the block's height
/// and its BlockId.
///
/// # Response
/// - The response is a `ChainIndex`, which contains the block's height and ID.
///   This corresponds to the `types.ChainIndex` type in Go.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/api/server.go#L158)
/// - [Go Source for the ChainIndex Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L194)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ConsensusIndexRequest {
    pub height: u64,
}

impl SiaApiRequest for ConsensusIndexRequest {
    type Response = ChainIndex;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        // Create the path_params HashMap to substitute {height} in the path schema
        let mut path_params = HashMap::new();
        path_params.insert("height".to_owned(), self.height.to_string());

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_CONSENSUS_INDEX.to_owned(), SchemaMethod::Get)
                .path_params(path_params)
                .build(),
        )
    }
}

/// Represents the request-response pair for fetching the current consensus tipstate of the Sia network.
///
/// # Walletd Endpoint
/// `GET /consensus/tipstate`
///
/// # Description
/// Returns the current consensus state of the Sia network.
///
/// # Response
/// - The response is a `ConsensusTipstateResponse`, which is a partial implementation of the `consensus.State` type in Go.
///   This response includes the current block's height and ID, as well as timestamps of the previous 11 blocks.
///   The median of the provided timestamps is the medianTimestamp used to evaluate SpendPolicy::After.
///   SpendPolicy::After(time) evaluates to true if `time > medianTimestamp`.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/d71cf08d4579ba952c51e535f988000e43ed8722/api/server.go#L162)
/// - [Go Source for the consensus.State Type](https://github.com/SiaFoundation/core/blob/00682daf422864b250b6bc750d4229dd76a8632d/consensus/state.go#L111)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ConsensusTipstateRequest;

impl SiaApiRequest for ConsensusTipstateRequest {
    type Response = ConsensusTipstateResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        Ok(EndpointSchemaBuilder::new(ENDPOINT_CONSENSUS_TIPSTATE.to_owned(), SchemaMethod::Get).build())
    }
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusTipstateResponse {
    pub index: ChainIndex,
    pub prev_timestamps: Vec<DateTime<Utc>>,
}

/// Represents the request-response pair for fetching consensus updates of the Sia network.
///
/// # Walletd Endpoint
/// `GET /consensus/updates/{height}::{hash}`
///
/// # Description
/// Returns consensus updates since the specific block height until the current consensus tip.
///
/// # Response
/// - The response is a `ConsensusUpdatesResponse`, which is a partial implementation of the `ConsensusUpdatesResponse` type in Go.
///   This endpoint only partially implements the Go type provided as response.
///   This endpoints implements the minimum required fields to facilitate the logic of ApiClientHelpers::find_where_utxo_spent method.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/d71cf08d4579ba952c51e535f988000e43ed8722/api/server.go#L162)
/// - [Go Source for the consensus.State Type](https://github.com/SiaFoundation/core/blob/00682daf422864b250b6bc750d4229dd76a8632d/consensus/state.go#L111)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct ConsensusUpdatesRequest {
    pub height: u64,
    pub block_hash: BlockId,
    pub limit: Option<i64>,
}

impl SiaApiRequest for ConsensusUpdatesRequest {
    type Response = ConsensusUpdatesResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        // Create the path_params HashMap to substitute {height} and {hash} in the path schema
        let mut path_params = HashMap::new();
        path_params.insert("height".to_owned(), self.height.to_string());
        path_params.insert("hash".to_owned(), format!("{}", self.block_hash.0));

        let mut query_params = HashMap::new();
        if let Some(limit) = self.limit {
            query_params.insert("limit".to_owned(), limit.to_string());
        }

        let query_params_option = (!query_params.is_empty()).then_some(query_params);

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_CONSENSUS_UPDATES.to_owned(), SchemaMethod::Get)
                .path_params(path_params) // Set the path params containing the height and hash
                .query_params(query_params_option) // Set the query params for ?limit=
                .build(),
        )
    }
}

#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusUpdatesResponse {
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub applied: Vec<ApiApplyUpdate>,
}

/// Represents the request-response pair for fetching the balance of an individual address.
///
/// # Walletd Endpoint
/// `GET /addresses/:addr/balance`
///
/// # Description
/// Retrieves the balance of the specified address. The behavior of this endpoint depends on the index mode:
/// - **Personal Index Mode**: The address must be associated with an existing wallet.
/// - **Full Index Mode**: The balance of any address can be checked.
///
/// # Fields
/// - `address`: The address for which to fetch the balance. In Go, this corresponds to `types.Address`.
///   - [Go Source for Address Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L165)
///
/// # Response
/// - The response provides two fields, `siacoins` and `immature_siacoins`, each representing the balance in Siacoins.
///   These are string representations of the `Currency` type in Go.
///   - [Go Source for Currency Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/currency.go#L26)
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/api/server.go#L752)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct AddressBalanceRequest {
    pub address: Address,
}

impl SiaApiRequest for AddressBalanceRequest {
    type Response = AddressBalanceResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        let mut path_params = HashMap::new();
        path_params.insert("address".to_owned(), self.address.to_string());

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_ADDRESSES_BALANCE.to_owned(), SchemaMethod::Get)
                .path_params(path_params) // Set the path parameters for the address
                .build(),
        )
    }
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct AddressBalanceResponse {
    pub siacoins: Currency,
    #[serde(rename = "immatureSiacoins")]
    pub immature_siacoins: Currency,
}

/// Represents the request-response pair for fetching a specific event by transaction ID (txid).
///
/// # Walletd Endpoint
/// `GET /events/:id`
///
/// # Description
/// Fetches an event based on the provided transaction ID (txid).
///
/// # Fields
/// - `txid`: The transaction ID for which to fetch the event. In Go, this corresponds to `types.Hash256`.
///   - [Go Source for Hash256](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L63)
///
/// # Response
/// - The response is an `Event` in Rust, corresponding to `types.Event` in Go.
///   - [Go Source for Event](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/wallet/wallet.go#L14)
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/134a28b063df60a687899ac33aa373bf461480bc/api/server.go#L828)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct GetEventRequest {
    pub txid: Hash256,
}

impl SiaApiRequest for GetEventRequest {
    type Response = Event;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        // Create the path_params HashMap to substitute {txid} in the path schema
        let mut path_params = HashMap::new();
        path_params.insert("txid".to_owned(), self.txid.to_string());

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_EVENTS.to_owned(), SchemaMethod::Get)
                .path_params(path_params) // Set the path params containing the txid
                .build(),
        )
    }
}

/// Represents the request-response pair for fetching events for a specific address.
///
/// # Walletd Endpoint
/// `GET /addresses/:addr/events`
///
/// # Fields
/// - `addr`: (`types.Address` in Go) the address for which events are fetched.
/// - `limit`: (`int` type in Go) optional limit for the number of results.
/// - `offset`: (`int` type in Go) optional offset for paginated results.
///
/// # Response
/// - `[]types.Event` in Go corresponds to `Vec<Event>` in Rust.
///   - An event represents an on-chain event capable of influencing the state of a wallet.
///   - As per comments in the Go source: "Events can either be created by sending Siacoins between
///     addresses or they can be created by consensus (e.g. a miner payout, a siafund claim, or a contract)."
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/134a28b063df60a687899ac33aa373bf461480bc/api/server.go#L761)
/// - [Go Source for the Event Object](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/wallet/wallet.go#L14)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct AddressesEventsRequest {
    pub address: Address,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl SiaApiRequest for AddressesEventsRequest {
    type Response = Vec<Event>;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        let mut path_params = HashMap::new();
        path_params.insert("address".to_owned(), self.address.to_string());

        let mut query_params = HashMap::new();
        if let Some(limit) = self.limit {
            query_params.insert("limit".to_owned(), limit.to_string());
        }
        if let Some(offset) = self.offset {
            query_params.insert("offset".to_owned(), offset.to_string());
        }

        let query_params_option = (!query_params.is_empty()).then_some(query_params);

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_ADDRESSES_EVENTS.to_owned(), SchemaMethod::Get)
                .path_params(path_params) // Set the path params containing the address
                .query_params(query_params_option) // Set the query params for limit and offset
                .build(),
        )
    }
}

pub type AddressesEventsResponse = Vec<Event>;

/// Represents the request-response pair for getting Siacoin UTXOs owned by a specific address.
///
/// # Walletd Endpoint
/// `GET /addresses/:addr/outputs/siacoin`
///
/// # Description
/// Fetches any Siacoin unspent transaction outputs (UTXOs) owned by the specified address.
///
/// # Fields
/// - `address`: The address for which to fetch UTXOs. In Go, this corresponds to `types.Address`.
///   - [Go Source for Address Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L165)
/// - `limit`: An optional limit on the number of results. Corresponds to `int64` in Go.
/// - `offset`: An optional offset for paginated results. Corresponds to `int64` in Go.
/// - `include_mempool`: boolean, set to `true` to include UTXOs from the mempool.
///
/// # Response
/// - The response is a `GetAddressUtxosResponse` in Rust, corresponding to `SiacoinElementsResponse` in Go.
///   - [Go Source for SiacoinElementsResponse Type](https://github.com/SiaFoundation/walletd/blob/94ac6b0543c7495752554ae543d4ad28b4a620a5/api/api.go#L177C1-L182C2)
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/94ac6b0543c7495752554ae543d4ad28b4a620a5/api/server.go#L1127)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct GetAddressUtxosRequest {
    pub address: Address,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub include_mempool: bool,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
/// equivalent of SiacoinElementsResponse in Go
/// The ChainIndex is required to be provided while broadcasting any transaction that spends any of
/// these UTXOs
pub struct UtxosWithBasis {
    pub basis: ChainIndex,
    pub outputs: Vec<SiacoinElement>,
}

impl SiaApiRequest for GetAddressUtxosRequest {
    type Response = UtxosWithBasis;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        let mut path_params = HashMap::new();
        path_params.insert("address".to_owned(), self.address.to_string());

        let mut query_params = HashMap::new();

        if let Some(limit) = self.limit {
            query_params.insert("limit".to_owned(), limit.to_string());
        }
        if let Some(offset) = self.offset {
            query_params.insert("offset".to_owned(), offset.to_string());
        }

        if self.include_mempool {
            query_params.insert("tpool".to_owned(), "true".to_owned());
        }

        let query_params_option = (!query_params.is_empty()).then_some(query_params);

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_ADDRESSES_UTXOS_SIACOIN.to_owned(), SchemaMethod::Get)
                .path_params(path_params) // Set the path params containing the address
                .query_params(query_params_option) // Set the query params for limit and offset
                .build(),
        )
    }
}

/// Represents the request-response pair for broadcasting transactions.
///
/// # Walletd Endpoint
/// `POST /txpool/broadcast`
///
/// # Description
/// Used for broadcasting transactions to the network.
///
/// # Fields
/// - `transactions`: an array of V1 transactions.
/// - `v2transactions`: an array of V2 transactions.
/// - `basis`: a `ChainIndex` that represents the basis for the transactions being broadcast.
///   In most cases, this is the most current response from the `GET /consensus/tip` endpoint.
///
/// # Response
/// - The response is a `TxpoolBroadcastResponse` in Rust, corresponding to `TxpoolBroadcastResponse` in Go.
///   - [Go Source for TxpoolBroadcastResponse Type](https://github.com/SiaFoundation/walletd/blob/82c855287c510a510969ae968af21d5423b849ba/api/api.go#L46)
///
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/82c855287c510a510969ae968af21d5423b849ba/api/server.go#L401)
/// - [Go Source for the V1Transaction Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L390)
/// - [Go Source for the V2Transaction Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L649)
/// - [Go Source for the ChainIndex Type](https://github.com/SiaFoundation/core/blob/4c58987023c736df2fb19dd71c41dab16f307654/types/types.go#L194)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct TxpoolBroadcastRequest {
    pub basis: ChainIndex,
    pub transactions: Vec<V1Transaction>,
    pub v2transactions: Vec<V2Transaction>,
}

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct TxpoolBroadcastResponse {
    pub basis: ChainIndex,
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub transactions: Vec<V1Transaction>,
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub v2transactions: Vec<V2Transaction>,
}

// TODO Alright - this was initially thought neccesary to implement methods on it, but it seems ()
// will work in its place
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct EmptyResponse;

impl SiaApiRequest for TxpoolBroadcastRequest {
    type Response = TxpoolBroadcastResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        // Serialize the transactions into a JSON body
        let body = serde_json::to_value(self).map_err(|e| SiaApiRequestError::Serde {
            error: e,
            request: format!("{:?}", self),
        })?;
        let body = body.to_string();
        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_TXPOOL_BROADCAST.to_owned(), SchemaMethod::Post)
                .body(Body::Utf8(body)) // Set the JSON body for the POST request
                .build(),
        )
    }
}

/// Represents the request-response pair for fetching the current fee to broadcast a transaction.
///
/// # Walletd Endpoint
/// `GET /txpool/fee`
///
/// # Description
/// Returns the current fee to broadcast a transaction. The fee is the number of Hastings per byte.
/// To calculate how much a transaction will cost to broadcast, take its encoded size and multiply it
/// by the returned value.
///
/// Most transactions are less than 1000 bytes, so using 1000 bytes as a constant size will work for
/// most transactions.
///
/// # Response
/// - The response is a `types.Currency` from the Go codebase, represented as a `String` in Rust.
///   This value represents the number of Hastings per byte. Hastings is the smallest unit in Sia,
///   similar to Satoshis in Bitcoin.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/api/server.go#L289)
/// - [Go Source for the Currency Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/currency.go#L26)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct TxpoolFeeRequest;

#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct TxpoolFeeResponse(pub Currency);

impl SiaApiRequest for TxpoolFeeRequest {
    type Response = TxpoolFeeResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_TXPOOL_FEE.to_owned(), SchemaMethod::Get).build(), // No path_params, query_params, or body needed for this request
        )
    }
}

/// Represents the request-response pair for fetching all transactions in the transaction pool.
///
/// # Walletd Endpoint
/// `GET /txpool/transactions`
///
/// # Description
/// Returns all transactions currently in the transaction pool. This includes transactions not associated
/// with any registered wallet.
///
/// # Response
/// - This request returns `HTTP 204 NO CONTENT` in Go, which is represented by `EmptyResponse` in Rust.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/6ff23fe34f6fa45a19bfb6e4bacc8a16d2c48144/api/server.go#L282C18-L282C43)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct TxpoolTransactionsRequest;

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct TxpoolTransactionsResponse {
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub transactions: Vec<V1Transaction>,
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub v2transactions: Vec<V2Transaction>,
}

impl SiaApiRequest for TxpoolTransactionsRequest {
    type Response = TxpoolTransactionsResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_TXPOOL_TRANSACTIONS.to_owned(), SchemaMethod::Get).build(), // No path_params, query_params, or body needed for this request
        )
    }
}

/// Represents the request-response pair for mining blocks on a Sia node.
///
/// # Walletd Endpoint
/// `POST api/debug/mine`
///
/// # Description
/// Mine n blocks to a specified address. This is a debug endpoint intended to be used on CI/CD test networks only.
/// This method is only supported on walletd nodes started with `-debug` flag.
///
/// # Fields
/// - `address`: The address where blocks will be mined to. (`types.Address` type in Go)
///   - [Go Source for Address Type](https://github.com/SiaFoundation/core/blob/300042fd2129381468356dcd87c5e9a6ad94c0ef/types/types.go#L165)
/// - `blocks`: The amount of blocks to mine. (`int` type in Go)
///
/// # Response
/// - The response is `HTTP 204 NO CONTENT`, which is represented by `EmptyResponse` in Rust.
///   This indicates that the request was successful but there is no response body.
///
/// # References
/// - [Go Source for the HTTP Endpoint](https://github.com/SiaFoundation/walletd/blob/1e56661fa23bb39438ec869c91d661d51bc889a4/api/server.go#L872)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct DebugMineRequest {
    pub address: Address,
    pub blocks: i64,
}

impl SiaApiRequest for DebugMineRequest {
    type Response = EmptyResponse;

    fn is_empty_response() -> Option<Self::Response> { Some(EmptyResponse) }

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        // Serialize the request into a JSON string
        let body = serde_json::to_string(self).map_err(|e| SiaApiRequestError::Serde {
            error: e,
            request: format!("{:?}", self),
        })?;
        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_DEBUG_MINE.to_owned(), SchemaMethod::Post)
                .body(Body::Utf8(body)) // Set the JSON body for the POST request
                .build(),
        )
    }
}
/// Represents the request-response pair for finding where a given Siacoin UTXO was spent.
///
/// # Walletd Endpoint
/// `GET /outputs/siacoin/:id/spent`
///
/// # Description
/// Returns the Event in which the given Siacoin UTXO was spent if it has been spent. Walletd only
/// maintains spent UTXOs for 144 blocks( ~24 hours) after they are spent. After this time, the response
/// will be `HTTP 404 NOT FOUND`.
///
/// # Response
/// - The response is a struct consisting of a boolean `spent` field and an optional `event` field of
///  type `types.Event`. The `spent` field indicates whether the UTXO has been spent, and the `event`
///  field contains the event in which the UTXO was spent.
///
/// # References
/// - [Go Source for the HTTP Endpoint] (https://github.com/SiaFoundation/walletd/blob/5dd23bc2d8344140d52d5a99855ef46b42f9c9b6/api/server.go#L768)
///
/// This type is ported from the Go codebase, representing the equivalent request-response pair in Rust.
#[derive(Clone, Deserialize, Serialize, Debug)]
pub struct OutputsSiacoinSpentRequest {
    pub output_id: SiacoinOutputId,
}

#[derive(Clone, Deserialize, Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct OutputsSiacoinSpentResponse {
    pub spent: bool,
    #[serde(default)]
    pub event: Option<Event>,
}

impl SiaApiRequest for OutputsSiacoinSpentRequest {
    type Response = OutputsSiacoinSpentResponse;

    fn to_endpoint_schema(&self) -> Result<EndpointSchema, SiaApiRequestError> {
        let path_params: HashMap<String, String> =
            HashMap::from([("output_id".to_owned(), self.output_id.to_string())]);

        Ok(
            EndpointSchemaBuilder::new(ENDPOINT_OUTPUTS_SIACOIN_SPENT.to_owned(), SchemaMethod::Get)
                .path_params(path_params)
                .build(),
        )
    }
}
