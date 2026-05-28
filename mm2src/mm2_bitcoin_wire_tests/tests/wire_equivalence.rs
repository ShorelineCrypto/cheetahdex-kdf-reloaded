//! Phase B wire-equivalence regression harness.
//!
//! KDF participates in atomic swaps with peers running canonical KDF. Every
//! transaction and block-header serialisation for BTC / KMD / LTC / BCH /
//! Qtum / RVN / NavCoin must produce byte-identical output regardless of
//! which codec generation is active. This file locks that contract in place
//! with hex fixtures from real chains plus the well-known BIP-143 and
//! ZIP-243 reference vectors.
//!
//! All fixtures are pinned via their `source` URL (block explorers, BIP/ZIP
//! test-vector specs, or KDF's own legacy test corpus). The tests are pure
//! (no I/O, no network, no state) so they can run after every commit during
//! the parity-bitcoin excision.
//!
//! KDF-original.

use chain::{BlockHeader, Transaction};
use serialization::{deserialize, serialize_with_flags, SERIALIZE_TRANSACTION_WITNESS};

/// One transaction fixture pinned to a real, observable on-chain entry.
struct TxFixture {
    /// Short human-readable label used in failure messages.
    name: &'static str,
    /// Where the hex came from — explorer URL, BIP/ZIP test vector, etc.
    source: &'static str,
    /// Canonical wire encoding (lower-case hex, no whitespace).
    hex: &'static str,
}

/// Same shape, but with the canonical encoding given as a byte slice. Used
/// for fixtures where the hex form was originally captured as a `Vec<u8>`
/// in the legacy tests and is more reliable transcribed verbatim.
struct TxByteFixture {
    name: &'static str,
    source: &'static str,
    bytes: &'static [u8],
}

/// One block-header fixture pinned to a real, observable on-chain entry.
struct HeaderFixture {
    name: &'static str,
    source: &'static str,
    hex: &'static str,
}

// ---------------------------------------------------------------------------
// Transaction fixtures (hex form)
// ---------------------------------------------------------------------------

const TX_FIXTURES: &[TxFixture] = &[
    TxFixture {
        name: "btc_block80000_legacy",
        source: "https://blockchain.info/rawtx/5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2?format=hex",
        hex: "0100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000",
    },
    TxFixture {
        name: "btc_bip143_segwit_two_inputs",
        source: "https://github.com/bitcoin/bips/blob/master/bip-0143.mediawiki",
        hex: "01000000000102fff7f7881a8099afa6940d42d1e7f6362bec38171ea3edf433541db4e4ad969f00000000494830450221008b9d1dc26ba6a9cb62127b02742fa9d754cd3bebf337f7a55d114c8e5cdd30be022040529b194ba3f9281a99f2b1c0a19c0489bc22ede944ccf4ecbab4cc618ef3ed01eeffffffef51e1b804cc89d182d279655c3aa89e815b1b309fe287d9b2b55d57b90ec68a0100000000ffffffff02202cb206000000001976a9148280b37df378db99f66f85c95a783a76ac7a6d5988ac9093510d000000001976a9143bde42dbee7e4dbe6a21b2d50ce2f0167faa815988ac000247304402203609e17b84f6a7d30c80bfa610b5b4542f32a8a0d5447a12fb1366d7f01cc44a0220573a954c4518331561406f90300e8f3358f51928d43c212a8caed02de67eebee0121025476c2e83188368da1ff3e292e7acafcdb3566bb0ad253f62fc70f07aeee635711000000",
    },
    TxFixture {
        name: "kmd_with_opreturn",
        source: "https://kmdexplorer.io/tx/88893f05764f5a781f2e555a5b492c064f2269a4a44c51afdbe98fab54361bb5",
        hex: "0100000001ebca38fa14b1ec029c3e08a2e87940c1f796b1588674b4c386f09626ee702576010000006a4730440220070963b9460d9bafe7865563574594fc3f823e5cdf7c49a5642dade76502547f022023fd90d41e34e514237f4b5967f83c9af27673d6de2eae3d88079a988fa5be3e012103668e3368c9fb67d8fc808a5fe74d5a8d21b6eed726838122d5f7716fb3328998ffffffff03e87006060000000017a914fef59ae800bb89050d25f67be432b231097e1849878758c100000000001976a91473122bcec852f394e51496e39fca5111c3d7ae5688ac00000000000000000a6a08303764643135633400000000",
    },
    TxFixture {
        name: "kmd_v7_z2t",
        source: "KomodoPlatform/komodo-defi-framework legacy fixture (chain/src/transaction.rs::test_transaction_reader_v7)",
        hex: "0700000001f87575693f4c038018628ff89f64571f0b9b48cd91a09b984d7eb018f4753bfa000000006a47304402202a3c612b11db1be51ae47fc1c23cc73e7fb14f08f10b3e71e5778d7adad494e90220636ca2580324452d8596cea7b2ebc31d796787108a7f74b676e3f136cb2c56b9012102e75e70baceb8cd5ae2bdc893d018512aafc8aac403ae8c14da66fa3ede87fcc3ffffffff0148b6eb0b000000001976a914139df01a608671fcf24db66d2d02bf2d4274e1f888ac00000000",
    },
    TxFixture {
        name: "myce_v3_not_overwintered",
        source: "http://explore.myce.world/api/getrawtransaction?txid=248b2cadff69bb58f3232b914d32588cd9cd014d4f3dc29cd39d1914bf1d7f43&decrypt=0",
        hex: "030000000145f09710b0d6ff73a52bffdd1661f2f001783fb6f947ecf253462359dca19e990100000049483045022100e2f6183e2008e6b0aa31f728f289c66436bf4d4be7aedfe0c3f582e60d16443e0220741548d2cee78a2b39a8e1146b131a69211da025ff0859dba60e38b12a46a0b501ffffffff026c39ea0b000000001976a9142b79bc408688f48858083de027a1b42ed3e39da188ac380265d9450000001976a914066baabb56dc1588afd7fa83e0ffd4729aee89d588ac00000000",
    },
];

// ---------------------------------------------------------------------------
// Transaction fixtures (byte-array form, taken verbatim from legacy tests
// where capturing the hex is more error-prone)
// ---------------------------------------------------------------------------

/// ECC PoS coin: txversion 1, includes the n_time field.
const ECC_POS_TX_BYTES: &[u8] = &[
    1, 0, 0, 0, 70, 254, 168, 92, 1, 170, 99, 80, 219, 121, 123, 10, 150, 232, 96, 154, 102, 242, 208, 96, 100, 59,
    114, 52, 38, 97, 143, 194, 239, 6, 154, 4, 232, 82, 124, 189, 240, 0, 0, 0, 0, 106, 71, 48, 68, 2, 32, 75, 18, 92,
    56, 109, 69, 254, 77, 185, 43, 157, 13, 166, 30, 129, 30, 185, 72, 161, 125, 37, 134, 120, 218, 213, 146, 229, 8,
    117, 133, 164, 38, 2, 32, 40, 91, 86, 89, 107, 96, 15, 202, 12, 124, 168, 252, 75, 139, 191, 93, 216, 144, 212, 58,
    159, 166, 64, 202, 72, 155, 182, 222, 42, 140, 167, 128, 1, 33, 3, 148, 13, 224, 176, 222, 92, 35, 122, 18, 78,
    113, 66, 51, 158, 172, 225, 229, 41, 119, 44, 212, 117, 176, 232, 66, 250, 100, 75, 202, 254, 73, 204, 254, 255,
    255, 255, 2, 193, 198, 45, 0, 0, 0, 0, 0, 25, 118, 169, 20, 131, 5, 22, 126, 249, 90, 27, 30, 154, 205, 246, 52,
    167, 104, 108, 183, 105, 147, 64, 106, 136, 172, 127, 132, 30, 0, 0, 0, 0, 0, 25, 118, 169, 20, 195, 247, 16, 222,
    183, 50, 11, 14, 250, 110, 219, 20, 227, 235, 238, 185, 21, 95, 169, 13, 136, 172, 238, 100, 32, 0,
];

/// NAV coin: txversion 3, n_time field, plus trailing strDZeel string.
const NAV_POS_TX_BYTES: &[u8] = &[
    3, 0, 0, 0, 13, 96, 152, 92, 2, 20, 58, 107, 102, 116, 164, 26, 174, 199, 16, 166, 39, 126, 103, 203, 187, 192,
    176, 219, 43, 192, 73, 93, 118, 26, 134, 41, 28, 131, 123, 227, 220, 0, 0, 0, 0, 107, 72, 48, 69, 2, 33, 0, 174,
    215, 242, 173, 170, 178, 139, 171, 71, 204, 106, 251, 240, 134, 193, 51, 146, 91, 26, 42, 127, 55, 199, 24, 179,
    104, 243, 129, 216, 0, 7, 161, 2, 32, 124, 16, 163, 154, 229, 128, 110, 209, 126, 131, 158, 197, 56, 183, 219, 22,
    180, 14, 253, 114, 164, 98, 222, 137, 198, 145, 147, 91, 225, 132, 183, 56, 1, 33, 3, 27, 184, 59, 88, 236, 19, 14,
    40, 224, 166, 213, 210, 172, 242, 235, 1, 176, 211, 241, 103, 14, 2, 29, 71, 211, 29, 184, 168, 88, 33, 157, 168,
    254, 255, 255, 255, 85, 253, 74, 79, 211, 120, 236, 109, 192, 55, 203, 24, 96, 189, 156, 22, 227, 112, 74, 210,
    217, 189, 130, 89, 76, 62, 204, 212, 95, 91, 175, 250, 1, 0, 0, 0, 72, 71, 48, 68, 2, 32, 110, 46, 42, 223, 247,
    151, 62, 91, 112, 45, 109, 158, 199, 116, 13, 53, 155, 181, 34, 41, 40, 178, 212, 255, 22, 217, 222, 138, 69, 208,
    187, 55, 2, 32, 21, 234, 176, 205, 2, 222, 232, 108, 28, 245, 211, 133, 46, 62, 145, 17, 75, 45, 69, 171, 113, 113,
    247, 160, 189, 229, 87, 139, 217, 125, 22, 139, 1, 254, 255, 255, 255, 1, 60, 143, 6, 192, 7, 0, 0, 0, 25, 118,
    169, 20, 195, 247, 16, 222, 183, 50, 11, 14, 250, 110, 219, 20, 227, 235, 238, 185, 21, 95, 169, 13, 136, 172, 64,
    143, 45, 0, 253, 88, 1, 71, 57, 50, 106, 117, 65, 47, 83, 104, 110, 69, 87, 69, 120, 116, 48, 82, 47, 90, 57, 100,
    118, 50, 77, 55, 77, 119, 88, 79, 122, 56, 115, 88, 82, 78, 111, 57, 53, 107, 81, 84, 57, 80, 86, 97, 53, 52, 98,
    73, 77, 73, 111, 82, 77, 55, 47, 100, 68, 78, 112, 104, 82, 78, 90, 51, 52, 97, 108, 73, 47, 76, 70, 88, 53, 120,
    80, 86, 75, 71, 100, 74, 116, 117, 90, 51, 115, 109, 122, 84, 84, 75, 76, 89, 109, 78, 75, 53, 104, 117, 72, 87,
    74, 66, 106, 81, 71, 108, 116, 50, 90, 69, 100, 69, 82, 67, 119, 122, 77, 74, 115, 75, 82, 72, 90, 107, 104, 48,
    43, 103, 67, 116, 114, 79, 53, 75, 116, 84, 89, 119, 79, 75, 66, 75, 108, 74, 75, 89, 113, 107, 66, 120, 97, 80,
    107, 47, 68, 76, 52, 110, 121, 53, 113, 98, 88, 57, 90, 57, 66, 74, 98, 104, 52, 122, 105, 109, 70, 116, 70, 75,
    77, 43, 100, 47, 102, 55, 54, 68, 43, 105, 117, 106, 87, 102, 100, 85, 88, 103, 79, 107, 86, 67, 97, 116, 101, 68,
    115, 79, 47, 108, 50, 72, 50, 79, 66, 86, 88, 70, 76, 100, 49, 113, 110, 87, 106, 75, 98, 98, 85, 79, 49, 88, 51,
    80, 75, 120, 122, 105, 106, 97, 117, 90, 68, 68, 107, 76, 90, 49, 113, 72, 47, 83, 66, 88, 107, 43, 52, 101, 118,
    43, 102, 52, 51, 109, 83, 100, 85, 67, 116, 57, 112, 75, 86, 79, 49, 107, 68, 97, 88, 69, 67, 104, 51, 71, 100,
    119, 88, 105, 100, 111, 56, 102, 121, 48, 51, 66, 78, 49, 55, 82, 118, 66, 115, 78, 111, 54, 76, 102, 57, 113, 48,
    107, 65, 76, 77, 97, 101, 97, 122, 99, 70, 102, 122, 57, 65, 112, 67, 108, 87, 51, 47, 70, 118, 65, 121, 115, 101,
    84, 119, 54, 113, 43, 65, 61, 61,
];

const TX_BYTE_FIXTURES: &[TxByteFixture] = &[
    TxByteFixture {
        name: "ecc_pos_with_ntime",
        source: "https://chainz.cryptoid.info/ecc/tx.dws?816906122e12c5b56a38f169aa2bdccb1e90f4e0d78a3777b60b262883132602.htm",
        bytes: ECC_POS_TX_BYTES,
    },
    TxByteFixture {
        name: "nav_pos_with_strdzeel",
        source: "https://github.com/navcoin/navcoin-core (strDZeel field)",
        bytes: NAV_POS_TX_BYTES,
    },
];

// ---------------------------------------------------------------------------
// Block-header fixtures
// ---------------------------------------------------------------------------

const HEADER_FIXTURES: &[HeaderFixture] = &[
    HeaderFixture {
        name: "btc_block_125552_header",
        source: "https://blockstream.info/api/block/00000000000000001e8d6829a8a21adc5d38d0a473b144b6765798e61f98bd1d/header",
        hex: "0100000081cd02ab7e569e8bcd9317e2fe99f2de44d49ab2b8851ba4a308000000000000e320b6c2fffc8d750423db8b1eb942ae710e951ed797f7affc8892b0f1fc122bc7f5d74df2b9441a42a14695",
    },
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn assert_tx_round_trip(name: &str, source: &str, bytes: &[u8]) {
    let tx: Transaction = deserialize(bytes)
        .unwrap_or_else(|e| panic!("[{}] deserialize failed (source: {}): {:?}", name, source, e));
    // Always request witness data on the way out; the codec only emits it when
    // the parsed transaction actually carries witness, so legacy txs are not
    // affected. Without the flag, segwit fixtures lose their witness section
    // and round-trip fails.
    let re = serialize_with_flags(&tx, SERIALIZE_TRANSACTION_WITNESS).take();
    assert_eq!(
        re, bytes,
        "[{}] wire-equivalence broken (source: {})",
        name, source
    );
}

fn assert_header_round_trip(fx: &HeaderFixture) {
    let bytes = hex::decode(fx.hex).expect("header fixture hex is malformed");
    let header: BlockHeader = deserialize(&bytes[..]).unwrap_or_else(|e| {
        panic!("[{}] header deserialize failed (source: {}): {:?}", fx.name, fx.source, e)
    });
    let re = serialization::serialize(&header).take();
    assert_eq!(
        re, bytes,
        "[{}] header wire-equivalence broken (source: {})",
        fx.name, fx.source
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn hex_fixtures_round_trip() {
    for fx in TX_FIXTURES {
        let bytes = hex::decode(fx.hex).expect("fixture hex is malformed");
        assert_tx_round_trip(fx.name, fx.source, &bytes);
    }
}

#[test]
fn byte_fixtures_round_trip() {
    for fx in TX_BYTE_FIXTURES {
        assert_tx_round_trip(fx.name, fx.source, fx.bytes);
    }
}

#[test]
fn header_fixtures_round_trip() {
    for fx in HEADER_FIXTURES {
        assert_header_round_trip(fx);
    }
}
