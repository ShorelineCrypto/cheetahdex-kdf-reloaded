# Chapter 28 — P2P Substrate Consolidation

**Status:** driving-spec.

This chapter binds the consolidated peer-to-peer substrate: the single
crate boundary, the composed network behaviour and its deliberately
constrained sub-behaviour set, the vendored relay-mesh-aware gossipsub
extension, the bound topic-naming and application-payload-signing surface,
the transport stack per build target, and the discovery / bootstrap /
mesh-maintenance discipline.

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
The bootstrap list supplied via configuration is the bound source of
truth for initial peer addresses.

**R22.** At swarm-spawn time the substrate MUST:

1. parse the configured bootstrap list (a vector of `RelayAddress`)
   into multiaddrs;
2. dial up to `mesh_n` random entries;
3. start a 10-second maintenance timer running R11 plus a periodic
   peers-exchange request to a random connected relay on a bound
   300-second interval after a bound 20-second initial delay.

**R23.** NAT traversal is bound to a *structural* solution: relay
nodes MUST be deployed at globally-routable addresses, and client
nodes that sit behind NAT reach the mesh exclusively via at least one
relay. The substrate MUST NOT advertise non-routable listener
addresses to peers; the bound `ip_helpers::is_global` predicate is
applied to listener announcements.

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

## 28.11 Deferred Work

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
  substrate row), and the external libp2p / cryptographic / wire-
  format specifications listed in §28.12.
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
  Chapter 11, Chapter 27.
- *Author of this chapter:* clean-room round-2 driving-spec working
  set.
- *Forbidden corpus:* not consulted.
