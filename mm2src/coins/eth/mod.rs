/******************************************************************************
 * Copyright © 2014-2019 The SuperNET Developers.                             *
 *                                                                            *
 * See the AUTHORS, DEVELOPER-AGREEMENT and LICENSE files at                  *
 * the top-level directory of this distribution for the individual copyright  *
 * holder information and the developer policies on copyright and licensing.  *
 *                                                                            *
 * Unless otherwise agreed in a custom licensing agreement, no part of the    *
 * SuperNET software, including this file may be copied, modified, propagated *
 * or distributed except according to the terms contained in the LICENSE file *
 *                                                                            *
 * Removal or modification of this copyright notice is prohibited.            *
 *                                                                            *
 ******************************************************************************/
//
//  eth — Ethereum and ERC-20 coin support
//
//  Split into sub-modules for maintainability:
//    eth_types      – constants, ABIs, structs, enums, error conversions
//    eth_impl       – EthCoinImpl methods, transaction helpers, utility fns
//    eth_swap_ops   – SwapOps + WatcherOps trait impls
//    eth_market_ops – MarketCoinOps trait impl
//    eth_mm_coin    – MmCoin, ParseCoinAssocTypes, V2 swap trait impls
//

// ─── Imports (pub(crate) so child modules inherit via `use super::*`) ───────

pub(crate) use async_trait::async_trait;
pub(crate) use bigdecimal::BigDecimal;
pub(crate) use bitcrypto::{keccak256, sha256};
pub(crate) use common::custom_futures::TimedAsyncMutex;
pub(crate) use common::executor::Timer;
pub(crate) use common::log::error;
pub(crate) use common::{now_ms, small_rng, DEX_FEE_ADDR_RAW_PUBKEY};
pub(crate) use derive_more::Display;
pub(crate) use ethabi::{Contract, Token};
pub(crate) use ethcore_transaction::{Action, Transaction as UnSignedEthTx, UnverifiedTransaction};
pub(crate) use ethereum_types::{Address, H160, H256, U256};
pub(crate) use ethkey::{public_to_address, KeyPair, Public, Signature};
pub(crate) use futures::compat::Future01CompatExt;
pub(crate) use futures::future::{join_all, select, Either, FutureExt, TryFutureExt};
pub(crate) use futures01::Future;
pub(crate) use http::StatusCode;
pub(crate) use mm2_core::mm_ctx::{MmArc, MmWeak};
pub(crate) use mm2_err_handle::prelude::*;
pub(crate) use mm2_net::transport::{slurp_url, SlurpError};
#[cfg(test)]
pub(crate) use mocktopus::macros::*;
pub(crate) use rand::seq::SliceRandom;
pub(crate) use rpc::v1::types::Bytes as BytesJson;
pub(crate) use secp256k1::PublicKey;
pub(crate) use serde_json::{self as json, Value as Json};
pub(crate) use sha3::{Digest, Keccak256};
pub(crate) use std::cmp::Ordering;
pub(crate) use std::collections::HashMap;
pub(crate) use std::fmt;
pub(crate) use std::ops::Deref;
pub(crate) use std::path::PathBuf;
pub(crate) use std::str::FromStr;
pub(crate) use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
pub(crate) use std::sync::{Arc, Mutex};
pub(crate) use web3::types::{
    Action as TraceAction, BlockId, BlockNumber, Bytes, CallRequest, FilterBuilder, Log, Trace, TraceFilterBuilder,
    Transaction as Web3Transaction, TransactionId,
};
pub(crate) use web3::{self, Web3};

pub(crate) use super::{
    BalanceError, BalanceFut, CoinBalance, CoinProtocol, CoinTransportMetrics, CoinsContext, FeeApproxStage,
    FoundSwapTxSpend, HistorySyncState, MarketCoinOps, MmCoin, NegotiateSwapContractAddrErr, NumConversError,
    NumConversResult, RawTransactionError, RawTransactionFut, RawTransactionRequest, RawTransactionRes,
    RawTransactionResult, RpcClientType, RpcTransportEventHandler, RpcTransportEventHandlerShared,
    SignEthTransactionParams, SignRawTransactionEnum, SignRawTransactionRequest, SignatureError, SignatureResult,
    SwapOps, TradeFee, TradePreimageError, TradePreimageFut, TradePreimageResult, TradePreimageValue, Transaction,
    TransactionDetails, TransactionEnum, UnexpectedDerivationMethod, ValidateAddressResult, VerificationError,
    VerificationResult, WithdrawError, WithdrawFee, WithdrawFut, WithdrawRequest, WithdrawResult,
};

pub use ethcore_transaction::SignedTransaction as SignedEthTx;
pub use rlp;

// ─── Sub-modules ────────────────────────────────────────────────────────────

pub mod eth_hd_wallet;
pub(crate) mod eth_swap_v2;
pub mod fee_estimation;
pub mod tron;
mod web3_transport;

mod eth_impl;
mod eth_market_ops;
mod eth_mm_coin;
mod eth_swap_ops;
mod eth_types;

// Re-export split module contents for backward-compatible access paths
pub use eth_impl::*;
pub use eth_mm_coin::EthTxFeeDetails;
pub use eth_types::*;

pub(crate) use crate::DerivationMethod;
pub(crate) use crate::{
    CommonSwapOpsV2, DexFee, FindPaymentSpendError, FundingTxSpend, GenPreimageResult, GenTakerFundingSpendArgs,
    GenTakerPaymentSpendArgs, MakerCoinSwapOpsV2, ParseCoinAssocTypes, RefundFundingSecretArgs,
    RefundMakerPaymentSecretArgs, RefundMakerPaymentTimelockArgs, RefundTakerPaymentArgs, SearchForFundingSpendErr,
    SendMakerPaymentArgs, SendTakerFundingArgs, SpendMakerPaymentArgs, SwapTxTypeWithSecretHash, TakerCoinSwapOpsV2,
    ToBytes, TransactionErr, TransactionFut, TxGenError, TxPreimageWithSig, ValidateFeeArgs, ValidateMakerPaymentArgs,
    ValidatePaymentInput, ValidateSwapV2TxError, ValidateSwapV2TxResult, ValidateTakerFundingArgs,
    ValidateTakerFundingSpendPreimageResult, ValidateTakerPaymentSpendPreimageResult, WatcherOps,
};
pub(crate) use common::mm_number::MmNumber;
pub(crate) use eth_hd_wallet::EthHDWallet;
pub(crate) use ethkey::{sign, verify_address};
pub(crate) use serialization::{CompactInteger, Serializable, Stream};
pub(crate) use web3_transport::{EthFeeHistoryNamespace, FeeHistoryResult, Web3Transport};

#[cfg(test)]
mod eth_tests;
#[cfg(target_arch = "wasm32")]
mod eth_wasm_tests;

// ─── EthCoin newtype ────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct EthCoin(Arc<EthCoinImpl>);
impl Deref for EthCoin {
    type Target = EthCoinImpl;
    fn deref(&self) -> &EthCoinImpl {
        &*self.0
    }
}
