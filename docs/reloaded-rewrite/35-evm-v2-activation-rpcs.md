# Chapter 35 -- EVM (Ethereum) V2 Activation & Token RPC Surface

**Status:** driving-spec (required port). This chapter specifies an **activation
RPC layer** over EVM coin code that already exists in reloaded; it does not
introduce a new coin.

> **One-sentence claim:** the project shall expose the standard Komodo DeFi
> Framework **mmrpc 2.0** activation surface for EVM (Ethereum-family) coins --
> a one-shot "platform coin with tokens" call, a single-token call (and its NFT
> alias), a long-running task variant of each, an on-chain token-info query, and
> read/write of the per-coin swap gas-fee policy -- on top of the EVM coin type
> that reloaded already ships, wire-compatible per method on request fields,
> success shape, `error_type` set, failure-to-`error_type` mapping, `error_data`
> shape, HTTP status and numeric encoding.

> **Treatment:** **T-PORT.** The EVM coin type and its legacy `enable` path are
> present in reloaded (see §35.7). What is required is the **activation
> integration** (the platform-coin-with-tokens and token activation trait
> implementations for the EVM coin/token, the task variants, and the dispatcher
> routes for the method strings named below). The wire contract distilled here
> is the source of truth for that port.

> **Binding scope (R36).** Requirements bind observable behaviour, the public
> mmrpc-2.0 method strings, their request/response JSON field names and types,
> the `error_type` discriminants, the JSON shape of each `error_data`, the HTTP
> status of each mmrpc-level error, and externally *dictated* interop (EVM
> JSON-RPC, EIP-1559 fee semantics, EIP-55 address checksums, ERC-20 contract ABI
> calls, CAIP-style chain identification). The human-readable `error` text,
> `error_path` and `error_trace` are **not** bound. Private Rust types, helper
> decomposition, and internal module structure are informative and are **not**
> bound by this chapter.

> **Sources of truth (binding order).**
> 1. The public Komodo DeFi Framework API reference
>    (`komodoplatform.com/en/docs/komodo-defi-framework/api/`: the v20
>    `coin_activation/enable_eth_with_tokens`, `coin_activation/enable_erc20`,
>    `coin_activation/task_managed/enable_eth`, `non_fungible_tokens/enable_nft`
>    pages, `common_structures/{activation,wallet,nfts,error_types}` and the v20
>    index) **governs where it is explicit**: a field table and its example agree,
>    or an example stands uncontradicted.
> 2. Where the reference is silent, ambiguous or contradicts itself -- including
>    every HTTP status code, which the reference never states -- the observable
>    behaviour of the **GLEEC KDF reference version `v3.0.0-beta`** (the most
>    recent GLEEC release tag available, dated 2026-04-01) governs. That behaviour
>    was established by the KDF Spec Reader and is stated here in behavioural
>    terms only.
> 3. Where the legacy reference `v2.6.0-beta` (netid 8762, AGENTS.md §3) differs
>    on an activation wire shape, an `Upstream divergence (informative)` note
>    records it; the two are not blended.
>
> Each request-field row and error row below carries a **Basis** marker:
> **D** = the public reference is explicit; **G** = the reference is silent,
> ambiguous or self-contradictory and the reference version decides; **D/G** =
> the reference is explicit about part of the contract and the reference version
> decides the rest (the row says which part).

---

## 35.0 Executive summary

EVM coins follow the project's **platform-coin-with-tokens** activation model:
the native gas coin (ETH and other EVM chains) is the *platform coin*, and ERC-20
contracts are its *child tokens*. The NFT pseudo-coin of a platform (ch. 19) is
activated either inline with the platform (`nft_req`, R35.1.4b) or afterwards
through the single-token handler (`enable_nft`, R35.2.4). The surface specified
here is:

| Method string | Tier | Targets | Purpose |
| --- | --- | --- | --- |
| `enable_eth_with_tokens` | mmrpc 2.0 | all (incl. WASM) | activate the EVM platform coin plus an inline batch of ERC-20 tokens (and optionally its NFT pseudo-coin) in one call |
| `enable_erc20` | mmrpc 2.0 | all (incl. WASM) | activate one ERC-20 token against an already-active EVM platform |
| `enable_nft` | mmrpc 2.0 | all (incl. WASM) | alias of the single-token handler for the NFT pseudo-coin (NFT specifics: ch. 19) |
| `task::enable_eth::{init,status,user_action,cancel}` | mmrpc 2.0 (task) | all (incl. WASM) | long-running variant of `enable_eth_with_tokens` |
| `task::enable_erc20::{init,status,user_action,cancel}` | mmrpc 2.0 (task) | all (incl. WASM) | long-running variant of `enable_erc20` (§35.3A) |
| `get_token_info` | mmrpc 2.0 | all | read an ERC-20 contract's on-chain info (symbol, decimals) |
| `get_swap_gas_fee_policy` | mmrpc 2.0 | all | read the per-coin swap gas-fee policy |
| `set_swap_gas_fee_policy` | mmrpc 2.0 | all | set the per-coin swap gas-fee policy |

**Shared conventions (binding).**

- **Envelope.** All methods use the mmrpc-2.0 request envelope with the method's
  fields directly in `params` (there is no `activation_params` wrapper on the
  platform or task-platform methods). Success is
  `{"mmrpc":"2.0","result":...,"id":...}`; errors use the error envelope
  `error, error_path, error_trace, error_type, error_data, id` with
  `error_type`/`error_data` adjacently tagged and `error_data` **absent** for a
  unit discriminant. The envelope, the `id` echo, and the HTTP-status policy are
  bound once in [Chapter 55](55-coin-activation-wire-contract.md) §55.1.
- **Request-parse failures.** A request that fails to deserialize -- a missing
  required field, a field of the wrong JSON type, an unknown enum value (for
  example `rpc_mode: "Metamask"` or `priv_key_policy: {"type":"Metamask"}` on a
  native build), a malformed address string -- is rejected before activation
  starts with the dispatcher-level `InvalidRequest` discriminant, `error_data` a
  string, HTTP 400 (ch. 55 §55.1). Unrecognised extra fields are **ignored**.
- **Task framework.** `status`, `user_action` and `cancel` of every `task::`
  family in this chapter use the shared task-framework contract of ch. 55 §55.2
  (task-status envelope, `NoSuchTask` / `TaskFinished` with integer
  `error_data`, `user_action`/`cancel` success `"success"`). This chapter binds
  only the per-family `init` request, in-progress / awaiting values, terminal
  result, and terminal error set.
- **Numeric encoding.** Every balance and amount (`spendable`, `unspendable`,
  NFT `amount`) is a JSON **string** holding a decimal number; there are no
  `*_rat` / `*_fraction` companions anywhere in activation. `current_block`,
  `required_confirmations`, `task_id`, `account_index`, `gap_limit`,
  `min_addresses_number`, `gas_station_decimals` and `mm2` are JSON integers.
- **Addresses.** Wallet addresses used as map keys and in `address` fields are
  0x-prefixed EIP-55 checksummed strings (TRON addresses for a TRX platform:
  ch. 21).

---

## 35.1 `enable_eth_with_tokens` -- platform coin with tokens

### 35.1.A Request

R35.1.1 The public RPC `enable_eth_with_tokens` shall activate an EVM platform
coin together with an inline list of ERC-20 tokens in a single mmrpc-2.0 call.
Its `params` object shall accept exactly the fields below (plus any unrecognised
field, which is ignored). "Absent" means the key is missing or `null`.

| Field | JSON type | Required / default | Basis | Effect |
| --- | --- | --- | --- | --- |
| `ticker` | string | required | D | Platform coin ticker. Its coins-config `protocol` must be the EVM platform protocol (`{"type":"ETH","protocol_data":{"chain_id":<integer>}}`, §35.5) or the TRON platform protocol of ch. 21. |
| `nodes` | array of CoinNode | documented as required; absent is treated as empty | D/G | EVM JSON-RPC endpoints, all caller-supplied (no endpoint is embedded). Empty or absent → `AtLeastOneNodeRequired` (G). |
| `nodes[].url` | string | required | D | Endpoint URL. Accepted schemes: `http`, `https` (HTTP JSON-RPC) and `ws`, `wss` (WebSocket JSON-RPC); the transport is selected by the scheme of `url` itself. An unparseable URL or any other scheme → `InvalidPayload` (G). TRX platform: HTTP(S) only (ch. 21). |
| `nodes[].komodo_proxy` | boolean | default `false` | D (examples) | The endpoint sits behind the Komodo signed proxy; requests to it carry the proxy authentication signature derived from the node's P2P identity. The reference's CoinNode *table* names this flag `komodo_auth`; every example and the reference version use `komodo_proxy` (see erratum note). `komodo_auth` is not recognised (ignored). |
| `nodes[].ws_url` | string | ignored | G | Appears in reference examples next to an HTTP `url`; the reference version does not consume it. Shall be accepted and ignored. |
| `erc20_tokens_requests` | array of TokensRequest | required (may be `[]`) | D | Tokens to activate with the platform. Absent → `InvalidRequest`. |
| `erc20_tokens_requests[].ticker` | string | required | D | Token ticker. |
| `erc20_tokens_requests[].required_confirmations` | integer | default: token config value, else the platform's effective value | G | Swap confirmations for this token. The reference table states a default of `3`; the reference version falls back as stated here. |
| `erc20_tokens_requests[].protocol` | coin-protocol object (§35.5) | optional | G | Inline protocol for a **custom token** that is not in the coins config (R35.2.7). |
| `swap_contract_address` | string (0x-hex EVM address) | documented as required; reference version: required unless the coin config marks the coin `wallet_only` | D/G | V1 swap (HTL) contract. Absent for a trading-capable coin, or the zero address → `InvalidPayload`. Ignored for a wallet-only coin. Malformed hex → `InvalidRequest`. |
| `fallback_swap_contract` | string (0x-hex EVM address) | documented as required; reference version: optional | D/G | Fallback V1 swap contract. The zero address → `InvalidPayload` (trading-capable coins). |
| `swap_v2_contracts` | object | optional; mandatory for a trading-capable coin when the node configuration enables trading protocol v2 (`use_trading_proto_v2`) | D/G | Members `maker_swap_v2_contract`, `taker_swap_v2_contract`, `nft_maker_swap_v2_contract`, each a required 0x-hex address string (ch. 17). Missing when mandatory, or any member the zero address → `InvalidPayload`; a missing member → `InvalidRequest`. |
| `contract_supports_watchers` | boolean | default `false` | G | Whether the V1 swap contract supports the watcher protocol. |
| `required_confirmations` | integer | default: coin config `required_confirmations`, else `3` | D/G | Swap confirmations for the platform coin. Maintainer decision (2026-10-11): config, else `3`. The docs table states `3`; the reference version uses config, else `1`; the docs' own BCH page states this config-then-3 rule. Legacy EVM `enable` / `electrum` (ch. 55 §55.4) keep their own default. |
| `requires_notarization` | boolean | default `false` | D | No effect on EVM coins. |
| `mm2` | integer | optional | D/G | Accepted as an unsigned 8-bit integer and not otherwise enforced (G); a non-integer or out-of-range value → `InvalidRequest`. |
| `tx_history` | boolean | default `false` | D | When `true`, background transaction-history preloading for the platform starts after activation (the documented effect). |
| `gas_station_url` | string | optional | D (examples) | Gas-station endpoint used for legacy gas-price estimation. |
| `gas_station_decimals` | integer | default `8` | D | Decimals that denominate the gas-station response in gwei. |
| `gas_station_policy` | object `{"policy": "MeanAverageFast" \| "Average"}` | default `MeanAverageFast` | D | How the gas price is derived from the station response. |
| `swap_gas_fee_policy` | string `Legacy` \| `Low` \| `Medium` \| `High` | default: coin config value, else `Legacy` | G | Initial swap gas-fee policy of the coin (§35.6). Unknown value → `InvalidRequest`. |
| `rpc_mode` | string `Default` \| `Metamask` | default `Default` | D | `Metamask` exists only on the WASM target (R35.1.4a). |
| `priv_key_policy` | tagged object | default `{"type":"ContextPrivKey"}` | D/G | Signing policy (R35.1.4). |
| `get_balances` | boolean | default `true` | D | When `false`, single-address results omit balances (R35.1.3). |
| `nft_req` | object | optional | D/G | Inline NFT activation (R35.1.4b). |
| `path_to_address` | object `{account_id: integer, chain: "External" \| "Internal", address_id: integer}` | default `{account_id: 0, chain: "External", address_id: 0}` | D/G | HD only: which derived address becomes the enabled (swap) address. The reference describes `chain` as an integer but its examples and the reference version use the strings. |
| `gap_limit` | integer | default `20` | D/G | HD only: address gap limit. |
| `scan_policy` | string `do_not_scan` \| `scan_if_new_wallet` \| `scan` | default `scan_if_new_wallet` | D/G | HD only: address-scan policy at activation (semantics as ch. 38 §38.8). |
| `min_addresses_number` | integer | optional | D | HD only: minimum known external addresses per account (semantics as ch. 38 R38.8.5). |

R35.1.1a The fields `tx_history`, `requires_notarization`, `gas_station_url`,
`gas_station_decimals` and `gas_station_policy` are not consumed by the
reference version at all. A malformed value in any of these five fields shall
**not** fail activation; the field is then treated as absent. When well-formed,
`tx_history` and the gas-station fields shall have the documented effect.

### 35.1.B Activation flow and failure precedence

R35.1.2 Activation shall proceed through the following phases, and when a
request has several faults the reported error shall be the one of the earliest
failing phase:

1. **Policy gate (one-shot only):** a hardware (`Trezor`) `priv_key_policy` is
   rejected (`UnexpectedDeviceActivationPolicy`).
2. **Already active:** `ticker` is already an active coin
   (`PlatformIsAlreadyActivated`).
3. **Platform config:** config entry present and its protocol parseable and of
   the EVM/TRON platform kind (`PlatformConfigIsNotFound`,
   `CoinProtocolParseError`, `UnexpectedPlatformProtocol`). An EVM platform
   protocol without an integer `chain_id` is a protocol parse failure.
4. **Platform construction:** signing-policy preparation (MetaMask session,
   WalletConnect session, hardware context), swap-contract validation, key /
   address derivation (including HD `path_to_address` and the config
   `derivation_path`), node-list validation, and `rpc_mode` / policy consistency.
   No network round trip to the EVM nodes is required in this phase.
5. **Tokens**, batch-wise in three passes, each pass covering every requested
   token in request order before the next pass starts: (a) config/protocol
   resolution (or custom-token validation) of all tokens; (b) platform
   consistency (`PlatformCoinMismatch`) of all tokens; (c) construction of each
   token in turn. The first failing token of the earliest failing pass decides
   the error. A token whose config lacks `decimals` (or declares `0`) has its
   decimals read from the contract on-chain during its construction.
6. **Inline NFT** (when `nft_req` is present).
7. **Result:** chain height, public key, balances (or token tickers) and the NFT
   summary.
8. **Registration** of the platform, its tokens and its NFT pseudo-coin; a
   concurrent registration of the same ticker yields
   `PlatformIsAlreadyActivated`.

A failure in any phase shall leave neither the platform coin nor any of its
tokens or NFT pseudo-coin registered (no partial activation). Activation shall
succeed when, for every request it needs, at least one of the supplied nodes
answers (multi-node failover); a subset of unresponsive nodes is not an error.

### 35.1.C Success result

R35.1.3 The success `result` is one of two shapes, selected by the platform's
derivation mode and distinguished structurally (untagged union); a client
matches on the fields present.

**(a) Single-address shape** -- used for the context key in Iguana mode and for
the MetaMask and WalletConnect policies:

| Field | JSON type | Presence | Content |
| --- | --- | --- | --- |
| `current_block` | integer | always | Current chain height. |
| `eth_addresses_infos` | object (map) | always | Exactly one entry, keyed by the wallet address, value an AddressInfo whose `balances` (when present) is a single BalanceInfo `{spendable, unspendable}` of the platform coin. |
| `erc20_addresses_infos` | object (map) | always | Exactly one entry, keyed by the same address, value an AddressInfo whose `balances` (when present) is a map *token ticker → BalanceInfo* covering every activated token (an empty object when no tokens were requested). |
| `nfts_infos` | object (map) | **always** | NFT summary: `{}` when `nft_req` was not supplied; otherwise a map keyed `"<token_address>,<token_id>"` whose values carry `token_address`, `token_id` (decimal string), `chain`, `contract_type`, `amount` (decimal string) as bound by ch. 19 §19.6.2. |

AddressInfo carries `derivation_method` (`{"type":"Iguana"}` in this shape),
`pubkey`, and then, depending on `get_balances`:

- `get_balances: true` (default): `balances` is present on both entries;
  `tickers` is absent.
- `get_balances: false`: `balances` is **omitted** from both entries; the
  `erc20_addresses_infos` entry carries `tickers`, a JSON array of the activated
  token tickers (set semantics, order not significant); the
  `eth_addresses_infos` entry carries neither `balances` nor `tickers`.

`pubkey` is the uncompressed secp256k1 public key of the wallet address as
lower-case hex **with** the leading `04` byte and **without** a `0x` prefix
(130 hex characters).

**(b) HD shape** -- used whenever the platform's address is derived
hierarchically: the daemon runs in HD (mnemonic) mode, or the `Trezor` policy
(task path only):

| Field | JSON type | Content |
| --- | --- | --- |
| `current_block` | integer | Current chain height. |
| `ticker` | string | The platform ticker. |
| `wallet_balance` | object | HD wallet-balance report: `{"wallet_type":"HD","accounts":[...]}`; each account carries `account_index` (integer), `derivation_path` (string), `total_balance` (map ticker → BalanceInfo) and `addresses` (array of `{address, derivation_path, chain, balance}` with `balance` a map ticker → BalanceInfo). Each balance map covers the platform ticker and every activated token ticker. Report shape and account/scan semantics are those of the HD activation report shared with the UTXO task family (ch. 55, ch. 38 §38.8). |
| `nfts_infos` | object (map) | **Always** present; `{}` (inline NFT activation is not available in HD mode, R35.1.4b). |

In the HD shape `get_balances` has no effect (balances are always reported)
(G: the reference shows no HD example with `get_balances: false`; the reference
version ignores the flag in HD mode).

The success HTTP status is 200.

### 35.1.D Signing policy, RPC mode, inline NFT

R35.1.4 `priv_key_policy` is an **object** tagged by `type`, with any payload in
a sibling `params` object (G for the payload key; see erratum). A bare string
(for example `"priv_key_policy": "Trezor"`) is a request-parse failure
(`InvalidRequest`). The variants are:

| `type` | `params` | Targets | Path | Effect |
| --- | --- | --- | --- | --- |
| `ContextPrivKey` (default) | none | all | one-shot and task | The daemon's own key: single-address in Iguana mode; HD derivation at the coin config `derivation_path` and `path_to_address` in HD mode. |
| `Trezor` | none | native (hardware support) | **task only** (R35.3.2) | Address and public key come from the connected device; HD shape. On the one-shot call → `UnexpectedDeviceActivationPolicy`. |
| `Metamask` | none | **WASM only** | one-shot and task | Signer is the connected MetaMask session (ch. 47); requires `rpc_mode: "Metamask"` (R35.1.4a). On a native build the value does not exist → `InvalidRequest`. |
| `WalletConnect` | `{"session_topic": "<hex string>"}` | all | one-shot and task | Signer is the external wallet of an established WalletConnect session (ch. 22, `wc_new_connection`); the address and public key are obtained from that wallet during activation; single-address shape. |

Reloaded may additionally accept `Iguana` / `IguanaPrivKey` as aliases of
`ContextPrivKey`.

R35.1.4a `rpc_mode` selects the JSON-RPC transport. `Default` uses `nodes`.
`Metamask` (WASM only) routes chain requests through the connected MetaMask
provider. On WASM, `rpc_mode` and `priv_key_policy` shall be **both** MetaMask
or **neither**; any other combination fails with `PlatformCoinCreationError`.
Under MetaMask: no connected session, or an active-account mismatch →
`Transport`; the provider reports the chain as not registered, or any other
provider failure while selecting the chain → `PlatformCoinCreationError`; the
provider answers with a chain id other than the coin's → `Internal`. A TRX
platform with `rpc_mode: "Metamask"` → `Internal`.

R35.1.4b `nft_req` activates the platform's NFT pseudo-coin (`NFT_<chain>`,
ch. 19) as part of the same call. Accepted forms:

- the reference-version form `{"provider": NftProvider}`, and
- the documented form, a bare `NftProvider` (G/D: the reference's table and
  example agree on the bare form, but the reference version requires the
  `provider` wrapper; both shall be accepted).

`NftProvider` is `{"type":"Moralis","info":{"url":<string URL>,"komodo_proxy":<boolean, default false>}}`
(ch. 19 §19.6.2). An unknown provider `type` or a syntactically invalid `url` is
a request-parse failure (`InvalidRequest`). Inline NFT activation performs the
same initial owned-inventory fetch as `enable_nft` for the platform's single
address and reports it in `nfts_infos`. Failures:

| Condition | `error_type` | HTTP |
| --- | --- | --- |
| Platform address is HD-derived (HD mode or hardware policy) | `UnexpectedDerivationMethod` | 500 |
| Platform ticker does not map to one of the NFT chains of ch. 19 §19.3, or the platform is TRX | `Internal` | 500 |
| Provider unreachable, non-success HTTP status, or unparseable envelope | `Transport` | 502 |
| Provider rejected the request as invalid, or an inventory entry lacks its contract type | `InvalidPayload` | 400 |
| Local storage or other internal failure | `Internal` | 500 |

An inline-NFT failure fails the whole activation (R35.1.2). In HD mode the
reference directs clients to activate NFTs with `enable_nft` after activation
instead (D).

### 35.1.E Error contract

R35.1.5 The `error_type` set, failure mapping, `error_data` shape and HTTP status
of `enable_eth_with_tokens` shall be exactly the following. All rows are
mmrpc-level for the one-shot call; for `task::enable_eth` see R35.3.5 for
placement.

| `error_type` | Failure class | `error_data` | HTTP | Basis |
| --- | --- | --- | --- | --- |
| `PlatformIsAlreadyActivated` | `ticker` already active, or lost a concurrent registration race | string: the ticker that is already active | 400 | D |
| `PlatformConfigIsNotFound` | no coins-config entry for `ticker` | string: `ticker` | 400 | D |
| `CoinProtocolParseError` | platform config `protocol` missing or unparseable (including an EVM protocol without integer `chain_id`) | object `{ticker: string, error: string}` | 500 | D |
| `UnexpectedPlatformProtocol` | platform protocol parses but is not an EVM/TRON platform protocol | object `{ticker: string, protocol: object}` -- `protocol` is the serialized coin protocol (`type`, plus `protocol_data` where the variant has it) | 400 | D |
| `TokenConfigIsNotFound` | a requested token without inline `protocol` has no config entry | string: the token ticker | 400 | D |
| `TokenProtocolParseError` | a token's config `protocol` missing or unparseable | object `{ticker: string, error: string}` | 500 | D |
| `UnexpectedTokenProtocol` | a token's protocol is not ERC-20 (TRC-20 under a TRX platform), or its `contract_address` is not a 0x-prefixed address in exact EIP-55 checksummed form | object `{ticker: string, protocol: object}` (serialized coin protocol incl. `protocol_data`) | 400 | D |
| `PlatformCoinMismatch` | a token's protocol names a platform other than `ticker` | absent | 400 | G |
| `CustomTokenError` | an inline token `protocol` was rejected (R35.2.7) | the custom-token sub-error, externally tagged (R35.2.7) | 500 | G |
| `AtLeastOneNodeRequired` | `nodes` empty or absent | string (human-readable) | 400 | G |
| `InvalidPayload` | node URL unparseable or of an unsupported scheme; swap contract absent for a trading-capable coin or zero; `swap_v2_contracts` mandatory but absent, or a member zero; fallback contract zero; coin config `derivation_path` unusable for an HD or hardware policy; `path_to_address` not derivable; inline-NFT provider rejected the request | string | 400 | G |
| `UnexpectedDeviceActivationPolicy` | `Trezor` policy on the one-shot call | absent | 400 | G |
| `WalletConnectError` | WalletConnect policy: WalletConnect context unavailable, unknown/expired session, relay failure, or the wallet declined to reveal its account | string | 400 | G |
| `PlatformCoinCreationError` | WASM MetaMask: `rpc_mode`/policy mismatch, or the provider cannot select the coin's chain | object `{ticker: string, error: string}` | 500 | D (shape from the reference's sibling platform pages) / G (mapping) |
| `UnexpectedDerivationMethod` | inline NFT or a token step required a single address but the platform is HD-derived | string (human-readable) | 500 | D (name) / G |
| `PrivKeyPolicyNotAllowed` | the selected signing policy cannot perform an operation activation needs (no such path exists in the reference version; reserved) | string | 500 | G (the method table spells it `PrivKeyNotAllowed`; the error catalogue and the reference version use `PrivKeyPolicyNotAllowed`) |
| `FailedSpawningBalanceEvents` | reserved (balance-event streaming could not be started) | string | 400 | G |
| `Transport` | the EVM nodes (or the MetaMask provider, or the inline-NFT provider) gave no usable answer -- R35.1.6 | string (human-readable) | 502 | D |
| `Internal` | hardware-policy failures (R35.3.2); HD wallet storage failure; inline-NFT chain unsupported; MetaMask chain-id mismatch; any other unexpected failure | string (human-readable) | 500 | D (name/shape) / G (mapping) |
| `NoSuchTask` | task variant only, framework-originated | integer: task id | 400 | G |
| `TaskTimedOut` | task variant only, framework-originated timeout | object `{duration: {secs: integer, nanos: integer}}` | 500 | G |

The reference's Error Types table lists `InternalError`; the discriminant on the
wire is `Internal` (G: the catalogue and the reference version agree on
`Internal`).

R35.1.6 **Backend failure mapping (binding).** For every point at which
activation needs an answer from a backend, the outcome shall be:

| Failure point | Condition | `error_type` | `error_data` | HTTP | Basis |
| --- | --- | --- | --- | --- | --- |
| Any node-reachability probe the implementation performs before use | no node answers | `Transport` | string | 502 | D |
| Chain-height read (phase 7) | no node returns a usable answer (connection failure, timeout, HTTP non-success status such as 502, JSON-RPC error, unparseable response) | `Transport` | string | 502 | D |
| On-chain token `decimals()` read (phase 5) | as above | `Transport` | string | 502 | G |
| Platform or token balance read, single-address or HD (phase 7) | as above | `Transport` | string | 502 | G |
| Inline-NFT inventory fetch (phase 6) | provider unreachable / non-success / unparseable | `Transport` | string | 502 | G |
| MetaMask provider (WASM) | no session / account mismatch / provider error on a chain request | `Transport` | string | 502 | G |
| WalletConnect relay or wallet | session/relay failure | `WalletConnectError` | string | 400 | G |
| Some, not all, nodes unresponsive | at least one node answers each request | success | -- | 200 | G |

The mandatory case -- **every supplied node unresponsive** (for example every
node answering HTTP 502) -- shall produce `Transport` with a **string**
`error_data` and HTTP 502, regardless of which phase first touches the network.
The reference's example for this case carries the message "Failed to get client
version for all nodes" (public-docs citation; the message text is not bound).

> **Upstream divergence (informative).** In the reference version
> `v3.0.0-beta` no reachability probe happens before use, and a chain-height
> read that gets no usable answer is reported as `Internal` (HTTP 500); with
> every node down and all token decimals present in config, that is the first
> network failure, so the reference version answers `Internal` where the public
> reference documents `Transport`. The public reference is explicit for this
> case and governs. In `v2.6.0-beta` an on-chain decimals read that gets no
> usable answer is reported as `Internal`; `v3.0.0-beta` reports it as
> `Transport`, which this chapter binds.

> **Upstream divergence (informative).** `v2.6.0-beta` treats
> `swap_contract_address` as mandatory at request-parse level for every coin (an
> absent value is `InvalidRequest`, a zero value `InvalidPayload`);
> `v3.0.0-beta` accepts its absence for a wallet-only coin and reports its
> absence for a trading-capable coin as `InvalidPayload`. This chapter binds the
> `v3.0.0-beta` behaviour; both answer HTTP 400 for a trading-capable coin.

> **Public-docs erratum (informative).** The public reference (a) names the
> proxy flag `komodo_auth` in its CoinNode table while every example uses
> `komodo_proxy`; (b) shows `ws_url`, which is not consumed; (c) links
> `swap_v2_contracts` to a table copied from TokensRequest -- the real members are
> those of its example; (d) documents `nfts_infos` as a "list" although every
> example (and the reference version) returns an object map; (e) shows the
> WalletConnect policy as `{"type":"WalletConnect","data":"<topic>"}` and types
> `data` as a list -- the reference version requires
> `{"type":"WalletConnect","params":{"session_topic":"<topic>"}}`, which this
> chapter binds (when EVM WalletConnect signing is implemented, the docs form
> shall be accepted alongside it, as ch. 36 R36.1.3 does for Tendermint by
> maintainer decision); (f) spells `PrivKeyNotAllowed` and `InternalError` in the method
> table; (g) states a default of `3` for `required_confirmations`; the reference
> version uses the coin config, else 1. By maintainer decision (2026-10-11) this
> chapter binds the coin config, else 3 -- the rule the docs' own BCH page
> states -- so a coin without a configured value waits for 3 confirmations, the
> safer of the two documented numbers; (h) marks
> `swap_contract_address` and `fallback_swap_contract` as required while the
> reference version accepts their absence as described in R35.1.1 -- accepting
> their absence is a superset that cannot break a client that sends them; (i) shows the
> bare `NftProvider` for `nft_req` while the reference version requires the
> `provider` wrapper -- both are accepted (R35.1.4b); (j) documents the
> HD-only `chain` as an integer while its examples use strings.

> **Upstream divergence (informative).** The reference version does not parse
> `tx_history`, `requires_notarization` or the gas-station fields of this method
> (no background history for an EVM platform activated this way, no gas-station
> pricing); this chapter keeps their documented effect (R35.1.1a), which changes
> no wire shape.

> **Reloaded status (informative).** Implemented on `fix/activation-api-parity`:
> the full request table, the R35.1.2 precedence including the three-pass token
> batch, both result shapes with `nfts_infos` and `get_balances`, the HD address
> at the coin's `derivation_path` plus `path_to_address` (Reloaded previously
> bound the daemon's internal identity key; see the upgrade note in
> `CHANGELOG.md`), the `04`-prefixed `pubkey`, the R35.1.5 error table and the
> R35.1.6 backend-failure mapping (every node unresponsive is `Transport`,
> string, 502). Remaining deviations:
> 1. The `WalletConnect` policy parses but answers `WalletConnectError` (400):
>    WalletConnect signing for EVM coins is not available in this build.
> 2. `ws://` / `wss://` node URLs are rejected as `InvalidPayload`, because the
>    node transport is HTTP-only.
> 3. `komodo_proxy` is parsed but requests are not proxy-signed.
> 4. `contract_supports_watchers` is parsed only.
> 5. Under MetaMask (WASM) chain requests still go to `nodes`, not through the
>    provider.

---

## 35.2 `enable_erc20` -- single token on an active platform (and `enable_nft`)

R35.2.1 The public RPC `enable_erc20` shall activate a single ERC-20 token
against an already-active EVM platform coin. Its `params` shall carry:

| Field | JSON type | Required / default | Basis | Effect |
| --- | --- | --- | --- | --- |
| `ticker` | string | required | D | Token ticker. |
| `activation_params` | object | required (may be `{}`) | D/G | Per-token parameters. Absent → `InvalidRequest` (G). |
| `activation_params.required_confirmations` | integer | default: token config value, else the platform's effective value | D | Swap confirmations for the token. |
| `protocol` | coin-protocol object (§35.5) | optional | G | Inline protocol for a custom token (R35.2.7). |

R35.2.2 Token construction: the platform binding and contract address come from
the token's config protocol (or the inline `protocol`); `decimals` comes from the
token config and, when the config lacks it or declares `0`, is read from the
contract on-chain over the platform's nodes. The configured `contract_address`
must be a 0x-prefixed address in exact EIP-55 checksummed form.

R35.2.3 The success `result` shall carry:

| Field | JSON type | Content | Basis |
| --- | --- | --- | --- |
| `balances` | object (map) | exactly one entry: the platform's single wallet address → BalanceInfo `{spendable, unspendable}` of the token | D |
| `platform_coin` | string | the platform ticker | D |
| `token_contract_address` | string | the token contract address, 0x-prefixed **lower-case** hex | D |
| `required_confirmations` | integer | the token's effective swap confirmations | D |

There is no `get_balances` option on this method; the balance is always read.

> **Upstream divergence (informative).** `v3.0.0-beta` renders
> `token_contract_address` in EIP-55 checksummed (mixed-case) form;
> `v2.6.0-beta` and the public reference example use lower-case hex. The public
> example is uncontradicted and governs. For a TRC-20 token under a TRX platform
> the reference version renders the TRON base58 form (ch. 21).

R35.2.4 `enable_nft` is dispatched to the same single-token handler. The handler
selects the token kind from the resolved protocol: an ERC-20 (or TRC-20)
protocol takes the ERC-20 parameters above; an NFT protocol takes
`activation_params.provider` and returns `{nfts, platform_coin}` -- request and
result shapes, the provider descriptor, and the NFT activation behaviour are
bound by ch. 19 §19.6.2. The **wire error contract of `enable_nft` is the
single-token contract of R35.2.5**, with the NFT-specific failure mapping of
R35.2.8.

R35.2.5 The `error_type` set, failure mapping, `error_data` shape and HTTP status
of `enable_erc20` (and `enable_nft`) shall be exactly the following; all are
mmrpc-level.

| `error_type` | Failure class | `error_data` | HTTP | Basis |
| --- | --- | --- | --- | --- |
| `TokenIsAlreadyActivated` | `ticker` already active, or lost a concurrent registration race | string: the token ticker | 400 | D |
| `TokenConfigIsNotFound` | no config entry and no inline `protocol` | string: the token ticker | 400 | D |
| `TokenProtocolParseError` | token config `protocol` missing or unparseable | object `{ticker: string, error: string}` | 500 | G |
| `UnexpectedTokenProtocol` | protocol is not ERC-20 / TRC-20 / NFT, or the ERC-20 `contract_address` is not in exact EIP-55 form | object `{ticker: string, protocol: object}` | 400 | G |
| `CustomTokenError` | inline `protocol` rejected (R35.2.7) | externally tagged custom-token sub-error (R35.2.7) | 400 | G |
| `PlatformCoinIsNotActivated` | the platform named by the token protocol is not active | string: the **platform** ticker | 400 | D |
| `UnsupportedPlatformCoin` | the platform is active but is not an EVM platform coin | object `{platform_coin_ticker: string, token_ticker: string}` | 500 | G |
| `UnexpectedDerivationMethod` | the platform is HD-derived (HD mode or hardware policy); the one-shot token call needs a single address | the serialized derivation sub-error: the string `"ExpectedSingleAddress"` for this case | 500 | G |
| `Transport` | on-chain decimals read or token balance read got no usable answer from the nodes (R35.1.6 conditions); NFT provider unreachable | string | **500** | G |
| `InvalidPayload` | NFT provider rejected the request; an inventory entry lacks its contract type | string | 400 | G |
| `PrivKeyPolicyNotAllowed` | reserved (as R35.1.5) | string | 500 | G |
| `PlatformCoinMismatch` | reserved on this method | absent | 400 | G |
| `CouldNotFetchBalance` | reserved (balance failures are reported as `Transport`) | string | 500 | G |
| `InvalidConfig` | reserved | string | 500 | G |
| `Internal` | token kind and parameters disagree (an ERC-20 ticker with a well-formed `activation_params.provider`, an NFT ticker without a usable one); NFT chain unsupported; other unexpected failures | string | 500 | G |

Note the deliberate difference from R35.1.5: `Transport` on this method is HTTP
**500**, not 502, and `TokenProtocolParseError` is 500 while
`UnexpectedTokenProtocol` and `CustomTokenError` are 400 (G).

R35.2.6 The checks of `enable_erc20` shall be applied in this order: already
active → config/protocol (or custom-token validation) → platform active →
platform is EVM → token construction (custom-token duplicate-activation check,
decimals) → single-address requirement → balance read → registration. A failure
shall leave the token unregistered and shall not register the token's info on
the platform.

R35.2.7 **Custom tokens (G).** When `protocol` is supplied (here, or on an
`erc20_tokens_requests` entry of §35.1), the token is activated from that
protocol instead of from the coins config, as a wallet-only coin. It is rejected
with `CustomTokenError` when: the ticker already exists in the coins config;
the contract address is already configured under another ticker on the same
platform; a token with the same platform and contract address is already active;
or the contract address cannot be parsed. The `error_data` of `CustomTokenError`
is the externally tagged sub-error:

| Case | `error_data` |
| --- | --- |
| ticker already in config | `{"DuplicateTickerInConfig": {"ticker_in_config": string}}` |
| contract already configured | `{"DuplicateContractInConfig": {"ticker_in_config": string}}` |
| same contract already active | `{"TokenWithSameContractAlreadyActivated": {"ticker": string, "contract_address": string}}` |
| unparseable contract address | `"InvalidTokenAddress"` |

> **Reloaded status (informative).** Implemented (kdf-reloaded/kdf#16): a
> custom token reports itself wallet-only on both activation routes, so order
> placement (`buy`, `sell`, `setprice`) rejects it like any other wallet-only
> coin. Tests assert that a custom token is wallet-only and a configured token
> is not, and that order placement rejects a wallet-only coin. Remaining: a few
> order-book read paths (trade preimage, best orders, order book and its depth
> and events) test wallet-only status by looking the ticker up in the coins
> configuration, which a custom token does not have; they do not place orders,
> so no swap can start, but they can list or price a custom-token pair.

R35.2.8 **`enable_nft` failure mapping (G).** Against the single-token table:
an NFT ticker with no config entry → `TokenConfigIsNotFound`; NFT pseudo-coin
already active → `TokenIsAlreadyActivated` (string, the NFT ticker); backing
platform not active → `PlatformCoinIsNotActivated` (string, the platform
ticker); backing platform not EVM → `UnsupportedPlatformCoin`; platform
HD-derived → `UnexpectedDerivationMethod`; unsupported chain → `Internal`;
`activation_params` without a usable `provider` (absent, an unknown provider
`type`, or a syntactically invalid `url`) → `Internal` (500), not a
request-parse failure; provider unreachable / non-success / unparseable → `Transport` (500); provider
rejected the request → `InvalidPayload`.

> **Reloaded status (informative).** Implemented on `fix/activation-api-parity`:
> custom tokens, the token-construction mapping, lower-case
> `token_contract_address`, the token-config confirmation fallback, token
> registration on the platform only after success, the full error set, and
> `enable_nft` served by the single-token handler; custom tokens are wallet-only
> (R35.2.7). Remaining: a provider rejection maps to `InvalidPayload` only for an
> indexer HTTP 400.

---

## 35.3 `task::enable_eth::{init,status,user_action,cancel}` -- task variant

R35.3.1 EVM platform activation shall also be exposed as the task family
`task::enable_eth::init`, `task::enable_eth::status`,
`task::enable_eth::user_action`, `task::enable_eth::cancel`.

- `init` `params`: exactly the `enable_eth_with_tokens` fields of R35.1.1, flat
  in `params`, plus the optional task-framework `client_id` (ch. 55 §55.2).
  `priv_key_policy` is the **object** of R35.1.4; the reference's task-page table
  types it as a string, but a string is a request-parse failure in the reference
  version (G). All signing policies are accepted, including `Trezor`.
- `init` success: `{"task_id": <integer>}`, returned before activation
  completes.
- `status` terminal success: the `enable_eth_with_tokens` result of R35.1.3,
  byte-for-byte (single-address or HD shape, `nfts_infos` always present), in
  `details` of `{"status":"Ok","details":...}`.
- `user_action` and `cancel`: ch. 55 §55.2.

R35.3.2 **Trezor path.** Under `priv_key_policy: {"type":"Trezor"}`:

- a hardware-wallet context must already exist (established by
  `task::init_trezor`, ch. 55); the coin config must declare its hardware coin
  name (`trezor_coin`) and a usable `derivation_path`;
- the address and account public key are read from the device; the platform is
  HD-derived and the terminal result is the HD shape of R35.1.3(b) for the
  account/address selected by `path_to_address`;
- device interaction surfaces through `status` as the in-progress and awaiting
  values of R35.3.3, and PIN / passphrase are supplied through
  `task::enable_eth::user_action` with
  `{"action_type":"TrezorPin","pin":"<pin-matrix response>"}` or
  `{"action_type":"TrezorPassphrase","passphrase":"<passphrase>"}` (shared
  hardware user-action contract, ch. 55 / ch. 50).

Failure mapping (status-level, G): no hardware-wallet context, a coin config
without hardware support, a device error, a device-interaction timeout, or a
user action of the wrong kind (a passphrase while a PIN is awaited, or the
reverse; `user_action` itself still answers `"success"`, ch. 55 R55.2.11) →
`Internal` (string); an unusable `derivation_path` → `InvalidPayload` (string).

R35.3.3 **In-progress and awaiting values.** `status` with
`"status":"InProgress"` carries `details` as one of the strings
`ActivatingCoin`, `RequestingWalletBalance`, `Finishing`,
`WaitingForTrezorToConnect`, `FollowHwDeviceInstructions` (D: the reference's
task page lists exactly these). The initial value is `ActivatingCoin`. A device
button/confirmation request is reported as `FollowHwDeviceInstructions`. With
`"status":"UserActionRequired"`, `details` is the string `EnterTrezorPin` or
`EnterTrezorPassphrase`. Clients shall treat any in-progress value as progress
and keep polling; the implementation may omit phases that do not apply
(the reference version reports only `ActivatingCoin` plus the hardware values
for this family). The framework imposes no activation deadline of its own (ch. 48
§48.3).

R35.3.4 (Moved.) The single-token task family is specified in §35.3A.

R35.3.5 **Error placement (G).**

| Placement | `error_type` | Condition | `error_data` | HTTP |
| --- | --- | --- | --- | --- |
| `init`, mmrpc-level | `InvalidRequest` | request-parse failure | string | 400 |
| `init`, mmrpc-level | `PlatformIsAlreadyActivated` | `ticker` already active when `init` is called | string: ticker | 400 |
| `init`, mmrpc-level | `Internal` | the task could not be created | string | 500 |
| `status`, task-level (`"status":"Error"`, full error object in `details`) | every row of R35.1.5 except `UnexpectedDeviceActivationPolicy` (`PlatformIsAlreadyActivated` here only for a registration race after `init`) | as R35.1.5 (including the backend mapping of R35.1.6 and the Trezor mapping of R35.3.2) | as R35.1.5 | the `status` response itself is HTTP 200 |
| `status` / `user_action` / `cancel`, mmrpc-level | task-framework errors | ch. 55 §55.2 | ch. 55 §55.2 | ch. 55 §55.2 |

In particular, platform config, protocol, node, token and NFT failures are
**not** reported synchronously by `init`; they appear as the task's terminal
error.

> **Public-docs erratum (informative).** The reference's task page lists
> `TaskTimedOut`, `CoinCreationError` and `HwError` as the possible terminal
> errors and describes a `details.result` wrapper; the reference version reports
> the platform discriminants of R35.1.5 directly in `details` (hardware failures
> as `Internal`), and never `CoinCreationError` or `HwError` for this family.

R35.3.6 The `task::enable_eth` family is delivered by the shared platform-coin
task-activation framework of [Chapter 48](48-platform-coin-task-activation.md),
whose unit of work is the activation of §35.1 (one activation path for one-shot
and task). Cancellation follows ch. 48 §48.7 and ch. 55 §55.2.

> **Reloaded status (informative).** Implemented on `fix/activation-api-parity`:
> the synchronous already-active check, the Trezor failure mapping to
> `Internal`, `FollowHwDeviceInstructions`, and the HD shape for the Trezor
> result. The §35.1 remaining deviations apply. A real Trezor flow is covered
> only by the emulator-gated tests.

---

## 35.3A `task::enable_erc20::{init,status,user_action,cancel}` -- token task variant

R35.3A.1 The task family `task::enable_erc20::init`, `::status`,
`::user_action`, `::cancel` shall exist on all targets as the task variant of
`enable_erc20`. It is delivered by the token-task substrate of
[Chapter 48](48-platform-coin-task-activation.md) (§48.11) and its framework
methods follow ch. 55 §55.2. (G for every rule of §35.3A: the public
reference names `task::enable_erc20::init` only in its changelog and documents
no request, result or error; the error catalogue lists its error enum by name.)

R35.3A.2 `init` `params`:

| Field | JSON type | Required / default | Effect |
| --- | --- | --- | --- |
| `ticker` | string | required | Token ticker. |
| `protocol` | coin-protocol object | optional | Custom token (R35.2.7). |
| `activation_params` | object | required (may be `{}`) | |
| `activation_params.required_confirmations` | integer | as R35.2.1 | |
| `activation_params.path_to_address` | object | default `{account_id:0, chain:"External", address_id:0}` | HD: enabled address of the token. |
| `activation_params.scan_policy` | string | default `scan_if_new_wallet` | HD scan policy. |
| `activation_params.min_addresses_number` | integer | optional | HD minimum addresses. |
| `client_id` | integer | optional (ch. 55 §55.2) | Task-event routing. |

`init` success: `{"task_id": <integer>}`.

R35.3A.3 `init` mmrpc-level errors (checked before the task is spawned, in this
order):

| `error_type` | Condition | `error_data` | HTTP |
| --- | --- | --- | --- |
| `InvalidRequest` | request-parse failure | string | 400 |
| `TokenIsAlreadyActivated` | `ticker` already active | **object** `{ticker: string}` | 400 |
| `TokenConfigIsNotFound` | no config and no inline `protocol` | string: ticker | 400 |
| `TokenProtocolParseError` | config protocol missing/unparseable | object `{ticker, error}` | **400** |
| `UnexpectedTokenProtocol` | not an ERC-20 (or TRC-20) protocol, or contract address not in exact EIP-55 form | object `{ticker, protocol}` | 400 |
| `CustomTokenError` | inline `protocol` rejected | as R35.2.7 | 400 |
| `PlatformCoinIsNotActivated` | platform not active | string: platform ticker | 400 |
| `UnsupportedPlatformCoin` | platform active but not EVM | object `{platform_coin_ticker, token_ticker}` | 500 |
| `Internal` | task could not be created | string | 500 |

Note that this family's already-activated `error_data` is an object, unlike the
string of the one-shot method, and its protocol-parse status is 400, unlike the
500 of the one-shot method.

R35.3A.4 In-progress values (strings): `ActivatingCoin` (initial),
`RequestingWalletBalance`, `Finishing`, `WaitingForTrezorToConnect`,
`FollowHwDeviceInstructions`; awaiting values `EnterTrezorPin`,
`EnterTrezorPassphrase`, answered through `user_action` with the hardware
user-action payloads of R35.3.2. The hardware values occur only when the
platform was activated under the `Trezor` policy.

R35.3A.5 Terminal success `details`:

| Field | JSON type | Content |
| --- | --- | --- |
| `ticker` | string | the token ticker |
| `platform_coin` | string | the platform ticker |
| `token_contract_address` | string | contract address in EIP-55 checksummed form (TRON base58 under TRX) |
| `current_block` | integer | current chain height |
| `required_confirmations` | integer | effective swap confirmations |
| `wallet_balance` | object | `{"wallet_type":"Iguana","address":string,"balance":{<token ticker>: BalanceInfo}}` for a single-address platform, or the HD report of R35.1.3(b) whose balance maps contain the token ticker only |

Unlike the one-shot `enable_erc20`, this family works on an HD-derived platform.

R35.3A.6 Terminal (status-level) errors:

| `error_type` | Condition | `error_data` |
| --- | --- | --- |
| `Transport` | on-chain decimals read or chain-height read got no usable answer (R35.1.6 conditions); also an invalid-payload class failure during token construction | string |
| `CouldNotFetchBalance` | balance enumeration got no usable answer | string |
| `HwError` | the platform uses the `Trezor` policy but no hardware-wallet context is available | string: the hardware error variant name (e.g. `"NotInitialized"`) |
| `TaskTimedOut` | framework-originated timeout | object `{duration: {secs, nanos}}` |
| `TokenIsAlreadyActivated` | lost a concurrent registration race | object `{ticker}` |
| `CustomTokenError` | custom-token duplicate-activation check failed | as R35.2.7 |
| `PlatformCoinMismatch` | reserved | absent |
| `Internal` | derivation or signing-policy mismatch, HD address derivation or account-creation failure, device-interaction failure, a user action of the wrong kind, other unexpected failures | string |

The `status` response carrying a terminal error is HTTP 200; the error object
sits directly in `details` (ch. 55 §55.2).

R35.3A.7 `cancel` of an in-flight token task shall undo that task's own
registration: if the cancelled task had already registered the token when
cancellation is processed, the token is removed again; a token registered by
another activation stays (ch. 55 §55.2 for the wire contract, R55.5.8 for the
scope of the cleanup).

> **Reloaded status (informative).** Routed on `fix/activation-api-parity` on
> every target, with its own registry and the full contract of §35.3A; see
> Chapter 48 §48.11 for the substrate's remaining limits.

---

## 35.4 `get_token_info` -- on-chain ERC-20 contract info

R35.4.1 The public RPC `get_token_info` shall, given a token's coin-protocol
descriptor naming its platform and contract address, return that contract's
on-chain information. Its `params` shall carry a single `protocol` object in the
coin-protocol shape of R35.5 selecting the token's platform and contract address.

R35.4.2 The platform coin named by the protocol must already be active; the call
queries the contract over the platform's EVM JSON-RPC client (ERC-20 ABI calls).

R35.4.3 The success `result` shall carry:
- an optional `config_ticker` -- the ticker under which this contract is known in
  coin configuration, when one is configured (omitted otherwise); and
- a tagged token-info object (tag field `type` with value `ERC20`, payload field
  `info`) whose `info` carries the contract's on-chain `symbol` (string) and
  `decimals` (integer).

R35.4.4 `error_type` discriminants and HTTP status shall include at least:

| `error_type` | HTTP status |
| --- | --- |
| `NoSuchCoin` | 404 |
| `UnsupportedTokenProtocol` | 400 |
| `InvalidRequest` | 400 |
| `RetrieveInfoError` | 500 |

A `protocol` that is not a token protocol, or whose platform is not an EVM
platform coin, shall be rejected as an invalid-request / unsupported-protocol
class error; a contract read that fails on-chain shall be a retrieve-info class
error.

---

## 35.5 Coin-protocol descriptor (dictated config contract)

R35.5.1 The `protocol` object used by `get_token_info` (and accepted as an
inline custom-token protocol by `enable_erc20`, `task::enable_erc20::init` and
`erc20_tokens_requests` entries) follows the project's public coin-protocol JSON
contract: a tagged object with tag field `type` and payload field
`protocol_data`. The EVM cases are:

```jsonc
// EVM platform coin protocol.
"protocol": { "type": "ETH",
              "protocol_data": { "chain_id": 1 } }

// ERC-20 token protocol.
"protocol": { "type": "ERC20",
              "protocol_data": { "platform": "ETH",
                                 "contract_address": "0x..." } }
```

The `ETH` / `ERC20` type strings, the `chain_id`, `platform`, and
`contract_address` field names, and the EVM chain id values are the dictated
public config contract; the Rust types that deserialise them are discretionary.
`chain_id` is a required integer for the EVM platform protocol (R35.1.2 phase 3).
A configured ERC-20 `contract_address` is accepted only in exact EIP-55
checksummed form (R35.1.5 `UnexpectedTokenProtocol`). The TRON cases (`TRX`,
`TRC20`) are bound by ch. 21; the NFT pseudo-coin case (`NFT`) by ch. 19 §19.13.

---

## 35.6 Swap gas-fee policy RPCs (EIP-1559 dictated semantics)

R35.6.1 The public RPC `get_swap_gas_fee_policy` shall read the swap gas-fee
policy currently in effect for a coin. Its `params` carry `coin` (string,
required). Its `result` is the policy value of R35.6.3.

R35.6.2 The public RPC `set_swap_gas_fee_policy` shall set the swap gas-fee policy
for a coin. Its `params` carry `coin` (string, required) and `swap_gas_fee_policy`
(the policy value of R35.6.3; optional, defaulting to the legacy policy). Its
`result` echoes the policy now in effect.

> **Naming (binding).** The method strings are exactly `get_swap_gas_fee_policy`
> and `set_swap_gas_fee_policy`, and the set-request field is exactly
> `swap_gas_fee_policy`. These are **not** `*_swap_transaction_fee_policy`.

R35.6.3 The policy is a published string enum over exactly four variants:
- `Legacy` -- pre-EIP-1559 single gas-price pricing (the default);
- `Low` -- low EIP-1559 priority-fee tier;
- `Medium` -- medium EIP-1559 priority-fee tier;
- `High` -- high EIP-1559 priority-fee tier.

The same enum is accepted as the optional `swap_gas_fee_policy` field of
`enable_eth_with_tokens` (R35.1.1), where it sets the coin's initial policy.

R35.6.4 These RPCs apply only to EVM-family coins (coins whose swaps price gas via
the EIP-1559 / legacy gas model). A request for a coin that does not support the
policy shall be rejected.

R35.6.5 `error_type` discriminants and HTTP status shall include at least:

| `error_type` | HTTP status |
| --- | --- |
| `NoSuchCoin` | 400 |
| `NotSupported` | 400 |

R35.6.6 The selected policy governs how the EVM coin prices gas for subsequent
swap transactions (legacy gas price vs. an EIP-1559 max-fee / max-priority-fee
pair at the chosen tier). The mapping from tier to concrete fee values is dictated
by EIP-1559 fee-history semantics, not by this chapter.

---

## 35.7 Implementation-substrate note (informative)

This note orients the implementer; it is not normative.

**Already present in reloaded:**
- the EVM coin type (the public `EthCoin` newtype and its `MmCoinEnum` variant),
  its coin-type enumeration (native ETH vs. ERC-20 vs. NFT), and the legacy
  builder that creates an EVM coin from coin config + request;
- the generic V2 activation entrypoints (platform-coin-with-tokens, single
  token) and the platform task substrate of ch. 48;
- one-shot `enable_eth_with_tokens`, `enable_erc20`, `enable_nft` (separate NFT
  handler), `get_token_info`, `get_swap_gas_fee_policy`,
  `set_swap_gas_fee_policy`, and `task::enable_eth::*`, routed on native and
  WASM, with the deviations recorded in the `Reloaded status` notes of §35.1,
  §35.2, §35.3 and §35.3A.

**What must be added or changed:** the wire contract of §§35.1--35.3A where the
status notes record a deviation, the `task::enable_erc20::*` family, and the
wire-shape tests of §35.8A. The placement of these implementations should follow
the existing activation layout used by the other platform coins; their internal
decomposition is the implementer's choice.

---

## 35.8 Acceptance criteria

- `enable_eth_with_tokens` activates an EVM platform coin plus zero or more
  ERC-20 tokens in one call and returns `current_block`, the per-address platform
  and token information, and an always-present `nfts_infos`, in the
  single-address or HD shape (R35.1.3); `get_balances: false` omits balances and
  lists token `tickers` (single-address shape).
- With every supplied node unresponsive the call fails with `Transport`, a
  string `error_data`, HTTP 502 (R35.1.6).
- Malformed requests are rejected with the bound discriminant, `error_data`
  shape and HTTP status of R35.1.5 without partially activating the coin.
- `enable_erc20` adds a single ERC-20 token to an already-active single-address
  EVM platform and returns `balances`, `platform_coin`, lower-case
  `token_contract_address`, and `required_confirmations` (R35.2); `enable_nft`
  reaches the same handler and error contract (R35.2.4).
- `task::enable_eth::*` and `task::enable_erc20::*` deliver the same results as
  their one-shot counterparts (respectively the token-task result of R35.3A.5),
  with the error placement of R35.3.5 / R35.3A.3 / R35.3A.6, and the Trezor
  policy drives the bound in-progress / awaiting values through `status` /
  `user_action` (R35.3.2, R35.3.3).
- `get_token_info` returns the ERC-20 contract `symbol` and `decimals` (plus the
  configured ticker when known) for an active EVM platform, and rejects
  non-token / non-EVM protocols with the bound errors (R35.4).
- `get_swap_gas_fee_policy` / `set_swap_gas_fee_policy` round-trip a policy in the
  set `{Legacy, Low, Medium, High}` for an EVM coin, use exactly those method
  strings and the `swap_gas_fee_policy` field, and reject unsupported coins
  (R35.6).
- The entire surface builds and is routed on native **and** WASM targets, with
  the MetaMask policy and `rpc_mode: "Metamask"` available only on WASM
  (R35.1.4, R35.1.4a).

## 35.8A Wire-shape tests (binding)

Each test below serializes the response or error exactly as the dispatcher
would and asserts the `error_type` string, the JSON type of `error_data` (and its
field names and types for objects, or its absence), and the HTTP status. A test
that only checks an internal variant does not satisfy these rules.

- **T35.1** `enable_eth_with_tokens` with every node answering HTTP 502 (and,
  separately, refusing connections): `Transport`, string, 502 -- both with all
  token decimals in config and with a token whose decimals must be read
  on-chain.
- **T35.2** Chain-height read failing after a successful node probe (or with no
  probe): `Transport`, string, 502.
- **T35.3** Balance read failing (single-address): `Transport`, string, 502.
- **T35.4** Absent/zero `swap_contract_address` on a trading-capable coin, zero
  fallback, invalid node URL, unsupported URL scheme: `InvalidPayload`, string,
  400; absent `swap_contract_address` on a wallet-only coin succeeds.
- **T35.5** Empty and absent `nodes`: `AtLeastOneNodeRequired`, string, 400.
  Absent `erc20_tokens_requests`: `InvalidRequest`, string, 400.
- **T35.6** `Trezor` policy on the one-shot call: `UnexpectedDeviceActivationPolicy`,
  no `error_data` key, 400.
- **T35.7** Token whose protocol names another platform: `PlatformCoinMismatch`,
  no `error_data`, 400. Token with a non-checksummed configured contract
  address: `UnexpectedTokenProtocol`, object `{ticker, protocol}`, 400.
- **T35.8** `PrivKeyPolicyNotAllowed` serializes under that exact name with a
  string `error_data`, HTTP 500.
- **T35.9** `rpc_mode: "Metamask"` and `priv_key_policy: {"type":"Metamask"}` on
  a native build: `InvalidRequest`, string, 400; a string-form
  `priv_key_policy`: `InvalidRequest`.
- **T35.10** Success shapes: single-address with `get_balances` true and false
  (`balances` omitted, `tickers` only on the token entry, `nfts_infos` = `{}`);
  `pubkey` is 130 hex characters starting `04` without `0x`; HD shape with
  `ticker`, `wallet_balance.wallet_type = "HD"` and `nfts_infos`.
- **T35.11** `nft_req` accepted in both the wrapped and the bare form.
- **T35.12** `enable_erc20`: decimals read failing → `Transport`, string, **500**;
  balance read failing → `Transport`, string, 500; `token_contract_address`
  is lower-case; `PlatformCoinIsNotActivated` carries the platform ticker as a
  string; each `CustomTokenError` case serializes as tabled in R35.2.7.
- **T35.13** `enable_nft` failure classes of R35.2.8 serialize with the
  single-token discriminants.
- **T35.14** `task::enable_eth::init` on an active ticker: mmrpc-level
  `PlatformIsAlreadyActivated`, string, 400; a config-not-found ticker yields a
  `task_id` and later a status-level `PlatformConfigIsNotFound`.
- **T35.15** Trezor task: no hardware context → status-level `Internal`, string;
  a device button request is reported as `FollowHwDeviceInstructions`.
- **T35.16** `task::enable_erc20::init` errors of R35.3A.3 (in particular
  `TokenIsAlreadyActivated` with object `{ticker}` and
  `TokenProtocolParseError` at 400); terminal success shape of R35.3A.5 for a
  single-address platform; terminal `Transport` / `CouldNotFetchBalance` per
  R35.3A.6.

## 35.9 Provenance Footer

- *Inputs:* the published Komodo DeFi Framework API reference (the
  `enable_eth_with_tokens`, `enable_erc20`, `task_managed/enable_eth`,
  `enable_nft` pages, `common_structures/{activation,wallet,nfts,enums,error_types}`
  and the v20 index) -- governing where explicit; the EVM JSON-RPC, EIP-55,
  EIP-1559, ERC-20 ABI and CAIP-style chain identification specifications;
  reloaded's existing EVM coin type, activation entrypoints and task substrate
  (§35.7); [Chapter 17](17-swap-v2-evm-path.md) (V2 swap contracts);
  [Chapter 19](19-nft-module-layout.md) (NFT activation and pseudo-coin);
  [Chapter 21](21-tron-integration.md) (TRON platform specifics);
  [Chapter 22](22-walletconnect-v2.md) (WalletConnect sessions);
  [Chapter 38](38-utxo-coin-maintenance-and-features.md) (HD activation
  semantics); [Chapter 47](47-metamask-integration.md) (MetaMask policy);
  [Chapter 48](48-platform-coin-task-activation.md) (task substrate);
  [Chapter 50](50-evm-trezor-signing.md) (Trezor EVM path);
  [Chapter 55](55-coin-activation-wire-contract.md) (envelope, HTTP policy, task
  framework, backend-failure table).
- *Permitted-input classes used:* external public specification (the published
  KDF API reference; EVM/EIP/ERC/CAIP specifications); baseline source;
  cross-chapter contracts; Interop / wire-and-API-bound reuse (R29/R31) for the
  dictated method strings, JSON field names, serialized discriminants and
  `error_data` shapes.
- *Sibling-allowlist consultations:* the chapters listed under *Inputs*.
- *Forbidden corpus:* consulted by the KDF Spec Reader (reference versions
  `v3.0.0-beta` and `v2.6.0-beta`) solely to decide behaviour where the public
  reference is silent, ambiguous or self-contradictory (marked **G**), including
  every HTTP status; no private implementation expression is normative in this
  chapter.
