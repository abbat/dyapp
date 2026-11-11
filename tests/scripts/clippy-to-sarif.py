"""Convert cargo compiler-message JSON records to a valid SARIF document.

With --budget, complexity findings of functions still within their committed
legacy budget are left out, so code scanning shows only regressions.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING

import rust_complexity as policy

if TYPE_CHECKING:
    import quality_contracts as qc

Functions = dict[str, list["qc.ParserFunction"]]

ROOT = Path(__file__).resolve().parents[2]


def parse_functions(paths: list[str]) -> Functions:
    run = subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "--manifest-path",
            str(ROOT / "tools/rust-functions/Cargo.toml"),
            "--",
            *paths,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if run.returncode:
        print(run.stderr, file=sys.stderr)
        return {}
    return {
        item["path"]: item["functions"]
        for item in map(json.loads, run.stdout.splitlines())
    }


def load_functions(paths: list[str], functions_file: Path | None) -> Functions:
    if not functions_file:
        return parse_functions(paths) if paths else {}
    return {
        item["path"]: item["functions"]
        for item in map(json.loads, functions_file.read_text().splitlines())
    }


def read_sources(paths: list[str]) -> dict[str, bytes]:
    sources = {}
    for path in paths:
        try:
            sources[path] = Path(path).read_bytes()
        except OSError:
            pass
    return sources


def merge(measurements: list[dict[str, int]]) -> dict[str, int]:
    merged: dict[str, int] = {}
    for values in measurements:
        for metric, value in values.items():
            if metric == "nested_regions":
                merged[metric] = merged.get(metric, 0) + value
            else:
                merged[metric] = max(merged.get(metric, 0), value)
    return merged


def within_budget(
    findings: dict[int, qc.ComplexityFinding],
    budget: Path,
    functions_file: Path | None,
) -> set[int]:
    """Return indexes of findings whose function stays within its budget."""
    allowed = {
        (row["path"], row["function"]): row["metrics"]
        for row in json.loads(budget.read_text())
    }
    paths = sorted({item["span"]["file_name"] for item in findings.values()})
    functions = load_functions(paths, functions_file)
    sources = read_sources(paths)
    groups: dict[tuple[str, str], dict[int, dict[str, int]]] = {}
    for index, finding in findings.items():
        try:
            ((key, values),) = policy.measure(
                [finding], sources, functions
            ).items()
        except (ValueError, KeyError):
            continue  # Unmatched findings stay visible.
        groups.setdefault(key, {})[index] = values
    result: set[int] = set()
    for key, items in groups.items():
        limits = allowed.get(key, {})
        merged = merge(list(items.values()))
        if all(
            value <= limits.get(metric, 0) for metric, value in merged.items()
        ):
            result.update(items)
    return result


def rule_id(message: qc.CompilerMessage) -> str:
    code = message.get("code")
    return code.get("code", "clippy") if code else "clippy"


def sarif_result(
    code: str, message: qc.CompilerMessage, spans: list[qc.CompilerSpan]
) -> dict[str, object]:
    result: dict[str, object] = {
        "ruleId": code,
        "level": message["level"],
        "message": {"text": message["message"]},
    }
    if spans:
        span = spans[0]
        result["locations"] = [
            {
                "physicalLocation": {
                    "artifactLocation": {"uri": span["file_name"]},
                    "region": {
                        "startLine": max(1, span["line_start"]),
                        "startColumn": max(1, span["column_start"]),
                    },
                }
            }
        ]
    return result


def complexity_finding(
    code: str, spans: list[qc.CompilerSpan], text: str
) -> qc.ComplexityFinding | None:
    if (
        code not in policy.LINTS
        or len(spans) != 1
        or "byte_start" not in spans[0]
    ):
        return None
    # Crates outside the root report paths such as a/../tests/x.rs.
    span = spans[0].copy()
    span["file_name"] = os.path.normpath(span["file_name"])
    return {"code": code, "span": span, "message": text}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("records", type=Path)
    parser.add_argument("--budget", type=Path)
    parser.add_argument("--functions", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    results: list[dict[str, object]] = []
    complexity: dict[int, qc.ComplexityFinding] = {}
    seen: set[str] = set()
    measured: set[tuple[str, str, int]] = set()
    finished = None
    for line in args.records.read_text().splitlines():
        item = json.loads(line)
        if item.get("reason") == "build-finished":
            finished = item["success"]
        if item.get("reason") != "compiler-message":
            continue
        message = item["message"]
        if message["level"] not in ("warning", "error"):
            continue
        spans = [span for span in message["spans"] if span["is_primary"]]
        code = rule_id(message)
        result = sarif_result(code, message, spans)
        # Cargo reports one source diagnostic for library and test targets.
        fingerprint = json.dumps(result, sort_keys=True)
        if fingerprint in seen:
            continue
        seen.add(fingerprint)
        finding = complexity_finding(code, spans, message["message"])
        if finding:
            span = finding["span"]
            diagnostic = (
                finding["code"],
                span["file_name"],
                span["byte_start"],
            )
            if diagnostic not in measured:
                measured.add(diagnostic)
                complexity[len(results)] = finding
        results.append(result)
    if finished is not True:
        raise RuntimeError("Collector failed or did not finish")
    if args.budget and complexity:
        hidden = within_budget(complexity, args.budget, args.functions)
        results = [r for i, r in enumerate(results) if i not in hidden]
    print(
        json.dumps(
            {
                "version": "2.1.0",
                "runs": [
                    {
                        "tool": {"driver": {"name": "clippy"}},
                        "results": results,
                    }
                ],
            }
        )
    )


if __name__ == "__main__":
    main()
