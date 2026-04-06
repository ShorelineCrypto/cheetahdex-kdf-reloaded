# KDF-Reloaded — How This Project Came To Be

This document is the public, reader-facing record of how the present codebase
was produced. It is the entry point to a multi-chapter narrative under this
directory.

## The question this document answers

> *Given the upstream Komodo DeFi Framework codebase as it existed at commit
> `c1d46c0c1592faa0860f704008b2b2381bc3840f` (3 June 2022, the last commit
> made under the GNU General Public License version 2), and given only
> publicly-available materials — protocol specifications, on-chain message
> formats, the live behaviour of the public peer-to-peer mesh, and
> sibling open-source projects under compatible licenses — could a
> competent Rust developer have arrived at the present KDF-Reloaded
> source tree?*

Every chapter in this directory is a piece of the answer. Each chapter
takes one area of functionality, names the publicly-available inputs that
informed it, and walks through the design decisions that produced the
current code. Read together, the chapters form an end-to-end derivation
record.

## The starting point

The starting point is a single Git commit: `c1d46c0c1592faa0860f704008b2b2381bc3840f`.
Everything in that commit's tree is the inherited baseline. Throughout
this document set, "the baseline" refers to that commit.

A short time after that commit, the upstream project relicensed its
codebase under GPL version 3. KDF-Reloaded does not incorporate any
material produced upstream after that relicensing. The detailed rules
under which post-baseline work was added to the project are set out in
[01-clean-room-rules.md](01-clean-room-rules.md).

## What this document *is*

- A **derivation record**. For every area of functionality that differs
  from the baseline, it states what changed, what publicly-available
  material informed the change, and how a reader could reach the same
  outcome.
- **Layered**. Every chapter begins with an Executive Summary readable
  by a non-engineer, followed by Reproduction Detail readable by an
  engineer who wants to verify or rebuild the work.
- **Spec-first**. When a behaviour is dictated by an external
  specification (a BIP, SLIP, EIP, IBC standard, gossipsub protocol
  document, contract ABI, RPC payload shape, etc.), the chapter cites
  that specification by name and version.

## What this document *is not*

- **Not a legal opinion.** Nothing here is intended as legal advice or
  as a legal defence. It is a technical and procedural record of how
  the code was produced.
- **Not a substitute for the source.** The chapters describe behaviour
  and design intent in plain language; they do not reproduce the source
  code itself. To understand the code, read the code.
- **Not an exhaustive feature catalog.** Areas that were not modified
  relative to the baseline are out of scope. The chapters cover only
  the post-baseline delta.
- **Not a changelog.** A changelog answers *what* changed and *when*. A
  derivation record answers *how* the change could have been produced
  from public materials, regardless of when it was committed.

## How to read this set

1. Start with [01-clean-room-rules.md](01-clean-room-rules.md). It
   defines the methodology this document set claims to follow.
2. Read [02-baseline-state.md](02-baseline-state.md). It establishes the
   shape of the inherited code at the baseline commit.
3. Read subsequent chapters in numerical order. Each chapter is
   self-contained and can also be read independently if you are only
   interested in one area of functionality.
4. Chapter [30-provenance-attribution.md](30-provenance-attribution.md)
   is a cross-cutting index: it tabulates, for each major artefact in
   the present tree, the specification, sibling project, or independent
   contribution it descends from.

## Table of contents

- [00 — Overview & Purpose](00-overview.md) *(this document)*
- [01 — Clean-Room Rules and Methodology](01-clean-room-rules.md)
- [02 — The Baseline State at the June 2022 Commit](02-baseline-state.md)
- [03 — Toolchain Modernization](03-toolchain-modernization.md)
- [04 — Error-Aggregation Type Adaptation to the Modern Trait Solver](04-error-aggregation-type-adaptation.md)
- [05 — Hierarchical-Deterministic Wallet Support](05-hd-wallet-support.md)
- [06 — Network-Identifier & Seed-Node Decoupling](06-network-id-seed-node.md)
- [07 — Wallet Lifecycle & Private-Key Export RPCs](07-wallet-lifecycle-and-key-export.md)
- [08 — Atomic-Swap Fee-Routing Engine](08-fee-routing-engine.md)
- [09 — Third-Party Watcher Reward Infrastructure](09-watcher-reward-infrastructure.md)
- [10 — Server-Sent-Events Streaming Backbone](10-sse-streaming.md)
- [11 — Order-Match Cancellation Race Mitigation](11-order-match-cancellation.md)
- [12 — Order-Match State Store](12-order-match-state-store.md)
- [13 — Atomic-Swap Version Negotiation Layer](13-swap-version-negotiation.md)
- [14 — Generic State-Machine Runtime](14-state-machine-runtime.md)
- [15 — Atomic-Swap V2 UTXO Path](15-swap-v2-utxo-path.md)
- [16 — Atomic-Swap V2 Pre-Burn Output](16-swap-v2-pre-burn-output.md)
- [17 — Atomic-Swap V2 EVM Path & Contract Interaction](17-swap-v2-evm-path.md)
- [18 — Tendermint, IBC, and Cross-Chain HTLC Surfaces](18-tendermint-ibc-htlc.md)
- [19 — Non-Fungible-Token Module Layout](19-nft-module-layout.md)
- [20 — Siacoin Network Integration](20-siacoin-integration.md)
- [21 — TRON Network Integration](21-tron-integration.md)
- [22 — WalletConnect v2 Pairing & Session](22-walletconnect-v2.md)
- [23 — External Trading-API Client](23-trading-api-client.md)
- [24 — GUI-Facing Account-State Persistence](24-gui-account-state.md)
- [25 — SQL Query-Builder Replacement](25-sql-query-builder.md)
- [26 — Cross-Platform Build & WASM Adaptation](26-cross-platform-and-wasm.md)
- [27 — Infrastructure-Crate Carve-Outs](27-infrastructure-crate-carve-outs.md)
- [28 — libp2p Modernization](28-libp2p-modernization.md)
- [29 — Treatment of License Conditions (e) and (f)](29-license-conditions-e-f.md)
- [30 — Provenance & Attribution Index](30-provenance-attribution.md)

## Stability of this document

This document set is treated as part of the codebase. It is maintained
on the same branch as the code it describes. When a chapter is added or
revised, the revision is recorded in the commit history of this
repository alongside any code change it accompanies.

If a chapter becomes inconsistent with the code it describes, that is a
bug in the document set, not in the code. Such inconsistencies should be
reported as issues against this directory.
