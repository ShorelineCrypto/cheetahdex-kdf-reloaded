# Chapter 20 -- Siacoin Network Integration

**Status:** driving-spec

> **One-sentence claim:** the project shall provide a Siacoin
> coin-support module that activates a single-address Sia wallet
> over a walletd HTTP backend, reports balance and broadcasts
> value transfers in Sia's native units, and participates in the
> V1 atomic-swap protocol by realising the HTLC as a native Sia
> spend policy whose success path is unlocked by a SHA-256
> preimage and whose refund path is unlocked by a block-height
> time lock -- conforming on the wire to the public Sia
> consensus/transaction formats and to the walletd HTTP API.

## 20.0 Executive Summary

Siacoin support is a coin-support module within the workspace's
multi-protocol coin crate. It is **not** a UTXO or EVM family
member: Sia has its own consensus model, its own address and
unit encodings, and its own daemon-side wallet (walletd) that
holds chain state. The module therefore owns no local persistent
chain store; it queries walletd on demand.

The integration delegates **all chain-side primitives** -- key
handling, address derivation, transaction construction and
signing, spend-policy HTLC construction, and the walletd HTTP
client -- to an external Sia Rust library bound through that
library's public API. This chapter's subject is the *KDF-side
adapter*: the code that makes a Sia wallet present as a coin to
the rest of the workspace (the coin trait surface, the
activation flow, the swap-protocol participation, the withdraw
path, and the deferred-work boundary).

The bound surface covers:

- a public coin type, generic over its HTTP backend, that the
  workspace's coin enum carries as a dedicated variant;
- a coin-activation RPC surface and a one-time DEX-fee address
  resolution keyed to the active network id;
- the protocol primitives and unit conventions Sia dictates;
- the atomic-swap HTLC realised as a native Sia spend policy,
  and the V1 swap-protocol obligations the module must meet;
- the walletd HTTP endpoints the module consumes;
- the value-transfer (withdraw) behaviour; and
- the explicitly deferred pieces (multi-account HD, history
  persistence, swap-spend search, message signing, V2 swap
  protocol, watcher eligibility).

> **Binding scope (R36).** Throughout this chapter, requirements
> bind observable *behaviour*, the *public* cross-crate interface
> the rest of the workspace calls, and externally *dictated*
> interop (the Sia consensus/transaction wire formats, the
> walletd HTTP API, the unit and address encodings, the SLIP-44
> coin type, the SHA-256 HTLC secret hash, and the public
> activation RPC method strings -- marked R29/R31/R33). The
> private types, field layouts, helper decomposition, control
> flow, local names, transaction-builder call chains, and
> diagnostic wording an implementation uses to meet these
> requirements are informative, not mandated. Where a fragment is
> reproduced because the wire format, the daemon API, or a public
> crate API dictates it, that is stated explicitly and
> distinguished from the discretionary Rust shape around it.

## 20.1 Baseline Verification

The Siacoin integration is a **post-2022** feature. It is absent
from the baseline anchor of [Chapter 02](02-baseline-state.md)
(commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`, 3 June 2022):
no Siacoin coin-support module, no Sia activation path, and no
Sia variant in the workspace coin enum exist at that revision.
The classification here is by *component role and first-
introduction epoch only*; it is derived from the project's own
revision history and does **not** transcribe code.

| Component (role) | Basis |
| --- | --- |
| Siacoin coin-support module (coin type, activation, swap-ops, withdraw) | Introduced after the baseline anchor. |
| Sia variant in the workspace coin enum | Introduced after the baseline anchor. |
| Sia coin-activation task surface | Introduced after the baseline anchor. |

Per §1.3, the entire module is in clean-room remediation scope
(it is **not** baseline-carryforward). This chapter is therefore
a clean-room driving-spec: it states the integration's contract
by behaviour and public/dictated interface only, and does not
reproduce the authorial expression of the post-2022 module.

## 20.2 Subsystem Placement

The Sia coin-support module lives inside the workspace's
multi-protocol coin crate, alongside the UTXO, EVM, and
Tendermint families. The split of responsibilities is binding;
the file layout that realises it is discretionary (R36):

- a **public coin type** and its cross-crate trait
  implementations (§20.3, §20.7);
- a **configuration / activation-request** surface and the
  builder that produces an activated coin (§20.4);
- a **protocol-primitive / unit** surface mediating between Sia's
  native units and the workspace's decimal-amount type (§20.5);
- a **swap-protocol** surface realising the HTLC lifecycle
  (§20.6, §20.7);
- a **withdraw** surface (§20.9).

All chain-side primitives are supplied by an external Sia Rust
library consumed through its public API. The module binds that
library; it does not vendor or reproduce its internals. The
network-id-keyed DEX-fee configuration is read from the
workspace's network-config substrate (see
[Chapter 06](06-network-id-seed-node.md)).

## 20.3 Public Coin Type and Cross-Crate Surface

The module exposes a public coin type that is **generic over its
HTTP backend**: the coin is parameterised by the Sia API client
trait(s) it talks to, so that a test build can substitute a mock
client without compiling out production behaviour, and a public
type alias fixes the concrete production client for normal use.

> **Binding scope (R36).** The genericity over the API-client
> trait and the existence of a concrete production alias are
> binding architectural facts. The exact struct name, its field
> set, the interior-mutability choices (e.g. atomic vs. locked
> storage of the confirmation count), and the private field used
> to cache the fee address are discretionary realisation details.

Binding cross-crate facts:

**R-T1.** The coin type MUST implement the workspace's standard
coin trait surface -- the general coin trait, the market/
chain-operations trait, and the V1 swap-operations trait -- so
that the swap engine, the RPC layer, and the activation layer
interact with Sia through the same abstractions they use for
every other coin. These traits are the *public* cross-crate
contract; their method semantics are bound by the coin-trait
and swap chapters, not redefined here.

**R-T2.** The coin type MUST be carried by the workspace coin
enum as a dedicated Sia variant, so that coin-generic call sites
(swap engine, RPC dispatch, balance reporting) can dispatch on
it.

**R-T3.** The coin MUST carry a cross-coin private-key policy
value (the same policy abstraction used by other coins: legacy
single-key, derived HD account, hardware-wallet placeholder).
At the time of writing only the single-key and the single-
address HD-account paths reach a successfully activated coin;
every other policy value MUST fail activation with a typed
error rather than panicking.

**R-T4.** The DEX-fee destination address MUST be resolved
exactly once, at activation, and cached on the coin; per-swap
code MUST reference the cached value rather than re-deriving it.

## 20.4 Activation and Fee-Address Resolution

Siacoin activates through the workspace's **standalone-coin
task-activation** mechanism. The public RPC method strings are
dictated by the activation framework's namespace convention and
are part of the external API contract:

| Method string | Role |
| --- | --- |
| `task::enable_sia::init` | Begin Sia activation (long-running task) |
| `task::enable_sia::status` | Poll activation status |
| `task::enable_sia::user_action` | Supply user action (e.g. HW interaction) |
| `task::enable_sia::cancel` | Cancel activation |
| `enable_sia` | Legacy one-shot activation |

> **Binding scope (R36).** The method strings above are dictated
> public API (R33). The activation-request shape is a public
> deserialization contract: it MUST carry at least the walletd
> client configuration (base URL and auth, owned by the client)
> and the required-confirmations setting, and it MAY carry a
> single-address-mode gap-limit field that is currently accepted
> but not yet acted upon. The concrete Rust type name and field
> identifiers of that request are discretionary except where a
> field name is itself part of the JSON contract a caller sends.

**R-A1 (activation flow).** Activation MUST:

1. Parse the coin's static configuration (ticker, required
   confirmations) from the coin's JSON config entry.
2. Derive the active Sia keypair from the supplied private-key
   policy: a single-key policy yields the key directly; an
   HD-account policy yields a single derived address via SLIP-10
   ed25519 derivation (§20.4.1); any other policy fails with a
   typed error.
3. Construct the walletd HTTP client from the request's client
   configuration. The client owns the walletd base URL and auth;
   **no walletd URL is embedded in the module.**
4. Resolve the DEX-fee address for the active network id
   (§20.4.2) and cache it on the coin.

### 20.4.1 HD derivation path (dictated coin type)

When the HD-account policy is used, the module derives a single
address using SLIP-10 ed25519 derivation under the Sia BIP-44/
SLIP-44 registered **coin type 1991**. The coin-type value is
dictated by the public SLIP-44 registry; operating in
single-address mode (one derived address per HD account, rather
than full multi-account discovery) is a project convention bound
as behaviour (§20.10 D1 names the multi-account gap). The
account/change/index components below the coin type are fixed
for single-address mode.

> **Binding scope (R36).** Coin type `1991` and the SLIP-10
> ed25519 scheme are dictated (R29/R33). The exact derivation-
> path string constant, the lazy-static carrier, and the
> single-address-mode index choices are discretionary.

### 20.4.2 DEX-fee address resolution (dictated by fee routing)

The module MUST look up the DEX-fee public key configured for
the active network id (via the network-config substrate of
[Chapter 06](06-network-id-seed-node.md) / the fee-routing engine
of [Chapter 08](08-fee-routing-engine.md)), decode it as an
ed25519 public key, and derive the corresponding Sia address.
This is the only place the fee address is computed. The set of
per-network fee public keys is dictated by the fee-routing
configuration, not by this module.

## 20.5 Protocol Primitives and Units (dictated interop)

The following are dictated by the public Sia network protocol and
its public Rust library API; the module binds them, it does not
define them. They are reproduced here only as the interop contract
(R29/R31/R33):

| Primitive | Dictated form |
| --- | --- |
| Signature scheme | ed25519 |
| Public key | 32-byte ed25519 public key |
| Address | derived from the public key; Sia hex address with checksum |
| HTLC secret hash | SHA-256 of the swap preimage |
| Transaction model | Sia V2 transactions, JSON-serialised on the walletd wire |
| Transaction id | Sia transaction hash (`Hash256`) |
| Smallest unit | hastings; **1 SC = 10^24 hastings** |

**R-U1 (units).** All amounts the module exchanges with the rest
of the workspace MUST be denominated in whole SC using the
workspace decimal-amount type, converting to/from hastings at the
dictated `10^24` ratio. Balance reporting, withdraw amounts, and
swap amounts all cross this boundary.

> **Binding scope (R36).** The ed25519 scheme, the SHA-256 secret
> hash, the address/units encodings, and the V2-transaction JSON
> wire form are dictated by Sia and walletd. The Rust types that
> carry them (supplied by the external Sia library) are that
> library's public API; the module's private unit-conversion
> helpers and constants are discretionary.

## 20.6 Atomic-Swap HTLC as a Native Spend Policy

Sia has no Bitcoin-style locking script and no Solidity contract.
The HTLC is realised as a **native Sia spend policy** constructed
through the external Sia library's public API. The behavioural
contract the module MUST meet:

**R-H1 (HTLC output).** A swap payment MUST fund a transaction
output locked to a spend policy that encodes: the two
counterparties' ed25519 public keys, a block-height time lock,
and the SHA-256 secret hash. The funding output MUST be placed at
a fixed, deterministic position in the funding transaction so
that validation and spend code can locate it positionally without
scanning.

**R-H2 (success path).** The spending counterparty MUST be able
to claim the output by satisfying the policy with the secret
preimage together with a valid signature. A claim is only valid
when `SHA-256(preimage)` equals the secret hash embedded in the
output.

**R-H3 (refund path).** The original funder MUST be able to
reclaim the output after the time lock elapses by satisfying the
policy with a signature plus the elapsed time-lock condition.

**R-H4 (refund eligibility).** Whether the refund path is yet
spendable MUST be decided by comparing the chain's median block
timestamp against the locktime, using the walletd consensus
tip-state (§20.8), not local wall-clock time.

> **Binding scope (R36).** The atomic-swap spend-policy shape,
> its success/refund satisfaction forms, and the SHA-256 binding
> are dictated by the Sia consensus model and the external
> library's public spend-policy API (R29/R31). The local
> variable names, the builder/satisfier call chains, and the
> fixed output-index constant are discretionary.

## 20.7 V1 Swap-Protocol Obligations

The module implements the workspace's **V1** swap-operations
trait (the V2 protocol is deferred, §20.10 D5). This section
binds the swap obligations behaviourally; it does **not**
enumerate internal method bodies. The trait method *names* are
the public cross-crate contract defined by the coin crate and
bound by the swap chapters ([Chapter 13](13-swap-version-negotiation.md));
their per-coin realisation for Sia MUST provide:

**R-S1 (DEX fee).** Send the taker DEX fee as a Sia value
transfer to the resolved fee address (§20.4.2), carrying the swap
UUID in the transaction's arbitrary-data channel so the fee can
be correlated to the swap and validated.

**R-S2 (payments).** Send maker and taker HTLC funding payments
per §20.6 (R-H1).

**R-S3 (spends).** Spend a counterparty's HTLC payment via the
success path (R-H2), supplying the secret preimage; mirror for
both maker and taker roles.

**R-S4 (refunds).** Reclaim one's own HTLC payment via the refund
path (R-H3) once eligible (R-H4); mirror for both roles.

**R-S5 (validation).** Validate an observed DEX-fee transaction
(output destination, amount, embedded UUID, minimum height) and
an observed HTLC payment (output amount and that the output is
locked to the expected spend policy for the agreed parameters).

**R-S6 (secret extraction).** Given a spend transaction that took
the success path, recover the secret preimage from the satisfied
policy's revealed preimage(s) by matching `SHA-256(preimage)`
against the expected secret hash.

**R-S7 (payment lookup).** Answer "has my payment been sent" by
querying walletd for the relevant address event(s).

**R-S8 (deferred swap-spend search).** The swap-spend search
operations MAY return "not found" pending the deferred event-walk
implementation (§20.10 D3); this is a documented gap, not a
correctness claim.

> **Binding scope (R36).** This section binds swap *behaviour*
> and references the public trait surface only. No per-method
> table keyed to private helpers, no method bodies, and no
> diagnostic strings are part of this contract.

## 20.8 Walletd HTTP Surface (dictated API)

All chain interaction goes through the external Sia library's
walletd HTTP client. The endpoint paths below are the **public
walletd HTTP API** (dictated interop, R29/R33); the module
consumes them, and the base URL plus auth come entirely from the
activation request -- no URL is embedded in the module:

| Endpoint | Module use |
| --- | --- |
| `GET /api/consensus/tip` | Current chain height and state |
| `GET /api/consensus/tipstate` | Recent block timestamps; the median is the refund-eligibility cutoff (R-H4) |
| `GET /api/addresses/:addr/events` | Address event/transaction history |
| `GET /api/addresses/:addr/outputs/siacoin` | UTXO set for funding |
| `GET /api/outputs/siacoin/:output_id/spent` | Locate a spending transaction |
| `GET /api/txpool/transactions` | Mempool inspection |
| `POST /api/txpool/broadcast` | Broadcast a signed transaction |

## 20.9 Withdraw (Value Transfer) Behaviour

The module provides a withdraw path producing a signed,
broadcastable Sia transaction for a plain value transfer. The
behavioural contract:

**R-W1.** Resolve the source address from the active keypair and
the destination address by parsing the request string as a Sia
address.

**R-W2.** Fetch the source address's UTXO set from walletd and
assemble a V2 transaction with one output of the requested amount
and a change output back to the source address.

**R-W3.** Set the miner fee from the request value. Size-aware
fee estimation is **not** performed at the time of writing
(§20.10 D6).

**R-W4.** Sign the transaction with the active keypair and return
the serialised transaction together with fee/amount details
denominated in SC (converted from hastings per R-U1).

## 20.10 Deferred Work

The following are explicit gaps, documented as deferred work
rather than defects. None is a correctness claim.

- **D1 -- Multi-account HD.** Only single-address HD derivation
  is wired (§20.4.1). Full multi-account HD support (gap-limit
  honouring, address discovery, multiple receive addresses) is
  deferred; the HD-wallet implementation surface is a stub.
- **D2 -- History persistence.** Sia chain state lives in
  walletd; the module keeps no SQLite/IndexedDB history store and
  the history-sync loop is inert (in-memory status only).
- **D3 -- Swap-spend search.** The event-walk that locates the
  spend of an HTLC output by id is not yet wired; the
  corresponding swap operations report "not found" (R-S8).
- **D4 -- Message signing.** Generic message sign/verify report
  "unsupported"; the ed25519 primitives exist but the
  coin-level wiring is absent.
- **D5 -- V2 swap protocol.** Neither V2 swap-operations trait is
  implemented for Sia; swaps involving Sia run only over the V1
  protocol ([Chapter 13](13-swap-version-negotiation.md)).
- **D6 -- Size-aware fee estimation.** The withdraw path takes
  the fee from the request rather than estimating from
  transaction size.
- **D7 -- Watcher eligibility.** The watcher-operations surface
  is empty; Sia HTLCs are not watcher-eligible.
- **D8 -- Raw-transaction fetch.** A unified historical
  raw-transaction fetch is not implemented; walletd's mempool
  lookup is the only available path in the bound client version.

## 20.11 Baseline Verifications

The following are verifiable from the baseline state defined in
[Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains **no** Siacoin coin-support
    module. A directory listing of the baseline coin crate
    (`git ls-tree`) shows no Sia module directory, and a
    tree-wide `git grep -li 'siacoin'` against the baseline
    returns no coin-support matches, confirming the feature is
    post-2022.

V2. The baseline workspace coin enum carries no Sia variant, and
    the baseline activation layer registers no `enable_sia` /
    `task::enable_sia::init` surface. The activation method
    strings of §20.4 are therefore additions, dictated by the
    standalone-coin task-activation namespace convention.

V3. The unit ratio, ed25519 scheme, address encoding, walletd
    endpoint paths, and the SHA-256 HTLC secret hash of §20.5 and
    §20.8 are properties of the public Sia protocol and the
    walletd HTTP API, not artefacts of any project lineage.

## 20.12 Provenance Footer

- *Inputs:* the project's own revision history (used for the
  epoch classification of §20.1 and the baseline verifications of
  §20.11, by component role and first-introduction epoch only --
  no code transcribed); the baseline anchor of
  [Chapter 02](02-baseline-state.md); the public Sia network
  protocol (consensus/transaction V2 formats, ed25519 signatures,
  spend-policy model, hex address + checksum encoding, the
  hastings unit and the `1 SC = 10^24 hastings` ratio); the
  public walletd HTTP API (the endpoint paths of §20.8); the
  public SLIP-44 registry (Sia coin type `1991`) and SLIP-10
  ed25519 derivation; the public Sia Rust library API the module
  binds (the key/address/transaction/spend-policy and API-client
  types); and cross-chapter contracts (Chapters 06, 08, 13).
- *Permitted-input classes used:* baseline source (epoch
  classification and absence verification only); external public
  specifications (the Sia protocol and consensus/transaction
  formats, the walletd HTTP API, SLIP-44/SLIP-10, the public Sia
  Rust library API); cross-chapter contracts (Chapters 06, 08,
  13); Interop / wire-and-API-bound reuse (R29/R31/R33) for the
  dictated fragments embedded in §20.4 (the `enable_sia` /
  `task::enable_sia::*` public method strings), §20.4.1 (the
  SLIP-44 coin type and SLIP-10 scheme), §20.5 (the ed25519
  scheme, SHA-256 secret hash, address/units encodings, and V2
  transaction wire form), §20.6 (the spend-policy HTLC shape and
  its success/refund satisfaction forms), and §20.8 (the walletd
  endpoint paths) -- whose authoritative source is the bytes and
  calls any conforming Sia node, walletd instance, or library
  consumer must exchange for interoperability, not the historical
  lineage's discretionary expression.
- *Sibling-allowlist consultations:* Chapter 06 (the network-id
  configuration the fee-address resolution reads); Chapter 08
  (the fee-routing engine that owns the per-network DEX-fee
  public keys); Chapter 13 (the swap version-negotiation path and
  the V1/V2 trait boundary the Sia swap-ops sit on).
- *Forbidden corpus:* consulted **only** to recover the
  externally-dictated public method strings of §20.4, the
  dictated derivation/coin-type and protocol/units facts of
  §20.4.1 and §20.5, the dictated spend-policy HTLC contract of
  §20.6, and the dictated walletd endpoint paths of §20.8 --
  embedded as Interop reuse under R29/R31/R33. No discretionary
  expression from the post-2022 module -- no struct field lists,
  internal type or enum definitions, private method names or
  bodies, transaction-builder/satisfier call chains, local
  variable names, per-method tables keyed to internal names,
  control-flow transcription, or diagnostic / panic / log string
  literals -- crosses into this chapter. All other content is
  clean-room driving-spec stated by behaviour and public/dictated
  interface. Any residual similarity of a conformant realisation
  to the historical lineage is governed by the R35 gate and the
  R36 binding-scope notes that head every code-bearing section of
  this chapter.
