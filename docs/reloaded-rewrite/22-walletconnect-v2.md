# Chapter 22 -- WalletConnect v2

**Status:** driving-spec

> **One-sentence claim:** the project provides an in-tree
> WalletConnect v2 dApp implementation that exposes a per-chain
> signing trait to coin support modules and persists sessions
> across native and browser builds, conforming byte-for-byte to
> the public WalletConnect v2 specification on the wire.

## 22.0 Executive Summary

A dedicated subsystem in this codebase implements the
WalletConnect v2 protocol as a **relay client / dApp**: it
generates pairing URIs, proposes and maintains sessions with
external wallets, and dispatches signing requests over an
encrypted JSON-RPC channel. It does *not* implement the
wallet-side role -- the codebase never settles inbound sessions
on behalf of a remote dApp and never signs on behalf of an
external requester.

The subsystem is divided cleanly between:

1. A **protocol layer** carrying the WC2 wire format, pairing
   and session lifecycle, encrypted envelope codec, and relay
   websocket loop.
2. A **persistence layer** that mirrors the active session set
   to durable storage on both native (SQLite) and browser
   (IndexedDB) builds, behind a single trait.
3. An **integration trait** that coin support modules implement
   to surface WalletConnect-backed signing for the chain family
   they own. The trait is intentionally minimal and chain-family
   agnostic.

At the time this chapter is written the trait exists in the tree
and the protocol/persistence layers are functionally complete;
no coin support module yet implements the trait, and the public
RPC dispatcher does not yet register WalletConnect handlers. The
trait surface and persistence shape are binding; the integration
gap is the explicit work named in §22.9.

## 22.1 Subsystem Shape

The subsystem is a standalone library compilation unit. It
depends only on:

- Standard async-Rust crates (futures, tokio-style channels in
  the workspace's runtime selection).
- The workspace's central context type (see
  [Chapter 8](08-mm-ctx-and-state-layering.md)) for the lazy
  per-context handle pattern.
- The workspace's database abstractions (see
  [Chapter 25](25-sql-query-builder.md) for the native SQL path
  and the IndexedDB wrapper for the browser path).
- Third-party crates carrying the WalletConnect v2 relay client,
  pairing API, and Type 0 envelope codec. These are external
  Cargo dependencies, not vendored sources; the codebase invokes
  them through their public Rust APIs and does not reproduce
  their internals.

The subsystem **does not depend on any coin support module**.
Coin support modules depend on the subsystem (through the
integration trait of §22.5), never the other way around. This
direction is a binding architectural rule: the WalletConnect
subsystem must remain coin-agnostic so it can be compiled and
tested without any chain backend present.

Internally the subsystem groups its source into the following
functional regions:

| Region              | Responsibility                                     |
|---------------------|----------------------------------------------------|
| Public API          | Per-context handle, pairing-URI struct, re-exports |
| Chain taxonomy      | CAIP-2 chain family enum, request-method enum      |
| Error               | Error enum with mapped WalletConnect error codes   |
| Inbound routing     | Request/response demultiplex by message id         |
| Connection handler  | Relay websocket event loop                         |
| Pairing             | Pairing lifecycle (propose, accept, expire)        |
| Metadata            | App identity, auth-token constants                 |
| Session             | Session struct, manager, key material              |
| Session RPC         | One submodule per WC2 method (propose, settle,    |
|                     | update, delete, event, extend, ping)               |
| Storage             | Trait + native and browser implementations         |

The subsystem is bounded in size (on the order of a couple of
thousand lines of Rust); it contains its own serialization
tests, no relay-loop or crypto tests.

## 22.2 Public Handle and Connection API

The subsystem exposes a single public handle type, accessed
through the per-context lazy-init pattern of
[Chapter 8](08-mm-ctx-and-state-layering.md). Call sites obtain
the handle from the central context; they do not construct it
directly.

The handle exposes the following operations:

| Operation                          | Purpose                              |
|------------------------------------|--------------------------------------|
| Generate a new pairing             | Returns a pairing topic and a `wc:`  |
|                                    | URI to be shown to the user          |
| Send a signing request, await reply| Encrypts the JSON-RPC payload, sends |
|                                    | over the relay, awaits the response  |
| Drop a session                     | Sends the WC2 delete RPC, removes    |
|                                    | the persisted row, unsubscribes      |
| Encode an outbound payload         | Applies the negotiated transport     |
|                                    | encoding (hex by default; base64 for |
|                                    | wallets that require it)             |
| Resolve account for a chain        | Looks up the active account address  |
|                                    | and metadata for a given chain id    |
| Wallet-type detection              | Identifies certain wallet families   |
|                                    | (Ledger Cosmos app; Keplr) where the |
|                                    | wire path must differ                |

The pairing URI follows the public WC2 format
(`wc:<topic>@2?...`) and is opaque to the codebase: it is
delivered verbatim to the consumer (a GUI, a CLI, a deep link).

## 22.3 The Integration Trait

Coin support modules that wish to expose WalletConnect-backed
signing implement an in-tree trait with the following shape:

- A method that returns the CAIP-2 chain id the coin is bound
  to, given the WalletConnect handle.
- A method that signs an unsigned transaction (associated
  parameter type and associated return type, both chosen by
  the implementor).
- A method that signs and broadcasts a transaction (same
  associated-type pattern).
- A method that returns the pairing topic the coin should use.

The trait is intentionally narrow: a chain-id resolver, a sign
flow, a send flow, and a session pointer. Associated types let
each chain family pick its own parameter and return shapes
(EVM transaction objects, Cosmos sign-direct payloads, UTXO
PSBTs) without the WalletConnect subsystem needing to know
anything about the specific tickers, transaction encodings,
or contract addresses involved.

The binding rule is that the trait must remain chain-family
agnostic in this codebase: the subsystem must not gain
knowledge of EVM, Cosmos, or UTXO transaction structures.
Chain-specific logic belongs in the coin support module that
implements the trait.

## 22.4 Protocol Role

The codebase fills the **relay-client / dApp** role of WC2 in
full and the **wallet** role not at all.

| Step                              | Codebase | External wallet |
|-----------------------------------|----------|------------------|
| Initiate pairing                  | yes      | no               |
| Send `wc_sessionPropose`          | yes      | no               |
| Receive `wc_sessionSettle`        | yes      | yes (sends)      |
| Send `wc_sessionRequest` (sign)   | yes      | no               |
| Sign and respond                  | no       | yes              |
| Broadcast the signed transaction  | depends on the WC method chosen   | usually yes |

There is no inbound-session settlement and no signing on behalf
of external dApps. Adding the wallet role would be a substantial
new feature and is not in scope for this subsystem.

## 22.5 Session Storage

Sessions persist across process restarts. A single trait
abstracts the storage backend; the build chooses between two
implementations:

| Build target | Implementation                                              |
|--------------|-------------------------------------------------------------|
| Native       | SQLite via the workspace's async SQL abstraction            |
| Browser/WASM | IndexedDB via the workspace's IndexedDB wrapper             |

The native schema is a single table keyed by topic:

```sql
CREATE TABLE wc_session (
    topic   CHAR(32) PRIMARY KEY,
    data    TEXT     NOT NULL,
    expiry  BIGINT   NOT NULL
);
```

The browser schema is the same shape: a single object store
keyed by topic, with the same three columns.

Lifecycle on relay connect:

1. Load every persisted session.
2. Expire any row whose `expiry` is in the past.
3. Re-subscribe to the session and pairing topics so that
   messages delivered while the relay was disconnected are
   processed.
4. Update rows in place when session state changes.
5. On user-initiated disconnect, delete the row and unsubscribe.

The binding rule is that the storage layer must remain a single
trait with one row per session; both backends store the same
opaque-JSON payload. Schemas may be extended additively but
must remain compatible between backends.

## 22.6 Cryptography

The transport-layer cryptography is fully specified by WC2 and
is reproduced byte-for-byte. The codebase performs:

1. **Key exchange.** x25519 ECDH between an ephemeral
   `StaticSecret` and the peer's `PublicKey`, producing a 32-byte
   shared secret.
2. **Symmetric-key derivation.** HKDF-SHA256 over the shared
   secret with **empty salt** (`None`) and **empty info**
   (`&[]`), expanded to 32 bytes. The empty salt and info are
   mandated by the WC2 specification; they are not authorial
   choices and must be matched byte-for-byte by any
   interoperable implementation.
3. **Symmetric encryption.** ChaCha20-Poly1305 with the
   derived key, packaged in the canonical WC2 Type 0 envelope
   (version byte, salt, ciphertext, Poly1305 tag). The envelope
   codec is delegated to the external WalletConnect SDK crate
   referenced by Cargo; the codebase does not reimplement the
   envelope byte layout.

The symmetric key is masked in any `Debug` output (substituted
with `"*******"`). The session-key type **shall additionally
implement zeroize-on-drop**; at the time of writing the
implementation masks but does not zeroize, and this is named
explicitly in §22.9 as required follow-on work.

## 22.7 Encrypted JSON-RPC Envelopes

Each WalletConnect message on the wire is a Type 0 envelope
wrapping a JSON-RPC payload. The envelope structure is:

| Field      | Size                | Notes                                  |
|------------|---------------------|----------------------------------------|
| Version    | 1 byte              | Type 0 = 0x00                          |
| Salt       | 32 bytes            | Per-message nonce material             |
| Ciphertext | variable            | ChaCha20-Poly1305 over JSON-RPC bytes  |
| Tag        | 16 bytes            | Poly1305 authentication tag            |

The codebase calls the external SDK's encode and decode entry
points to produce and consume envelopes; this is the only path
through which session-keyed bytes leave or enter the subsystem.

The **transport encoding** is applied to the whole envelope
after the envelope is produced:

- Hex encoding is the default and is used for most wallets.
- Base64 encoding is used for wallets that require it (Keplr is
  the notable case in this category).

The wallet-type detection in §22.2 selects the encoding.

A JSON-RPC payload on the WC channel has the standard shape:

```jsonc
{
  "jsonrpc": "2.0",
  "id":      <numeric message id>,
  "method":  "wc_sessionRequest",
  "params":  {
    "chainId": "eip155:1",
    "request": {
      "method": "eth_signTransaction",
      "params": [ /* method-specific */ ]
    }
  }
}
```

**Request/response correlation** uses a oneshot channel keyed
by the JSON-RPC message id. The send path registers the
oneshot under the id, the inbound router matches incoming
responses to pending ids and wakes the waiter, and a fixed
time-to-live (five minutes at the time of writing) caps the
wait if no response arrives. The TTL is currently a constant;
a per-call override is named as follow-on work in §22.9.

## 22.8 Chain Taxonomy and Request Methods

The subsystem models the multi-chain surface of WC2 through
CAIP-2 chain identifiers (`<family>:<reference>`). Three chain
families are recognised at the time of writing:

| CAIP-2 family    | Meaning                                          |
|------------------|--------------------------------------------------|
| `eip155:<id>`    | Ethereum and EVM-compatible chains               |
| `cosmos:<id>`    | Cosmos SDK chains                                |
| `bip122:<hash>`  | UTXO chains (Bitcoin family, prefix of genesis)  |

Adding a new family is an additive change (a new enum variant
plus a new RPC submodule for the methods that family exposes).
Removing a family would be a breaking change to coin
implementors and is not envisaged.

The wire method names the subsystem is prepared to issue on
behalf of an integration are enumerated explicitly; the
mapping from internal variant to wire name is one-to-one and
total. At the time of writing the set is:

| Variant family   | Wire method name             | Chain family |
|------------------|------------------------------|--------------|
| Sign EVM tx      | `eth_signTransaction`        | eip155       |
| Send EVM tx      | `eth_sendTransaction`        | eip155       |
| EVM personal sign| `personal_sign`              | eip155       |
| Cosmos direct    | `cosmos_signDirect`          | cosmos       |
| Cosmos amino     | `cosmos_signAmino`           | cosmos       |
| Cosmos accounts  | `cosmos_getAccounts`         | cosmos       |
| UTXO accounts    | `getAccountAddresses`        | bip122       |
| UTXO send        | `sendTransfer`               | bip122       |
| UTXO sign PSBT   | `signPsbt`                   | bip122       |
| UTXO personal    | `personal_sign` (UTXO route) | bip122       |

`cosmos_signAmino` is the Ledger-compatible path (the Cosmos
Ledger app supports Amino-JSON sign payloads only);
`cosmos_signDirect` is the default for software wallets.

`eth_signTypedData_v4` is intentionally not in the enum at the
time of writing; it is a common EVM method and is expected to
be added when the first integrator needs it. Adding it is an
additive enum + match-arm change.

## 22.9 Binding Requirements and Deferred Work

The following are **binding rules** for this subsystem and any
coin support modules that integrate with it:

R1. **Coin agnosticism.** The WalletConnect subsystem must not
    depend on any coin support module. Chain-family-specific
    transaction encoding lives in the integrating coin module.

R2. **Single trait integration boundary.** Coin integration
    must be expressed through the integration trait of §22.3.
    No coin module may reach into the subsystem's internals.

R3. **Spec-byte-faithful crypto.** The HKDF salt and info, the
    Type 0 envelope layout, and the JSON-RPC message shape are
    set by the WC2 specification and are not authorial. Any
    change that diverges from spec bytes is a bug.

R4. **Storage uniformity.** Both storage backends must expose
    the same trait, the same row shape, and the same lifecycle
    (load -> expire -> re-subscribe -> update -> delete).

R5. **Zeroize-on-drop for session-key material.** The
    session-key type shall implement zeroize-on-drop. At the
    time of writing the type masks the key in `Debug` only;
    closing this gap is required follow-on work.

R6. **Public dispatcher integration.** The codebase shall expose
    the subsystem's user-facing operations (start pairing,
    list sessions, drop session) through the public RPC
    dispatcher. The subsystem's public handle methods are the
    intended targets of those RPCs; the dispatcher wiring is
    required follow-on work.

The following items are **deferred work** required to reach a
"first integration shipped" milestone:

D1. At least one coin support module shall implement the
    integration trait. The EVM and Cosmos families are the
    natural first integrators given the trait surface and the
    request-method enum.

D2. Public RPC handlers shall be registered (see R6).

D3. The session-key type shall acquire `Zeroize` /
    `ZeroizeOnDrop` (see R5).

D4. Wallet-type detection currently uses heuristics
    (`sessionProperties.keys[0].is_nano_ledger` for Ledger;
    `controller.metadata.name == "Keplr"` for Keplr). These are
    documented but not normative; a more robust capability-based
    detection is desirable.

D5. CAIP-10 account-address parsing currently uses a simple
    delimiter split. Stricter validation would reject malformed
    wallet responses earlier.

D6. No defence-in-depth message deduplication is implemented;
    the codebase relies on the relay to deduplicate. A local
    dedup layer is desirable.

D7. The response time-to-live is a constant. A per-call override
    is desirable.

## 22.10 External References

- The WalletConnect v2 protocol specification (transport,
  pairing, session, JSON-RPC envelope, namespaces). The
  binding spec for the wire-level behaviour described above.
- CAIP-2 (chain identifiers) and CAIP-10 (account identifiers)
  for the namespace identifiers used in §22.8.
- The Ethereum JSON-RPC method names (`eth_signTransaction`,
  `eth_sendTransaction`, `personal_sign`, `eth_signTypedData_v4`)
  as defined by the Ethereum and EIP standards.
- The Cosmos signing schemes (Amino-JSON and SignDirect /
  Protobuf) as defined by the Cosmos SDK.
- The PSBT format (BIP-174) for the UTXO `signPsbt` flow.
- RFC 5869 (HKDF), RFC 7539 (ChaCha20-Poly1305 / Poly1305),
  RFC 7748 (x25519) for the cryptographic primitives.

## 22.11 Baseline Verifications

The following are verifiable from the baseline state defined in
[Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains **no** WalletConnect v2
    subsystem. A directory listing of the baseline tree
    (`git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f`)
    returns no path containing any WalletConnect-related crate
    or module. The subsystem described in this chapter is
    therefore material introduced after the baseline in its
    entirety.

V2. The baseline tree contains no integration trait of the
    shape described in §22.3. A tree-wide `git grep` for the
    integration-trait name against the baseline returns no
    matches.

V3. The third-party Cargo dependencies that provide the relay
    client and Type 0 envelope codec are referenced from the
    workspace `Cargo.toml` by tag-pinned git URL. The codebase
    consumes their public Rust APIs only; no vendored copies of
    their sources are present.

## 22.12 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  subsystem at baseline verified via
  `git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the integration-trait name
  against the baseline; the public WalletConnect v2
  specification; the CAIP-2 and CAIP-10 namespaces; the
  Ethereum JSON-RPC method definitions; the Cosmos SDK signing
  schemes (Amino and SignDirect); BIP-174 (PSBT); RFC 5869
  (HKDF), RFC 7539 (ChaCha20-Poly1305), RFC 7748 (x25519).
- *Forbidden corpus:* not consulted.
