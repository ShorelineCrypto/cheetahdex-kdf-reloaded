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
| WalletConnect session storage | `wc_session_persistence` | `open` *(also the default)* | No | [CRD ch.22 §22.5](reloaded-rewrite/22-walletconnect-v2.md) |

**`wc_session_persistence`.** Controls whether and how WalletConnect v2
sessions are written to durable storage. It governs *saving* only — existing
stored sessions are always read and used regardless of the value. Values:

- `open` *(default)* — sessions are stored in the GLEEC-compatible plaintext
  format, including the session symmetric key. This keeps the on-disk format
  byte-interchangeable with GLEEC KDF in both directions. The key is stored
  unencrypted at rest; the bounded exposure, and why it is accepted for
  compatibility, is described in CRD chapter 22. This is **not** the
  fund-controlling wallet secret, which is encrypted independently.
- `none` — sessions are never written to storage. Existing rows are still
  read and used, but never rewritten. Choose this if you do not want the
  session key persisted at all.
- `encrypted` — reserved for a future encrypted-at-rest format; not yet
  available (selecting it stops startup with an explanatory error).

## Future fork chapters

The same chapter pattern (one document, one exhaustive table) can be reused if other significant forks or upstream divergences ever need an analogous compatibility surface — for example, `UPSTREAM_COMPATIBILITY.md` for the upstream Komodo DeFi Framework, should that ever diverge meaningfully from KDF Reloaded. Each such chapter is independent of the others and lists only the settings relevant to its named target.

## See also

- [`COMPAT_SWITCHES.md`](COMPAT_SWITCHES.md) — the developer-facing rule and convention.
- [`../RELOADED_VS_GLEEC.md`](../RELOADED_VS_GLEEC.md) — the catalogue of divergences (what changed, not how to undo it).
- [`../CHANGELOG.md`](../CHANGELOG.md) — every divergent change is logged here.
