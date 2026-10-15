#!/usr/bin/env python3
"""Practice B1: coverage ratchet over a complete LLVM coverage report.

Usage: check_coverage.py COVERAGE.json [COVERAGE.lcov]
Total line coverage must not drop below the baseline in baselines.toml.
With the lcov report, lines changed since COVERAGE_BASE (default
origin/main; the working tree is compared) must be >= 80% covered.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
BASELINES = ROOT / "scripts/baselines.toml"
MIN_DIFF = 80
CRATES = (
    "bootstrap", "ffi", "identity", "messaging", "p2p-net", "profile", "video",
)
HUNK = re.compile(r"^@@ -\S+ \+(\d+)(?:,(\d+))? @@")


def validate(report, baseline):
    if report.get("type") != "llvm.coverage.json.export":
        raise ValueError("Not an LLVM coverage JSON export")
    data = report.get("data")
    if not isinstance(data, list) or len(data) != 1:
        raise ValueError("Missing or ambiguous coverage dataset")
    files = data[0].get("files")
    if not isinstance(files, list) or not files:
        raise ValueError("Empty coverage file inventory")
    paths = [item["filename"].replace("\\", "/") for item in files]
    missing = [crate for crate in CRATES
               if not any(f"/rust/{crate}/src/" in path for path in paths)]
    if missing:
        raise ValueError(f"Partial workspace coverage: missing {missing}")
    lines = data[0]["totals"]["lines"]
    count, covered = lines["count"], lines["covered"]
    if type(count) is not int or type(covered) is not int:
        raise ValueError("Invalid covered line counts")
    if count <= 0 or not 0 <= covered <= count:
        raise ValueError("Empty or invalid covered line counts")
    percentage = covered * 100 / count
    if percentage < baseline:
        raise ValueError(f"Line coverage {percentage:.2f}% < baseline "
                         f"{baseline}% (scripts/baselines.toml)")
    return percentage


def changed(diff):
    """{repository path: new-side line numbers} of a `git diff -U0`."""
    result, path = {}, None
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[len("+++ b/"):] if line != "+++ /dev/null" else None
        elif path and (hunk := HUNK.match(line)):
            start, length = int(hunk.group(1)), int(hunk.group(2) or 1)
            result.setdefault(path, set()).update(
                range(start, start + length))
    return result


def instrumented(lcov, root):
    """{repository path: {line: hits}} of an lcov report."""
    result, lines = {}, None
    for line in lcov.splitlines():
        if line.startswith("SF:"):
            path = Path(line[3:])
            lines = result.setdefault(
                str(path.relative_to(root)) if path.is_relative_to(root)
                else str(path), {})
        elif line.startswith("DA:") and lines is not None:
            number, hits = line[3:].split(",")[:2]
            lines[int(number)] = int(hits)
    return result


def diff_coverage(changes, lines):
    """(covered, instrumented) changed lines; missed as path:line."""
    covered, missed = 0, []
    for path, numbers in sorted(changes.items()):
        for number in sorted(numbers & lines.get(path, {}).keys()):
            if lines[path][number]:
                covered += 1
            else:
                missed.append(f"{path}:{number}")
    return covered, covered + len(missed), missed


def check_diff(lcov):
    base = os.environ.get("COVERAGE_BASE") or "origin/main"
    if set(base) == {"0"}:  # a push that creates a branch
        base = "origin/main"
    diff = subprocess.check_output(
        ["git", "-C", str(ROOT), "-c", f"safe.directory={ROOT}", "diff",
         "-U0", "--no-renames", "--no-color", "--no-ext-diff", base, "--",
         "*.rs"], text=True)
    covered, total, missed = diff_coverage(
        changed(diff), instrumented(lcov, ROOT))
    if covered * 100 < MIN_DIFF * total:
        raise ValueError(
            f"Changed-line coverage {covered}/{total} < {MIN_DIFF}% since "
            f"{base}; uncovered (first 50):\n" + "\n".join(missed[:50]))
    return covered, total


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("coverage")
    baseline = tomllib.loads(BASELINES.read_text())["coverage"]["lines"]
    percentage = validate(json.loads(Path(sys.argv[1]).read_text()), baseline)
    print(f"Complete Rust workspace: {percentage:.2f}% line coverage "
          f"(baseline {baseline}%)")
    if percentage >= baseline + 0.5:
        print(f"Raise [coverage] lines in scripts/baselines.toml to "
              f"{int(percentage * 10) / 10}")
    if len(sys.argv) > 2:
        covered, total = check_diff(Path(sys.argv[2]).read_text())
        print(f"Changed lines: {covered}/{total} covered")
