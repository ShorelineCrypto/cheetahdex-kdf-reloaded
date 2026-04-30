//! TP1 — Mock RPC Response Deserialization Tests
//!
//! Canned JSON response fixtures for verifying that our response types
//! correctly deserialize real-world blockchain RPC responses.
//! No network, no Docker — pure unit tests.

use serde_json as json;

// ═══════════════════════════════════════════════════════════════════════
//  UTXO Native RPC Response Tests
// ═══════════════════════════════════════════════════════════════════════

mod native_rpc_responses {
    use super::*;
    use crate::utxo::rpc_clients::{
        EstimateSmartFeeRes, ListSinceBlockRes, NativeUnspent, NetworkInfo, ValidateAddressRes, VerboseBlock,
    };

    #[test]
    fn test_native_unspent_btc() {
        let json_str = r#"{
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "vout": 1,
            "address": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa",
            "scriptPubKey": "76a91462e907b15cbf27d5425399ebf6f0fb50ebb88f1888ac",
            "amount": 0.001,
            "confirmations": 6,
            "spendable": true
        }"#;
        let unspent: NativeUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.vout, 1);
        assert_eq!(unspent.confirmations, 6);
        assert!(unspent.spendable);
        assert_eq!(unspent.address, "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa");
    }

    #[test]
    fn test_native_unspent_with_optional_account() {
        // Some coin daemons include the "account" field
        let json_str = r#"{
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "vout": 0,
            "address": "RQq6fWoy8aGGMLjvRfMY5mBNVm2RQxJyLa",
            "account": "",
            "scriptPubKey": "76a91483762a373935ca241d557dfce89171d582b486de88ac",
            "amount": 1.0,
            "confirmations": 100,
            "spendable": true
        }"#;
        let unspent: NativeUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.account, Some(String::new()));
    }

    #[test]
    fn test_native_unspent_without_account() {
        let json_str = r#"{
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "vout": 0,
            "address": "RQq6fWoy8aGGMLjvRfMY5mBNVm2RQxJyLa",
            "scriptPubKey": "76a91483762a373935ca241d557dfce89171d582b486de88ac",
            "amount": 1.0,
            "confirmations": 100,
            "spendable": false
        }"#;
        let unspent: NativeUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.account, None);
        assert!(!unspent.spendable);
    }

    #[test]
    fn test_estimate_smart_fee_success() {
        let json_str = r#"{"feerate": 0.00012345, "blocks": 2}"#;
        let res: EstimateSmartFeeRes = json::from_str(json_str).unwrap();
        assert!((res.fee_rate - 0.00012345).abs() < f64::EPSILON);
        assert_eq!(res.blocks, 2);
        assert!(res.errors.is_empty());
    }

    #[test]
    fn test_estimate_smart_fee_with_errors() {
        // Bitcoin Core returns errors when it can't estimate
        let json_str = r#"{"errors": ["Insufficient data or no feerate found"], "blocks": 0}"#;
        let res: EstimateSmartFeeRes = json::from_str(json_str).unwrap();
        assert!((res.fee_rate - 0.0).abs() < f64::EPSILON); // default
        assert_eq!(res.errors.len(), 1);
        assert_eq!(res.blocks, 0);
    }

    #[test]
    fn test_verbose_block_btc() {
        let json_str = r#"{
            "hash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
            "confirmations": 800000,
            "size": 285,
            "height": 0,
            "version": 1,
            "merkleroot": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "tx": ["4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"],
            "time": 1231006505,
            "nonce": 2083236893,
            "bits": "1d00ffff",
            "difficulty": 1.0,
            "chainwork": "0000000000000000000000000000000000000000000000000000000100010001",
            "nextblockhash": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048"
        }"#;
        let block: VerboseBlock = json::from_str(json_str).unwrap();
        assert_eq!(block.confirmations, 800000);
        assert_eq!(block.height, Some(0));
        assert_eq!(block.time, 1231006505);
        assert_eq!(block.tx.len(), 1);
        assert_eq!(block.previousblockhash, None); // genesis has no previous
        assert!(block.nextblockhash.is_some());
    }

    #[test]
    fn test_verbose_block_with_string_nonce() {
        // KMD / Zcash forks use a hex string nonce
        let json_str = r#"{
            "hash": "027e3758c3a65b12aa1046462b486d0a63bfa1beae327897f56c5cfb7daaae71",
            "confirmations": 100,
            "size": 1853,
            "height": 1000000,
            "version": 4,
            "merkleroot": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "tx": ["4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b"],
            "time": 1620000000,
            "mediantime": 1619999000,
            "nonce": "00000000000000000000000000000000000000000000000000000000000007e8",
            "bits": "200e0377",
            "difficulty": 1234.5,
            "chainwork": "0000000000000000000000000000000000000000000000000000000100010001",
            "previousblockhash": "0000000000000000000000000000000000000000000000000000000000000001",
            "finalsaplingroot": "0000000000000000000000000000000000000000000000000000000000000000"
        }"#;
        let block: VerboseBlock = json::from_str(json_str).unwrap();
        assert_eq!(block.height, Some(1000000));
        assert!(block.final_sapling_root.is_some());
        assert!(block.mediantime.is_some());
    }

    #[test]
    fn test_verbose_block_negative_confirmations() {
        // Side-chain blocks can have negative confirmations
        let json_str = r#"{
            "hash": "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f",
            "confirmations": -1,
            "size": 285,
            "height": 100,
            "version": 1,
            "merkleroot": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "tx": [],
            "time": 1231006505,
            "nonce": 2083236893,
            "bits": "1d00ffff",
            "difficulty": 1.0,
            "chainwork": "0000000000000000000000000000000000000000000000000000000100010001"
        }"#;
        let block: VerboseBlock = json::from_str(json_str).unwrap();
        assert_eq!(block.confirmations, -1);
    }

    #[test]
    fn test_validate_address_valid() {
        let json_str = r#"{
            "isvalid": true,
            "address": "RQq6fWoy8aGGMLjvRfMY5mBNVm2RQxJyLa",
            "scriptPubKey": "76a91483762a373935ca241d557dfce89171d582b486de88ac",
            "segid": 45,
            "ismine": true,
            "iswatchonly": false,
            "isscript": false
        }"#;
        let res: ValidateAddressRes = json::from_str(json_str).unwrap();
        assert!(res.is_valid);
        assert!(!res.is_script);
        assert_eq!(res.seg_id, Some(45));
        assert_eq!(res.is_mine, Some(true));
    }

    #[test]
    fn test_validate_address_invalid() {
        // Bitcoin Core response for invalid address
        let json_str = r#"{
            "isvalid": false,
            "address": "",
            "scriptPubKey": "",
            "isscript": false
        }"#;
        let res: ValidateAddressRes = json::from_str(json_str).unwrap();
        assert!(!res.is_valid);
        assert_eq!(res.seg_id, None);
        assert_eq!(res.is_mine, None);
    }

    #[test]
    fn test_network_info_kmd() {
        let json_str = r#"{
            "version": 2001526,
            "subversion": "/MagicBean:2.0.15-beta1/",
            "protocolversion": 170009,
            "localservices": "0000000000000001",
            "timeoffset": -1,
            "connections": 32,
            "networks": [{"name": "ipv4", "limited": false, "reachable": true, "proxy": "", "proxy_randomize_credentials": false}],
            "relayfee": 0.000001,
            "localaddresses": [{"address": "1.2.3.4", "port": 7770, "score": 4}],
            "warnings": ""
        }"#;
        let _info: NetworkInfo = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_network_info_btc() {
        let json_str = r#"{
            "version": 180100,
            "subversion": "/Satoshi:0.18.1/",
            "protocolversion": 70015,
            "localservices": "000000000000040d",
            "localrelay": true,
            "timeoffset": 0,
            "networkactive": true,
            "connections": 10,
            "networks": [
                {"name": "ipv4", "limited": false, "reachable": true, "proxy": "", "proxy_randomize_credentials": true},
                {"name": "ipv6", "limited": false, "reachable": true, "proxy": "", "proxy_randomize_credentials": true},
                {"name": "onion", "limited": true, "reachable": false, "proxy": "", "proxy_randomize_credentials": true}
            ],
            "relayfee": 0.00001000,
            "incrementalfee": 0.00001000,
            "localaddresses": [],
            "warnings": ""
        }"#;
        let _info: NetworkInfo = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_list_since_block_btc() {
        // Real BTC listsinceblock response (abbreviated from existing test)
        let json_str = r#"{
            "lastblock": "000000000000000000066f896cca2a6c667ca85fff28ed6731d64e3c39ecb119",
            "removed": [],
            "transactions": [{
                "abandoned": false,
                "address": "1Q3kQ1jsB2VyH83PJT1NXJqEaEcR6Yuknn",
                "amount": -0.01788867,
                "bip125-replaceable": "no",
                "blockhash": "0000000000000000000db4be4c2df08790e1027326832cc90889554bbebc69b7",
                "blockindex": 437,
                "blocktime": 1572174214,
                "category": "send",
                "confirmations": 197,
                "fee": -0.00012924,
                "involvesWatchonly": true,
                "time": 1572173721,
                "timereceived": 1572173721,
                "txid": "29606e6780c69a39767b56dc758e6af31ced5232491ad62dcf25275684cb7701",
                "vout": 0,
                "walletconflicts": []
            }]
        }"#;
        let _res: ListSinceBlockRes = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_list_since_block_empty_transactions() {
        let json_str = r#"{"lastblock": "0000000000000000000000000000000000000000000000000000000000000000", "transactions": []}"#;
        let _res: ListSinceBlockRes = json::from_str(json_str).unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Electrum Protocol Response Tests
// ═══════════════════════════════════════════════════════════════════════

mod electrum_responses {
    use super::*;
    use crate::utxo::rpc_clients::{
        ElectrumBalance, ElectrumBlockHeader, ElectrumBlockHeaderV12, ElectrumBlockHeaderV14,
        ElectrumBlockHeadersRes, ElectrumTxHistoryItem, ElectrumUnspent, TxMerkleBranch,
    };

    #[test]
    fn test_electrum_unspent_confirmed() {
        let json_str = r#"{
            "height": 650000,
            "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "tx_pos": 1,
            "value": 100000
        }"#;
        let unspent: ElectrumUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.height, Some(650000));
        assert_eq!(unspent.tx_pos, 1);
        assert_eq!(unspent.value, 100000);
    }

    #[test]
    fn test_electrum_unspent_unconfirmed() {
        // Unconfirmed UTXOs have height 0 or null
        let json_str = r#"{
            "height": 0,
            "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "tx_pos": 0,
            "value": 50000
        }"#;
        let unspent: ElectrumUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.height, Some(0));
    }

    #[test]
    fn test_electrum_unspent_null_height() {
        let json_str = r#"{
            "height": null,
            "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "tx_pos": 0,
            "value": 50000
        }"#;
        let unspent: ElectrumUnspent = json::from_str(json_str).unwrap();
        assert_eq!(unspent.height, None);
    }

    #[test]
    fn test_electrum_balance() {
        let json_str = r#"{"confirmed": 1000000, "unconfirmed": 50000}"#;
        let _balance: ElectrumBalance = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_electrum_balance_negative_unconfirmed() {
        // Pending spends can cause negative unconfirmed balance
        let json_str = r#"{"confirmed": 1000000, "unconfirmed": -50000}"#;
        let _balance: ElectrumBalance = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_electrum_tx_history_confirmed() {
        let json_str = r#"{
            "height": 650000,
            "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "fee": 1234
        }"#;
        let item: ElectrumTxHistoryItem = json::from_str(json_str).unwrap();
        assert_eq!(item.height, 650000);
        assert_eq!(item.fee, Some(1234));
    }

    #[test]
    fn test_electrum_tx_history_unconfirmed() {
        // Unconfirmed transactions have height 0, -1, or -2
        let json_str = r#"{
            "height": -1,
            "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0"
        }"#;
        let item: ElectrumTxHistoryItem = json::from_str(json_str).unwrap();
        assert_eq!(item.height, -1);
        assert_eq!(item.fee, None);
    }

    #[test]
    fn test_electrum_block_header_v12() {
        // Electrum protocol v1.2 returns structured header
        let json_str = r#"{
            "bits": 486604799,
            "block_height": 0,
            "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "nonce": 2083236893,
            "prev_block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "timestamp": 1231006505,
            "version": 1
        }"#;
        let header: ElectrumBlockHeaderV12 = json::from_str(json_str).unwrap();
        assert_eq!(header.block_height, 0);
        assert_eq!(header.bits, 486604799);
        assert_eq!(header.timestamp, 1231006505);
    }

    #[test]
    fn test_electrum_block_header_v12_hash_nonce() {
        // Some chains (Zcash forks) have a hash-type nonce
        let json_str = r#"{
            "bits": 520617983,
            "block_height": 100,
            "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "nonce": "00000000000000000000000000000000000000000000000000000000000007e8",
            "prev_block_hash": "0000000000000000000000000000000000000000000000000000000000000001",
            "timestamp": 1620000000,
            "version": 4
        }"#;
        let header: ElectrumBlockHeaderV12 = json::from_str(json_str).unwrap();
        assert_eq!(header.version, 4);
    }

    #[test]
    fn test_electrum_block_header_v14() {
        // Electrum protocol v1.4 returns compact hex-encoded header
        let json_str = r#"{"height": 724609, "hex": "00200020eab6fa183da8f9e4c761b31a67a76fa6a7658eb84c760200000000000000000063cd9585d434ec0db25894ec4b1f03735f10e31709c4395ea67c50c8378f134b972f166278100a17bfd87203"}"#;
        let header: ElectrumBlockHeaderV14 = json::from_str(json_str).unwrap();
        assert_eq!(header.height, 724609);
        assert!(!header.hex.is_empty());
    }

    #[test]
    fn test_electrum_block_header_untagged_v12() {
        // The untagged enum should auto-detect V12 format
        let json_str = r#"{
            "bits": 486604799,
            "block_height": 0,
            "merkle_root": "4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b",
            "nonce": 2083236893,
            "prev_block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "timestamp": 1231006505,
            "version": 1
        }"#;
        let header: ElectrumBlockHeader = json::from_str(json_str).unwrap();
        assert!(matches!(header, ElectrumBlockHeader::V12(_)));
    }

    #[test]
    fn test_electrum_block_header_untagged_v14() {
        let json_str = r#"{"height": 724609, "hex": "00200020eab6fa183da8f9e4c761b31a67a76fa6a7658eb84c760200000000000000000063cd9585d434ec0db25894ec4b1f03735f10e31709c4395ea67c50c8378f134b972f166278100a17bfd87203"}"#;
        let header: ElectrumBlockHeader = json::from_str(json_str).unwrap();
        assert!(matches!(header, ElectrumBlockHeader::V14(_)));
    }

    #[test]
    fn test_tx_merkle_branch() {
        let json_str = r#"{
            "merkle": [
                "73dfb53e6f49854b09d98500d4899d5c4e703c4fa3a2ddadc2cd7f12b72d4182",
                "4274d707b2308d39a04f2940024d382fa80d994152a50d4258f5a7feead2a563"
            ],
            "block_height": 1431628,
            "pos": 1
        }"#;
        let branch: TxMerkleBranch = json::from_str(json_str).unwrap();
        assert_eq!(branch.merkle.len(), 2);
        assert_eq!(branch.block_height, 1431628);
        assert_eq!(branch.pos, 1);
    }

    #[test]
    fn test_tx_merkle_branch_coinbase_only() {
        // Coinbase-only block: empty merkle path, pos=0
        let json_str = r#"{"merkle": [], "block_height": 0, "pos": 0}"#;
        let branch: TxMerkleBranch = json::from_str(json_str).unwrap();
        assert!(branch.merkle.is_empty());
        assert_eq!(branch.pos, 0);
    }

    #[test]
    fn test_electrum_block_headers_res() {
        let json_str = r#"{"count": 3, "hex": "00200020eab6fa183da8f9e4c761b31a67a76fa6a7658eb84c760200000000000000000063cd9585d434ec0db25894ec4b1f03735f10e31709c4395ea67c50c8378f134b972f166278100a17bfd87203", "max": 2016}"#;
        let res: ElectrumBlockHeadersRes = json::from_str(json_str).unwrap();
        assert_eq!(res.count, 3);
        assert!(!res.hex.is_empty());
    }

    #[test]
    fn test_electrum_unspent_list() {
        // Multiple UTXOs in one response
        let json_str = r#"[
            {"height": 650000, "tx_hash": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0", "tx_pos": 0, "value": 100000},
            {"height": 650001, "tx_hash": "b08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf1", "tx_pos": 1, "value": 200000},
            {"height": null, "tx_hash": "c08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf2", "tx_pos": 0, "value": 50000}
        ]"#;
        let unspents: Vec<ElectrumUnspent> = json::from_str(json_str).unwrap();
        assert_eq!(unspents.len(), 3);
        assert_eq!(unspents[0].value, 100000);
        assert_eq!(unspents[2].height, None);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Bitcoin RPC Transaction Type Tests
// ═══════════════════════════════════════════════════════════════════════

mod rpc_transaction_responses {
    use super::*;
    use rpc::v1::types::Transaction as RpcTransaction;

    #[test]
    fn test_verbose_transaction_kmd() {
        // Real KMD getrawtransaction verbose output
        let json_str = r#"{"hex":"0400008085202f8901afcadb73880bc1c9e7ce96b8274c2e2a4547415e649f425f98791685be009b73020000006b483045022100b8fbb77efea482b656ad16fc53c5a01d289054c2e429bf1d7bab16c3e822a83602200b87368a95c046b2ce6d0d092185138a3f234a7eb0d7f8227b196ef32358b93f012103b1e544ce2d860219bc91314b5483421a553a7b33044659eff0be9214ed58adddffffffff01dd15c293000000001976a91483762a373935ca241d557dfce89171d582b486de88ac99fe9960000000000000000000000000000000","txid":"535ffa3387d3fca14f4a4d373daf7edf00e463982755afce89bc8c48d8168024","hash":null,"size":null,"vsize":null,"version":4,"locktime":1620704921,"vin":[{"txid":"739b00be851679985f429f645e4147452a2e4c27b896cee7c9c10b8873dbcaaf","vout":2,"scriptSig":{"asm":"3045022100b8fbb77efea482b656ad16fc53c5a01d289054c2e429bf1d7bab16c3e822a83602200b87368a95c046b2ce6d0d092185138a3f234a7eb0d7f8227b196ef32358b93f[ALL] 03b1e544ce2d860219bc91314b5483421a553a7b33044659eff0be9214ed58addd","hex":"483045022100b8fbb77efea482b656ad16fc53c5a01d289054c2e429bf1d7bab16c3e822a83602200b87368a95c046b2ce6d0d092185138a3f234a7eb0d7f8227b196ef32358b93f012103b1e544ce2d860219bc91314b5483421a553a7b33044659eff0be9214ed58addd"},"sequence":4294967295,"txinwitness":null}],"vout":[{"value":24.78970333,"n":0,"scriptPubKey":{"asm":"OP_DUP OP_HASH160 83762a373935ca241d557dfce89171d582b486de OP_EQUALVERIFY OP_CHECKSIG","hex":"76a91483762a373935ca241d557dfce89171d582b486de88ac","reqSigs":1,"type":"pubkeyhash","addresses":["RMGJ9tRST45RnwEKHPGgBLuY3moSYP7Mhk"]}}],"blockhash":"0b438a8e50afddb38fb1c7be4536ffc7f7723b76bbc5edf7c28f2c17924dbdfa","confirmations":33186,"rawconfirmations":33186,"time":1620705483,"blocktime":1620705483,"height":2387532}"#;
        let tx: RpcTransaction = json::from_str(json_str).unwrap();
        assert_eq!(tx.version, 4);
        assert_eq!(tx.vin.len(), 1);
        assert_eq!(tx.vout.len(), 1);
        assert_eq!(tx.confirmations, 33186);
        assert_eq!(tx.height, Some(2387532));
        assert!(tx.rawconfirmations.is_some());
    }

    #[test]
    fn test_verbose_transaction_coinbase() {
        // Coinbase transaction (block reward)
        let json_str = r#"{
            "hex": "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000",
            "txid": "0e3e2357e806b6cdb1f70b54c3a3a17b6714ee1f0e68bebb44a74b1efd512098",
            "hash": null,
            "size": null,
            "vsize": null,
            "version": 1,
            "locktime": 0,
            "vin": [{"coinbase": "04ffff001d0104", "sequence": 4294967295}],
            "vout": [{"value": 50.0, "n": 0, "scriptPubKey": {
                "asm": "0496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858ee OP_CHECKSIG",
                "hex": "410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac",
                "reqSigs": 1,
                "type": "pubkey",
                "addresses": ["12c6DSiU4Rq3P4ZxziKxzrL5LmMBrzjrJX"]
            }}],
            "blockhash": "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048",
            "confirmations": 800000,
            "time": 1231469665,
            "blocktime": 1231469665
        }"#;
        let tx: RpcTransaction = json::from_str(json_str).unwrap();
        assert_eq!(tx.vin.len(), 1);
        assert_eq!(tx.locktime, 0);
    }

    #[test]
    fn test_verbose_transaction_segwit() {
        // SegWit transaction with witness data
        let json_str = r#"{
            "hex": "0100000000010100000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100f2052a01000000160014a9974100aeee974a20cda9a2f545704a0ab54f1502483045022100a9974100aeee974a20cda9a2f545704a0ab54f1502483045022100a9974100aeee9700000000",
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "hash": "b08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "size": 150,
            "vsize": 120,
            "version": 1,
            "locktime": 0,
            "vin": [{"txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0", "vout": 0, "scriptSig": {"asm": "", "hex": ""}, "sequence": 4294967295, "txinwitness": ["3045022100a9974100aeee974a20cda9a2f545704a0ab54f1502483045022100a9974100aeee9701", "03b1e544ce2d860219bc91314b5483421a553a7b33044659eff0be9214ed58addd"]}],
            "vout": [{"value": 50.0, "n": 0, "scriptPubKey": {
                "asm": "0 a9974100aeee974a20cda9a2f545704a0ab54f15",
                "hex": "0014a9974100aeee974a20cda9a2f545704a0ab54f15",
                "reqSigs": 1,
                "type": "witness_v0_keyhash",
                "addresses": ["bc1q4xthyqp4mhewjqe6nx5l29wqfg9t2nutdf3z3"]
            }}],
            "blockhash": "0000000000000000000000000000000000000000000000000000000000000001",
            "confirmations": 100,
            "time": 1620000000,
            "blocktime": 1620000000
        }"#;
        let tx: RpcTransaction = json::from_str(json_str).unwrap();
        assert!(tx.hash.is_some()); // wtxid
        assert!(tx.size.is_some());
        assert!(tx.vsize.is_some());
    }

    #[test]
    fn test_verbose_transaction_null_blockhash() {
        // Mempool transactions have null blockhash
        let json_str = r#"{
            "hex": "0100000001deadbeef",
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "hash": null,
            "size": null,
            "vsize": null,
            "version": 1,
            "locktime": 0,
            "vin": [],
            "vout": [],
            "blockhash": null,
            "confirmations": 0,
            "time": null,
            "blocktime": null
        }"#;
        let tx: RpcTransaction = json::from_str(json_str).unwrap();
        assert_eq!(tx.confirmations, 0);
    }

    #[test]
    fn test_verbose_transaction_multiple_outputs() {
        let json_str = r#"{
            "hex": "01000000",
            "txid": "a08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf0",
            "hash": null,
            "size": null,
            "vsize": null,
            "version": 2,
            "locktime": 500000,
            "vin": [{"txid": "b08e6907dbbd3d809776dbfc5d82e371b764ed838b5655e72f463568df1aadf1", "vout": 0, "scriptSig": {"asm": "sig pubkey", "hex": "deadbeef"}, "sequence": 4294967294, "txinwitness": null}],
            "vout": [
                {"value": 0.5, "n": 0, "scriptPubKey": {"asm": "OP_DUP OP_HASH160 hash OP_EQUALVERIFY OP_CHECKSIG", "hex": "76a91488ac", "reqSigs": 1, "type": "pubkeyhash", "addresses": ["1A1"]}},
                {"value": 0.49, "n": 1, "scriptPubKey": {"asm": "OP_DUP OP_HASH160 hash2 OP_EQUALVERIFY OP_CHECKSIG", "hex": "76a91488ac", "reqSigs": 1, "type": "pubkeyhash", "addresses": ["1B2"]}},
                {"value": 0.0, "n": 2, "scriptPubKey": {"asm": "OP_RETURN data", "hex": "6a04deadbeef", "type": "nulldata"}}
            ],
            "blockhash": "0000000000000000000000000000000000000000000000000000000000000001",
            "confirmations": 50,
            "time": 1620000000,
            "blocktime": 1620000000
        }"#;
        let tx: RpcTransaction = json::from_str(json_str).unwrap();
        assert_eq!(tx.vout.len(), 3);
        assert_eq!(tx.vout[2].script.script_type, rpc::v1::types::ScriptType::NullData);
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  ETH RPC Response Tests
// ═══════════════════════════════════════════════════════════════════════

mod eth_responses {
    use super::*;
    use crate::eth::GasStationData;

    #[test]
    fn test_gas_station_data_standard_field() {
        // EthGasStation / Matic gas station uses "standard"
        let json_str = r#"{"standard": "30.5", "fast": "50.0"}"#;
        let _data: GasStationData = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_gas_station_data_average_field() {
        // Some gas stations use "average"
        let json_str = r#"{"average": "25.0", "fast": "40.0"}"#;
        let _data: GasStationData = json::from_str(json_str).unwrap();
    }

    #[test]
    fn test_gas_station_data_numeric() {
        // MmNumber accepts both string and numeric formats
        let json_str = r#"{"average": 25, "fast": 40}"#;
        let _data: GasStationData = json::from_str(json_str).unwrap();
    }
}
