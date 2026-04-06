# Clean-Room Rules and Methodology

This document defines the methodology this project claims to have
followed when producing the post-baseline portion of the KDF-Reloaded
source tree. The rules below apply to every chapter in this directory
and to every code change recorded in the commit history that postdates
the baseline.

If a chapter, a commit message, or a piece of source code is found to
contradict these rules, that finding is a bug to be tracked and
remediated.

## 1. The baseline

The baseline is exactly one Git commit:

> `c1d46c0c1592faa0860f704008b2b2381bc3840f` (3 June 2022)

Everything present in that commit's working tree, under the license
attached to that tree at the time, is the inherited starting material.

A short time after the baseline, the upstream project relicensed its
codebase under GPL version 3. No material produced upstream after the
baseline has been incorporated into this project.

## 2. Permitted inputs

The following materials are permitted inputs for any post-baseline
change to this project:

1. **The baseline itself.** The full source tree at commit
   `c1d46c0c1592faa0860f704008b2b2381bc3840f`, under its original
   license. This includes source code, comments, documentation, and
   tests.

2. **Material that predates the baseline.** Any artefact (including
   prior commits in the upstream history) that existed on or before
   the baseline date and remained under a license compatible with the
   project's relicensing intent.

3. **External, public specifications.** Examples include but are not
   limited to:
   - Bitcoin Improvement Proposals (BIPs)
   - SatoshiLabs Improvement Proposals (SLIPs)
   - Ethereum Improvement Proposals (EIPs)
   - Inter-Blockchain Communication (IBC) standards
   - The libp2p protocol family, including gossipsub
   - WalletConnect protocol documents
   - Lightning Network BOLTs
   - Smart-contract ABIs published on public blockchains
   - JSON-RPC interfaces published by blockchain node implementations

4. **Wire formats and external APIs the project must inter-operate
   with.** When a third party (a wallet, a counterparty node, a
   blockchain node, a hardware-wallet device, a browser extension)
   defines a message shape that this project must produce or consume,
   that shape is a permitted input — it is dictated by the
   inter-operability requirement, not by any particular implementation.

5. **Sibling open-source projects under compatible licenses.** When a
   piece of code or a type definition appears in another publicly
   available repository operated by the same or a related organisation
   under a license compatible with this project's intent (for example
   GPL version 2, LGPL, MIT, BSD, or Apache 2.0), it may be referenced
   as an example of how that organisation publicly expressed the
   concept before the baseline.

6. **Behavioural observation of public networks.** Observable
   behaviour of the live peer-to-peer mesh, of public blockchains, and
   of any other publicly-reachable endpoint is permitted input. This
   includes traffic shapes, message frequencies, error responses, and
   any other fact that any external observer could collect.

7. **The author's own independent work.** Designs and code produced
   without reference to forbidden inputs (clause 3) are permitted.

## 3. Forbidden inputs

The following are not permitted inputs:

1. **Post-baseline upstream source.** Any code, comment, documentation,
   commit message, or pull-request discussion produced by the upstream
   project after the baseline commit, under the relicensed terms.

2. **Derived analyses of post-baseline upstream source.** Summaries,
   paraphrases, ports, transliterations, or reconstructions of
   post-baseline upstream material are equivalent to the material
   itself and are forbidden.

3. **Private channels.** Material obtained from non-public channels
   (private chats, internal documents, leaked archives, etc.),
   regardless of its license status.

## 4. Identifier hygiene

This document set, and the post-baseline code it describes, treats
identifier names as expression and applies the following rule:

- An identifier may be quoted verbatim in this document set only if it
  satisfies at least one of the following conditions:
  - it is part of an externally-visible interface this project must
    inter-operate with (a JSON-RPC method name, a JSON payload field
    name, a peer-to-peer topic or protocol name, a protobuf field
    name, a contract ABI symbol, a header name, etc.);
  - it is the name of a published Rust crate whose name is itself the
    consumer-facing contract;
  - it is the name of an external specification or of a symbol defined
    by such a specification (a BIP number, a SLIP-defined string, an
    EIP-defined constant, a gossipsub control-message name, etc.);
  - it existed in the baseline tree on or before the baseline commit;
  - it appears in a sibling open-source repository under a compatible
    license (per §2.5 above), and that appearance predates the
    baseline.

- Identifiers that do not satisfy any of the above (internal module
  names, internal struct names, internal field names, internal
  function names not part of a public Rust API surface that downstream
  users depend on) are described in this document set by their
  behaviour rather than by name.

This rule applies to identifier *quotation*. It does not constrain the
identifier names actually used in the source tree; those are part of
the source and are governed by ordinary software-engineering taste.

## 5. Citation discipline

Every chapter cites its inputs.

- External specifications are cited by document identifier (BIP-32,
  SLIP-21, EIP-712, IBC-20, etc.) and, where relevant, version.
- Wire formats that an external counterparty defines are cited by
  pointing to the counterparty's publicly-available definition.
- Sibling open-source repositories are cited by repository URL and
  by the commit hash of the earliest commit in that repository
  containing the referenced material.
- The baseline is cited as "the baseline" (commit
  `c1d46c0c1592faa0860f704008b2b2381bc3840f`).

A chapter that cannot cite the input that produced a given behaviour
is incomplete; the missing citation is a bug in the chapter.

## 6. Chapter shape

Every chapter under this directory follows the same outline:

1. **Executive Summary.** Three to ten short paragraphs in plain
   language. A non-engineer reader can finish this section knowing
   what the chapter is about and why the work was done.

2. **Reproduction Detail.** As long as necessary. Walks through the
   design decisions in enough technical depth that an engineer with
   the baseline, the cited inputs, and reasonable Rust competence
   could rebuild the feature to behavioural equivalence.

3. **External References.** A flat list of the external
   specifications, wire formats, and sibling repositories the chapter
   cites. Each reference is sufficient to locate the cited material
   without further hints.

4. **Provenance Footer.** A one-paragraph note recording the chapter's
   version and the materials it was checked against. (Not a changelog
   — a single statement of the chapter's current state.)

## 7. Principles the document set tries to embody

The following are not procedural rules but stylistic commitments. They
exist so that future contributors can extend the document set in a
consistent voice.

- **Think first, then write.** Each chapter is the product of explicit
  design work, not of stream-of-consciousness drafting. Before any
  prose is committed, the author has formed a mental model of what is
  being described and what the public reader needs to take away.

- **Simplicity over completeness.** A chapter that explains one thing
  clearly is preferable to a chapter that explains five things
  partially. When a chapter would grow beyond what a reader can hold
  in mind, it is split.

- **Describe the smallest sufficient delta.** When a chapter describes
  a change from the baseline, it describes that change and nothing
  else. Unrelated behaviour is left to the chapter that owns it.

- **Goal-driven structure.** Every chapter answers a specific question
  about a specific area of functionality. Chapters whose purpose
  cannot be stated in one sentence are not yet ready to be written.

## 8. What to do when a chapter cannot be written

It can happen that, while preparing a chapter, the author finds that
the current source contains a behaviour for which no clean derivation
from the permitted inputs (§2) can be constructed. When that happens:

- The chapter is not committed in an incomplete state.
- The discrepancy is recorded as an issue against the source code,
  not against this document set.
- The chapter is held until the discrepancy is resolved, either by
  identifying the public input the author had missed, or by altering
  the source so that a clean derivation exists.

The document set will not paper over such discrepancies.

## 9. Relationship to the source tree

This document set is part of the source tree. It lives on the same
branches, is committed under the same review process, and is subject
to the same code-of-conduct and contribution rules as the source.

When a chapter and a piece of source disagree about behaviour:

- If the source is right and the chapter is wrong, the chapter is
  updated.
- If the chapter is right and the source is wrong, the source is
  fixed under ordinary review.
- If both are internally consistent but represent different intents,
  the disagreement is resolved by discussion in the issue tracker
  before either is changed.

## 10. Stability of these rules

This document is the authoritative statement of the methodology. If
the rules change, the change is announced in the commit history with
a clear explanation of what was changed and why. Past chapters are
re-checked against revised rules; chapters found inconsistent are
revised.

A rule cannot be relaxed retroactively to permit a chapter that
violates it. A rule may be tightened retroactively to require
revision of chapters that the older rule would have allowed.
