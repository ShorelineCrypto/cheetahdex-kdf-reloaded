#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Clean-room residual-similarity gate (R35 / R36).

This tool implements the residual-similarity gate that chapter 01 (rule R35,
deferral D1) and chapter 30 of the clean-room rewrite documents defer to an
automated check. It does NOT touch the forbidden corpus (R8). It compares a
reloaded source file against a *relicensed historical record* reference (an
allowed input: the GPLv2-anchored base or another permitted baseline) and
scores similarity over *discretionary expression only*.

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

Manifest entries (JSON list):
    [
      {
        "reloaded": "mm2src/foo/src/bar.rs",
        "reference": "git:c1d46c0c1592faa0860f704008b2b2381bc3840f:mm2src/foo/src/bar.rs",
        "margin": 0.15,          // optional override of --margin
        "max_discretionary": 0.50 // optional absolute cap override
      }
    ]

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
    re.compile(r"^\s*(pub(\s*\([^)]*\))?\s+)?(unsafe\s+)?(async\s+)?fn\s+\w+"),
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
    residual_identical: list[str] = field(default_factory=list)
    excluded_lines: int = 0
    measured_lines: int = 0
    error: str | None = None

    @property
    def passed(self) -> bool:
        if self.error is not None:
            return False
        if self.measured_lines == 0:
            # Nothing discretionary to measure (interface-only file): pass.
            return True
        below_whole = self.discretionary_sim <= self.whole_file_sim - self.margin_required
        under_cap = self.discretionary_sim <= self.max_discretionary
        return below_whole and under_cap


def read_source(spec: str, repo_root: str) -> str:
    """Read a file by path or by `git:<ref>:<path>` spec."""
    if spec.startswith("git:"):
        _, ref, path = spec.split(":", 2)
        out = subprocess.run(
            ["git", "-C", repo_root, "show", f"{ref}:{path}"],
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
) -> FileResult:
    try:
        rel_text = read_source(reloaded, repo_root)
        ref_text = read_source(reference, repo_root)
    except (FileNotFoundError, OSError) as exc:
        return FileResult(reloaded, reference, 0.0, 0.0, margin, max_discretionary, error=str(exc))

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
        residual_identical=residual_identical_lines(rel_measured, ref_measured),
        excluded_lines=len(rel_excluded),
        measured_lines=len(rel_measured),
    )


def load_manifest(path: str) -> list[dict]:
    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    if not isinstance(data, list):
        raise ValueError("manifest must be a JSON list of entries")
    return data


def print_result(r: FileResult, verbose: bool) -> None:
    status = "PASS" if r.passed else "FAIL"
    if r.error:
        print(f"[{status}] {r.reloaded}: ERROR: {r.error}")
        return
    print(
        f"[{status}] {r.reloaded}\n"
        f"        whole-file similarity   : {r.whole_file_sim:6.1%}\n"
        f"        discretionary similarity: {r.discretionary_sim:6.1%} "
        f"(cap {r.max_discretionary:.0%}, must be >= {r.margin_required:.0%} below whole-file)\n"
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
    ap.add_argument("--auto-pin", action="store_true", help="also exclude structural interface lines")
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
    if args.manifest:
        entries.extend(load_manifest(args.manifest))
    if args.reloaded and args.reference:
        entries.append({"reloaded": args.reloaded, "reference": args.reference})

    if not entries:
        print("No gated files in manifest; R35 gate is a no-op.")
        return 0

    results: list[FileResult] = []
    for entry in entries:
        results.append(
            gate_file(
                reloaded=entry["reloaded"],
                reference=entry["reference"],
                repo_root=repo_root,
                margin=float(entry.get("margin", args.margin)),
                max_discretionary=float(entry.get("max_discretionary", args.max_discretionary)),
                auto_pin=args.auto_pin,
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
