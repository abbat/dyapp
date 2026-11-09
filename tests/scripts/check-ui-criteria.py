#!/usr/bin/env python3
"""Reject new UI findings and stale accessibility/reflow baseline entries."""

from collections.abc import Collection, Mapping, Sequence
import json
from pathlib import Path
import subprocess
import sys
from typing import TypedDict


class Finding(TypedDict):
    rule: str
    element: str
    detail: str


BASELINE = Path(__file__).with_name("desktop-ui-baseline.json")


def findings(report: Mapping[str, Sequence[Finding]]) -> dict[str, str]:
    """Keep each state/language/width/theme/scale violation independent."""
    return {
        f"{shot} {finding['rule']} {finding['element']}": finding["detail"]
        for shot, rows in sorted(report.items())
        for finding in rows
    }


def errors(found: Mapping[str, str], known: Collection[str]) -> list[str]:
    """Only accessibility and clipping may use the initial baseline."""
    result = [
        f"new UI violation: {key}: {detail}"
        for key, detail in found.items()
        if key not in known
        or not key.split(" ", 2)[1].startswith(("axe:", "axe-review:", "T1:"))
    ]
    result.extend(
        f"fixed, remove from UI baseline: {key}"
        for key in sorted(known)
        if key not in found
    )
    return result


def baseline_growth(known: Collection[str]) -> list[str]:
    """Intersect committed budgets so manual CI cannot reset the ratchet."""
    shallow = subprocess.run(
        ["git", "rev-parse", "--is-shallow-repository"],
        check=True,
        capture_output=True,
        text=True,
    )
    if shallow.stdout.strip() != "false":
        raise ValueError("Full git history is required for the UI baseline")
    name = "tests/scripts/desktop-ui-baseline.json"
    history = subprocess.run(
        ["git", "log", "--format=%H", "--diff-filter=AM", "--", name],
        check=True,
        capture_output=True,
        text=True,
    )
    allowed = set(known)
    for revision in history.stdout.splitlines():
        previous = subprocess.run(
            ["git", "show", f"{revision}:{name}"],
            check=True,
            capture_output=True,
            text=True,
        )
        allowed.intersection_update(json.loads(previous.stdout)["findings"])
    return [
        f"UI baseline may only shrink: {key}"
        for key in sorted(set(known) - allowed)
    ]


def main(path: str) -> int:
    data = json.loads(Path(path).read_text())
    report = data["shots"]
    if set(data["expected"]) != set(report):
        raise ValueError(
            "UI criteria report has missing/unexpected measurements"
        )
    if not isinstance(report, dict) or not report:
        raise ValueError(
            "UI criteria report must contain screenshot measurements"
        )
    known = json.loads(BASELINE.read_text())["findings"]
    if not isinstance(known, list) or len(known) != len(set(known)):
        raise ValueError("UI baseline must contain unique finding keys")
    local_known = [key for key in known if key.split(" ", 1)[0] in report]
    problems = errors(findings(report), local_known) + baseline_growth(known)
    problems.extend(
        f"fixed, remove obsolete UI baseline state: {key}"
        for key in known
        if key.split(" ", 1)[0] not in data["all_shots"]
    )
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        return 1
    print(
        f"UI criteria: {len(report)} shots, {len(known)} known findings; "
        "no new violations"
    )
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        raise SystemExit("usage: check-ui-criteria.py <criteria.json>")
    sys.exit(main(sys.argv[1]))
