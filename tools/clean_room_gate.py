#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Clean-room residual-similarity gate (R35 / R36).

This tool implements the residual-similarity gate that chapter 01 (rule R35,
deferral D1) and chapter 30 of the clean-room rewrite documents defer to an
automated check. It compares a reloaded source file against a reference tree and
scores similarity over *discretionary expression only*.

The reference may be a relicensed historical baseline (e.g. the GPLv2-anchored
base) OR the upstream we assert independence from, pinned by commit in the
manifest. To honour the R8 wall, the upstream is never vendored into the clean
checkout: it is read only through `git show` inside a GATED reference repository
(an existing corpus clone that holds the pinned commit, or a shallow clone
materialised in a cache directory kept OUTSIDE the clean tree). Only
clean-channel results cross back -- similarity scores, line counts, and
residual-identical lines. Every residual-identical line, by construction, also
exists verbatim in the reloaded source, so no upstream-only expression is ever
emitted.

What R35 requires (see docs/reloaded-rewrite/01-clean-room-rules.md):

  * Similarity MUST be measured over discretionary body only: function bodies,
    private helper decomposition, control flow, local naming, and
    comment/diagnostic wording, after stripping comments and normalizing
    whitespace.
  * Lines that R35 EXCLUDES from the measured set:
      - fragments classified under R28-R33 (generated artifact, interop /
        wire-format reuse, convergent idiomatic shape, third-party-API-bound
        shape), and
      - pinned interface surface: public type/enum/struct/trait/function
        signatures, serde wire renames, on-disk schema and table/column names,
        `#[repr]` discriminants, pinned constant values, and import lists.
    Those lines are *required* to match, so matching carries no inference of
    copying.
  * A file PASSES when its discretionary-body similarity is materially below
    its whole-file similarity AND each residual identical discretionary line is
    individually attributable to an excluded (R28-R33 / pinned-interface) line.

R36 is the interpretive companion: a chapter's code blocks bind only functional
and interface content; discretionary expression shown alongside is informative.
This tool therefore scores the discretionary remainder, never the pinned
contract.

Exclusion is driven by explicit markers, because R34/R35 require fragments to be
*marked* at the point of reuse rather than guessed:

  * `// crd:pin` (or `# crd:pin`) at end of a line excludes that single line.
  * `// crd:pin-begin` ... `// crd:pin-end` excludes the enclosed block.
  * `// crd:r28` .. `// crd:r33` behave like `crd:pin` (explicit fragment
    class, recorded for the report).

A conservative structural fallback (`--auto-pin`) additionally excludes obvious
interface lines (import/use statements, `fn`/`struct`/`enum`/`trait`/`impl`
signature lines, `#[repr(...)]`, and serde `rename` attributes). The fallback is
advisory only; explicit markers are authoritative.

Usage:
    clean_room_gate.py --manifest tools/clean_room_gate.manifest.json
    clean_room_gate.py --reloaded a.rs --reference git:<ref>:path/to/a.rs

Manifest forms (JSON):

  Object form -- pins an upstream origin; each file's "reference" is the upstream
  PATH, expanded to git:<reference_origin.commit>:<path> and resolved against the
  gated reference repository:
    {
      "reference_origin": {
        "url": "https://github.com/EXAMPLE/repo",
        "branch": "dev",
        "commit": "<40-hex sha>",
        "local_clones": ["/optional/hint/to/an/existing/clone"]
      },
      "defaults": { "margin": 0.15, "max_discretionary": 0.50 },
      "files": [
        { "reloaded": "mm2src/foo/src/bar.rs", "reference": "mm2src/foo/src/bar.rs" }
      ]
    }

  Legacy list form -- each "reference" is used verbatim and resolved against the
  local repo:
    [
      {
        "reloaded": "mm2src/foo/src/bar.rs",
        "reference": "git:c1d46c0c1592faa0860f704008b2b2381bc3840f:mm2src/foo/src/bar.rs",
        "margin": 0.15,          // optional override of --margin
        "max_discretionary": 0.50 // optional absolute cap override
      }
    ]

Reference repository resolution (object form), in order: --reference-repo, the
CRD_GATE_REFERENCE_REPO env var, manifest 'local_clones' hints (each used only
if it already contains the pinned commit), else a shallow clone of the upstream
into --cache-dir (default under ~/.cache), outside the clean checkout.

Exit status is non-zero if any gated file fails, so the tool can run in CI.
"""
from __future__ import annotations

import argparse
import difflib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass, field

PIN_LINE = re.compile(r"(?://|#)\s*crd:(pin|r2[89]|r3[0-3])\b", re.IGNORECASE)
PIN_BEGIN = re.compile(r"(?://|#)\s*crd:pin-begin\b", re.IGNORECASE)
PIN_END = re.compile(r"(?://|#)\s*crd:pin-end\b", re.IGNORECASE)

# Conservative structural interface patterns used only with --auto-pin.
AUTO_PIN = [
    re.compile(r"^\s*(pub\s+)?use\s+"),
    re.compile(r"^\s*#\[repr\("),
    re.compile(r'rename\s*=\s*"'),
    re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?(const\s+)?(unsafe\s+)?(async\s+)?fn\s+\w+"),
    re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?(struct|enum|trait|union|type)\s+\w+"),
    re.compile(r"^\s*impl(\s|<)"),
]

# Line/character sequences treated as comments to strip before scoring. Kept
# deliberately simple and language-agnostic for the line-oriented normalizer.
LINE_COMMENT = re.compile(r"(//|#).*$")


@dataclass
class FileResult:
    reloaded: str
    reference: str
    whole_file_sim: float
    discretionary_sim: float
    margin_required: float
    max_discretionary: float
    margin_floor: float = 0.50
    interface_only: bool = False
    residual_identical: list[str] = field(default_factory=list)
    excluded_lines: int = 0
    measured_lines: int = 0
    error: str | None = None

    @property
    def margin_applies(self) -> bool:
        """The body-vs-whole margin sub-test is only meaningful when overall
        similarity is high enough to leave headroom. Below ``margin_floor`` a
        file cannot be ``margin`` points below an already-low whole-file score,
        so the test only produces false failures on genuinely low-overlap
        files; there the absolute discretionary cap governs alone."""
        return self.whole_file_sim >= self.margin_floor

    @property
    def passed(self) -> bool:
        if self.error is not None:
            return False
        if self.interface_only or self.measured_lines == 0:
            # Interface-only / re-export / module-glue file: nothing
            # discretionary to measure or it is identical by necessity.
            return True
        under_cap = self.discretionary_sim <= self.max_discretionary
        below_whole = (
            self.discretionary_sim <= self.whole_file_sim - self.margin_required
            if self.margin_applies
            else True
        )
        return under_cap and below_whole


def read_source(spec: str, repo_root: str, git_repo: str | None = None) -> str:
    """Read a file by path or by `git:<ref>:<path>` spec.

    Plain paths resolve against ``repo_root`` (the clean checkout). ``git:``
    specs resolve against ``git_repo`` when provided (the gated reference
    repository), otherwise against ``repo_root`` for backwards compatibility.
    """
    if spec.startswith("git:"):
        _, ref, path = spec.split(":", 2)
        src_repo = git_repo or repo_root
        out = subprocess.run(
            ["git", "-C", src_repo, "show", f"{ref}:{path}"],
            capture_output=True,
            text=True,
        )
        if out.returncode != 0:
            raise FileNotFoundError(f"git show {ref}:{path} failed: {out.stderr.strip()}")
        return out.stdout
    abspath = spec if os.path.isabs(spec) else os.path.join(repo_root, spec)
    with open(abspath, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def normalize(line: str) -> str:
    """Strip line comments and collapse whitespace for scoring."""
    line = LINE_COMMENT.sub("", line)
    return re.sub(r"\s+", " ", line).strip()


def split_excluded(text: str, auto_pin: bool) -> tuple[list[str], list[str]]:
    """Return (measured_lines, excluded_lines) after normalization.

    Excluded lines are the pinned-interface / R28-R33 lines that R35 removes
    from the measured set. Blank and pure-comment lines are dropped entirely.
    """
    measured: list[str] = []
    excluded: list[str] = []
    in_block = False
    for raw in text.splitlines():
        if PIN_BEGIN.search(raw):
            in_block = True
            continue
        if PIN_END.search(raw):
            in_block = False
            continue
        norm = normalize(raw)
        if not norm:
            continue
        pinned = in_block or bool(PIN_LINE.search(raw))
        if not pinned and auto_pin:
            pinned = any(p.search(raw) for p in AUTO_PIN)
        (excluded if pinned else measured).append(norm)
    return measured, excluded


def ratio(a: list[str], b: list[str]) -> float:
    return difflib.SequenceMatcher(None, a, b, autojunk=False).ratio()


def residual_identical_lines(measured_a: list[str], measured_b: list[str]) -> list[str]:
    """Discretionary lines that are byte-identical between the two measured sets.

    Per R35 each such line must be individually attributable to an excluded
    fragment; this list is surfaced for the reviewer to confirm or reject.
    """
    set_b = set(measured_b)
    seen: set[str] = set()
    out: list[str] = []
    for line in measured_a:
        if line in set_b and line not in seen:
            seen.add(line)
            out.append(line)
    return out


def gate_file(
    reloaded: str,
    reference: str,
    repo_root: str,
    margin: float,
    max_discretionary: float,
    auto_pin: bool,
    reference_repo: str | None = None,
    margin_floor: float = 0.50,
    interface_only: bool = False,
) -> FileResult:
    try:
        rel_text = read_source(reloaded, repo_root)
        ref_text = read_source(reference, repo_root, git_repo=reference_repo)
    except (FileNotFoundError, OSError) as exc:
        return FileResult(
            reloaded,
            reference,
            0.0,
            0.0,
            margin,
            max_discretionary,
            margin_floor=margin_floor,
            interface_only=interface_only,
            error=str(exc),
        )

    rel_all = [normalize(l) for l in rel_text.splitlines() if normalize(l)]
    ref_all = [normalize(l) for l in ref_text.splitlines() if normalize(l)]
    whole = ratio(rel_all, ref_all)

    rel_measured, rel_excluded = split_excluded(rel_text, auto_pin)
    ref_measured, _ = split_excluded(ref_text, auto_pin)
    disc = ratio(rel_measured, ref_measured) if rel_measured and ref_measured else 0.0

    return FileResult(
        reloaded=reloaded,
        reference=reference,
        whole_file_sim=whole,
        discretionary_sim=disc,
        margin_required=margin,
        max_discretionary=max_discretionary,
        margin_floor=margin_floor,
        interface_only=interface_only,
        residual_identical=residual_identical_lines(rel_measured, ref_measured),
        excluded_lines=len(rel_excluded),
        measured_lines=len(rel_measured),
    )


def load_manifest(path: str) -> tuple[dict | None, dict, list[dict]]:
    """Load a manifest, supporting the legacy flat-list form and the object form.

    Returns ``(origin, defaults, entries)``. For the legacy list form ``origin``
    is ``None``, ``defaults`` is empty, and each entry's ``reference`` is used
    verbatim. For the object form each file's ``reference`` is an upstream PATH
    that the caller expands to ``git:<origin.commit>:<path>``.
    """
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    if isinstance(data, list):
        return None, {}, data
    if isinstance(data, dict):
        defaults = data.get("defaults", {})
        entries = data.get("files", [])
        if not isinstance(entries, list):
            raise ValueError("manifest 'files' must be a JSON list")
        return data.get("reference_origin"), defaults, entries
    raise ValueError("manifest must be a JSON list or object")


DEFAULT_CACHE = os.path.join(
    os.environ.get("XDG_CACHE_HOME", os.path.expanduser("~/.cache")),
    "crd-clean-room-gate",
)


def _git(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(args, capture_output=True, text=True)


def repo_has_commit(repo: str, commit: str) -> bool:
    """True if ``commit`` resolves to a commit object inside ``repo``."""
    if not repo or not os.path.isdir(repo):
        return False
    return _git(["git", "-C", repo, "cat-file", "-e", f"{commit}^{{commit}}"]).returncode == 0


def _provision_clone(url: str, commit: str, branch: str | None, dest: str) -> None:
    """Materialise a shallow clone of ``url`` holding ``commit`` at ``dest``.

    Tries to fetch the exact commit first; falls back to the pinned branch tip
    (sufficient when the commit is the branch tip). The tree is only ever read
    via ``git show`` afterwards -- nothing is checked out into the clean repo.
    """
    os.makedirs(dest, exist_ok=True)
    _git(["git", "-C", dest, "init", "-q"])
    if _git(["git", "-C", dest, "remote", "get-url", "origin"]).returncode != 0:
        _git(["git", "-C", dest, "remote", "add", "origin", url])
    if _git(["git", "-C", dest, "fetch", "--depth", "1", "origin", commit]).returncode != 0 and branch:
        _git(["git", "-C", dest, "fetch", "--depth", "1", "origin", branch])


def resolve_reference_repo(origin: dict, explicit: str | None, cache_dir: str) -> str:
    """Return a git repo that contains ``origin['commit']`` (clean-room access).

    Resolution order: an explicit ``--reference-repo``/``CRD_GATE_REFERENCE_REPO``
    (e.g. a local corpus clone), then manifest ``local_clones`` hints, then a
    shallow clone of ``origin['url']`` under ``cache_dir`` -- kept outside the
    clean checkout so the upstream tree is never mixed with code under review.
    """
    commit = origin["commit"]
    candidates = [explicit, os.environ.get("CRD_GATE_REFERENCE_REPO"), *origin.get("local_clones", [])]
    for cand in candidates:
        if cand:
            expanded = os.path.expanduser(cand)
            if repo_has_commit(expanded, commit):
                return expanded

    dest = os.path.join(cache_dir, commit)
    if repo_has_commit(dest, commit):
        return dest

    url = origin.get("url")
    if not url:
        raise FileNotFoundError(
            f"commit {commit} not found in any reference repo and no upstream 'url' to clone"
        )
    _provision_clone(url, commit, origin.get("branch"), dest)
    if not repo_has_commit(dest, commit):
        raise FileNotFoundError(f"failed to provision reference repo for {commit} from {url}")
    return dest


def print_result(r: FileResult, verbose: bool) -> None:
    status = "PASS" if r.passed else "FAIL"
    if r.error:
        print(f"[{status}] {r.reloaded}: ERROR: {r.error}")
        return
    if r.interface_only or r.measured_lines == 0:
        margin_note = "interface-only: cap/margin waived"
    elif r.margin_applies:
        margin_note = f"must be >= {r.margin_required:.0%} below whole-file"
    else:
        margin_note = f"margin n/a below {r.margin_floor:.0%} whole-file"
    flag = " [interface-only]" if (r.interface_only or r.measured_lines == 0) else ""
    print(
        f"[{status}] {r.reloaded}{flag}\n"
        f"        whole-file similarity   : {r.whole_file_sim:6.1%}\n"
        f"        discretionary similarity: {r.discretionary_sim:6.1%} "
        f"(cap {r.max_discretionary:.0%}, {margin_note})\n"
        f"        excluded lines          : {r.excluded_lines}\n"
        f"        measured lines          : {r.measured_lines}\n"
        f"        residual identical body : {len(r.residual_identical)}"
    )
    if verbose and r.residual_identical:
        for line in r.residual_identical:
            print(f"            ~ {line}")


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Clean-room residual-similarity gate (R35/R36).")
    ap.add_argument("--manifest", help="path to a JSON manifest of gated files")
    ap.add_argument("--reloaded", help="single reloaded file (with --reference)")
    ap.add_argument("--reference", help="single reference spec (path or git:<ref>:<path>)")
    ap.add_argument("--repo-root", default=None, help="repository root (default: git toplevel)")
    ap.add_argument(
        "--margin",
        type=float,
        default=0.15,
        help="minimum gap (0-1) discretionary similarity must sit below whole-file similarity",
    )
    ap.add_argument(
        "--max-discretionary",
        type=float,
        default=0.60,
        help="absolute cap (0-1) on discretionary-body similarity",
    )
    ap.add_argument(
        "--margin-floor",
        type=float,
        default=0.50,
        help="whole-file similarity (0-1) below which the body-vs-whole margin sub-test is skipped",
    )
    ap.add_argument("--auto-pin", action="store_true", help="also exclude structural interface lines")
    ap.add_argument(
        "--reference-repo",
        default=None,
        help="path to a gated repo holding the pinned upstream commit; overrides CRD_GATE_REFERENCE_REPO",
    )
    ap.add_argument(
        "--cache-dir",
        default=None,
        help=f"where to shallow-clone the upstream when no local reference repo is found (default: {DEFAULT_CACHE})",
    )
    ap.add_argument("-v", "--verbose", action="store_true", help="list residual identical body lines")
    args = ap.parse_args(argv)

    repo_root = args.repo_root
    if repo_root is None:
        top = subprocess.run(
            ["git", "rev-parse", "--show-toplevel"], capture_output=True, text=True
        )
        repo_root = top.stdout.strip() if top.returncode == 0 else os.getcwd()

    if not args.manifest and not (args.reloaded and args.reference):
        ap.error("provide --manifest or both --reloaded and --reference")

    entries: list[dict] = []
    origin: dict | None = None
    manifest_defaults: dict = {}
    if args.manifest:
        origin, manifest_defaults, manifest_entries = load_manifest(args.manifest)
        entries.extend(manifest_entries)
    if args.reloaded and args.reference:
        entries.append({"reloaded": args.reloaded, "reference": args.reference})

    if not entries:
        print("No gated files in manifest; R35 gate is a no-op.")
        return 0

    # Resolve the gated reference repository when an upstream origin is pinned,
    # then expand each upstream PATH into a git:<commit>:<path> spec.
    reference_repo = args.reference_repo
    if origin is not None:
        try:
            reference_repo = resolve_reference_repo(
                origin, args.reference_repo, args.cache_dir or DEFAULT_CACHE
            )
        except (FileNotFoundError, OSError) as exc:
            print(f"ERROR: could not resolve reference repository: {exc}", file=sys.stderr)
            return 2
        commit = origin["commit"]
        for entry in entries:
            ref = entry.get("reference", entry["reloaded"])
            if not ref.startswith("git:"):
                entry["reference"] = f"git:{commit}:{ref}"

    margin_default = float(manifest_defaults.get("margin", args.margin))
    maxd_default = float(manifest_defaults.get("max_discretionary", args.max_discretionary))
    floor_default = float(manifest_defaults.get("margin_floor", args.margin_floor))

    results: list[FileResult] = []
    for entry in entries:
        results.append(
            gate_file(
                reloaded=entry["reloaded"],
                reference=entry["reference"],
                repo_root=repo_root,
                margin=float(entry.get("margin", margin_default)),
                max_discretionary=float(entry.get("max_discretionary", maxd_default)),
                auto_pin=args.auto_pin,
                reference_repo=reference_repo,
                margin_floor=float(entry.get("margin_floor", floor_default)),
                interface_only=bool(entry.get("interface_only", False)),
            )
        )

    failed = 0
    for r in results:
        print_result(r, args.verbose)
        if not r.passed:
            failed += 1

    print(f"\n{len(results) - failed}/{len(results)} files passed the R35 gate.")
    if failed:
        print(
            "Note: a FAIL is not necessarily copying. R35 requires that each residual "
            "identical discretionary line be attributable to an R28-R33 fragment or a "
            "pinned-interface line; mark such lines with `crd:pin` / `crd:r2x` and re-run."
        )
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
