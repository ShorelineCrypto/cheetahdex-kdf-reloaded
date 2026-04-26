pub mod block_native;
pub mod infura;
pub mod simple;

use ethereum_types::U256;
use serde::Deserialize;

pub(crate) const FEE_PRIORITY_LEVEL_N: usize = 3;

/// Indicates which provider was used to get fee per gas estimations
#[derive(Clone, Debug, Default)]
pub enum EstimationSource {
    #[default]
    Empty,
    Simple,
    Infura,
    Blocknative,
}

impl std::fmt::Display for EstimationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EstimationSource::Empty => write!(f, "empty"),
            EstimationSource::Simple => write!(f, "simple"),
            EstimationSource::Infura => write!(f, "infura"),
            EstimationSource::Blocknative => write!(f, "blocknative"),
        }
    }
}

pub(crate) enum PriorityLevelId {
    Low = 0,
    Medium = 1,
    High = 2,
}

/// Supported gas api providers
#[derive(Clone, Deserialize)]
pub enum GasApiProvider {
    Infura,
    Blocknative,
}

/// Gas api provider configuration
#[derive(Clone, Deserialize)]
pub struct GasApiConfig {
    pub provider: GasApiProvider,
    /// Gas api provider or proxy base URL
    pub url: String,
}

/// Priority level estimated max fee per gas (in wei)
#[derive(Clone, Debug, Default)]
pub struct FeePerGasLevel {
    /// Estimated max priority tip fee per gas in wei
    pub max_priority_fee_per_gas: U256,
    /// Estimated max fee per gas in wei
    pub max_fee_per_gas: U256,
    /// Estimated transaction min wait time in mempool in ms
    pub min_wait_time: Option<u32>,
    /// Estimated transaction max wait time in mempool in ms
    pub max_wait_time: Option<u32>,
}

/// Estimated fee per gas for low/medium/high priority levels (in wei)
#[derive(Default, Debug, Clone)]
pub struct FeePerGasEstimated {
    /// Base fee for the next block in wei
    pub base_fee: U256,
    pub low: FeePerGasLevel,
    pub medium: FeePerGasLevel,
    pub high: FeePerGasLevel,
    pub source: EstimationSource,
    pub base_fee_trend: String,
    pub priority_fee_trend: String,
}
