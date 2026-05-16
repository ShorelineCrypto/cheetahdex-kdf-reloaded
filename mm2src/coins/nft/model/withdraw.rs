//! NFT withdraw request payloads.
//!
//! The dispatcher accepts a tagged union (`type` + `withdraw_data`) so that
//! ERC-721 and ERC-1155 withdrawals can share the same RPC endpoint while
//! still carrying contract-specific fields.

use crate::nft::model::chain::Chain;
use crate::nft::serde_helpers::{optional_token_amount, token_id_from_string};
use crate::WithdrawFee;
use mm2_number::BigUint;
use serde::Deserialize;

/// Parameters required to withdraw an ERC-1155 token.
#[derive(Debug, Deserialize)]
pub struct WithdrawErc1155 {
    /// Chain on which the token contract lives.
    pub chain: Chain,
    /// Recipient address as a string (kept generic to accept both 0x and
    /// chain-native formats).
    pub to: String,
    /// Address of the ERC-1155 contract.
    pub token_address: String,
    /// Identifier of the token within the contract.
    #[serde(deserialize_with = "token_id_from_string")]
    pub token_id: BigUint,
    /// Optional quantity to transfer; defaults to one when omitted.
    #[serde(default, deserialize_with = "optional_token_amount")]
    pub amount: Option<BigUint>,
    /// When `true`, transfer the full balance; overrides `amount`.
    #[serde(default)]
    pub max: bool,
    /// Optional withdrawal fee override.
    pub fee: Option<WithdrawFee>,
}

/// Parameters required to withdraw an ERC-721 token.
#[derive(Debug, Deserialize)]
pub struct WithdrawErc721 {
    /// Chain on which the token contract lives.
    pub chain: Chain,
    /// Recipient address as a string.
    pub to: String,
    /// Address of the ERC-721 contract.
    pub token_address: String,
    /// Identifier of the token within the contract.
    #[serde(deserialize_with = "token_id_from_string")]
    pub token_id: BigUint,
    /// Optional withdrawal fee override.
    pub fee: Option<WithdrawFee>,
}

/// Tagged union of NFT withdraw requests.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "withdraw_data", rename_all = "snake_case")]
pub enum WithdrawNftReq {
    /// Withdraw an ERC-1155 multi-token transfer.
    WithdrawErc1155(WithdrawErc1155),
    /// Withdraw an ERC-721 single-token transfer.
    WithdrawErc721(WithdrawErc721),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deserialize_erc721_request() {
        let req: WithdrawNftReq = serde_json::from_value(json!({
            "type": "withdraw_erc721",
            "withdraw_data": {
                "chain": "ETH",
                "to": "0xabc",
                "token_address": "0xdef",
                "token_id": "1"
            }
        }))
        .unwrap();
        match req {
            WithdrawNftReq::WithdrawErc721(inner) => {
                assert_eq!(inner.chain, Chain::Eth);
                assert_eq!(inner.token_id, BigUint::from(1u32));
            },
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn deserialize_erc1155_request_with_optional_amount() {
        let req: WithdrawNftReq = serde_json::from_value(json!({
            "type": "withdraw_erc1155",
            "withdraw_data": {
                "chain": "POLYGON",
                "to": "0xabc",
                "token_address": "0xdef",
                "token_id": "5",
                "amount": "10"
            }
        }))
        .unwrap();
        match req {
            WithdrawNftReq::WithdrawErc1155(inner) => {
                assert_eq!(inner.amount, Some(BigUint::from(10u32)));
                assert!(!inner.max);
            },
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn erc1155_request_without_amount_defaults_to_none() {
        let req: WithdrawNftReq = serde_json::from_value(json!({
            "type": "withdraw_erc1155",
            "withdraw_data": {
                "chain": "BSC",
                "to": "0xabc",
                "token_address": "0xdef",
                "token_id": "1"
            }
        }))
        .unwrap();
        match req {
            WithdrawNftReq::WithdrawErc1155(inner) => assert_eq!(inner.amount, None),
            other => panic!("unexpected variant: {other:?}"),
        }
    }

    #[test]
    fn unknown_variant_is_rejected() {
        let outcome: Result<WithdrawNftReq, _> = serde_json::from_value(json!({
            "type": "withdraw_unknown",
            "withdraw_data": {"chain": "ETH", "to": "x", "token_address": "y", "token_id": "1"}
        }));
        assert!(outcome.is_err());
    }
}
