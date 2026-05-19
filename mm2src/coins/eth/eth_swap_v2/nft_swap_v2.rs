//! NFT swap V2 calldata layer (P10.3.7 — Slice 1).
//!
//! Pure-function builders and validators for the EtomicSwapMakerV2-NFT
//! contract entrypoints that lock an ERC-721 or ERC-1155 NFT into a
//! hash-time-locked contract (HTLC). Modelled on the fungible-token
//! maker swap V2 layer in [`crate::eth::eth_swap_v2::eth_maker_swap_v2`]
//! but parameterised by an NFT's `(token_address, token_id)` pair (and
//! `amount` for ERC-1155 partial fills).
//!
//! The NFT swap follows the same maker/taker/secret HTLC topology as the
//! fungible-token V2 swaps:
//!
//! 1. **`erc721MakerPayment` / `erc1155MakerPayment`** — Maker locks the
//!    NFT into the contract committing to `(takerSecretHash, makerSecretHash,
//!    paymentLockTime)`.
//! 2. **`spendErc{721,1155}MakerPayment`** — Taker reveals
//!    `makerSecret` (whose hash is `makerSecretHash`) to claim the NFT.
//! 3. **`refundErc{721,1155}MakerPaymentTimelock`** — After
//!    `paymentLockTime`, maker reclaims the NFT.
//! 4. **`refundErc{721,1155}MakerPaymentSecret`** — Taker can refund to
//!    maker by revealing `takerSecret`, used in cooperative aborts.
//!
//! This slice intentionally exposes only the maker-side calldata
//! surface. Wiring into [`crate::eth::EthCoin`]'s `SwapOps` and the V2
//! state machines is deferred to a follow-up slice (the maker is the
//! only side that locks an NFT; the taker side stays on fungible-token
//! V2 paths for NFT-for-fungible swaps, the only kind of NFT swap KDF
//! supports today).

use ethabi::{Contract, Function, Token};
use ethereum_types::{Address, U256};
use lazy_static::lazy_static;

/// Inline minimal ABI for the NFT swap V2 maker contract. Lives next to
/// the calldata builders so the contract surface stays self-documenting
/// and a single source of truth for selectors. The contract layout is
/// our own clean-room design — it deliberately mirrors the fungible
/// `MakerSwapV2` shape so the state-machine layer can be parametrised
/// over both contract families.
pub(crate) const MAKER_NFT_SWAP_V2_ABI: &str = r#"[
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"taker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"paymentLockTime","type":"uint256"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"}
     ],"name":"erc721MakerPayment","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"amount","type":"uint256"},
        {"name":"taker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"paymentLockTime","type":"uint256"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"}
     ],"name":"erc1155MakerPayment","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"maker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecret","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"}
     ],"name":"spendErc721MakerPayment","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"amount","type":"uint256"},
        {"name":"maker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecret","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"}
     ],"name":"spendErc1155MakerPayment","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"taker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"},
        {"name":"paymentLockTime","type":"uint256"}
     ],"name":"refundErc721MakerPaymentTimelock","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"amount","type":"uint256"},
        {"name":"taker","type":"address"},
        {"name":"takerSecretHash","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"},
        {"name":"paymentLockTime","type":"uint256"}
     ],"name":"refundErc1155MakerPaymentTimelock","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"taker","type":"address"},
        {"name":"takerSecret","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"},
        {"name":"paymentLockTime","type":"uint256"}
     ],"name":"refundErc721MakerPaymentSecret","outputs":[],"stateMutability":"nonpayable","type":"function"},
    {"inputs":[
        {"name":"id","type":"bytes32"},
        {"name":"amount","type":"uint256"},
        {"name":"taker","type":"address"},
        {"name":"takerSecret","type":"bytes32"},
        {"name":"makerSecretHash","type":"bytes32"},
        {"name":"tokenAddress","type":"address"},
        {"name":"tokenId","type":"uint256"},
        {"name":"paymentLockTime","type":"uint256"}
     ],"name":"refundErc1155MakerPaymentSecret","outputs":[],"stateMutability":"nonpayable","type":"function"}
]"#;

lazy_static! {
    pub(crate) static ref MAKER_NFT_SWAP_V2: Contract =
        Contract::load(MAKER_NFT_SWAP_V2_ABI.as_bytes()).expect("MAKER_NFT_SWAP_V2 ABI is valid");
}

/// Family of an NFT involved in a swap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NftKind {
    Erc721,
    Erc1155,
}

impl NftKind {
    fn payment_fn(self) -> &'static str {
        match self {
            NftKind::Erc721 => "erc721MakerPayment",
            NftKind::Erc1155 => "erc1155MakerPayment",
        }
    }

    fn spend_fn(self) -> &'static str {
        match self {
            NftKind::Erc721 => "spendErc721MakerPayment",
            NftKind::Erc1155 => "spendErc1155MakerPayment",
        }
    }

    fn refund_timelock_fn(self) -> &'static str {
        match self {
            NftKind::Erc721 => "refundErc721MakerPaymentTimelock",
            NftKind::Erc1155 => "refundErc1155MakerPaymentTimelock",
        }
    }

    fn refund_secret_fn(self) -> &'static str {
        match self {
            NftKind::Erc721 => "refundErc721MakerPaymentSecret",
            NftKind::Erc1155 => "refundErc1155MakerPaymentSecret",
        }
    }
}

/// Errors that can occur while building or validating NFT swap V2
/// calldata.
#[derive(Debug, PartialEq, Eq)]
pub enum NftSwapV2Error {
    /// The contract ABI did not contain the expected entrypoint, or the
    /// `ethabi` encoder rejected the input. Caller has a bug.
    Abi(String),
    /// The decoded calldata did not match the supplied expectations.
    /// Field name carries which argument differed.
    Mismatch { field: &'static str, detail: String },
    /// `amount` argument is required for ERC-1155 (and disallowed for
    /// ERC-721).
    AmountRequiredForErc1155,
    /// ERC-1155 `amount` must be non-zero.
    ZeroAmount,
}

impl std::fmt::Display for NftSwapV2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NftSwapV2Error::Abi(e) => write!(f, "NFT swap V2 ABI error: {e}"),
            NftSwapV2Error::Mismatch { field, detail } => {
                write!(f, "NFT swap V2 calldata mismatch on field `{field}`: {detail}")
            },
            NftSwapV2Error::AmountRequiredForErc1155 => {
                write!(f, "NFT swap V2: ERC-1155 entrypoints require an `amount` argument")
            },
            NftSwapV2Error::ZeroAmount => write!(f, "NFT swap V2: ERC-1155 `amount` must be non-zero"),
        }
    }
}

impl std::error::Error for NftSwapV2Error {}

impl From<ethabi::Error> for NftSwapV2Error {
    fn from(e: ethabi::Error) -> Self {
        NftSwapV2Error::Abi(e.to_string())
    }
}

/// Arguments that uniquely identify an NFT maker payment lock-up.
#[derive(Debug, Clone)]
pub struct NftMakerPaymentArgs {
    pub kind: NftKind,
    /// Pre-computed swap id (32 bytes, typically `keccak(timelock || maker_secret_hash)`).
    pub swap_id: [u8; 32],
    /// `Some(amount)` for ERC-1155, `None` for ERC-721.
    pub amount: Option<U256>,
    pub taker: Address,
    pub taker_secret_hash: [u8; 32],
    pub maker_secret_hash: [u8; 32],
    pub payment_time_lock: u64,
    pub token_address: Address,
    pub token_id: U256,
}

/// Arguments for the taker spend (reveals `maker_secret`) of an NFT
/// maker payment.
#[derive(Debug, Clone)]
pub struct NftSpendMakerPaymentArgs {
    pub kind: NftKind,
    pub swap_id: [u8; 32],
    pub amount: Option<U256>,
    pub maker: Address,
    pub taker_secret_hash: [u8; 32],
    pub maker_secret: [u8; 32],
    pub token_address: Address,
    pub token_id: U256,
}

/// Arguments for the maker timelock refund of an NFT maker payment.
#[derive(Debug, Clone)]
pub struct NftRefundTimelockArgs {
    pub kind: NftKind,
    pub swap_id: [u8; 32],
    pub amount: Option<U256>,
    pub taker: Address,
    pub taker_secret_hash: [u8; 32],
    pub maker_secret_hash: [u8; 32],
    pub token_address: Address,
    pub token_id: U256,
    pub payment_time_lock: u64,
}

/// Arguments for the cooperative-secret refund of an NFT maker payment.
#[derive(Debug, Clone)]
pub struct NftRefundSecretArgs {
    pub kind: NftKind,
    pub swap_id: [u8; 32],
    pub amount: Option<U256>,
    pub taker: Address,
    pub taker_secret: [u8; 32],
    pub maker_secret_hash: [u8; 32],
    pub token_address: Address,
    pub token_id: U256,
    pub payment_time_lock: u64,
}

fn require_amount(args_amount: Option<U256>, kind: NftKind) -> Result<Option<U256>, NftSwapV2Error> {
    match (kind, args_amount) {
        (NftKind::Erc1155, Some(a)) if a.is_zero() => Err(NftSwapV2Error::ZeroAmount),
        (NftKind::Erc1155, Some(a)) => Ok(Some(a)),
        (NftKind::Erc1155, None) => Err(NftSwapV2Error::AmountRequiredForErc1155),
        // ERC-721 ignores `amount`; pass None through unconditionally so
        // a misconfigured caller doesn't accidentally encode it.
        (NftKind::Erc721, _) => Ok(None),
    }
}

/// Build calldata for `erc{721,1155}MakerPayment`.
pub fn encode_maker_payment(args: &NftMakerPaymentArgs) -> Result<Vec<u8>, NftSwapV2Error> {
    let amount = require_amount(args.amount, args.kind)?;
    let function = MAKER_NFT_SWAP_V2.function(args.kind.payment_fn())?;
    let mut tokens: Vec<Token> = vec![Token::FixedBytes(args.swap_id.to_vec())];
    if let Some(a) = amount {
        tokens.push(Token::Uint(a));
    }
    tokens.extend([
        Token::Address(args.taker),
        Token::FixedBytes(args.taker_secret_hash.to_vec()),
        Token::FixedBytes(args.maker_secret_hash.to_vec()),
        Token::Uint(U256::from(args.payment_time_lock)),
        Token::Address(args.token_address),
        Token::Uint(args.token_id),
    ]);
    Ok(function.encode_input(&tokens)?)
}

/// Build calldata for `spendErc{721,1155}MakerPayment`.
pub fn encode_spend_maker_payment(args: &NftSpendMakerPaymentArgs) -> Result<Vec<u8>, NftSwapV2Error> {
    let amount = require_amount(args.amount, args.kind)?;
    let function = MAKER_NFT_SWAP_V2.function(args.kind.spend_fn())?;
    let mut tokens: Vec<Token> = vec![Token::FixedBytes(args.swap_id.to_vec())];
    if let Some(a) = amount {
        tokens.push(Token::Uint(a));
    }
    tokens.extend([
        Token::Address(args.maker),
        Token::FixedBytes(args.taker_secret_hash.to_vec()),
        Token::FixedBytes(args.maker_secret.to_vec()),
        Token::Address(args.token_address),
        Token::Uint(args.token_id),
    ]);
    Ok(function.encode_input(&tokens)?)
}

/// Build calldata for `refundErc{721,1155}MakerPaymentTimelock`.
pub fn encode_refund_timelock(args: &NftRefundTimelockArgs) -> Result<Vec<u8>, NftSwapV2Error> {
    let amount = require_amount(args.amount, args.kind)?;
    let function = MAKER_NFT_SWAP_V2.function(args.kind.refund_timelock_fn())?;
    let mut tokens: Vec<Token> = vec![Token::FixedBytes(args.swap_id.to_vec())];
    if let Some(a) = amount {
        tokens.push(Token::Uint(a));
    }
    tokens.extend([
        Token::Address(args.taker),
        Token::FixedBytes(args.taker_secret_hash.to_vec()),
        Token::FixedBytes(args.maker_secret_hash.to_vec()),
        Token::Address(args.token_address),
        Token::Uint(args.token_id),
        Token::Uint(U256::from(args.payment_time_lock)),
    ]);
    Ok(function.encode_input(&tokens)?)
}

/// Build calldata for `refundErc{721,1155}MakerPaymentSecret`.
pub fn encode_refund_secret(args: &NftRefundSecretArgs) -> Result<Vec<u8>, NftSwapV2Error> {
    let amount = require_amount(args.amount, args.kind)?;
    let function = MAKER_NFT_SWAP_V2.function(args.kind.refund_secret_fn())?;
    let mut tokens: Vec<Token> = vec![Token::FixedBytes(args.swap_id.to_vec())];
    if let Some(a) = amount {
        tokens.push(Token::Uint(a));
    }
    tokens.extend([
        Token::Address(args.taker),
        Token::FixedBytes(args.taker_secret.to_vec()),
        Token::FixedBytes(args.maker_secret_hash.to_vec()),
        Token::Address(args.token_address),
        Token::Uint(args.token_id),
        Token::Uint(U256::from(args.payment_time_lock)),
    ]);
    Ok(function.encode_input(&tokens)?)
}

/// Decode calldata for `erc{721,1155}MakerPayment` against `kind`.
/// Returns the token list in declaration order.
pub fn decode_maker_payment(kind: NftKind, calldata: &[u8]) -> Result<Vec<Token>, NftSwapV2Error> {
    decode_call(kind.payment_fn(), calldata)
}

/// Decode calldata for `spendErc{721,1155}MakerPayment` against `kind`.
pub fn decode_spend_maker_payment(kind: NftKind, calldata: &[u8]) -> Result<Vec<Token>, NftSwapV2Error> {
    decode_call(kind.spend_fn(), calldata)
}

fn decode_call(name: &str, calldata: &[u8]) -> Result<Vec<Token>, NftSwapV2Error> {
    if calldata.len() < 4 {
        return Err(NftSwapV2Error::Mismatch {
            field: "selector",
            detail: "calldata shorter than 4 bytes".to_owned(),
        });
    }
    let function = MAKER_NFT_SWAP_V2.function(name)?;
    let expected_selector = function.short_signature();
    if calldata[..4] != expected_selector {
        return Err(NftSwapV2Error::Mismatch {
            field: "selector",
            detail: format!(
                "expected selector {} for `{}`, got {}",
                hex::encode(expected_selector),
                name,
                hex::encode(&calldata[..4])
            ),
        });
    }
    Ok(function.decode_input(calldata)?)
}

/// Validate decoded `erc{721,1155}MakerPayment` calldata against the
/// expected [`NftMakerPaymentArgs`]. Returns `Ok(())` on full match.
pub fn validate_maker_payment(decoded: &[Token], args: &NftMakerPaymentArgs) -> Result<(), NftSwapV2Error> {
    let mut idx = 0usize;
    expect_fixed_bytes(decoded, &mut idx, "id", &args.swap_id)?;
    if args.kind == NftKind::Erc1155 {
        let amount = args.amount.ok_or(NftSwapV2Error::AmountRequiredForErc1155)?;
        expect_uint(decoded, &mut idx, "amount", amount)?;
    }
    expect_address(decoded, &mut idx, "taker", args.taker)?;
    expect_fixed_bytes(decoded, &mut idx, "takerSecretHash", &args.taker_secret_hash)?;
    expect_fixed_bytes(decoded, &mut idx, "makerSecretHash", &args.maker_secret_hash)?;
    expect_uint(decoded, &mut idx, "paymentLockTime", U256::from(args.payment_time_lock))?;
    expect_address(decoded, &mut idx, "tokenAddress", args.token_address)?;
    expect_uint(decoded, &mut idx, "tokenId", args.token_id)?;
    Ok(())
}

/// Compute the on-chain function selector (4-byte) for a given
/// `(kind, op)` pair. Useful for log-based event matching.
pub fn maker_payment_selector(kind: NftKind) -> [u8; 4] {
    MAKER_NFT_SWAP_V2
        .function(kind.payment_fn())
        .expect("ABI contains entrypoint")
        .short_signature()
}

// ──────────────────────────────────────────────────────────────────────
//  Decoded-token helpers
// ──────────────────────────────────────────────────────────────────────

fn token_at<'a>(decoded: &'a [Token], idx: usize, field: &'static str) -> Result<&'a Token, NftSwapV2Error> {
    decoded.get(idx).ok_or(NftSwapV2Error::Mismatch {
        field,
        detail: format!("missing argument at position {idx}"),
    })
}

fn expect_fixed_bytes(
    decoded: &[Token],
    idx: &mut usize,
    field: &'static str,
    expected: &[u8],
) -> Result<(), NftSwapV2Error> {
    match token_at(decoded, *idx, field)? {
        Token::FixedBytes(bytes) if bytes.as_slice() == expected => {
            *idx += 1;
            Ok(())
        },
        other => Err(NftSwapV2Error::Mismatch {
            field,
            detail: format!("expected FixedBytes({}), got {other:?}", hex::encode(expected)),
        }),
    }
}

fn expect_address(
    decoded: &[Token],
    idx: &mut usize,
    field: &'static str,
    expected: Address,
) -> Result<(), NftSwapV2Error> {
    match token_at(decoded, *idx, field)? {
        Token::Address(addr) if *addr == expected => {
            *idx += 1;
            Ok(())
        },
        other => Err(NftSwapV2Error::Mismatch {
            field,
            detail: format!("expected Address({expected:?}), got {other:?}"),
        }),
    }
}

fn expect_uint(decoded: &[Token], idx: &mut usize, field: &'static str, expected: U256) -> Result<(), NftSwapV2Error> {
    match token_at(decoded, *idx, field)? {
        Token::Uint(u) if *u == expected => {
            *idx += 1;
            Ok(())
        },
        other => Err(NftSwapV2Error::Mismatch {
            field,
            detail: format!("expected Uint({expected}), got {other:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(byte: u8) -> Address {
        Address::from([byte; 20])
    }

    fn hash32(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn sample_erc721_args() -> NftMakerPaymentArgs {
        NftMakerPaymentArgs {
            kind: NftKind::Erc721,
            swap_id: hash32(0xAA),
            amount: None,
            taker: addr(0x11),
            taker_secret_hash: hash32(0xBB),
            maker_secret_hash: hash32(0xCC),
            payment_time_lock: 1_900_000_000,
            token_address: addr(0x22),
            token_id: U256::from(1234u64),
        }
    }

    fn sample_erc1155_args() -> NftMakerPaymentArgs {
        NftMakerPaymentArgs {
            kind: NftKind::Erc1155,
            swap_id: hash32(0xDD),
            amount: Some(U256::from(7u64)),
            taker: addr(0x33),
            taker_secret_hash: hash32(0xEE),
            maker_secret_hash: hash32(0x99),
            payment_time_lock: 1_950_000_000,
            token_address: addr(0x44),
            token_id: U256::from(56u64),
        }
    }

    #[test]
    fn maker_payment_selector_is_stable_for_erc721() {
        let sel = maker_payment_selector(NftKind::Erc721);
        // Recompute via ethabi to confirm it matches.
        let want = MAKER_NFT_SWAP_V2
            .function("erc721MakerPayment")
            .unwrap()
            .short_signature();
        assert_eq!(sel, want);
    }

    #[test]
    fn maker_payment_selector_is_stable_for_erc1155() {
        let sel = maker_payment_selector(NftKind::Erc1155);
        let want = MAKER_NFT_SWAP_V2
            .function("erc1155MakerPayment")
            .unwrap()
            .short_signature();
        assert_eq!(sel, want);
    }

    #[test]
    fn erc721_and_erc1155_selectors_differ() {
        assert_ne!(
            maker_payment_selector(NftKind::Erc721),
            maker_payment_selector(NftKind::Erc1155)
        );
    }

    #[test]
    fn encode_then_decode_roundtrip_erc721_payment() {
        let args = sample_erc721_args();
        let calldata = encode_maker_payment(&args).expect("encode");
        // First 4 bytes are selector.
        assert_eq!(calldata[..4], maker_payment_selector(NftKind::Erc721));
        let decoded = decode_maker_payment(NftKind::Erc721, &calldata).expect("decode");
        validate_maker_payment(&decoded, &args).expect("validate");
    }

    #[test]
    fn encode_then_decode_roundtrip_erc1155_payment() {
        let args = sample_erc1155_args();
        let calldata = encode_maker_payment(&args).expect("encode");
        assert_eq!(calldata[..4], maker_payment_selector(NftKind::Erc1155));
        let decoded = decode_maker_payment(NftKind::Erc1155, &calldata).expect("decode");
        validate_maker_payment(&decoded, &args).expect("validate");
    }

    #[test]
    fn validate_rejects_wrong_taker() {
        let args = sample_erc721_args();
        let calldata = encode_maker_payment(&args).expect("encode");
        let decoded = decode_maker_payment(NftKind::Erc721, &calldata).expect("decode");
        let mut bad = args.clone();
        bad.taker = addr(0xFF);
        let err = validate_maker_payment(&decoded, &bad).unwrap_err();
        match err {
            NftSwapV2Error::Mismatch { field, .. } => assert_eq!(field, "taker"),
            other => panic!("expected Mismatch on taker, got {other:?}"),
        }
    }

    #[test]
    fn validate_rejects_wrong_token_id() {
        let args = sample_erc1155_args();
        let calldata = encode_maker_payment(&args).expect("encode");
        let decoded = decode_maker_payment(NftKind::Erc1155, &calldata).expect("decode");
        let mut bad = args.clone();
        bad.token_id = U256::from(9999u64);
        let err = validate_maker_payment(&decoded, &bad).unwrap_err();
        match err {
            NftSwapV2Error::Mismatch { field, .. } => assert_eq!(field, "tokenId"),
            other => panic!("expected Mismatch on tokenId, got {other:?}"),
        }
    }

    #[test]
    fn decode_rejects_wrong_kind() {
        let args = sample_erc721_args();
        let calldata = encode_maker_payment(&args).expect("encode");
        let err = decode_maker_payment(NftKind::Erc1155, &calldata).unwrap_err();
        match err {
            NftSwapV2Error::Mismatch { field, .. } => assert_eq!(field, "selector"),
            other => panic!("expected selector mismatch, got {other:?}"),
        }
    }

    #[test]
    fn erc1155_requires_amount() {
        let mut args = sample_erc1155_args();
        args.amount = None;
        let err = encode_maker_payment(&args).unwrap_err();
        assert_eq!(err, NftSwapV2Error::AmountRequiredForErc1155);
    }

    #[test]
    fn erc1155_rejects_zero_amount() {
        let mut args = sample_erc1155_args();
        args.amount = Some(U256::zero());
        let err = encode_maker_payment(&args).unwrap_err();
        assert_eq!(err, NftSwapV2Error::ZeroAmount);
    }

    #[test]
    fn erc721_ignores_amount() {
        let mut args = sample_erc721_args();
        args.amount = Some(U256::from(42u64));
        // Encoding still succeeds and produces the no-amount form.
        let calldata = encode_maker_payment(&args).expect("encode");
        let decoded = decode_maker_payment(NftKind::Erc721, &calldata).expect("decode");
        let mut clean = args.clone();
        clean.amount = None;
        validate_maker_payment(&decoded, &clean).expect("validate");
    }

    #[test]
    fn spend_payment_roundtrip_erc721() {
        let args = NftSpendMakerPaymentArgs {
            kind: NftKind::Erc721,
            swap_id: hash32(0x10),
            amount: None,
            maker: addr(0x55),
            taker_secret_hash: hash32(0x20),
            maker_secret: hash32(0x30),
            token_address: addr(0x66),
            token_id: U256::from(7u64),
        };
        let calldata = encode_spend_maker_payment(&args).expect("encode");
        let decoded = decode_spend_maker_payment(NftKind::Erc721, &calldata).expect("decode");
        // Spot-check: third token is `taker` for payment, `maker` for spend.
        assert!(matches!(decoded.first(), Some(Token::FixedBytes(b)) if b.as_slice() == args.swap_id));
        assert!(matches!(decoded.get(1), Some(Token::Address(a)) if *a == args.maker));
    }

    #[test]
    fn spend_payment_roundtrip_erc1155() {
        let args = NftSpendMakerPaymentArgs {
            kind: NftKind::Erc1155,
            swap_id: hash32(0x40),
            amount: Some(U256::from(3u64)),
            maker: addr(0x77),
            taker_secret_hash: hash32(0x50),
            maker_secret: hash32(0x60),
            token_address: addr(0x88),
            token_id: U256::from(99u64),
        };
        let calldata = encode_spend_maker_payment(&args).expect("encode");
        let decoded = decode_spend_maker_payment(NftKind::Erc1155, &calldata).expect("decode");
        // Position 1 must be `amount` for ERC-1155 spend.
        assert!(matches!(decoded.get(1), Some(Token::Uint(u)) if *u == U256::from(3u64)));
    }

    #[test]
    fn refund_timelock_encodes_for_both_kinds() {
        for (kind, amount) in [(NftKind::Erc721, None), (NftKind::Erc1155, Some(U256::from(2u64)))] {
            let args = NftRefundTimelockArgs {
                kind,
                swap_id: hash32(0x70),
                amount,
                taker: addr(0xAA),
                taker_secret_hash: hash32(0x80),
                maker_secret_hash: hash32(0x90),
                token_address: addr(0xBB),
                token_id: U256::from(1u64),
                payment_time_lock: 1_700_000_000,
            };
            let calldata = encode_refund_timelock(&args).expect("encode");
            assert!(calldata.len() >= 4, "calldata must include selector");
        }
    }

    #[test]
    fn refund_secret_encodes_for_both_kinds() {
        for (kind, amount) in [(NftKind::Erc721, None), (NftKind::Erc1155, Some(U256::from(5u64)))] {
            let args = NftRefundSecretArgs {
                kind,
                swap_id: hash32(0xA0),
                amount,
                taker: addr(0xCC),
                taker_secret: hash32(0xB0),
                maker_secret_hash: hash32(0xC0),
                token_address: addr(0xDD),
                token_id: U256::from(8u64),
                payment_time_lock: 1_800_000_000,
            };
            let calldata = encode_refund_secret(&args).expect("encode");
            assert!(calldata.len() >= 4);
        }
    }

    #[test]
    fn decode_rejects_short_calldata() {
        let err = decode_maker_payment(NftKind::Erc721, &[0u8, 1, 2]).unwrap_err();
        match err {
            NftSwapV2Error::Mismatch { field, .. } => assert_eq!(field, "selector"),
            other => panic!("expected selector mismatch, got {other:?}"),
        }
    }

    // P10.3.7.b — gas limit dispatch tests
    use crate::eth::eth_swap_v2::PaymentMethod;
    use crate::eth::eth_types::EthGasLimitV2;

    #[test]
    fn nft_gas_limit_dispatches_per_kind_and_method() {
        let g = EthGasLimitV2::default();
        // Defaults defined in EthGasLimitV2::default(): 200k for ERC-721,
        // 220k for ERC-1155, across all four maker-side methods.
        for method in [
            PaymentMethod::Send,
            PaymentMethod::Spend,
            PaymentMethod::RefundTimelock,
            PaymentMethod::RefundSecret,
        ] {
            assert_eq!(g.nft_gas_limit(NftKind::Erc721, method), 200_000);
            assert_eq!(g.nft_gas_limit(NftKind::Erc1155, method), 220_000);
        }
    }

    #[test]
    fn nft_gas_limit_methods_are_distinct_fields() {
        // Mutating one method's slot must not affect others.
        let mut g = EthGasLimitV2::default();
        g.maker.nft_erc721_payment = 1;
        g.maker.nft_erc721_taker_spend = 2;
        g.maker.nft_erc721_maker_refund_timelock = 3;
        g.maker.nft_erc721_maker_refund_secret = 4;
        assert_eq!(g.nft_gas_limit(NftKind::Erc721, PaymentMethod::Send), 1);
        assert_eq!(g.nft_gas_limit(NftKind::Erc721, PaymentMethod::Spend), 2);
        assert_eq!(g.nft_gas_limit(NftKind::Erc721, PaymentMethod::RefundTimelock), 3);
        assert_eq!(g.nft_gas_limit(NftKind::Erc721, PaymentMethod::RefundSecret), 4);
        // ERC-1155 row untouched.
        assert_eq!(g.nft_gas_limit(NftKind::Erc1155, PaymentMethod::Send), 220_000);
    }
}
