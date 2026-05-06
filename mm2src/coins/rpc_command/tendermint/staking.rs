//! Tendermint staking RPC types and the `validators_rpc` handler.

use bigdecimal::BigDecimal;
use common::PagingOptions;
use cosmrs::staking::{Commission, Description, Validator};
use mm2_err_handle::prelude::*;

use crate::tendermint::TendermintCoinRpcError;
use crate::{MmCoinEnum, StakingInfosError, WithdrawFee};

// ---------------------------------------------------------------------------
// Validator query
// ---------------------------------------------------------------------------

/// Filter validators by bonding status.
#[derive(Debug, Default, Deserialize)]
pub(crate) enum ValidatorStatus {
    All,
    #[default]
    Bonded,
    Unbonded,
}

impl std::fmt::Display for ValidatorStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidatorStatus::All => Ok(()),
            ValidatorStatus::Bonded => f.write_str("BOND_STATUS_BONDED"),
            ValidatorStatus::Unbonded => f.write_str("BOND_STATUS_UNBONDED"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ValidatorsQuery {
    #[serde(flatten)]
    paging: PagingOptions,
    #[serde(default)]
    filter_by_status: ValidatorStatus,
}

#[derive(Clone, Serialize)]
pub struct ValidatorsQueryResponse {
    validators: Vec<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Delegation / Undelegation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct DelegationPayload {
    pub validator_address: String,
    pub fee: Option<WithdrawFee>,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub amount: BigDecimal,
    #[serde(default)]
    pub max: bool,
}

#[derive(Debug, Deserialize)]
pub struct ClaimRewardsPayload {
    pub validator_address: String,
    pub fee: Option<WithdrawFee>,
    #[serde(default)]
    pub memo: String,
    /// When `true`, claim even if the fee exceeds the reward.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
pub struct SimpleListQuery {
    #[serde(flatten)]
    pub(crate) paging: PagingOptions,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Serialize)]
pub struct DelegationsQueryResponse {
    pub(crate) delegations: Vec<Delegation>,
}

#[derive(Debug, PartialEq, Serialize)]
pub(crate) struct Delegation {
    pub(crate) validator_address: String,
    pub(crate) delegated_amount: BigDecimal,
    pub(crate) reward_amount: BigDecimal,
}

#[derive(Serialize)]
pub struct UndelegationsQueryResponse {
    pub(crate) ongoing_undelegations: Vec<Undelegation>,
}

#[derive(Serialize)]
pub(crate) struct Undelegation {
    pub(crate) validator_address: String,
    pub(crate) entries: Vec<UndelegationEntry>,
}

#[derive(Serialize)]
pub(crate) struct UndelegationEntry {
    pub(crate) creation_height: i64,
    pub(crate) completion_datetime: String,
    pub(crate) balance: BigDecimal,
}

// ---------------------------------------------------------------------------
// Error conversion
// ---------------------------------------------------------------------------

impl From<TendermintCoinRpcError> for StakingInfosError {
    fn from(e: TendermintCoinRpcError) -> Self {
        match e {
            TendermintCoinRpcError::InvalidResponse(msg) | TendermintCoinRpcError::RpcClientError(msg) => {
                StakingInfosError::Transport(msg)
            },
            TendermintCoinRpcError::Prost(msg) | TendermintCoinRpcError::InternalError(msg) => {
                StakingInfosError::Internal(msg)
            },
            TendermintCoinRpcError::UnexpectedAccountType { prefix } => {
                StakingInfosError::Internal(format!("unexpected account type: {prefix}"))
            },
            TendermintCoinRpcError::PerformanceFeeIsTooLow => {
                StakingInfosError::Internal("performance fee too low".into())
            },
        }
    }
}

// ---------------------------------------------------------------------------
// validators_rpc handler
// ---------------------------------------------------------------------------

pub async fn validators_rpc(
    coin: MmCoinEnum,
    req: ValidatorsQuery,
) -> Result<ValidatorsQueryResponse, MmError<StakingInfosError>> {
    fn maybe_jsonize_description(desc: Option<Description>) -> Option<serde_json::Value> {
        desc.map(|d| {
            serde_json::json!({
                "moniker": d.moniker,
                "identity": d.identity,
                "website": d.website,
                "security_contact": d.security_contact,
                "details": d.details,
            })
        })
    }

    fn maybe_jsonize_commission(comm: Option<Commission>) -> Option<serde_json::Value> {
        comm.map(|c| {
            let rates = c.commission_rates.map(|cr| {
                serde_json::json!({
                    "rate": cr.rate,
                    "max_rate": cr.max_rate,
                    "max_change_rate": cr.max_change_rate
                })
            });
            serde_json::json!({
                "commission_rates": rates,
                "update_time": c.update_time
            })
        })
    }

    fn jsonize_validator(v: Validator) -> serde_json::Value {
        serde_json::json!({
            "operator_address": v.operator_address,
            "consensus_pubkey": v.consensus_pubkey,
            "jailed": v.jailed,
            "status": v.status,
            "tokens": v.tokens,
            "delegator_shares": v.delegator_shares,
            "description": maybe_jsonize_description(v.description),
            "unbonding_height": v.unbonding_height,
            "unbonding_time": v.unbonding_time,
            "commission": maybe_jsonize_commission(v.commission),
            "min_self_delegation": v.min_self_delegation,
        })
    }

    let validators = match coin {
        MmCoinEnum::TendermintCoin(coin) => coin
            .validators_list(req.filter_by_status, req.paging)
            .await
            .map_mm_err()?,
        MmCoinEnum::TendermintToken(token) => token
            .platform_coin
            .validators_list(req.filter_by_status, req.paging)
            .await
            .map_mm_err()?,
        other => {
            return MmError::err(StakingInfosError::InvalidPayload {
                reason: format!("{} is not a Cosmos coin", other.ticker()),
            })
        },
    };

    Ok(ValidatorsQueryResponse {
        validators: validators.into_iter().map(jsonize_validator).collect(),
    })
}
