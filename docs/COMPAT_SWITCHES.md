# Compatibility switches

KDF Reloaded supports a single global compatibility mode controlled by the `kdf_compat_mode` field in `MM2.json`:

```json
{ "kdf_compat_mode": "reloaded" }
```

| Value            | Meaning |
|------------------|---------|
| `"reloaded"`     | KDF Reloaded native behaviour. **Default.** |
| `"gleec_legacy"` | Opt in to behaviours matching GLEEC KDF where they would otherwise diverge from `reloaded`. |

This document enumerates the specific behavioural switches that respect `kdf_compat_mode`. Each switch lists its default in `reloaded`, the alternative behaviour in `gleec_legacy`, and the release that introduced it.

## Active switches

| Switch | Default (`reloaded`) | `gleec_legacy` | Introduced | Notes |
|--------|----------------------|----------------|------------|-------|
| *(none in v0.1.0-alpha.1)* | — | — | — | The framework is shipped without any divergent switches active. |

## Adding a new switch

When a contributor needs to introduce a behaviour that differs between modes:

1. Add a row to the table above with a stable, descriptive `snake_case` switch name.
2. Read the mode through `MmCtx::compat_mode()` at the decision point — do not introduce ad-hoc string comparisons.
3. Default the `reloaded` branch to the desired KDF Reloaded behaviour; the `gleec_legacy` branch should reproduce GLEEC behaviour as closely as practical.
4. Cover both branches with tests.
5. Note the change in `CHANGELOG.md` and in the relevant section of `RELOADED_VS_GLEEC.md`.

## Removing a switch

A switch may be removed when:

- The `gleec_legacy` branch is no longer reachable in any supported configuration; or
- A migration path (with a deprecation warning issued for at least one minor release) has been provided to operators.

Removed switches are listed below with their final release.

| Switch | Removed in | Final behaviour |
|--------|------------|-----------------|
| *(none)* | — | — |
