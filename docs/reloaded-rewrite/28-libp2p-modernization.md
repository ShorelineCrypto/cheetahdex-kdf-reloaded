# Chapter 28 — P2P Substrate Consolidation

**Status:** driving-spec.

This chapter binds the consolidated peer-to-peer substrate: the single
crate boundary, the composed network behaviour and its deliberately
constrained sub-behaviour set, the vendored relay-mesh-aware gossipsub
extension, the bound topic-naming and application-payload-signing surface,
the transport stack per build target, and the discovery / bootstrap /
mesh-maintenance discipline. It also binds, as **required ports**
(§28.9A), three post-baseline application-level P2P behaviours
reloaded must gain: the peer-connection health-check RPC, the
network time-synchronisation peer-admission rule, and expirable
pubkey bans.

## 28.1 Executive Summary

The baseline tree ships four cooperating P2P substrates: a glue crate
holding the composed behaviour, a vendored relay-mesh-aware
gossipsub crate, a vendored floodsub crate, and a separate peer-discovery
crate. Each contributes a piece of the composed network behaviour that the
application layer drives.

This chapter binds a single consolidated P2P substrate that exposes the
same composed behaviour through one crate boundary. The vendored
gossipsub and floodsub implementations and the peer-exchange primitives
all become submodules of the consolidated substrate. The application
layer and every other substrate that needs P2P facilities targets the
single consolidated crate.

The substrate is *not* a libp2p version bump: both before and after, the
underlying libp2p revision is the same pinned external-dependency commit,
with the same per-target feature flags. The change is a crate-layout consolidation
plus the small set of post-substrate-introduction additions that hang off
it: the proxy-signature substrate (Chapter 27), the network-id scoping
substrate (Chapter 06), and the helpers that used to live in the glue
crate (relay-address parser, peers-exchange wrapper, ping-with-disconnect
wrapper, request-response wrapper, swarm runtime helpers, IP helpers).

## 28.2 Subsystem Shape

The consolidated substrate exposes a small public surface (R5 below) and
keeps everything else internal. The composed network behaviour is a
five-element `NetworkBehaviour` derive over the canonical libp2p protocol
families plus the substrate-internal `PeersExchange` and `AdexPing`
wrappers (R6).

The libp2p protocol families *not* present in the composed behaviour are
themselves bound (R7): no Kademlia DHT, no mDNS, no Identify, no Relay
(v1 or v2), no DCUtR, no AutoNAT. Peer discovery is bootstrap-list-plus-
peers-exchange-only; NAT traversal is solved structurally (R18) by
always having a small set of globally-routable relay nodes in the
bootstrap list.

The vendored gossipsub carries a relay-mesh extension that the standard
gossipsub specification does not have (R10–R13). This is the structural
reason the substrate uses a vendored gossipsub rather than the standard
libp2p implementation directly.

## 28.3 Bound Crate Boundary

**R1.** The substrate exposes exactly one crate boundary to all
downstream consumers. The composed behaviour, the vendored gossipsub
implementation, the vendored floodsub implementation, the peer-exchange
protocol, the ping-with-disconnect wrapper, the request-response
wrapper, the swarm runtime helpers, the relay-address parser, and the
IP-detection helpers all live inside this single crate.

**R2.** The substrate's submodule layout (one submodule per concern) is
not part of the contract; downstream consumers MUST consume the
re-exported public surface from the crate root (R5) and MUST NOT
import from submodule paths.

**R3.** Application crates that previously imported from any of the
four baseline P2P substrates MUST be updated to import from the
consolidated substrate. No application-side code path may retain a
direct import of a deleted baseline P2P crate.

**R4.** A single baseline P2P substrate (the peer-discovery substrate)
MAY be retained on disk as a workspace member with no active consumer.
Its retention is purely for repository-history continuity (`git log`
and `git blame` against pre-substrate-introduction code). No
post-substrate-introduction consumer MUST depend on it.

## 28.4 Bound Public Surface

**R5.** The consolidated substrate's crate root MUST re-export
exactly the following named surface:

- the swarm-spawning entry point, its associated error type, and its
  configuration enums — including the relay-vs-client distinction
  carried on a bound `NodeType` enum, and the optional TLS bundle type
  for WSS transports;
- the gossipsub event, message, and message-id types the consumer
  matches on;
- a small libp2p-identity re-export — `PeerId`, `Multiaddr`, and the
  secp256k1 public-key wrappers;
- the `PeerAddresses` type returned by the peer-exchange protocol;
- the `RelayAddress` parser used to decode bootstrap entries from
  configuration;
- the bound application-payload signing pair `encode_and_sign` /
  `decode_signed` defined in §28.7;
- the `pub_sub_topic` helper and the `TOPIC_SEPARATOR` constant defined
  in §28.7.

Every other symbol in the substrate MUST be private to the crate.

## 28.5 Bound Composed Behaviour

**R6.** The composed network behaviour MUST be a `NetworkBehaviour`
derive over exactly five sub-behaviours, with bound substrate role for
each:

| Sub-behaviour                  | Bound role                                                     |
| ------------------------------ | -------------------------------------------------------------- |
| `Gossipsub` (vendored)         | Mesh-based publish-subscribe for orderbook and swap traffic.   |
| `Floodsub`                     | Flood-based publish-subscribe for the bound peers topic.       |
| `RequestResponseBehaviour`     | Direct one-shot RPC over the mesh.                              |
| `PeersExchange`                | Request-response over a bound protocol identifier (R8).         |
| `AdexPing`                     | Ping wrapper that forces a disconnect on consecutive failures. |

**R7.** The composed behaviour MUST NOT include any of the following
libp2p protocol families: Kademlia DHT, mDNS, Identify, Relay (v1 or
v2), DCUtR, AutoNAT. Their absence is a substrate contract.

**R8.** The peer-exchange request-response protocol identifier is bound
as the string `/peers-exchange/1`. The peer-exchange responder MUST cap
each reply at no more than 100 addresses.

**R9.** The ping wrapper MUST disconnect a peer after a bounded number
of consecutive ping failures (the exact count is a substrate-internal
tuning parameter, not bound here), as opposed to merely logging the
failure.

## 28.6 Bound Gossipsub Relay-Mesh Extension

**R10.** The vendored gossipsub behaviour MUST carry, on top of the
standard gossipsub state, five additional pieces of state:

- a *connected-relays* set — peers that have advertised themselves
  with the substrate's `IAmRelay` control message;
- a *relay-mesh* map — the subset of connected relays the local node
  treats as its mesh peers, with per-peer counters;
- a *reverse-mesh* set — the relays that have included this node in
  *their* mesh;
- an *explicit-relay* list — relays pinned by configuration and never
  evicted from the mesh;
- a relay-mesh-maintenance timer running on a bound 10-second
  interval.

**R11.** The relay-mesh-maintenance tick MUST:

1. fill the relay-mesh up to the substrate's configured low watermark
   (`mesh_n_low`) by drawing from connected relays not currently in
   the mesh;
2. prune the relay-mesh back to the high watermark (`mesh_n_high`)
   when it exceeds it;
3. preserve every entry in the explicit-relay list against eviction.

**R12.** The substrate's `IAmRelay` control message is bound: nodes
that act as relays MUST emit it; nodes that act as clients MUST NOT.
The configuration flag that selects which role applies is bound as a
boolean `i_am_relay` field on the substrate's gossipsub configuration
surface.

**R13.** The substrate's gossipsub configuration surface MUST further
expose:

- a content-addressed message-id function that hashes payload-plus-
  sequence-number, so duplicate payloads collapse to a single
  message id;
- the three mesh-size watermarks (`mesh_n_low`, `mesh_n`,
  `mesh_n_high`), with substrate-level defaults differing between
  client and relay roles;
- a manual-propagation flag — when set, the consumer is responsible
  for invoking the substrate's propagate-message method after
  validating a message, which gives the application the ability to
  drop invalid messages before forwarding;
- a maximum transmit size bound at slightly under 1 MiB.

**R14.** The substrate MUST NOT carry the peer-scoring / reputation
extension present in later revisions of the standard gossipsub
implementation. Abusive-peer handling falls to the consumer's manual-
disconnect logic and to R9's force-disconnect.

## 28.7 Bound Topic and Application-Payload Signing Surface

**R15.** Pub/sub topic strings MUST be constructed by the bound
`pub_sub_topic(prefix, topic)` helper. The topic separator MUST be
bound as the single byte `/`, exposed as the `TOPIC_SEPARATOR` constant
on the substrate's public surface. The resulting topic string is the
verbatim concatenation `<prefix><TOPIC_SEPARATOR><topic>`.

**R16.** Application payloads published on the mesh MUST be wrapped in
a secp256k1-signed envelope before publication. The substrate exposes
exactly two functions for this:

- `encode_and_sign<T: Serialize>(message: &T, secret: &[u8; 32]) ->
  Vec<u8>` — msgpack-encodes the payload, SHA-256 hashes the encoded
  bytes, signs the hash with the supplied secp256k1 secret, and packs
  `{ pubkey, signature, payload }` back into a msgpack envelope.
- `decode_signed<'de, T: Deserialize<'de>>(encoded: &'de [u8]) ->
  Result<(T, Signature, PublicKey), _>` — the inverse: parses the
  envelope, verifies the signature against the embedded public key,
  and returns the payload alongside the verified signature and public
  key.

**R17.** This application-payload signing surface is bound as *separate*
from the proxy-signature substrate of Chapter 27. The two operate on
different key spaces (raw secp256k1 here; libp2p-identity keypairs for
proxy signing) and address different threat models (mesh-message
authenticity here; HTTP-relay request-authentication for proxy signing).
Unification of the two signing surfaces is recorded in §28.10 as
deferred.

## 28.8 Bound Transport and Upgrade Stack

**R18.** The transport stack is target-dependent and is bound per
target:

| Target  | Bound transport                                                                                       |
| ------- | ----------------------------------------------------------------------------------------------------- |
| Native  | TCP plus DNS, optionally upgraded with WebSocket and WSS when the configuration supplies a TLS bundle. |
| WASM    | Browser WebSocket via the libp2p WASM FFI substrate.                                                  |
| Testing | An in-process memory transport for the substrate's own integration tests.                              |

**R19.** The application-level upgrade pipeline is bound to be uniform
across all targets: noise XX (`Xx25519Spec`) key agreement followed by
mplex stream multiplexing, with a 20-second upgrade timeout.

**R20.** The bootstrap-multiaddr formats the configuration accepts are
bound to the standard libp2p forms: `/ip4/<addr>/tcp/<port>`,
`/ip4/<addr>/tcp/<port>/wss`, `/dns/<host>/tcp/<port>`, and
`/memory/<port>` for the testing transport.

## 28.9 Bound Discovery, Mesh Maintenance, NAT

**R21.** The substrate MUST NOT attempt open-internet peer discovery.
The bootstrap list supplied by the daemon is the bound source of truth
for initial peer addresses. In production that list is expected to come
from operator-provided `seednodes` in `MM2.json`; a compiled registry
fallback may contribute entries, but compiled production seed nodes are
not required by the substrate contract.

**R22.** At swarm-spawn time the substrate MUST:

1. parse the configured bootstrap list (a vector of `RelayAddress`)
   into multiaddrs;
2. dial up to `mesh_n` random entries; if the parsed list is empty,
   start the swarm without dialing any relay;
3. start a 10-second maintenance timer running R11 plus a periodic
   peers-exchange request to a random connected relay on a bound
   300-second interval after a bound 20-second initial delay.

**R23.** NAT traversal is bound to a *structural* solution: relay
nodes MUST be deployed at globally-routable addresses, and client
nodes that sit behind NAT reach the mesh exclusively via at least one
relay. The substrate MUST NOT advertise non-routable listener
addresses to peers; the bound `ip_helpers::is_global` predicate is
applied to listener announcements.

**R24.** A client node with no configured or reachable relay peers SHALL
remain in an empty relay-mesh state until at least one relay connection is
established. The relay-mesh maintenance loop may report that the mesh is
below its low watermark, but this condition is diagnostic rather than a
startup failure. While the relay mesh is empty, orderbook and swap pub/sub
traffic cannot reach the wider network through this substrate.

**R25.** A node configured as a relay may start with an empty bootstrap
list and act as the first reachable relay for a deployment. Other nodes
must receive that relay's address through `seednodes` or another
operator-controlled bootstrap channel before they can join its mesh.

## 28.9A Required Port — Peer Health-check, Time-sync Admission, Expirable Bans (driving-spec)

**STATUS.** The three behaviours in this section are post-baseline
upstream additions that hang off the consolidated substrate. They
are **required ports in reloaded**. RP1 and RP2 are implemented,
and RP3 is implemented with expirable bans. Per the PORT decision
these are binding requirements, not optional deferred work.

### 28.9A.1 RP1 — Peer connection health-check RPC (implemented in reloaded)

**RP1.** A public top-level JSON-RPC v2 method
`peer_connection_healthcheck` MUST be added. It answers whether a
named peer is currently reachable on the mesh.

- **Request:** an object with a single field `peer_address` — the
  string rendering of the target peer's libp2p peer id.
- **Response:** a bare JSON boolean — `true` if the peer
  acknowledged within the timeout (or is the local node itself),
  `false` otherwise.
- **Behaviour:** if `peer_address` equals the local node's own
  peer id, return `true` immediately. Otherwise the node MUST
  build a signed health-check probe (signed with the §28.7
  application-payload signing surface), register an **expirable**
  one-shot waiter keyed by the target peer address (the record
  self-clears on expiry), publish the probe on a dedicated
  per-peer health-check pub/sub topic derived from the peer
  address via the §28.7 `pub_sub_topic` helper, and await an
  acknowledgement. The call returns `true` if an ack arrives
  before a bound timeout (the health-check message expiry) and
  `false` on timeout.
- **Responder side:** a node receiving a health-check probe on the
  health-check topic MUST verify the signed envelope and, when the
  probe targets it, reply on the same topic so the originator's
  waiter is woken.
- **Errors:** failures MUST surface through the project's typed-
  error envelope (`error_type` / `error_data`) with wire tokens
  distinguishing a probe-generation failure, a probe-encoding
  failure, and a generic internal failure; all map to server-error
  (500). The human-readable message wording is NOT part of the
  contract.

**RP1 acceptance:** a caller can ask `peer_connection_healthcheck`
for a connected peer and get `true`, for an unreachable/unknown
peer get `false` after the timeout, and for its own peer id get
`true` immediately.

### 28.9A.2 RP2 — Network time-synchronisation peer admission (implemented in reloaded)

**RP2.** Immediately after a connection to a peer is established,
the node MUST validate that peer's clock and disconnect peers
whose clock is too far from local time. This guards swap timing
assumptions that depend on near-synchronised clocks.

- **Mechanism:** the node issues a request-response query over the
  substrate's request-response sub-behaviour (§28.5 R6) asking the
  newly-connected peer for its current UTC timestamp (Unix epoch
  seconds). The peer replies with a msgpack-encoded unsigned
  epoch-seconds value.
- **Admission rule:** the node compares the reported timestamp to
  its own UTC time. If the absolute difference is within the bound
  maximum gap, the peer is admitted. If the difference exceeds the
  gap — or the peer fails to return a well-formed timestamp — the
  node MUST disconnect that peer.
- **Bound threshold:** the maximum acceptable gap is **20
  seconds**, exposed as a single named constant in the P2P layer.
  This value is depended on by swap-timing defaults and MUST NOT be
  changed casually.
- **Gating:** the admission check is gated behind the substrate's
  `application` build feature (it is part of the application-level
  P2P behaviour, not the bare transport).

**RP2 acceptance:** a peer whose reported UTC differs from local
by ≤ 20 s stays connected; a peer reporting a timestamp outside
that gap is disconnected shortly after connection establishment.

### 28.9A.3 RP3 — Expirable pubkey bans (partially present; expiry missing)

**RP3.** The pubkey-ban store MUST become **expirable**: a ban
entry MAY carry a time-to-live and, when it does, MUST auto-clear
once the TTL elapses without requiring an explicit unban. In
reloaded today the ban store is a plain map and every ban is
permanent until manually unbanned; the port adds expiry semantics
and a duration knob.

- **Manual ban — `ban_pubkey`.** The existing legacy RPC request
  `{ "pubkey": <pubkey hash>, "reason": <string> }` MUST gain an
  optional field `duration_min` (unsigned minutes). When
  `duration_min` is present, the ban is inserted with that expiry
  and auto-clears afterwards; when absent, the ban is **constant**
  (persists until an explicit unban). Banning an already-banned
  pubkey MUST be rejected. The response is the existing success
  acknowledgement (`{ "result": "success" }`).
- **Failed-swap auto-ban.** The automatic ban applied when a swap
  fails MUST become **time-limited** with a bound penalty of **one
  hour (3600 seconds)**, expiring automatically, rather than
  permanent.
- **List — `list_banned_pubkeys`.** Returns the current,
  non-expired ban set as `{ "result": <map of pubkey hash → ban
  reason> }`. The ban-reason wire shape is a `type`-tagged object:
  `{ "type": "Manual", "reason": <string> }` or
  `{ "type": "FailedSwap", "caused_by_swap": <uuid>,
  "caused_by_event": <swap-event> }`.
- **Unban — `unban_pubkeys`.** Request
  `{ "unban_by": { "type": "All" } }` or
  `{ "unban_by": { "type": "Few", "data": [ <pubkey hash>, … ] } }`.
- **Difference from the existing swap-failure ban:** in reloaded
  every ban is currently permanent; the port makes failed-swap
  bans self-expire after one hour and lets manual bans opt into a
  TTL via `duration_min`, while a manual ban with no `duration_min`
  remains permanent. Expired entries disappear from
  `list_banned_pubkeys` and stop being enforced without an explicit
  unban.

**RP3 acceptance:** a manual ban with `duration_min = N` disappears
from `list_banned_pubkeys` and stops being enforced after N
minutes; a manual ban with no `duration_min` persists until
unbanned; a failed-swap ban self-expires after one hour; the
`ban_pubkey` / `list_banned_pubkeys` / `unban_pubkeys` wire shapes
above are preserved.

## 28.10 Tests (test invariants)

**T1.** *Composed-behaviour shape.* A reflection-style test (or a
documentation-extracting audit) MUST confirm that the composed
`NetworkBehaviour` contains exactly the five sub-behaviours bound in
R6, and none of the seven libp2p protocol families excluded by R7.

**T2.** *Relay-mesh maintenance.* An in-process mesh test using the
testing memory transport MUST:

1. spawn one relay node and two client nodes;
2. observe that each client's connected-relays set includes the
   relay;
3. observe that the relay-mesh of each client includes the relay
   after one or two maintenance ticks (within roughly 30 seconds of
   simulated time);
4. force the relay to disconnect and observe that the maintenance
   tick removes it from each client's relay-mesh.

**T3.** *Pinned explicit relays.* A node configured with an explicit
relay MUST retain that relay in its relay-mesh across a
maintenance-tick cycle that would otherwise prune it when the mesh
exceeds the high watermark (R11 step 3).

**T4.** *Application-payload signing round-trip.* For arbitrary
serializable values, `decode_signed(encode_and_sign(value, sk))` MUST
return `Ok((value, signature, pubkey))` with `pubkey` matching the
public key derived from `sk`. Any single-byte mutation of the encoded
bytes MUST cause `decode_signed` to return an error and MUST NOT
return a parseable but invalid payload.

**T5.** *Topic construction.* `pub_sub_topic("orderbook",
"KMD:BTC")` MUST return exactly `"orderbook/KMD:BTC"`; the substrate
MUST NOT perform any URL-encoding or canonicalisation of the inputs.
A test or audit MUST confirm the helper's implementation reads
`TOPIC_SEPARATOR` (not a hard-coded `/`) so a future separator change
remains a one-symbol substrate edit.

**T6.** *Peer-exchange bound.* A peers-exchange request MUST receive
no more than 100 addresses in its response, regardless of how many
peers the responder is connected to.

**T7.** *Empty bootstrap list.* A client spawned with no bootstrap
relays MUST start without a fatal P2P initialisation error, keep an
empty relay mesh, and report zero connected relays until a reachable
relay is introduced. A relay node spawned with no bootstrap relays
MUST still listen on its configured reachable address.

## 28.11 Deferred Work

> **Note.** The §28.9A items (peer health-check RPC, time-sync
> peer admission, expirable pubkey bans) are **required ports**,
> NOT deferred work — they are binding driving-spec requirements
> an implementer MUST land. The items below are genuine
> deferrals.

**D1.** A libp2p version bump (and the accompanying touch on every
sub-behaviour and the swarm-builder code) is deferred. The substrate
preserves the baseline libp2p revision pin and feature flags.

**D2.** Removal of the retained pre-substrate peer-discovery crate
(R4) from the workspace is deferred. Its retention costs build time
but preserves repository-history continuity.

**D3.** Unification of the two signing surfaces — application-payload
signing in R16 and proxy-signature signing in Chapter 27 — is
deferred. The two key spaces and threat models are currently kept
separate by design.

**D4.** Addition of peer-scoring / reputation extensions to the
vendored gossipsub (or migration to a modern standard gossipsub
implementation that carries them) is deferred. The current substrate
handles abusive peers via manual-disconnect logic plus R9 force-
disconnect.

**D5.** Addition of libp2p relay-v2 plus DCUtR fallback for clients
whose only path to a relay is blocked is deferred. The substrate
currently degrades to "no connection" for such clients.

**D6.** Automatic production seed-node discovery is deferred. Operators
remain responsible for supplying reachable relay addresses when the binary
does not carry a usable registry fallback for the selected netid.

## 28.12 External References

- *libp2p* — the underlying networking substrate; the substrate pins
  to the project's existing external-dependency revision (the pin
  itself is workspace-side metadata, not bound here).
- *libp2p gossipsub specification* — the basis the vendored gossipsub
  extends with R10–R13.
- *libp2p floodsub specification* — the basis of the substrate's
  flood-based topic.
- *libp2p noise handshake specification* — the bound key-agreement
  protocol in R19.
- *libp2p multiaddr format* — the bound bootstrap-address grammar in
  R20.
- *secp256k1* — the bound signing primitive in R16.
- *msgpack serialization format* — the bound envelope serialization
  in R16.
- Chapter 06 (network-id and seed-node decoupling) — bound source of
  the network-identifier scoping that all mesh traffic carries.
- Chapter 27 (infrastructure substrate inventory), specifically the
  proxy-signature substrate row — the *other* signing surface, kept
  separate per R17.
- Chapter 11 (order-match cancellation race) and Chapter 09
  (watcher infrastructure) — primary consumers of the bound topic
  surface of R15.

## 28.13 Baseline Verifications

**V1.** The baseline workspace MUST be confirmed to contain four
distinct P2P-related substrate directories under the project's
crate root, with the layout bound in §28.1 (one glue substrate,
two vendored protocol substrates, one peer-discovery substrate). A
`git ls-tree` over the project crate root at the baseline commit
MUST list all four.

**V2.** The baseline composed network behaviour MUST be confirmed to
contain the five sub-behaviours of R6 (the bound substrate
preserves the shape, only consolidates its location). A `git grep`
for the bound sub-behaviour names against the baseline glue crate's
behaviour module MUST confirm all five.

**V3.** The baseline libp2p revision pin and per-target feature flags
MUST be confirmed identical to the substrate's pin and flags. The
bound substrate is a crate-layout consolidation, not a libp2p version
bump:

```
git -C <baseline> show c1d46c0:<root>/<glue-crate>/Cargo.toml | grep -E 'libp2p|features'
```

## 28.14 Provenance Footer

- *Inputs consulted for this chapter:* the baseline tree at project
  baseline commit `c1d46c0c1592faa0860f704008b2b2381bc3840f`,
  Chapter 06 (network-id substrate), Chapter 09 (watcher topic
  conventions), Chapter 11 (order-match cancellation cache), Chapter
  27 (infrastructure substrate inventory and proxy-signature
  substrate row), Chapter 31 (the central application-context
  substrate the `p2p_ctx` sub-context slot, the `peer_id` once-set
  field, and the P2P command-channel sender are bound on per
  chapter 31 R6 / R7), and the external libp2p / cryptographic /
  wire-format specifications listed in §28.12.
- *Permitted-input classes used:* baseline source; chapter-bound
  substrate identifiers introduced here as contract surface
  (`AtomicDexBehaviour` composed-behaviour shape, `PeersExchange`,
  `AdexPing`, `RelayAddress`, `PeerAddresses`, `encode_and_sign`,
  `decode_signed`, `pub_sub_topic`, `TOPIC_SEPARATOR`, the bound
  `/peers-exchange/1` protocol identifier, the `IAmRelay` control-
  message name, the `i_am_relay` configuration field, the
  `mesh_n_low`/`mesh_n`/`mesh_n_high` parameter names, the bound
  numeric constants 10 s / 300 s / 20 s / 100 / ~1 MiB); standard
  libp2p protocol names; standard cryptographic primitive names
  (secp256k1, noise XX, SHA-256, msgpack).
- *Sibling chapters cross-referenced:* Chapter 06, Chapter 09,
  Chapter 11, Chapter 27, Chapter 31.
- *Author of this chapter:* clean-room round-2 driving-spec working
  set.
- *Forbidden corpus:* not consulted.
