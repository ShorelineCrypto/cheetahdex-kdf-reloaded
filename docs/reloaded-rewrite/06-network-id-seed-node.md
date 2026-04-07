# Chapter 06 — Network-ID and Seed-Node Decoupling

## Executive Summary

At the baseline the project's notion of a "network" — that is, of
a particular peer-to-peer subnetwork separated from all others by
a numeric tag — was scattered. The numeric tag itself lived as
`netid` in the JSON configuration consumed by `MmCtx`; one
specific tag, the literal `7777`, was hard-coded as the magic
constant `NETID_7777` in the `mm2_libp2p` crate and used to gate
two unrelated behaviours (the floodsub-compatibility flag and
seed-node bootstrapping); a list of fifteen production seed-node
`(PeerId, IPv4)` pairs for that one network was a hard-coded
slice in `mm2_libp2p::network`; and a parallel list of three DNS
seed names for the same network lived in `lp_native_dex.rs`. Any
new netid required edits in three or four files and no
mechanism existed to refuse unknown netids.

The post-baseline modernisation introduces a dedicated crate,
`mm2_net_config`, that defines a public `NetConfig` trait and a
`net_config_for(netid: u16) -> Option<&'static dyn NetConfig>`
registry. Each network supported by the binary gets its own
small module implementing the trait at compile time
(`netid_8762.rs`, `netid_6133.rs`, an optional `test_netids.rs`
behind a `regtest-netid` Cargo feature). The trait surface covers
not only seed-node DNS names but every per-network constant that
used to be hard-coded against `NETID_7777`: the human-readable
network name, the DEX fee address (in three flavours —
compressed secp256k1, Z-address, and ed25519 — to support
Bitcoin-family, Zcash-family, and Sia-style chains), the DEX
fee rate (as a precise `BigRational`), the fee discount tickers
and their discounted rate, the minimum fee threshold, and the
burn-share parameters. The binary refuses to start on an
unsupported netid.

The peer-to-peer crate itself becomes netid-agnostic.
`spawn_gossipsub` and `start_gossipsub` no longer take a
`netid` parameter; the `AtomicDexBehaviour` struct no longer
stores one; the `Floodsub` construction no longer branches on
the `NETID_7777` literal (it now uses the unconditional
behaviour that the baseline reserved for non-`7777` networks);
and the hard-coded seed-node slice is removed from
`mm2_libp2p::network`. The startup path in `lp_native_dex.rs`
now consults `net_config_for(ctx.netid()).seed_nodes()` for
default seed-node resolution instead of a baked-in slice.

The wire side is unaffected. `netid` remains a `u16` field in the
daemon's JSON configuration; the on-the-wire peer-to-peer
protocol does not include the netid; the daemon still computes
all per-coin operations against the chosen netid in the same way.
What changes is exclusively the *source of truth* for per-network
parameters: from "hard-coded constants inside the peer-to-peer
crate" to "a compile-time registry implementing a public trait".

A reader leaving this chapter should be able to (a) reconstruct
the `mm2_net_config` crate's file layout and trait surface,
(b) name every kind of per-network constant the trait covers,
and (c) explain why the new design lets unknown netids be
rejected at startup without any change to the peer-to-peer
protocol.

### Why this changed

The per-netid registry crate and the production/test netid split were introduced by the project's own commits `4ac13459f` (*feat(net_config): add feature-gated regtest netid 9000 for docker tests*) and `5ea034dcc` (*feat(net_config): consolidate test netids (8100, 8999, 9000, 9998)*). The first commit gives the operational reason verbatim:

> *The docker_tests harness configures spawned MM2 instances with "netid": 9000 (88 occurrences across 7 files), but RELOADED's compile-time netid registry only included 8762 (AtomicDEX) and 6133 (GLEEC). Every test that spawned an MM2 instance panicked at startup with "Unsupported netid 9000: no compiled configuration", masquerading in CI as connection-reset failures. … The production `mm2` binary leaves the feature off and continues to reject netid 9000 at startup.*

The second commit generalises the same fix to additional test netids and consolidates them behind a single `define_test_netid!` macro. Both commits reference the broader legal-mitigation work tracked as LP-3F.C1, which required all DEX-fee-address material to flow through `NetConfig` so it could be cleanly varied per netid without leaking constants across the workspace.

In clean-room voice: the post-baseline project chose a compile-time netid registry over the baseline's runtime branching on a single hard-coded numeric tag (`NETID_7777`) so that (a) production binaries refuse to start on any netid they cannot describe, (b) per-network constants — including the DEX-fee address and rate — have a single typed home, and (c) test-only networks can be enabled behind a Cargo feature without polluting the production surface.

## Reproduction Detail

### 6.1 The baseline shape of "what a netid means"

At commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` the netid
concept is split across four locations:

1. **`mm2src/mm2_core/src/mm_ctx.rs`** — `MmCtx::netid(&self)`
   reads `self.conf["netid"].as_u64().unwrap_or(0)`, checks the
   value fits in a `u16`, panicking if not, and returns it. This
   is the only per-runtime read of the field.
2. **`mm2src/mm2_libp2p/src/network.rs`** — defines the magic
   constant `pub const NETID_7777: u16 = 7777` and the hard-coded
   slice `ALL_NETID_7777_SEEDNODES: &[(&str, &str)]` of fifteen
   `(PeerId-as-string, IPv4-as-string)` pairs. Its public
   `get_all_network_seednodes(netid: u16) -> Vec<(PeerId, RelayAddress)>`
   function returns an empty vector for any netid other than
   `NETID_7777`.
3. **`mm2src/mm2_libp2p/src/atomicdex_behaviour.rs`** — the
   `AtomicDexBehaviour` struct stores `netid: u16` as a field;
   `spawn_gossipsub` and `start_gossipsub` take `netid: u16` as
   their first parameter after the force-key option; inside
   `start_gossipsub` two distinct behaviours branch on the
   constant:
   - `Floodsub::new(local_peer_id, netid != NETID_7777)` —
     enables (or disables, for the well-known network)
     floodsub-style fan-out subscription;
   - the seed-bootstrap loop `for (peer_id, addr) in get_all_network_seednodes(netid) { … }`.
4. **`mm2src/mm2_main/src/lp_native_dex.rs`** — `default_seednodes(netid)`
   returns three hard-coded DNS names for netid `7777` and an
   empty vector otherwise; the names are baked into a `const`
   array in the same file.

Adding a new netid at the baseline therefore required edits to
files (2), (3) twice (one for the floodsub branch and one for
the seed-bootstrap loop), and (4) — without any compile-time
guarantee that an unknown netid would be caught.

### 6.2 The post-baseline `mm2_net_config` crate

The post-baseline modernisation introduces a new workspace
member, `mm2src/mm2_net_config/`, with the following file
layout:

```
mm2src/mm2_net_config/
├── Cargo.toml
└── src/
    ├── lib.rs           # NetConfig trait, net_config_for registry, helpers
    ├── netid_8762.rs    # Unit struct + NetConfig impl for the main network
    ├── netid_6133.rs    # Unit struct + NetConfig impl for an alternate network
    └── test_netids.rs   # Feature-gated registrations for regtest IDs
```

The crate's stated purpose, paraphrased from its module
documentation, is: each registered netid gets a Rust module that
implements `NetConfig`, encoding all peer-to-peer network
parameters at compile time, and the binary rejects unknown
netids at startup. The published Cargo feature `regtest-netid`
gates the additional test-only netids so they are excluded from
release builds.

### 6.3 The `NetConfig` trait

`mm2_net_config::NetConfig` is the chapter's central public
surface. It is defined in `lib.rs` as:

```rust
pub trait NetConfig: Send + Sync + 'static {
    fn netid(&self) -> u16;
    fn network_name(&self) -> &'static str;

    // DEX fee address (three flavours).
    fn dex_fee_addr_pubkey(&self) -> &'static str;
    fn dex_fee_addr_raw_pubkey(&self) -> &'static [u8];
    fn dex_fee_z_addr(&self) -> &'static str;
    fn dex_fee_pubkey_ed25519(&self) -> &'static str;

    // Fee rates.
    fn dex_fee_rate(&self) -> BigRational;
    fn fee_discount_tickers(&self) -> &'static [&'static str];
    fn dex_fee_rate_discounted(&self) -> BigRational;
    fn dex_fee_min_threshold(&self) -> BigRational;

    // Burn (post-baseline addition — covered in chapters 08 and 16).
    fn burn_enabled(&self) -> bool { false }
    fn dex_fee_share(&self) -> BigRational { BigRational::from_integer(1.into()) }
    fn burn_addr_pubkey(&self) -> &'static str { "" }
    fn burn_addr_raw_pubkey(&self) -> &'static [u8] { &[] }

    // Seed nodes.
    fn seed_nodes(&self) -> &'static [&'static str];
}
```

The trait collects every per-network constant that the baseline
held in scattered hard-coded form. Its return-type discipline is:

- Numeric identity returns are plain values (`u16`).
- String returns are `&'static str` (the data is interned into
  the binary at compile time).
- Byte-slice returns are `&'static [u8]` (raw pubkey bytes
  decoded from the hex string at the same compile time).
- Rate returns are `num_rational::BigRational` so the fee math
  never loses precision.
- The burn-related methods have default implementations that
  produce the "burn-disabled" answer, so a network that does not
  burn does not need to spell those out.

### 6.4 The registry and the startup guard

`lib.rs` exposes two registry functions:

```rust
pub fn net_config_for(netid: u16) -> Option<&'static dyn NetConfig> { … }
pub fn net_config_or_panic(netid: u16) -> &'static dyn NetConfig { … }
```

`net_config_for` performs a `match netid { 8762 => …, 6133 => …, … _ => None }`.
The two baseline-style production netids — `8762` and `6133` —
are always present; the four regtest netids — `8100`, `8999`,
`9000`, `9998` — are gated behind `#[cfg(feature = "regtest-netid")]`.
The numbers themselves are on-the-wire values that any node on
the network must already recognise, and are exposed by the public
`net_config_for` registry; they are part of the
public-protocol surface of the crate.

`net_config_or_panic` is the daemon-startup convenience: it
returns the trait object or panics with a descriptive message
listing every supported netid. The "deny except config exists"
policy is implemented by the daemon calling this function from
its initialisation path; on an unrecognised netid the daemon
refuses to start.

The trait object is `&'static dyn NetConfig` — a fat pointer to
a zero-sized unit struct living in static storage. There is no
heap allocation, no synchronisation cost, no shutdown ordering
issue. The cost of looking up a network's parameters at runtime
is one match arm and one indirect call.

### 6.5 What `mm2_libp2p` loses

The post-baseline peer-to-peer crate (renamed `mm2_p2p` — the
broader rename is treated in a later chapter) drops every netid-
shaped artefact:

- `NETID_7777` and `ALL_NETID_7777_SEEDNODES` are gone from
  `src/network.rs`. The file ceases to be a per-network seed-node
  registry.
- `get_all_network_seednodes(netid)` is removed. The seed-bootstrap
  loop inside `start_gossipsub` is removed with it.
- `spawn_gossipsub` and `start_gossipsub` lose their `netid: u16`
  first parameter; the function's other parameters
  (`force_key`, `spawn_fn`, `to_dial`, `node_type`, `on_poll`)
  retain their baseline meaning and order.
- The `AtomicDexBehaviour` struct loses its `netid` field.
- The floodsub construction inside `start_gossipsub`, which at
  the baseline read
  `Floodsub::new(local_peer_id, netid != NETID_7777)`, becomes
  `Floodsub::new(local_peer_id, true)` — the baseline's
  non-`7777` branch wins unconditionally.
- The `to_dial: Vec<RelayAddress>` parameter now carries the
  seed-node list that the caller has resolved through
  `mm2_net_config`.

After these removals the peer-to-peer crate has no notion of
"network identity" at all. It accepts a list of relay addresses
to dial on startup and a node-type descriptor; it does not care
where those came from or which network number they belong to.

### 6.6 The new startup path in `lp_native_dex.rs`

`init_p2p` in `mm2_main/src/lp_native_dex.rs` is shortened to:

```rust
async fn init_p2p(ctx: MmArc) -> P2PResult<()> {
    let i_am_seed = ctx.conf["i_am_seed"].as_bool().unwrap_or(false);
    let seednodes = seednodes(&ctx)?;
    // …force_p2p_key, node_type, metrics callback…
    let spawn_result = spawn_gossipsub(
        force_p2p_key, spawn_boxed,
        seednodes, node_type, move |swarm| { /* metrics */ },
    ).await;
    // …
}
```

The supporting `seednodes(ctx)` helper consults `ctx.conf["seednodes"]`
first (allowing an operator-supplied list to win); otherwise it
falls back to `default_seednodes(ctx.netid())`, which in turn
consults `mm2_net_config::net_config_for(netid)?.seed_nodes()`
and produces either a list of `RelayAddress::Dns` values
(under WASM, where DNS resolution is delegated to the browser)
or a list of `RelayAddress::IPv4` values resolved through
`addr_to_ipv4_string` (under native targets). Both branches
return an empty list if `net_config_for(netid)` returns `None`,
but in practice this cannot happen: startup validation has
already rejected unknown netids by the time `seednodes` runs.

### 6.7 Why this is a decoupling, not a removal

The change is best read as "extract a per-network registry,
delete the scattered hard-coded copies, and make the peer-to-
peer crate netid-blind". No on-the-wire behaviour is altered:

- The JSON configuration shape (`"netid": u16`,
  `"seednodes": [string]`, `"i_am_seed": bool`) is unchanged.
- The peer-to-peer protocol does not encode the netid — peers
  on different netids would in any case be unable to participate
  in the same gossipsub mesh because their seed-node sets do
  not intersect.
- The seed-node DNS names registered for production networks
  match the names that were operational at baseline time; the
  baseline's `defimania.live` placeholder names were specific
  to the netid `7777` network and have no counterpart in the
  post-baseline registry, which covers different networks.
- The floodsub behaviour switch was preserved in its
  more-permissive form; this is observable to peers only
  insofar as a daemon on the post-baseline binary will
  fan-out floodsub subscriptions where a baseline daemon on
  the old `NETID_7777` network would not have.

### 6.8 Reproducing the modernisation from the baseline

A reader can reproduce the modernisation step by step:

1. Add a new workspace member `mm2src/mm2_net_config/` to the
   top-level `Cargo.toml`. Its `Cargo.toml` declares
   `num-rational` as its only non-trivial dependency and exposes
   a single Cargo feature, `regtest-netid`, that gates the test
   netid registrations.
2. Create `mm2src/mm2_net_config/src/lib.rs` containing the
   `NetConfig` trait as in §6.3 and the `net_config_for` /
   `net_config_or_panic` registry as in §6.4.
3. For every netid the binary is supposed to support, create a
   `src/netid_XXXX.rs` module containing one unit struct
   (`pub struct NetidXXXX;`) and one `impl NetConfig for NetidXXXX`
   block that hard-codes the per-network constants. Register the
   struct inside the `match` in `net_config_for`.
4. Gate any test-only netids behind `#[cfg(feature = "regtest-netid")]`
   and collect them in a `src/test_netids.rs` module.
5. In `mm2src/mm2_main/src/lp_native_dex.rs`, replace the body
   of `default_seednodes(netid)` with a call to
   `net_config_for(netid)?.seed_nodes()`, mapping each DNS
   string to a `RelayAddress::Dns` (WASM) or to a
   `RelayAddress::IPv4` resolved through `addr_to_ipv4_string`
   (native).
6. Remove the now-dead `NETID_7777_SEEDNODES` constant from
   `lp_native_dex.rs`.
7. In `mm2src/mm2_libp2p/src/network.rs`, delete the
   `NETID_7777` constant, the `ALL_NETID_7777_SEEDNODES` slice,
   and the `get_all_network_seednodes` function. The file
   reduces to declarations of `RelayAddress` helpers (or, after
   the broader rename, vanishes when its contents move into
   neighbouring modules).
8. In `mm2src/mm2_libp2p/src/atomicdex_behaviour.rs`:
   - Remove the `netid: u16` field from `AtomicDexBehaviour`.
   - Drop the `netid: u16` parameter from `spawn_gossipsub` and
     `start_gossipsub`.
   - Replace the `Floodsub::new(local_peer_id, netid != NETID_7777)`
     call with `Floodsub::new(local_peer_id, true)`.
   - Delete the `for (peer_id, addr) in get_all_network_seednodes(netid) { … }`
     loop. (The remaining code already uses the caller-supplied
     `to_dial` list for bootstrap, so the loop's effect is
     subsumed.)
9. Update every caller of `spawn_gossipsub` — in `lp_native_dex.rs`
   and in the peer-to-peer crate's own tests — to drop the
   `netid` argument.
10. Wire the startup-time `net_config_or_panic(ctx.netid())` call
    into the daemon's initialisation, before the peer-to-peer
    subsystem is brought up, so that unknown netids cause a clean
    panic with a list of supported netids.

After steps 1–10, every per-network constant flows from
`mm2_net_config`, the peer-to-peer crate has no netid, and the
binary refuses to start on an unrecognised netid.

## External References

- *libp2p — Gossipsub specification v1.1.*
  https://github.com/libp2p/specs/blob/master/pubsub/gossipsub/gossipsub-v1.1.md
- *libp2p — Floodsub specification.*
  https://github.com/libp2p/specs/tree/master/pubsub
- *crates.io — `num-rational`.* https://crates.io/crates/num-rational
- *The Cargo Book — Features.*
  https://doc.rust-lang.org/cargo/reference/features.html
- *The Rust Reference — Conditional compilation (`cfg`).*
  https://doc.rust-lang.org/reference/conditional-compilation.html

## Provenance Footer

*This chapter v1; verified directly against the baseline tree at
commit `c1d46c0c1592faa0860f704008b2b2381bc3840f` and the current
tree on 2026-05-31. Reviewer #1 and reviewer #2 reports stored at
`local/clean-room-doc/reviews/06-network-id-seed-node-r{1,2}.md`.*
