//! Regression guard for the v2 RPC dispatcher routing contract.
//!
//! This guard exists because a namespaced RPC surface (`task::enable_utxo::init`) was once
//! silently missing from the dispatcher, causing GUI activation to fail with `NoSuchMethod`.
//! These tests assert that every canonical namespaced wire method we commit to keeps a routing
//! entry in `dispatcher.rs`, so a namespace router or arm cannot be removed unnoticed.
//!
//! Scope: this verifies that the *route exists* (the method string is matched and dispatched).
//! It deliberately does not exercise handler logic — that is covered by handler-level tests.
//! The canonical wire names are tracked in `docs/reloaded-rewrite/rpc-method-census.md`.

/// The full source of the v2 dispatcher. The method-name literals asserted below live only in
/// this guard file (not in `dispatcher.rs`), so the substring checks are not self-satisfying.
const DISPATCHER_SOURCE: &str = include_str!("dispatcher.rs");

/// Asserts that `prefix` has a `strip_prefix` router and that every `arm` is routed under it.
fn assert_namespace_routed(prefix: &str, arms: &[&str]) {
    assert!(
        DISPATCHER_SOURCE.contains(&format!("strip_prefix(\"{prefix}\")")),
        "v2 dispatcher lost the `{prefix}` namespace router (strip_prefix missing)"
    );
    for arm in arms {
        assert!(
            DISPATCHER_SOURCE.contains(&format!("\"{arm}\"")),
            "v2 dispatcher lost routing for `{prefix}{arm}`"
        );
    }
}

#[test]
fn task_namespace_methods_are_routed() {
    assert_namespace_routed("task::", &[
        "enable_utxo::init",
        "enable_utxo::status",
        "enable_utxo::user_action",
        "enable_utxo::cancel",
        "enable_qtum::init",
        "enable_qtum::status",
        "enable_qtum::user_action",
        "enable_qtum::cancel",
        "enable_z_coin::init",
        "enable_z_coin::status",
        "enable_z_coin::user_action",
        "enable_z_coin::cancel",
        "enable_lightning::init",
        "enable_lightning::status",
        "enable_lightning::user_action",
        "enable_lightning::cancel",
        "init_trezor::init",
        "init_trezor::status",
        "init_trezor::user_action",
        "account_balance::init",
        "account_balance::status",
        "create_new_account::init",
        "create_new_account::status",
        "create_new_account::user_action",
        "scan_for_new_addresses::init",
        "scan_for_new_addresses::status",
        "withdraw::init",
        "withdraw::status",
        "withdraw::user_action",
    ]);
}

#[test]
fn lightning_namespace_methods_are_routed() {
    assert_namespace_routed("lightning::", &[
        "channels::open_channel",
        "channels::close_channel",
        "channels::get_channel_details",
        "channels::get_claimable_balances",
        "channels::list_open_channels_by_filter",
        "channels::list_closed_channels_by_filter",
        "nodes::connect_to_node",
        "payments::generate_invoice",
        "payments::send_payment",
        "payments::get_payment_details",
        "payments::list_payments_by_filter",
    ]);
}

#[test]
fn preexisting_namespaces_remain_routed() {
    assert_namespace_routed("stream::", &[
        "balance::enable",
        "heartbeat::enable",
        "order_status::enable",
        "orderbook::enable",
        "swap_status::enable",
    ]);
    assert_namespace_routed("gui_storage::", &[
        "enable_account",
        "add_account",
        "delete_account",
        "get_accounts",
        "get_account_coins",
        "get_enabled_account",
        "set_account_name",
        "set_account_description",
        "set_account_balance",
        "activate_coins",
        "deactivate_coins",
    ]);
    assert_namespace_routed("experimental::staking::", &[
        "delegate",
        "undelegate",
        "claim_rewards",
        "query::delegations",
        "query::ongoing_undelegations",
        "query::validators",
    ]);
    assert_namespace_routed("experimental::1inch_v6_0::", &[
        "classic_swap_contract",
        "classic_swap_quote",
        "classic_swap_create",
        "classic_swap_liquidity_sources",
        "classic_swap_tokens",
    ]);
}
