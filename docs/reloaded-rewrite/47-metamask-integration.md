# Chapter 47 -- MetaMask (Browser EIP-1193 Wallet) Integration

**Status:** driving-spec (required port). This chapter specifies an **RPC and
activation-policy surface** over MetaMask plumbing that already exists in
reloaded; it does not introduce a new coin or a new low-level transport.

> **One-sentence claim:** the project shall expose, **on the WASM target only**,
> a long-running connection task `task::connect_metamask::{init,status,cancel}`
> that establishes an authenticated MetaMask session in the framework crypto
> context, and shall let EVM coins (ch. 35) activate under a MetaMask signing
> policy so that transaction signing and broadcast are delegated to the connected
> browser wallet (via `eth_sendTransaction`) instead of a locally held secret,
> restricting such coins to non-swap operations (balance / send / withdraw /
> approve / message-sign).

> **Treatment:** **T-PORT.** The low-level EIP-1193 transport, the MetaMask
> session abstraction, and the crypto-context login handshake are all present in
> reloaded (see §47.8). What is required is (a) the task-RPC family that drives
> the existing handshake, (b) an EVM signing path that delegates to the connected
> MetaMask session, and (c) threading the MetaMask policy through EVM activation.
> The wire contract distilled here is the source of truth for that port.

> **Binding scope.** Requirements bind observable behaviour, the public mmrpc-2.0
> method strings and their request/response JSON field names, and externally
> *dictated* interop (the EIP-1193 provider contract, the MetaMask JSON-RPC
> method names, EIP-712 typed-data signing, EIP-155 chain identification,
> secp256k1 public-key recovery). Those public contracts are the source of truth,
> not this project's code. Private Rust types, helper decomposition, and internal
> module structure are informative and are **not** bound by this chapter.

> **Source of truth (informative).** The method strings, JSON field names, and
> error discriminants below are governed by the published Komodo DeFi Framework
> API documentation (the public KDF API `task::connect_metamask` and EVM
> activation sections). Where this chapter and the public API docs disagree, the
> public API docs govern. Where the public docs are silent, behaviour is
> distilled from current framework behaviour and flagged where it diverges.

---

## 47.0 Executive summary

MetaMask is a browser-injected EIP-1193 wallet. Integration has two parts:

| Surface | Tier | Targets | Purpose |
| --- | --- | --- | --- |
| `task::connect_metamask::init` | mmrpc 2.0 (task) | **WASM only** | start a connection task; returns a `task_id` |
| `task::connect_metamask::status` | mmrpc 2.0 (task) | **WASM only** | poll connection progress / final result |
| `task::connect_metamask::cancel` | mmrpc 2.0 (task) | **WASM only** | abort a pending connection task |
| `enable_eth_with_tokens` (MetaMask policy) | mmrpc 2.0 | **WASM only** for this policy | activate an EVM platform coin whose signer is the connected MetaMask session |

All methods use the mmrpc-2.0 envelope (`{"mmrpc":"2.0","method":...,"params":
{...},"id":...}`) and, on success, return `{"mmrpc":"2.0","result":{...},"id":
...}`. On error they return the standard mmrpc-2.0 error envelope carrying
`error`, `error_path`, `error_trace`, `error_type`, and `error_data`; only the
**`error_type` discriminant** and its HTTP status are bound below (the human-
readable `error` text is not part of the contract).

The connection task is a *one-step* establishment of a session: it detects the
injected provider, requests the active account, proves account ownership through
an EIP-712 login-challenge signature, and registers the resulting session in the
crypto context. It does **not** expose a `user_action` channel (see §47.3).

> **Whole surface is WASM-gated (R47.6).** On native targets none of these method
> strings are routed; see §47.7.

---

## 47.1 `task::connect_metamask::init` -- start a connection task

R47.1.1 The public RPC `task::connect_metamask::init` shall start a long-running
task that establishes a MetaMask session, and shall return immediately with a
`task_id`. Its `params` object shall carry:

- `project` (string, required) -- the calling application's name. It is used as
  the domain/identity presented to the user inside the EIP-712 login-challenge
  that MetaMask asks the user to sign, so the wallet prompt shows which
  application is requesting the connection.

R47.1.2 The success `result` shall be `{ "task_id": <integer> }`, the handle used
by §47.2 and §47.4.

R47.1.3 The task shall, in order: (a) detect the browser-injected EIP-1193
provider; (b) request the active account from the wallet; (c) build an EIP-712
login-challenge bound to `project` and ask the wallet to sign it; (d) recover the
secp256k1 public key from that signature; (e) verify the recovered address equals
the active account; (f) register the resulting authenticated session (account
address, account public key, and the live provider handle) in the framework
crypto context. Steps (b) and (c) require user interaction inside the MetaMask
extension popup.

R47.1.4 Only one MetaMask connection may be initialized at a time. If a
connection is already initializing, `init` shall fail with the
already-initializing error of §47.6 rather than starting a second task.

R47.1.5 A successfully completed task leaves the crypto context holding a ready
MetaMask session that subsequent EVM activation (§47.5) consumes. The session is
authenticated: ownership of the account was proven by signature recovery, not
merely asserted by the wallet.

---

## 47.2 `task::connect_metamask::status` -- poll progress / final result

R47.2.1 The public RPC `task::connect_metamask::status` shall report the state of
a connection task. Its `params` object shall carry:

- `task_id` (integer, required) -- the handle returned by §47.1.
- `forget_if_finished` (boolean, optional, default **true**) -- when the task has
  reached a terminal state, whether to drop it from the task manager after this
  status read.

R47.2.2 While running, the result is the standard task-status `InProgress`
envelope whose payload is one of a small, ordered set of in-progress phase
discriminants. The bound wire set is:

| In-progress phase (wire value) | Meaning |
| --- | --- |
| `Initializing` | detecting the provider and requesting the active account |
| `SigningLoginMetadata` | awaiting the user's EIP-712 login-challenge signature in the wallet popup |

R47.2.3 On success the result is the standard task-status `Ok` envelope whose
payload reports the connected account:

- `eth_address` (string) -- the connected MetaMask account address (`0x`-prefixed,
  EIP-55 checksummed as returned by the wallet).

R47.2.4 On failure the result is the standard task-status `Error` envelope
carrying one of the §47.6 `error_type` discriminants.

> **Upstream divergence (informative).** The connection-task `Ok` payload binds
> only `eth_address`. The connected account's public key, the wallet/provider
> name, and the active EIP-155 chain id are **not** carried on this task's result
> wire; the public key is available downstream from the crypto-context session
> accessors and the chain identity is established per-coin at EVM activation
> (§47.5). If the public API docs specify additional fields on this result, the
> docs govern and the Coder shall add them by reading from the existing session
> accessors (§47.8) without altering the handshake. See §47.10.

---

## 47.3 `task::connect_metamask::user_action` -- not part of this surface

R47.3.1 The MetaMask connection task shall **not** define a `user_action`
channel. The user's in-wallet confirmation (account selection and login-challenge
signing) is collected directly by the MetaMask extension popup and surfaces back
to the task through the EIP-1193 provider promise; it is **not** relayed through
an RPC `user_action` call. Accordingly, the task's user-action and
awaiting-input types are the empty/never type, and `task::connect_metamask::
user_action` shall not be routed by the dispatcher.

> **Upstream divergence (informative).** This departs from the generic
> `{init,status,user_action,cancel}` task template (used e.g. by
> `task::enable_eth`, ch. 35). For MetaMask there is no `user_action` method. The
> Coder must not synthesize one. See §47.10 open question O-1.

---

## 47.4 `task::connect_metamask::cancel` -- abort a pending connection

R47.4.1 The public RPC `task::connect_metamask::cancel` shall abort a pending
connection task. Its `params` object shall carry `task_id` (integer, required).

R47.4.2 On success the result shall be the standard success acknowledgement
(`{ "result": "success" }`).

R47.4.3 Cancelling shall reset any partially established MetaMask session in the
crypto context so a subsequent `init` starts cleanly. Cancelling an unknown
`task_id` shall fail with the no-such-task error of §47.6.

---

## 47.5 EVM-coin activation and operation under a MetaMask signing policy

This section is the behavioural contract for running an EVM coin (ch. 35) whose
signer is a connected browser MetaMask session rather than a locally held
secret. It is the source of truth for the EVM signing port (§47.8 gap #2). The
governing principle: under MetaMask the framework **delegates** signing — and,
for transactions, broadcast — to the wallet over the EIP-1193 provider, and the
framework never holds, derives, or exports the account's private key.

### 47.5.A Activation contract

R47.5.1 The V2 EVM platform activation RPC `enable_eth_with_tokens` (ch. 35)
shall accept a MetaMask signing policy selected through its `priv_key_policy`
field. The wire shape is the tagged policy object of R35.1.4: a MetaMask policy
is `"priv_key_policy": { "type": "Metamask" }` (no payload). Any standalone EVM
platform-enable RPC that exposes a `priv_key_policy` field shall accept the same
tagged value with the same semantics. This policy value is defined and routed
**only on the WASM target** (§47.7).

R47.5.2 When the MetaMask policy is selected, activation shall consume the
already-connected MetaMask session from the crypto context (established via
§47.1). A MetaMask session **must already be connected** (the caller must have
run `task::connect_metamask::init` to a successful terminal state) before EVM
activation under this policy. Activation shall **not** itself perform the
connection handshake. If no MetaMask session is present, activation shall fail
with the not-initialized condition of §47.6 (R47.5.16), instructing the caller
to connect first.

R47.5.3 The activated EVM coin's address and account public key shall be derived
from the **connected MetaMask account** — the account whose ownership was proven
at connect time (§47.1) — and not from any local seed or derivation path. At
activation, the address bound to the coin is that connected account.

R47.5.4 Activation-time account-consistency: activation shall bind the coin to
the account that the connected session authenticated. If the wallet's
currently-selected active account no longer matches the session's connected
account at activation time, activation shall fail with the account-mismatch
condition of §47.6 rather than binding a coin to an address the framework cannot
prove the user controls.

R47.5.5 The MetaMask policy applies to the EVM platform coin and, transitively,
to its ERC-20 child tokens activated in the same `enable_eth_with_tokens` call;
the tokens inherit the platform's signing policy. There is no per-token MetaMask
override.

### 47.5.B Signing and broadcast model

R47.5.6 **Delegated sign-and-broadcast.** Under the MetaMask policy, an EVM coin
shall NOT produce an offline-signed raw transaction and broadcast it itself.
Instead, for any on-chain transaction it shall hand the unsigned transaction
request to the connected wallet via the EIP-1193 `eth_sendTransaction` request;
the wallet signs the transaction with the account's key (held only by the
extension) **and broadcasts it**, returning a transaction hash. The framework's
role is reduced to building the transaction request fields, issuing the provider
request, and observing the returned hash and subsequent chain state.

R47.5.7 **No offline raw signature is available.** Explicitly: there is no
MetaMask path that returns a detached raw signature, a locally re-broadcastable
signed raw transaction, or the account private key. The wallet only signs
transactions that it then broadcasts itself (R47.5.6), and only signs *messages*
it presents to the user (R47.5.8). The framework cannot obtain a signature over a
transaction it intends to broadcast later on its own schedule, nor a signature it
can hand to a third party for later broadcast.

R47.5.8 **Provider request surface.** The framework shall interact with the
connected wallet only through the standard, wire-dictated EIP-1193 / MetaMask
JSON-RPC requests. The bound interop surface is:

| EIP-1193 request | Purpose | Wallet returns |
| --- | --- | --- |
| `eth_requestAccounts` / `eth_accounts` | obtain / re-read the connected account(s) | account address list |
| `eth_chainId` | read the wallet's active EIP-155 chain id | chain id (hex quantity) |
| `wallet_switchEthereumChain` | ask the wallet to switch its active chain to the coin's chain | success / user-rejection error |
| `personal_sign` / `eth_signTypedData_v4` | message / EIP-712 typed-data signing (user-presented) | a signature over the presented message |
| `eth_sendTransaction` | sign **and broadcast** a transaction | the broadcast transaction hash |

The connect-time login challenge (§47.1) is the only typed-data signature the
framework relies on for session establishment. The wallet `personal_sign` /
`eth_signTypedData_v4` methods are over user-presented payloads, never an
extractable transaction signature. **Implementation note:** binding these to a
post-activation message-signing RPC is **deferred** — the synchronous
`MarketCoinOps::sign_message` trait cannot drive the asynchronous wallet call,
so (as with WalletConnect) a dedicated async message-signing RPC path is
required; see R47.5.11 and the deferral note below.

R47.5.9 **Per-operation active-account consistency re-check.** Because the user
can switch the active account inside the extension at any moment, before each
signing/broadcast request the framework shall re-read the wallet's currently
active account and verify it still equals the account the coin was activated
under (R47.5.3). A mismatch shall fail the operation with the account-mismatch
condition of §47.6 **before** any `eth_sendTransaction` / signing request is
issued, so the wrong account is never asked to sign.

R47.5.10 **Chain-consistency.** Before broadcasting, the framework shall ensure
the wallet's active chain (R47.5.8 `eth_chainId`) matches the coin's EIP-155
chain; if it does not, it shall request `wallet_switchEthereumChain`. A
user-rejected or failed switch shall fail the operation rather than broadcasting
on the wrong chain.

### 47.5.C Operation support matrix

R47.5.11 The following operation classes ARE supported for an EVM coin under the
MetaMask policy (they need only the connected account, read-only chain access, or
a single wallet-side sign-and-broadcast):

| Operation class | Supported | Mechanism |
| --- | --- | --- |
| Activation / enable (platform + ERC-20 tokens) | yes | §47.5.A |
| Balance query (platform + tokens) | yes | read-only, address only |
| Address display (`my_address`) | yes | connected account (R47.5.14) |
| Public-key display | yes | connected account public key (R47.5.14) |
| `withdraw` (the user-facing send) | yes | single `eth_sendTransaction` (R47.5.6); already broadcast by the wallet, so no re-broadcastable `tx_hex` is returned (R47.5.7) |
| Plain send / ERC-20 `approve` as standalone operations | n/a in reloaded | reloaded reaches these only through the swap path (no standalone non-swap RPC exposes them); under the non-swap MetaMask policy they are therefore not separately available, and `withdraw` is the user-facing broadcast op |
| Message signing | deferred | requires a future async message-signing RPC: the synchronous `MarketCoinOps::sign_message` trait cannot drive the asynchronous wallet `personal_sign` / `eth_signTypedData_v4`, mirroring the WalletConnect async signing path |

R47.5.12 **Atomic swaps are NOT supported under the MetaMask policy.** An EVM
coin activated under MetaMask shall be treated as a non-swap (balance / address /
`withdraw`) account. Any attempt to use a MetaMask-policy EVM coin in
an atomic swap (as maker or taker) shall be rejected as an unsupported-operation
condition (§47.6) rather than silently producing an unusable swap.

R47.5.13 **Rationale (normative requirement, behavioural).** Atomic swaps impose
two demands that the delegated sign-and-broadcast model (R47.5.6–R47.5.7) cannot
satisfy:

- *Pre-signed, framework-scheduled broadcast.* The swap protocol must build and
  broadcast HTLC transactions on the framework's own timeline (payment, spend,
  refund), and must be able to derive a per-swap HTLC key-pair from a local
  secret to do so. Under MetaMask the framework holds no secret and cannot derive
  that key material; the wallet will only sign transactions it immediately
  broadcasts itself.
- *Detached offline signatures for third-party broadcast.* The swap protocol must
  produce raw, signed refund/spend transactions (and watcher-assisted variants)
  that a **watcher node or the counterparty** may broadcast later. MetaMask never
  yields a detached raw signature or a re-broadcastable signed raw transaction
  (R47.5.7), so the watcher / refund pre-sign requirement cannot be met.

Accordingly, the swap-signing and per-swap HTLC key-derivation paths shall remain
unavailable under the MetaMask policy, and watcher participation for a
MetaMask-policy EVM coin shall be disabled.

### 47.5.D Address / key model

R47.5.14 Under the MetaMask policy the coin has **no local private key**. Its
address (`my_address`) shall be the connected MetaMask account address, and its
public-key display shall be the connected account's public key as recovered at
connect time (§47.1). Private-key export / display for a MetaMask-policy EVM coin
shall be refused as an unsupported operation (the framework cannot expose a key
it does not hold).

R47.5.15 HD / derivation-path operations and any local-secret-dependent feature
(BIP-32/SLIP-10 derivation, address-at-derivation-path, multi-address HD
accounts, local-key message-proofs beyond wallet message signing) are
**unavailable** under the MetaMask policy. The coin presents the single connected
account only; requests that presuppose local-key derivation shall be rejected as
unsupported.

### 47.5.E Error surface (cross-reference)

R47.5.16 On the `enable_eth_with_tokens` wire, a missing/uninitialized MetaMask
session, and an activation-time active-account mismatch, surface through the
platform-coin-with-tokens aggregated activation error contract of ch. 35 (the
MetaMask-specific cause carried in the human-readable `error`, which is not
bound); see §47.6 (R47.6.7) for the bound `error_type` / HTTP mapping. A
post-activation per-operation account mismatch (R47.5.9) and an
unsupported-operation rejection (R47.5.12, R47.5.14, R47.5.15) likewise map per
§47.6.

> **Upstream divergence / open semantics (informative).** The published KDF API
> documentation specifies the `task::connect_metamask` family and the
> `priv_key_policy` MetaMask value, but is largely **silent** on the
> transaction-level semantics of an EVM coin operating under MetaMask — in
> particular whether atomic swaps are offered. The signing/broadcast model
> (R47.5.6–R47.5.7), the operation-support matrix (R47.5.11–R47.5.12), and the
> swap restriction (R47.5.12–R47.5.13) are therefore distilled from current
> framework behaviour and from the structural impossibility of meeting the swap
> pre-sign / detached-signature requirement with a delegate-only browser wallet.
> **Swap-support verdict: UNSUPPORTED under MetaMask** — MetaMask-policy EVM coins
> are non-swap accounts in this project. If the public API docs are later updated
> to define a MetaMask swap flow, the docs govern and this section shall be
> revisited (see §47.10 O-3).

---

## 47.6 Error variants and HTTP status codes

R47.6.1 `task::connect_metamask::init` -- bound `error_type` discriminants:

| `error_type` | `error_data` | HTTP status | Condition |
| --- | --- | --- | --- |
| `MetamaskInitializingAlready` | -- | 400 | a connection is already initializing (R47.1.4) |
| `MetamaskError` | a fieldless MetaMask cause discriminant (see R47.6.2) | 500 | a MetaMask-classified failure |
| `Timeout` | duration | 408 | the task exceeded its time budget |
| `Internal` | string | 500 | any other internal failure |

R47.6.2 The `MetamaskError` `error_data` is a small fieldless discriminant the
GUI may special-case. The bound value set shall include at least:

| MetaMask cause discriminant | Condition |
| --- | --- |
| `EthProviderNotFound` | no EIP-1193 provider was injected (MetaMask not installed/enabled) |
| `UserCancelled` | the user rejected the account request or the login-challenge signature (EIP-1193 code 4001) |
| `UnexpectedAccountSelected` | the active wallet account does not match the connected/expected account (ownership-verification or later re-check failure) |
| `MetamaskCtxNotInitialized` | a MetaMask session was required but none is established |

R47.6.3 `task::connect_metamask::status` -- failure to resolve the supplied
`task_id` shall return the standard task-status no-such-task error (the shared
task-framework status-error contract). In-flight task failures surface as the
`Error` task-status envelope of R47.2.4 carrying the R47.6.1 discriminants.

R47.6.4 `task::connect_metamask::cancel` -- an unknown or already-finished
`task_id` shall return the standard task-framework cancel error (no-such-task /
task-already-finished); these reuse the shared task-cancel error contract used by
every `task::*` namespace.

R47.6.5 EVM activation under the MetaMask policy -- error discriminants are those
of `enable_eth_with_tokens` (ch. 35, R35.1.5), with the MetaMask-not-initialized
case and the activation-time account-mismatch case mapping per R47.6.7.

R47.6.6 Native-target invocation of any `task::connect_metamask::*` method shall
return the dispatcher's standard method-not-found error (the same response any
unrecognized method produces); see §47.7.

R47.6.7 EVM operation under the MetaMask policy -- bound condition-to-status
mapping (the human-readable cause is not bound):

| Condition | Surfacing channel | `error_type` | HTTP status |
| --- | --- | --- | --- |
| No active MetaMask session at activation (not connected) | `enable_eth_with_tokens` aggregated activation error | `Transport` | 500 |
| Active-account mismatch at activation | `enable_eth_with_tokens` aggregated activation error | `Transport` | 500 |
| Per-operation active-account mismatch (R47.5.9) | the invoked operation's error envelope | account-mismatch / transport-class discriminant of that operation | 500 |
| Unsupported operation under MetaMask -- atomic swap (R47.5.12), private-key export (R47.5.14), HD/derivation-path op (R47.5.15) | the invoked operation's error envelope | that operation's unsupported / not-supported discriminant | 400 |
| MetaMask `priv_key_policy` requested on a native build | EVM activation rejects the value | invalid-policy / unsupported discriminant | 400 |

The per-operation discriminant names are inherited from each operation's own
ch. 35 error contract; this chapter binds only the **condition** and its HTTP
status class, not new discriminants.

---

## 47.7 Platform / target gating

R47.7.1 The entire `task::connect_metamask::*` surface, the EVM MetaMask
`priv_key_policy` value, and the EVM MetaMask signing path shall be compiled and
routed **only** on `target_arch = "wasm32"`. MetaMask is a browser extension; the
EIP-1193 provider exists only in the browser/WASM runtime.

R47.7.2 On native targets the `connect_metamask::*` method strings shall be
**absent** from the dispatcher and shall resolve to the standard method-not-found
error (R47.6.6). The `{ "type": "Metamask" }` `priv_key_policy` value shall not be
a valid signing policy on native; a native request carrying it shall be rejected
by EVM activation rather than attempting MetaMask signing.

---

## 47.8 Reloaded substrate (informative, high-level)

Reloaded **already ships**, WASM-only, the lower layers this chapter builds on.
The Coder shall reuse them rather than reinvent the MetaMask handshake.

**Low-level EIP-1193 / session layer (`mm2_metamask` crate):**
- `Eip1193Provider` -- the browser-injected provider transport (detection +
  `request`-style method calls). MetaMask JSON-RPC method names
  (`eth_requestAccounts`, `wallet_switchEthereumChain`, `eth_signTypedData_v4`)
  are wire-dictated and fixed.
- `MetamaskSession` -- a process-serialized session guard exposing
  `eth_request_account`, `wallet_switch_ethereum_chain`, and `sign_typed_data_v4`.
- `MetamaskError` / `MetamaskResult`, and the fieldless `MetamaskRpcError`
  feeding R47.6.2.

**Crypto-context layer (`crypto` crate):**
- `MetamaskCtx` with an async `init(project_name)` that **already performs**
  detect -> request-account -> EIP-712 login-sign -> public-key recover ->
  account-verify, and exposes accessors for the connected account address,
  account public key, the live provider, and a current-account re-check.
- `MetamaskArc` / `MetamaskWeak` reference wrappers.
- `CryptoCtx::init_metamask_ctx(project_name)`, `CryptoCtx::metamask_ctx()`, and
  `CryptoCtx::reset_metamask_ctx()` for establishing, reading, and clearing the
  session.

**Gaps the Coder must implement:**
1. A long-running RpcTask family wrapping `CryptoCtx::init_metamask_ctx` to expose
   `task::connect_metamask::{init,status,cancel}` (§47.1-§47.4). The task's
   `run` shall call `init_metamask_ctx(project)`; its cancel shall call
   `reset_metamask_ctx`. **No `user_action` method** (§47.3). Note: reloaded has
   **no prior user-action RpcTask precedent** (its Trezor surface is a status
   query only), and this connect task does not need one either -- it is a single
   `run` step driven entirely by the browser popup.
2. An EVM private-key policy value that selects MetaMask signing, plus an
   EIP-1193 signing/broadcast path in the EVM coin. The reloaded EVM coin
   currently signs **only** with a locally held secp256k1 secret and has **no
   MetaMask branch** -- this must be added, delegating each transaction to the
   connected `MetamaskSession` via `eth_sendTransaction` (wallet signs **and**
   broadcasts; no offline raw transaction is produced, R47.5.6-R47.5.7),
   enforcing the per-operation active-account re-check (R47.5.9), and keeping the
   swap-signing and per-swap HTLC key-derivation paths unavailable under this
   policy (R47.5.12-R47.5.13).
3. Threading the MetaMask policy through `enable_eth_with_tokens`: the activation
   request's `priv_key_policy` shall accept the WASM-only MetaMask value
   (R47.5.1) and resolve it to the connected `MetamaskCtx` from
   `CryptoCtx::metamask_ctx()`, failing with the not-initialized condition when
   absent (R47.5.2).

The Coder shall not re-implement provider detection, account request, the
EIP-712 login challenge, signature recovery, or account verification -- all of
that is `MetamaskCtx::init`. The task layer is a thin wrapper.

---

## 47.9 Acceptance criteria

A1. On WASM, `task::connect_metamask::init` with `{ "project": "<name>" }` returns
`{ "task_id": <n> }` and spawns exactly one connection task; a second concurrent
`init` returns `MetamaskInitializingAlready` (400).

A2. `task::connect_metamask::status` reports `Initializing` then
`SigningLoginMetadata` while in progress, and on success returns an `Ok` payload
carrying the connected `eth_address`; `forget_if_finished` defaults to true.

A3. With no provider injected, the task fails with `MetamaskError` /
`EthProviderNotFound`; a user rejection fails with `MetamaskError` /
`UserCancelled`; a recovered-address mismatch fails with `MetamaskError` /
`UnexpectedAccountSelected`.

A4. `task::connect_metamask::cancel` aborts a pending task, returns the success
acknowledgement, and resets the crypto-context MetaMask session; an unknown
`task_id` returns the standard no-such-task error.

A5. After a successful connect, `enable_eth_with_tokens` with
`"priv_key_policy": { "type": "Metamask" }` activates the EVM platform coin whose
signing account equals the connected MetaMask account; without a prior connect it
fails with `Transport` (500, the shared platform-coin activation-error status).

A6. An EVM coin activated under the MetaMask policy performs balance / address /
`withdraw` operations by delegating each transaction to the wallet via
`eth_sendTransaction` (wallet signs **and** broadcasts, returns a tx hash) —
never with a local secret, never producing a detached raw signature — and
rejects any operation when the wallet's active account no longer matches the
connected account. Standalone send / `approve` are swap-coupled in reloaded and
not separately exposed under the non-swap policy; message signing is deferred to
a future async RPC path (R47.5.11).

A6b. An attempt to use a MetaMask-policy EVM coin in an atomic swap (maker or
taker), to export its private key, or to perform an HD/derivation-path operation
is rejected as unsupported (R47.5.12, R47.5.14, R47.5.15); watcher participation
is disabled for such coins.

A7. On native builds, every `task::connect_metamask::*` method returns the
standard method-not-found error, and `{ "type": "Metamask" }` is not a valid EVM
`priv_key_policy`.

A8. No `task::connect_metamask::user_action` method is routed on any target.

---

## 47.10 Open questions

O-1. Does the published KDF API documentation list a `user_action` method for
`task::connect_metamask`? Current framework behaviour exposes only
`init`/`status`/`cancel` with an empty user-action type. This chapter specifies
**no** `user_action` (§47.3). If the docs say otherwise, the docs govern and this
should be revisited.

O-2. Does the published `status` `Ok` payload carry more than `eth_address`
(e.g. account public key, wallet name, EIP-155 chain id)? Current behaviour binds
only `eth_address` (R47.2.3). The richer fields exist in the crypto-context
session and could be surfaced from there if the docs require them, without
touching the handshake.

O-3. Does the published KDF API documentation define an atomic-swap flow for EVM
coins under MetaMask? This chapter records the swap-support verdict as
**UNSUPPORTED** (R47.5.12–R47.5.13), distilled from current framework behaviour
and the structural impossibility of meeting the swap pre-sign / detached-raw-
signature requirement with a delegate-only browser wallet. If the docs define
such a flow, they govern and §47.5.C must be revisited.
