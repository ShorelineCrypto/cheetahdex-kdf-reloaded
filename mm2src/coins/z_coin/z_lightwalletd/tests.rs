//! Acceptance tests for CRD ch.39 §39.11 (T39.11.a, c, d, f, h, i, j and the byte-order pinning of
//! R39.11.4c). Everything runs against a scripted in-process transport: no network.

use super::super::z_ironwood::{discover, IronwoodDiscoveryError, IronwoodStatus};
use super::*;
use serde_json::json;
use std::collections::VecDeque;

const SAPLING_BRANCH: u32 = 0x76b8_09bb;
const IRONWOOD_BRANCH: u32 = 0x37a5_165b;
const ARRR_TIP: u64 = 4_167_700;

fn arrr_params() -> ZcoinConsensusParams {
    serde_json::from_value(json!({
        "overwinter_activation_height": 152855,
        "sapling_activation_height": 152855,
        "blossom_activation_height": null,
        "heartwood_activation_height": null,
        "canopy_activation_height": null,
        "coin_type": 133,
        "hrp_sapling_extended_spending_key": "secret-extended-key-main",
        "hrp_sapling_extended_full_viewing_key": "zxviews",
        "hrp_sapling_payment_address": "zs",
        "b58_pubkey_address_prefix": [0x1c, 0xb8],
        "b58_script_address_prefix": [0x1c, 0xbd]
    }))
    .unwrap()
}

/// A configuration that declares every upgrade up to Canopy and nothing after it.
fn canopy_params() -> ZcoinConsensusParams {
    serde_json::from_value(json!({
        "overwinter_activation_height": 100,
        "sapling_activation_height": 200,
        "blossom_activation_height": 300,
        "heartwood_activation_height": 400,
        "canopy_activation_height": 500,
        "coin_type": 133,
        "hrp_sapling_extended_spending_key": "secret-extended-key-main",
        "hrp_sapling_extended_full_viewing_key": "zxviews",
        "hrp_sapling_payment_address": "zs",
        "b58_pubkey_address_prefix": [0x1c, 0xb8],
        "b58_script_address_prefix": [0x1c, 0xbd]
    }))
    .unwrap()
}

fn info(branch: u32, height: u64) -> ServerInfo {
    ServerInfo {
        chain_name: "main".to_owned(),
        sapling_activation_height: 152855,
        consensus_branch_id: format!("{:08x}", branch),
        block_height: height,
    }
}

fn probes(entries: Vec<(&str, Result<ServerInfo, &str>)>) -> Vec<(String, Result<ServerInfo, String>)> {
    entries
        .into_iter()
        .map(|(server, probe)| (server.to_owned(), probe.map_err(str::to_owned)))
        .collect()
}

fn fresh_wallet() -> WalletBranchState { WalletBranchState::default() }

fn scanned_wallet(recorded: u32) -> WalletBranchState {
    WalletBranchState {
        has_scanned_state: true,
        recorded: Some(recorded),
    }
}

// ---------------------------------------------------------------------------------------------
// T39.11.f -- endpoint admission
// ---------------------------------------------------------------------------------------------

#[test]
fn a_different_network_or_sapling_height_excludes_the_endpoint() {
    let policy = AdmissionPolicy::from_params(&arrr_params());
    let mut other_network = info(IRONWOOD_BRANCH, ARRR_TIP);
    other_network.chain_name = "test".to_owned();
    let mut other_sapling = info(IRONWOOD_BRANCH, ARRR_TIP);
    other_sapling.sapling_activation_height = 1;

    let outcome = decide_admission(
        probes(vec![
            ("net", Ok(other_network)),
            ("sapling", Ok(other_sapling)),
            ("a", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
            ("b", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
        ]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap();

    assert_eq!(outcome.admitted, vec!["a", "b"]);
    assert!(matches!(
        outcome.excluded.iter().find(|(s, _)| s == "net"),
        Some((_, ExclusionReason::NetworkMismatch { .. }))
    ));
    assert!(matches!(
        outcome.excluded.iter().find(|(s, _)| s == "sapling"),
        Some((_, ExclusionReason::SaplingActivationMismatch { .. }))
    ));
}

/// The inversion case R39.11.7a exists for. ARRR's published configuration does not declare the
/// Ironwood upgrade, so the branch the *configuration* derives at today's tip is the pre-upgrade
/// Sapling value. A check that compares each endpoint with that expectation would admit the stale
/// fork and exclude every healthy endpoint.
#[test]
fn an_undeclared_upgrade_does_not_make_the_stale_fork_the_expected_chain() {
    let params = arrr_params();
    let policy = AdmissionPolicy::from_params(&params);

    // The naive local expectation really is the pre-upgrade value at the tip ...
    assert_eq!(
        u32::from(BranchId::for_height(&params, BlockHeight::from_u32(ARRR_TIP as u32))),
        SAPLING_BRANCH
    );
    // ... which is exactly why the configuration must decline to judge at that height.
    assert_eq!(policy.schedule.expected_at(ARRR_TIP), None);

    // Five endpoints on the healthy chain, three of cipig's on the stale fork (some of them with a
    // *higher* reported tip than the healthy ones).
    let mut entries = Vec::new();
    for healthy in ["h1", "h2", "h3", "h4", "h5"] {
        entries.push((healthy, Ok(info(IRONWOOD_BRANCH, ARRR_TIP))));
    }
    for stale in ["s1", "s2", "s3"] {
        entries.push((stale, Ok(info(SAPLING_BRANCH, ARRR_TIP + 10))));
    }
    let outcome = decide_admission(probes(entries), &policy, &fresh_wallet()).unwrap();

    // The assertion the step is built around: the healthy endpoints are admitted and the stale fork
    // is excluded -- never the other way round.
    assert_eq!(outcome.admitted, vec!["h1", "h2", "h3", "h4", "h5"]);
    assert_eq!(outcome.excluded.len(), 3);
    assert!(outcome.excluded.iter().all(|(server, reason)| server.starts_with('s')
        && matches!(reason, ExclusionReason::OutvotedBranch { majority, .. } if *majority == IRONWOOD_BRANCH)));
    assert_eq!(outcome.branch_to_record, Some(IRONWOOD_BRANCH));
}

/// Declaring the Ironwood height binds `Nu6_3` (Step E), so the configuration now determines the
/// branch at every height -- and determines it correctly on both sides of the height: Sapling's below,
/// Ironwood's from it. The inversion this guards against is a determination that is right below the
/// height and wrong above it; a configuration that declares *no* height must still decline to judge.
#[test]
fn carrying_the_ironwood_height_makes_the_expectation_follow_it_on_both_sides() {
    let mut params = serde_json::to_value(arrr_params()).unwrap();
    params["ironwood_activation_height"] = json!(4_161_185);
    let params: ZcoinConsensusParams = serde_json::from_value(params).unwrap();
    assert_eq!(params.ironwood_activation_height(), Some(4_161_185));
    let policy = AdmissionPolicy::from_params(&params);
    assert_eq!(policy.schedule.expected_at(4_161_184), Some(SAPLING_BRANCH));
    assert_eq!(policy.schedule.expected_at(4_161_185), Some(IRONWOOD_BRANCH));
    assert_eq!(policy.schedule.expected_at(ARRR_TIP), Some(IRONWOOD_BRANCH));

    // Without a declared height nothing is judged at the tip.
    let undeclared = AdmissionPolicy::from_params(&arrr_params());
    assert_eq!(undeclared.schedule.expected_at(ARRR_TIP), None);
}

/// A height *discovered* after the policy was built reaches the build through the shared parameters
/// but cannot re-arm the policy: its horizon was fixed when the upgrade was undeclared, so it keeps
/// declining to judge. That is the safe direction -- a late-learned height must never be able to
/// exclude the healthy endpoints it was learned from.
#[test]
fn a_discovered_height_does_not_re_arm_a_policy_built_before_it() {
    let params = arrr_params();
    let policy = AdmissionPolicy::from_params(&params);
    params.record_discovered_ironwood_height(4_161_185);
    assert_eq!(
        params.activation_height(NetworkUpgrade::Nu6_3),
        Some(BlockHeight::from_u32(4_161_185))
    );
    assert_eq!(policy.schedule.expected_at(ARRR_TIP), None);
}

#[test]
fn where_the_configuration_determines_the_branch_a_mismatch_excludes() {
    let policy = AdmissionPolicy::from_params(&canopy_params());
    let canopy = u32::from(BranchId::Canopy);
    let heartwood = u32::from(BranchId::Heartwood);
    // Below the horizon (the Canopy height) the configuration is authoritative.
    assert_eq!(policy.schedule.expected_at(450), Some(heartwood));

    let mut right = info(heartwood, 450);
    right.sapling_activation_height = 200;
    let mut wrong = info(canopy, 450);
    wrong.sapling_activation_height = 200;
    let outcome = decide_admission(
        probes(vec![("right", Ok(right)), ("wrong", Ok(wrong))]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap();
    assert_eq!(outcome.admitted, vec!["right"]);
    assert!(matches!(
        outcome.excluded[0].1,
        ExclusionReason::ConfigBranchMismatch { reported, expected, .. } if reported == canopy && expected == heartwood
    ));
    // Determined by the configuration: nothing to record, no majority needed.
    assert_eq!(outcome.branch_to_record, None);
}

/// A wallet whose scanned state follows one chain admits only that partition, and an endpoint of
/// another partition is excluded however high its tip is. This is what stops the wallet rewinding
/// ~99 blocks and following a stale fork's chain.
#[test]
fn a_wallet_with_scanned_state_stays_on_its_recorded_branch() {
    let policy = AdmissionPolicy::from_params(&arrr_params());
    let outcome = decide_admission(
        probes(vec![
            ("fork", Ok(info(SAPLING_BRANCH, ARRR_TIP + 5_000))),
            ("mine", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
        ]),
        &policy,
        &scanned_wallet(IRONWOOD_BRANCH),
    )
    .unwrap();
    assert_eq!(outcome.admitted, vec!["mine"]);
    assert!(matches!(
        outcome.excluded[0].1,
        ExclusionReason::OutsideWalletBranch { reported, bound }
            if reported == SAPLING_BRANCH && bound == IRONWOOD_BRANCH
    ));
    // The binding is already recorded; it is never rewritten by a later decision.
    assert_eq!(outcome.branch_to_record, None);

    // A single endpoint is enough when the wallet already knows its chain.
    assert!(decide_admission(
        probes(vec![("only", Ok(info(IRONWOOD_BRANCH, ARRR_TIP)))]),
        &policy,
        &scanned_wallet(IRONWOOD_BRANCH),
    )
    .is_ok());

    // And if every endpoint is on another branch the wallet stops rather than switching.
    let err = decide_admission(
        probes(vec![("fork", Ok(info(SAPLING_BRANCH, ARRR_TIP)))]),
        &policy,
        &scanned_wallet(IRONWOOD_BRANCH),
    )
    .unwrap_err();
    assert!(matches!(err, LightwalletdError::AllExcluded(_)), "{err:?}");
}

#[test]
fn a_fresh_wallet_needs_a_strict_majority_and_never_a_single_voice() {
    let policy = AdmissionPolicy::from_params(&arrr_params());

    // One reachable endpoint: no partition holds a majority, and every observed identifier is
    // reported.
    let err = decide_admission(
        probes(vec![("only", Ok(info(IRONWOOD_BRANCH, ARRR_TIP)))]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap_err();
    match err {
        LightwalletdError::NoBranchMajority(observed) => assert!(observed.contains("37a5165b x1"), "{observed}"),
        other => panic!("{other:?}"),
    }

    // A tie.
    let err = decide_admission(
        probes(vec![
            ("a", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
            ("b", Ok(info(SAPLING_BRANCH, ARRR_TIP))),
        ]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap_err();
    match err {
        LightwalletdError::NoBranchMajority(observed) => {
            assert!(
                observed.contains("37a5165b x1") && observed.contains("76b809bb x1"),
                "{observed}"
            )
        },
        other => panic!("{other:?}"),
    }

    // A wallet with scanned state but no recorded identifier (it predates the binding) is decided
    // by majority and gets its identifier recorded.
    let legacy = WalletBranchState {
        has_scanned_state: true,
        recorded: None,
    };
    let outcome = decide_admission(
        probes(vec![
            ("a", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
            ("b", Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
            ("c", Ok(info(SAPLING_BRANCH, ARRR_TIP))),
        ]),
        &policy,
        &legacy,
    )
    .unwrap();
    assert_eq!(outcome.admitted, vec!["a", "b"]);
    assert_eq!(outcome.branch_to_record, Some(IRONWOOD_BRANCH));
}

#[test]
fn unreachable_and_excluded_endpoint_sets_fail_with_different_errors() {
    let policy = AdmissionPolicy::from_params(&arrr_params());
    let unreachable = decide_admission(
        probes(vec![("a", Err("connection refused")), ("b", Err("timed out"))]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap_err();
    assert!(
        matches!(unreachable, LightwalletdError::AllUnreachable(_)),
        "{unreachable:?}"
    );

    let mut other_network = info(IRONWOOD_BRANCH, ARRR_TIP);
    other_network.chain_name = "test".to_owned();
    let excluded = decide_admission(
        probes(vec![("a", Ok(other_network)), ("b", Err("timed out"))]),
        &policy,
        &fresh_wallet(),
    )
    .unwrap_err();
    assert!(matches!(excluded, LightwalletdError::AllExcluded(_)), "{excluded:?}");

    assert_eq!(
        decide_admission(Vec::new(), &policy, &fresh_wallet()).unwrap_err(),
        LightwalletdError::NoEndpoints
    );
}

#[test]
fn branch_identifiers_parse_as_the_service_sends_them() {
    assert_eq!(parse_branch_id("76b809bb"), Some(SAPLING_BRANCH));
    assert_eq!(parse_branch_id("0x37a5165b"), Some(IRONWOOD_BRANCH));
    assert_eq!(parse_branch_id(""), None);
    assert_eq!(parse_branch_id("not hex"), None);
    assert_eq!(parse_branch_id("1ffffffff"), None);
}

// ---------------------------------------------------------------------------------------------
// A scripted transport
// ---------------------------------------------------------------------------------------------

#[derive(Default)]
struct Script {
    info: Option<Result<ServerInfo, EndpointError>>,
    tips: VecDeque<Result<u64, EndpointError>>,
    transaction: Option<Result<RawTx, EndpointError>>,
    send: Option<Result<(), EndpointError>>,
    balance: Option<Result<i64, EndpointError>>,
}

#[derive(Default)]
struct FakeTransport {
    scripts: Mutex<HashMap<String, Script>>,
    calls: Mutex<Vec<(String, &'static str)>>,
    last_balance_request: Mutex<Option<Vec<String>>>,
    last_txid: Mutex<Option<[u8; 32]>>,
}

impl fmt::Debug for FakeTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("FakeTransport") }
}

impl FakeTransport {
    fn serve(&self, server: &str, script: Script) { self.scripts.lock().insert(server.to_owned(), script); }

    fn record(&self, server: &str, method: &'static str) { self.calls.lock().push((server.to_owned(), method)); }

    fn called(&self, server: &str, method: &'static str) -> usize {
        self.calls
            .lock()
            .iter()
            .filter(|(s, m)| s == server && *m == method)
            .count()
    }

    fn unscripted() -> EndpointError { EndpointError::Transport("not scripted".to_owned()) }
}

#[async_trait]
impl LightwalletdTransport for FakeTransport {
    async fn light_info(&self, server: &str) -> Result<ServerInfo, EndpointError> {
        self.record(server, "info");
        self.scripts
            .lock()
            .get(server)
            .and_then(|s| s.info.clone())
            .unwrap_or_else(|| Err(Self::unscripted()))
    }

    async fn latest_block(&self, server: &str) -> Result<u64, EndpointError> {
        self.record(server, "tip");
        self.scripts
            .lock()
            .get_mut(server)
            .and_then(|s| s.tips.pop_front())
            .unwrap_or_else(|| Err(Self::unscripted()))
    }

    async fn block_time(&self, server: &str, height: u64) -> Result<u32, EndpointError> {
        self.record(server, "time");
        Ok(1_000 + height as u32)
    }

    async fn transaction(&self, server: &str, txid: [u8; 32]) -> Result<RawTx, EndpointError> {
        self.record(server, "tx");
        *self.last_txid.lock() = Some(txid);
        self.scripts
            .lock()
            .get(server)
            .and_then(|s| s.transaction.clone())
            .unwrap_or_else(|| Err(Self::unscripted()))
    }

    async fn send_transaction(&self, server: &str, _data: Vec<u8>) -> Result<(), EndpointError> {
        self.record(server, "send");
        self.scripts
            .lock()
            .get(server)
            .and_then(|s| s.send.clone())
            .unwrap_or_else(|| Err(Self::unscripted()))
    }

    async fn taddr_balance(&self, server: &str, addresses: Vec<String>) -> Result<i64, EndpointError> {
        self.record(server, "balance");
        *self.last_balance_request.lock() = Some(addresses);
        self.scripts
            .lock()
            .get(server)
            .and_then(|s| s.balance.clone())
            .unwrap_or_else(|| Err(Self::unscripted()))
    }

    async fn taddr_txids(&self, server: &str, _address: &str, _start: u64, _end: u64) -> WindowRead {
        self.record(server, "history");
        WindowRead::default()
    }

    async fn address_utxos(&self, server: &str, _address: &str) -> Result<Vec<TransparentUtxo>, EndpointError> {
        self.record(server, "utxos");
        Ok(Vec::new())
    }
}

fn backend_over(transport: &Arc<FakeTransport>, servers: &[&str]) -> LightwalletdBackend {
    LightwalletdBackend::new(
        transport.clone(),
        servers.iter().map(|s| (*s).to_owned()).collect(),
        AdmissionPolicy::from_params(&arrr_params()),
        0xbd,
    )
}

fn healthy(tips: &[u64]) -> Script {
    Script {
        info: Some(Ok(info(IRONWOOD_BRANCH, ARRR_TIP))),
        tips: tips.iter().map(|t| Ok(*t)).collect(),
        ..Script::default()
    }
}

fn stale_fork() -> Script {
    Script {
        info: Some(Ok(info(SAPLING_BRANCH, ARRR_TIP + 5_000))),
        tips: VecDeque::from([Ok(ARRR_TIP + 5_000)]),
        ..Script::default()
    }
}

#[tokio::test]
async fn an_excluded_endpoint_is_never_asked_for_anything() {
    let transport = Arc::new(FakeTransport::default());
    transport.serve("fork", stale_fork());
    transport.serve("a", healthy(&[ARRR_TIP]));
    transport.serve("b", healthy(&[ARRR_TIP]));
    let backend = backend_over(&transport, &["fork", "a", "b"]);

    assert_eq!(backend.chain_tip().await.unwrap(), ARRR_TIP);
    assert_eq!(
        transport.called("fork", "tip"),
        0,
        "an off-branch endpoint must not be used"
    );
    assert_eq!(backend.excluded_endpoints().len(), 1);
    assert_eq!(backend.excluded_endpoints()[0].0, "fork");
}

/// D39.11.g: an endpoint can pass admission and still fail the method that is actually needed.
#[tokio::test]
async fn an_admitted_endpoint_that_fails_the_needed_method_is_skipped() {
    let transport = Arc::new(FakeTransport::default());
    let mut broken = healthy(&[]);
    broken.tips = VecDeque::from([Err(EndpointError::Service("502 Bad Gateway".to_owned()))]);
    transport.serve("broken", broken);
    transport.serve("good", healthy(&[ARRR_TIP]));
    let backend = backend_over(&transport, &["broken", "good"]);

    assert_eq!(backend.chain_tip().await.unwrap(), ARRR_TIP);
    assert_eq!(transport.called("broken", "tip"), 1);
    assert_eq!(transport.called("good", "tip"), 1);
}

/// T39.11.i: a lagging read neither lowers the coin's height nor is reported as it.
#[tokio::test]
async fn the_reported_height_never_goes_backwards() {
    let transport = Arc::new(FakeTransport::default());
    transport.serve("a", healthy(&[ARRR_TIP, ARRR_TIP - 7, ARRR_TIP + 1]));
    transport.serve("b", healthy(&[]));
    let backend = backend_over(&transport, &["a", "b"]);

    assert_eq!(backend.chain_tip().await.unwrap(), ARRR_TIP);
    let lagging = backend.fresh_tip().await.unwrap();
    assert_eq!(lagging, FreshTip {
        read: ARRR_TIP - 7,
        highest_before: ARRR_TIP
    });
    assert_eq!(lagging.monotonic(), ARRR_TIP);
    assert_eq!(backend.chain_tip().await.unwrap(), ARRR_TIP + 1);
}

/// T39.11.c / T39.11.d: a failing tip read is an error, never a fallback value.
#[tokio::test]
async fn a_failed_tip_read_is_an_error_and_not_a_default() {
    let transport = Arc::new(FakeTransport::default());
    transport.serve("a", healthy(&[]));
    let backend = backend_over(&transport, &["a"]);
    let err = backend.fresh_tip().await.unwrap_err();
    // (a single endpoint cannot be admitted by majority on a fresh wallet, so the failure is the
    // admission one; with a recorded branch it is the read itself)
    assert!(matches!(err, LightwalletdError::NoBranchMajority(_)), "{err:?}");

    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    backend.bind_wallet(Arc::new(Bound));
    let err = backend.fresh_tip().await.unwrap_err();
    assert!(
        matches!(err, LightwalletdError::AllFailed {
            op: "GetLatestBlock",
            ..
        }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_rejected_broadcast_is_reported_and_not_retried_elsewhere() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    let mut first = healthy(&[]);
    first.send = Some(Err(EndpointError::Rejected {
        code: -26,
        message: "tx-expiring-soon".to_owned(),
    }));
    let mut second = healthy(&[]);
    second.send = Some(Ok(()));
    transport.serve("first", first);
    transport.serve("second", second);
    let backend = backend_over(&transport, &["first", "second"]);
    backend.bind_wallet(Arc::new(Bound));

    let err = backend.send_transaction(vec![1, 2, 3]).await.unwrap_err();
    assert_eq!(err, LightwalletdError::BroadcastRejected {
        code: -26,
        message: "tx-expiring-soon".to_owned()
    });
    assert_eq!(
        transport.called("second", "send"),
        0,
        "a consensus rejection is not retried"
    );
}

#[tokio::test]
async fn a_transport_failure_on_broadcast_does_move_on_to_the_next_endpoint() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    let mut first = healthy(&[]);
    first.send = Some(Err(EndpointError::Transport("reset".to_owned())));
    let mut second = healthy(&[]);
    second.send = Some(Ok(()));
    transport.serve("first", first);
    transport.serve("second", second);
    let backend = backend_over(&transport, &["first", "second"]);
    backend.bind_wallet(Arc::new(Bound));

    backend.send_transaction(vec![1]).await.unwrap();
    assert_eq!(transport.called("second", "send"), 1);
}

// ---------------------------------------------------------------------------------------------
// T39.11.c -- one builder height
// ---------------------------------------------------------------------------------------------

#[test]
fn every_construction_path_derives_the_same_target_and_expiry_from_one_tip() {
    let tip = FreshTip {
        read: ARRR_TIP,
        highest_before: ARRR_TIP,
    };
    // The ordinary send path and the HTLC spend/refund path both call this one function, so one
    // injected tip gives both the same target height and therefore the same expiry height.
    let ordinary_send = builder_target_height(tip, ARRR_TIP).unwrap();
    let htlc_spend_or_refund = builder_target_height(tip, ARRR_TIP).unwrap();
    assert_eq!(ordinary_send, htlc_spend_or_refund);
    assert_eq!(u64::from(ordinary_send), ARRR_TIP + 1);
    assert_eq!(
        u64::from(ordinary_send) + u64::from(BUILDER_EXPIRY_DELTA),
        ARRR_TIP + 1 + 40
    );
}

#[test]
fn a_tip_more_than_the_expiry_delta_behind_what_was_observed_refuses_to_build() {
    let behind = FreshTip {
        read: ARRR_TIP - 6_000,
        highest_before: ARRR_TIP,
    };
    assert_eq!(
        builder_target_height(behind, ARRR_TIP).unwrap_err(),
        LightwalletdError::StaleTip {
            tip: ARRR_TIP - 6_000,
            observed: ARRR_TIP
        }
    );

    // The wallet's own scanned height corroborates too, even with no earlier tip read.
    let first_read = FreshTip {
        read: ARRR_TIP - 100,
        highest_before: 0,
    };
    assert!(matches!(
        builder_target_height(first_read, ARRR_TIP),
        Err(LightwalletdError::StaleTip { .. })
    ));

    // Within the delta it is ordinary propagation lag, and the height used does not go backwards.
    let lagging = FreshTip {
        read: ARRR_TIP - BUILDER_EXPIRY_DELTA as u64,
        highest_before: ARRR_TIP,
    };
    assert_eq!(u64::from(builder_target_height(lagging, 0).unwrap()), ARRR_TIP + 1);
}

// ---------------------------------------------------------------------------------------------
// T39.11.d -- confirmations and the uncorroborated expiry
// ---------------------------------------------------------------------------------------------

#[test]
fn confirmation_depth_is_derived_from_the_mined_height_and_the_tip() {
    assert_eq!(confirmation_depth(100, Some(100)), 1);
    assert_eq!(confirmation_depth(100, Some(91)), 10);
    assert_eq!(confirmation_depth(100, None), 0, "unmined");
    assert_eq!(confirmation_depth(100, Some(101)), 0, "above the tip");
    assert_eq!(confirmation_depth(100, Some(0)), 0);
}

#[tokio::test]
async fn an_unfetchable_transaction_past_its_expiry_is_not_declared_expired() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let txid = H256Json::from([7u8; 32]);

    // The backend merely fails the lookup.
    let transport = Arc::new(FakeTransport::default());
    let mut failing = healthy(&[ARRR_TIP]);
    failing.transaction = Some(Err(EndpointError::Service("502".to_owned())));
    transport.serve("a", failing);
    let client = LightwalletdClient::new(transport.clone(), vec!["a".to_owned()], &arrr_params(), 0xbd);
    client.0.bind_wallet(Arc::new(Bound));
    let err = client
        .wait_for_confirmations(txid, (ARRR_TIP - 10) as u32, 1, false, u64::MAX, 1)
        .await
        .unwrap_err();
    assert!(err.contains("does not prove it expired"), "{err}");
    assert!(!err.contains("has expired"), "{err}");

    // The service explicitly answers "no such transaction": that is the corroborated expiry.
    let transport = Arc::new(FakeTransport::default());
    let mut missing = healthy(&[ARRR_TIP]);
    missing.transaction = Some(Err(EndpointError::NotFound("unknown txid".to_owned())));
    transport.serve("a", missing);
    let client = LightwalletdClient::new(transport.clone(), vec!["a".to_owned()], &arrr_params(), 0xbd);
    client.0.bind_wallet(Arc::new(Bound));
    let err = client
        .wait_for_confirmations(txid, (ARRR_TIP - 10) as u32, 1, false, u64::MAX, 1)
        .await
        .unwrap_err();
    assert!(err.contains("has expired"), "{err}");
}

#[tokio::test]
async fn a_confirmed_transaction_satisfies_the_wait_without_any_server_side_decode() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    let mut script = healthy(&[ARRR_TIP]);
    script.transaction = Some(Ok(RawTx {
        data: vec![1, 2, 3],
        height: Some(ARRR_TIP - 2),
    }));
    transport.serve("a", script);
    let client = LightwalletdClient::new(transport, vec!["a".to_owned()], &arrr_params(), 0xbd);
    client.0.bind_wallet(Arc::new(Bound));
    client
        .wait_for_confirmations(H256Json::from([9u8; 32]), 0, 3, false, u64::MAX, 1)
        .await
        .unwrap();
}

#[tokio::test]
async fn notarized_depth_is_refused_and_not_substituted() {
    let transport = Arc::new(FakeTransport::default());
    let client = LightwalletdClient::new(transport, vec!["a".to_owned()], &arrr_params(), 0xbd);
    let err = client
        .wait_for_confirmations(H256Json::from([9u8; 32]), 0, 1, true, u64::MAX, 1)
        .await
        .unwrap_err();
    assert!(err.contains("notarized"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// R39.11.4c -- byte order
// ---------------------------------------------------------------------------------------------

#[test]
fn the_txid_conversion_point_is_explicit_and_works_in_both_directions() {
    // Not byte-order symmetric.
    let mut internal = [0u8; 32];
    for (i, byte) in internal.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let display = txid_internal_to_display(internal);
    assert_eq!(display.0[0], 31);
    assert_eq!(display.0[31], 0);
    assert_eq!(txid_display_to_internal(&display), internal);
    assert_ne!(display.0, internal);
}

#[tokio::test]
async fn the_lookup_sends_the_internal_byte_order() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    let mut script = healthy(&[]);
    script.transaction = Some(Ok(RawTx {
        data: vec![0],
        height: None,
    }));
    transport.serve("a", script);
    let client = LightwalletdClient::new(transport.clone(), vec!["a".to_owned()], &arrr_params(), 0xbd);
    client.0.bind_wallet(Arc::new(Bound));

    let mut display_bytes = [0u8; 32];
    for (i, byte) in display_bytes.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let display = H256Json::from(display_bytes);
    client.raw_transaction(&display).await.unwrap();
    let sent = transport.last_txid.lock().unwrap();
    let mut expected = display_bytes;
    expected.reverse();
    assert_eq!(sent, expected);
}

// ---------------------------------------------------------------------------------------------
// T39.11.j -- the balance request
// ---------------------------------------------------------------------------------------------

#[test]
fn the_balance_request_is_a_flat_repeated_string_field_on_the_wire() {
    let request = balance_request(&["RM6uy".to_owned(), "RXyz".to_owned()]);
    // field 1, wire type 2, length, bytes -- once per address, nothing nested.
    let mut expected = Vec::new();
    for address in ["RM6uy", "RXyz"] {
        expected.push(0x0a);
        expected.push(address.len() as u8);
        expected.extend_from_slice(address.as_bytes());
    }
    assert_eq!(request.encode_to_vec(), expected);

    // The malformed nesting the probes tripped over: field 1 containing a submessage whose field 1
    // is the address. It encodes differently, and is what the node rejects as an invalid list.
    let mut nested = Vec::new();
    let inner = [&[0x0a, 5][..], b"RM6uy"].concat();
    nested.push(0x0a);
    nested.push(inner.len() as u8);
    nested.extend_from_slice(&inner);
    assert_ne!(request.encode_to_vec(), nested);
}

#[tokio::test]
async fn several_addresses_go_out_as_one_call() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    let mut script = healthy(&[]);
    script.balance = Some(Ok(179_356_160_014));
    transport.serve("a", script);
    let backend = backend_over(&transport, &["a"]);
    backend.bind_wallet(Arc::new(Bound));

    let address = |hash: u8| Address {
        prefix: 0x3c,
        t_addr_prefix: 0,
        hrp: None,
        hash: keys::AddressHashEnum::AddressHash([hash; 20].into()),
        checksum_type: kdf_crypto::ChecksumType::DSHA256,
        addr_format: keys::AddressFormat::Standard,
    };
    let total = backend
        .taddr_balance(vec![address(1), address(2), address(3)])
        .await
        .unwrap();
    assert_eq!(total, 179_356_160_014);
    assert_eq!(transport.called("a", "balance"), 1);
    assert_eq!(transport.last_balance_request.lock().as_ref().unwrap().len(), 3);
}

#[tokio::test]
async fn a_script_hash_address_is_refused_and_never_answered_with_an_empty_success() {
    let transport = Arc::new(FakeTransport::default());
    let backend = backend_over(&transport, &["a"]);
    let p2sh = Address {
        prefix: 0xbd,
        t_addr_prefix: 0,
        hrp: None,
        hash: keys::AddressHashEnum::AddressHash([5u8; 20].into()),
        checksum_type: kdf_crypto::ChecksumType::DSHA256,
        addr_format: keys::AddressFormat::Standard,
    };
    assert!(matches!(
        backend.taddr_balance(vec![p2sh.clone()]).await,
        Err(LightwalletdError::Unsupported { .. })
    ));
    assert!(matches!(
        backend.address_utxos(&p2sh).await,
        Err(LightwalletdError::Unsupported { .. })
    ));
    assert_eq!(transport.called("a", "balance"), 0);
    assert_eq!(transport.called("a", "utxos"), 0);
}

// ---------------------------------------------------------------------------------------------
// T39.11.h -- the transparent history reader
// ---------------------------------------------------------------------------------------------

fn tx_at(height: u64, tag: u8) -> RawTx {
    RawTx {
        data: vec![tag; 4],
        height: Some(height),
    }
}

fn serve_failure() -> WindowError {
    WindowError {
        serve_failure: true,
        message: "No information available about transaction".to_owned(),
    }
}

/// A server model: transactions by height, an unservable height that aborts any stream containing
/// it, and the real server's habit of dropping the last height group of every stream.
fn history_server(
    chain: Vec<RawTx>,
    unservable: Option<u64>,
    requests: Arc<Mutex<Vec<(u64, u64)>>>,
) -> impl FnMut(u64, u64) -> futures::future::Ready<WindowRead> {
    move |start, end| {
        requests.lock().push((start, end));
        let mut groups: Vec<u64> = chain
            .iter()
            .filter_map(|tx| tx.height)
            .filter(|h| *h >= start && *h <= end)
            .collect();
        groups.sort_unstable();
        groups.dedup();
        let mut delivered = Vec::new();
        for (idx, height) in groups.iter().enumerate() {
            let is_last_group = idx + 1 == groups.len();
            if Some(*height) == unservable {
                return futures::future::ready(WindowRead {
                    txs: delivered,
                    error: Some(serve_failure()),
                });
            }
            if is_last_group {
                // dropped
                continue;
            }
            delivered.extend(chain.iter().filter(|tx| tx.height == Some(*height)).cloned());
        }
        futures::future::ready(WindowRead {
            txs: delivered,
            error: None,
        })
    }
}

#[tokio::test]
async fn a_failing_stream_is_not_no_history_and_its_deliveries_are_retained() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let chain = vec![tx_at(10, 1), tx_at(20, 2), tx_at(30, 3), tx_at(40, 4)];
    let read = read_history(1, 100, 100, history_server(chain, Some(30), requests))
        .await
        .unwrap();
    assert!(!read.is_clean());
    assert_eq!(read.gaps, vec![30]);
    assert!(read.transactions.iter().any(|tx| tx.height == Some(10)));
    assert!(read.transactions.iter().any(|tx| tx.height == Some(20)));
}

#[tokio::test]
async fn the_failing_window_is_bisected_and_the_offending_height_skipped_and_recorded() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let chain = vec![
        tx_at(10, 1),
        tx_at(20, 2),
        tx_at(30, 3),
        tx_at(40, 4),
        tx_at(45, 5),
        tx_at(90, 9),
    ];
    let read = read_history(1, 100, 100, history_server(chain, Some(30), requests.clone()))
        .await
        .unwrap();
    assert_eq!(
        read.gaps,
        vec![30],
        "the skipped height is a recorded gap, not forgotten"
    );
    assert!(read.failure.is_none(), "a serve failure does not fail the pass");
    // Everything servable around the gap arrived.
    for height in [10, 20, 40, 45] {
        assert!(read.transactions.iter().any(|tx| tx.height == Some(height)), "{height}");
    }
    // Bisection narrowed the failing range down to the single height.
    assert!(requests.lock().iter().any(|(s, e)| s == e && *s == 30));
    // A swap-path negative answer over this range is refused while sync/history still advanced.
    assert!(!read.is_clean());
    assert!(!read.transactions.is_empty());
}

#[tokio::test]
async fn resumption_starts_at_the_last_received_height_and_duplicates_are_suppressed() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    // Windows of 50: the first window's last group (height 45) is dropped by the server, so the
    // next window has to start at the last *received* height (20), not at 51.
    let chain = vec![tx_at(10, 1), tx_at(20, 2), tx_at(45, 3), tx_at(70, 4), tx_at(95, 5)];
    let read = read_history(1, 100, 50, history_server(chain, None, requests.clone()))
        .await
        .unwrap();
    let starts: Vec<u64> = requests.lock().iter().map(|(s, _)| *s).collect();
    assert_eq!(starts[0], 1);
    assert_eq!(
        starts[1], 20,
        "resumed from the last received height, not one past the window"
    );
    let heights: Vec<Option<u64>> = read.transactions.iter().map(|tx| tx.height).collect();
    // The height-45 group, dropped as the tail of the first two windows, arrives once the window is
    // widened to contain a later group; height 20 was delivered several times on the wire and
    // appears once.
    assert_eq!(heights.iter().filter(|h| **h == Some(20)).count(), 1);
    assert!(heights.contains(&Some(45)));
}

#[tokio::test]
async fn no_request_is_open_ended_or_below_one() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let chain = vec![tx_at(5, 1), tx_at(2_500, 2)];
    read_history(0, 3_000, 1_000, history_server(chain, None, requests.clone()))
        .await
        .unwrap();
    let requests = requests.lock();
    assert!(!requests.is_empty());
    for (start, end) in requests.iter() {
        assert!(*start >= 1 && *end >= 1, "{start}..{end}");
        assert!(
            end - start < 1_000 * HISTORY_MAX_WIDEN_FACTOR,
            "{start}..{end} is not bounded by the widest window"
        );
    }

    assert!(matches!(
        read_history(10, 5, 100, |_, _| futures::future::ready(WindowRead::default())).await,
        Err(LightwalletdError::InvalidRequest(_))
    ));
}

#[tokio::test]
async fn a_transport_failure_ends_the_read_without_claiming_a_clean_answer() {
    let read = read_history(1, 100, 100, |_, _| {
        futures::future::ready(WindowRead {
            txs: vec![tx_at(3, 1)],
            error: Some(WindowError {
                serve_failure: false,
                message: "connection reset".to_owned(),
            }),
        })
    })
    .await
    .unwrap();
    assert!(!read.is_clean());
    assert_eq!(read.failure.as_deref(), Some("connection reset"));
    assert_eq!(read.transactions.len(), 1);
}

#[tokio::test]
async fn a_complete_read_with_no_gap_is_clean_even_when_it_finds_nothing() {
    let read = read_history(1, 100, 100, |_, _| futures::future::ready(WindowRead::default()))
        .await
        .unwrap();
    assert!(read.is_clean());
    assert!(read.transactions.is_empty());
}

// ---------------------------------------------------------------------------------------------
// R39.11.1 -- the median time past
// ---------------------------------------------------------------------------------------------

#[tokio::test]
async fn median_time_past_is_computed_locally_from_compact_block_times() {
    struct Bound;
    impl WalletBranchStore for Bound {
        fn state(&self) -> Result<WalletBranchState, String> { Ok(scanned_wallet(IRONWOOD_BRANCH)) }

        fn record(&self, _: u32) -> Result<(), String> { Ok(()) }
    }
    let transport = Arc::new(FakeTransport::default());
    transport.serve("a", healthy(&[]));
    let backend = backend_over(&transport, &["a"]);
    backend.bind_wallet(Arc::new(Bound));
    // times are 1000 + height; the median of heights 90..=100 is 95.
    assert_eq!(backend.median_time_past(100, 11).await.unwrap(), 1_095);
    assert_eq!(transport.called("a", "time"), 11);
    assert!(matches!(
        backend.median_time_past(100, MEDIAN_TIME_WINDOW_MAX + 1).await,
        Err(LightwalletdError::InvalidRequest(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// Ironwood activation discovery (R39.6.4e) against the scripted transport, whose block at height
// `h` carries time `1000 + h`.
// ---------------------------------------------------------------------------------------------

/// The activation time under which block `transition` is the first one over it.
fn time_of_transition(transition: u64) -> u32 { 1_000 + transition as u32 - 1 }

async fn discovered(
    branch: u32,
    tip: u64,
    activation_time: Option<u32>,
) -> Result<IronwoodStatus, IronwoodDiscoveryError> {
    let transport = Arc::new(FakeTransport::default());
    for server in ["a", "b"] {
        transport.serve(server, Script {
            info: Some(Ok(info(branch, tip))),
            ..Script::default()
        });
    }
    discover(&backend_over(&transport, &["a", "b"]), activation_time).await
}

#[tokio::test]
async fn a_server_on_ironwood_with_a_known_time_gives_the_derived_height() {
    // Transition at 4_000_000, tip far beyond: height is the transition plus the settling margin.
    let status = discovered(IRONWOOD_BRANCH, ARRR_TIP, Some(time_of_transition(4_000_000))).await;
    assert_eq!(status, Ok(IronwoodStatus::ActiveFrom(4_000_060)));
}

#[tokio::test]
async fn a_server_on_ironwood_without_a_time_is_taken_at_its_word() {
    // The test chain's case: no built-in time applies, the server says Ironwood is active.
    assert_eq!(
        discovered(IRONWOOD_BRANCH, ARRR_TIP, None).await,
        Ok(IronwoodStatus::ActiveFrom(ARRR_TIP as u32))
    );
}

#[tokio::test]
async fn a_server_before_ironwood_stays_not_activated_until_the_height_is_known() {
    let t = time_of_transition(4_167_000);
    // The tip has not reached the activation time at all.
    assert_eq!(
        discovered(SAPLING_BRANCH, 4_166_999, Some(t)).await,
        Ok(IronwoodStatus::NotActivated)
    );
    // The transition block exists but is not yet 30 deep: the chain itself does not know the height.
    assert_eq!(
        discovered(SAPLING_BRANCH, 4_167_020, Some(t)).await,
        Ok(IronwoodStatus::NotActivated)
    );
    // Buried: the upcoming height is known, and the tip is still below it.
    assert_eq!(
        discovered(SAPLING_BRANCH, 4_167_030, Some(t)).await,
        Ok(IronwoodStatus::ActiveFrom(4_167_060))
    );
}

#[tokio::test]
async fn a_server_that_disagrees_with_the_timestamp_rule_is_refused() {
    let t = time_of_transition(4_167_000);
    let err = discovered(SAPLING_BRANCH, ARRR_TIP, Some(t)).await.unwrap_err();
    assert!(matches!(err, IronwoodDiscoveryError::Inconsistent(_)), "{err}");
}

#[tokio::test]
async fn an_unreachable_server_is_an_error_and_not_a_default() {
    let transport = Arc::new(FakeTransport::default());
    let err = discover(&backend_over(&transport, &["a", "b"]), Some(1))
        .await
        .unwrap_err();
    assert!(matches!(err, IronwoodDiscoveryError::Unavailable(_)), "{err}");
}
