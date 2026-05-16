//! Supported chains and NFT contract types.
//!
//! At the moment NFT support is restricted to a handful of EVM-compatible
//! networks (Ethereum mainnet, BNB Smart Chain, Polygon, Avalanche C-Chain
//! and Fantom). The helper trait [`ChainTicker`] decouples coin and "NFT
//! pseudo-coin" tickers from the enum itself so that other modules can map
//! between strings and the typed [`Chain`] without depending on serde.

use crate::nft::errors::{ParseChainError, ParseContractTypeError};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::fmt;
use std::str::FromStr;

/// EVM-compatible blockchains for which the NFT module can fetch and store
/// ownership information.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Chain {
    /// Avalanche C-Chain (network ticker `AVAX`).
    Avalanche,
    /// BNB Smart Chain (network ticker `BNB`).
    Bsc,
    /// Ethereum mainnet.
    Eth,
    /// Fantom Opera (network ticker `FTM`).
    Fantom,
    /// Polygon PoS (network ticker `MATIC`).
    Polygon,
}

impl Chain {
    /// All variants of [`Chain`] in declaration order.
    pub const ALL: [Chain; 5] = [Chain::Avalanche, Chain::Bsc, Chain::Eth, Chain::Fantom, Chain::Polygon];
}

/// Conversions between a [`Chain`] and the various string identifiers used
/// by the dispatcher and storage layers (coin ticker and NFT-prefixed
/// pseudo-coin ticker).
pub trait ChainTicker {
    /// The native coin ticker advertised in `coins` files (e.g. `ETH`).
    fn coin_ticker(self) -> &'static str;
    /// Pseudo-coin ticker used to namespace per-chain NFT records
    /// (e.g. `NFT_ETH`).
    fn nft_ticker(self) -> &'static str;
    /// Parse a coin ticker string back into a [`Chain`].
    fn from_coin_ticker(text: &str) -> Result<Chain, ParseChainError>;
    /// Parse a NFT pseudo-coin ticker back into a [`Chain`].
    fn from_nft_ticker(text: &str) -> Result<Chain, ParseChainError>;
}

impl ChainTicker for Chain {
    fn coin_ticker(self) -> &'static str {
        match self {
            Chain::Avalanche => "AVAX",
            Chain::Bsc => "BNB",
            Chain::Eth => "ETH",
            Chain::Fantom => "FTM",
            Chain::Polygon => "MATIC",
        }
    }

    fn nft_ticker(self) -> &'static str {
        match self {
            Chain::Avalanche => "NFT_AVAX",
            Chain::Bsc => "NFT_BNB",
            Chain::Eth => "NFT_ETH",
            Chain::Fantom => "NFT_FTM",
            Chain::Polygon => "NFT_MATIC",
        }
    }

    fn from_coin_ticker(text: &str) -> Result<Chain, ParseChainError> {
        match text.to_ascii_uppercase().as_str() {
            "AVAX" => Ok(Chain::Avalanche),
            "BNB" => Ok(Chain::Bsc),
            "ETH" => Ok(Chain::Eth),
            "FTM" => Ok(Chain::Fantom),
            "MATIC" => Ok(Chain::Polygon),
            _ => Err(ParseChainError::Unsupported),
        }
    }

    fn from_nft_ticker(text: &str) -> Result<Chain, ParseChainError> {
        match text.to_ascii_uppercase().as_str() {
            "NFT_AVAX" => Ok(Chain::Avalanche),
            "NFT_BNB" => Ok(Chain::Bsc),
            "NFT_ETH" => Ok(Chain::Eth),
            "NFT_FTM" => Ok(Chain::Fantom),
            "NFT_MATIC" => Ok(Chain::Polygon),
            _ => Err(ParseChainError::Unsupported),
        }
    }
}

impl fmt::Display for Chain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Chain::Avalanche => "AVALANCHE",
            Chain::Bsc => "BSC",
            Chain::Eth => "ETH",
            Chain::Fantom => "FANTOM",
            Chain::Polygon => "POLYGON",
        };
        f.write_str(label)
    }
}

impl FromStr for Chain {
    type Err = ParseChainError;

    fn from_str(text: &str) -> Result<Chain, ParseChainError> {
        match text.to_ascii_uppercase().as_str() {
            "AVALANCHE" => Ok(Chain::Avalanche),
            "BSC" => Ok(Chain::Bsc),
            "ETH" => Ok(Chain::Eth),
            "FANTOM" => Ok(Chain::Fantom),
            "POLYGON" => Ok(Chain::Polygon),
            _ => Err(ParseChainError::Unsupported),
        }
    }
}

impl<'de> Deserialize<'de> for Chain {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// Family of smart contract that backs a token entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ContractType {
    /// ERC-1155 multi-token contract; quantities can exceed one.
    Erc1155,
    /// ERC-721 single-token contract; each `token_id` is unique.
    Erc721,
}

impl fmt::Display for ContractType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ContractType::Erc1155 => "ERC1155",
            ContractType::Erc721 => "ERC721",
        })
    }
}

impl FromStr for ContractType {
    type Err = ParseContractTypeError;

    fn from_str(text: &str) -> Result<ContractType, ParseContractTypeError> {
        match text.to_ascii_uppercase().as_str() {
            "ERC1155" => Ok(ContractType::Erc1155),
            "ERC721" => Ok(ContractType::Erc721),
            _ => Err(ParseContractTypeError::Unsupported),
        }
    }
}

impl<'de> Deserialize<'de> for ContractType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coin_ticker_round_trips() {
        for chain in Chain::ALL {
            let ticker = chain.coin_ticker();
            assert_eq!(Chain::from_coin_ticker(ticker).unwrap(), chain);
            assert_eq!(Chain::from_coin_ticker(&ticker.to_ascii_lowercase()).unwrap(), chain);
        }
    }

    #[test]
    fn nft_ticker_round_trips() {
        for chain in Chain::ALL {
            let ticker = chain.nft_ticker();
            assert!(ticker.starts_with("NFT_"));
            assert_eq!(Chain::from_nft_ticker(ticker).unwrap(), chain);
        }
    }

    #[test]
    fn chain_from_str_is_case_insensitive() {
        assert_eq!("eth".parse::<Chain>().unwrap(), Chain::Eth);
        assert_eq!("Polygon".parse::<Chain>().unwrap(), Chain::Polygon);
    }

    #[test]
    fn chain_unknown_value_is_rejected() {
        assert!("MARS".parse::<Chain>().is_err());
        assert!(Chain::from_coin_ticker("XYZ").is_err());
        assert!(Chain::from_nft_ticker("BTC").is_err());
    }

    #[test]
    fn chain_serializes_uppercase() {
        let json = serde_json::to_string(&Chain::Bsc).unwrap();
        assert_eq!(json, "\"BSC\"");
    }

    #[test]
    fn chain_deserializes_from_lowercase() {
        let chain: Chain = serde_json::from_str("\"polygon\"").unwrap();
        assert_eq!(chain, Chain::Polygon);
    }

    #[test]
    fn contract_type_serde_round_trip() {
        let json = serde_json::to_string(&ContractType::Erc721).unwrap();
        assert_eq!(json, "\"ERC721\"");
        let back: ContractType = serde_json::from_str("\"erc1155\"").unwrap();
        assert_eq!(back, ContractType::Erc1155);
    }

    #[test]
    fn contract_type_unknown_value_is_rejected() {
        assert!("ERC20".parse::<ContractType>().is_err());
        assert!(serde_json::from_str::<ContractType>("\"ERC998\"").is_err());
    }

    #[test]
    fn display_matches_uppercase_form() {
        assert_eq!(Chain::Eth.to_string(), "ETH");
        assert_eq!(Chain::Fantom.to_string(), "FANTOM");
        assert_eq!(ContractType::Erc1155.to_string(), "ERC1155");
    }
}
