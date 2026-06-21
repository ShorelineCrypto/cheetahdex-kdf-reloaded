# Chapter 06 -- Network-ID and Network-Configuration Registry

**Status:** driving-spec

> **One-sentence claim:** the project's notion of a "network" --
> a peer-to-peer subnetwork identified by a 16-bit numeric
> netid -- shall be expressed as a compile-time registry
> implementing a single public trait that covers every
> per-network constant (optional seed nodes, DEX-fee addresses in
> three flavours, fee rates, burn-share parameters), and the
> daemon shall refuse to start on any netid the binary cannot
> describe.

## 6.0 Executive Summary

Every peer-to-peer instance of the codebase belongs to exactly
one **network**, identified by a 16-bit numeric tag (`netid`)
supplied in the daemon's JSON configuration. Two daemons with
different netids cannot exchange swap traffic with each other:
their seed-node sets do not intersect, their DEX-fee addresses
differ, their fee-rate constants differ, and their burn
behaviour differs. The netid is the single coarse-grained
namespace that separates one operational network from another.

This chapter defines a binding shape for the source-of-truth
that backs every per-netid constant in the codebase:

1. A dedicated workspace crate provides a **network-config
   trait** and a **compile-time registry** that maps each
   supported netid to a trait object. The trait covers every
   per-network constant the codebase needs.
2. Each supported network is registered by adding a small Rust
   module that implements the trait against a zero-sized unit
   struct. Production netids are unconditionally registered;
   test-only netids are gated behind a single Cargo feature so
   release builds cannot accidentally accept them.
3. The peer-to-peer subsystem is **netid-blind**: it accepts a
   resolved list of relay addresses to dial on startup and does
   not know which netid produced that list. Production operation
   expects operators to provide reachable `seednodes` in
   `MM2.json`; compiled-in seed-node lists are optional bootstrap
   conveniences, not a precondition for a working binary.
4. The daemon **refuses to start** on any netid the registry
   does not describe. This is enforced at initialisation, before
   the peer-to-peer subsystem is brought up.

The wire protocol does not encode the netid; the JSON
configuration shape is unchanged from any prior arrangement
(`"netid": u16`, `"seednodes": [string]`, `"i_am_seed": bool`).
What this chapter binds is the *source of truth* for
per-network constants: a typed, compile-time registry behind a
single trait, not scattered constants across the codebase.

## 6.1 The Network-Config Trait

A single workspace crate exposes a public trait,
**`NetConfig`** (this is the in-tree symbol name and is bound by
this chapter), with the operations enumerated below. The trait
is `Send + Sync + 'static` so trait-object handles can be held
across threads.

| Operation                          | Returns                    | Purpose                                          |
|------------------------------------|----------------------------|--------------------------------------------------|
| Netid identity                     | `u16`                      | The netid this configuration belongs to          |
| Human-readable network name        | `&'static str`             | Display / logging                                |
| **DEX-fee address (secp256k1)**    | `&'static str`             | Hex-encoded compressed-secp256k1 pubkey          |
| DEX-fee raw pubkey                 | `&'static [u8]`            | The same pubkey as raw bytes                     |
| **DEX-fee Z-address**              | `&'static str`             | The Zcash-family shielded-address variant        |
| **DEX-fee ed25519 pubkey**         | `&'static str`             | The ed25519 variant for Sia-style chains         |
| DEX-fee rate                       | `BigRational`              | Precise fee rate (no floating-point)             |
| Fee-discount ticker set            | `&'static [&'static str]`  | Tickers that receive the discounted fee rate     |
| DEX-fee discounted rate            | `BigRational`              | The discounted rate applied to the ticker set    |
| DEX-fee minimum threshold          | `BigRational`              | Floor below which the fee does not drop          |
| Burn-share enabled                 | `bool` (default `false`)   | Whether a fraction of the fee is burned          |
| DEX-fee share                      | `BigRational` (default 1)  | Fraction retained as fee (vs burned)             |
| Burn address (secp256k1)           | `&'static str` (default "")| Hex-encoded compressed-secp256k1 pubkey          |
| Burn address raw pubkey            | `&'static [u8]` (default &[])| Burn pubkey as raw bytes                       |
| Seed-node list                     | `&'static [&'static str]`  | Optional DNS names or address strings            |

Return-type discipline:

- Numeric identity returns are plain values.
- String and byte-slice returns are `&'static`, with the data
  interned into the binary at compile time. Hex-decoded bytes
  are produced at compile time via a `const`-decoded helper so
  there is no runtime parse cost and no possibility of a parse
  failure at runtime.
- Rate returns use a big-rational type so DEX-fee math never
  loses precision.
- Burn-related methods carry default implementations that
  produce the "burn-disabled" answer; networks that do not burn
  do not need to spell those methods out.

The DEX-fee address is exposed in **three flavours** because the
codebase serves chains with three distinct address-encoding
families (Bitcoin-family compressed secp256k1, Zcash-family
shielded, Sia-style ed25519). A single network has a single
DEX-fee identity expressed three ways; consumers select the
flavour appropriate to the chain being charged.

## 6.2 The Registry

The same crate exposes two registry entry points:

| Function                           | Return                          | Purpose                                              |
|------------------------------------|---------------------------------|------------------------------------------------------|
| **`net_config_for(netid: u16)`**   | `Option<&'static dyn NetConfig>`| Lookup; returns `None` for unknown netids            |
| **`net_config_or_panic(netid)`**   | `&'static dyn NetConfig`        | Lookup; panics with a descriptive message on failure |

Both names are the in-tree symbol names and are bound by this
chapter.

The lookup is implemented as a single `match` over the netid
value, with one arm per registered network returning a static
reference to a unit-struct trait object. The returned trait
object is a fat pointer to a zero-sized unit struct living in
`'static` storage; there is no heap allocation, no
synchronisation cost, no shutdown ordering issue. The cost of
looking up a network's parameters at runtime is one match arm
and one indirect call.

The descriptive panic emitted by the second entry point shall
list every netid the binary was compiled to support, so that an
operator who supplies an unsupported netid is informed which
netids would be accepted.

## 6.3 Compile-Time Registration of a Network

Each supported network is registered by a small Rust module
inside the crate. The module convention is:

| Element                       | Shape                                                  |
|-------------------------------|--------------------------------------------------------|
| File name                     | `netid_NNNN.rs` where `NNNN` is the netid              |
| Public type                   | One unit struct, `pub struct NetidNNNN;`               |
| Implementation                | One `impl NetConfig for NetidNNNN { ... }` block       |
| Constants                     | All per-network values appear in this one impl block   |
| Crate registration            | One match arm in `net_config_for` returning `&NetidNNNN`|

The unit-struct + per-module convention is binding: it keeps
each network's constants in exactly one file, ensures there is
exactly one statically-known instance per network, and lets the
registry's match arm return a `&'static dyn NetConfig` without
any allocation.

Adding a new network is therefore three changes in one crate:
the new file, the new use, the new match arm. No file outside
this crate needs to change.

## 6.4 Production and Test Netid Separation

The codebase distinguishes **production** netids from
**test-only** netids:

- Production netids are registered unconditionally; they are
  always present in any build of the daemon.
- Test-only netids are registered behind a single Cargo
  feature, **`regtest-netid`** (this is the in-tree feature
  name and is bound by this chapter). The release build of the
  daemon does not enable this feature.

This separation is binding: it is not acceptable to register a
test netid unconditionally, and it is not acceptable to gate a
production netid behind a feature flag. The intent is that the
release binary cannot be tricked into accepting a test-only
netid through a configuration alone; the cost of doing so is a
rebuild with a non-release feature enabled.

The test netids are collected in a single Rust module that
exists only when the feature is enabled; the production netids
each have their own module. A single macro inside the test-
netid module produces one unit struct, one impl block, and one
constructor call per test netid, keeping the test-netid set
short and uniform.

## 6.5 Peer-to-Peer Netid-Blindness

The peer-to-peer subsystem (covered in detail in
[Chapter 28](28-libp2p-modernization.md)) is **netid-blind**:

R1. The peer-to-peer subsystem shall not accept a netid
    parameter on any of its public entry points.
R2. The peer-to-peer subsystem shall not store a netid in any
    of its state structures.
R3. The peer-to-peer subsystem shall not contain any per-netid
    branch on a numeric netid value.
R4. The peer-to-peer subsystem shall not carry an in-source
    list of seed-node addresses for any specific netid.
R5. The peer-to-peer subsystem shall accept its bootstrap
    seed-node list as a parameter at startup; the caller is
    responsible for having resolved that list from operator
    configuration and, only when appropriate, the network-config
    registry fallback.

R1-R5 together mean the peer-to-peer subsystem can be compiled
and tested without knowing any netid at all. The "what network
am I on" question is answered exclusively by the caller's
selection of which network-config trait object to consult.

The pub/sub fan-out behaviour at the peer-to-peer layer is
**not netid-conditional**: the codebase shall use the
unconditional more-permissive variant of the fan-out behaviour
across all networks. There is no per-network branch on whether
the well-known fan-out variant applies.

## 6.6 Daemon Startup Guard

The daemon initialisation path shall, **before** bringing up the
peer-to-peer subsystem:

S1. Read `netid` from the daemon's JSON configuration, treating
    a missing value as `0`.
S2. Call the registry's `net_config_or_panic(netid)` entry
    point. This either returns a trait object or terminates the
    daemon with the descriptive message of §6.2.
S3. Use the returned trait object as the source of truth for
    every per-network constant the daemon needs at startup. The
    seed-node list is an exception in priority only: operator
    configuration wins when supplied.

The seed-node-resolution helper called by the daemon shall:

S4. Prefer an operator-supplied `seednodes` list in the JSON
    configuration if present. This is the expected production
    bootstrap path.
S5. Treat an explicitly supplied empty `seednodes` list as "dial
    no bootstrap relays"; the daemon may still start, but it has
    no initial route into the relay mesh unless it is itself a
    reachable relay or peers are supplied later by another
    mechanism.
S6. If `seednodes` is absent, an implementation MAY fall back to
    the registry's seed-node list for the active netid. That
    fallback is not required to be non-empty; release builds MUST
    NOT depend on hard-coded production seed nodes being present.
S7. Treat each registry-supplied seed-node string as a bare
    host string: either an IPv4 literal or a DNS name, not a
    full multiaddr. On native targets, map that host to the TCP
    P2P port derived from the active netid before passing it to
    the peer-to-peer subsystem. DNS may be preserved as a DNS
    address or resolved before dialing according to the active
    target transport.
S8. WSS seed connectivity is optional transport support, not a
    replacement for the native TCP seed mapping. When WSS is
    configured, WSS addresses use the configured WSS port and
    TLS bundle. In-process memory addresses are test-only and
    shall not appear in production registry seed-node lists.
    Operator-supplied `seednodes` are parsed as the P2P
    relay-address surface of Chapter 28.

Because S2 has already rejected unknown netids by the time
seed-node resolution runs, the fallback in S6 never has to
handle a missing registry entry.

> **Upstream divergence (informative).** The historical lineage treated
> operator-provided `seednodes` as the normal bootstrap source. The active
> RELOADED branch may additionally use a network-registry fallback. This
> chapter keeps the fallback optional and requires production deployments to
> work without hard-coded production relay addresses in the binary.

## 6.7 Wire and Configuration Invariants

The following must remain true regardless of how the registry
is extended:

I1. The daemon's JSON configuration shape is unchanged: the
    `netid`, `seednodes`, and `i_am_seed` fields retain their
    types and meanings.
I2. The peer-to-peer wire protocol does not encode the netid.
    Two daemons with different netids end up on disjoint
    gossipsub meshes because their seed-node sets do not
    intersect, not because the protocol carries a netid byte.
I3. The DEX-fee identity for a given netid is a single value
    expressed three ways (one per address-encoding family); the
    three representations correspond to the same key material.
I4. The DEX-fee rate is exact (`BigRational`), not
    floating-point. No part of the codebase shall convert it to
    `f64` for fee math.
I5. The burn-share parameters are optional: a network that does
    not burn omits the corresponding overrides and inherits the
    defaults of §6.1.

## 6.8 Binding Requirements

R1. **Single trait.** All per-network constants flow through
    the one trait of §6.1. No per-network constant shall live
    outside that trait.

R2. **Closed registry.** The lookup function of §6.2 is the
    single point at which a netid is mapped to its
    configuration. No code outside the network-config crate
    shall match on a numeric netid value.

R3. **Compile-time storage.** Each network's data is stored as
    `&'static`-backed constants on a unit struct. No allocation
    and no synchronisation participates in the lookup path.

R4. **Refuse-unknown.** The daemon shall call the panicking
    registry entry point during initialisation, before bringing
    up the peer-to-peer subsystem. Operating on an unrecognised
    netid is a fatal startup error.

R5. **Production / test separation.** Production netids are
    unconditional; test netids are gated behind the
    `regtest-netid` Cargo feature. The release build does not
    enable that feature.

R6. **Netid-blindness of the peer-to-peer subsystem.** R1-R5
    of §6.5 are binding; the peer-to-peer subsystem does not
    receive, store, or branch on a netid.

R7. **Wire/config invariants.** I1-I5 of §6.7 are binding.

R8. **Three-flavour DEX-fee identity.** Every registered
    network exposes the DEX-fee identity in all three flavours
    of §6.1, even if some flavour is currently unused by any
    consumer; this keeps the trait object total and avoids
    later widening that would force every existing
    registration to be edited.

## 6.9 Deferred and Out-of-Scope Items

D1. **DEX-fee semantics** beyond identity (when the fee is
    charged, who charges it, who receives it, how the burn
    share is computed and emitted) are covered in
    [Chapter 08](08-fee-routing-engine.md). This chapter binds only the
    *source of truth* for the fee identity and rate constants,
    not their interpretation.

D2. **Per-coin activation** is handled by the per-coin activation
    layer, which consults the network-config registry to resolve the
    DEX-fee identity for the chain it is activating.

D3. **Burn-emission integration** is covered in the
    daemon-wide central-context substrate (how that context
    exposes the network-config handle) and
    [Chapter 08](08-fee-routing-engine.md) (how the burn share is split
    out and emitted on-chain).

D4. **Macro consolidation** for the test-netid module is an
    implementation choice; the binding rule is that all test
    netids live in a single feature-gated module of §6.4, not
    that they are produced by any specific macro shape.

## 6.10 External References

- The libp2p gossipsub specification (the fan-out semantics
  the codebase relies on across all networks).
- The libp2p floodsub specification (the more-permissive
  fan-out variant of §6.5 used unconditionally).
- The `num-rational` crate (the big-rational type backing the
  precise fee-rate returns of §6.1).
- The Cargo features model (the mechanism backing the
  production / test netid separation of §6.4).
- The Rust conditional-compilation reference (the `cfg`
  attribute used to feature-gate the test-netid module).

## 6.11 Baseline Verifications

The following are verifiable from the baseline state defined in
[Chapter 02](02-baseline-state.md), commit
`c1d46c0c1592faa0860f704008b2b2381bc3840f`:

V1. The baseline tree contains no `mm2_net_config` workspace
    member. A directory listing of the baseline tree
    (`git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`)
    contains no `mm2_net_config` entry; a tree-wide
    `git grep -l 'NetConfig\|net_config_for'` against the
    baseline returns no matches.

V2. At baseline, the per-network constant set the trait of
    §6.1 collects is scattered across multiple files:
    seed-node lists, fee-address constants, and the magic
    fan-out gate live in different places. The driving rule
    R1 of §6.8 (single trait) is therefore a strengthening of
    the baseline shape, not a restatement of it.

V3. At baseline, the peer-to-peer subsystem accepts a `netid`
    parameter on its public entry points and stores it in its
    behaviour struct. R1-R5 of §6.5 (netid-blindness) are
    therefore a strengthening of the baseline shape and
    require coordinated change in both the peer-to-peer
    subsystem and its callers.

V4. At baseline, the daemon does not refuse to start on an
    unrecognised netid: a daemon configured with an unknown
    netid will boot with empty seed-node lists. R4 of §6.8
    (refuse-unknown) is therefore a strengthening of the
    baseline shape.

V5. The 16-bit width of `netid` is preserved from the
    baseline. The `netid` field in the JSON configuration is
    a `u16` at baseline and remains a `u16` under the rules
    of this chapter.

## 6.12 Provenance Footer

- *Status:* driving-spec.
- *Version:* v2.
- *Verified against:* baseline commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`; absence of the
  network-config crate at baseline verified via
  `git ls-tree c1d46c0c1592faa0860f704008b2b2381bc3840f`
  and tree-wide `git grep` for the trait and registry symbol
  names against the baseline; the libp2p gossipsub
  specification; the libp2p floodsub specification; the
  `num-rational` crate (rate-arithmetic substrate); the
  Cargo features model; the Rust conditional-compilation
  reference.
- *Forbidden corpus:* not consulted.
