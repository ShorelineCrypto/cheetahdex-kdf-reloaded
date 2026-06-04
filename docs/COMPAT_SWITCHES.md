# Compatibility switches

KDF Reloaded is a continuation of the upstream Komodo DeFi Framework. Where our behaviour intentionally diverges from the GLEEC KDF fork (or the upstream KDF), we expose a **per-feature compatibility switch** under the `compatibility` object in `MM2.json` so operators, third-party API clients, and existing integrations can opt back into the original behaviour on a feature-by-feature basis.

There is **no single global compatibility mode**. Each divergent feature ships with its own switch, its own default, and its own documentation entry below.

## Why per-feature

A single global flag would force operators to choose between "all old" and "all new" — they could not, for instance, opt into our improved order-matching while keeping legacy fee-reporting semantics. Per-feature switches make every divergence a deliberate, narrow operator decision and keep the migration path granular.

## Configuration shape

```json
{
  "netid": 8762,
  "compatibility": {
    "<switch_name>": "<value>"
  }
}
```

A `compatibility` object that omits a switch implicitly takes that switch's documented default. Unknown keys are rejected at startup so typos cannot silently change behaviour.

## `MM2_classic.json` template

A maintained template, [`MM2_classic.json`](../MM2_classic.json), pins every active switch to the value that reproduces upstream / GLEEC KDF behaviour as closely as practical. Operators replacing a GLEEC KDF deployment with KDF Reloaded should start from this template and remove individual switches as they evaluate the corresponding KDF Reloaded behaviour.

The template is updated alongside every new switch.

## Active switches

| Switch | Default | Compatibility value | Introduced | Risk if mismatched | Notes |
|--------|---------|---------------------|------------|--------------------|-------|
| *(none in v0.1.0-alpha.1)* | — | — | — | — | The framework is shipped without any divergent switches active. |

Each switch added to this table also gets its own subsection below explaining the behavioural difference, the rationale, the third-party-API impact, and the migration path.

## Adding a new switch (developer rule)

**Any change to KDF Reloaded that diverts from upstream / GLEEC KDF behaviour in a way that can:**

- break an existing third-party integration,
- change the shape of an RPC response,
- change the meaning of an RPC argument,
- change order-matching, fee, or settlement semantics,
- cause user error reports of the form "it worked in GLEEC KDF",
- or in the worst case lead to coin loss,

**must ship together with a compatibility switch** in `docs/COMPAT_SWITCHES.md`, the `MM2_classic.json` template, the `compatibility` schema, and the `RELOADED_VS_GLEEC.md` summary lists.

Procedure:

1. Add a row to the active-switches table above with a stable, descriptive `snake_case` name.
2. Decide and document the **default**. The default should be whichever behaviour we expect the majority of new operators to want; it is **not** required to be the upstream-compatible value.
3. Add a subsection below describing: the divergent behaviour, the compatibility behaviour, the chosen default and why, the risk if the operator picks the wrong value, the third-party-API impact, and the migration path.
4. Wire the switch through the configuration loader so it lands in `MmCtx` as a typed value (no `String`/`Value` access at decision points).
5. Add the switch to `MM2_classic.json` with the compatibility value.
6. Add the switch to the `Added` section of `RELOADED_VS_GLEEC.md`.
7. Cover both branches with tests.
8. Note the change in `CHANGELOG.md`.

### When the default departs from GPLv2-or-fair-trading principles

If GLEEC KDF (or upstream) introduces a behaviour that conflicts with this project's principles — non-GPLv2 distribution, non-free trading, restrictions on who may participate, or similar — the corresponding switch:

- defaults to the KDF Reloaded behaviour;
- still exposes the original-compatible value, but **only behind an explicit acknowledgement gate** (a second config key such as `i_understand_<switch>_implications: true`); and
- emits a prominent runtime warning whenever the original-compatible value is selected.

Document the rationale in the per-switch subsection.

## Removing a switch

A switch may be removed when:

- the compatibility branch is no longer reachable in any supported configuration; or
- a migration path (with a deprecation warning issued for at least one minor release) has been provided to operators.

Removed switches are listed below with their final release.

| Switch | Removed in | Final behaviour | Migration |
|--------|------------|-----------------|-----------|
| *(none)* | — | — | — |

## Related documents

- [`RELOADED_VS_GLEEC.md`](../RELOADED_VS_GLEEC.md) — public catalogue of the same divergences.
- [`MM2_classic.json`](../MM2_classic.json) — drop-in compatibility template.
- [`CHANGELOG.md`](../CHANGELOG.md) — every switch addition or removal is logged.
