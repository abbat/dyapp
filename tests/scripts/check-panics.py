"""Ratchet panic and integer-overflow lints (B8)."""

from __future__ import annotations

import argparse
from collections.abc import Iterable, Mapping
from collections import Counter
import json
from pathlib import Path
import subprocess
import tomllib
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import quality_contracts as qc

ROOT = Path(__file__).resolve().parents[2]
# One baselines.toml table per kind, so one kind cannot pay for another.
RATCHETS = {
    "panics": ("clippy::unwrap_used", "clippy::expect_used", "clippy::panic"),
    "arithmetic": ("clippy::arithmetic_side_effects",),
}
LINTS = tuple(lint for lints in RATCHETS.values() for lint in lints)


def count(
    metadata: qc.CargoMetadata,
    messages: Iterable[qc.CompilerRecord],
    lints: tuple[str, ...] = RATCHETS["panics"],
) -> tuple[dict[str, int], list[qc.PanicLocation], dict[str, int]]:
    packages = {
        p["id"]: p["name"]
        for p in metadata["packages"]
        if p["id"] in metadata["workspace_members"]
    }
    counts = Counter(dict.fromkeys(packages.values(), 0))
    seen: set[tuple[str, str, tuple[tuple[str, int, int], ...]]] = set()
    locations: list[qc.PanicLocation] = []
    excluded: Counter[str] = Counter()
    finished = False
    target = Path(metadata["target_directory"]).resolve()
    for item in messages:
        if item["reason"] == "build-finished":
            finished = item["success"]
        message = item.get("message", {})
        code = (message.get("code") or {}).get("code")
        package_id = item.get("package_id")
        name = packages.get(package_id) if package_id is not None else None
        if not name or code is None or code not in lints:
            continue
        spans = []
        for span in message["spans"]:
            path = (ROOT / span["file_name"]).resolve()
            if all(
                (
                    span["is_primary"],
                    path.is_relative_to(ROOT),
                    not path.is_relative_to(target),
                )
            ):
                spans.append(
                    (
                        str(path.relative_to(ROOT)),
                        span["line_start"],
                        span["column_start"],
                    )
                )
        if not spans:
            excluded[name] += 1
            continue
        key = name, code, tuple(sorted(spans))
        if key not in seen:
            seen.add(key)
            counts[name] += 1
            locations.append({"package": name, "lint": code, "spans": spans})
    if not finished or not packages:
        raise ValueError("Missing successful Clippy build/workspace inventory")
    return dict(counts), locations, dict(excluded)


def errors(
    counts: Mapping[str, int],
    budgets: Mapping[str, int],
    section: str = "panics",
) -> list[str]:
    result = []
    if section == "panics" and budgets.get("dyapp-ffi") != 0:
        result.append("FFI panic budget must stay zero")
    for name in sorted(counts.keys() | budgets.keys()):
        if name not in counts or name not in budgets:
            result.append(
                f"{section} {name}: missing workspace member or budget"
            )
        elif counts[name] > budgets[name]:
            result.append(
                f"{section} {name}: {counts[name]} > {budgets[name]}"
                " lint diagnostics"
            )
        elif counts[name] < budgets[name]:
            result.append(
                f"{section} {name}: lower lint budget to {counts[name]}"
            )
    return result


def main() -> bool:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    args.report = args.report or Path("build") / "panics.json"
    result: qc.PanicReport = {"errors": []}
    try:
        metadata: qc.CargoMetadata = json.loads(
            subprocess.check_output(
                [
                    "cargo",
                    "metadata",
                    "--no-deps",
                    "--locked",
                    "--format-version=1",
                ],
                cwd=ROOT,
                text=True,
            )
        )
        flags = [flag for lint in LINTS for flag in ("--force-warn", lint)]
        run = subprocess.run(
            [
                "cargo",
                "clippy",
                "--workspace",
                "--all-features",
                "--lib",
                "--bins",
                "--locked",
                "--message-format=json",
                "--",
                *flags,
            ],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        if run.returncode:
            raise ValueError("Clippy failed: " + run.stderr[-4096:])
        messages = [json.loads(line) for line in run.stdout.splitlines()]
        policy = tomllib.loads(
            (ROOT / "tests/scripts/baselines.toml").read_text()
        )
        result["counts"] = {}
        result["budgets"] = {}
        result["diagnostics"] = []
        result["excluded_diagnostics"] = {}
        for section, lints in RATCHETS.items():
            counts, locations, excluded = count(metadata, messages, lints)
            budgets = policy[section]
            result["counts"][section] = counts
            result["budgets"][section] = budgets
            result["diagnostics"] += locations
            result["excluded_diagnostics"][section] = excluded
            result["errors"] += errors(counts, budgets, section)
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        result["errors"].append(str(error))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2) + "\n")
    summary = {"counts": result.get("counts"), "errors": result["errors"]}
    print(json.dumps(summary))
    return bool(result["errors"])


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("panics")
    raise SystemExit(main())
