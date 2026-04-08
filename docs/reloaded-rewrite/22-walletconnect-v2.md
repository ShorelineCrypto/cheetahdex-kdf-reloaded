# Chapter 22 — WalletConnect v2

> **Chapter type:** document existing. No IMPL marker.

## 22.0 Executive summary

The reloaded WalletConnect v2 implementation is a standalone
library crate at
[`mm2src/kdf_walletconnect/`](../../mm2src/kdf_walletconnect/).
The crate provides the WC2 protocol surface (relay client,
pairing, session management, encrypted JSON-RPC envelopes,
persistent session storage) and a `WalletConnectOps` trait that
coin crates are expected to implement to gain WC-backed signing.

The integration boundary is the trait itself:
**`kdf_walletconnect` does not depend on any coin crate**, and
no coin crate in reloaded today implements `WalletConnectOps`.
The crate is also not yet wired into the JSON-RPC dispatcher in
`mm2_main`. The full WC2 protocol is in place; the integration
layer (per-coin signing impls and RPC handlers) is the explicit
follow-on work.

Key characteristics:

- **Relay-client / dApp role.** Reloaded initiates pairings,
  proposes sessions, and sends signing requests to external
  wallets. It does not act as a wallet (no inbound session
  settlement or signing).
- **Cross-platform persistence.** Sessions persist to SQLite on
  native and to IndexedDB on WASM, behind a single
  `SessionStorage` trait.
- **Modern crypto.** x25519 ECDH + HKDF-SHA256 +
  ChaCha20-Poly1305 (the canonical WC2 transport).
- **Encoding negotiation.** Default hex envelopes; base64 path
  for wallets (e.g. Keplr) that require it.

## 22.1 Crate layout

```
mm2src/kdf_walletconnect/
|-- Cargo.toml
`-- src/
    |-- lib.rs                Public API, WalletConnectCtx,
    |                         WalletConnectOps trait
    |-- chain.rs              WcChain, WcChainId, WcRequestMethods
    |-- error.rs              WalletConnectError + WC error codes
    |-- inbound_message.rs    Request / response routing
    |-- connection_handler.rs Relay websocket event handler
    |-- pairing.rs            Pairing lifecycle
    |-- metadata.rs           App name, auth token constants
    |-- session/
    |   |-- mod.rs            Session, SessionManager, tests
    |   |-- key.rs            SymKeyPair, SessionKey, x25519 + HKDF
    |   `-- rpc/
    |       |-- mod.rs
    |       |-- propose.rs    SessionPropose
    |       |-- settle.rs     SessionSettle
    |       |-- update.rs     Namespace updates
    |       |-- delete.rs     Session deletion + cleanup
    |       |-- event.rs      chainChanged, accountsChanged
    |       |-- extend.rs     Session extension
    |       `-- ping.rs       Ping / heartbeat
    `-- storage/
        |-- mod.rs            SessionStorage trait, platform dispatch
        |-- sqlite.rs         Native (db_common AsyncConnection)
        `-- indexed_db.rs     WASM (mm2_db indexed_db)
```

Roughly two thousand lines of Rust, all post-baseline.

## 22.2 Public API

[`lib.rs`](../../mm2src/kdf_walletconnect/src/lib.rs) exports:

```rust
pub mod chain;             // WcChain, WcChainId, WcRequestMethods
pub mod error;
pub mod inbound_message;
pub mod session;           // Session, SessionManager, KeyInfo, ...
pub use relay_rpc::domain::Topic as WcTopic;

/// Thread-safe handle to the per-MmArc WC subsystem.
pub struct WalletConnectCtx(pub Arc<WalletConnectCtxImpl>);

pub struct NewConnection {
    pub url: String,            // wc:<...>@2?... pairing URI
    pub pairing_topic: Topic,
}
```

`WalletConnectCtx` is the single entry point. The
`from_ctx(&MmArc)` constructor follows the same lazy-init
pattern as the rest of the codebase
([Chapter 8](08-mm-ctx-and-state-layering.md)); call sites do
not own or build the instance directly.

Headline methods on `WalletConnectCtx`:

| Method                                    | Purpose                                |
|-------------------------------------------|----------------------------------------|
| `new_connection(required, optional)`      | Generate a pairing URI / topic         |
| `send_session_request_and_wait<R>(...)`   | Encrypted JSON-RPC -> wallet, await `R`|
| `drop_session(topic)`                     | Gracefully terminate a pairing         |
| `is_ledger_connection(topic)`             | Wallet-type detection (Cosmos / Ledger)|
| `is_keplr_connection(topic)`              | Encoding-quirk detection               |
| `get_account_and_properties_for_chain_id` | Resolve active account + metadata      |
| `encode(session_topic, data)`             | Encode response (hex or base64)        |

## 22.3 The `WalletConnectOps` trait

Coin crates that want to surface WC-backed signing implement:

```rust
pub trait WalletConnectOps {
    type Error;
    type Params<'a>;
    type SignTxData;
    type SendTxData;

    async fn wc_chain_id(&self, ctx: &WalletConnectCtx)
        -> Result<WcChainId, Self::Error>;

    async fn wc_sign_tx<'a>(
        &self, wc: &WalletConnectCtx, params: Self::Params<'a>,
    ) -> Result<Self::SignTxData, Self::Error>;

    async fn wc_send_tx<'a>(
        &self, wc: &WalletConnectCtx, params: Self::Params<'a>,
    ) -> Result<Self::SendTxData, Self::Error>;

    fn session_topic(&self) -> Result<&Topic, Self::Error>;
}
```

The trait is intentionally minimal: a chain-id resolver, a sign
flow, a send flow, and a way to identify the pairing the coin
should use. Each implementor chooses its own associated types
for params and return shapes; the WC crate does not need to
know about EthCoin or TendermintCoin.

Today **no coin in reloaded implements `WalletConnectOps`** -- a
grep of `mm2src/coins/` for `WalletConnectOps` or
`wallet_connect` returns no matches. The trait is in place, and
the integration is left as the next step (see §22.10).

## 22.4 Protocol role

Reloaded acts strictly as the **relay client / dApp** side of
WC2:

| Role                              | Reloaded | Wallet (external) |
|-----------------------------------|----------|--------------------|
| Initiates pairing                 | yes      | no                 |
| Sends `wc_sessionPropose`         | yes      | no                 |
| Receives `wc_sessionSettle`       | yes      | yes (sends it)     |
| Sends `wc_sessionRequest` (sign)  | yes      | no                 |
| Signs and responds                | no       | yes                |
| Broadcasts the signed transaction | depends on `wc_sendTransaction` path | usually yes |

There is no wallet-side endpoint (reloaded does not settle
inbound sessions or sign on behalf of external dApps).

## 22.5 Session storage

The `SessionStorage` trait
([`storage/mod.rs`](../../mm2src/kdf_walletconnect/src/storage/mod.rs))
abstracts persistence:

```rust
#[cfg(target_arch = "wasm32")]
type DB = indexed_db::IDBSessionStorage;
#[cfg(not(target_arch = "wasm32"))]
type DB = sqlite::SqliteSessionStorage;
```

Native ([`storage/sqlite.rs`](../../mm2src/kdf_walletconnect/src/storage/sqlite.rs))
uses the shared `db_common::AsyncConnection` from
[Chapter 25](25-sql-query-builder.md) and stores rows in a
single table:

```sql
CREATE TABLE wc_session (
    topic   CHAR(32) PRIMARY KEY,
    data    TEXT     NOT NULL,
    expiry  BIGINT   NOT NULL
);
```

WASM
([`storage/indexed_db.rs`](../../mm2src/kdf_walletconnect/src/storage/indexed_db.rs))
mirrors this with a single `sessions` object store inside the
`wc_session_storage` database, built on
[`mm2_db::indexed_db::ConstructibleDb`](../../mm2src/mm2_db/src/indexed_db/).

Lifecycle on relay connect:

1. Load every persisted session.
2. Expire any row where `now > expiry`.
3. Re-subscribe to the session and pairing topics so messages
   that landed while the relay was disconnected are processed.
4. Update rows in place via `storage.update_session(...)`.
5. On user-initiated disconnect or `drop_session`, delete the
   row.

## 22.6 Cryptography

[`session/key.rs`](../../mm2src/kdf_walletconnect/src/session/key.rs)
implements the WC2 key-exchange and symmetric-key derivation:

```rust
use x25519_dalek::{StaticSecret, PublicKey};
use hkdf::Hkdf;
use sha2::Sha256;

let static_secret = StaticSecret::random_from_rng(OsRng);
let shared = static_secret.diffie_hellman(&peer_public_key);   // x25519

let hk = Hkdf::<Sha256>::new(None, shared.as_bytes());
let mut sym_key = [0u8; 32];
hk.expand(&[], &mut sym_key)?;                                  // HKDF-SHA256
```

The empty HKDF salt (`None`) and empty `info` (`&[]`) are not
arbitrary choices: they are mandated by the WalletConnect v2
specification and must be matched byte-for-byte by any
interoperable implementation.

Symmetric encryption uses ChaCha20-Poly1305 via the
`wc_common::{encrypt_and_encode, decode_and_decrypt_type0}`
helpers (`EnvelopeType::Type0`). `wc_common` is vendored from
the upstream WalletConnect Rust SDK
(`github.com/komodoplatform/walletconnectrust`, tag `k-0.1.3`)
and carries the canonical envelope codec, so any wire-level
details (version byte, salt size, tag size, byte order) are
delegated to that crate rather than reimplemented here.

`SessionKey` masks the raw symmetric key in its `Debug` impl by
substituting `"*******"`. Note: explicit zeroize-on-drop is not
applied today; this is called out in §22.10.

## 22.7 Encrypted JSON-RPC envelopes

Each WC message is a Type 0 envelope -- a version byte, salt,
ChaCha20-Poly1305 ciphertext, and Poly1305 auth tag -- wrapping
a JSON-RPC payload. The exact serialization is defined by the
WalletConnect v2 specification and implemented in the vendored
`wc_common` crate; reloaded calls
`wc_common::encrypt_and_encode(EnvelopeType::Type0, payload,
&sym_key)` on send and
`wc_common::decode_and_decrypt_type0(msg.as_bytes(), &key)` on
receive (see
[`lib.rs`](../../mm2src/kdf_walletconnect/src/lib.rs#L48)).

The transport encoding (hex by default, base64 for Keplr) is
applied to the entire envelope after encryption, not to the
ciphertext alone.

The JSON-RPC payload itself looks like:

```jsonc
{
  "jsonrpc": "2.0",
  "id": <MessageId>,
  "method": "wc_sessionRequest",
  "params": {
    "chainId": "eip155:1",
    "request": {
      "method": "eth_signTransaction",
      "params": [ /* ... */ ]
    }
  }
}
```

Outbound encoding defaults to hex; the path switches to base64
when `is_keplr_connection(topic)` is true (Keplr requires
base64-encoded envelopes).

Request / response correlation is handled by
[`inbound_message.rs`](../../mm2src/kdf_walletconnect/src/inbound_message.rs):
`send_session_request_and_wait` registers a oneshot under the
message id, the inbound handler matches incoming responses by
id and wakes the waiter, and a TTL (default five minutes) caps
the wait.

## 22.8 Chain abstraction and request methods

[`chain.rs`](../../mm2src/kdf_walletconnect/src/chain.rs)
defines the chain family taxonomy and the canonical RPC method
names. The `WcChain` enum currently models three CAIP-2
families:

- `Eip155` -- EVM chains (`eip155:<id>`).
- `Cosmos` -- Cosmos SDK chains (`cosmos:<chain-id>`).
- `Bip122` -- UTXO chains (`bip122:<genesis-hash-prefix>`).

`WcChainId` represents a `<family>:<reference>` pair as a
strongly-typed value.

`WcRequestMethods` enumerates the wallet-side method names the
crate knows how to issue:

| Variant                    | Wire method name             | Family  |
|----------------------------|------------------------------|---------|
| `EthSignTransaction`       | `eth_signTransaction`        | eip155  |
| `EthSendTransaction`       | `eth_sendTransaction`        | eip155  |
| `EthPersonalSign`          | `personal_sign`              | eip155  |
| `CosmosSignDirect`         | `cosmos_signDirect`          | cosmos  |
| `CosmosSignAmino`          | `cosmos_signAmino`           | cosmos  |
| `CosmosGetAccounts`        | `cosmos_getAccounts`         | cosmos  |
| `UtxoGetAccountAddresses`  | `getAccountAddresses`        | bip122  |
| `UtxoSendTransfer`         | `sendTransfer`               | bip122  |
| `UtxoSignPsbt`             | `signPsbt`                   | bip122  |
| `UtxoPersonalSign`         | `personal_sign` (UTXO route) | bip122  |

`cosmos_signAmino` is the Ledger-compatible path (Ledger Cosmos
apps only support Amino-JSON sign payloads); `cosmos_signDirect`
is the default for software wallets.

Note: `eth_signTypedData_v4` is *not* in the enum today, even
though it is a common WC2 method on the EVM side. Adding it is
straightforward (a new variant + match arm in the wire-name
mapping) once a coin implementation needs it.

The trait surface and request taxonomy is the contract a coin
implementer has to satisfy; the crate has no awareness of
specific tickers or contract addresses.

## 22.9 Tests

[`session/mod.rs`](../../mm2src/kdf_walletconnect/src/session/mod.rs)
contains a small set of serialization tests:

- `test_deserialize_keys_from_string`
- `test_deserialize_keys_from_vec`
- `test_deserialize_empty_keys`
- `test_deserialize_no_keys`
- `test_serialize_deserialize_roundtrip`

There are no tests covering the relay loop, the crypto
primitives, storage round-trips, or the message-id correlation
logic; these are typically integration-test material requiring
either a mocked relay server or a live test relay endpoint.

## 22.10 Known limitations and deferred work

1. **No coin implementations of `WalletConnectOps`.** The trait
   is defined; no coin crate in `mm2src/coins/` implements it
   today. EVM signing (via `eth_signTransaction` /
   `eth_sendTransaction` / `personalSign`) and Cosmos signing
   (via `cosmos_signDirect` / `cosmos_signAmino`) are the
   expected first integrators.
2. **No JSON-RPC dispatcher wiring.** `kdf_walletconnect` is not
   a dependency of `mm2_main`. RPC methods such as
   `wc::new_pairing`, `wc::get_sessions`, `wc::drop_session`
   exist in concept (mapped onto the public crate API) but are
   not registered in
   [`mm2_main/src/rpc/dispatcher/dispatcher.rs`](../../mm2src/mm2_main/src/rpc/dispatcher/dispatcher.rs).
3. **Symmetric key zeroize.** `SessionKey` masks the key in
   `Debug` but does not implement `Zeroize` / `ZeroizeOnDrop`;
   adding it requires a `zeroize` feature on the holder type.
4. **Wallet-detection heuristics.** Ledger detection reads
   `sessionProperties.keys[0].is_nano_ledger`; Keplr detection
   matches `controller.metadata.name == "Keplr"`. Both work but
   are heuristic.
5. **CAIP-10 account parsing.** Today a simple split on `:`;
   stricter validation would help reject malformed wallet
   responses earlier.
6. **No message deduplication.** The crate relies on the relay
   server to deduplicate; a defence-in-depth dedup layer is not
   present.
7. **Hard-coded response TTL** of five minutes for any pending
   session request; tunable per call would be a small follow-on.

## 22.11 Provenance

`mm2src/kdf_walletconnect/` is post-baseline; `git ls-tree
c1d46c0 -- mm2src/kdf_walletconnect` returns empty. The
implementation conforms to the WalletConnect v2 specification
(public protocol) and uses the third-party `relay_rpc` and
`wc_common` crates for the relay client and envelope codec
respectively; both are listed under the legal classification
register
([`local/legal/AUDIT_FILE_CLASSIFICATION.md`](../../local/legal/AUDIT_FILE_CLASSIFICATION.md)).
