# Chapter 30 -- Provenance & Attribution Index

## 30.0 Executive Summary

The previous twenty-nine chapters of this document set each carry,
in their own provenance footers, a record of the materials they were
checked against. Read in sequence they describe the post-baseline
delta of this project; read individually they record where the
contents of one chapter came from.

This closing chapter does not add new substantive content. It is a
cross-cutting index of what the rest of the document set already
contains, intended to make three classes of question fast to answer
without reading every chapter end-to-end:

1. *Which chapter covers a given crate or module?* The reverse map
   in [§30.3](#303-per-crate-reverse-map) answers this.

2. *Which external specifications, sibling repositories, or wire
   formats does the document set as a whole cite?* The aggregated
   input register in [§30.2](#302-aggregated-input-register)
   answers this by category, pointing back to the chapter that
   carries the specific citation.

3. *What is the per-chapter summary?* The capsule index in
   [§30.1](#301-per-chapter-capsule-index) answers this in one row
   per chapter: title, primary crate(s), and the one-sentence
   purpose of the chapter.

The index is not authoritative on any individual claim it
summarises. The authoritative statement for any individual claim is
the chapter that makes it; this index is a lookup table. Where the
index appears to disagree with a chapter, the chapter is right and
the index is the bug.

## 30.1 Per-chapter Capsule Index

The columns are: chapter number, title (linkable), primary
mm2src/ area(s) covered, and a one-sentence summary of what the
chapter documents.

| # | Title | Primary area(s) | Summary |
|---|---|---|---|
| 00 | [Overview & Purpose](00-overview.md) | (meta) | States what the document set is, who it is for, and how it is structured. |
| 01 | [Clean-Room Rules and Methodology](01-clean-room-rules.md) | (meta) | The normative rules: permitted inputs, forbidden inputs, identifier hygiene, citation discipline, chapter shape. |
| 02 | [Baseline State](02-baseline-state.md) | (workspace root) | Records the exact baseline commit and what the inherited tree looked like at that commit. |
| 03 | [Toolchain Modernization](03-toolchain-modernization.md) | workspace, `rust-toolchain.toml` | The migration from nightly to stable Rust and the removal of post-baseline unstable features. |
| 04 | [Error Aggregation & Trait-Solver Adaptation](04-error-aggregation-type-adaptation.md) | `mm2_err_handle`, `derives/ser_error{,_derive}` | Replacement of `NotEqual` and the explicit-conversion helpers introduced to keep the error framework compiling on stable. |
| 05 | [HD Wallet Support](05-hd-wallet-support.md) | `crypto` | The BIP-32/39/43/44 HD derivation stack, encrypted mnemonic storage, dual-curve (secp256k1 / ed25519) key paths. |
| 06 | [Network-ID & Seed-Node Decoupling](06-network-id-seed-node.md) | `mm2_net_config`, `mm2_p2p` | The pluggable per-netid configuration registry and removal of hard-coded seed addresses. |
| 07 | [Wallet Lifecycle and Key Export](07-wallet-lifecycle-and-key-export.md) | `mm2_main::lp_wallet`, `crypto` | Named wallets, encrypted mnemonic file format, create / list / delete RPCs. |
| 08 | [Atomic-Swap Fee-Routing Engine](08-fee-routing-engine.md) | `mm2_main::lp_swap::dex_fee`, `mm2_net_config` | The `DexFee` value type and the per-network fee resolution path. |
| 09 | [Watcher Reward Infrastructure](09-watcher-reward-infrastructure.md) | `mm2_main::lp_swap::swap_watcher`, `coins::lp_coins` | The swap-watcher protocol, its gossipsub topic, and the watcher-reward bounds. |
| 10 | [SSE Streaming](10-sse-streaming.md) | `mm2_event_stream`, `mm2_main::rpc` | The push-mode Server-Sent-Events backbone and the streamer registry. |
| 11 | [Order-Match Cancellation Race](11-order-match-cancellation.md) | `mm2_main::lp_ordermatch` | The 120-second recently-cancelled time-cache and the gossipsub-skew problem it solves. |
| 12 | [Maker-Order State Store](12-order-match-state-store.md) | `mm2_main::lp_ordermatch` | The `MakerOrdersContext` store, its TTL backing, and the ticker reverse indices. |
| 13 | [Swap Version Negotiation](13-swap-version-negotiation.md) | `mm2_main::lp_swap`, `coins` | The `SwapVersion` tag and its placement on order and swap messages. |
| 14 | [State-Machine Runtime](14-state-machine-runtime.md) | `mm2_state_machine` | The generic persistent state-machine runtime carved out of the legacy pattern module. |
| 15 | [Swap V2 UTXO Path](15-swap-v2-utxo-path.md) | `coins::utxo` (specification) | The V2 UTXO two-stage taker funding and the dual-secret maker HTLC specification. |
| 16 | [Swap V2 Pre-Burn Output](16-swap-v2-pre-burn-output.md) | `coins::utxo` (specification) | The split-DEX-fee specification with a burn share and a residual fee share. |
| 17 | [Swap V2 EVM Path](17-swap-v2-evm-path.md) | `coins::eth::eth_swap_v2` | The `EtomicSwapMakerV2` / `EtomicSwapTakerV2` contract pair and the reveal-on-spend flow. |
| 18 | [Tendermint, IBC, and Cross-Chain HTLC Surfaces](18-tendermint-ibc-htlc.md) | `coins::tendermint` | The Iris and Nucleus HTLC dialects, the IBC `MsgTransfer` envelope, and CW20-style token support. |
| 19 | [NFT Module Layout](19-nft-module-layout.md) | `coins::nft` | The EVM-NFT trait surface, the storage abstraction, and the pluggable metadata-refresh path. |
| 20 | [Siacoin Integration](20-siacoin-integration.md) | `coins::siacoin` | The `sia-rust-patched` integration, `SpendPolicy::atomic_swap`, and the walletd-backed account model. |
| 21 | [Tron Integration](21-tron-integration.md) | `coins::eth::tron` | Withdraw-only Tron support: Base58Check addresses, protobuf transactions, bandwidth-and-energy fees. |
| 22 | [WalletConnect v2](22-walletconnect-v2.md) | `kdf_walletconnect` | The WalletConnect v2 relay client, the x25519 + HKDF + ChaCha20-Poly1305 transport, and the session-persistence stores. |
| 23 | [Trading API Client](23-trading-api-client.md) | `trading_api` | The 1inch v6.0 typed HTTP client; quote / build-tx / tokens / portfolio request and response types. |
| 24 | [GUI Account-State Persistence](24-gui-account-state.md) | `mm2_gui_storage` | The named-account storage layer (Iguana / HD / hardware-wallet variants) and its SQLite + IndexedDB backends. |
| 25 | [SQL Query-Builder Replacement](25-sql-query-builder.md) | `db_common` | The async SQLite facade, the typed query-builder DSL, and the pragma helpers. |
| 26 | [Cross-Platform & WASM](26-cross-platform-and-wasm.md) | `mm2_bin_lib`, `common`, `mm2_db`, `mm2_net` | The seven build targets, the `cfg_native!` / `cfg_wasm32!` macros, dual storage and transport, the dual entry-point pattern. |
| 27 | [Infrastructure Crate Carve-Outs](27-infrastructure-crate-carve-outs.md) | (registry) | A register of the cross-cutting infrastructure crates the rest of the chapters reference. |
| 28 | [libp2p Stack Consolidation](28-libp2p-modernization.md) | `mm2_p2p` | The four-into-one consolidation of the baseline P2P crates, the composed `AtomicDexBehaviour`, and the relay-mesh extension. |
| 29 | [Treatment of License Conditions (e) and (f)](29-license-conditions-e-f.md) | `mm2_main::lp_swap::dex_fee`, `mm2_net_config`, `common`, `coins::z_coin` | The project position on the post-baseline upstream conditions: not binding, values preserved for protocol reasons, logic independently re-expressed. |
| 30 | (this chapter) | (meta) | Cross-cutting index. |

## 30.2 Aggregated Input Register

This register lists the categories of external input the document
set cites, with the chapters in which they appear. It is sorted by
category. It does not duplicate the citations themselves; each
listed chapter carries its own *External References* section that is
the authoritative form of the citation.

### 30.2.1 BIP / SLIP family

- Hierarchical-deterministic-wallet BIPs and SLIPs (BIP-32 family,
  SLIP-0010, SLIP-0044, and adjacent documents) -- cited in
  [Chapter 05](05-hd-wallet-support.md).
- The mnemonic-encoding BIP (BIP-39) -- cited in
  [Chapter 05](05-hd-wallet-support.md) and referenced from
  [Chapter 07](07-wallet-lifecycle-and-key-export.md).

### 30.2.2 EIP family

- Ethereum token standards (ERC-20, ERC-721, ERC-1155) -- cited in
  [Chapter 17](17-swap-v2-evm-path.md) and
  [Chapter 19](19-nft-module-layout.md).
- Typed structured data signing (EIP-712) -- cited in
  [Chapter 17](17-swap-v2-evm-path.md).
- Related ERC and proxy / permit standards as required for the V2
  contract pair and the NFT inventory -- cited in
  [Chapter 17](17-swap-v2-evm-path.md) and
  [Chapter 19](19-nft-module-layout.md).

### 30.2.3 IBC / Cosmos family

- Inter-Blockchain Communication core packet types, including the
  fungible-token `MsgTransfer` envelope -- cited in
  [Chapter 18](18-tendermint-ibc-htlc.md).
- Iris HTLC module and Nucleus HTLC dialect -- cited in
  [Chapter 18](18-tendermint-ibc-htlc.md).

### 30.2.4 IETF / W3C standards

- Server-Sent Events (W3C) -- cited in
  [Chapter 10](10-sse-streaming.md).
- HKDF (RFC 5869), ChaCha20-Poly1305 (RFC 7539), Curve25519 / x25519
  (RFC 7748), Argon2 (RFC 9106), and adjacent RFCs governing the
  WalletConnect v2 transport and the mnemonic-encryption format --
  cited in [Chapter 22](22-walletconnect-v2.md) and
  [Chapter 05](05-hd-wallet-support.md) /
  [Chapter 07](07-wallet-lifecycle-and-key-export.md).

### 30.2.5 libp2p protocol family

- Gossipsub, floodsub, request-response, ping, noise, mplex, the DNS
  and websocket transports, secp256k1 keys -- cited in
  [Chapter 28](28-libp2p-modernization.md), with topic-naming
  consumers in [Chapter 09](09-watcher-reward-infrastructure.md)
  and [Chapter 11](11-order-match-cancellation.md).

### 30.2.6 Per-chain protocol references

- Bitcoin Script and the UTXO HTLC construction -- cited in
  [Chapter 15](15-swap-v2-utxo-path.md) and
  [Chapter 16](16-swap-v2-pre-burn-output.md).
- Solidity contracts published on public blockchains and their ABIs
  -- cited in [Chapter 17](17-swap-v2-evm-path.md).
- Tendermint ABCI / Cosmos SDK message types -- cited in
  [Chapter 18](18-tendermint-ibc-htlc.md).
- Sia spend-policy semantics -- cited in
  [Chapter 20](20-siacoin-integration.md).
- Tron transaction protobuf, TAPOS, and TRC-20 -- cited in
  [Chapter 21](21-tron-integration.md).
- Zcash shielded-transaction primitives (outgoing viewing keys, the
  ARRR-specific fee path) -- cited in
  [Chapter 29](29-license-conditions-e-f.md).

### 30.2.7 External APIs and wire-format counterparties

- WalletConnect v2 protocol (the relay role specifically) -- cited
  in [Chapter 22](22-walletconnect-v2.md).
- The 1inch Swap API v6.0 -- cited in
  [Chapter 23](23-trading-api-client.md).
- Trezor wire protocol and the Trezor protobuf schemas -- referenced
  by [Chapter 05](05-hd-wallet-support.md) and
  [Chapter 27](27-infrastructure-crate-carve-outs.md).

### 30.2.8 Sibling open-source repositories

Sibling repositories under licenses compatible with the project's
GPLv2 intent are governed by
[01-clean-room-rules.md §2.5](01-clean-room-rules.md#2-permitted-inputs)
and by the internal allow-list documented at
`local/clean-room-doc/SIBLING_ALLOWLIST.md`. That allow-list lives
in the project's `local/` directory and is not part of the
published document set; it is reviewed by the chapter author at
draft time. Chapters that quote an allow-listed identifier name the
identifier and cite the sibling repository in their own External
References section.

The two repositories currently on the allow-list (under GPLv2 and
MIT respectively) are the publicly-available GUI clients that
preceded the baseline date: a desktop client and a mobile client.
The chapter most likely to invoke that allowance is
[Chapter 24](24-gui-account-state.md), where the data shapes
exchanged between the framework and a GUI consumer are defined.

### 30.2.9 Baseline tree

The single largest input cited by every chapter is the baseline
tree at commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`. It is
the inherited corpus on which every post-baseline change builds;
[Chapter 02](02-baseline-state.md) describes its contents in
detail. Where a later chapter says "at baseline X was Y", the X is
resolvable as `git show c1d46c0:<path>` against the project tree.

### 30.2.10 Behavioural observation

[01-clean-room-rules.md §2.6](01-clean-room-rules.md#2-permitted-inputs)
admits observable behaviour of the live peer-to-peer mesh and of
public blockchains as a permitted input. Two chapters cite it
explicitly:

- [Chapter 11](11-order-match-cancellation.md) cites the observed
  message-ordering behaviour of the live gossipsub mesh as the
  motivation for the recently-cancelled time cache.
- [Chapter 13](13-swap-version-negotiation.md) cites the observed
  presence of mixed-version peers as the motivation for the
  `SwapVersion` tag.

## 30.3 Per-Crate Reverse Map

The reverse map answers "which chapter(s) cover this crate". It is
sorted by `mm2src/` path. Where a chapter cites a crate
peripherally rather than as primary subject, the citation is marked
*(ref)*.

| Crate (under `mm2src/`) | Chapter(s) |
|---|---|
| `coins` (top-level) | 13 *(ref)*, 15, 16, 17, 18, 19, 20, 21, 29 |
| `coins/eth` | 17, 19, 21 |
| `coins/eth/eth_swap_v2` | 17 |
| `coins/eth/tron` | 21 |
| `coins/nft` | 19, 25 *(ref)* |
| `coins/siacoin` | 20 |
| `coins/tendermint` | 18 |
| `coins/utxo` | 15, 16 |
| `coins/z_coin` | 29 |
| `coins_activation` | 18 *(ref)* |
| `common` | 03 *(ref)*, 14, 26, 29 |
| `common/shared_ref_counter` | 27 |
| `crypto` | 05, 07 |
| `db_common` | 25 |
| `derives/enum_derives` | 27 |
| `derives/ser_error{,_derive}` | 04, 27 |
| `hw_common` | 27 |
| `kdf_*` (chain, codec, codec_derive, crypto, keys, primitives, rpc_types, script, spv_validation) | 15 *(ref)* |
| `kdf_walletconnect` | 22 |
| `ledger` | 27 |
| `mm2_bin_lib` | 26 |
| `mm2_core` | 27 *(ref)* |
| `mm2_db` | 26 |
| `mm2_err_handle` | 04, 27 |
| `mm2_eth` | 26 *(ref)* |
| `mm2_event_stream` | 10, 27 |
| `mm2_git` | 27 *(ref)* |
| `mm2_gui_storage` | 24 |
| `mm2_io` | 26 |
| `mm2_main` | 07, 08, 09, 10, 11, 12, 13, 14, 17, 29 |
| `mm2_main::lp_native_dex` | 06, 29 |
| `mm2_main::lp_ordermatch` | 11, 12 |
| `mm2_main::lp_swap` | 08, 09, 13, 15, 16, 17, 29 |
| `mm2_main::lp_wallet` | 07 |
| `mm2_main::rpc` | 10 *(ref)*, 27 *(ref)* |
| `mm2_metamask` | 05 *(ref)*, 26 |
| `mm2_metrics` | 27 |
| `mm2_net` | 26 |
| `mm2_net_config` | 06, 08, 28 *(ref)*, 29 |
| `mm2_number` | 27 |
| `mm2_p2p` | 06 *(ref)*, 09 *(ref)*, 11 *(ref)*, 28 |
| `mm2_rpc` | 27 |
| `mm2_state_machine` | 14 |
| `proxy_signature` | 27, 28 |
| `rpc_task` | 27 |
| `shared_ref_counter` | -- *(listed above under `common/shared_ref_counter`)* |
| `trading_api` | 23 |
| `trezor` | 05 *(ref)*, 27 |

The reverse map is a navigation aid. A crate may legitimately be
touched by post-baseline commits that no chapter discusses
individually (test scaffolding, dependency bumps, configuration
follow-ups); the absence of a chapter listing here means the
chapter set does not single that crate out for treatment, not that
the crate is untouched.

Workspace crates that are present but not given a row of their
own are: the renamed Bitcoin-primitive family `kdf_chain` /
`kdf_codec` / `kdf_codec_derive` / `kdf_crypto` / `kdf_keys` /
`kdf_primitives` / `kdf_rpc_types` / `kdf_script` /
`kdf_spv_validation` (referenced collectively on the single
`kdf_*` row above; their renaming from the baseline-era
`mm2_bitcoin/*` is a workspace-wide refactor without a dedicated
chapter); the vendoring crates `ethabi-vendored` and
`testcontainers-vendored`; the test-helper crates
`mm2_test_helpers`, `kdf_test_helpers`, and
`mm2_bitcoin_wire_tests`; and the baseline `peers` crate retained
for git-history continuity as discussed in
[Chapter 28 §28.1.2](28-libp2p-modernization.md#2812-legacy-crates----migration-status).

## 30.4 How the document set composes as an attribution record

Every chapter satisfies the same shape (see
[01-clean-room-rules.md §6](01-clean-room-rules.md#6-chapter-shape)):
executive summary, reproduction detail, external references,
provenance footer. The four-section shape is what makes a chapter
auditable in isolation, and what makes the chapter set composable
as a whole. Two consequences follow:

1. **Each chapter is a self-contained attribution statement.** A
   reader who picks up [Chapter 17](17-swap-v2-evm-path.md) alone
   can see what the EVM V2 swap path derives from -- the EVM token
   standards, the published Solidity contract ABIs, the
   inter-operability requirement that this project must produce
   transactions other clients can consume. The reader does not need
   the rest of the document set to verify that one chapter's
   attribution.

2. **The set of chapters describes the whole delta.** Where a
   post-baseline behaviour is not covered by a chapter, the
   methodology
   ([01-clean-room-rules.md §8](01-clean-room-rules.md#8-what-to-do-when-a-chapter-cannot-be-written))
   requires that the chapter be drafted or the source be brought
   back into a derivable state; un-derivable behaviours are not
   allowed to accumulate. The size of the chapter set therefore
   tracks the size of the post-baseline delta itself.

Re-auditing the project from the document set means, in practice:

- read the chapter that covers the area of interest;
- follow its *External References* to confirm the inputs are public
  and dated;
- compare the chapter's *Reproduction Detail* against the current
  source at the path the chapter cites;
- consult this index for adjacent chapters when the area of
  interest spans multiple crates.

The index supports the third bullet by giving the reader a single
table per question (per-chapter, per-input, per-crate) instead of
sending them on a multi-chapter search.

## 30.5 External References

This chapter is itself an index and does not introduce new external
citations. The chapters indexed in
[§30.2](#302-aggregated-input-register) and
[§30.3](#303-per-crate-reverse-map) carry their own *External
References* sections; those are the authoritative citations.

The two documents this chapter does cite are internal to the
project:

- [`docs/reloaded-rewrite/01-clean-room-rules.md`](01-clean-room-rules.md)
  -- the methodology against which every other chapter is read.
- [`docs/reloaded-rewrite/02-baseline-state.md`](02-baseline-state.md)
  -- the description of the inherited corpus that every chapter's
  delta is computed against.

## 30.6 Provenance Footer

This index was assembled by reading every chapter from 00 through
29 in the working tree at the time of writing, baseline
`c1d46c0c1592faa0860f704008b2b2381bc3840f`. The per-chapter capsule
summaries paraphrase the executive summaries of the chapters they
point to; the input register groups citations by category against
the *External References* of those chapters; the per-crate reverse
map was assembled by scanning the same chapters for their primary
crate(s). The chapters indexed here are the authoritative form of
any individual claim; where this index appears to disagree with a
chapter, the chapter is right.
