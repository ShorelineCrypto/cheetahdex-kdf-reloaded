//! Opt-in live measurement against the ARRR test chain: does Pirate's transparent address index
//! resolve a funded HTLC under the P2SH `b...` form, under the R-address form of the same
//! twenty bytes, or under neither?
//!
//! The test broadcasts a real transaction and spends real (test-chain) coins, so it is inert
//! unless `ARRR_TESTCHAIN_LIVE_DIR` names an instance directory holding `MM2.json` (the
//! `passphrase` is read from it) and `coins` (the ARRR entry of the test chain). The wallet
//! database is kept in `<dir>/db-live-probe`, never in the instance's own `dbdir`. Without the
//! variable the test returns immediately, so CI can never broadcast.
//!
//! Results are printed (`--nocapture`). "Found" is reported separately from "failed": an
//! empty success and a transport or service error are different answers.

use super::z_lightwalletd::{GrpcTransport, LightwalletdTransport};
use super::{z_coin_from_conf_and_params, z_send_htlc, ZCoin, ZcoinBackend, ZcoinProtocolInfo};
use crate::lp_coins_traits::MarketCoinOps;
use crate::utxo::utxo_common::payment_script;
use crate::utxo::{UtxoActivationParams, UtxoAddressFormat};
use common::executor::Timer;
use common::{block_on, now_ms};
use crypto::privkey::key_pair_from_secret;
use crypto::CryptoCtx;
use futures::compat::Future01CompatExt;
use kdf_crypto::dhash160;
use keys::Address;
use mm2_core::mm_ctx::MmCtxBuilder;
use rpc::v1::types::H256 as H256Json;
use serde_json::{json, Value as Json};

const LIVE_DIR_VAR: &str = "ARRR_TESTCHAIN_LIVE_DIR";
const START_HEIGHT: u64 = 175;

fn describe<T>(r: &Result<T, String>, ok: impl Fn(&T) -> String) -> String {
    match r {
        Ok(v) => format!("OK     {}", ok(v)),
        Err(e) => format!("FAILED {}", e),
    }
}

async fn probe(
    transport: &GrpcTransport,
    server: &str,
    label: &str,
    address: &str,
    from: u64,
    to: u64,
    txid_hex: &str,
) {
    println!("--- {label}: {address}");
    let txids = transport.taddr_txids(server, address, from, to).await;
    match &txids.error {
        Some(e) => println!(
            "  GetTaddressTxids    FAILED after {} tx(s): {} (serve_failure={})",
            txids.txs.len(),
            e.message,
            e.serve_failure
        ),
        None => println!("  GetTaddressTxids    OK     {} tx(s) returned", txids.txs.len()),
    }
    // A returned transaction is the HTLC when its raw bytes hash to the txid; compare by length of
    // evidence only here, the txid check is done on the transaction bytes below.
    let found_in_txids = txids.txs.iter().any(|t| {
        zcash_primitives::transaction::Transaction::read(&t.data[..], zcash_protocol::consensus::BranchId::Sapling)
            .map(|tx| hex::encode(tx.txid().as_ref()) == txid_hex)
            .unwrap_or(false)
    });
    println!("  GetTaddressTxids    contains the HTLC txid: {found_in_txids}");

    let utxos = transport
        .address_utxos(server, address)
        .await
        .map_err(|e| e.to_string());
    println!(
        "  GetAddressUtxos     {}",
        describe(&utxos, |u| format!(
            "{} utxo(s) {:?}",
            u.len(),
            u.iter()
                .map(|x| (hex::encode(x.txid), x.index, x.value_zat, x.height))
                .collect::<Vec<_>>()
        ))
    );
    let balance = transport
        .taddr_balance(server, vec![address.to_owned()])
        .await
        .map_err(|e| e.to_string());
    println!("  GetTaddressBalance  {}", describe(&balance, |b| format!("{b} zat")));
}

#[test]
fn live_probe_of_the_transparent_index_for_a_funded_htlc() {
    let Some(dir) = std::env::var_os(LIVE_DIR_VAR) else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    let mm2: Json = serde_json::from_slice(&std::fs::read(dir.join("MM2.json")).unwrap()).unwrap();
    let passphrase = mm2["passphrase"].as_str().unwrap().to_owned();
    let coins: Json = serde_json::from_slice(&std::fs::read(dir.join("coins")).unwrap()).unwrap();
    let mut conf = coins
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["coin"] == "ARRR")
        .cloned()
        .unwrap();
    // The chain is past its Ironwood activation, which this build gates by default; the gate is
    // lifted so the build reaches the network and the network's own answer is what is measured.
    conf["protocol"]["protocol_data"]["consensus_params"]["ironwood_gate_disabled"] = json!(true);
    let servers: Vec<String> = conf["light_wallet_d_servers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let mut protocol_data = conf["protocol"]["protocol_data"].clone();
    // Same shape the activation layer feeds the builder.
    let protocol_info: ZcoinProtocolInfo = serde_json::from_value(protocol_data.take()).unwrap();

    let db_root = dir.join("db-live-probe");
    std::fs::create_dir_all(&db_root).unwrap();
    let ctx = MmCtxBuilder::default()
        .with_conf(json!({ "dbdir": db_root.to_str().unwrap(), "netid": mm2["netid"].clone() }))
        .into_mm_arc();
    let crypto_ctx = CryptoCtx::init_with_iguana_passphrase(ctx.clone(), &passphrase).unwrap();
    let priv_key = crypto_ctx.mm2_internal_privkey_secret();
    let params = UtxoActivationParams::from_legacy_req(&json!({ "method": "enable", "coin": "ARRR" })).unwrap();

    let coin: ZCoin = block_on(z_coin_from_conf_and_params(
        &ctx,
        "ARRR",
        &conf,
        &params,
        priv_key.as_slice(),
        0,
        None,
        protocol_info,
        ZcoinBackend::Light {
            light_wallet_d_servers: servers.clone(),
        },
    ))
    .unwrap();
    println!("address: {}", coin.my_z_address_encoded());

    block_on(async {
        coin.admitted_lightwalletd_servers().await.unwrap();
        while !coin.is_sapling_state_synced() {
            assert!(!coin.sapling_loop_stopped(), "the commitment-tree sync loop died");
            Timer::sleep(1.).await;
        }
        let tip = coin.current_block().compat().await.unwrap();
        println!("tip at start: {tip}");
        coin.fetch_lightwalletd_compact_blocks_to_height(tip, Some(START_HEIGHT), false, &|_, _| {})
            .await
            .unwrap();
        coin.scan_shielded_wallet_db_to_height(tip, |_, _| {}).unwrap();
        println!("balance: {:?}", coin.my_balance().compat().await.unwrap());

        // The HTLC.
        let my_pub = *coin.utxo_arc.priv_key_policy.key_pair_or_err().unwrap().public();
        let other_pub = *key_pair_from_secret(&[3; 32]).unwrap().public();
        let secret: [u8; 32] = rand::random();
        let secret_hash = dhash160(&secret);
        let time_lock = (now_ms() / 1000) as u32 + 2 * 24 * 3600;
        let script = payment_script(time_lock, secret_hash.as_slice(), &my_pub, &other_pub);
        let hash160 = dhash160(&script);
        println!("payment script dhash160 (hex): {}", hex::encode(hash160.as_slice()));

        let tx = z_send_htlc(
            &coin,
            time_lock,
            &my_pub,
            &other_pub,
            secret_hash.as_slice(),
            "0.001".parse().unwrap(),
        )
        .await
        .unwrap();
        let txid_internal: [u8; 32] = *tx.txid().as_ref();
        let txid_hex = hex::encode(txid_internal);
        let txid_display = H256Json::from(txid_internal).reversed();
        println!("HTLC txid (display order): {}", hex::encode(txid_display.0));
        println!("HTLC txid (internal order): {txid_hex}");

        // Wait for it to be mined.
        let mined_height = loop {
            match coin.raw_transaction_with_height(&txid_display).await {
                Ok((_, Some(h))) => break h,
                Ok((_, None)) => println!("not mined yet"),
                Err(e) => println!("transaction not retrievable yet: {e}"),
            }
            Timer::sleep(20.).await;
        };
        println!("mined at height {mined_height}");
        // Let the index and both endpoints catch up past the block.
        Timer::sleep(30.).await;
        let tip = coin.current_block().compat().await.unwrap();

        let p2sh = Address {
            prefix: coin.utxo_arc.conf.p2sh_addr_prefix,
            t_addr_prefix: coin.utxo_arc.conf.p2sh_t_addr_prefix,
            hash: hash160.into(),
            checksum_type: coin.utxo_arc.conf.checksum_type,
            addr_format: UtxoAddressFormat::Standard,
            hrp: None,
        };
        let r_addr = Address {
            prefix: 60,
            t_addr_prefix: 0,
            hash: hash160.into(),
            checksum_type: coin.utxo_arc.conf.checksum_type,
            addr_format: UtxoAddressFormat::Standard,
            hrp: None,
        };
        println!("p2sh prefix {} t-prefix {}", p2sh.prefix, p2sh.t_addr_prefix);
        let transport = GrpcTransport::default();
        for server in &servers {
            println!("===== endpoint {server} (tip {tip})");
            probe(
                &transport,
                server,
                "P2SH form",
                &p2sh.to_string(),
                START_HEIGHT,
                tip,
                &txid_hex,
            )
            .await;
            probe(
                &transport,
                server,
                "R-address form",
                &r_addr.to_string(),
                START_HEIGHT,
                tip,
                &txid_hex,
            )
            .await;
        }
    });
}
