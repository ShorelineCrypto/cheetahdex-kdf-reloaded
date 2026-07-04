//! Wire-compatibility golden vectors for ABI-encoded calldata.
//!
//! These tests freeze the exact bytes produced by the ABI encoder for the
//! contract functions KDF builds calldata for. They guard the ethabi 6.1 -> 17
//! migration (and the later alloy migration): encoded calldata is funds-critical
//! and MUST remain byte-identical across the upgrade.
//!
//! Two groups:
//! * `legacy` — v1 swap / ERC-20 / NFT swap v2 / QTUM delegation. Their ABI JSON
//!   is parseable by vendored ethabi 6.1, so a real pre-migration baseline is
//!   captured and frozen; it MUST stay identical after the upgrade.
//! * `v2` — maker/taker swap v2. Their ABI JSON contains Solidity custom-`error`
//!   entries that ethabi 6.1 CANNOT parse (it panics on `Contract::load`), so no
//!   6.1 baseline exists. This group is `#[ignore]`d until the ethabi 17 upgrade,
//!   then un-ignored and frozen against a spec-correct baseline (selectors are
//!   independently cross-checked in `v2_selectors_match_signatures`).
//!
//! If a frozen vector fails after a dependency change, that is a real on-the-wire
//! divergence to investigate — do NOT edit the `EXPECTED` value to make it pass.

use super::eth_swap_v2::nft_swap_v2::{encode_maker_payment, encode_refund_secret, encode_refund_timelock,
                                      encode_spend_maker_payment, NftKind, NftMakerPaymentArgs, NftRefundSecretArgs,
                                      NftRefundTimelockArgs, NftSpendMakerPaymentArgs};
use super::eth_types::{ERC20_CONTRACT, MAKER_SWAP_V2, SWAP_CONTRACT, TAKER_SWAP_V2};
use ethabi::{Contract, Token};
use ethereum_types::{Address, U256};

/// QTUM delegation ABI, inlined verbatim from `utxo::qtum_delegation` so this
/// module (in the `eth` tree) can exercise the same encoder without adding a
/// cross-module `pub` just for tests.
const QTUM_DELEGATE_CONTRACT_ABI: &str = r#"[{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"_staker","type":"address"},{"indexed":true,"internalType":"address","name":"_delegate","type":"address"},{"indexed":false,"internalType":"uint8","name":"fee","type":"uint8"},{"indexed":false,"internalType":"uint256","name":"blockHeight","type":"uint256"},{"indexed":false,"internalType":"bytes","name":"PoD","type":"bytes"}],"name":"AddDelegation","type":"event"},{"anonymous":false,"inputs":[{"indexed":true,"internalType":"address","name":"_staker","type":"address"},{"indexed":true,"internalType":"address","name":"_delegate","type":"address"}],"name":"RemoveDelegation","type":"event"},{"constant":false,"inputs":[{"internalType":"address","name":"_staker","type":"address"},{"internalType":"uint8","name":"_fee","type":"uint8"},{"internalType":"bytes","name":"_PoD","type":"bytes"}],"name":"addDelegation","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"},{"constant":true,"inputs":[{"internalType":"address","name":"","type":"address"}],"name":"delegations","outputs":[{"internalType":"address","name":"staker","type":"address"},{"internalType":"uint8","name":"fee","type":"uint8"},{"internalType":"uint256","name":"blockHeight","type":"uint256"},{"internalType":"bytes","name":"PoD","type":"bytes"}],"payable":false,"stateMutability":"view","type":"function"},{"constant":false,"inputs":[],"name":"removeDelegation","outputs":[],"payable":false,"stateMutability":"nonpayable","type":"function"}]"#;

struct Fixtures {
    addr1: Address,
    addr2: Address,
    b32a: Vec<u8>,
    b32b: Vec<u8>,
    b32c: Vec<u8>,
    b20: Vec<u8>,
    amount: U256,
    amount2: U256,
    lock64: U256,
    lock32: U256,
    lock32b: U256,
    token_id: U256,
    fee: U256,
    pod: Vec<u8>,
}

/// Fixed, arbitrary-but-deterministic inputs used to build every vector.
fn fixtures() -> Fixtures {
    Fixtures {
        addr1: Address::from([0x11u8; 20]),
        addr2: Address::from([0x22u8; 20]),
        b32a: vec![0x33u8; 32],
        b32b: vec![0x44u8; 32],
        b32c: vec![0x55u8; 32],
        b20: vec![0x66u8; 20],
        amount: U256::from(123_456_789u64),
        amount2: U256::from(987_654_321u64),
        lock64: U256::from(1_700_000_000u64),
        lock32: U256::from(1_800_000_000u64),
        lock32b: U256::from(1_900_000_000u64),
        token_id: U256::from(42u64),
        fee: U256::from(7u64),
        pod: vec![0x77u8; 40],
    }
}

fn enc(contract: &Contract, name: &str, tokens: &[Token]) -> String {
    let f = contract
        .function(name)
        .unwrap_or_else(|e| panic!("function {name}: {e:?}"));
    let data = f
        .encode_input(tokens)
        .unwrap_or_else(|e| panic!("encode_input {name}: {e:?}"));
    hex::encode(data)
}

/// Legacy set — parseable by ethabi 6.1; real pre-migration baseline.
fn legacy_vectors() -> Vec<(&'static str, String)> {
    let f = fixtures();
    let qtum = Contract::load(QTUM_DELEGATE_CONTRACT_ABI.as_bytes()).unwrap();
    let mut v: Vec<(&'static str, String)> = Vec::new();

    // --- v1 swap contract ---
    v.push((
        "v1_ethPayment",
        enc(&SWAP_CONTRACT, "ethPayment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Address(f.addr1),
            Token::FixedBytes(f.b20.clone()),
            Token::Uint(f.lock64),
        ]),
    ));
    v.push((
        "v1_erc20Payment",
        enc(&SWAP_CONTRACT, "erc20Payment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b20.clone()),
            Token::Uint(f.lock64),
        ]),
    ));
    v.push((
        "v1_receiverSpend",
        enc(&SWAP_CONTRACT, "receiverSpend", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::FixedBytes(f.b32b.clone()),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
        ]),
    ));
    v.push((
        "v1_senderRefund",
        enc(&SWAP_CONTRACT, "senderRefund", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::FixedBytes(f.b20.clone()),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
        ]),
    ));

    // --- ERC-20 ---
    v.push((
        "erc20_approve",
        enc(&ERC20_CONTRACT, "approve", &[
            Token::Address(f.addr1),
            Token::Uint(f.amount),
        ]),
    ));
    v.push((
        "erc20_transfer",
        enc(&ERC20_CONTRACT, "transfer", &[
            Token::Address(f.addr1),
            Token::Uint(f.amount),
        ]),
    ));
    v.push((
        "erc20_transferFrom",
        enc(&ERC20_CONTRACT, "transferFrom", &[
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::Uint(f.amount),
        ]),
    ));
    v.push((
        "erc20_balanceOf",
        enc(&ERC20_CONTRACT, "balanceOf", &[Token::Address(f.addr1)]),
    ));

    // --- NFT swap v2 (production standalone helpers; ABI has no custom errors) ---
    let payment = |kind, amount| NftMakerPaymentArgs {
        kind,
        swap_id: [0x33u8; 32],
        amount,
        taker: f.addr1,
        taker_secret_hash: [0x44u8; 32],
        maker_secret_hash: [0x55u8; 32],
        payment_time_lock: 1_700_000_000,
        token_address: f.addr2,
        token_id: f.token_id,
    };
    v.push((
        "nft_erc721_makerPayment",
        hex::encode(encode_maker_payment(&payment(NftKind::Erc721, None)).unwrap()),
    ));
    v.push((
        "nft_erc1155_makerPayment",
        hex::encode(encode_maker_payment(&payment(NftKind::Erc1155, Some(U256::from(5u64)))).unwrap()),
    ));
    let spend = |kind, amount| NftSpendMakerPaymentArgs {
        kind,
        swap_id: [0x33u8; 32],
        amount,
        maker: f.addr1,
        taker_secret_hash: [0x44u8; 32],
        maker_secret: [0x55u8; 32],
        token_address: f.addr2,
        token_id: f.token_id,
    };
    v.push((
        "nft_erc721_spendMakerPayment",
        hex::encode(encode_spend_maker_payment(&spend(NftKind::Erc721, None)).unwrap()),
    ));
    v.push((
        "nft_erc1155_spendMakerPayment",
        hex::encode(encode_spend_maker_payment(&spend(NftKind::Erc1155, Some(U256::from(5u64)))).unwrap()),
    ));
    let refund_tl = |kind, amount| NftRefundTimelockArgs {
        kind,
        swap_id: [0x33u8; 32],
        amount,
        taker: f.addr1,
        taker_secret_hash: [0x44u8; 32],
        maker_secret_hash: [0x55u8; 32],
        token_address: f.addr2,
        token_id: f.token_id,
        payment_time_lock: 1_700_000_000,
    };
    v.push((
        "nft_erc721_refundTimelock",
        hex::encode(encode_refund_timelock(&refund_tl(NftKind::Erc721, None)).unwrap()),
    ));
    v.push((
        "nft_erc1155_refundTimelock",
        hex::encode(encode_refund_timelock(&refund_tl(NftKind::Erc1155, Some(U256::from(5u64)))).unwrap()),
    ));
    let refund_sec = |kind, amount| NftRefundSecretArgs {
        kind,
        swap_id: [0x33u8; 32],
        amount,
        taker: f.addr1,
        taker_secret: [0x44u8; 32],
        maker_secret_hash: [0x55u8; 32],
        token_address: f.addr2,
        token_id: f.token_id,
        payment_time_lock: 1_700_000_000,
    };
    v.push((
        "nft_erc721_refundSecret",
        hex::encode(encode_refund_secret(&refund_sec(NftKind::Erc721, None)).unwrap()),
    ));
    v.push((
        "nft_erc1155_refundSecret",
        hex::encode(encode_refund_secret(&refund_sec(NftKind::Erc1155, Some(U256::from(5u64)))).unwrap()),
    ));

    // --- QTUM delegation ---
    v.push((
        "qtum_addDelegation",
        enc(&qtum, "addDelegation", &[
            Token::Address(f.addr1),
            Token::Uint(f.fee),
            Token::Bytes(f.pod.clone()),
        ]),
    ));
    v.push(("qtum_removeDelegation", enc(&qtum, "removeDelegation", &[])));

    v
}

/// v2 set — needs an ABI parser that understands Solidity custom errors
/// (ethabi >= ~14 / alloy). Panics under vendored ethabi 6.1.
fn v2_vectors() -> Vec<(&'static str, String)> {
    let f = fixtures();
    let mut v: Vec<(&'static str, String)> = Vec::new();

    v.push((
        "makerV2_erc20MakerPayment",
        enc(&MAKER_SWAP_V2, "erc20MakerPayment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
        ]),
    ));
    v.push((
        "makerV2_ethMakerPayment",
        enc(&MAKER_SWAP_V2, "ethMakerPayment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Address(f.addr1),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
        ]),
    ));
    for (label, name) in [
        ("makerV2_refundMakerPaymentSecret", "refundMakerPaymentSecret"),
        ("makerV2_refundMakerPaymentTimelock", "refundMakerPaymentTimelock"),
        ("makerV2_spendMakerPayment", "spendMakerPayment"),
    ] {
        v.push((
            label,
            enc(&MAKER_SWAP_V2, name, &[
                Token::FixedBytes(f.b32a.clone()),
                Token::Uint(f.amount),
                Token::Address(f.addr1),
                Token::FixedBytes(f.b32b.clone()),
                Token::FixedBytes(f.b32c.clone()),
                Token::Address(f.addr2),
            ]),
        ));
    }

    v.push((
        "takerV2_erc20TakerPayment",
        enc(&TAKER_SWAP_V2, "erc20TakerPayment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Uint(f.amount2),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
            Token::Uint(f.lock32b),
        ]),
    ));
    v.push((
        "takerV2_ethTakerPayment",
        enc(&TAKER_SWAP_V2, "ethTakerPayment", &[
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Address(f.addr1),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
            Token::Uint(f.lock32b),
        ]),
    ));
    for (label, name) in [
        ("takerV2_refundTakerPaymentSecret", "refundTakerPaymentSecret"),
        ("takerV2_refundTakerPaymentTimelock", "refundTakerPaymentTimelock"),
        ("takerV2_spendTakerPayment", "spendTakerPayment"),
        ("takerV2_takerPaymentApprove", "takerPaymentApprove"),
    ] {
        v.push((
            label,
            enc(&TAKER_SWAP_V2, name, &[
                Token::FixedBytes(f.b32a.clone()),
                Token::Uint(f.amount),
                Token::Uint(f.amount2),
                Token::Address(f.addr1),
                Token::FixedBytes(f.b32b.clone()),
                Token::FixedBytes(f.b32c.clone()),
                Token::Address(f.addr2),
            ]),
        ));
    }

    v
}

fn check(vectors: &[(&'static str, String)], expected: &[(&str, &str)], group: &str) {
    println!("---- ABI golden vectors [{group}] (paste into EXPECTED) ----");
    for (label, hex) in vectors {
        println!("    (\"{label}\", \"{hex}\"),");
    }
    println!("---- end [{group}] ({} vectors) ----", vectors.len());

    if expected.is_empty() {
        panic!("EXPECTED[{group}] baseline is empty — capture the printed vectors and freeze them");
    }
    let exp: std::collections::HashMap<&str, &str> = expected.iter().copied().collect();
    assert_eq!(
        exp.len(),
        vectors.len(),
        "[{group}] vector count changed: EXPECTED {}, computed {}",
        exp.len(),
        vectors.len()
    );
    for (label, got) in vectors {
        match exp.get(label) {
            Some(want) => assert_eq!(got, want, "[{group}] ABI calldata wire mismatch for `{label}`"),
            None => panic!("[{group}] no frozen baseline for `{label}`"),
        }
    }
}

/// Encode -> decode round-trip guard. ethabi 17's `decode_input` takes the
/// parameter bytes WITHOUT the 4-byte selector; the vendored ethabi 6.1 fork
/// stripped it internally. Every production decode site therefore slices
/// `&calldata[4..]`. A regression here (decoding the full calldata) shifts every
/// field by 4 bytes and would make swap-payment validation misread on-chain
/// calldata — this test locks the correct behavior in.
#[test]
fn abi_decode_roundtrip_strips_selector() {
    let f = fixtures();
    let cases: Vec<(&Contract, &str, Vec<Token>)> = vec![
        (&SWAP_CONTRACT, "erc20Payment", vec![
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b20.clone()),
            Token::Uint(f.lock64),
        ]),
        (&SWAP_CONTRACT, "receiverSpend", vec![
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::FixedBytes(f.b32b.clone()),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
        ]),
        (&ERC20_CONTRACT, "approve", vec![
            Token::Address(f.addr1),
            Token::Uint(f.amount),
        ]),
        (&MAKER_SWAP_V2, "erc20MakerPayment", vec![
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
        ]),
        (&TAKER_SWAP_V2, "erc20TakerPayment", vec![
            Token::FixedBytes(f.b32a.clone()),
            Token::Uint(f.amount),
            Token::Uint(f.amount2),
            Token::Address(f.addr1),
            Token::Address(f.addr2),
            Token::FixedBytes(f.b32b.clone()),
            Token::FixedBytes(f.b32c.clone()),
            Token::Uint(f.lock32),
            Token::Uint(f.lock32b),
        ]),
    ];
    for (contract, name, tokens) in cases {
        let func = contract.function(name).unwrap();
        let calldata = func.encode_input(&tokens).unwrap();
        let decoded = func
            .decode_input(&calldata[4..])
            .unwrap_or_else(|e| panic!("decode_input {name}: {e:?}"));
        assert_eq!(decoded, tokens, "decode round-trip mismatch for `{name}`");
    }
}

/// Frozen baseline captured from vendored ethabi 6.1.0. MUST stay byte-identical
/// across the ethabi/ethereum-types upgrade.
const LEGACY_EXPECTED: &[(&str, &str)] = &[
    ("v1_ethPayment", "152cf3af333333333333333333333333333333333333333333333333333333333333333300000000000000000000000011111111111111111111111111111111111111116666666666666666666666666666666666666666000000000000000000000000000000000000000000000000000000000000000000000000000000006553f100"),
    ("v1_erc20Payment", "9b415b2a333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000111111111111111111111111111111111111111100000000000000000000000022222222222222222222222222222222222222226666666666666666666666666666666666666666000000000000000000000000000000000000000000000000000000000000000000000000000000006553f100"),
    ("v1_receiverSpend", "02ed292b333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15444444444444444444444444444444444444444444444444444444444444444400000000000000000000000011111111111111111111111111111111111111110000000000000000000000002222222222222222222222222222222222222222"),
    ("v1_senderRefund", "46fc0294333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15666666666666666666666666666666666666666600000000000000000000000000000000000000000000000011111111111111111111111111111111111111110000000000000000000000002222222222222222222222222222222222222222"),
    ("erc20_approve", "095ea7b3000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000075bcd15"),
    ("erc20_transfer", "a9059cbb000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000075bcd15"),
    ("erc20_transferFrom", "23b872dd0000000000000000000000001111111111111111111111111111111111111111000000000000000000000000222222222222222222222222222222222222222200000000000000000000000000000000000000000000000000000000075bcd15"),
    ("erc20_balanceOf", "70a082310000000000000000000000001111111111111111111111111111111111111111"),
    ("nft_erc721_makerPayment", "a72539ed3333333333333333333333333333333333333333333333333333333333333333000000000000000000000000111111111111111111111111111111111111111144444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006553f1000000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a"),
    ("nft_erc1155_makerPayment", "631fadb433333333333333333333333333333333333333333333333333333333333333330000000000000000000000000000000000000000000000000000000000000005000000000000000000000000111111111111111111111111111111111111111144444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006553f1000000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a"),
    ("nft_erc721_spendMakerPayment", "c8d9009b33333333333333333333333333333333333333333333333333333333333333330000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a"),
    ("nft_erc1155_spendMakerPayment", "fc45ef82333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000000000050000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a"),
    ("nft_erc721_refundTimelock", "a24bcbe533333333333333333333333333333333333333333333333333333333333333330000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000000000000000000000000000000000006553f100"),
    ("nft_erc1155_refundTimelock", "f1617c1f333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000000000050000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000000000000000000000000000000000006553f100"),
    ("nft_erc721_refundSecret", "4666d80533333333333333333333333333333333333333333333333333333333333333330000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000000000000000000000000000000000006553f100"),
    ("nft_erc1155_refundSecret", "c8900740333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000000000050000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000000000000000000000000000000000006553f100"),
    ("qtum_addDelegation", "4c0e968c000000000000000000000000111111111111111111111111111111111111111100000000000000000000000000000000000000000000000000000000000000070000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002877777777777777777777777777777777777777777777777777777777777777777777777777777777000000000000000000000000000000000000000000000000"),
    ("qtum_removeDelegation", "3d666e8b"),
];

/// Frozen baseline for maker/taker swap v2. Captured under ethabi 17 (6.1 cannot
/// parse these ABIs). Populated when the v2 test is un-ignored post-migration.
const V2_EXPECTED: &[(&str, &str)] = &[
    ("makerV2_erc20MakerPayment", "a53bc126333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd150000000000000000000000001111111111111111111111111111111111111111000000000000000000000000222222222222222222222222222222222222222244444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006b49d200"),
    ("makerV2_ethMakerPayment", "7466be603333333333333333333333333333333333333333333333333333333333333333000000000000000000000000111111111111111111111111111111111111111144444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006b49d200"),
    ("makerV2_refundMakerPaymentSecret", "74a4788a333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd150000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("makerV2_refundMakerPaymentTimelock", "9b949dee333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd150000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("makerV2_spendMakerPayment", "1299a27a333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd150000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("takerV2_erc20TakerPayment", "d6a71eb4333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000000000000000000000000000000000003ade68b10000000000000000000000001111111111111111111111111111111111111111000000000000000000000000222222222222222222222222222222222222222244444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006b49d20000000000000000000000000000000000000000000000000000000000713fb300"),
    ("takerV2_ethTakerPayment", "9b4603f2333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000111111111111111111111111111111111111111144444444444444444444444444444444444444444444444444444444444444445555555555555555555555555555555555555555555555555555555555555555000000000000000000000000000000000000000000000000000000006b49d20000000000000000000000000000000000000000000000000000000000713fb300"),
    ("takerV2_refundTakerPaymentSecret", "3e6af5f2333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000000000000000000000000000000000003ade68b10000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("takerV2_refundTakerPaymentTimelock", "65e26617333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000000000000000000000000000000000003ade68b10000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("takerV2_spendTakerPayment", "cc90c199333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000000000000000000000000000000000003ade68b10000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
    ("takerV2_takerPaymentApprove", "146e5b24333333333333333333333333333333333333333333333333333333333333333300000000000000000000000000000000000000000000000000000000075bcd15000000000000000000000000000000000000000000000000000000003ade68b10000000000000000000000001111111111111111111111111111111111111111444444444444444444444444444444444444444444444444444444444444444455555555555555555555555555555555555555555555555555555555555555550000000000000000000000002222222222222222222222222222222222222222"),
];

#[test]
fn abi_wire_golden_legacy() { check(&legacy_vectors(), LEGACY_EXPECTED, "legacy"); }

#[test]
fn abi_wire_golden_v2() { check(&v2_vectors(), V2_EXPECTED, "v2"); }
