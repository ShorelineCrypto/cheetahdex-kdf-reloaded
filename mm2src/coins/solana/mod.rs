// solana module — Solana blockchain support.
//
// Split into sub-modules for maintainability:
//   solana_types      – constants, structs, enums, error types, core types
//   solana_helpers    – SolanaCommonOps impl, inherent methods
//   solana_swap_ops   – SwapOps and WatcherOps trait implementations
//   solana_market_ops – MarketCoinOps trait implementation
//   solana_mm_coin    – MmCoin trait implementation and withdraw logic
//

// ─── Imports (pub(crate) so child modules inherit via `use super::*`) ───────

pub(crate) use super::{
    CoinBalance, HistorySyncState, MarketCoinOps, MmCoin, SwapOps, TradeFee, TransactionEnum, WatcherOps,
};
pub(crate) use crate::solana::solana_common::{lamports_to_sol, PrepareTransferData, SufficientBalanceError};
pub(crate) use crate::solana::spl::SplTokenInfo;
pub(crate) use crate::{
    BalanceError, BalanceFut, DexFee, FeeApproxStage, FoundSwapTxSpend, NegotiateSwapContractAddrErr,
    RawTransactionFut, RawTransactionRequest, SignatureResult, TradePreimageFut, TradePreimageResult,
    TradePreimageValue, TransactionDetails, TransactionFut, TransactionType, UnexpectedDerivationMethod,
    ValidateAddressResult, ValidateFeeArgs, ValidatePaymentInput, VerificationResult, WithdrawError, WithdrawFut,
    WithdrawRequest, WithdrawResult,
};
pub(crate) use async_trait::async_trait;
pub(crate) use base58::ToBase58;
pub(crate) use bigdecimal::BigDecimal;
pub(crate) use bincode::{deserialize, serialize};
pub(crate) use common::{async_blocking, mm_number::MmNumber, now_ms};
pub(crate) use derive_more::Display;
pub(crate) use futures::{FutureExt, TryFutureExt};
pub(crate) use futures01::Future;
pub(crate) use keys::KeyPair;
pub(crate) use mm2_core::mm_ctx::MmArc;
pub(crate) use mm2_err_handle::prelude::*;
pub(crate) use rpc::v1::types::Bytes as BytesJson;
pub(crate) use serde_json::{self as json, Value as Json};
pub(crate) use solana_client::rpc_request::TokenAccountsFilter;
pub(crate) use solana_client::{
    client_error::{ClientError, ClientErrorKind},
    rpc_client::RpcClient,
};
pub(crate) use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
pub(crate) use solana_sdk::program_error::ProgramError;
pub(crate) use solana_sdk::pubkey::ParsePubkeyError;
pub(crate) use solana_sdk::transaction::Transaction;
pub(crate) use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
pub(crate) use std::collections::HashMap;
pub(crate) use std::str::FromStr;
pub(crate) use std::sync::Mutex;
pub(crate) use std::{
    convert::TryFrom,
    fmt::{Debug, Formatter, Result as FmtResult},
    ops::Deref,
    sync::Arc,
};

// ─── Existing sub-modules ───────────────────────────────────────────────────

pub mod solana_common;
#[cfg(test)]
mod solana_common_tests;
mod solana_decode_tx_helpers;
#[cfg(test)]
mod solana_tests;
pub mod spl;
#[cfg(test)]
mod spl_tests;

// ─── Split sub-modules ─────────────────────────────────────────────────────

mod solana_helpers;
mod solana_market_ops;
mod solana_mm_coin;
mod solana_swap_ops;
mod solana_types;

// Re-export split module contents for backward-compatible access paths
pub use solana_types::*;
