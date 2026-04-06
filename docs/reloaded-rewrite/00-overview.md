# KDF-Reloaded — How This Project Came To Be

This document is the public, reader-facing record of how the present codebase
was produced. It is the entry point to a multi-chapter narrative under this
directory.

## The question this document answers

> *Given the upstream Komodo DeFi Framework codebase as it existed at commit
> `c1d46c0c1592faa0860f704008b2b2381bc3840f` (6 June 2022, the last commit
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
4. Chapter [29-provenance-attribution.md](29-provenance-attribution.md)
   is a cross-cutting index: it tabulates, for each major artefact in
   the present tree, the specification, sibling project, or independent
   contribution it descends from.

## Stability of this document

This document set is treated as part of the codebase. It is maintained
on the same branch as the code it describes. When a chapter is added or
revised, the revision is recorded in the commit history of this
repository alongside any code change it accompanies.

If a chapter becomes inconsistent with the code it describes, that is a
bug in the document set, not in the code. Such inconsistencies should be
reported as issues against this directory.
