# Chapter 37 -- UTXO SPV & Block-Header Validation

**Status:** driving-spec (target parity; documents the shipped reloaded SPV
subsystem plus the active chain-reorganization detector now being ported to
upstream/corpus parity -- see §37.7).

> **One-sentence claim:** for UTXO-family coins the project shall, when SPV
> proof is configured, maintain a persistent local store of block headers that
> it syncs forward in bounded chunks, validate those headers against the chain's
> proof-of-work and difficulty-retarget rules, parse each chain's header bytes
> according to a configuration-selected chain variant, and use the verified
> header chain to prove that swap-relevant transactions are confirmed -- on the
> native target backed by SQLite and on the WASM target backed by IndexedDB.

> **Treatment:** **T-PORT (mostly T-DOC).** Reloaded already ships most of the
> SPV / block-header subsystem (an `kdf_spv_validation` crate plus per-coin
> block-header storage backends in the UTXO coin module, a configuration-selected
> chain-variant reader, and proof-of-work / retarget validation); that part is
> documented as-built. One sub-behaviour -- the **active** chain-reorganization
> detect-and-resolve routine -- is reduced relative to upstream/corpus and is now
> a **target** requirement to be ported (§37.7); it is specified here
> behaviourally so it can be implemented and unit-tested.

## 37.0 Executive Summary

SPV (Simplified Payment Verification) lets a UTXO coin confirm transactions
against block-header proof-of-work without trusting a single remote server. The
subsystem has four observable parts:

1. **Activation configuration** -- an optional SPV configuration object supplied
   at coin-activation time (not a core coin field). When absent, the coin runs
   without header verification. On the WASM target the configuration is honoured
   the same way as native.
2. **Header sync loop** -- a per-coin background task that fetches headers from
   the coin's RPC/Electrum backend in bounded chunks, persists them, retries on
   transient failure, and advances a stored tip.
3. **Header storage** -- a persistent store with a total-count query, bounded
   retention (oldest headers beyond a configured limit are pruned), and bulk
   removal up to a height; native uses SQLite, WASM uses IndexedDB.
4. **Header validation** -- proof-of-work target checks, difficulty-retarget
   height computation at the adjustment boundary, and per-chain header byte
   parsing keyed to a configured chain variant.

> **Binding scope (R36).** Requirements bind observable behaviour, the public
> cross-crate/activation interface, and externally *dictated* interop: the
> Bitcoin block-header wire layout and its chain-specific variants (AuxPoW,
> KawPoW, Sapling-root, PoSV-style alternative layouts), the 2016-block
> difficulty-retarget interval, and the double-SHA256 proof-of-work rule. Private
> type names, helper decomposition, control flow, and diagnostic wording are
> informative.

## 37.1 Activation configuration (R29 dictated by config schema)

R37.1.1 A UTXO coin's activation request MAY carry an SPV configuration object.
When present, the coin enables header verification; when absent, it does not.
The configuration is consumed at activation and is **not** a persistent field of
the running coin's core state.

R37.1.2 **Dictated config schema (config-compat).** The SPV configuration is
supplied as a single optional object under the coin's `conf` at the key
**`spv_conf`**. Third-party coin-config files populate it, so the exact JSON key
names below are dictated interop and shall be accepted verbatim. The object's
fields are:

| Key | Type | Required | Meaning |
|-----|------|----------|---------|
| `starting_block_header` | object (see R37.1.2a) | required | Trusted anchor: the height/header from which header sync and validation begin. |
| `max_stored_block_headers` | positive integer (non-zero) | optional | Maximum number of headers retained in the store. When the stored set would exceed this, the **oldest** headers are pruned (see §37.3). When omitted, retention is unbounded. |
| `validation_params` | object (see R37.1.2b) | optional | How fetched headers are validated. When omitted, headers are stored **without** proof-of-work / difficulty validation (trusted-RPC mode). |

R37.1.2a The **`starting_block_header`** object (the trusted anchor) has the
dictated fields:

| Key | Type | Meaning |
|-----|------|---------|
| `height` | unsigned integer | Block height of the anchor. |
| `hash` | string (hex) | Block hash of the anchor, given in the usual displayed (big-endian) hex form. |
| `time` | unsigned 32-bit integer | Anchor block timestamp, epoch seconds. |
| `bits` | unsigned 32-bit integer | Anchor block's compact difficulty bits. |

R37.1.2b The optional **`validation_params`** object selects difficulty
validation behaviour with the dictated fields:

| Key | Type | Required | Meaning |
|-----|------|----------|---------|
| `difficulty_check` | bool | required within the object | Whether to validate each header's proof-of-work / difficulty against the chain rule. |
| `constant_difficulty` | bool | required within the object | Whether the chain uses a fixed (non-retargeting) difficulty, so retarget computation is skipped. |
| `difficulty_algorithm` | string enum | optional | Chain difficulty-algorithm / chain-variant selector. Dictated string values: `"Bitcoin Mainnet"` and `"Bitcoin Testnet"`. When omitted, no algorithm-specific retarget rule is applied. |

R37.1.3 The configuration shall be validated at activation:
- When `difficulty_algorithm` is `"Bitcoin Mainnet"`: the
  `starting_block_header.height` shall be an exact multiple of the
  difficulty-retarget interval (2016), i.e. a retarget-boundary height;
  otherwise the configuration is rejected. If `max_stored_block_headers` is set,
  it shall be strictly greater than the retarget interval (2016); otherwise the
  configuration is rejected.
- `"Bitcoin Testnet"` is not currently supported and shall be rejected.
- At sync start the anchor fetched from the coin's RPC shall match the configured
  `starting_block_header` (its `bits`, `hash`, and `time`); a mismatch rejects
  activation.

R37.1.4 SPV verification shall be available on both native and WASM targets.

## 37.2 Header sync loop

R37.2.1 For an SPV-enabled coin the project shall run a background loop that
fetches block headers forward from the stored tip toward the chain's current
height and persists them.

R37.2.2 Each fetch shall be bounded to at most the chain's difficulty-retarget
interval of **2016** headers per request, so that a sync never spans more than
one retarget window in a single fetch.

R37.2.3 The loop shall compute its starting block from stored state and the
configured anchor, retry on transient backend failure, and surface its status
(e.g. progress / temporary error) without aborting the coin.

## 37.3 Header storage (native SQLite + WASM IndexedDB)

R37.3.1 The header store presents a single backend-agnostic contract (a public
cross-module storage trait) with these operations and semantics:

- **Initialize / check-initialized** -- create the coin's header collection and
  report whether it already exists.
- **Insert-or-overwrite a batch** -- add a set of headers keyed by height;
  writing a height that already exists **overwrites** it (last-writer-wins per
  height). This is the only write path and is how a divergent height is replaced.
- **Retrieve by height** -- return the stored header for a given height (in both
  decoded and raw-hex forms), or nothing if absent.
- **Total-count / emptiness query** -- report whether the store holds any
  headers (a `COUNT`-based check), used to decide first-time initialization and
  by tests.
- **Highest stored height (tip)** -- return the greatest stored height, or
  nothing when empty.
- **Height-by-hash lookup** -- return the height of a stored header matching a
  given hash, or nothing.
- **Most-recent non-limit-bits header** -- return the most recent stored header
  whose compact difficulty bits differ from a supplied maximum/limit value; used
  by the retarget computation.
- **Bulk removal of an inclusive height range** -- delete every stored header
  whose height lies in the closed interval `[from, to]` (**both endpoints
  inclusive**). This single primitive serves two directions:
  - **Oldest-pruning (retention):** remove the range `[0, bound]` where
    `bound = (tip_after_this_batch − max_stored_block_headers)`, deleting the
    OLDEST headers (heights `<= bound`) so the retained count stays within the
    configured limit. Pruning runs only when `tip_after_this_batch` exceeds the
    limit; otherwise nothing is removed.
  - **Divergent-suffix removal (reorg):** remove the range
    `[fork_height, current_tip]`, deleting every header at and above the fork
    height (heights `>= fork_height`). See §37.7.

R37.3.2 The native backend shall persist headers in SQLite; the WASM backend
shall persist them in IndexedDB. Both backends shall present the same storage
contract above. The IndexedDB backend shall store **real header records** in an
object store indexed by height and shall iterate with a **height-bounded cursor**
(reversed when locating the tip) that remains valid across asynchronous
continuation; it shall not be a no-op stub.

## 37.4 Header validation (R29/R31 dictated by Bitcoin consensus)

R37.4.1 The project shall validate a header's proof-of-work by checking that its
double-SHA256 hash meets the difficulty target encoded in its compact bits.

R37.4.2 At each difficulty-adjustment boundary the project shall compute the
retarget height and validate the next-block difficulty against the chain's
retarget rule.

R37.4.3 A header that fails proof-of-work or difficulty validation shall be
rejected and shall not be used to prove transaction confirmation.

## 37.5 Chain-variant header parsing (R31 externally dictated)

R37.5.1 Block-header byte parsing shall be selected by a **chain-variant**
indicator drawn from the coin's configuration, not hardcoded per ticker, so that
non-Bitcoin UTXO chains parse their headers correctly.

R37.5.2 The chain-variant set shall cover at least these externally-dictated
header families, each parsed per that chain's actual format:
- standard Bitcoin-style headers (BTC family);
- AuxPoW (merged-mining) headers (e.g. Namecoin-style chains);
- KawPoW / alternative-version headers (e.g. Ravencoin-style and BCH-using-KawPoW
  version layouts);
- Sapling-root-bearing headers (e.g. PIVX-style);
- alternative-layout / median-time-sensitive variants (e.g. PeerCoin-style PoSV
  layouts).

R37.5.3 The variant selection shall guard against mis-detecting AuxPoW where it
does not apply, so that headers for non-AuxPoW chains in a shared family parse
without an unexpected-end-of-input failure.

> **Interop note (R31).** The exact byte layout of each family's header is
> dictated by that chain, not by this project; R37.5 binds the *selection
> contract* (config-driven variant) and the *requirement to parse each family
> correctly*, not any particular parser implementation.

## 37.6 Use in swap confirmation & current MTP

R37.6.1 The verified header chain shall back the SPV confirmation path used when
deciding whether a swap-relevant transaction is sufficiently confirmed.
Concretely: a UTXO coin's configuration carries an independent boolean flag,
**`enable_spv_proof`**, under the coin's `conf` (dictated config-compat, like
`spv_conf` of R37.1.2 but a separate switch -- a coin may set either without
the other). When `enable_spv_proof` is true, the coin-layer payment-validation
operation that [chapter 15](15-swap-v2-utxo-path.md) R12, and the legacy and
version-two swap state machines
([chapter 51](51-legacy-v1-swap-state-machine.md),
[chapter 52](52-swap-v2-state-machine.md)) invoke as an external dependency to
validate a maker or taker payment shall, in addition to the ordinary
confirmation-count wait, fetch a Merkle inclusion proof for the swap-relevant
transaction and validate that proof against the header of the block the
transaction is included in, retrying until a deadline; a transaction whose
proof does not validate shall not be accepted as a validated payment. This
proof-of-inclusion check applies only when the coin is connected through the
project's Electrum-family RPC backend; a coin connected through the chain's
native-daemon RPC backend does not perform it. When the payment's own
required-confirmations value is zero, the check is skipped along with the
confirmation wait.

R37.6.1a The block header the proof-of-inclusion check of R37.6.1 validates
against shall be drawn from the persistent header store of §37.3 -- and
therefore already carries the proof-of-work / difficulty-retarget validation
of §37.4 -- whenever the coin's `spv_conf` (R37.1.2) is configured. When
`spv_conf` is not configured for that coin, `enable_spv_proof` may still be
set independently, and the check falls back to a header fetched directly from
the RPC backend for that request, with no proof-of-work/difficulty validation
applied to it and nothing about it persisted.

> **Code-quality finding (informative).** The fallback of R37.6.1a is a real
> reduction in what the proof-of-inclusion check proves, not merely a
> documented option. When `enable_spv_proof` is set without a configured
> `spv_conf`, the header the Merkle proof is checked against is trusted from
> the same RPC backend the check exists to avoid fully trusting, with no
> proof-of-work or difficulty validation and no persisted cross-request
> record of it. A Merkle inclusion proof against an unauthenticated header
> only shows that the transaction is included in *some* block the server
> handed back for that height, not that the block belongs to the coin's
> actual heaviest valid chain -- the exact property §37.0 states this
> subsystem exists to avoid trusting a single remote server for. Requiring
> `spv_conf` whenever `enable_spv_proof` is set (or deriving one flag's
> effective value from the other's presence, rather than treating them as
> fully independent switches) would close the gap. This chapter does not
> resolve the choice; it is a config-validation decision belonging to the
> owning coin-activation path.

R37.6.1b The proof-of-inclusion check of R37.6.1 changes what a payment-
validation call can conclude, not when it is called or how long it waits: the
confirmation-wait deadlines, stage/state transitions, and event vocabularies
bound by chapters 51 and 52 are identical whether or not SPV is configured for
a coin, because both state machines treat payment validation as a single
opaque external step regardless of its internal implementation (chapter 51
§51.2, chapter 52 §52.2). SPV is therefore a trust-minimization layer nested
inside an existing validation step, not a parallel or alternative
confirmation mechanism with its own timing.

R37.6.2 The public `get_current_mtp` RPC shall report a coin's current
median-time-past, computed from the relevant recent headers.

## 37.7 Chain-reorganization handling

R37.7.1 The header store shall tolerate a reorganization by overwriting a stored
height with a newly-fetched header value for that height (last-writer-wins on a
per-height basis), so that a shorter divergent suffix is replaced as the loop
re-fetches.

R37.7.2 **(Target -- active reorg detect-and-resolve.)** In addition to the
passive overwrite of R37.7.1, the sync loop shall run an **active** reorg
detector with the following behaviour:

**(a) Trigger.** During validation of a freshly-fetched batch, the loop compares
each header's recorded parent/previous-block hash against the hash of its stored
(or just-validated) predecessor. A discontinuity -- a header whose parent hash
does not match the predecessor at the height immediately below it -- signals a
fork and yields the height at which the mismatch was observed (the candidate fork
height). Detection of this parent-hash discontinuity is what arms the resolver;
absent it, normal forward sync proceeds.

**(b) Resolve.** On detection the resolver re-fetches a bounded chunk of headers
starting at the candidate fork height from the backend and re-validates it
against the stored header one below the chunk's start. If that chunk now
validates cleanly (parent hashes line up and proof-of-work/difficulty pass), the
divergent suffix already in the store -- the inclusive range from the fork height
up to the current stored tip -- is removed (per the suffix-removal direction of
§37.3.1), and the loop resumes forward sync, which re-fetches and re-stores the
now-correct suffix from the heavier valid chain. If, instead, re-validation
surfaces a parent-hash discontinuity at a still-lower height, the search window
moves strictly further back (by up to one chunk) and the attempt repeats from
there.

**(c) Walk-back bound.** The backward search is bounded **below by the configured
trusted anchor** (`starting_block_header`): each step clamps the next fetch range
so it never goes below the anchor height, and the window moves back by at least
one position per unresolved step. It does **not** rely on stopping at the
retarget interval. If the search reaches the anchor without finding a consistent
join -- i.e. the divergence extends down to the preconfigured starting header --
the resolver reports a **bad starting-header chain** condition (the trusted
anchor itself must be reconfigured) rather than looping further.

**(d) Convergence.** Because every unresolved step moves the search window
strictly backward toward a fixed lower bound (the anchor) and any header failing
proof-of-work/difficulty is rejected, the resolver terminates in a bounded number
of steps. Each successful resolution deletes the divergent suffix and re-syncs
the valid one, so the store converges to the heaviest valid chain consistent with
the trusted anchor.

> **Upstream divergence (informative).** Reloaded historically shipped only the
> passive per-height overwrite of R37.7.1; the active detector of R37.7.2 is the
> behaviour being ported here to reach upstream/corpus parity. Both converge to
> the same verified chain once sync catches up, but the active detector converges
> faster and rejects an invalid fork earlier. This is a behavioural (not
> wire-format) port. The implementation shall be expressed independently; this
> section binds the contract, not any particular branch structure.

**Acceptance (binds the §37.7 unit test).** A unit test shall seed a stored
header chain, then feed a **divergent, heavier valid suffix** whose first header's
parent hash does not match the stored predecessor at the fork height, and assert
that the detector (i) identifies the fork height, (ii) removes the stale suffix
from the fork height through the old tip, and (iii) leaves the store converged on
the heavier valid chain. A companion test feeding a divergence that extends to
the trusted anchor shall assert the bad-starting-header-chain condition is
reported rather than an unbounded walk-back.

## 37.8 Acceptance criteria

- An SPV-enabled UTXO coin activates only with a valid SPV configuration
  (R37.1.3) and rejects an invalid starting header or an under-sized
  max-stored-headers limit.
- The sync loop persists headers in <= 2016-header chunks and advances the tip
  (R37.2.2).
- Header storage round-trips on both SQLite (native) and IndexedDB (WASM),
  including count, inclusive-range removal, and oldest-pruning (R37.3).
- Proof-of-work and retarget validation reject a header with invalid bits
  (R37.4).
- The active reorg detector converges to the heavier valid chain on a divergent
  suffix and reports a bad-starting-header-chain condition when the divergence
  reaches the trusted anchor (R37.7.2).
- Headers for at least one coin from each dictated family (BTC, AuxPoW, KawPoW,
  Sapling-root, PoSV-layout) parse correctly under the configured chain variant
  (R37.5.2).
- `get_current_mtp` returns a plausible median-time-past for an SPV-enabled coin
  (R37.6.2).
- A swap-relevant payment on an Electrum-connected coin with `enable_spv_proof`
  set is validated only when its Merkle inclusion proof checks against the
  applicable block header, and that header carries the §37.4 proof-of-work
  validation whenever `spv_conf` is also configured for that coin (R37.6.1,
  R37.6.1a). A coin's confirmation-wait deadlines and swap-stage transitions
  are unchanged by whether SPV is configured (R37.6.1b).
