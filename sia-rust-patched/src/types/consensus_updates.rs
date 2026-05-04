use crate::types::{Hash256, V2Transaction};
use crate::utils::deserialize_null_as_empty_vec;

use serde::{Deserialize, Serialize};

/// This module consists of types related to walletd's `api/consensus/updates/:index` endpoint.
/// Only a partial implementation is done here to facilitate `ApiClientHelpers::find_where_utxo_spent`
/// It's possible these may be extended in the future, so a dedicated module is created for this.

// NOTE: this is now unneccessary with the addition of [GET] /outputs/siacoin/:id/spent
// This is not used by KDF at all at this point, but there is no harm leaving it here for now.

/// Minimal implementation of Go type `api.ApplyUpdate`
/// As per walletd: "An ApplyUpdate is a consensus update that was applied to the best chain."
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApiApplyUpdate {
    pub update: Update,
    pub block: Block,
}

/// Minimal implementation of Go type `consensus.ApplyUpdate`
/// As per sia-core: "An ApplyUpdate represents the effects of applying a block to a state."
#[derive(Clone, Serialize, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub spent: Vec<Hash256>,
}

/// Minimal implementation of Go type `types.Block`
/// As per sia-core: "A Block is a set of transactions grouped under a header."
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub v2: V2BlockData,
}

/// Equivalent of Go type `types.V2BlockData`
/// As per sia-core: "V2BlockData contains additional fields not present in v1 blocks.""
#[derive(Clone, Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct V2BlockData {
    pub height: u64,
    pub commitment: Hash256,
    #[serde(deserialize_with = "deserialize_null_as_empty_vec")]
    pub transactions: Vec<V2Transaction>,
}
