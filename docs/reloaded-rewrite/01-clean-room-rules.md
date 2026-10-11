# Chapter 01 — Clean-Room Rules and Methodology

**Status:** legal-position.

The chapter binds the rules every other chapter in the document set
claims to follow: the pinned baseline anchor, the seven
permitted-input classes, the three forbidden-input classes, the
two-team procedure by which forbidden-class input is taken, cleared
and recorded, the identifier-hygiene rule, the citation discipline,
the canonical chapter shape, the missing-derivation policy, and the
rule-stability and source-of-truth discipline.

## 1.1 Executive Summary

This chapter is the *authoritative methodology* for the document set
and for source changes explicitly developed as clean-room-derived
work after the chapter-02-anchored baseline tree. Components
intentionally imported or adapted from publicly available,
license-compatible sources are governed by provenance recording and
must not be represented as clean-room-derived. A chapter or a source
change that contradicts the rules below is a bug to be tracked and
remediated against the offending artefact, not against this chapter.

The methodology is structured as six normative groups: an *input*
group (permitted-input classes R1–R7; forbidden-input classes R8–R10);
a *procedure* group (the two-team clean-room procedure R37–R44, which
is how the forbidden-input classes are honoured in the one case where
only forbidden-class material can answer a question); an *expression*
group (identifier-hygiene rule R11; citation discipline R12–R15); a
*shape* group (per-chapter status and section layout R16–R20); a
*process* group (missing-derivation discipline R21; source-and-chapter
consistency R22–R24; rule-stability R25–R26; the provenance-ledger
duty R27); and a *component-provenance* group (the bounded reuse
categories, the residual-similarity gate, and the binding force of
chapter code blocks, R28–R36).

The legal-position status applies because the chapter records a
methodology statement rather than a substrate-binding driving
specification. The chapter is binding on the rest of the document
set notwithstanding the legal-position status.

## 1.2 Subsystem Shape

The methodology applies to two artefact classes:

| Artefact class                    | Binding scope                                                          |
| --------------------------------- | ---------------------------------------------------------------------- |
| Documentation chapters (this directory) | Every chapter under the document directory beyond chapter 02.       |
| Source-tree commits               | Commits explicitly developed and documented as clean-room-derived post-baseline work. |

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

**R8.** *Forbidden input for clean-room-derived work:* source
produced under the relicensing of the historical record the baseline
tree descends from, after the chapter-02-anchored baseline date. Any
code, comment, documentation, commit message, or request-discussion
thread from that corpus is a forbidden input when claiming
clean-room derivation.

**R9.** *Derived analyses of R8 material.* Summaries, paraphrases,
ports, transliterations, or reconstructions of R8 material are
equivalent to the material itself and are forbidden.

**R10.** *Private channels.* Material obtained from non-public
channels (private chats, internal documents, leaked archives, etc.),
regardless of its license status, is forbidden.

## 1.4A Bound Two-Team Clean-Room Procedure

R8–R10 bind *what* must not enter the document set, and R11 binds
what must not be quoted. None of them binds *how* a question that
only forbidden-class material can answer is answered without
breaching them. This section binds that procedure.

The procedure is the *means* by which R8–R11 are satisfied in that
case, never a relaxation of them. No rule in this section permits any
content R8–R11 forbid, no rule in this section widens a
permitted-input class, and R26 continues to bar reading any of them
that way.

Throughout this section, *forbidden-class material* is material of
the three classes R8–R10, and *taking forbidden-class input* is
reading, searching, listing, diffing, or otherwise inspecting such
material.

The section's rules are numbered R37–R44 and its number is §1.4A
because R1–R36 and §1.5–§1.11 were already bound and cited from
other chapters when the procedure was codified; renumbering them
would break those citations. The section is placed next to the
forbidden-input classes it serves rather than at the end of the
chapter where its rule numbers would fall.

**R37.** *Bound role separation.* The procedure binds exactly three
separated roles:

| Role                            | Bound mandate                                                                                                                                          | Bound prohibition                                                                                                                                     |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| Dirty-side specification author | Takes forbidden-class input and writes clean-channel chapter text — behavioural requirements, public and dictated interface surface, and nothing else. | MUST NOT write or edit source code; MUST NOT carry forbidden-class expression into a chapter, a report, a verdict, or an instruction to another role. |
| Clean-room gate                 | Compares a candidate chapter against the forbidden-class material and returns a sanitized verdict (R39).                                               | MUST NOT write or edit source code; MUST NOT author or edit the candidate it clears; MUST NOT emit forbidden-class expression, nor the candidate text it flags. |
| Clean-side implementer          | Reads a cleared chapter and this repository, and writes the source that satisfies the chapter.                                                         | MUST NOT take forbidden-class input by any route, including an excerpt relayed by another role, a generated artefact, a cache, or a human.            |

The dirty-side specification author is the only
implementation-analysis role permitted to take forbidden-class input,
and it takes that input for a single purpose: to establish facts that
are themselves of the permitted-input classes — the published
specification and node interfaces of R3, the counterparty-dictated
inter-operability shapes of R4, and the externally observable
behaviour of R6 — and to restate those facts as this document set's
own prose. Expression is not carried across; it is left where it was
read.

The clean-room gate also takes forbidden-class input, because a
comparison requires both sides. It is not an implementation-analysis
role: it authors nothing, specifies nothing, writes no source, and
emits only the output R39 bounds. These two roles are exhaustive — no
other role, context, or contributor takes forbidden-class input at
all.

**R38.** *Bound mandatory route.* Where a chapter's content cannot be
established from the permitted-input classes R1–R7 alone and
forbidden-class input is taken in order to establish it, that input
MUST be taken only by the R37 dirty-side specification author, and
the resulting chapter text MUST be cleared by the R39 gate before
R40 lifts. A context acting in neither of the two R37 roles that may
take forbidden-class input MUST NOT take it at all.

A self-certification is not a substitute for the route. An assertion
by an arbitrary context that it looked but took only behaviour is not
checkable by any reader of the document set, carries no evidence that
R11 was honoured, and therefore does not discharge the citation duty
of R12. The procedure is checkable precisely because the role that
took the input is not the role that cleared what came out.

**R39.** *Bound sanitized verdict.* The gate's output MUST consist of
the following parts and nothing else, each part separately labelled:

1. a *leak report* — the pass-or-fail outcome; a
   discretionary-similarity measure taken on R35's
   discretionary-expression basis; and, on failure, a list whose
   entries name a candidate line location, a leak category, and at
   most one remedy class of R39's closed vocabulary, and carry nothing
   else;
2. a *scope attestation*, which every verdict MUST carry;
3. optionally, a separately labelled *non-leak section*.

The three parts are bound in detail by *The bound parts of the R39
verdict* below. The gate MUST NOT quote, paraphrase, transcribe, or
otherwise reproduce forbidden-class material in any part of its
output, and MUST NOT quote the candidate text it flags.

That limit is load-bearing rather than hygienic. The verdict crosses
to the clean side; a verdict carrying the expression it objected to
would breach the wall in the act of policing it and would contaminate
every context that read it. A gate that cannot describe a finding
without reproducing it MUST describe it by category alone.

What the gate verifies is the substance of R11 and R35: whether each
candidate line carries clean-channel content — public and dictated
interface surface, abstract behaviour, lifecycle and ordering
semantics — or discretionary expression. The judgement is made per
identifier and not by aggregate match density, because a
dictated-interop chapter is *required* to match on most of its
identifiers and that matching carries no inference of copying (R35).

**R40.** *Bound clearance precondition.* A candidate produced under
R38 is not usable until the R39 gate has cleared it. Until then it
MUST NOT be relied on as a statement of required behaviour, MUST NOT
be implemented from, and MUST NOT be committed as content satisfying
this chapter's standard. A chapter or subsection written into the
tree while its clearance is pending MUST say so at the point at
which it appears, so that neither a reader nor an implementer
mistakes a candidate for a cleared requirement.

A failed clearance is remediated by scrubbing the lines the verdict
named and re-clearing the result. Rewording a flagged line is not
remediation: R9 makes a paraphrase of forbidden-class material
equivalent to the material itself.

**R41.** *Bound non-combination.* The three roles of R37 MUST NOT be
combined in one context. Specifically:

- a context that has taken forbidden-class input MUST NOT act as the
  clean-side implementer, and MUST NOT write or edit source code
  presented as clean-room-derived;
- the gate MUST be a context other than any context that authored the
  candidate it clears, and other than any context that commissioned or
  specified that candidate, because none of those can independently
  verify a boundary it drew or asked for;
- an orchestrating context that has taken forbidden-class input MAY
  direct the work, but MUST NOT pass forbidden-class material into an
  instruction to the clean-side implementer, and MUST NOT substitute
  its own text for a cleared chapter's.

The gate-independence requirement reaches the commissioning party and
not the drafting party alone, because the boundary under examination is
set as much by the instruction as by the drafting. A context that
specified what the candidate should contain has the same interest in
its clearance as the context that composed it; and where the
commissioning context has itself taken forbidden-class input, what it
specified may carry the very expression the gate exists to detect.
Independence from the drafting party alone would leave that case
unbound, which is why independence is required over all three
relations — authored, commissioned, specified.

Separation is a property of the context, not of an intention. A
context cannot un-take input it has already taken, so each
prohibition above attaches at the moment the input is taken and does
not lapse within that context's lifetime.

**R42.** *Bound breach discipline.* Forbidden-class material that
reaches a chapter, a report, an instruction, or the source tree other
than through R38–R40 is a clean-room breach. A suspected breach is
handled as a breach until an investigation establishes otherwise. The
discipline MUST be exactly:

1. commit and push activity in this repository halts — every
   in-flight change, not only the one that triggered the breach;
2. the material is removed rather than reworded (R9);
3. every affected chapter is held under R21 and MUST NOT be committed
   in its present state;
4. the breach is reported promptly to the maintainer;
5. each affected artefact is re-established under R38–R40, or
   re-derived from the permitted-input classes, before commit
   activity resumes;
6. every context that read the material is treated as having taken
   forbidden-class input, and R41 attaches to it.

The document set records the provenance of its content, not an
incident narrative: R20's provenance footer states a chapter's inputs
and says nothing about the handling of a breach. The report of item 4
is the record of the incident.

**R43.** *Bound temporal scope.* R37–R44 bind every chapter revision
committed on or after the commit that adopts them. A chapter revision
committed earlier is not defective merely for carrying no record of
the procedure. Three consequences follow, and no others:

- the content standard of R8–R11 bound such a revision when it was
  written and binds it still; a chapter in which forbidden-class
  material is actually present is held under R21 whatever its date;
- where such a chapter is revised in a way that takes fresh
  forbidden-class input, that fresh input is subject to R38–R40;
- a pre-adoption chapter whose record is silent about the route by
  which a fact was established is not thereby required to be
  re-opened; recording such a route retrospectively is the deferred
  work of D3.

This is a statement of the rules' temporal scope and not a
retroactive relaxation barred by R26: no pre-revision rule of this
chapter bound the procedure, so R43 permits no chapter that an
earlier rule forbade.

**R44.** *Bound operational elaboration.* The repository's own role
definitions under `.github/agents/` — `kdf-spec-reader.agent.md`,
`kdf-dirty-gate.agent.md` and `coder.agent.md` — together with the
two-team section of the repository's contributor instructions
(`AGENTS.md` §2) are the *operational elaboration* of R37–R43: they
fix dispatch mechanics, the wording of each role's own output
contract, the concrete verdict format, and the host location at which
the forbidden-class material happens to sit. The elaboration is
subordinate to this chapter:

- it MAY add operational detail;
- it MUST NOT weaken, broaden, or substitute for R37–R43, and MUST
  NOT authorise any input R8–R10 forbid;
- where the elaboration and this chapter disagree, this chapter
  governs and the elaboration is the defect of record;
- it is not itself subject to the rule-stability discipline of
  R25–R26, which is the reason the normative content of the procedure
  is bound here rather than there.

The file paths and role names quoted in this rule are this
repository's own artefacts, cited as evidence under R11's
legal-position evidentiary-citation carve-out; chapter 01 — this
chapter — is the legal-position chapter on which that citation
depends.

### The bound parts of the R39 verdict

**What R39 takes from R35.** The measure of the leak report is taken
on R35's basis and not by R35's literal test. R35's test is stated for
a file of source: it compares body lines after stripping comments and
normalising whitespace, and passes a file whose discretionary-body
similarity is materially below its whole-file figure. A chapter is
prose and has no bodies to compare, so that test does not map onto it.
What R39 borrows is the part of R35 that is general: the measured set
excludes dictated-interop and pinned-interface content, which is
required to match and whose matching supports no inference of copying,
and admits discretionary expression only. R35's own test continues to
govern a file of source measured for clean-room sufficiency, unchanged
by this section.

**The remedy-class vocabulary.** A finding is more useful when it says
what kind of repair would answer it, and a gate permitted to say
nothing beyond a location and a category cannot answer the question it
is most often asked. The gate MAY therefore attach at most one remedy
class to a finding, or one to the candidate as a whole, drawn from
exactly this vocabulary:

| Remedy class       | Bound meaning                                                                                                                                                                                                |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `remove`           | The flagged expression is deleted and nothing is substituted for it.                                                                                                                                          |
| `re-establish`     | The requirement the flagged line carried is established afresh from the permitted-input classes and stated as behaviour, the flagged line having been removed first — rewording it is not remediation (R40).  |
| `embed-as-interop` | The content is required for inter-operability and belongs in the chapter as a marked reuse fragment under R29–R31, with the citation those rules require, rather than as prose.                                |
| `name-carve-out`   | The flagged identifier may stand only if the chapter names the R11 carve-out class that admits it.                                                                                                             |
| `hold`             | The finding is not remediable within the candidate; the chapter is held under R21.                                                                                                                            |
| `none`             | The gate offers no remedy class.                                                                                                                                                                              |

The vocabulary is closed: a remedy class outside it MUST NOT be used,
and free prose about a finding MUST NOT be offered in place of one. A
class names a route and never a repair text — the gate MUST NOT supply
replacement wording, because a gate that drafts the passage it would
accept has authored the candidate it clears, which R37 and R41 forbid.
A class is advisory: it does not bind the author, who remains bound by
the leak report and by R40.

**The scope attestation.** A verdict that states an outcome and
nothing else would replace an unverifiable author certification with
an unverifiable gate certification, and R38's case against
self-certification would then apply to the gate in turn. Every verdict
MUST therefore carry a scope attestation of exactly three elements:

- *extent* — which part of the forbidden-class material was compared,
  identified by the subject matter this document set names in its own
  terms (a subsystem, a protocol surface, a chapter's scope) and by
  reference version, together with a quantity of material examined
  where one can be given without locating expression (a count of files
  compared, for instance);
- *method* — how the comparison was made: by reading, by a mechanical
  comparison, by a named tool of this repository, or a stated
  combination of those;
- *measure* — the resulting discretionary-similarity measure, which is
  the same figure the leak report carries and not a second one,
  together with the threshold or judgement standard against which it
  was read.

The attestation is a statement about the comparison and never about
what the material says. It MUST NOT name a path, a file, a module, an
identifier, or any other location within the forbidden-class material;
MUST NOT characterise that material's content beyond the leak
categories of the leak report; and MUST stay at three elements rather
than growing into a narrative. Its reference-version element is
subject to R11 on the terms §1.7 states for R20's. Where an element
cannot be given without breaching this paragraph, the attestation says
so, which leaves the gap visible instead of papered over.

The attestation is bound because it is what makes a clearance
checkable rather than merely asserted. A reader who is told the
extent, the method and the measure can tell a comparison of the right
scope from one of the wrong scope; a reader given a bare outcome
cannot, and is back to taking a certification on trust — the defect
R38 identifies in self-certification, relocated one role along.

**The non-leak section.** The leak report's limit exists to keep
expression from crossing, not to put the gate under a general vow of
silence. A gate MAY add a separately labelled non-leak section,
bounded as follows:

- its subject MUST be this document set's own text and rules, the
  procedure of this section, or the candidate's conformance with the
  shape rules of §1.7 — never the content of the forbidden-class
  material;
- a statement that cannot be made without reference to what the
  forbidden-class material says belongs in the leak report, by
  category, or nowhere;
- it MUST NOT supply chapter text, for the reason a remedy class MUST
  NOT (R37, R41);
- it is advisory: nothing in it binds the author.

This is what lets a gate answer a question about the rules that bind
it — including a question about this section — without touching the
limit that makes its verdict safe to read. The labelling is what keeps
the two apart: a reader must be able to see which statements carry the
strict limit of the leak report and which do not.

### Why the procedure is required and not merely recognised

R38 makes the procedure the only permitted route rather than one
sufficient means among several. The alternative — recognising the
procedure as sufficient while leaving a direct, self-certified
consultation open beside it — is rejected on three grounds.

First, a self-certification is not checkable. A reader of the
document set has no way to distinguish a context that took only
R3 / R4 / R6 facts from one that took expression and restated it,
and R9 makes the second indistinguishable from clean work in its
output alone — it becomes distinguishable only when the comparison
against the material is actually performed. Second, R12 obliges every
chapter to cite the input that produced a behaviour; an input no
second party ever examined is a citation to a closed record. Third,
the one measurement this chapter already binds — the
residual-similarity gate of R35 — cannot be performed by a context
that may not see both sides, so the comparison must be delegated to a
role that may, and R39 is what makes that delegation safe.

The third ground is worth stating at full strength, because it is
less an argument for the procedure than an observation about the
regime the procedure joins. R35's gate scores a file against the
relicensed historical record, so it needs both operands: the file,
and material of the R8 class. R8–R10 withhold the second operand
from every contributor and from every implementation role this
chapter otherwise contemplates. R35's gate has therefore never been
executable by any role this chapter permitted — not because the rule
is wrong, but because the chapter named no role that could hold both
operands and bound nothing about what such a role might emit if it
did. R39 is the first rule that does both. On this one point §1.4A is
not an addition to the regime: it supplies the only role that can
execute a gate this chapter has bound since R35 was written, and the
only output contract under which that gate's result can cross back
without carrying what it measured.

That observation has a limit worth recording. R35 is conditional — it
governs a measurement when one is taken, and its mechanical
thresholds and tooling are deferred to chapter 30 D1 — so its
unexecutability was never an unmet standing obligation, and no
chapter is retrospectively defective for having been committed
without a score. The gap was in executability, not in compliance.

One thing still follows for R38, and it is not a point about who
decides to measure. Because R35 triggers conditionally, the party
being measured already holds whatever discretion the trigger leaves;
requiring the procedure neither creates that discretion nor removes
it, so an argument resting on it proves less than it sounds. The
argument that survives is about the measurement itself. Whenever a
measurement is taken, exactly one role may lawfully hold both operands
(R37), and exactly one output contract can carry the result back to a
context that may not hold the second (R39). An optional route would
therefore not be one lawful way of measuring among several: in every
case where it was not used, the measurement could not lawfully have
occurred at all. R38 makes the route mandatory for that reason, and
for the unverifiability of a self-certification.

The conditionality concession has to be read next to what the document
set actually says, because several chapters put it under pressure. The
provenance footers of chapters 20, 21, 23, 24, 32 and 53 each declare
that any residual similarity of a conformant realisation to the
historical lineage *is governed by* the R35 gate; chapter 34 records
the shipped code's residual similarity as deferred to that gate and
states that the gate enforces the position going forward. That is a
standing-governance form rather than a conditional one: read plainly,
six chapters and the provenance ledger declare themselves governed by
a measurement that, until §1.4A, no role this chapter permitted could
lawfully take. The tension is recorded rather than argued away. Those
declarations are honest as to *content* — R8–R11 bound each of those
chapters when it was written and bind it still, and R43 makes none of
them defective for carrying no score — but as to *governance* they
named a gate that was unperformable, and §1.4A is what turns them into
declarations about something performable. What remains outstanding is
the measurement itself, which still waits on chapter 30 D1 for its
thresholds and tooling; D4 carries that forward.

Requiring the route costs the project nothing it was relying on. R43
scopes the requirement forward, so no already-committed chapter
becomes defective by its adoption; and because the dirty-side role is
already the only role permitted to take the input at all (R37),
requiring clearance of what that role produces narrows no
permitted-input class and closes no question the document set can
otherwise answer.

### Why the normative content is bound here

R44 binds the content of the procedure in this chapter and leaves the
mechanics to the role definitions, rather than citing those
definitions as the authority for the procedure. The document set is
the project's derivation record, and a rule the record relies on must
sit inside the regime R25 makes announceable and R26 makes
un-relaxable. An operational instruction file is not in that regime —
it can be rewritten without a rule-change announcement and without
the re-checking of past chapters R25 requires — so a chapter drawing
its authority from one would make the derivation record depend on a
document the record cannot hold still.

Honesty requires the converse to be stated as well. The procedure as
actually practised *is* bounded in part by the role definitions: the
precise output contract each role works to, the exact shape of the
verdict, and the dispatch mechanics are theirs and are not restated
here. R44 therefore names them rather than ignoring them, and directs
them to conform. The citation is deliberately inward — this chapter
cites the elaboration as subordinate detail, and the elaboration
cites this chapter as its authority — so that the dependency runs
from the process document to the specification and not the other way.

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
| Legal-position evidentiary citation      | The chapter is a legal-position chapter and the identifier (a baseline file path, a baseline-existing constant name, a present-tree symbol cited as evidence of a position the chapter binds) is quoted as evidence rather than as substrate. The chapter MUST name itself as the legal-position chapter on which the citation depends. |
| Project-internal source-comment marker   | The identifier is a project-internal mnemonic notation that appears verbatim in the source tree as a comment marker (a deferral-tracking tag, a TODO-style identifier, a workplan-cross-reference token). The chapter MUST cite the source-tree location at which the marker appears and MUST NOT treat the marker itself as substrate. |

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

Meta-chapters (chapter 00, this chapter, chapter 30, chapter 34) MAY
adjust the substrate-binding sections to bind methodology or index
discipline rather than substrate contract surface; the rules-tests-
deferred-verifications shape applies otherwise unchanged.

**R20.** The *Provenance Footer* of every chapter MUST be a short
bulleted block recording exactly the following four bound items:

- *Inputs:* the permitted-input classes the chapter consumed,
  enumerated per chapter;
- *Permitted-input classes used:* the R-numbered classes from §1.3 the
  chapter consumed (per-chapter enumeration);
- *Sibling-allowlist consultations:* the sibling-repository citations
  the chapter relied on per R5 / R14, or *none* if the chapter relied
  on no sibling repositories;
- *Forbidden corpus:* a statement about the chapter's *inputs*, in
  one of the two bound forms of the following sub-section — the
  blanket trailer `not consulted`, or a scoped record. Where neither
  form can honestly be written, the chapter MUST NOT be committed in
  its present state and the missing-derivation discipline of R21
  applies.

### The *Forbidden corpus* item of R20

The item is a statement about the chapter's *inputs*, not about how
any fact in it was discovered. It asserts one thing only: that the
chapter contains no R8–R10 material — no forbidden-class expression,
and no identifier outside R11's carve-out classes. Two bound forms
make that assertion, and a chapter carries exactly one of them:

1. the *blanket trailer* `not consulted`, which reads exactly so. A
   chapter MAY add clarifying prose after the trailer, but MUST NOT
   vary the trailer itself;
2. a *scoped record* stating what, if anything, was taken from
   forbidden-class material, for which sections, and under which
   reference versions. A scoped record MAY be framed positively
   ("consulted only for …") or as a qualified negative ("not
   consulted for …, except …"). It asserts the same thing about the
   chapter's content as the blanket trailer does; it adds the route
   and withholds nothing.

The reference-version element of a scoped record is itself subject to
R11, because a record precise enough to locate expression is a leak
surface however briefly it is written. A publicly published release
designation of the historical lineage — a release tag or version
string its own public releases carry — is a public-release fact of the
kind R6 admits, is what the project's compatibility policy already
binds as the reference contract, and MAY be named. A post-anchor
commit identifier, a branch state, or a location within
forbidden-class material MUST NOT be named: paired with a section
scope it points at the expression the record exists to keep out, and
no carve-out class of R11 admits that pairing. Where a record cannot
be made more precise without such an identifier, it names the public
release designation or the reference epoch and says that this is as
precise as the record goes.

That constraint binds a scoped record written on or after the commit
that adopts it. R26 permits a tightening to reach backwards but not to
reach silently, so the existing records were re-checked against it as
R25 requires: one pre-adoption derivation record — chapter 39's
shielded-coin subsection — names post-anchor commit identifiers of
exactly the kind this paragraph now withholds, paired with the
subsection they scope, and no other record in the document set does.
Such an identifier is a locator and not expression, so its presence
breaches neither R8–R11 nor that chapter's content standard; what the
constraint removes is a pointer the record does not need. Bringing
that one record into line is D5.

Either form is satisfied when the chapter's content rests on the
permitted-input classes of §1.3 — including R3's published
specification and node interfaces, R4's dictated inter-operability
shapes, and R6's externally observable behaviour. R8 forbids
forbidden-class material *as input to a clean-room claim*; chapter
34's provenance model states that scoping. Establishing an
R3 / R4 / R6 fact does not become an R8 derivation by reason of the
route through which the fact was noticed, provided that nothing
outside those classes entered the chapter and that the route was the
procedure of §1.4A wherever that procedure applies (R38, R43).

Where §1.4A applies, the scoped record is also how the assertion is
*verified* rather than merely claimed: the record names the route,
and the gate clearance of R39–R40 is the verification. A chapter MAY
carry that detail alongside a blanket trailer instead, in this item
or in a separate footer item; doing so adds route information and
neither weakens nor contradicts the assertion.

The item records inputs and the procedure under which they were
taken. It MUST NOT identify the person, the session, or the
authoring context behind any passage. Naming a clean-room role of
R37 is procedure rather than author attribution, and is permitted.

Where the assertion cannot honestly be made in either form —
forbidden-class expression is present, or an identifier fails R11 —
the chapter MUST NOT be committed in its present state and the
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

**R27.** Components intentionally imported or adapted from public,
license-compatible sources MUST be recorded in a repository-level
provenance ledger. Each record MUST identify destination path,
introducing commit in this repository, source repository/commit (when
known), and license basis. Such components MUST NOT be described as
clean-room-derived in user-facing documentation.

## 1.11 Bound Component Provenance Categories

Most components in this repository are produced by clean-room derivation
under Sections 1.3–1.5. This section names the additional, narrowly-bound
categories under which verbatim reuse of upstream or third-party content is
permitted *within an otherwise clean-room-governed file*, and the rules
that govern each. Restricted to that scope, the five categories are
mutually exhaustive: a fragment falling under this section (and each such
fragment of a file) belongs to exactly one of them. **Clean-room** is the
default category and is the only one that does not require an explicit
category marker in CRD chapters; the four reuse categories defined below
MUST be explicitly marked at the point of reuse.

This section's five-way split does not by itself cover every file in the
repository. Content that is the baseline itself, or that predates the
baseline, is governed by the permitted-input classes of §1.3 (R1, R2), not
by this section. Post-anchor content that is a derivative of GPLv2
baseline or upstream-lineage code, and whole third-party projects imported
wholesale rather than reused as a fragment, are additional provenance
categories with their own license basis, outside this section's scope.
The repository-level provenance ledger (chapter 34) tracks all of these
alongside the categories of this section; chapter 34 §34.1 is the
complete, ledger-level provenance taxonomy this section's five categories
are a bounded part of.

### Category: Generated artifact

**R28.** A file or fragment classified as *Generated artifact* is the
deterministic output of a code generator (a `build.rs` step, a protobuf
compiler, a `bindgen` run, or equivalent) executed over project-controlled
input. The generator and its input MUST be checked into this repository or
pinned to a specific external version recorded in the CRD chapter. A
Generated-artifact file is reproduced byte-identically by re-running the
generator over the recorded input; any divergence from the generator's
output is a defect.

### Category: Interop / wire-format reuse

**R29.** A file or fragment classified as *Interop / wire-format reuse*
exists because functional identity with an external counterparty is
required — a deployed smart contract, a protobuf wire format consumed by
third-party peers, an on-disk schema read by another version of the
application, or the public payload shape of an external service. For such
content, expression and function are merged: differing bytes break
interoperability.

**R30.** When the authoritative source for an Interop / wire-format
artifact is a third-party project, the CRD chapter MUST cite that source
by URL, version (or commit), and license. The blind implementer MAY fetch
the verbatim content from the cited source.

**R31.** When the only available source for an Interop / wire-format
artifact is the relicensed historical record (the corpus forbidden under
R8), the CRD chapter MUST embed the verbatim bytes required for
interoperability into the chapter itself. Short inclusions (approximately
twenty lines or fewer) appear inline in the relevant chapter; longer
inclusions appear in an appendix chapter referenced from the relevant
chapter. The embedding MUST observe the following sanitization discipline:

- Upstream comments MUST NOT be copied. If commentary is needed in the
  embedded content, it is authored fresh in the CRD.
- Only the bytes strictly necessary for interoperability are embedded.
  Unrelated declarations, dead branches, and non-load-bearing formatting
  are dropped.
- Internal identifiers that do not affect external identity (private
  variable names, private constant names, struct field order where the
  serialized form is order-insensitive) MAY be renamed; identifiers that
  ARE part of the external interface (protobuf field numbers, JSON key
  names, ABI signatures, exported symbol names) MUST be preserved exactly.

This procedure permits the blind implementer to operate from the CRD alone
without consulting the forbidden corpus.

### Category: Convergent idiomatic shape

**R32.** A file or fragment classified as *Convergent idiomatic shape* is
short, follows a canonical pattern (either a project-wide pattern
documented in `AGENTS.md` or the standard Rust idiom for a specific
construct), and any competent implementer would arrive at substantially the
same expression. The CRD chapter MUST cite the canonical pattern source.
Convergent idiomatic content does not require behavioural specification
beyond naming the pattern.

### Category: Third-party-API-bound shape

**R33.** A file or fragment classified as *Third-party-API-bound shape*
exists as a thin wrapper whose shape is dictated by an external crate's
API surface or by a public protocol specification not maintained by this
project. The CRD chapter MUST cite the external crate or specification by
name and version. Variants, methods, and field names that mirror the
cited API are preserved as named by the cited API.

### File-level classification

**R34.** A file's overall category is the least-permissive category of any
fragment it contains. A file containing even a single clean-room fragment
is itself classified as clean-room; the file passes through the clean-room
procedure, while fragments individually classified as Generated artifact,
Interop / wire-format reuse, Convergent idiomatic shape, or
Third-party-API-bound shape are marked in the CRD chapter at the point of
their reuse, with the source citation required by the relevant rule above,
and the blind implementer treats those fragments as direct CRD input.

### Residual-similarity gate

**R35.** When a file's textual similarity to the relicensed historical
record is measured to assess clean-room sufficiency, the measurement MUST be
taken over *discretionary expression only*. Lines belonging to fragments
classified under R28–R33 (Generated artifact, Interop / wire-format reuse,
Convergent idiomatic shape, Third-party-API-bound shape), and lines of
interface surface the chapter pins by necessity — public type, enum, struct,
trait, and function signatures; serde wire renames; on-disk schema and table
or column names; `#[repr]` discriminant values; pinned constant values; and
import lists — are EXCLUDED from the measured set. Those lines are required to
match, so their matching carries no inference of copying. Consequently a
whole-file similarity figure is NOT a valid clean-room gate for
interface-dense files and MUST NOT be used as one. The gate is the
*discretionary-body* similarity: function bodies, private helper
decomposition, control flow, local naming, and comment / diagnostic wording,
compared after stripping comments and normalizing whitespace. A file passes
when its discretionary-body similarity is materially below its whole-file
similarity AND each residual identical body line is individually attributable
to an R28–R33 fragment or a pinned-interface line. Mechanical thresholds and
the automated tool that applies this gate are deferred to chapter 30 (D1).

### Binding force of chapter code blocks

**R36.** A code block in a substantive chapter is binding only as to the
content that carries function or external identity: public type, enum,
struct, trait, and function signatures; serde wire renames; on-disk schema
and SQL text; protocol path tokens, header names, and constant values;
`#[repr]` discriminants; control-flow branches whose outcome is observable;
and the set (not the wording) of diagnostics. Everything else a code block
happens to show -- private field and local-variable names, private helper
decomposition, statement ordering that does not change observable output,
and comment or log-message wording -- is INFORMATIVE: it records one in-tree
realization, not a mandate. A blind implementer MAY satisfy the binding
contract with different private names, decomposition, or wording, and such
divergence is conformant, not a defect. Where a chapter's prose says a
structure "MUST have exactly" certain fields or bodies, that phrasing binds
the functional and interface content enumerated above; it does not extend
this project's clean-room obligation to the discretionary expression shown
alongside it, and a chapter author MUST NOT rely on such phrasing to import
verbatim authorial expression from the forbidden corpus (R8) into a chapter.
This rule is the interpretive companion to R35, which governs measurement:
R36 fixes what a chapter's code blocks oblige, R35 fixes how residual
similarity to that obligation is scored.

## 1.12 Stylistic Commitments

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

## 1.13 Tests

Most of this chapter binds methodology and has no test surface of its
own; the testing discipline those rules bind is carried per chapter on
the substantive chapters' *Tests* sections (R19). The procedure of
§1.4A is the exception: it is checkable against the document set
itself, so the three documentary checks below are bound here. Each is
a check over the repository's own files, performed by inspection until
the tooling of D1 exists.

**T1.** *Clearance record.* For every chapter whose provenance footer
records forbidden-class input taken under R38 on or after the adoption
of §1.4A, a clearance statement exists: either the footer records that
the gate cleared the consulted sections, or each consulted section
carries the pending-clearance statement R40 requires. A chapter that
records the input and carries neither statement fails T1. A statement
recording a completed clearance also carries, in corpus-free terms,
the three elements of the R39 scope attestation — the extent compared,
the method, and the measure — or states that those elements are not
recorded in the document set; a statement reporting only that a gate
passed, with neither the attestation nor that note, fails T1.

**T2.** *Verdict sanitation.* No clearance statement, derivation
record, or relayed verdict anywhere in the document set quotes the
candidate text it concerns or any forbidden-class material. A record
that identifies a finding by anything beyond its location, its
category, and at most one remedy class of R39's closed vocabulary
fails T2; a remedy class accompanied by replacement wording or by
prose describing the flagged content fails T2 on the same rule. A
separately labelled non-leak statement permitted by R39, and a scope
attestation, do not fail T2 provided each quotes neither the candidate
nor forbidden-class material and neither names a location within that
material (R39).

**T3.** *Attribution hygiene.* No R20 *Forbidden corpus* item, no
derivation record, and no clearance statement identifies the person,
the session, or the authoring context behind a passage. Naming a
clean-room role of R37 is permitted and does not fail T3 (R20).

## 1.14 Deferred Work

**D1.** Automated linting of R11 (identifier hygiene), R13–R15
(citation discipline), R16 / R19 / R20 (per-chapter shape), the
§1.4A documentary checks T1–T3, and the R35 residual-similarity gate
(mechanical thresholds and the tool that applies it) is deferred to
chapter 30 D1; chapter 30 binds the audit-tooling-gap.

**D2.** A bound machine-readable manifest of the seven permitted-input
classes (R1–R7) and the three forbidden-input classes (R8–R10)
suitable for cross-checking commit-message and chapter-citation
metadata against is deferred.

**D3.** Retrospectively recording the §1.4A route for chapter
revisions committed before R37–R44 were adopted — the chapters whose
provenance footers record forbidden-class input without naming the
role that took it or the clearance it passed — is deferred. R43 does
not require it: those revisions were bound by the content standard of
R8–R11, which this chapter leaves unchanged, and R26 bars curing a
record gap by relaxing a rule. The work is therefore a record
improvement rather than a remediation, and is taken chapter by
chapter as each chapter is next revised.

**D4.** Taking the R35 measurement for the chapters that declare their
realisation governed by that gate — chapters 20, 21, 23, 24, 32 and
53, together with the shipped-code position chapter 34 records — is
deferred. §1.4A now supplies the one role that may hold both operands
and the output contract under which a result may cross back, so the
gate has become performable; chapter 30 D1 still owes the mechanical
thresholds and the tool that applies them. Until that tooling exists,
each such declaration is forward-looking governance and not a record
that a measurement was taken, and no chapter may be read as asserting
a measure it does not carry. When the tooling lands, each declaration is
either backed by a measurement taken through §1.4A and recorded with
the R39 scope attestation, or restated in the conditional form R35
actually binds. This item is the record of the tension §1.4A's
"Why the procedure is required" discussion states, and it is a record
improvement for the same reason D3 is: R43 makes no affected chapter
defective, and R26 bars curing a record gap by relaxing a rule.

**D5.** Bringing the one pre-adoption scoped record that names
post-anchor commit identifiers — the derivation record of chapter 39's
shielded-coin subsection — into line with the reference-version
constraint of §1.7 is deferred to that chapter's next revision. The
identifiers are locators rather than expression, so the chapter's
content standard under R8–R11 is unaffected and R21 does not attach;
the revision replaces the pointer with the public release designation
and the reference epoch, which is all the record needs. The R25
re-check that accompanied the constraint found no other instance in
the document set.

## 1.15 Baseline Verifications

This chapter makes no claims of the form *X already existed in the
baseline*. The baseline anchor itself is bound by chapter 02; every
chapter that consumes the baseline cites it per R15. The section is
present per R19 with no V-numbered entries.

## 1.16 External References

This chapter cites no external-specification, wire-format, or
sibling-allowlist material directly. The chapter's substrate is the
project's own methodology. The classes of external citation the rest
of the document set is required to honour are bound by R3 (named
specification families), R4 (counterparty-defined inter-operability
shapes), and R5 (sibling-allowlist repositories under compatible
licenses).

## 1.17 Provenance Footer

- *Inputs:* the baseline workspace at the pinned baseline-revision
  commit of chapter 02; chapter 00 (the document-set framing and the
  per-chapter shape claim R5 of chapter 00 this chapter elaborates);
  chapter 02 (the pinned baseline anchor every R-rule in §1.3 refers
  to); chapter 29 (the second of the two legal-position chapters,
  paired with this one under R18); chapter 30 (the audit-tooling-gap
  this chapter's D1 hands off to); chapter 34 (the repository-level
  provenance ledger that operationalises the R27–R36 categories of
  §1.10–§1.11 and that this chapter's R19 now names as a fourth
  meta-chapter, and whose §34.1 taxonomy §1.11's five-category split
  is a bounded part of). For §1.4A and the R20 *Forbidden corpus*
  item: this repository's own role definitions under `.github/agents/`
  and the two-team section of its contributor instructions
  (`AGENTS.md` §2), read as the existing operational practice R37–R44
  now bind and R44 subordinates; and the provenance footers of the
  document set as they already stand, read to establish which bound
  forms of the R20 item are in use. For the R35-gate tension recorded
  in §1.4A and in D4: chapters 20, 21, 23, 24, 32 and 53 (the
  provenance footers and binding-scope notes that declare a conformant
  realisation's residual similarity governed by the R35 gate) and
  chapter 34 (the ledger-level form of the same declaration). For the
  R25 re-check behind the §1.7 reference-version constraint and D5:
  the document set's scoped records and derivation records as they
  stand, read for the reference identifiers they name.
- *Permitted-input classes used:* the document set as it stands; this
  repository's own instruction and role-definition files, cited as
  evidence under R11's legal-position evidentiary-citation carve-out
  (chapter 01 being the legal-position chapter that citation depends
  on).
- *Sibling-allowlist consultations:* none.
- *Forbidden corpus:* not consulted. The forbidden-class material is
  this chapter's subject matter, not an input to it: §1.4A, the R20
  *Forbidden corpus* item, and the R39 verdict contract are bound from
  this document set's own text and from this repository's own role
  definitions and contributor instructions, which R44 subordinates to
  this chapter. Because no forbidden-class input was taken, R38
  required no clearance for this revision and the footer accordingly
  records no scope attestation (T1).
