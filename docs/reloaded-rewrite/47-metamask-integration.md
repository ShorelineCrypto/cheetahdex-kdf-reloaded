# Chapter 47 -- MetaMask (Browser EIP-1193 Wallet) Integration

**Status:** driving-spec (required port). This chapter specifies an **RPC and
activation-policy surface** over MetaMask plumbing that already exists in
reloaded; it does not introduce a new coin or a new low-level transport.

> **One-sentence claim:** the project shall expose, **on the WASM target only**,
> a long-running connection task `task::connect_metamask::{init,status,cancel}`
> that establishes an authenticated MetaMask session in the framework crypto
> context, and shall let EVM coins (ch. 35) activate under a MetaMask signing
> policy so that swap signing is delegated to the connected browser wallet
> instead of a locally held secret.

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

## 47.5 EVM-coin activation under a MetaMask signing policy

R47.5.1 The V2 EVM platform activation RPC `enable_eth_with_tokens` (ch. 35)
shall accept a MetaMask signing policy selected through its `priv_key_policy`
field. The wire shape is the tagged policy object of R35.1.4: a MetaMask policy
is `"priv_key_policy": { "type": "Metamask" }` (no payload). This policy value is
defined **only on the WASM target**.

R47.5.2 When the MetaMask policy is selected, activation shall consume the
already-connected MetaMask session from the crypto context (established via
§47.1). If no MetaMask session is present, activation shall fail with the
context-not-initialized condition of §47.6 (R47.5.6), instructing the caller to
run `task::connect_metamask::init` first. Activation shall **not** itself perform
the connection handshake.

R47.5.3 An EVM coin activated under the MetaMask policy shall delegate all
transaction and message signing to the connected MetaMask session over EIP-1193,
rather than signing with a locally held secp256k1 secret. The EVM coin's account
public key and address for swaps shall be those of the connected MetaMask
account.

R47.5.4 Address-consistency requirement: the address the activated EVM coin signs
for shall be the connected MetaMask account verified at connect time. Because the
user can switch the active account inside the extension at any moment, signing
under the MetaMask policy shall verify, before each signing operation, that the
wallet's currently active account still equals the connected account; a mismatch
shall fail the signing operation with the account-mismatch condition of §47.6
rather than signing with the wrong key.

R47.5.5 The MetaMask policy applies to the EVM platform coin and, transitively,
to its ERC-20 child tokens activated in the same `enable_eth_with_tokens` call;
the tokens inherit the platform's signing policy.

R47.5.6 On the `enable_eth_with_tokens` wire, a missing/uninitialized MetaMask
session surfaces as the platform-coin-with-tokens **`Transport`** `error_type`
(HTTP 502), consistent with ch. 35's aggregated activation error contract; the
MetaMask-specific cause is conveyed in the human-readable `error` text, which is
not bound.

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
case mapping to `Transport`/502 per R47.5.6.

R47.6.6 Native-target invocation of any `task::connect_metamask::*` method shall
return the dispatcher's standard method-not-found error (the same response any
unrecognized method produces); see §47.7.

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
   EIP-1193 signing path in the EVM coin. The reloaded EVM coin currently signs
   **only** with a locally held secp256k1 secret and has **no MetaMask signing
   branch** -- this must be added, delegating signing to the connected
   `MetamaskSession` and enforcing the per-signature active-account re-check
   (R47.5.4).
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
fails with `Transport` (502).

A6. An EVM coin activated under the MetaMask policy signs swap/withdraw
transactions via the MetaMask session, never with a local secret, and rejects
signing when the wallet's active account no longer matches the connected account.

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
