# Chapter 28 -- libp2p Stack Consolidation

> **Chapter type:** document existing. No IMPL marker.

## 28.0 Executive summary

The baseline shipped four cooperating crates for its
peer-to-peer mesh:
[`mm2_libp2p/`](../../mm2src/mm2_libp2p/),
[`gossipsub/`](../../mm2src/gossipsub/) (a vendored,
relay-mesh-aware fork of the upstream libp2p gossipsub),
[`floodsub/`](../../mm2src/floodsub/) (a vendored copy of
the upstream libp2p floodsub), and a separate
[`peers/`](../../mm2src/peers/) crate that wrapped peer
discovery primitives. Together they composed the
`AtomicDexBehaviour` network behaviour the application
crate (`mm2_main`) drove.

The reloaded tree consolidates this stack into a single
crate, [`mm2src/mm2_p2p/`](../../mm2src/mm2_p2p/), with
the two vendored protocol implementations moved to
sub-modules under it (`mm2_p2p::gossipsub`,
`mm2_p2p::floodsub`) and the peer-discovery primitives
absorbed as `mm2_p2p::peers_exchange`. The legacy
`mm2_libp2p/`, `gossipsub/`, and `floodsub/` directories
have been deleted from the workspace; only
[`mm2src/peers/`](../../mm2src/peers/) remains on disk,
kept so that `git log -- mm2src/peers/` queries against
the baseline-era code continue to work. Every post-baseline
consumer (`mm2_main`, `mm2_event_stream`, `mm2_net`,
[Chapter 22](22-walletconnect-v2.md), the V2 swap
chapters) targets `mm2_p2p`.

The underlying libp2p version itself was *not* bumped: both
baseline and reloaded pin to the same upstream
`github.com/libp2p/rust-libp2p` Git revision (the `floodsub`
+ `mplex` + `noise` + `ping` + `request-response` +
`secp256k1` feature set on both sides; the native side adds
`dns-tokio` + `tcp-tokio` + `websocket`; the WASM side adds
`wasm-ext` + `wasm-ext-websocket`). The work in this chapter
is the *crate-layout* modernization that lets the rest of
the workspace see one P2P crate instead of four, plus the
small post-baseline additions that hang off it:

- the [`proxy_signature`](../../mm2src/proxy_signature/)
  crate ([Chapter 27 §27.10](27-infrastructure-crate-carve-outs.md#2710-libp2p-proxy-signing----proxy_signature))
  for authenticating HTTP requests that the P2P mesh
  forwards on behalf of a client;
- the [`mm2_net_config`](../../mm2src/mm2_net_config/) crate
  ([Chapter 6](06-network-id-seed-node.md)) for declaring
  the bootstrap list and the network identifier the mesh
  uses to scope its messages;
- a small number of helpers (`relay_address`,
  `peers_exchange`, `adex_ping`, `request_response`,
  `runtime`, `ip_helpers`) that used to live in
  `mm2_libp2p` and that are now part of the same crate as
  the behaviour they support.

This chapter documents the consolidated layout, the
composed behaviour, the topic and message-signing surface
the application uses on top of it, the bootstrap-and-mesh
maintenance loop, and the post-baseline additions
(proxy signing, mesh-aware gossipsub relay extension,
network-id scoping).

## 28.1 Crate consolidation

### 28.1.1 What moved into `mm2_p2p`

The post-baseline
[`mm2src/mm2_p2p/src/`](../../mm2src/mm2_p2p/src/) tree is
flat at the top level and contains both the workspace's
"glue" modules and the two vendored protocol
sub-implementations:

```
mm2_p2p/src/
|-- lib.rs                  re-exports + message-signing API
|-- atomicdex_behaviour.rs  composed NetworkBehaviour + swarm builder
|-- atomicdex_behaviour/    test helpers
|-- gossipsub/              vendored relay-mesh-aware gossipsub
|   |-- behaviour.rs
|   |-- config.rs
|   |-- handler.rs
|   |-- mcache.rs           message cache
|   |-- protocol.rs
|   `-- topic.rs
|-- floodsub/               vendored floodsub
|   |-- layer.rs
|   |-- protocol.rs
|   `-- topic.rs
|-- peers_exchange.rs       request-response peer exchange
|-- relay_address.rs        RelayAddress parsing
|-- adex_ping.rs            ping wrapper that disconnects on failure
|-- request_response.rs     request-response behaviour wrapper
|-- runtime.rs              swarm runtime helpers
`-- ip_helpers.rs           global IP detection
```

The cargo manifest declares the upstream libp2p Git pin on
both targets, with the per-target feature lists noted in
[§28.0](#280-executive-summary).

### 28.1.2 Legacy crates -- migration status

The post-baseline consolidation removed three of the four
original baseline crates from the active workspace. The
`mm2src/mm2_libp2p/`, `mm2src/gossipsub/`, and
`mm2src/floodsub/` directories have been deleted; they are
no longer present on disk and no longer appear in the
workspace `[workspace.members]` list. Their content was
folded into [`mm2src/mm2_p2p/`](../../mm2src/mm2_p2p/) as
described in [§28.1.1](#2811-what-moved-into-mm2_p2p).

The fourth crate, [`mm2src/peers/`](../../mm2src/peers/),
is still on disk and is still a workspace member. It is
kept so that git-blame and `git log -- <path>` queries
against the baseline-era code continue to work, and so
that the provenance footers in this chapter and elsewhere
can cite specific baseline files. No post-baseline
consumer depends on it; the dependency graph from
`mm2_main` and all the other post-baseline crates that
need P2P routes entirely through `mm2_p2p`.

### 28.1.3 Public surface

[`mm2_p2p/src/lib.rs`](../../mm2src/mm2_p2p/src/lib.rs)
re-exports a small public surface:

- the swarm-spawning entry point (`spawn_gossipsub`) and
  its associated error type and configuration enums
  (`NodeType` distinguishing relay vs client, `WssCerts`
  for the optional TLS bundle);
- the gossipsub event, message, and message-id types the
  consumer matches on;
- a small libp2p-identity re-export (`PeerId`, `Multiaddr`,
  the secp256k1 public-key wrappers);
- the `PeerAddresses` type returned by the peer-exchange
  protocol;
- the `RelayAddress` parser used to decode bootstrap
  entries from configuration;
- the `encode_and_sign` / `decode_signed` pair that wraps
  application payloads in a secp256k1-signed envelope; and
- the `pub_sub_topic` helper plus the `TOPIC_SEPARATOR`
  constant.

Everything else is private to the crate.

## 28.2 The composed behaviour

The application network behaviour, declared in
[`atomicdex_behaviour.rs`](../../mm2src/mm2_p2p/src/atomicdex_behaviour.rs),
is a `#[derive(NetworkBehaviour)]` struct composed of five
sub-behaviours:

| Sub-behaviour          | Role                                          |
|------------------------|-----------------------------------------------|
| `Gossipsub` (vendored) | Mesh-based pub/sub for orderbook and swap     |
| `Floodsub`             | Flood-based pub/sub for the peers topic       |
| `RequestResponseBehaviour` | Direct one-shot RPCs over the mesh        |
| `PeersExchange`        | Request-response protocol `/peers-exchange/1` |
| `AdexPing`             | Ping with forced disconnect on failure        |

The list of upstream behaviours that are *not* composed is
itself meaningful: there is no Kademlia DHT, no libp2p
mDNS, no libp2p Identify, no libp2p Relay protocol, no
DCUtR direct-connection-upgrade, and no AutoNAT. Peer
discovery happens entirely through the bootstrap list plus
the peers-exchange protocol, and NAT traversal is solved
by always having a small set of globally reachable relay
nodes in the bootstrap list -- not by hole-punching at the
libp2p layer.

This is a deliberate choice: the workspace is shipping a
production trading mesh on a curated set of known relay
nodes, not a generic libp2p application, and the smaller
behaviour surface is both simpler to reason about and
smaller to ship to a browser.

## 28.3 Relay-mesh-aware gossipsub

The most substantial post-baseline-survivor in
`mm2_p2p::gossipsub` is the relay-mesh extension that
upstream libp2p-gossipsub does not have. The vendored
behaviour
([`gossipsub/behaviour.rs`](../../mm2src/mm2_p2p/src/gossipsub/behaviour.rs))
adds, on top of the standard gossipsub state:

- a set of *connected relays* (peers that have advertised
  themselves with the protocol's `IAmRelay` control
  message);
- a *relay mesh* map (a subset of connected relays the
  node treats as its mesh peers, with per-peer counters);
- a *reverse* mesh set (the relays that have included
  this node in *their* mesh);
- an *explicit relay list* -- relays that are pinned and
  never evicted from the mesh;
- a 10-second *relay-mesh-maintenance* timer that fills
  the mesh up to the configured low water mark and prunes
  it back to the high water mark.

This is the structural reason the workspace uses a
vendored gossipsub: relay-aware mesh management is a
project-specific extension to the spec, not something that
upstream libp2p-gossipsub provides today.

The accompanying configuration builder
([`gossipsub/config.rs`](../../mm2src/mm2_p2p/src/gossipsub/config.rs))
exposes a small set of knobs the application sets at
swarm-spawn time:

- a content-addressed `message_id_fn` (the message id is a
  hash of payload + sequence number, so duplicate payloads
  collapse);
- the `i_am_relay` flag (controls whether the node
  announces itself as a relay via the `IAmRelay` control
  message);
- the three mesh size watermarks (`mesh_n_low`, `mesh_n`,
  `mesh_n_high`) which are tuned differently for clients
  and relays;
- `manual_propagation` (the consumer is responsible for
  calling `propagate_message` after it has validated a
  message, which gives the application the ability to drop
  invalid messages before forwarding);
- a maximum transmit size of just under 1 MiB.

The implementation does *not* carry the peer-scoring /
reputation system from later upstream gossipsub, and does
not carry a separate message-validation filter (the
manual-propagation hook is the equivalent).

## 28.4 Transports and the upgrade stack

The transport stack is target-dependent and the choices
follow the per-target libp2p feature flags noted in
[§28.0](#280-executive-summary):

| Target  | Transport                                            |
|---------|------------------------------------------------------|
| Native  | TCP + DNS, optionally upgraded with WebSocket and    |
|         | WSS (if the configuration supplies a `WssCerts`).    |
| WASM    | Browser WebSocket via `libp2p::wasm_ext` FFI.        |
| Testing | `MemoryTransport` for the in-process mesh used by    |
|         | the integration tests in `mm2_p2p::atomicdex_behaviour/`.|

The application-level upgrade pipeline is the same on all
targets: noise XX key agreement
(`Xx25519Spec`), then mplex multiplexing, with a 20-second
upgrade timeout.

Bootstrap-multiaddr formats the configuration accepts are
the standard libp2p forms: `/ip4/<addr>/tcp/<port>`,
`/ip4/<addr>/tcp/<port>/wss`, `/dns/<host>/tcp/<port>`,
and `/memory/<port>` for tests.

## 28.5 Topics and message signing

The pub/sub topic surface is a flat namespace whose strings
are part of the wire contract between mesh peers. Topic
names are constructed via the `pub_sub_topic(prefix, topic)`
helper in `mm2_p2p::lib.rs`, which formats them as
`<prefix><TOPIC_SEPARATOR><topic>` (the separator is `/`).
The conventional prefixes the `mm2_main` consumer uses are
documented in
[`mm2_main::lp_network`](../../mm2src/mm2_main/src/lp_network.rs)
and split into three buckets:

- the orderbook gossipsub topics
  (`<orderbook-prefix>/<base>:<rel>`);
- the swap gossipsub topics
  (`<swap-prefix>/<uuid>`);
- the floodsub peers topic (`PEERS`).

Application payloads are wrapped in a secp256k1-signed
envelope before publication. The signing surface is
intentionally small:

- `encode_and_sign<T: Serialize>(message: &T, secret: &[u8;
  32]) -> Vec<u8>` -- msgpack-encodes the payload, hashes it
  with SHA-256, signs the hash with the supplied secp256k1
  secret, and packs `{ pubkey, signature, payload }` back
  into a msgpack envelope.
- `decode_signed<'de, T: Deserialize<'de>>(encoded: &'de
  [u8]) -> Result<(T, Signature, PublicKey), ...>` --
  reverse direction; verifies the signature and returns the
  payload alongside the verified public key.

Note that this signing surface is *separate* from the
`proxy_signature` crate (Chapter
[27 §27.10](27-infrastructure-crate-carve-outs.md#2710-libp2p-proxy-signing----proxy_signature)).
`encode_and_sign`/`decode_signed` use raw `secp256k1` keys
and serve the P2P mesh's *application payload*
authenticity; `proxy_signature` uses libp2p-identity
keypairs and serves the *HTTP* relay-proxy authentication
flow described in [§28.7](#287-proxy-signing-for-http-relay).

## 28.6 Discovery, mesh maintenance, NAT

The application does not attempt to discover peers in the
open-internet sense; the bootstrap list in the configuration
is the source of truth. The workflow at swarm spawn time is:

1. Parse the bootstrap list (a vector of
   `RelayAddress`) into multiaddrs.
2. Dial up to `mesh_n` random entries.
3. Start a 10-second maintenance timer that:
   - tops the connected-relays set back up if it falls
     below `mesh_n_low` (by dialing additional bootstrap
     entries and peers-exchange responses);
   - prunes back to the high water mark if it grows past
     it; and
   - issues a peers-exchange request to a random connected
     relay periodically (300-second interval, 20-second
     initial delay) to pull in fresh peer addresses.

The peers-exchange responder caps replies at 100 addresses
per request to keep the protocol bounded.

NAT traversal is solved structurally rather than
dynamically: relay nodes are expected to be on globally
routable addresses, and client nodes that sit behind NAT
reach the mesh via at least one relay. The
`ip_helpers::is_global` predicate filters out non-routable
addresses from listener announcements so that other peers
do not try to reach a client at its RFC1918 address.

## 28.7 Proxy signing for HTTP relay

The post-baseline addition of the
[`proxy_signature`](../../mm2src/proxy_signature/) crate
([Chapter 27 §27.10](27-infrastructure-crate-carve-outs.md#2710-libp2p-proxy-signing----proxy_signature))
is what lets the trading mesh act as a relay for HTTP
requests on behalf of light clients. The pattern is:

1. The light client constructs a `RawMessage` envelope:
   - a fixed magic prefix (`b"Proxy Auth Payload\n"`),
   - the target URI,
   - the request body size (hashed in to bind the body to
     the signature),
   - the libp2p-encoded public key of the client,
   - an expiry timestamp.
2. The client signs the envelope with its libp2p-identity
   keypair (the same key that identifies the client on the
   mesh) and attaches the signature as a request header.
3. The proxy node receives the HTTP request, parses the
   envelope from the request headers, re-derives the magic
   prefix, and calls `verify` on the envelope-and-signature
   pair against the embedded public key.
4. If the signature verifies and the expiry has not
   elapsed, the proxy forwards the HTTP request to the
   real target; otherwise it returns Unauthorized.

The point of this scheme is that the client does not
need to share any secret with the proxy operator -- the
client's libp2p-identity public key, which is already
broadcast on the mesh, is the authentication material.

## 28.8 Network identifier scoping

The post-baseline `mm2_net_config` crate
([Chapter 6](06-network-id-seed-node.md)) adds an explicit
network-identifier mechanism that scopes the mesh:
topics, peer-exchange responses, and signed payloads all
carry the network id, so that two nodes configured for
different networks do not see each other's traffic even if
they happen to have overlapping bootstrap lists. This
replaces a baseline-era practice of relying solely on the
bootstrap list to keep networks apart, which was fragile
when a node operator misconfigured an entry.

## 28.9 Limitations and known gaps

1. **libp2p upstream version is the same as at baseline.**
   The reloaded tree still pins to the project's existing
   Git revision of `rust-libp2p`. A version bump would
   touch every behaviour and most of the swarm-builder code
   and is held as a separate work item.
2. **The legacy `peers/` crate is still in the workspace.**
   It has no active consumer but appears in `cargo check`,
   `cargo clippy`, and `cargo build` output. Removing it
   is a follow-up; keeping it costs build time but
   preserves git-blame against baseline.
3. **No DHT, no mDNS, no AutoNAT.** Peer discovery is
   bootstrap-list + peers-exchange-only; nodes that lose
   contact with every bootstrap relay cannot recover
   automatically.
4. **The vendored gossipsub is not the modern peer-scoring
   gossipsub.** Reputation, score thresholds, and
   negative-score eviction are not implemented; abusive
   peers are handled by manual disconnect logic plus the
   ping-based force-disconnect in `AdexPing`.
5. **Application-level message signing uses raw
   `secp256k1`, not libp2p identity.** The two signing
   surfaces (this chapter's `encode_and_sign` /
   `decode_signed` and `proxy_signature`'s libp2p-identity
   sign / verify) exist in parallel and a future unification
   is plausible but not in scope here.
6. **NAT traversal degrades to "no connection" for clients
   whose only path to a relay is blocked.** There is no
   libp2p relay-v2 + DCUtR fallback; the deployment
   assumption is that relays are reachable from anywhere.

## 28.10 External references

- The `rust-libp2p` upstream project
  ([github.com/libp2p/rust-libp2p](https://github.com/libp2p/rust-libp2p)).
- The libp2p gossipsub specification
  ([github.com/libp2p/specs/tree/master/pubsub/gossipsub](https://github.com/libp2p/specs/tree/master/pubsub/gossipsub)).
- The libp2p floodsub specification
  ([github.com/libp2p/specs/tree/master/pubsub](https://github.com/libp2p/specs/tree/master/pubsub)).
- The libp2p noise handshake specification
  ([github.com/libp2p/specs/tree/master/noise](https://github.com/libp2p/specs/tree/master/noise)).
- The libp2p multiaddr format
  ([github.com/multiformats/multiaddr](https://github.com/multiformats/multiaddr)).
- secp256k1 (used for application-payload signing)
  ([github.com/bitcoin-core/secp256k1](https://github.com/bitcoin-core/secp256k1)).
- The msgpack serialization format used by
  `encode_and_sign` / `decode_signed`
  ([msgpack.org](https://msgpack.org/)).

## 28.11 Provenance

The four baseline crates (`mm2src/mm2_libp2p/`,
`mm2src/gossipsub/`, `mm2src/floodsub/`, `mm2src/peers/`)
were all present at `c1d46c0`; this was verified with
`git ls-tree c1d46c0 -- mm2src/`. The first three have
since been deleted; the fourth (`mm2src/peers/`) is
retained as described in
[§28.1.2](#2812-legacy-crates----migration-status). The
vendored gossipsub and floodsub sub-modules now under
[`mm2_p2p/src/gossipsub/`](../../mm2src/mm2_p2p/src/gossipsub/)
and
[`mm2_p2p/src/floodsub/`](../../mm2src/mm2_p2p/src/floodsub/)
carry forward the baseline implementations as the starting
point; their post-baseline changes include the relay-mesh
state and timer extension, the message-id hashing function,
and the manual-propagation hook.

The peers-exchange, relay-address, adex-ping, and runtime
modules are absorbed from the baseline `mm2_libp2p` crate
into `mm2_p2p`. The proxy-signature and network-config
crates ([§28.7](#287-proxy-signing-for-http-relay) and
[§28.8](#288-network-identifier-scoping)) are entirely
post-baseline and are documented in their own chapters
(Chapter [27 §27.10](27-infrastructure-crate-carve-outs.md#2710-libp2p-proxy-signing----proxy_signature)
and Chapter [6](06-network-id-seed-node.md)).
