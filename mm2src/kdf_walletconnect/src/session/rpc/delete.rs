//! `wc_sessionDelete` payloads.

use super::IrnTag;
use serde::{Deserialize, Serialize};

/// IRN relay tags for `wc_sessionDelete`.
pub const TAG: IrnTag = IrnTag { request: 1112, response: 1113 };

/// JSON-RPC `method` name for a session-delete request.
pub const METHOD: &str = "wc_sessionDelete";

/// `wc_sessionDelete` request: a reason code and message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteRequest {
    pub code: u32,
    pub message: String,
}

impl Default for DeleteRequest {
    fn default() -> Self {
        // 6000 = USER_DISCONNECTED in the WalletConnect reason registry.
        DeleteRequest { code: 6000, message: "User disconnected".to_string() }
    }
}
