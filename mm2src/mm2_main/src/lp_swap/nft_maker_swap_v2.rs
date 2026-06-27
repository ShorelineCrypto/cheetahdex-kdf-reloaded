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

use crate::mm2::lp_swap::swap_versioning::SwapVersion;
use coins::eth::nft_swap_v2::{EthCoinNftError, NftKind, NftMakerPaymentArgs, NftRefundSecretArgs,
                              NftRefundTimelockArgs, NftSpendMakerPaymentArgs};
use coins::eth::{EthCoin, SignedEthTx};
use ethereum_types::{Address, U256};
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

/// A maker-side NFT that is eligible for the NFT swap V2 maker branch.
///
/// The constructors encode the ERC-721 vs ERC-1155 shape required by Chapter
/// 17: ERC-721 carries exactly one token id, while ERC-1155 carries token id
/// plus a non-zero amount. Activation code is still responsible for proving
/// the token belongs to an enabled EVM NFT collection before constructing this
/// value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmNftMakerAsset {
    kind: NftKind,
    token_address: Address,
    token_id: U256,
    amount: Option<U256>,
}

impl EvmNftMakerAsset {
    pub fn erc721(token_address: Address, token_id: U256) -> Self {
        EvmNftMakerAsset {
            kind: NftKind::Erc721,
            token_address,
            token_id,
            amount: None,
        }
    }

    pub fn erc1155(token_address: Address, token_id: U256, amount: U256) -> Result<Self, EvmNftMakerAssetError> {
        if amount.is_zero() {
            return Err(EvmNftMakerAssetError::ZeroErc1155Amount);
        }

        Ok(EvmNftMakerAsset {
            kind: NftKind::Erc1155,
            token_address,
            token_id,
            amount: Some(amount),
        })
    }

    pub fn kind(&self) -> NftKind { self.kind }

    pub fn token_address(&self) -> Address { self.token_address }

    pub fn token_id(&self) -> U256 { self.token_id }

    pub fn amount(&self) -> Option<U256> { self.amount }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmNftMakerAssetError {
    ZeroErc1155Amount,
}

impl std::fmt::Display for EvmNftMakerAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvmNftMakerAssetError::ZeroErc1155Amount => write!(f, "ERC-1155 NFT swap amount must be non-zero"),
        }
    }
}

impl std::error::Error for EvmNftMakerAssetError {}

/// Taker payment asset family for the NFT V2 dispatcher.
///
/// NFT V2 is maker-NFT-for-taker-fungible only; there is intentionally no
/// taker-side NFT operation surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSwapV2TakerAsset {
    Fungible,
    Nft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSwapV2MakerBranch {
    Nft {
        kind: NftKind,
        token_address: Address,
        token_id: U256,
        amount: Option<U256>,
    },
    NegotiatedFungiblePath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSwapV2TakerBranch {
    FungibleEvmV2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSwapV2Fallback {
    NegotiatedFungiblePathIfRepresentable,
}

/// State-machine dispatch result for a candidate maker-NFT V2 swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftSwapV2StateMachineDispatch {
    UseNftV2Path {
        maker: NftSwapV2MakerBranch,
        taker: NftSwapV2TakerBranch,
    },
    VersionMismatch {
        maker: SwapVersion,
        taker: SwapVersion,
        fallback: NftSwapV2Fallback,
    },
    NoNftContractConfigured,
    UnsupportedTakerNft,
}

/// Resolve the maker/taker state-machine branches for a candidate maker-NFT
/// swap. This is a pure local decision: no network or chain state is read.
pub fn select_nft_swap_v2_state_machine_dispatch(
    maker_version: SwapVersion,
    taker_version: SwapVersion,
    maker_has_nft_contract: bool,
    maker_asset: &EvmNftMakerAsset,
    taker_asset: NftSwapV2TakerAsset,
) -> NftSwapV2StateMachineDispatch {
    if taker_asset == NftSwapV2TakerAsset::Nft {
        return NftSwapV2StateMachineDispatch::UnsupportedTakerNft;
    }

    match should_use_nft_swap_v2(maker_version, taker_version, maker_has_nft_contract) {
        NftSwapV2NegotiationOutcome::Use => NftSwapV2StateMachineDispatch::UseNftV2Path {
            maker: NftSwapV2MakerBranch::Nft {
                kind: maker_asset.kind(),
                token_address: maker_asset.token_address(),
                token_id: maker_asset.token_id(),
                amount: maker_asset.amount(),
            },
            taker: NftSwapV2TakerBranch::FungibleEvmV2,
        },
        NftSwapV2NegotiationOutcome::VersionMismatch { maker, taker } => {
            NftSwapV2StateMachineDispatch::VersionMismatch {
                maker,
                taker,
                fallback: NftSwapV2Fallback::NegotiatedFungiblePathIfRepresentable,
            }
        },
        NftSwapV2NegotiationOutcome::NoNftContract => NftSwapV2StateMachineDispatch::NoNftContractConfigured,
    }
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
    use crate::mm2::lp_swap::swap_versioning::{LEGACY_SWAP_VERSION, NFT_SWAP_V2_VERSION, TPU_SWAP_VERSION};

    fn v(n: u8) -> SwapVersion { SwapVersion { version: n } }

    fn token_address() -> Address {
        let mut bytes = [0u8; 20];
        bytes[18] = 0xE7;
        bytes[19] = 0x21;
        Address::from(bytes)
    }

    fn erc721_asset() -> EvmNftMakerAsset { EvmNftMakerAsset::erc721(token_address(), U256::from(721u64)) }

    fn erc1155_asset() -> EvmNftMakerAsset {
        EvmNftMakerAsset::erc1155(token_address(), U256::from(1155u64), U256::from(3u64)).unwrap()
    }

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

    #[test]
    fn t17_9_1_use_nft_v2_path_dispatches_erc721_and_erc1155_maker_with_fungible_taker() {
        for asset in [erc721_asset(), erc1155_asset()] {
            let dispatch = select_nft_swap_v2_state_machine_dispatch(
                v(NFT_SWAP_V2_VERSION),
                v(NFT_SWAP_V2_VERSION),
                true,
                &asset,
                NftSwapV2TakerAsset::Fungible,
            );

            assert_eq!(dispatch, NftSwapV2StateMachineDispatch::UseNftV2Path {
                maker: NftSwapV2MakerBranch::Nft {
                    kind: asset.kind(),
                    token_address: asset.token_address(),
                    token_id: asset.token_id(),
                    amount: asset.amount(),
                },
                taker: NftSwapV2TakerBranch::FungibleEvmV2,
            });
        }
    }

    #[test]
    fn t17_9_2_version_mismatch_preserves_versions_and_uses_fungible_fallback() {
        for (maker_version, taker_version) in [
            (NFT_SWAP_V2_VERSION, TPU_SWAP_VERSION),
            (TPU_SWAP_VERSION, NFT_SWAP_V2_VERSION),
        ] {
            let dispatch = select_nft_swap_v2_state_machine_dispatch(
                v(maker_version),
                v(taker_version),
                true,
                &erc721_asset(),
                NftSwapV2TakerAsset::Fungible,
            );

            assert_eq!(dispatch, NftSwapV2StateMachineDispatch::VersionMismatch {
                maker: v(maker_version),
                taker: v(taker_version),
                fallback: NftSwapV2Fallback::NegotiatedFungiblePathIfRepresentable,
            });
            assert!(!matches!(dispatch, NftSwapV2StateMachineDispatch::UseNftV2Path {
                maker: NftSwapV2MakerBranch::Nft { .. },
                ..
            }));
        }
    }

    #[test]
    fn t17_9_3_no_nft_contract_refuses_without_fallback() {
        let dispatch = select_nft_swap_v2_state_machine_dispatch(
            v(NFT_SWAP_V2_VERSION),
            v(NFT_SWAP_V2_VERSION),
            false,
            &erc721_asset(),
            NftSwapV2TakerAsset::Fungible,
        );

        assert_eq!(dispatch, NftSwapV2StateMachineDispatch::NoNftContractConfigured);
        assert!(!matches!(dispatch, NftSwapV2StateMachineDispatch::VersionMismatch {
            fallback: NftSwapV2Fallback::NegotiatedFungiblePathIfRepresentable,
            ..
        }));
    }

    #[test]
    fn t17_9_4_rejects_taker_nft_and_keeps_fungible_taker_for_valid_maker_nft() {
        let unsupported = select_nft_swap_v2_state_machine_dispatch(
            v(NFT_SWAP_V2_VERSION),
            v(NFT_SWAP_V2_VERSION),
            true,
            &erc721_asset(),
            NftSwapV2TakerAsset::Nft,
        );
        assert_eq!(unsupported, NftSwapV2StateMachineDispatch::UnsupportedTakerNft);

        let supported = select_nft_swap_v2_state_machine_dispatch(
            v(NFT_SWAP_V2_VERSION),
            v(NFT_SWAP_V2_VERSION),
            true,
            &erc1155_asset(),
            NftSwapV2TakerAsset::Fungible,
        );
        assert!(matches!(supported, NftSwapV2StateMachineDispatch::UseNftV2Path {
            maker: NftSwapV2MakerBranch::Nft {
                kind: NftKind::Erc1155,
                ..
            },
            taker: NftSwapV2TakerBranch::FungibleEvmV2,
        }));
    }

    #[test]
    fn r17_9_1_erc1155_maker_asset_requires_non_zero_amount() {
        assert_eq!(
            EvmNftMakerAsset::erc1155(token_address(), U256::from(1155u64), U256::zero()),
            Err(EvmNftMakerAssetError::ZeroErc1155Amount)
        );
    }
}
