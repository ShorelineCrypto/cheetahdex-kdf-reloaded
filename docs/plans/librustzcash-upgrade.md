# Plan: bump the vendored `librustzcash` (shielded-sync performance)

> **Status:** deferred / not started. Scoping + workload analysis only. This is
> the single biggest lever for shielded (ARRR/ZHTLC) sync speed, because the
> dominant cost is the scan inside the vendored zcash library, not our fetch.

## Why

Shielded activation time is dominated by `scan_cached_blocks` (trial-decryption
of every Sapling output + incremental witness/commitment-tree updates + SQLite
writes), which lives in the **vendored** zcash crates, not in our code. Our fetch
is already cheap and easily pipelined; the scan is the wall.

## Current state (what we vendor)

Vendored under `librustzcash-patched/` (anchor-era, ~2021, GPL-compatible):

| Crate | Vendored version | Modern upstream (2025) |
| --- | --- | --- |
| `zcash_primitives` | 0.5.0 | ~0.15+ |
| `zcash_client_backend` | 0.5.0 | ~0.12+ |
| `zcash_client_sqlite` | 0.3.0 | ~0.10+ |
| `zcash_proofs` | 0.5.0 | ~0.15+ |
| `zcash_note_encryption` (component) | 0.0.0 (local) | 0.4+ (crates.io) |

Only **`mm2src/coins`** consumes them. The tree is a *patched full copy* (listed
in `rustfmt.toml`'s ignore list and the root `[patch.crates-io]`), so the local
modifications must be catalogued before any bump (open question — see risks).

Old-API surface our `z_coin` code binds (small but load-bearing):

- `scan_cached_blocks(&params, &block_db, &mut update_ops, None)` (×6)
- `WalletDb::for_path` (×5), `BlockDb::for_path` (×2)
- `init_wallet_db`, `init_accounts_table`, `init_blocks_table`, `init_cache_database`
- `get_balance`, `get_update_ops`

## What modern librustzcash changes (the hard part)

The scan/wallet API was **rewritten** between 0.5/0.3 and modern releases:

1. **Commitment tree → `shardtree`.** The old per-block `sapling_tree` BLOB in the
   `blocks` table is replaced by an incremental `shardtree` shard store. This is a
   **schema change** and changes how we seed a checkpoint (`GetTreeState` →
   `put_sapling_subtree`/frontier insert) and how witnesses are produced. Our
   entire "seed at `start-1`, scan forward" model in
   `z_coin_wallet_db.rs` is rewritten against the new store.
2. **Batched note decryption.** `zcash_note_encryption::batch` +
   `zcash_client_backend::scanning::{ScanningKeys, scan_block}` give the large
   speedup — this is the actual goal of the bump.
3. **`WalletRead`/`WalletWrite` traits + `WalletMigrator`.** `WalletDb` gains a
   migration framework (`schemerz`); `get_balance` → `get_wallet_summary` /
   `get_target_and_anchor_heights`; `get_update_ops` disappears.
4. **`ScanRange`/`suggest_scan_ranges` sync engine.** The "cache all → scan all"
   flow is replaced by a spend-before-sync range planner; this actually *helps*
   our fetch-plan work but requires adopting the new driver.
5. **Orchard** is threaded through many signatures even if unused.

## Rough workload

- **Storage/scan rewrite in `z_coin_wallet_db.rs`** (open_or_create, checkpoint
  seeding, fetch plan, `scan_cached_blocks_to_height`, balance): **large.** This
  is the bulk of the work — effectively re-implementing the shielded store
  against the new traits + shardtree, with a **DB migration** for existing
  wallets (old `blocks.sapling_tree` → shardtree).
- **`z_coin.rs` sapling-cache loop / builder**: medium — the native
  commitment-tree cache overlaps the new shardtree; likely simplify/retire it.
- **Tx building (`gen_tx`/`send_outputs`, `z_htlc.rs`)**: medium — `zcash_primitives`
  transaction builder + prover API changed (`LocalTxProver`, `Builder`).
- **Re-vendor + re-license**: medium/uncertain — re-copy a modern tag, re-apply
  and re-justify local patches, re-check GPL compatibility of any new transitive
  deps (the reason `bitcoinconsensus`/some crates are excluded still applies).
- **WASM**: the modern stack has better WASM support; revisit R39.6.1.

## Risks / open questions (resolve before committing)

- **Local patches:** what were the anchor-era modifications and are they still
  needed? Must diff the vendored tree against its original upstream tag.
- **On-disk migration:** existing users have old-schema `ARRR_WALLET.db`. A
  one-way migration (or a documented re-scan) is required; funds-safety review
  needed (witnesses must remain valid or be rebuilt).
- **License/provenance:** every new transitive dependency must pass the same
  GPL-compatibility gate (`deny.toml`) as the rest of the tree.
- **Compatibility:** none of this is wire-facing (compact blocks + lightwalletd
  are unchanged), so there is no netid/`v2.6.0-beta` wire risk — the risk is
  purely local storage + a DB migration.

## Suggested phasing

1. Spike: pin modern `zcash_client_backend`/`_sqlite` from crates.io in a branch,
   catalogue the vendored patches, and see whether we can drop the vendored tree
   entirely (best outcome) or must re-vendor.
2. Rewrite the shielded store + scan against the new traits behind the existing
   `ZCoinShieldedHistory` interface (keep the fetch-plan semantics from
   `shielded-sync-cache-reuse.md`).
3. Add a wallet-DB migration + a deterministic re-scan fallback.
4. Adopt batched scanning; benchmark ARRR activation before/after.
5. Re-check WASM (R39.6.1) and update the CRD ch.39 storage sections.
