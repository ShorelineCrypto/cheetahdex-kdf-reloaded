//! Local `geth --dev` integration tests for the v1 ETH/ERC20 HTLC swap
//! payment + refund path (`send_maker_payment` -> `send_maker_refunds_payment`).
//!
//! The upstream versions of these tests were `#[ignore]`d ("temporary ignore,
//! will refactor later to use dev chain") because they broadcast against a
//! private, now-unreachable geth dev chain and asserted nothing. Here we stand
//! up a throwaway `geth --dev` chain, deploy a clean-room `EtomicSwap` HTLC
//! contract (authored from this crate's embedded `SWAP_CONTRACT_ABI`, see
//! `for_tests/EtomicSwap.sol`) and a minimal ERC20, then exercise the real
//! payment + refund flow and assert the on-chain outcome.
//!
//! The tests skip themselves when the `geth` binary is not on `PATH`, so the
//! default offline suite stays green; a CI job with geth installed runs them.

use super::*;
use alloy::providers::Provider as _;
use common::{block_on, now_ms};
use mm2_core::mm_ctx::{MmArc, MmCtxBuilder};
use serde_json::{json, Value};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Test-only DEX-fee destination pubkey (community netid), used as the swap receiver.
fn test_dex_fee_addr_raw_pubkey() -> &'static [u8] {
    mm2_net_config::net_config_or_panic(8762).dex_fee_addr_raw_pubkey()
}

/// Deployable bytecode of the clean-room fixtures (compiled with solc 0.8.26,
/// see `for_tests/*.sol`). No `0x` prefix.
const ETOMIC_SWAP_BYTECODE: &str = include_str!("for_tests/EtomicSwap_sol_EtomicSwap.bin");
const TEST_ERC20_BYTECODE: &str = include_str!("for_tests/TestErc20_sol_TestErc20.bin");

/// Well-known throwaway test key (the same one the ignored upstream tests used).
const TEST_PRIV_KEY: &str = "809465b17d0a4ddb3e4c69e8f23c2cabad868f51f8bed5c765ad1d6516c3306f";

/// A running `geth --dev` node. Killed and cleaned up on drop.
struct GethDev {
    child: Child,
    datadir: std::path::PathBuf,
    rpc_url: String,
    dev_account: Address,
    chain_id: u64,
}

impl GethDev {
    /// Starts a `geth --dev` node, or returns `None` if the `geth` binary is
    /// not available so the caller can skip the test.
    fn start() -> Option<GethDev> {
        if Command::new("geth").arg("version").output().is_err() {
            return None;
        }

        // Grab a free TCP port for the HTTP-RPC endpoint.
        let port = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
            listener.local_addr().ok()?.port()
        };
        let rpc_url = format!("http://127.0.0.1:{}", port);
        let datadir = std::env::temp_dir().join(format!("kdf-geth-dev-{}-{}", std::process::id(), port));
        let _ = std::fs::create_dir_all(&datadir);

        let child = Command::new("geth")
            .args([
                "--dev",
                "--http",
                "--http.addr",
                "127.0.0.1",
                "--http.port",
                &port.to_string(),
                "--http.api",
                "eth,web3,net,debug",
                "--datadir",
                datadir.to_str().unwrap(),
                "--ipcdisable",
                "--nodiscover",
                "--maxpeers",
                "0",
                "--rpc.allow-unprotected-txs",
                "--verbosity",
                "1",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut node = GethDev {
            child,
            datadir,
            rpc_url,
            dev_account: Address::default(),
            chain_id: 0,
        };

        // Wait for the RPC to come up.
        let mut ready = false;
        for _ in 0..120 {
            if node.try_rpc("eth_blockNumber", json!([])).is_some() {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if !ready {
            return None;
        }

        let accounts = node.rpc("eth_accounts", json!([]));
        let dev_account: Address = accounts
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .expect("geth --dev exposes a developer account")
            .parse()
            .expect("valid dev account address");
        let chain_id_hex = node.rpc("eth_chainId", json!([]));
        let chain_id =
            u64::from_str_radix(chain_id_hex.as_str().unwrap().trim_start_matches("0x"), 16).expect("valid chain id");

        node.dev_account = dev_account;
        node.chain_id = chain_id;
        Some(node)
    }

    /// Non-panicking single JSON-RPC call, used for readiness polling.
    fn try_rpc(&self, method: &str, params: Value) -> Option<Value> { self.rpc_result(method, params).ok() }

    fn rpc_result(&self, method: &str, params: Value) -> Result<Value, String> {
        let provider =
            crate::eth::alloy_compat::build_provider(vec![self.rpc_url.clone()], vec![]).map_err(|e| e.to_string())?;
        let method = method.to_string();
        block_on(async move {
            provider
                .raw_request::<Value, Value>(std::borrow::Cow::Owned(method), params)
                .await
                .map_err(|e| e.to_string())
        })
    }

    fn rpc(&self, method: &str, params: Value) -> Value {
        self.rpc_result(method, params.clone())
            .unwrap_or_else(|e| panic!("rpc {} failed: {} (params={})", method, e, params))
    }

    /// Waits for a transaction receipt and returns it.
    fn wait_receipt(&self, tx_hash: &str) -> Value {
        for _ in 0..200 {
            // Tolerate the transient "transaction indexing is in progress" error geth
            // returns right after startup, and a null (not-yet-mined) result.
            if let Ok(receipt) = self.rpc_result("eth_getTransactionReceipt", json!([tx_hash])) {
                if !receipt.is_null() {
                    return receipt;
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("timed out waiting for receipt of {}", tx_hash);
    }

    /// Deploys a contract from the unlocked dev account, returning its address.
    fn deploy(&self, bytecode: &str, ctor_args_hex: &str) -> Address {
        let data = format!("0x{}{}", bytecode.trim(), ctor_args_hex);
        let tx = json!({
            "from": format!("0x{:x}", self.dev_account),
            "data": data,
            "gas": "0x3d0900", // 4_000_000
        });
        let hash = self.rpc("eth_sendTransaction", json!([tx]));
        let receipt = self.wait_receipt(hash.as_str().unwrap());
        assert_eq!(
            receipt["status"].as_str(),
            Some("0x1"),
            "contract deploy failed: receipt={}",
            receipt
        );
        receipt["contractAddress"]
            .as_str()
            .expect("deploy receipt has contractAddress")
            .parse()
            .expect("valid contract address")
    }

    /// Sends `wei` from the dev account to `to`.
    fn fund_eth(&self, to: Address, wei: U256) {
        let tx = json!({
            "from": format!("0x{:x}", self.dev_account),
            "to": format!("0x{:x}", to),
            "value": format!("0x{:x}", wei),
        });
        let hash = self.rpc("eth_sendTransaction", json!([tx]));
        self.wait_receipt(hash.as_str().unwrap());
    }

    /// Sends an arbitrary call (with `data`) from the dev account to `to`.
    fn send_call(&self, to: Address, data: Vec<u8>) {
        let tx = json!({
            "from": format!("0x{:x}", self.dev_account),
            "to": format!("0x{:x}", to),
            "data": format!("0x{}", hex::encode(data)),
            "gas": "0x100000",
        });
        let hash = self.rpc("eth_sendTransaction", json!([tx]));
        self.wait_receipt(hash.as_str().unwrap());
    }
}

impl Drop for GethDev {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.datadir);
    }
}

fn test_key_pair() -> KeyPair { KeyPair::from_secret_slice(&hex::decode(TEST_PRIV_KEY).unwrap()).unwrap() }

/// Builds an `EthCoin` bound to the local geth node, using the throwaway test key.
/// Returns the `MmArc` too: the coin only holds a weak ctx ref, so the caller must
/// keep the returned `MmArc` alive for the lifetime of the coin.
fn dev_eth_coin(coin_type: EthCoinType, node: &GethDev, swap_contract: Address) -> (MmArc, EthCoin) {
    let key_pair = test_key_pair();
    let my_addr = key_pair.address();
    let web3 = crate::eth::alloy_compat::build_provider(vec![node.rpc_url.clone()], vec![]).unwrap();
    let ctx = MmCtxBuilder::new().into_mm_arc();
    let ticker = match &coin_type {
        EthCoinType::Erc20 { .. } => "TEST".to_string(),
        _ => "ETH".to_string(),
    };
    let coin = EthCoin(Arc::new(EthCoinImpl {
        ticker,
        coin_type,
        my_address: my_addr,
        sign_message_prefix: Some(String::from("Ethereum Signed Message:\n")),
        signer: EthSigner::Local(key_pair),
        swap_contract_address: swap_contract,
        fallback_swap_contract: None,
        web3_instances: vec![Web3Instance {
            web3: web3.clone(),
            is_parity: false,
        }],
        web3,
        decimals: 18,
        gas_station_url: None,
        gas_station_decimals: ETH_GAS_STATION_DECIMALS,
        gas_station_policy: GasStationPricePolicy::MeanAverageFast,
        history_sync_state: Mutex::new(HistorySyncState::NotStarted),
        ctx: ctx.weak(),
        required_confirmations: 1.into(),
        tron_api: None,
        nft_swap_v2_contract: None,
        swap_gas_fee_policy: Mutex::new(SwapGasFeePolicy::default()),
        erc20_tokens_infos: Default::default(),
        chain_id: Some(node.chain_id),
        logs_block_range: DEFAULT_LOGS_BLOCK_RANGE,
        derivation_method: DerivationMethod::Iguana(my_addr),
        swap_v2_contracts: None,
        gas_limit_v2: EthGasLimitV2::default(),
    }));
    (ctx, coin)
}

#[test]
fn send_and_refund_eth_payment() {
    let node = match GethDev::start() {
        Some(node) => node,
        None => {
            log!("geth binary not available; skipping send_and_refund_eth_payment");
            return;
        },
    };

    let test_addr = test_key_pair().address();
    // Fund the test key with 100 ETH for gas + the locked value.
    node.fund_eth(test_addr, U256::from(100u64) * U256::exp10(18));

    let swap_addr = node.deploy(ETOMIC_SWAP_BYTECODE, "");
    let (_ctx, coin) = dev_eth_coin(EthCoinType::Eth, &node, swap_addr);

    let secret_hash = [1u8; 20];
    // A timelock in the past so the refund is immediately valid.
    let time_lock = (now_ms() / 1000) as u32 - 200;

    let payment = coin
        .send_maker_payment(
            time_lock,
            &[],
            test_dex_fee_addr_raw_pubkey(),
            &secret_hash,
            "0.001".parse().unwrap(),
            &coin.swap_contract_address(),
        )
        .wait()
        .unwrap();

    let payment_hash = format!("0x{}", hex::encode(payment.tx_hash().0));
    let receipt = node.wait_receipt(&payment_hash);
    assert_eq!(receipt["status"].as_str(), Some("0x1"), "payment tx reverted");

    let refund = coin
        .send_maker_refunds_payment(
            &payment.tx_hex(),
            time_lock,
            test_dex_fee_addr_raw_pubkey(),
            &secret_hash,
            &[],
            &coin.swap_contract_address(),
        )
        .wait()
        .unwrap();

    let refund_hash = format!("0x{}", hex::encode(refund.tx_hash().0));
    let refund_receipt = node.wait_receipt(&refund_hash);
    assert_eq!(refund_receipt["status"].as_str(), Some("0x1"), "refund tx reverted");

    // The payment must now be in the SenderRefunded state (3).
    let id = coin.etomic_swap_id(time_lock, &secret_hash);
    let state = coin.payment_status(swap_addr, Token::FixedBytes(id)).wait().unwrap();
    assert_eq!(state, U256::from(3u64), "payment state should be SenderRefunded");
}

#[test]
fn send_and_refund_erc20_payment() {
    let node = match GethDev::start() {
        Some(node) => node,
        None => {
            log!("geth binary not available; skipping send_and_refund_erc20_payment");
            return;
        },
    };

    let test_addr = test_key_pair().address();
    node.fund_eth(test_addr, U256::from(100u64) * U256::exp10(18));

    let swap_addr = node.deploy(ETOMIC_SWAP_BYTECODE, "");
    // Deploy the ERC20 (full supply minted to the dev account), then move a chunk
    // to the test key.
    let token_addr = node.deploy(TEST_ERC20_BYTECODE, "");
    let transfer_data = ERC20_CONTRACT
        .function("transfer")
        .unwrap()
        .encode_input(&[
            Token::Address(test_addr),
            Token::Uint(U256::from(1000u64) * U256::exp10(18)),
        ])
        .unwrap();
    node.send_call(token_addr, transfer_data);

    let coin = dev_eth_coin(
        EthCoinType::Erc20 {
            platform: "ETH".to_string(),
            token_addr,
        },
        &node,
        swap_addr,
    );
    let (_ctx, coin) = coin;

    let secret_hash = [1u8; 20];
    let time_lock = (now_ms() / 1000) as u32 - 200;

    let payment = coin
        .send_maker_payment(
            time_lock,
            &[],
            test_dex_fee_addr_raw_pubkey(),
            &secret_hash,
            "0.001".parse().unwrap(),
            &coin.swap_contract_address(),
        )
        .wait()
        .unwrap();

    let payment_hash = format!("0x{}", hex::encode(payment.tx_hash().0));
    let receipt = node.wait_receipt(&payment_hash);
    assert_eq!(receipt["status"].as_str(), Some("0x1"), "erc20 payment tx reverted");

    let refund = coin
        .send_maker_refunds_payment(
            &payment.tx_hex(),
            time_lock,
            test_dex_fee_addr_raw_pubkey(),
            &secret_hash,
            &[],
            &coin.swap_contract_address(),
        )
        .wait()
        .unwrap();

    let refund_hash = format!("0x{}", hex::encode(refund.tx_hash().0));
    let refund_receipt = node.wait_receipt(&refund_hash);
    assert_eq!(
        refund_receipt["status"].as_str(),
        Some("0x1"),
        "erc20 refund tx reverted"
    );

    let id = coin.etomic_swap_id(time_lock, &secret_hash);
    let state = coin.payment_status(swap_addr, Token::FixedBytes(id)).wait().unwrap();
    assert_eq!(state, U256::from(3u64), "payment state should be SenderRefunded");
}
