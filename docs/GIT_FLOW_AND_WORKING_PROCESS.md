# Git flow and branch strategy

KDF-Reloaded uses a three-tier permanent branch model plus short-lived
feature branches.

## Permanent branches

| Branch | Purpose | Stability |
|---|---|---|
| `main` | Tagged releases only | Always shippable |
| `staging` | Pre-release integration | Green CI, QA-tested |
| `dev` | Active development | Green CI, may have unreleased features |

Hierarchy: `main` ← `staging` ← `dev` ← feature branches.

`reloaded-gplv2-base` is a historical branch. It stopped receiving work in
June 2026 and is far behind `dev`, so do not branch from it or compare
against it. It is kept deliberately rather than pruned: it records the
GPL-2.0-only base that the project's distribution rights rest on (see
[`LEGAL/LICENSING-POLICY.md`](../LEGAL/LICENSING-POLICY.md)), so it remains
available for provenance and review even though no further work will land on
it.

Promotion is one-way and explicit:
1. Feature branch → `dev` after PR review + green CI.
2. `dev` → `staging` when a release candidate is ready.
3. `staging` → `main` after QA sign-off, then tag `vX.Y.Z` (annotated, GPG-signed).

Release tags use the `v*` convention and drive the signed release pipeline
([`.github/workflows/release.yml`](../.github/workflows/release.yml)):

- **Pre-release tags** (`vX.Y.Z-alpha.N` / `-beta.N` / `-rc.N`) are cut on
  `staging`; they publish a signed GitHub **pre-release** (not marked "Latest").
- **Final tags** (`vX.Y.Z`) are cut on `main`; they publish the signed, latest
  GitHub Release.

Only `release.yml` signs and publishes. Unsigned, untagged branch snapshots come
from [`dev-build.yml`](../.github/workflows/dev-build.yml) (manual) and
[`staging-build.yml`](../.github/workflows/staging-build.yml) (automatic on push
to `staging`).

When comparing against an ancestor, use:

```bash
git merge-base HEAD origin/dev
```

If `origin/staging` and `origin/main` already exist in your clone, you may
include them as additional candidates.

The deprecated upstream `mm2.1` branch is not used in Reloaded.

## Feature branches

- Lifetime ≤ 1–2 weeks. Decompose larger work into multiple feature branches.
- Branch from `origin/dev`. Never branch from `main`, `staging`, or
  `reloaded-gplv2-base`.
- Hotfixes for `main` are exceptional and must be back-merged into
  `staging` and `dev` immediately after.

## Commits

- Small, self-contained commits. Each commit must leave the tree compiling
  and tests passing.
- Format touched packages with the pinned toolchain
  (`cargo +nightly-2026-05-08 fmt -p <package>`) before committing. CI fails
  on unformatted code. Never run a bare workspace-wide `cargo fmt`.
- Run `cargo clippy -p <package> --all-targets --no-deps -- -D warnings` on
  touched packages. For WASM-only code add `--target wasm32-unknown-unknown`.
  `AGENTS.md` §6 has the full verification list.
- For larger work, write or follow a plan under [`docs/plans/`](./plans/)
  and record progress there as items ship. The rule, the threshold and the
  plan header are in [Plans and the changelog](#plans-and-the-changelog) below.

## PRs

- PR title prefix indicates state: `[wip]`, `[r2r]` (ready to review).
- Reference the relevant phase / plan item in the PR description.
- A PR that changes work covered by a plan updates that plan in the same PR
  (rule **P1** below).
- See [PR_REVIEW_CHECKLIST.md](./PR_REVIEW_CHECKLIST.md) for review criteria.

## Plans and the changelog

Adopted 2026-10-18. The rule is one sentence; the four refinements under it are
what make it hold. Cite them by id (**P0**–**P4**).

**P0 — the rule.** Everything we work on has a plan in [`docs/plans/`](./plans/),
kept up to date — except small, simple fixes of the kind done in a single
commit. Everything important is in [`CHANGELOG.md`](../CHANGELOG.md), kept
brief, derived from commit history.

**P1 — currency needs a trigger.** The PR that changes the work updates its plan
**in the same PR**. A rule with no trigger decays, and a stale plan is worse
than no plan. Reviewers enforce this: if a PR moves work a plan describes and
the plan is untouched, that is a review comment. The same PR also bumps the
plan's `Last reviewed` date (**P2**), and that bump is what makes "up to date"
checkable from outside.

**P2 — every plan carries a status and a last-reviewed date.** The second line of
every plan, immediately after its title, is exactly:

```
**Status:** <status> — **Last reviewed:** YYYY-MM-DD
```

so that a finished plan is distinguishable from an abandoned one and from a
current one. A short free-text qualifier may follow the status word in
parentheses — `parked (T1–T10 shipped; T11–T13 queued)`. The status word is one
of six:

| Status | Meaning |
|---|---|
| `planning` | The approach is not settled yet. The document is input to a design pass, not something to execute. |
| `in progress` | Being implemented now. |
| `blocked` | Ready to execute, but waiting on something outside this repository (an upstream release, a third party, a decision someone else owes us). Name the blocker. |
| `parked` | Approach settled, nothing in flight, not scheduled. Priority is the only thing in the way. |
| `done` | The work shipped. The plan stays as the record of the decisions behind it. Residual gaps are named in the qualifier or the body. |
| `superseded` | The approach was abandoned or replaced. **Must** point at what replaced it. |

`Last reviewed` is the date someone last checked the plan against the tree — not
the date it was written. Where the real status genuinely cannot be determined,
write `unknown` with a one-line reason rather than guessing; that is a defect to
clear, not a seventh status.

A plan may be deleted once it is `done` or `superseded` and nothing references
it; prefer keeping it, because the decisions it records are not in the code.

Checking by inspection:

```bash
# every plan has a well-formed header
grep -L '^\*\*Status:\*\* \(planning\|in progress\|blocked\|parked\|done\|superseded\|unknown\)' docs/plans/*.md
# current statuses and review dates
grep -H '^\*\*Status:\*\*' docs/plans/*.md
```

**P3 — the threshold is a decision the code does not record.** "One commit" is
the rough size cut; the sharp test is whether the work carries a decision the
code cannot show — a rejected alternative, a why-not, an accepted cost, a
measurement a later reader would otherwise have to repeat. That is what a plan
uniquely holds, and it needs one whatever the diff size. A mechanical fix whose
reasoning is fully visible in the diff needs no plan, however large. A two-line
change that encodes a choice between two defensible designs does.

Work that has already shipped and carries such decisions gets a plan written
**retrospectively**, marked `done`. Recording it late is worth more than not
recording it.

**P4 — the changelog is user- and integrator-facing.** `CHANGELOG.md` is filtered
to what someone running or integrating against KDF Reloaded would notice: RPC
and wire behaviour, configuration keys, defaults, compatibility switches,
platform support, security-relevant behaviour. Internal hygiene — refactors,
test scaffolding, CI, dependency moves with no observable effect — belongs in
the PR description and, where it carries decisions (**P3**), in a plan. Entries
stay brief and derive from commit history; the reasoning lives in the plan, and
the entry may point at it.

## CI

- `test.yml` runs on GitHub-hosted runners for every pull request into, and
  every push to, `dev`, `staging` and `main` (and `reloaded-gplv2-base`).
  Branch protection requires its `tests-passed` job, which succeeds only when
  `fmt`, `clippy`, every `unit-tests` leg and `wasm32-check` all succeeded. To
  make a new test job required, add it to `tests-passed`'s `needs:` list in
  `test.yml`; repository settings do not change. `docker-tests`,
  `trezor-emulator-tests` and `external-network-tests` run but are not
  required, because they depend on third-party services or live networks.
- Platform builds (Windows, macOS, iOS, Android, WASM) live under
  `.github/workflows/build-*.yml` and are dispatched manually until each
  runner is verified — see [CI_RUNNERS.md](./CI_RUNNERS.md).
- The umbrella `dev-build.yml` (manual) fans out to every platform-build child
  via `workflow_call` for on-demand `dev` snapshots.
- `staging-build.yml` runs the same fan-out **automatically on every push to
  `staging`**, giving testers fresh unsigned beta/rc binaries.
- `release.yml` runs on `main` `v*` tags and produces the signed, published
  release (see [DEV_BUILD.md](./DEV_BUILD.md) §7 and [RELEASE.md](./RELEASE.md)).
