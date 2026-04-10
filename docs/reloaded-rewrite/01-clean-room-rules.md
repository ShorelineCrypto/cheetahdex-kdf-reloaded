# Chapter 01 — Clean-Room Rules and Methodology

**Status:** legal-position.

The chapter binds the rules every other chapter in the document set
claims to follow: the pinned baseline anchor, the seven
permitted-input classes, the three forbidden-input classes, the
identifier-hygiene rule, the citation discipline, the canonical
chapter shape, the missing-derivation policy, and the rule-stability
and source-of-truth discipline.

## 1.1 Executive Summary

This chapter is the *authoritative methodology* for the document set
and for every source change recorded in the commit history that
postdates the chapter-02-anchored baseline tree. A chapter or a
source change that contradicts the rules below is a bug to be
tracked and remediated against the offending artefact, not against
this chapter.

The methodology is structured as four normative groups: an *input*
group (permitted-input classes R1–R7; forbidden-input classes R8–R10);
an *expression* group (identifier-hygiene rule R11; citation
discipline R12–R15); a *shape* group (per-chapter status and section
layout R16–R20); a *process* group (missing-derivation discipline R21;
source-and-chapter consistency R22–R24; rule-stability R25–R26).

The legal-position status applies because the chapter records a
methodology statement rather than a substrate-binding driving
specification. The chapter is binding on the rest of the document
set notwithstanding the legal-position status.

## 1.2 Subsystem Shape

The methodology applies to two artefact classes:

| Artefact class                    | Binding scope                                                          |
| --------------------------------- | ---------------------------------------------------------------------- |
| Documentation chapters (this directory) | Every chapter under the document directory beyond chapter 02.       |
| Source-tree commits               | Every commit in the project history that postdates the baseline anchor of chapter 02. |

The baseline anchor itself, the inherited tree at the baseline
commit, and any artefact that predates the baseline date under a
license compatible with the project's relicensing intent are *not*
constrained by this methodology — those are the inputs the
methodology operates on, bound by chapter 02.

## 1.3 Bound Permitted-Input Classes

The seven permitted-input classes below are exhaustive: a chapter's
or commit's permitted inputs MUST be sourced from these classes and
no others.

**R1.** *The baseline itself.* The full source tree at the
chapter-02-anchored baseline commit, under its original license, is
a permitted input. This covers source code, comments, documentation,
and tests in the baseline tree.

**R2.** *Material that predates the baseline.* Any artefact (a prior
commit in the historical record the baseline tree descends from
included) that existed on or before the chapter-02-anchored baseline
date AND remained under a license compatible with the project's
relicensing intent is a permitted input.

**R3.** *External, public specifications.* The chapter-bound
specification classes are exactly:

| Class                                                | Examples (non-exhaustive)                                                                                |
| ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Bitcoin Improvement Proposals                        | BIP-16, BIP-32, BIP-39, BIP-43, BIP-44, BIP-65, BIP-49, BIP-84.                                          |
| Satoshi Labs Improvement Proposals                   | SLIP-0010, SLIP-0021, SLIP-0044.                                                                          |
| Ethereum Improvement Proposals                       | EIP-191, EIP-712, EIP-2612.                                                                              |
| Inter-Blockchain Communication standards             | ICS-20 fungible-token transfer; the cross-chain hash-time-locked-contract dialects.                       |
| libp2p protocol family                               | gossipsub, request-response, identify, kademlia, the noise transport.                                     |
| WalletConnect protocol documents                     | The WalletConnect v2 relay and session protocols.                                                         |
| Lightning Network specifications                     | The BOLT series.                                                                                          |
| Published smart-contract Application Binary Interfaces | Public-ledger ERC-20 / ERC-721 / ERC-1155 surfaces; per-contract project ABIs published by their authors. |
| Published blockchain-node request-and-response interfaces | Public JSON-based and request-and-response interfaces of mainnet-running node implementations.       |

**R4.** *Wire formats and external interfaces the project must
inter-operate with.* When a third party (a counterparty node, a
blockchain node, a hardware-wallet device, a browser extension, a
wallet) defines a message shape that the project MUST produce or
consume to inter-operate, that shape is a permitted input. Such
shapes are dictated by the inter-operability requirement and do not
constitute derivation from any particular implementation.

**R5.** *Sibling open-source repositories under compatible licenses.*
A piece of code or a type definition that appears in another publicly-
available repository operated by the same or a related organisation
under a license compatible with the project's intent (GPL version 2,
LGPL, MIT, BSD, Apache 2.0, or the chapter-bound sibling-allowlist
equivalent) MAY be referenced as an example of how the organisation
publicly expressed the concept *before* the chapter-02-anchored
baseline. Citation MUST satisfy R14.

**R6.** *Behavioural observation of public networks.* Observable
behaviour of the live peer-to-peer mesh, of public blockchains, and
of any other publicly-reachable endpoint is a permitted input. This
covers traffic shapes, message frequencies, response codes, error
arms, and any other fact that any external observer could collect
without privileged access.

**R7.** *Independent work.* Designs and code produced without
reference to forbidden inputs (R8–R10) are permitted.

## 1.4 Bound Forbidden-Input Classes

The three forbidden-input classes below are exhaustive: every
chapter and every commit MUST be free of derivation from these
classes.

**R8.** *Source produced under the relicensing of the historical
record the baseline tree descends from, after the chapter-02-anchored
baseline date.* Any code,
comment, documentation, commit message, or request-discussion thread
produced after the chapter-02-anchored baseline date under the
relicensed terms is a forbidden input.

**R9.** *Derived analyses of R8 material.* Summaries, paraphrases,
ports, transliterations, or reconstructions of R8 material are
equivalent to the material itself and are forbidden.

**R10.** *Private channels.* Material obtained from non-public
channels (private chats, internal documents, leaked archives, etc.),
regardless of its license status, is forbidden.

## 1.5 Bound Identifier-Hygiene Rule

**R11.** Identifier names are expression. An identifier MAY be
quoted verbatim in the document set only if it satisfies at least
one of the following chapter-bound carve-out classes:

| Carve-out class                          | Bound condition                                                                                       |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Externally-visible inter-operability surface | The identifier is part of an interface the project MUST inter-operate with (a request-method name, a payload field name, a peer-to-peer topic or protocol name, a protobuf field name, a contract Application Binary Interface symbol, a header name, etc.). |
| Published-crate consumer-facing name     | The identifier is the published name of a crate whose name is itself the consumer-facing contract.    |
| External-specification symbol            | The identifier is the name of an external specification, or of a symbol defined by such a specification (a Bitcoin Improvement Proposal number, a Satoshi Labs Improvement Proposal-defined string, an Ethereum Improvement Proposal-defined constant, a gossipsub control-message name, etc.). |
| Baseline-existing identifier             | The identifier existed in the baseline tree on or before the chapter-02-anchored baseline commit.     |
| Sibling-allowlist identifier             | The identifier appears in a sibling open-source repository under a compatible license per R5, and that appearance predates the baseline. |
| Chapter-bound substrate identifier       | The identifier is bound by the chapter itself as substrate contract surface (the substrate's chapter is the bounding authority for the identifier's expression). |

Identifiers that do not satisfy any carve-out class — internal module
names, internal struct names, internal field names, internal function
names not part of a public consumer-facing surface — MUST be described
in the document set by their *behaviour* rather than by name.

The identifier-hygiene rule constrains identifier *quotation in the
document set*; it does NOT constrain the identifier names actually
used in the source tree. Source-tree identifier names are part of the
source and are governed by ordinary software-engineering taste.

## 1.6 Bound Citation Discipline

**R12.** Every chapter MUST cite its inputs. A chapter that cannot
cite the input that produced a given behaviour is incomplete; the
missing citation is a defect against the chapter.

**R13.** *External-specification citations.* External specifications
MUST be cited by document identifier (BIP-32, SLIP-21, EIP-712,
ICS-20, etc.) and, where the specification carries a version, by
version.

**R14.** *Sibling-repository citations.* Sibling open-source
repositories MUST be cited by repository identifier and by the
commit identifier of the earliest commit in the repository
containing the referenced material. Where the earliest-commit
identifier is unknown at the time of citation, the citation MUST
flag the gap explicitly rather than be silently omitted.

**R15.** *Wire-format and baseline citations.* Wire formats that an
external counterparty defines MUST be cited by pointing to the
counterparty's publicly-available definition. The baseline is cited
as *the baseline* in chapter prose, with the underlying anchor
itself bound by chapter 02.

## 1.7 Bound Per-Chapter Shape

**R16.** Every chapter under the document directory MUST declare its
status on the third line in the canonical form:

```
**Status:** driving-spec.
```

or, for the two chapter-bound legal-methodology chapters (chapter 01
and chapter 29):

```
**Status:** legal-position.
```

These two values are exhaustive in the published document set.
Internal working statuses (drafting, under review, blocked) are
tracked outside the document directory.

**R17.** *driving-spec* chapters MUST read as forward-looking design
briefs: a competent implementer reading only the chapter, the
chapter-02-anchored baseline tree, and the external inputs the
chapter cites MUST plausibly arrive at the corresponding part of the
present source tree.

**R18.** *legal-position* chapters MUST record legal-methodology
statements (this chapter's rules; chapter 29's treatment of license
conditions) rather than technical-substrate designs.

**R19.** Every substantive chapter MUST contain the canonical
section sequence: a one-paragraph claim under the status line; an
*Executive Summary*; a *Subsystem Shape* or substrate-shape section;
the substrate-binding sections containing the chapter's R-numbered
rules; a *Tests* section containing the chapter's T-numbered tests;
a *Deferred Work* section containing the chapter's D-numbered
deferrals; a *Baseline Verifications* section containing the
chapter's V-numbered baseline-state verifications (omitted entirely
if the chapter binds no such claims); an *External References*
section listing the chapter's cited external-specification, wire-
format, and sibling-allowlist citations; a bulleted *Provenance
Footer* (R20).

Meta-chapters (chapter 00, this chapter, chapter 30) MAY adjust the
substrate-binding sections to bind methodology or index discipline
rather than substrate contract surface; the rules-tests-deferred-
verifications shape applies otherwise unchanged.

**R20.** The *Provenance Footer* of every chapter MUST be a short
bulleted block recording exactly the following four bound items:

- *Inputs:* the permitted-input classes the chapter consumed,
  enumerated per chapter;
- *Permitted-input classes used:* the R-numbered classes from §1.3 the
  chapter consumed (per-chapter enumeration);
- *Sibling-allowlist consultations:* the sibling-repository citations
  the chapter relied on per R5 / R14, or *none* if the chapter relied
  on no sibling repositories;
- *Forbidden corpus:* the mandatory trailer `not consulted`. The
  trailer reads exactly so; if it cannot honestly be written, the
  chapter MUST NOT be committed in its present state and the
  missing-derivation discipline of R21 applies.

## 1.8 Bound Missing-Derivation Discipline

**R21.** When a chapter author finds that the current source
contains a behaviour for which no clean derivation from the
permitted-input classes (R1–R7) can be constructed, the chapter MUST
NOT be committed in a state that papers over the gap. The discipline
MUST be exactly:

1. the chapter is held;
2. the discrepancy is recorded as an issue against the source code,
   not against the document set;
3. the chapter is committed only after the discrepancy is resolved,
   either by identifying the permitted input the author had missed,
   or by altering the source so a clean derivation exists.

The document set MUST NOT paper over such discrepancies.

## 1.9 Bound Source-and-Chapter Consistency

**R22.** The document set is part of the source tree. It lives on
the same branches, is committed under the same review process, and
is subject to the same code-of-conduct and contribution rules as the
source.

**R23.** When a chapter and a piece of source disagree on behaviour,
the resolution discipline MUST be:

| Diagnosis                                  | Resolution                                                                                       |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| Source correct, chapter incorrect.         | Chapter updated to match source.                                                                 |
| Chapter correct, source incorrect.         | Source fixed under ordinary review.                                                              |
| Both internally consistent but represent different intents. | Resolved by discussion in the issue tracker before either is changed.                       |

**R24.** Where a chapter becomes inconsistent with the source it
describes, the chapter is the defect of record and is reported as
an issue against the document directory; this is the converse of R23
and applies during routine maintenance when the source has moved
ahead and chapters have not yet caught up.

## 1.10 Bound Rule-Stability Discipline

**R25.** A rule change in this chapter MUST be announced in the
commit history with a clear explanation of what was changed and
why. Past chapters MUST be re-checked against the revised rules;
chapters found inconsistent MUST be revised.

**R26.** A rule MUST NOT be relaxed retroactively to permit a
chapter that violated it under the pre-revision rules. A rule MAY
be tightened retroactively to require revision of chapters that the
older rule would have permitted.

## 1.11 Stylistic Commitments

The principles in this section are not procedural rules but
stylistic commitments. They exist so that future chapter authors can
extend the document set in a consistent voice.

- *Think first, then write.* Each chapter is the product of explicit
  design work, not of stream-of-consciousness drafting. Before any
  prose is committed, the author has formed a mental model of what
  is being described and what the reader needs to take away.
- *Simplicity over completeness.* A chapter that explains one thing
  clearly is preferable to a chapter that explains five things
  partially. When a chapter would grow beyond what a reader can hold
  in mind, it is split.
- *Smallest sufficient delta.* When a chapter describes a change
  from the baseline tree, it describes that change and nothing else.
  Unrelated behaviour is left to the chapter that owns it.
- *Goal-driven structure.* Every chapter answers a specific question
  about a specific substrate. Chapters whose purpose cannot be
  stated in one sentence are not yet ready to be written.

## 1.12 Tests

This chapter binds methodology; it has no test surface of its own.
The testing discipline the rules bind is carried per chapter on the
substantive chapters' *Tests* sections (R19).

## 1.13 Deferred Work

**D1.** Automated linting of R11 (identifier hygiene), R13–R15
(citation discipline), and R16 / R19 / R20 (per-chapter shape) is
deferred to chapter 30 D1; chapter 30 binds the audit-tooling-gap.

**D2.** A bound machine-readable manifest of the seven permitted-input
classes (R1–R7) and the three forbidden-input classes (R8–R10)
suitable for cross-checking commit-message and chapter-citation
metadata against is deferred.

## 1.14 Baseline Verifications

This chapter makes no claims of the form *X already existed in the
baseline*. The baseline anchor itself is bound by chapter 02; every
chapter that consumes the baseline cites it per R15. The section is
present per R19 with no V-numbered entries.

## 1.15 External References

This chapter cites no external-specification, wire-format, or
sibling-allowlist material directly. The chapter's substrate is the
project's own methodology. The classes of external citation the rest
of the document set is required to honour are bound by R3 (named
specification families), R4 (counterparty-defined inter-operability
shapes), and R5 (sibling-allowlist repositories under compatible
licenses).

## 1.16 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of chapter 02; chapter 00 (the document-set framing and the
  per-chapter shape claim R5 of chapter 00 this chapter elaborates);
  chapter 02 (the pinned baseline anchor every R-rule in §1.3 refers
  to); chapter 29 (the second of the two legal-position chapters,
  paired with this one under R18); chapter 30 (the audit-tooling-gap
  this chapter's D1 hands off to).
- *Permitted-input classes used:* the document set as it stands.
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* not consulted.
