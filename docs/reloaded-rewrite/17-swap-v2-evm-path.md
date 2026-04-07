# Chapter 17 — Atomic-Swap V2 EVM Path & Contract Interaction

> **Status in reloaded:** *fully implemented; this chapter documents
> the existing surface.* All V2 EVM functionality landed in baseline
> (commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`) and is carried
> forward unchanged except for one removed `TODO add burnFee` comment
> ([§17.7](#177-dexfee-delivery)) and the addition of a maker-side
> NFT state-machine bridge ([§17.9](#179-nft-variant)).
>
> No IMPL marker — no code changes are commissioned by this chapter.

---

## 17.0 Executive Summary

The V2 EVM atomic-swap path delivers a two-party trustless trade
between an EVM-native asset (ETH, ERC-20, ERC-721, ERC-1155) and any
counterparty asset (UTXO, Tendermint, another EVM chain) using two
deployed Solidity contracts:

- `EtomicSwapMakerV2` — holds the maker's locked payment.
- `EtomicSwapTakerV2` — holds the taker's funding (which carries the
  dex fee) and the taker's payment.

Each contract maintains a `(swapId -> state)` mapping; swap state
transitions happen by calling typed entry points that take the
`swapId`, the secret hashes, the participant addresses, the lock
times, and the amounts. Reveal-on-spend (the EVM equivalent of
SIGHASH_ALL cooperative-branch script_sig) is implemented as a
contract method that requires the spender to pass the maker secret
(or taker secret, on the symmetric path); the contract recomputes
the hash and accepts the call iff it matches the value committed at
payment time.

The Rust surface lives in
[`mm2src/coins/eth/eth_swap_v2/`](../../mm2src/coins/eth/eth_swap_v2/)
(four files: `mod.rs`, `eth_maker_swap_v2.rs`, `eth_taker_swap_v2.rs`,
`nft_swap_v2.rs`) and implements the cross-coin traits
`MakerCoinSwapOpsV2` (5 methods) and `TakerCoinSwapOpsV2` (14 methods)
on `EthCoin`. The state-machine driver in
[`mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs)
and
[`mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs)
dispatches to these trait methods symmetrically with the UTXO V2
path ([Chapter 15](15-swap-v2-utxo-path.md)).

---

## 17.1 Why this exists

The V1 EVM swap path relied on a single legacy contract whose
fee-handling semantics could not express the V2 protocol's
funding-vs-payment split. V2 EVM adds:

1. **Separation of funding and payment** on the taker side. The
   taker first deposits funds (including the dex fee) into
   `EtomicSwapTakerV2`, and only later promotes them to a "payment"
   state once the maker has confirmed the trade. This makes early
   abort (before the maker's payment is observed) reclaim-safe by
   timelock without involving the maker.
2. **Reveal-on-spend on-chain** via the two contracts' `spend*`
   methods. Each spend call requires the spender to pass the
   counterparty's secret; the contract enforces the hash check.
3. **Dex-fee delivery in the same transaction** as the taker payment.
   The fee amount is encoded as a contract argument (ETH path) or as
   an explicit ERC-20 allowance + transfer (ERC-20 path) and is
   forwarded by the contract to the network's fee-collection address
   atomically with the payment lock.
4. **NFT support** for maker-side ERC-721 and ERC-1155 payments via
   `nft_swap_v2.rs`, with NFT-for-fungible-token swaps (the taker
   uses the fungible-token V2 path).

---

## 17.2 Contract architecture

Two contracts back the protocol; both are deployed once per EVM
chain and addressed from coin configuration.

### 17.2.1 `EtomicSwapMakerV2`

State map: `mapping(bytes32 => MakerPayment) public makerPayments;`

Lifecycle methods (signatures, plain ABI types):

| Method                            | Behaviour                                                                 |
|-----------------------------------|---------------------------------------------------------------------------|
| `ethMakerPayment`                 | Lock `msg.value` into a new `makerPayments[id]` entry (ETH).              |
| `erc20MakerPayment`               | Pull `amount` via `transferFrom` and lock under `makerPayments[id]`.     |
| `erc721MakerPayment`              | Receive an ERC-721 token; lock under `makerPayments[id]`.                |
| `erc1155MakerPayment`             | Receive an ERC-1155 token (`amount` units); lock under `makerPayments[id]`.|
| `spendMakerPayment`               | Caller passes `makerSecret`; contract recomputes the hash, releases funds to caller (the taker). |
| `refundMakerPaymentTimelock`      | After `paymentLockTime`, maker reclaims.                                 |
| `refundMakerPaymentSecret`        | Cooperative abort: maker reclaims by revealing `takerSecret`.            |

Common arguments across all `*MakerPayment` entry points:
`(bytes32 id, address taker, bytes32 takerSecretHash, bytes32 makerSecretHash,
uint256 paymentLockTime)` plus the amount/token parameters
appropriate to the asset class.

### 17.2.2 `EtomicSwapTakerV2`

State map: `mapping(bytes32 => TakerPayment) public takerPayments;`

Lifecycle methods:

| Method                            | Behaviour                                                                                       |
|-----------------------------------|-------------------------------------------------------------------------------------------------|
| `ethTakerPayment`                 | Lock `msg.value = paymentAmount + dexFee` into `takerPayments[id]` (ETH).                       |
| `erc20TakerPayment`               | Pull `paymentAmount + dexFee` via `transferFrom` and lock.                                      |
| `takerPaymentApprove`             | (ERC-20 only) Confirm allowance; updates `takerPayments[id]` to *approved* state.               |
| `spendTakerPayment`               | Caller (the maker) passes `takerSecret`; contract sends `paymentAmount` to caller and `dexFee` to the network's fee-collection address.|
| `refundTakerPaymentTimelock`      | After `paymentLockTime`, taker reclaims everything.                                             |
| `refundTakerPaymentSecret`        | Cooperative abort: taker reclaims by revealing `makerSecret`.                                   |

Common arguments across `*TakerPayment` entry points:
`(bytes32 id, uint256 dexFee, uint256 paymentAmount, address maker,
bytes32 takerSecretHash, bytes32 makerSecretHash,
uint256 fundingLockTime, uint256 paymentLockTime)` plus token
address for the ERC-20 variant.

### 17.2.3 Events

Both contracts emit per-action events that the Rust side consumes
via topic filters:

- `MakerPaymentSent(bytes32 id)`
- `MakerPaymentSpent(bytes32 id, bytes32 makerSecret)`
- `MakerPaymentRefundedTimelock(bytes32 id)`
- `MakerPaymentRefundedSecret(bytes32 id, bytes32 takerSecret)`
- `TakerPaymentSent(bytes32 id)`
- `TakerPaymentApproved(bytes32 id)`
- `TakerPaymentSpent(bytes32 id, bytes32 takerSecret)`
- `TakerPaymentRefundedTimelock(bytes32 id)`
- `TakerPaymentRefundedSecret(bytes32 id, bytes32 makerSecret)`

These appear in
[`mm2src/coins/eth/maker_swap_v2_abi.json`](../../mm2src/coins/eth/maker_swap_v2_abi.json)
and
[`mm2src/coins/eth/taker_swap_v2_abi.json`](../../mm2src/coins/eth/taker_swap_v2_abi.json).

---

## 17.3 ABI files

The two JSON files referenced above are factual contract
specifications (function signatures, event signatures, parameter
types). They are loaded by `ethabi::Contract::load()` at coin
activation and used to encode calldata and decode events.

The files are bit-identical with the baseline; this chapter does
not modify them. They are *factual content* (machine-derived from
the Solidity sources) and ride under the same Conditions E/F
licensing rationale used for other contract ABIs in the tree —
see [Chapter 29 — License conditions E and F](29-license-conditions-e-f.md).

---

## 17.4 Maker side — `MakerCoinSwapOpsV2 for EthCoin`

Five trait methods, all delegating to corresponding `*_impl`
functions in
[`mm2src/coins/eth/eth_swap_v2/eth_maker_swap_v2.rs`](../../mm2src/coins/eth/eth_swap_v2/eth_maker_swap_v2.rs):

| Trait method                          | Impl function                              |
|--------------------------------------|--------------------------------------------|
| `send_maker_payment_v2`              | `send_maker_payment_v2_impl`               |
| `validate_maker_payment_v2`          | `validate_maker_payment_v2_impl`           |
| `refund_maker_payment_v2_timelock`   | `refund_maker_payment_v2_timelock_impl`    |
| `refund_maker_payment_v2_secret`     | `refund_maker_payment_v2_secret_impl`      |
| `spend_maker_payment_v2`             | `spend_maker_payment_v2_impl`              |

### 17.4.1 `send_maker_payment_v2_impl`

1. Derive `swapId = etomic_swap_id_v2(paymentLockTime, makerSecretHash)`
   — an internal helper that hashes the lock time (big-endian) with
   the maker secret hash. The contract recomputes the same id from
   the call arguments so both sides converge on the same
   `mapping` key.
2. Branch on `coin_type`:
   - `EthCoinType::Eth` → call `ethMakerPayment(id, taker,
     takerSecretHash, makerSecretHash, paymentLockTime)` with
     `msg.value = paymentAmount`.
   - `EthCoinType::Erc20 { token_addr }` → call
     `erc20MakerPayment(id, token_addr, amount, taker,
     takerSecretHash, makerSecretHash, paymentLockTime)` after the
     usual `approve` flow.
   - `EthCoinType::Nft { contract_addr }` → dispatch to
     `nft_swap_v2.rs::erc721MakerPayment` or `erc1155MakerPayment`
     per token standard.
3. Sign and broadcast; return the `SignedEthTx`.

### 17.4.2 `validate_maker_payment_v2_impl`

Given a `SignedEthTx` and the negotiation parameters, the maker
side (the validator is actually the taker, but in trait terms it's
the "maker payment validator") performs:

1. Decode the tx input via the loaded ABI; assert the method
   selector matches one of the four `*MakerPayment` selectors.
2. Assert decoded arguments match negotiation:
   `(takerSecretHash, makerSecretHash, paymentLockTime, taker)`.
3. Assert `msg.value` (ETH path) or `amount` (ERC-20 path) equals
   the expected payment amount.
4. Read `makerPayments[id]` via an `eth_call` and assert the state
   is `Sent` (i.e. funds are locked and not yet spent / refunded).

### 17.4.3 `spend_maker_payment_v2_impl`

Called by the taker once they observe a valid maker payment and
have learned the maker secret (from the cooperative-spend tx — see
§17.5.7). Builds and broadcasts a `spendMakerPayment(id, amount,
makerSecret, taker, ...)` call. The contract recomputes
`keccak256(makerSecret)` and accepts the call iff it equals the
committed `makerSecretHash`.

### 17.4.4 Refund paths

- `refund_maker_payment_v2_timelock_impl` — `block.timestamp >=
  paymentLockTime` precondition; the contract releases funds back
  to the maker.
- `refund_maker_payment_v2_secret_impl` — cooperative abort. The
  maker passes the *taker* secret (which the maker learned via the
  cooperative-spend protocol); contract recomputes the hash and
  releases.

---

## 17.5 Taker side — `TakerCoinSwapOpsV2 for EthCoin`

15 trait methods. The taker side has more surface because of the
funding-vs-payment split and the EVM-specific approval step.

| Trait method                                | Impl / behaviour                                                |
|--------------------------------------------|------------------------------------------------------------------|
| `send_taker_funding`                        | `send_taker_funding_impl` — call `*TakerPayment` entry point.   |
| `validate_taker_funding`                    | `validate_taker_funding_impl` — decode + state-check.           |
| `refund_taker_funding_timelock`             | `refund_taker_payment_with_timelock_impl`.                      |
| `refund_taker_funding_secret`               | `refund_taker_funding_secret_impl`.                             |
| `search_for_taker_funding_spend`            | `search_for_taker_funding_spend_impl` — locate the spend tx.    |
| `gen_taker_funding_spend_preimage`          | EVM-specific: returns RLP-encoded funding tx as "preimage", dummy sig. The approve flow replaces the preimage exchange. |
| `validate_taker_funding_spend_preimage`     | Always returns `Ok` (no real preimage to validate).             |
| `sign_and_send_taker_funding_spend`         | (ERC-20 only) Promote funding → payment by calling `takerPaymentApprove(id)`; for ETH this is a no-op-equivalent path. |
| `refund_combined_taker_payment`             | EVM-specific timelock refund that handles the combined funding+payment state in a single call (no separate funding refund needed on the EVM contract). |
| `gen_taker_payment_spend_preimage`          | EVM-specific stub (returns `Ok` with empty preimage).           |
| `validate_taker_payment_spend_preimage`     | Always returns `Ok`.                                            |
| `skip_taker_payment_spend_preimage`         | Returns `true` — the state machine skips the preimage exchange. |
| `sign_and_broadcast_taker_payment_spend`    | `sign_and_broadcast_taker_payment_spend_impl` — maker calls `spendTakerPayment`. |
| `find_taker_payment_spend_tx`               | `find_taker_payment_spend_tx_impl` — log polling for `TakerPaymentSpent`. |
| `extract_secret_v2`                         | `extract_secret_v2_impl` — decode `takerSecret` from the spend tx's calldata. |

### 17.5.1 EVM "no real preimage" optimisation

The UTXO V2 protocol uses a preimage-exchange round so the taker can
partial-sign a transaction that the maker completes. EVM cannot work
the same way because every signed tx is already broadcast-ready —
there is no "preimage + partial sig" intermediate. The Rust
implementation handles this by:

- `gen_taker_funding_spend_preimage` returns the RLP-encoded funding
  tx as a stand-in "preimage" and an empty signature vector.
- `validate_taker_funding_spend_preimage` always succeeds.
- `skip_taker_payment_spend_preimage` returns `true`, instructing
  the state machine to skip the preimage round entirely.

This is the correct behaviour for EVM and is intentional, not a
stub.

### 17.5.2 The `takerPaymentApprove` step (ERC-20 only)

For ERC-20 tokens, `erc20TakerPayment` deposits the funds into the
contract but leaves them in a *funding* state. The taker must
follow up with `takerPaymentApprove(id)` to promote the funds to a
*payment* state (which the maker can then claim via
`spendTakerPayment`). For ETH the funding and payment states are
unified — no separate approval call is needed.

The approval call exists because the ERC-20 path requires an
explicit allowance update before the contract can move tokens to
the fee-collection address inside `spendTakerPayment`.

This call is reached through the `sign_and_send_taker_funding_spend`
trait method, not through a dedicated `taker_payment_approve` trait
entry — the trait surface uses a single "funding → payment
advancement" method that branches on `coin_type`.

### 17.5.3 `send_taker_funding_impl` / `validate_taker_funding_impl`

`send_taker_funding_impl` selects the `*TakerPayment` ABI entry
point per `coin_type`, packs the arguments
`(id, dexFee, paymentAmount, maker, takerSecretHash,
makerSecretHash, fundingLockTime, paymentLockTime[, tokenAddr])`,
signs the transaction, and broadcasts it. For ETH,
`msg.value = paymentAmount + dexFee`; for ERC-20, `msg.value = 0`
and the contract pulls tokens via `transferFrom` (the taker's
allowance must already cover `paymentAmount + dexFee`).

`validate_taker_funding_impl` decodes the broadcast tx's calldata,
asserts the method selector and arguments match negotiation, and
reads `takerPayments[id]` via `eth_call` to confirm the on-chain
state is `Sent`.

### 17.5.4 Refund paths

- `refund_taker_payment_with_timelock_impl` — invoked when
  `block.timestamp >= paymentLockTime` and no spend has been
  observed. Calls the contract's `refundTakerPaymentTimelock(id)`
  (or, on EVM, the consolidated `refund_combined_taker_payment`
  entry that handles both funding-only and funding+payment states).
- `refund_taker_funding_secret_impl` — cooperative abort. The
  taker calls `refundTakerPaymentSecret(id, makerSecret)` (or the
  funding-specific equivalent); the contract recomputes the hash
  and releases.

### 17.5.5 `search_for_taker_funding_spend_impl`

Given a funding tx hash, polls `eth_getLogs` for the matching
`TakerPaymentApproved` event (ERC-20) or scans subsequent blocks
for a state transition (ETH). Returns `Some(FundingTxSpend)` when
found, `None` otherwise. The lookback range is bounded by the
funding tx's confirmation block.

### 17.5.6 `sign_and_broadcast_taker_payment_spend_impl`

Called by the **maker** to claim the taker's payment. Builds and
broadcasts `spendTakerPayment(id, paymentAmount, takerSecret, ...)`.
The contract recomputes `keccak256(takerSecret)`, asserts it
equals `takerSecretHash`, then forwards `paymentAmount` to the
caller and `dexFee` to the network fee-collection address — both
transfers happen atomically in the same call frame.

### 17.5.7 Reveal-on-spend and `extract_secret_v2`

When the taker calls `spendMakerPayment(id, ..., makerSecret, ...)`,
the maker observes the broadcast tx, decodes its calldata via the
loaded ABI, and extracts the `makerSecret` argument. This is the
EVM equivalent of UTXO's "secret-in-script_sig" reveal. The Rust
implementation lives in `extract_secret_v2_impl` and decodes either
the `makerSecret` (when called from the taker-side spending the
maker's payment) or `takerSecret` (symmetric).

---

## 17.6 ETH vs ERC-20 differentiation

| Aspect                       | ETH                                                | ERC-20                                                                                  |
|------------------------------|----------------------------------------------------|-----------------------------------------------------------------------------------------|
| Funds movement               | `msg.value` carries amount                         | `approve()` + `transferFrom()` inside the contract                                       |
| Maker payment entry          | `ethMakerPayment(...)`                             | `erc20MakerPayment(...)` with `token_addr`                                              |
| Taker payment entry          | `ethTakerPayment(...)`, `msg.value = amt + fee`    | `erc20TakerPayment(...)`, no `msg.value`; tokens pulled via `transferFrom`              |
| Approval round               | None                                               | `takerPaymentApprove(id)` between funding and payment states                            |
| Fee delivery                 | Contract forwards from `msg.value` on `spendTakerPayment` | Contract `transferFrom` taker's allowance, then `transfer` to fee address          |
| Dust / minimum               | Network-level gas floor                            | Token-contract-specific (no on-chain dust rule)                                          |

The branches are explicit `match coin_type { Eth => ..., Erc20 { .. } => ..., Nft { .. } => ... }`
in each impl function.

---

## 17.7 DexFee delivery

The V2 EVM contracts accept a flat `dexFee` `uint256` argument on
`*TakerPayment` and forward the entire amount to the network's
fee-collection address on `spendTakerPayment`.

Today the V2 EVM path handles **only `DexFee::Standard`**.
[Chapter 16 — V2 Pre-Burn Output](16-swap-v2-pre-burn-output.md)
documents that `EthCoin`'s `MmCoin::should_burn_dex_fee()` returns
`false`, which means the factory `DexFee::new_from_taker_coin`
always yields `Standard` for EVM-side dex-fee delivery. Adding
pre-burn to EVM requires the contract ABI to grow `burnAmount` and
`burnAddress` parameters; that change is on the V2 contract roadmap
and is out of scope for reloaded's licence-rebase work.

The baseline carried a `// TODO add burnFee support` comment on the
single line that converts `dex_fee.fee_amount()` into a `U256`;
reloaded removed the TODO because the structured `DexFee` type now
correctly returns the full fee under `fee_amount()` for the
`Standard` variant, and the factory ensures EVM never sees `WithBurn`.
There is no behavioural delta.

---

## 17.8 Event monitoring

`find_taker_payment_spend_tx_impl` polls `eth_getLogs` filtered by:

- contract address (`EtomicSwapTakerV2` deployment),
- topic 0 = `keccak256("TakerPaymentSpent(bytes32,bytes32)")`,
- topic 1 = the `swapId`,
- block range starting at the funding-tx confirmation block.

Once a matching log is found, the txhash is fetched and the calldata
decoded for the secret. Confirmations are tracked via the standard
`eth_blockNumber` polling mechanism shared with V1.

The same pattern (different topic, different contract address)
covers `MakerPaymentSpent` for the symmetric path.

---

## 17.9 NFT variant

[`mm2src/coins/eth/eth_swap_v2/nft_swap_v2.rs`](../../mm2src/coins/eth/eth_swap_v2/nft_swap_v2.rs)
implements maker-side ERC-721 and ERC-1155 payment construction.
The corresponding ABI selectors `erc721MakerPayment` and
`erc1155MakerPayment` are part of `EtomicSwapMakerV2`.

A small bridge layer
[`mm2src/mm2_main/src/lp_swap/nft_maker_swap_v2.rs`](../../mm2src/mm2_main/src/lp_swap/nft_maker_swap_v2.rs)
(added post-baseline) wires the NFT path into the maker
state-machine driver, including a `should_use_nft_swap_v2()`
decision helper with outcomes:

- `Use` — both sides advertise NFT V2 and the chain has a deployed
  NFT-aware contract.
- `VersionMismatch` — protocol version mismatch; fall back to
  fungible.
- `NoNftContract` — chain has no NFT-aware contract; refuse the
  trade.

NFT *taker-side* support is intentionally absent: NFT swaps are
NFT-for-fungible (the taker always uses the fungible-token V2 path
above). This keeps the taker surface small and avoids a
2x2 maker/taker × NFT/fungible matrix.

See [Chapter 19 — NFT Module Layout](19-nft-module-layout.md) for
the broader NFT activation and storage surface.

---

## 17.10 State-machine integration

The maker-side state machine (`MakerSwapEvent` enum in
[`maker_swap_v2.rs:63`](../../mm2src/mm2_main/src/lp_swap/maker_swap_v2.rs#L63))
walks the sequence:

```
Initialized
  → WaitingForTakerFunding
  → TakerFundingReceived
  → MakerPaymentSentFundingSpendGenerated
  → TakerPaymentReceived
  → TakerPaymentSpent
  → Completed
```

with the error branch:

```
... (any state) → MakerPaymentRefundRequired → MakerPaymentRefunded
... (pre-payment) → Aborted
```

The taker-side state machine (`TakerSwapEvent` enum in
[`taker_swap_v2.rs:56`](../../mm2src/mm2_main/src/lp_swap/taker_swap_v2.rs#L56))
walks:

```
Initialized
  → Negotiated
  → TakerFundingSent
  → MakerPaymentAndFundingSpendPreimgReceived
  → MakerPaymentConfirmed
  → TakerPaymentSent
  → TakerPaymentSpent
  → MakerPaymentSpent
  → Completed
```

with parallel error/abort branches.

Both state machines dispatch through the `MakerCoinSwapOpsV2` and
`TakerCoinSwapOpsV2` traits — the same dispatch surface used by the
UTXO V2 path ([§15.5](15-swap-v2-utxo-path.md#155-protocol-surface)).
This means a single state-machine driver supports both UTXO×UTXO,
EVM×EVM, and cross-asset (UTXO×EVM, EVM×Tendermint, etc) trades.

---

## 17.11 Watcher reward

`watcher_reward: false` for V2 EVM swaps today (visible in the
state-machine driver). The V2 EVM contracts do not have a
watcher-reward field; the watcher-reward feature
([Chapter 9 — Watcher-Reward Infrastructure](09-watcher-reward-infrastructure.md))
remains a V1-only opt-in. Extending it to V2 EVM would require
contract redeployment with a `watcherReward` parameter.

---

## 17.12 Tests

Unit tests live alongside each impl function in
`mm2src/coins/eth/eth_swap_v2/eth_*_v2.rs`. Integration tests live
in `mm2src/mm2_main/tests/docker_tests/swap_v2_*.rs` (gated behind
`docker_tests` feature). End-to-end V2 EVM coverage against an
Anvil node is part of the docker test fleet; this chapter does
not enumerate per-test specs.

---

## 17.13 Out of scope / known limitations

1. **Pre-burn on EVM** — see [Chapter 16 §16.10](16-swap-v2-pre-burn-output.md#1610-evm-and-tendermint).
   Requires contract ABI extension.
2. **Watcher reward on V2 EVM** — requires contract redeployment.
3. **NFT taker side** — intentional design decision; NFT trades are
   maker-NFT-for-taker-fungible only.
4. **Cross-EVM-chain swap atomicity** — each side runs its own
   contract; cross-chain finality is at-most-once per chain's
   confirmation policy.
5. **Gas estimation** — the impl uses `eth_estimateGas` with the
   standard +10% safety margin shared with V1; documented in
   [Chapter 8 — Fee-Routing Engine](08-fee-routing-engine.md).

---

## 17.14 External references

- Ethereum ABI v2 encoding: Solidity docs §"Contract ABI Specification".
- `eth_getLogs` filter semantics: JSON-RPC method documented at
  ethereum.org/developers/docs/apis/json-rpc.
- ERC-20: EIP-20. ERC-721: EIP-721. ERC-1155: EIP-1155.
- `keccak256` hash: SHA-3 candidate, used as Ethereum's canonical
  hash.

---

## 17.15 Provenance

- The `eth_swap_v2/` module structure, both contract ABIs, the 19
  trait method implementations on `EthCoin`, and the state-machine
  event enums are all carried forward from the GPLv2 baseline at
  commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`.
- The maker-side NFT state-machine bridge in
  `mm2src/mm2_main/src/lp_swap/nft_maker_swap_v2.rs` was added in
  reloaded to wire the pre-existing NFT calldata builders to the
  state-machine driver; that file is the only material addition to
  the V2 EVM surface in this chapter.
- The `// TODO add burnFee` comment removal is documented in
  §17.7 with the rationale (factory ensures EVM never sees
  `WithBurn`, so the TODO is moot).
