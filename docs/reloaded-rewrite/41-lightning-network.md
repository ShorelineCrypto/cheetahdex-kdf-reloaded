# Chapter 41 -- Lightning Network

**Status:** driving-spec (as-built; documents shipped reloaded behaviour).
**Native target only.**

> **One-sentence claim:** the project shall support Bitcoin Lightning as a
> native-only Layer-2 coin built on a BOLT-conformant Lightning implementation,
> activating a node through a long-running task RPC, managing peers and channels,
> issuing and paying BOLT-11 invoices, and participating in atomic swaps whose
> on-chain HTLC timelock semantics align with the swap's lock-duration contract.

> **Treatment:** **T-DOC.** Reloaded ships a Lightning coin (`LightningCoin`)
> with a complete node/channel/payment RPC surface, gated to the native target.
> This chapter documents that shipped capability.

## 41.0 Executive Summary

Lightning is integrated as a Layer-2 coin layered on a parent UTXO coin (BTC-
family). A node is brought up by a task-based activation, after which the wallet
can connect to peers, open and close channels, and route BOLT-11 payments. The
underlying protocol and node engine (the LDK-style Lightning library and the BOLT
specifications) are externally dictated; this chapter binds the project's public
surface and the swap-relevant timelock contract, not the engine internals.

> **Binding scope (R36).** Requirements bind observable behaviour, the public RPC
> method strings and their request/response field names, and externally
> *dictated* interop: the **BOLT** specifications (BOLT-11 invoices, channel and
> HTLC semantics) and the underlying Lightning library's network behaviour. The
> Lightning library's internal API and version are an implementation dependency
> (§41.6), not a wire contract. Private types and helper structure are
> informative.

## 41.1 Platform guard

R41.1.1 The Lightning coin shall be compiled and exposed on the **native** target
only. It shall be absent from the WASM build.

## 41.2 Node activation (task RPC)

R41.2.1 Lightning node activation shall be a long-running task exposed as the
public RPC quartet `init_lightning` / `init_lightning_status` /
`init_lightning_user_action` / `cancel_init_lightning`.

R41.2.2 A simplified single-call enable path shall also be exposed as the public
RPC `enable_lightning`.

R41.2.3 Activation shall bind the Lightning coin to its parent UTXO coin, load or
create the node's persistent state, and start the background node event loop.

## 41.3 Peer & channel management

R41.3.1 The project shall expose the following public RPCs with the stated roles:
- `connect_to_lightning_node` -- connect to a peer by node-pubkey@host:port;
- `open_channel` -- open a channel to a peer with a requested capacity and
  options (e.g. announced/unannounced, push amount);
- `close_channel` -- cooperatively or force-close a channel;
- `get_channel_details` -- return the details of one channel;
- `list_open_channels_by_filter` -- list open channels matching a filter;
- `list_closed_channels_by_filter` -- list historically-closed channels matching
  a filter;
- `get_claimable_balances` -- report balances claimable from channels (including
  pending close states).

## 41.4 Invoices & payments (R31 dictated by BOLT-11)

R41.4.1 The project shall expose the following public RPCs:
- `generate_invoice` -- create a BOLT-11 invoice for a requested amount and
  description;
- `send_payment` -- pay a BOLT-11 invoice (or a keysend/spontaneous payment to a
  node), returning a payment identifier;
- `get_payment_details` -- return the status/details of one payment;
- `list_payments_by_filter` -- list payments matching a filter.

R41.4.2 Invoice encoding/decoding and payment routing shall conform to the BOLT
specifications; those specifications are the source of truth.

## 41.5 Swap timelock contract (R31 dictated by swap protocol)

R41.5.1 When Lightning participates as a swap leg, the on-chain HTLC timelock
used for that leg shall be derived from the swap's lock-duration contract so that
the Lightning final-CLTV expiry is consistent with the counterpart chain's
HTLC timelock (the maker lock duration governs the final CLTV expiry delta).
This preserves the atomicity guarantee: neither side can claim without revealing
the secret, and refunds become possible only after the agreed timeout.

## 41.6 Engine dependency

R41.6.1 The Lightning engine is an external library dependency pinned to a
specific version; upgrading it is an implementation concern. Requirements in this
chapter bind the project's public behaviour and the BOLT-level wire contract, not
the engine's internal API. The implementation shall track a maintained engine
version.

> **Upstream divergence (informative).** Reloaded exposes the channel/payment
> operations under **flat** method strings (e.g. `open_channel`,
> `send_payment`). Upstream later regrouped several of these under a
> `lightning::` RPC namespace (e.g. `lightning::channels::open_channel`). This is
> a public-interface naming difference, not a behavioural one. **Recommended (if
> aligning to upstream):** introduce the namespaced aliases while keeping the
> flat names for backward compatibility, or document the flat names as the
> reloaded contract. No behavioural change is required.

## 41.7 Acceptance criteria

- Lightning is absent from the WASM build and present on native (R41.1).
- A node activates via `init_lightning` and reaches a ready state (R41.2).
- A channel can be opened to a peer, listed, its details fetched, and closed
  (R41.3).
- A BOLT-11 invoice can be generated and paid; the payment is then listed and its
  details fetched (R41.4).
- A swap leg using Lightning derives its final-CLTV expiry from the swap lock
  duration (R41.5).
