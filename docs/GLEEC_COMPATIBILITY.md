# Running KDF Reloaded in full GLEEC-KDF compatibility

This chapter is the single place that lists every configuration value an operator must set to make a KDF Reloaded node behave equivalently to a GLEEC KDF node, for the benefit of operators migrating an existing GLEEC KDF deployment or third-party integrations that target the GLEEC behaviour.

The convention behind this chapter — including the developer rule that requires every divergent change to land an entry here — is documented in [`COMPAT_SWITCHES.md`](COMPAT_SWITCHES.md).

## How to use this chapter

1. Start from your existing `MM2.json` (or whichever configuration surface you use).
2. Walk down the table below. For each row, set the listed value if you want the GLEEC-equivalent behaviour for that area.
3. Skip any row whose KDF Reloaded default already matches what you want; rows are listed exhaustively, not by impact.
4. For acknowledgement-gated settings (marked **⚠ gated**), additionally set the listed acknowledgement key — and read the per-setting documentation linked from the row, because these settings select behaviour the project does not endorse but still supports for compatibility.

There is no global "GLEEC mode" switch and no shared JSON object — every setting is its own thing in its own place. This chapter is the unifying reference, not a code construct.

## Settings

| Area | Setting | GLEEC-compatible value | Acknowledgement-gated? | Per-setting docs |
|------|---------|------------------------|------------------------|------------------|
| *(none in v0.1.0-alpha.1)* | — | — | — | KDF Reloaded v0.1.0-alpha.1 ships with no behavioural divergences from GLEEC KDF that require operator configuration. The framework is in place; entries land here as divergent features land. |

## Future fork chapters

The same chapter pattern (one document, one exhaustive table) can be reused if other significant forks or upstream divergences ever need an analogous compatibility surface — for example, `UPSTREAM_COMPATIBILITY.md` for the upstream Komodo DeFi Framework, should that ever diverge meaningfully from KDF Reloaded. Each such chapter is independent of the others and lists only the settings relevant to its named target.

## See also

- [`COMPAT_SWITCHES.md`](COMPAT_SWITCHES.md) — the developer-facing rule and convention.
- [`../RELOADED_VS_GLEEC.md`](../RELOADED_VS_GLEEC.md) — the catalogue of divergences (what changed, not how to undo it).
- [`../CHANGELOG.md`](../CHANGELOG.md) — every divergent change is logged here.
