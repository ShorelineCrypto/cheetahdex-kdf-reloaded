//! NFT maker swap V2 driver (P10.3.7.d).
//!
//! Thin state-machine wiring on top of [`coins::eth::EthCoin`]'s
//! NFT-aware build helpers (P10.3.7.c). This module is the bridge
//! between an order's negotiated [`SwapVersion`] and the NFT HTLC
//! call execution: it decides whether a pair runs the NFT V2
//! protocol, and broadcasts the resulting [`NftCall`] via the EVM
//! `sign_and_send_transaction` path.
//!
//! Full state-machine integration (event log, persistence, P2P
//! coordination) layers on top of these driver entrypoints in a
//! later slice. This module's job is to make the dispatch
//! decision and the on-chain action testable in isolation.

use crate::mm2::lp_swap::swap_versioning::{SwapVersion, NFT_SWAP_V2_VERSION};
use coins::eth::nft_swap_v2::{EthCoinNftError, NftMakerPaymentArgs, NftRefundSecretArgs, NftRefundTimelockArgs,
                              NftSpendMakerPaymentArgs};
use coins::eth::{EthCoin, SignedEthTx};
use futures::compat::Future01CompatExt;

/// Outcome of a `should_use_nft_swap_v2` check.
///
/// Carries enough information for callers to log the negotiation
/// decision and surface meaningful diagnostics on rejection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NftSwapV2NegotiationOutcome {
    /// Both sides advertised NFT V2 and the maker coin has an NFT
    /// HTLC contract configured. Run the NFT V2 path.
    Use,
    /// One or both sides did not advertise the NFT V2 protocol
    /// version. Fall back to the negotiated baseline (TPU or legacy).
    VersionMismatch { maker: SwapVersion, taker: SwapVersion },
    /// Both sides advertise NFT V2 but the maker coin has no NFT
    /// HTLC contract configured for this chain. Hard-fail the swap
    /// rather than silently running a non-NFT path.
    NoNftContract,
}

/// Decide whether a maker/taker pair should execute the NFT swap V2
/// protocol. Returns [`NftSwapV2NegotiationOutcome::Use`] only when
/// **both sides** advertise [`NFT_SWAP_V2_VERSION`] AND the maker
/// coin reports a configured NFT HTLC contract.
///
/// Pure function — does not touch network state.
pub fn should_use_nft_swap_v2(
    maker_version: SwapVersion,
    taker_version: SwapVersion,
    maker_has_nft_contract: bool,
) -> NftSwapV2NegotiationOutcome {
    if !maker_version.is_nft_v2() || !taker_version.is_nft_v2() {
        return NftSwapV2NegotiationOutcome::VersionMismatch {
            maker: maker_version,
            taker: taker_version,
        };
    }
    if !maker_has_nft_contract {
        return NftSwapV2NegotiationOutcome::NoNftContract;
    }
    NftSwapV2NegotiationOutcome::Use
}

/// Errors raised by the broadcasting drivers.
#[derive(Debug)]
pub enum NftSwapV2DriverError {
    /// The build phase rejected the inputs (e.g. ERC-1155 without amount).
    Build(EthCoinNftError),
    /// Broadcasting the signed transaction failed.
    Broadcast(String),
}

impl std::fmt::Display for NftSwapV2DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NftSwapV2DriverError::Build(e) => write!(f, "NFT swap V2 build error: {e}"),
            NftSwapV2DriverError::Broadcast(e) => write!(f, "NFT swap V2 broadcast error: {e}"),
        }
    }
}

impl std::error::Error for NftSwapV2DriverError {}

impl From<EthCoinNftError> for NftSwapV2DriverError {
    fn from(e: EthCoinNftError) -> Self { NftSwapV2DriverError::Build(e) }
}

async fn broadcast(
    coin: &EthCoin,
    call: coins::eth::nft_swap_v2::NftCall,
) -> Result<SignedEthTx, NftSwapV2DriverError> {
    coin.send_nft_call(call)
        .compat()
        .await
        .map_err(|e| NftSwapV2DriverError::Broadcast(format!("{e:?}")))
}

/// Build and broadcast an `erc{721,1155}MakerPayment` HTLC.
pub async fn dispatch_nft_maker_payment(
    coin: &EthCoin,
    args: &NftMakerPaymentArgs,
) -> Result<SignedEthTx, NftSwapV2DriverError> {
    let call = coin.build_send_nft_maker_payment(args)?;
    broadcast(coin, call).await
}

/// Build and broadcast a `spendErc{721,1155}MakerPayment` (taker reveals
/// `maker_secret` to claim the NFT).
pub async fn dispatch_nft_spend_maker_payment(
    coin: &EthCoin,
    args: &NftSpendMakerPaymentArgs,
) -> Result<SignedEthTx, NftSwapV2DriverError> {
    let call = coin.build_spend_nft_maker_payment(args)?;
    broadcast(coin, call).await
}

/// Build and broadcast a maker timelock refund of an NFT HTLC.
pub async fn dispatch_nft_refund_maker_payment_timelock(
    coin: &EthCoin,
    args: &NftRefundTimelockArgs,
) -> Result<SignedEthTx, NftSwapV2DriverError> {
    let call = coin.build_refund_nft_maker_payment_timelock(args)?;
    broadcast(coin, call).await
}

/// Build and broadcast a cooperative-secret refund of an NFT HTLC.
pub async fn dispatch_nft_refund_maker_payment_secret(
    coin: &EthCoin,
    args: &NftRefundSecretArgs,
) -> Result<SignedEthTx, NftSwapV2DriverError> {
    let call = coin.build_refund_nft_maker_payment_secret(args)?;
    broadcast(coin, call).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mm2::lp_swap::swap_versioning::{LEGACY_SWAP_VERSION, TPU_SWAP_VERSION};

    fn v(n: u8) -> SwapVersion { SwapVersion { version: n } }

    #[test]
    fn should_use_when_both_sides_advertise_nft_v2_and_contract_present() {
        assert_eq!(
            should_use_nft_swap_v2(v(NFT_SWAP_V2_VERSION), v(NFT_SWAP_V2_VERSION), true),
            NftSwapV2NegotiationOutcome::Use
        );
    }

    #[test]
    fn rejects_when_maker_only_advertises_tpu() {
        let out = should_use_nft_swap_v2(v(TPU_SWAP_VERSION), v(NFT_SWAP_V2_VERSION), true);
        assert!(matches!(out, NftSwapV2NegotiationOutcome::VersionMismatch { .. }));
    }

    #[test]
    fn rejects_when_taker_only_advertises_tpu() {
        let out = should_use_nft_swap_v2(v(NFT_SWAP_V2_VERSION), v(TPU_SWAP_VERSION), true);
        assert!(matches!(out, NftSwapV2NegotiationOutcome::VersionMismatch { .. }));
    }

    #[test]
    fn rejects_when_either_side_is_legacy() {
        for (m, t) in [
            (LEGACY_SWAP_VERSION, NFT_SWAP_V2_VERSION),
            (NFT_SWAP_V2_VERSION, LEGACY_SWAP_VERSION),
            (LEGACY_SWAP_VERSION, LEGACY_SWAP_VERSION),
        ] {
            let out = should_use_nft_swap_v2(v(m), v(t), true);
            assert!(matches!(out, NftSwapV2NegotiationOutcome::VersionMismatch { .. }));
        }
    }

    #[test]
    fn rejects_when_maker_has_no_nft_contract() {
        assert_eq!(
            should_use_nft_swap_v2(v(NFT_SWAP_V2_VERSION), v(NFT_SWAP_V2_VERSION), false),
            NftSwapV2NegotiationOutcome::NoNftContract
        );
    }

    #[test]
    fn version_mismatch_carries_advertised_versions() {
        let out = should_use_nft_swap_v2(v(LEGACY_SWAP_VERSION), v(NFT_SWAP_V2_VERSION), true);
        match out {
            NftSwapV2NegotiationOutcome::VersionMismatch { maker, taker } => {
                assert_eq!(maker.version, LEGACY_SWAP_VERSION);
                assert_eq!(taker.version, NFT_SWAP_V2_VERSION);
            },
            _ => panic!("expected VersionMismatch"),
        }
    }
}
