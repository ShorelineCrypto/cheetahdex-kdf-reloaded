//! `wc_sessionUpdate` payloads.

use super::IrnTag;
use super::settle::SettleNamespace;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// IRN relay tags for `wc_sessionUpdate`.
pub const TAG: IrnTag = IrnTag { request: 1104, response: 1105 };

/// `wc_sessionUpdate` request: a fresh namespace set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRequest {
    pub namespaces: BTreeMap<String, SettleNamespace>,
}
