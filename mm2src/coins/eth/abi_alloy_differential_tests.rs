//! Live dual-codec differential gate: ethabi 17 vs alloy (`alloy-json-abi` +
//! `alloy-dyn-abi`).
//!
//! Stage 1 of the ethabi -> alloy migration. Before any production code is
//! switched over, this proves that alloy is a **byte-identical drop-in** for
//! ethabi on every ABI KDF actually uses. It runs BOTH codecs in the same test
//! and asserts:
//!   1. selector equivalence — `alloy Function::selector()` ==
//!      `ethabi Function::short_signature()` == first 4 bytes of the calldata;
//!   2. wire round-trip — decoding ethabi-produced calldata with alloy and
//!      re-encoding it with alloy reproduces the *exact same bytes*.
//!
//! The ethabi calldata used as ground truth is the same set already frozen
//! byte-for-byte in `abi_golden_tests` (`abi_wire_golden_*`), so a pass here
//! means "swapping ethabi for alloy keeps the on-the-wire calldata identical".
//!
//! This module is test-only and exists only for the duration of the migration;
//! it is removed once ethabi is gone. The token sets are duplicated from
//! `abi_golden_tests` on purpose, to keep this gate self-contained and avoid
//! perturbing the frozen golden helpers.

use super::eth_types::{ERC20_ABI, ERC20_CONTRACT, MAKER_SWAP_V2, MAKER_SWAP_V2_ABI, SWAP_CONTRACT, SWAP_CONTRACT_ABI,
                       TAKER_SWAP_V2, TAKER_SWAP_V2_ABI};
use alloy_dyn_abi::JsonAbiExt;
use alloy_json_abi::JsonAbi;
use ethabi::{Contract, Token};
use ethereum_types::{Address, U256};

const QTUM_DELEGATE_CONTRACT_ABI: &str = r#"[{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"_staker","type":"address"},{"indexed":true,"internalType":"address","name":"_delegate","type":"address"},{"indexed":false,"internalType":"uint8","name":"fee","type":"uint8"},{"indexed":false,"internalType":"uint256","name":"blockHeight","type":"uint256"},{"indexed":false,"internalType":"bytes","name":"PoD","type":"bytes"}],"name":"AddDelegation","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"_staker","type":"address"},{"indexed":true,"internalType":"address","name":"_delegate","type":"address"}],"name":"RemoveDelegation","type":"event"},{"constant":false,"inputs":[{"internalType":"address","name":"_staker","type":"address"},{"internalType":"uint8","name":"_fee","type":"uint8"},{"internalType":"bytes","name":"_PoD","type":"bytes"}],"name":"addDelegation","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"},{"constant":true,"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"delegations","outputs":[{"internalType":"address","name":"staker","type":"address"},{"internalType":"uint8","name":"fee","type":"uint8"},{"internalType":"uint256","name":"blockHeight","type":"uint256"},{"internalType":"bytes","name":"PoD","type":"bytes"}],"payable":false,"stateMutability":"view","type":"function"},{"constant":false,"inputs":[],"name":"removeDelegation","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"}]"#;

fn b(byte: u8, n: usize) -> Vec<u8> { vec![byte; n] }

/// For one ABI: prove alloy's selector + wire round-trip match ethabi for every
/// listed function. `ethabi` is the already-frozen encoder; `abi_json` is fed to
/// alloy's JSON-ABI parser.
fn assert_codecs_agree(group: &str, abi_json: &str, ethabi_contract: &Contract, cases: &[(&str, Vec<Token>)]) {
    let alloy_abi: JsonAbi =
        serde_json::from_str(abi_json).unwrap_or_else(|e| panic!("[{group}] alloy failed to parse ABI JSON: {e:?}"));

    for (name, tokens) in cases {
        let ethabi_fn = ethabi_contract
            .function(name)
            .unwrap_or_else(|e| panic!("[{group}] ethabi missing `{name}`: {e:?}"));
        let calldata = ethabi_fn
            .encode_input(tokens)
            .unwrap_or_else(|e| panic!("[{group}] ethabi encode `{name}`: {e:?}"));

        let overloads = alloy_abi
            .function(name)
            .unwrap_or_else(|| panic!("[{group}] alloy ABI missing `{name}`"));
        assert_eq!(
            overloads.len(),
            1,
            "[{group}] `{name}` is overloaded — case list needs disambiguation"
        );
        let alloy_fn = &overloads[0];

        // 1. selector: alloy == ethabi == calldata prefix.
        assert_eq!(
            alloy_fn.selector().as_slice(),
            &ethabi_fn.short_signature()[..],
            "[{group}] selector mismatch alloy vs ethabi for `{name}`"
        );
        assert_eq!(
            alloy_fn.selector().as_slice(),
            &calldata[..4],
            "[{group}] alloy selector != calldata prefix for `{name}`"
        );

        // 2. wire round-trip: alloy decodes ethabi's calldata body and
        //    re-encodes it to the exact same full calldata (selector + args).
        let decoded = alloy_fn
            .abi_decode_input(&calldata[4..])
            .unwrap_or_else(|e| panic!("[{group}] alloy decode `{name}`: {e:?}"));
        let reencoded = alloy_fn
            .abi_encode_input(&decoded)
            .unwrap_or_else(|e| panic!("[{group}] alloy encode `{name}`: {e:?}"));
        assert_eq!(
            hex::encode(&reencoded),
            hex::encode(&calldata),
            "[{group}] alloy round-trip diverged from ethabi calldata for `{name}`"
        );
    }
}

#[test]
fn alloy_matches_ethabi_v1_swap() {
    let addr1 = Address::from([0x11u8; 20]);
    let addr2 = Address::from([0x22u8; 20]);
    let amount = U256::from(123_456_789u64);
    let lock64 = U256::from(1_700_000_000u64);
    let cases: &[(&str, Vec<Token>)] = &[
        ("ethPayment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Address(addr1),
            Token::FixedBytes(b(0x66, 20)),
            Token::Uint(lock64),
        ]),
        ("erc20Payment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Address(addr1),
            Token::Address(addr2),
            Token::FixedBytes(b(0x66, 20)),
            Token::Uint(lock64),
        ]),
        ("receiverSpend", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::FixedBytes(b(0x44, 32)),
            Token::Address(addr1),
            Token::Address(addr2),
        ]),
        ("senderRefund", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::FixedBytes(b(0x66, 20)),
            Token::Address(addr1),
            Token::Address(addr2),
        ]),
    ];
    assert_codecs_agree("v1_swap", SWAP_CONTRACT_ABI, &SWAP_CONTRACT, cases);
}

#[test]
fn alloy_matches_ethabi_erc20() {
    let addr1 = Address::from([0x11u8; 20]);
    let addr2 = Address::from([0x22u8; 20]);
    let amount = U256::from(123_456_789u64);
    let cases: &[(&str, Vec<Token>)] = &[
        ("approve", vec![Token::Address(addr1), Token::Uint(amount)]),
        ("transfer", vec![Token::Address(addr1), Token::Uint(amount)]),
        ("transferFrom", vec![
            Token::Address(addr1),
            Token::Address(addr2),
            Token::Uint(amount),
        ]),
        ("balanceOf", vec![Token::Address(addr1)]),
    ];
    assert_codecs_agree("erc20", ERC20_ABI, &ERC20_CONTRACT, cases);
}

#[test]
fn alloy_matches_ethabi_maker_swap_v2() {
    let addr1 = Address::from([0x11u8; 20]);
    let addr2 = Address::from([0x22u8; 20]);
    let amount = U256::from(123_456_789u64);
    let lock32 = U256::from(1_800_000_000u64);
    let refund_spend = |name: &'static str| {
        (name, vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Address(addr1),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Address(addr2),
        ])
    };
    let cases: &[(&str, Vec<Token>)] = &[
        ("erc20MakerPayment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Address(addr1),
            Token::Address(addr2),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Uint(lock32),
        ]),
        ("ethMakerPayment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Address(addr1),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Uint(lock32),
        ]),
        refund_spend("refundMakerPaymentSecret"),
        refund_spend("refundMakerPaymentTimelock"),
        refund_spend("spendMakerPayment"),
    ];
    assert_codecs_agree("maker_v2", MAKER_SWAP_V2_ABI, &MAKER_SWAP_V2, cases);
}

#[test]
fn alloy_matches_ethabi_taker_swap_v2() {
    let addr1 = Address::from([0x11u8; 20]);
    let addr2 = Address::from([0x22u8; 20]);
    let amount = U256::from(123_456_789u64);
    let amount2 = U256::from(987_654_321u64);
    let lock32 = U256::from(1_800_000_000u64);
    let lock32b = U256::from(1_900_000_000u64);
    let refund_spend = |name: &'static str| {
        (name, vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Uint(amount2),
            Token::Address(addr1),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Address(addr2),
        ])
    };
    let cases: &[(&str, Vec<Token>)] = &[
        ("erc20TakerPayment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Uint(amount2),
            Token::Address(addr1),
            Token::Address(addr2),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Uint(lock32),
            Token::Uint(lock32b),
        ]),
        ("ethTakerPayment", vec![
            Token::FixedBytes(b(0x33, 32)),
            Token::Uint(amount),
            Token::Address(addr1),
            Token::FixedBytes(b(0x44, 32)),
            Token::FixedBytes(b(0x55, 32)),
            Token::Uint(lock32),
            Token::Uint(lock32b),
        ]),
        refund_spend("refundTakerPaymentSecret"),
        refund_spend("refundTakerPaymentTimelock"),
        refund_spend("spendTakerPayment"),
        refund_spend("takerPaymentApprove"),
    ];
    assert_codecs_agree("taker_v2", TAKER_SWAP_V2_ABI, &TAKER_SWAP_V2, cases);
}

#[test]
fn alloy_matches_ethabi_qtum_delegation() {
    // Exercises a dynamic `bytes` tail + uint8, which the swap/erc20 cases do
    // not. ethabi contract is loaded from the same inline ABI.
    let qtum = Contract::load(QTUM_DELEGATE_CONTRACT_ABI.as_bytes()).unwrap();
    let addr1 = Address::from([0x11u8; 20]);
    let fee = U256::from(7u64);
    let cases: &[(&str, Vec<Token>)] = &[
        ("addDelegation", vec![
            Token::Address(addr1),
            Token::Uint(fee),
            Token::Bytes(b(0x77, 40)),
        ]),
        ("removeDelegation", vec![]),
    ];
    assert_codecs_agree("qtum_delegation", QTUM_DELEGATE_CONTRACT_ABI, &qtum, cases);
}
