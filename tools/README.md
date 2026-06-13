# Clean-room tooling

## `clean_room_gate.py` — R35 / R36 residual-similarity gate

Automated implementation of the residual-similarity gate that
[`docs/reloaded-rewrite/01-clean-room-rules.md`](../docs/reloaded-rewrite/01-clean-room-rules.md)
defers (rule **R35**, deferral **D1**, and chapter 30). The gate scores a
reloaded file's **discretionary expression** against a *relicensed historical
record* reference and never reads the forbidden corpus (R8).

### What it measures

R35 forbids using a whole-file similarity figure as a clean-room gate for
interface-dense files: signatures, serde renames, schema names, `#[repr]`
discriminants, pinned constants, and import lists are *required* to match, so
their matching proves nothing. The gate therefore scores the **discretionary
body** (function bodies, helper decomposition, control flow, local naming,
comment/diagnostic wording) after stripping comments and normalizing
whitespace, with pinned-interface / R28–R33 lines excluded from the measured
set.

A file **passes** when its discretionary-body similarity is materially below its
whole-file similarity (configurable margin) and under an absolute cap. Residual
byte-identical discretionary lines are listed so a reviewer can confirm each is
attributable to an excluded fragment, as R35 requires.

### Marking excluded (pinned) lines

Exclusion is explicit, matching the CRD requirement that reused fragments be
*marked* at the point of reuse:

| Marker | Effect |
| --- | --- |
| `// crd:pin` (end of line) | exclude that single line |
| `// crd:pin-begin` … `// crd:pin-end` | exclude the enclosed block |
| `// crd:r28` … `// crd:r33` | exclude one line, recording its fragment class |

`#` comment markers work too (for non-Rust files). `--auto-pin` additionally
excludes obvious structural interface lines (imports, `fn`/`struct`/`enum`/
`trait`/`impl` signatures, `#[repr(...)]`, serde `rename`); it is advisory —
explicit markers are authoritative.

### Usage

```sh
# Gate everything listed in the manifest
python3 tools/clean_room_gate.py --manifest tools/clean_room_gate.manifest.json

# One-off check against the GPLv2 anchor commit
python3 tools/clean_room_gate.py \
  --reloaded mm2src/foo/src/bar.rs \
  --reference git:c1d46c0c1592faa0860f704008b2b2381bc3840f:mm2src/foo/src/bar.rs \
  --verbose
```

### Manifest format

`clean_room_gate.manifest.json` is a JSON list. Each entry names a reloaded
file and its reference record; `reference` may be a working-tree path or a
`git:<ref>:<path>` spec resolved with `git show`.

```json
[
  {
    "reloaded": "mm2src/foo/src/bar.rs",
    "reference": "git:c1d46c0c1592faa0860f704008b2b2381bc3840f:mm2src/foo/src/bar.rs",
    "margin": 0.15,
    "max_discretionary": 0.50
  }
]
```

The manifest ships empty; entries are added as clean-room chapters land. CI runs
the gate via [`.github/workflows/clean-room-gate.yml`](../.github/workflows/clean-room-gate.yml).
