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
wallet-side role -- the project never settles inbound sessions
on behalf of a remote dApp and never signs on behalf of an
external requester.

The subsystem is divided cleanly between:

1. A **protocol layer** carrying the WC2 wire format, pairing
   and session lifecycle, encrypted envelope codec, and relay
   websocket loop.
2. A **persistence layer** that mirrors the active session set
   to durable storage on both native (SQLite) and browser
   (IndexedDB) builds, behind a single trait. Whether and how
   sessions are *written* is governed by the `wc_session_persistence`
   node-configuration setting (§22.5); *loading* is unconditional.
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
- The workspace's central context substrate for the lazy
  per-context handle pattern.
- The workspace's database abstractions (see
  [Chapter 25](25-sql-query-builder.md) for the native SQL path
  and the IndexedDB wrapper for the browser path).
- Third-party crates carrying the WalletConnect v2 relay client,
  pairing API, and Type 0 envelope codec. These are external
  Cargo dependencies, not vendored sources; the project invokes
  them through their public Rust APIs and does not reproduce
  their internals.

The subsystem **does not depend on any coin support module**.
Coin support modules depend on the subsystem (through the
integration trait of §22.5), never the other way around. This
direction is a binding architectural rule: the WalletConnect
subsystem must remain coin-agnostic so it can be compiled and
tested without any chain backend present.

Behaviourally the subsystem must cover the following set of
responsibilities. This is a functional decomposition, not a
prescribed module layout: a conforming implementation MAY group
these differently.

- a consumer-facing surface comprising the per-context handle and
  the pairing-URI type;
- a chain taxonomy expressing the CAIP-2 chain families and the
  supported request-method set;
- error reporting that maps failures onto WalletConnect error
  codes;
- demultiplexing of inbound traffic into request and response
  paths keyed by message id;
- a relay-connection event loop driving the websocket transport;
- the pairing lifecycle (propose, accept, expire);
- app-identity metadata and the authentication-token constants;
- session state together with its key material and lifetime
  management;
- handling for each WC2 session method (propose, settle, update,
  delete, event, extend, ping);
- persistence with both a native and a browser backend behind a
  common abstraction.

The subsystem is bounded in size (on the order of a couple of
thousand lines of Rust); it contains its own serialization
tests, no relay-loop or crypto tests.

## 22.2 Public Handle and Connection API

The subsystem exposes a single public handle type, accessed
through the per-context lazy-init pattern of the
codebase's central-context substrate. Call sites obtain
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
(`wc:<topic>@2?...`) and is opaque to the project: it is
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

### 22.5.1 The `wc_session_persistence` setting

A single string setting in the node configuration
(`MM2.json`), `wc_session_persistence`, governs whether and how
sessions are **written** to durable storage. It controls
*saving only* and never affects loading. Its values are:

| Value       | Meaning                                                          |
|-------------|-----------------------------------------------------------------|
| `open`      | *(default)* Write the session record in the GLEEC-compatible    |
|             | plaintext on-disk format (§22.5.3), including the session        |
|             | symmetric key. The key is therefore stored **unencrypted at     |
|             | rest**.                                                          |
| `none`      | Never write sessions to storage. Existing rows are still read    |
|             | and used at startup, but are never rewritten or updated.         |
| `encrypted` | Reserved for a future encrypted-at-rest format. Not yet          |
|             | implemented; selecting it now must stop startup with an          |
|             | explanatory error.                                               |

The `open` default is a **documented Security-versus-compatibility
departure** (see `CODING_STANDARDS.md` §5.1 and the
"Security-versus-compatibility departures" subsection of
`COMPAT_SWITCHES.md`, and the `wc_session_persistence` row of
`GLEEC_COMPATIBILITY.md`). The bounded exposure it creates is
explicit: theft of a persisted session symmetric key permits
WalletConnect session hijack — soliciting a signing prompt from
the paired wallet — but **not** direct theft of funds. The
fund-controlling wallet seed/mnemonic is encrypted independently
(Argon2id / SLIP-0021) and is unaffected by this setting. This
exception is the minimum necessary to keep the on-disk format
byte-interchangeable with GLEEC KDF in both directions (GLEEC can
read records written here and this project can read GLEEC's
records).

> *Future-work note (to be carried as a code comment when the
> encrypted format ships): once `encrypted` is available and a
> stronger-security value can be offered, selecting the
> less-secure `open` value would at that point warrant a
> prominent runtime warning and possibly acknowledgement-gating.
> That warning is **not** added now — nothing better is yet
> offered — it is recorded only as deferred work (§22.9 D9).*

### 22.5.2 Loading is unconditional and format-autodetecting

On relay connect the loader reads **every** persisted session
record regardless of the setting value, auto-detecting the
on-disk format. Today only the `open` (plaintext) format exists;
the `encrypted` format is future. The future format is
distinguished from `open` on read by a distinct on-disk
discriminator (a renamed field/column or an explicit
format/version marker) so the loader can select the right
decoder; the exact discriminator shape is deferred (§22.9 D8).

Because loading is independent of the setting, the setting
enables an in-place migration path: run with `open` (plaintext on
disk), stop, set `encrypted`, restart — the existing plaintext
rows are still read correctly and are subsequently re-written in
the encrypted form. The setting decides only how (and whether)
rows are written, never whether they are read.

Lifecycle on relay connect:

1. Load every persisted session record, auto-detecting its
   on-disk format (load is unconditional; the setting is not
   consulted here).
2. Reconstruct each session's in-memory state fully from the
   record, including the session symmetric key, so the restored
   session can **decrypt** subsequent messages (§22.6).
3. Expire any record whose `expiry` (Unix epoch seconds) is in
   the past.
4. Re-subscribe to the session and pairing topics so that
   messages delivered while the relay was disconnected are
   processed.
5. When session state changes, write the record back — subject
   to `wc_session_persistence` (under `none`, the write is
   skipped).
6. On user-initiated disconnect, delete the record and
   unsubscribe.

### 22.5.3 On-disk record format (Interop / wire-format reuse, R29)

The `open` on-disk session record exists to be byte-interchangeable
with GLEEC KDF. It is therefore an **Interop / wire-format reuse**
fragment under clean-room rule R29: expression and function are
merged, because differing bytes break interoperability. Its only
authoritative source is the relicensed historical record (the
corpus forbidden under R8), so R31 applies: the externally-required
names below are embedded here, sanitized per R31 (authored prose,
only the names strictly necessary for interop, externally-identifying
JSON keys preserved exactly).

The native schema is a single table keyed by topic:

```sql
CREATE TABLE wc_session (
    topic   CHAR(32) PRIMARY KEY,
    data    TEXT     NOT NULL,
    expiry  BIGINT   NOT NULL
);
```

The browser schema is the same shape: a single object store
indexed by `topic`, holding the same serialized record.

- `topic` — session topic; primary key / index key.
- `expiry` — session expiry as **Unix epoch seconds**.
- `data` — the JSON-serialized session record.

The `data` payload carries the following JSON keys, which are
part of the externally-required on-disk identity and MUST be
emitted and consumed exactly as named for byte-interop:

| JSON key             | Carries                                            |
|----------------------|----------------------------------------------------|
| `topic`              | Session topic                                      |
| `subscription_id`    | Relay subscription id                              |
| `session_key`        | Session key material (see below)                   |
| `controller`         | Controlling party (wallet) descriptor              |
| `proposer`           | Proposing party (this dApp) descriptor             |
| `relay`              | Relay descriptor                                   |
| `namespaces`         | Agreed namespaces                                  |
| `propose_namespaces` | Proposed namespaces                                |
| `expiry`             | Expiry, Unix epoch seconds (mirrors the column)    |
| `pairing_topic`      | Pairing topic                                      |
| `session_type`       | Controller / Proposer role                         |
| `session_properties` | Optional wallet-reported session properties        |
| `active_chain_id`    | Optional active CAIP-2 chain id                    |
| `encoding_algo`      | Negotiated transport encoding (hex / base64)       |

The session symmetric key is carried inside `session_key`, an
object with two keys:

| JSON key      | Carries                                                       |
|---------------|---------------------------------------------------------------|
| `sym_key`     | The 32-byte ChaCha20-Poly1305 session symmetric key. In the   |
|               | `open` format this is stored in plaintext, and it MUST        |
|               | round-trip so a restored session can decrypt (§22.6).         |
| `public_key`  | The local x25519 public key used in key derivation.           |

The values of `controller`, `proposer`, `relay`, and the
namespace entries are shaped by the external WalletConnect relay
SDK types (§22.1, Third-party-API-bound shape, R33) and are
serialized as those types dictate.

The binding rule is that the storage layer must remain a single
trait with one record per session; both backends store the same
serialized payload, and the `open`-format names above are fixed by
the GLEEC interop requirement. Schemas may be extended additively
but must remain compatible between backends and must not break
byte-interop with GLEEC in the `open` format.

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
   referenced by Cargo; the project does not reimplement the
   envelope byte layout.

The symmetric key is redacted in any `Debug` output: the
formatter substitutes a fixed placeholder for the key bytes
rather than rendering them. The session-key type **shall
additionally implement zeroize-on-drop**; in the chapter-bound substrate the
implementation masks but does not zeroize, and this is named
explicitly in §22.9 as required follow-on work.

Persistence does not relax these in-memory protections. The
session symmetric key is part of the serialized/deserialized
on-disk record (§22.5.3): it is written into the `open`-format
record in plaintext at rest — the documented
Security-versus-compatibility departure — and on load it is read
back so the in-memory session key is **fully reconstructed**. A
restored session must be fully usable: it must be able to
**decrypt** inbound messages after a process restart, not merely
re-subscribe. The in-memory key value continues to be masked in
`Debug` and remains subject to the zeroize-on-drop obligation of
§22.9 regardless of how it is stored at rest.

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
time-to-live (the chapter-bound five-minute default) caps the
wait if no response arrives. The TTL is currently a constant;
a per-call override is named as follow-on work in §22.9.

## 22.8 Chain Taxonomy and Request Methods

The subsystem models the multi-chain surface of WC2 through
CAIP-2 chain identifiers (`<family>:<reference>`). Three chain
families are recognised by the chapter-bound substrate:

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
total. The chapter-bound set is:

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

R7. **Save governed by `wc_session_persistence`; save-only
    semantics.** Whether and how a session record is *written*
    to storage is governed solely by the `wc_session_persistence`
    setting: `open` writes the GLEEC-compatible plaintext record
    (§22.5.3), `none` never writes, `encrypted` is reserved and
    must stop startup with an explanatory error until implemented.
    The setting MUST NOT affect loading.

R8. **Unconditional, format-autodetecting load.** On relay
    connect the loader MUST read every persisted session record
    irrespective of the setting, auto-detecting the on-disk
    format (today only the `open` plaintext format). This is what
    enables the `open` → `encrypted` migration path of §22.5.2.

R9. **Restored sessions must be decryptable.** The session
    symmetric key MUST round-trip through the persisted record so
    that, on load, the in-memory session key is fully
    reconstructed and the restored session can decrypt subsequent
    messages — not merely re-subscribe. The in-memory key remains
    masked in `Debug` (and subject to R5) regardless of its
    at-rest representation.

R10. **`open`-format byte-interop.** The `open` on-disk record is
    an Interop / wire-format reuse fragment (R29): its
    externally-identifying names (§22.5.3) MUST stay
    byte-interchangeable with GLEEC KDF in both directions. Schema
    changes must be additive and must not break that interop.

The following items are **deferred work** required to reach a
"first integration shipped" milestone:

D1. At least one coin support module shall implement the
    integration trait. The EVM and Cosmos families are the
    natural first integrators given the trait surface and the
    request-method enum.

D2. Public RPC handlers shall be registered (see R6).

D3. The session-key type shall acquire `Zeroize` /
    `ZeroizeOnDrop` (see R5).

D4. Wallet-type detection is currently heuristic: the subsystem
    infers whether the peer is a Ledger hardware wallet or the
    Keplr wallet from advisory fields the peer reports in its
    session/pairing metadata. These heuristics are documented but
    not normative; a more robust capability-based detection is
    desirable.

D5. CAIP-10 account-address parsing currently uses a simple
    delimiter split. Stricter validation would reject malformed
    wallet responses earlier.

D6. No defence-in-depth message deduplication is implemented;
    the project relies on the relay to deduplicate. A local
    dedup layer is desirable.

D7. The response time-to-live is a constant. A per-call override
    is desirable.

D8. The `encrypted` value of `wc_session_persistence` is reserved
    but not yet implemented: an encrypted-at-rest on-disk record
    format, plus the distinct on-disk discriminator (a renamed
    field/column or an explicit format/version marker) that lets
    the unconditional loader auto-detect format and select the
    right decoder. The discriminator's exact shape is deferred
    (TODO); it must not be over-specified before the format is
    designed. Until then, selecting `encrypted` must stop startup
    with an explanatory error.

D9. When the encrypted format of D8 ships and a stronger-security
    value can be offered, selecting the less-secure `open` value
    should at that point emit a prominent runtime warning and may
    be acknowledgement-gated. This warning is intentionally **not**
    added now (nothing better is yet offered); it is recorded here
    as future work only.

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

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  subsystem at baseline verified via
  `git ls-tree -r c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the integration-trait name
  against the baseline; chapter 31 (the central application-
  context substrate the `wallet_connect` sub-context slot is
  registered on per chapter 31 R7 / R8); the public WalletConnect v2
  specification; the CAIP-2 and CAIP-10 namespaces; the
  Ethereum JSON-RPC method definitions; the Cosmos SDK signing
  schemes (Amino and SignDirect); BIP-174 (PSBT); RFC 5869
  (HKDF), RFC 7539 (ChaCha20-Poly1305), RFC 7748 (x25519); the
  relicensed historical record, consulted under R31 solely as the
  Interop / wire-format source for the §22.5.3 `open` on-disk
  session-record names (see *Forbidden corpus* below).
- *Permitted-input classes used:* baseline source; external public
  specifications (WalletConnect v2; CAIP-2; CAIP-10; Ethereum
  JSON-RPC; Cosmos SDK signing schemes; BIP-174; RFC 5869, RFC 7539,
  RFC 7748); cross-chapter contracts (Chapter 31); Interop /
  wire-format reuse (R29) for the `open` on-disk session-record
  names embedded in §22.5.3, with the relicensed historical record
  cited as the R31 source (see *Forbidden corpus* below).
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* not consulted for clean-room derivation. The
  single exception is the §22.5.3 `open` on-disk session-record
  format, an Interop / wire-format reuse fragment (R29) whose only
  authoritative source is the relicensed historical record: the
  corpus was consulted under R31 **solely** to transcribe the
  externally-required on-disk and JSON field names needed for
  byte-interop with GLEEC KDF (`wc_session`/`topic`/`data`/`expiry`
  and the `data`-payload keys, including `session_key`/`sym_key`).
  No discretionary expression — no bodies, private identifiers,
  comments, or diagnostics — was taken from the corpus; the
  surrounding prose is authored fresh per the R31 sanitization
  discipline.
