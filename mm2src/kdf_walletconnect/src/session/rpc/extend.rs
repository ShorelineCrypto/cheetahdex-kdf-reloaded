//! `wc_sessionExtend` payloads.

use super::IrnTag;
use serde::{Deserialize, Serialize};

/// IRN relay tags for `wc_sessionExtend`.
pub const TAG: IrnTag = IrnTag { request: 1106, response: 1107 };

/// `wc_sessionExtend` request: the new absolute expiry (unix seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtendRequest {
    pub expiry: u64,
}
