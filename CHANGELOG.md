# Changelog

All notable changes to KDF Reloaded are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Pre-1.0 releases use the `0.MAJOR.MINOR-PRERELEASE.N` convention; expect breaking changes between alpha and beta.

## [Unreleased]

### Added

- **Z-coin (ARRR/ZHTLC) WASM support — R39.6.1.** The shielded coin module is now available on the `wasm32-unknown-unknown` target. `zcash_primitives` and `zcash_client_backend` are added as WASM dependencies. The sapling state cache is abstracted behind a `SaplingStateCacheOps` trait with a SQLite backend for native and an IndexedDB backend (mm2_db) for WASM. `MmCoinEnum::ZCoin` and the `z_coin` module are now gated for all targets. Transaction building (`gen_tx`/`send_outputs`) remains native-only pending WASM delivery of the sapling parameter files. Code: `mm2src/coins/z_coin/`, `mm2src/coins/Cargo.toml`, `mm2src/coins/lp_coins.rs`, `mm2src/coins/lp_coins_context.rs`.
- **Z-coin sync-parameter control — R39.6.2.** The `task::enable_z_coin::init` activation request accepts two optional throughput-tuning fields: `blocks_per_iteration` (u32, default 1) and `inter_iteration_interval_ms` (u64, default 0). A `sync_start` field accepting `{"type":"Height","data":<u32>}` or `{"type":"Date","data":"<YYYY-MM-DD>"}` is also accepted; height-based start is wired through the sync loop; date-to-height resolution is deferred. Code: `mm2src/coins_activation/src/z_coin_activation.rs`, `mm2src/coins/z_coin.rs`.
- **Tendermint `denom` / `decimals` / `ibc_channels` in `CoinProtocol`.** `CoinProtocol::TENDERMINT` now carries optional `denom`, `decimals`, and `ibc_channels` fields (R36.3.3), aligned with the komodo-coins config schema for ATOM-family coins. Code: `mm2src/coins/tendermint/`.
- **V1 swap and taker order-status SSE.** The SSE streaming infrastructure now emits live maker/taker swap and order-status events under the existing `stream::*` namespace. Code: `mm2src/mm2_main/`.
- **SSE streamer parity endpoints.** The CRD and implementation now cover `stream::disable`, `stream::fee_estimator::enable`, `stream::tx_history::enable`, and native non-Windows `stream::shutdown_signal::enable`, completing the currently bound `stream::*` parity surface. Code: `docs/reloaded-rewrite/10-sse-streaming.md`, `mm2src/mm2_main/`.
- **RPC-dump development build workflow.** A manual `dev-build-rpc.yml` workflow builds with the RPC dump feature set, and the cross-platform build workflows pass feature flags consistently on Linux, macOS, Windows, iOS, and Android. Code: `.github/workflows/`.

### Fixed

- **`CoinProtocol::NFT` variant accepted.** A permissive NFT variant is added to `CoinProtocol` so NFT-typed coin configs no longer fail deserialization. Code: `mm2src/coins/lp_coins.rs`.
- **NFT subsystem activation RPC parity.** `enable_nft` is routed as the explicit NFT subsystem activation method, with CRD coverage for the activation contract. Code: `docs/reloaded-rewrite/19-nft-module-layout.md`, `mm2src/mm2_main/`, `mm2src/coins/nft/`.
- **SSE activation response wire shape.** `stream::*::enable` success payloads now expose the upstream-compatible `streamer_id` field without the reloaded-only `active` boolean. Code: `docs/reloaded-rewrite/10-sse-streaming.md`, `mm2src/mm2_main/`.
- **ZHTLC `protocol_data` deserialization.** Bare `{"type":"ZHTLC","protocol_data":{...}}` configs now deserialize correctly; previously missing `protocol_data` support caused ARRR activation to fail. Code: `mm2src/coins/z_coin_activation.rs`.
- **Zcash consensus parameters sourced from `protocol_data` — R39.6.4.** The shielded builder now reads all network parameters (`consensus_params`, `check_point_block`, `z_derivation_path`) from `protocol.protocol_data` rather than hardcoded Zcash-mainnet constants. A ZHTLC coin with non-mainnet HRP/b58 prefixes or activation heights uses its declared parameters end-to-end. Code: `mm2src/coins/z_coin.rs`, `mm2src/coins_activation/src/z_coin_activation.rs`.
- **`{"type":"ETH"}` protocol config accepted again.** A regression caused standard ETH coins using the `{"type":"ETH"}` protocol object to fail activation; restored. Code: `mm2src/coins/lp_coins.rs`.
- **UTXO dynamic-fee trade preimage is consistent** for `UpperBound` vs `Exact` fee policies. Code: `mm2src/coins/utxo/`.
- **ETH `estimate_gas` insufficient-balance revert mapped to `NotSufficientBalance`** instead of a generic transport error. Code: `mm2src/coins/eth/`.
- **getrandom 0.3 `wasm_js` backend enabled on wasm32** so entropy sources compile correctly on the WASM target. Code: `Cargo.toml`.
- **sia-rust bumped to `0e65d62`** (null `V2StorageProof.proof` fix).
- **ARRR/ZCoin activation and light-mode scanning.** ZHTLC activation no longer panics through the task manager when a failing Electrum candidate is encountered; activation fails over across available Electrum servers. Light-mode activation creates the required Sapling cache, bounds shielded-history scanning, handles stale checkpoints, uses the Pirate-compatible lightwalletd gRPC package, supports TLS lightwalletd endpoints, and reports shielded wallet DB balances for ARRR instead of returning zero. Code: `mm2src/coins/`, `mm2src/coins_activation/`.
- **ARRR/ZCoin and direct-withdraw coin previews through `task::withdraw`.** ZCoin/ARRR shielded withdraws, plus BCH, QRC20, SLP, Solana/SPL, Sia, and Tendermint native/token withdraws, are routed through the task-withdraw API instead of failing preview/status with `CoinDoesntSupportInitWithdraw` when clients use the task path. Lightning remains intentionally unsupported by withdraw because invoices are the payment entrypoint. Code: `mm2src/coins/rpc_command/init_withdraw.rs`, `mm2src/coins/z_coin.rs`.
- **ARRR/ZCoin light-mode shielded withdrawal construction.** Shielded ARRR withdraw previews in light mode now build spends from the scanned `ARRR_WALLET.db` note/witness data instead of entering the native zcashd `ZRpcOps` path, preventing preview-time KDF crashes on Electrum-backed activations. Code: `mm2src/coins/z_coin/`.
- **Orderbook address handling for Tendermint/Sia/Solana-family configs.** Orderbook and best-orders rendering no longer panic when a P2P order references non-UTXO protocol configs encountered by multi-coin wallet sessions. Tendermint addresses are derived from the order pubkey where possible; unsupported address families now return structured per-order errors and are skipped instead of crashing KDF. Code: `mm2src/mm2_main/src/lp_ordermatch/`.
- **Legacy maker-swap refund event compatibility.** Saved swap files using the historical `MakerPaymentRefundStarted` event name now deserialize as `MakerPaymentWaitRefundStarted`, so old refund-path swaps no longer disappear from swap history/status with an unknown-variant error. Code: `mm2src/mm2_main/src/lp_swap/`.
- **Legacy swap instruction-event compatibility.** Saved maker/taker swap files whose `MakerPaymentInstructionsReceived` or `TakerPaymentInstructionsReceived` event omitted the optional `data` field now deserialize as `None`, preventing repeated `missing field data` errors in swap history/status polling. Code: `mm2src/mm2_main/src/lp_swap/`.
- **Ordermatch trie-delta removal test made deterministic.** The orderbook sync test no longer depends on nondeterministic ordering when asserting a delta after removed orders. Code: `mm2src/mm2_main/src/ordermatch_tests.rs`.
- **Coin activation/runtime hardening for multi-asset wallets.** ERC20/BEP20 tokens with missing or zero config decimals now fall back to on-chain `decimals()`, NFT activation lazily opens the native async SQLite store instead of failing with `async_sqlite_connection is not initialized`, and Tendermint RPC node selection falls back from `/health` to `abci_info` before declaring all nodes unavailable. Code: `mm2src/coins/`, `mm2src/coins_activation/`.
- **Runtime stubs converted to explicit no-op/errors where safe.** Unsupported TRON V1 swap paths now return structured errors, L2 SQL transaction-history queries return an unsupported-history error instead of panicking, wasm crash-report initialization is an intentional no-op, and public-key trait methods for enabled coin families no longer panic when called by production paths. Code: `mm2src/coins/`, `mm2src/common/`.

### Changed / dependencies

- **`rand` 0.7 → 0.8** across all direct reloaded usages (RUSTSEC-2026-0097; upstream-blocked advisory). Code: multiple crates.
- **`mm2_metrics` rewritten** with a hand-rolled Prometheus registry; drops the dead `metrics-runtime 0.13` / `metrics-util` stack, clearing RUSTSEC-2021-0113. Code: `mm2src/mm2_metrics/`.
- **`anyhow` bumped to 1.0.103, `crossbeam-epoch` to 0.9.20** (advisory clears). Code: `Cargo.toml`.
- **CI test failures now include Rust backtraces.** `tests.yml` sets `RUST_BACKTRACE=1` for test jobs so panics provide actionable stack traces in CI logs. Code: `.github/workflows/tests.yml`.

## [0.1.0-beta.2] — 2026-07-07

Stability release focused on HD-wallet interoperability with the Komodo DeFi wallet (Flutter SDK). Continues from `0.1.0-beta.1` under GPLv2-only.

### Fixed

- **HD-wallet login no longer hangs on a spinner.** `RpcTaskStatus` now serialises the terminal state as `status: "Ok"` / `status: "Error"` (tagged `status`/`details`) instead of the previous `Ready` envelope, matching what the SDK's task-status polling expects. Applies to `task::enable_utxo`, `task::account_balance`, `task::create_new_account`, `task::withdraw` and other long-running task-status responses. Code: `mm2src/rpc_task/`.
- **Concurrent coin activation no longer throws `CoinIsAlreadyActivated`.** A coin already registered under a racing activation request is now treated as an idempotent success rather than surfacing an uncaught exception to the client. Code: `coins_activation/src/standalone_coin/init_standalone_coin.rs`.
- **HD balance responses are ticker-keyed.** `HDAccountBalance.total_balance` and `HDAddressBalance.balance` are now `{ "<TICKER>": { spendable, unspendable } }` maps, fixing the wallet's `PUBKEY_ACTIVATION_ERROR` / `"String is not a subtype of Map"` on HD activation. The legacy `my_balance` response is unchanged (remains a flat object). Code: `coins/coin_balance.rs` and HD balance/activation paths.

### Changed / divergences

- **Withdraw with an omitted HD `from` defaults to the enabled address** (account 0 / external / index 0) instead of failing with `FromAddressNotFound`. This is a deliberate divergence from GLEEC KDF, documented in [`RELOADED_VS_GLEEC.md`](RELOADED_VS_GLEEC.md); there is no compatibility switch in this release. An explicit but invalid `from` is still rejected.
- **Legacy `<db_root>/wallets/*.wallet` wallets are read (login/list/delete) but never written**; new wallets are always the canonical `<name>.json` record. Documented in [`RELOADED_VS_GLEEC.md`](RELOADED_VS_GLEEC.md) and CRD chapter 07 §7.5/§7.7 (R9A). The storage format does not by itself select HD vs single-address signing.

## [0.1.0-beta.1] — 2026-07-05

Initial public beta (first published pre-release). Continues the Komodo DeFi Framework codebase from the GPLv2 anchor commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` (2022-06-03) under GPLv2-only.

For the full feature delta against the GLEEC fork of the upstream codebase, see [`RELOADED_VS_GLEEC.md`](RELOADED_VS_GLEEC.md).

Highlights:

- Mainnet support on netid 8762 (AtomicDEX) and netid 6133 (GLEEC).
- Stable Rust toolchain (no nightly pin).
- Self-hosted CI with split format / unit-test / docker-test jobs.
- On-demand release builds for Linux (additional platforms via the umbrella dev-build workflow).
- Documentation-only compatibility convention for GLEEC-equivalent operation (developer rule in [`docs/COMPAT_SWITCHES.md`](docs/COMPAT_SWITCHES.md), central admin chapter in [`docs/GLEEC_COMPATIBILITY.md`](docs/GLEEC_COMPATIBILITY.md)); no divergent settings active in this release.
- Test-only netids (8100, 8999, 9000, 9998) gated behind the `regtest-netid` Cargo feature, off by default in production builds.
- EVM ABI encoding/decoding migrated from `ethabi` to `alloy` (`alloy-dyn-abi` / `alloy-json-abi` / `alloy-primitives`), guarded by a byte-identity golden regression suite.
- Tag-driven, GPG-signed release pipeline: signed `SHA256SUMS` manifest and drafted GitHub Releases — pre-releases from `staging` (`-alpha/-beta/-rc` tags), finals from `main`. Linux binaries build inside a Debian 11 container (glibc 2.31) for broad backwards-compatibility; unsigned all-platform snapshots via manual `dev-build.yml` and automatic `staging-build.yml`.

[Unreleased]: https://github.com/kdf-reloaded/kdf/compare/v0.1.0-beta.2...HEAD
[0.1.0-beta.2]: https://github.com/kdf-reloaded/kdf/compare/v0.1.0-beta.1...v0.1.0-beta.2
[0.1.0-beta.1]: https://github.com/kdf-reloaded/kdf/releases/tag/v0.1.0-beta.1
