//! `wc_sessionEvent` payloads.

use super::IrnTag;
use serde::{Deserialize, Serialize};

/// IRN relay tags for `wc_sessionEvent`.
pub const TAG: IrnTag = IrnTag { request: 1110, response: 1111 };

/// The inner event description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    pub data: serde_json::Value,
}

/// `wc_sessionEvent` request: an event bound to a CAIP-2 chain id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventRequest {
    pub event: Event,
    pub chain_id: String,
}
