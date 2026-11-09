"""Gate workspace-unused public APIs while retaining explicitly marked code."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from quality_contracts import PublicLocation, PublicReport

ROOT = Path(__file__).resolve().parents[2]


def failures(report: PublicReport, status: int) -> list[str]:
    if report.get("schema_version") != 5:
        raise ValueError("Unsupported or missing Hawk report schema")
    summary = report["summary"]
    if not isinstance(summary.get("target"), str) or not summary["target"]:
        raise ValueError("Missing Hawk compilation target")
    diagnostics = report["diagnostics"]
    if not isinstance(diagnostics, list):
        raise ValueError("Missing Hawk diagnostic inventory")
    count = summary.get("diagnostic_count")
    if type(count) is not int or count != len(diagnostics):
        raise ValueError("Incomplete Hawk diagnostic inventory")
    if summary.get("includes_non_production_targets") is not True:
        raise ValueError("Hawk omitted tests and other non-production targets")
    if "all-features" not in summary.get("feature_profiles", []):
        raise ValueError("Hawk omitted the all-features profile")
    targets = {
        (item["package"], item.get("binary"), item.get("library"))
        for item in summary["production"]
    }
    required = {
        ("dyapp-bootstrap", "dyappd", None),
        ("dyapp-ffi", None, "dyapp_ffi"),
    }
    if not required <= targets:
        raise ValueError("Hawk omitted a required production target")
    errors = []
    for diagnostic in diagnostics:
        identity = diagnostic.get("identity", {})
        location: PublicLocation = diagnostic.get("location") or {}
        errors.append(
            f"{location.get('file', 'hawk.toml')}:"
            f"{location.get('line', 1)}: {diagnostic['code']}: "
            f"{identity.get('crate', 'configuration')}:"
            f"{identity.get('item', diagnostic.get('message', ''))}"
        )
    if status:
        errors.append(f"Hawk exited with status {status}")
    return errors


def main() -> "bool":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--report", type=Path, default=Path("build/unused-public.json")
    )
    args = parser.parse_args()
    report = (ROOT / args.report).resolve()
    report.parent.mkdir(parents=True, exist_ok=True)
    stderr = report.with_suffix(".stderr")
    errors = []
    try:
        with report.open("w") as output, stderr.open("w") as error_output:
            result = subprocess.run(
                [
                    "cargo",
                    "hawk",
                    "check",
                    "--manifest-path",
                    str(ROOT / "Cargo.toml"),
                    "--config",
                    str(ROOT / "hawk.toml"),
                    "--only",
                    "dead-public",
                    "--output-format=json",
                    "-D",
                    "warnings",
                ],
                cwd=ROOT,
                stdout=output,
                stderr=error_output,
                check=False,
            )
        errors = failures(json.loads(report.read_text()), result.returncode)
    except (ValueError, KeyError, TypeError, AttributeError, OSError) as error:
        errors.append(f"Invalid or incomplete Hawk audit: {error}")
    if errors and stderr.exists():
        selected = [
            line
            for line in stderr.read_text().splitlines()
            if re.match(r"^(error|help:|Caused by:)", line)
        ]
        errors.extend(selected[-8:])
        errors.append(f"Raw Hawk report: {report}; compiler log: {stderr}")
    print(json.dumps({"errors": errors}))
    return bool(errors)


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("unused-public")
    raise SystemExit(main())
