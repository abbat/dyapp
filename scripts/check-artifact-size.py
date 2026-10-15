"""D5: compare a release artifact against its measured environment baseline."""
import argparse
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def inventory(path):
    entries = json.loads(path.read_text())
    if not isinstance(entries, dict) or not entries:
        raise ValueError("Artifact inventory must be a nonempty object")
    for name, entry in entries.items():
        if not isinstance(entry, dict):
            raise ValueError(f"{name}: invalid baseline record")
        size = entry.get("bytes")
        if type(size) is not int or size <= 0:
            raise ValueError(f"{name}: expected positive integer bytes")
        if not isinstance(entry.get("environment"), str):
            raise ValueError(f"{name}: environment is required")
        if not entry["environment"].strip():
            raise ValueError(f"{name}: environment is empty")
        if not re.fullmatch(r"[0-9a-f]{40}", str(entry.get("revision", ""))):
            raise ValueError(f"{name}: full measured source revision required")
    return entries


def exceptions(base, head="HEAD", root=ROOT):
    def git(*args):
        return subprocess.check_output(
            ["git", "-c", f"safe.directory={root}", *args],
            cwd=root, text=True, stderr=subprocess.PIPE).strip()

    if git("rev-parse", "--is-shallow-repository") != "false":
        raise ValueError("Artifact sizes require full Git history")
    git("merge-base", "--is-ancestor", base, head)
    text = git("log", "--format=%(trailers:key=Size-Increase,valueonly)",
               f"{base}..{head}")
    return [line.strip() for line in text.splitlines() if line.strip()]


def measure(name, artifact, entries, reasons):
    if name not in entries:
        raise ValueError(f"Unregistered artifact: {name}")
    if not artifact.is_file() or artifact.stat().st_size == 0:
        raise ValueError(f"Missing or empty artifact: {artifact}")
    baseline = entries[name]
    actual = artifact.stat().st_size
    overflow = actual * 100 > baseline["bytes"] * 105
    errors = []
    if overflow and not reasons:
        errors.append("Growth exceeds 5%; Size-Increase trailer required")
    return {"artifact": name, "path": str(artifact), "actual_bytes": actual,
            "baseline": baseline, "growth_percent": (
                100 * (actual - baseline["bytes"]) / baseline["bytes"]),
            "exception_reasons": reasons if overflow else [],
            "errors": errors}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--base", required=True,
                        help="ancestor starting the actual change range")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--inventory", type=Path,
                        default=ROOT / "scripts/artifact-sizes.json")
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    report = {"artifact": args.name, "errors": [],
              "base": args.base, "head": args.head}
    try:
        entries = inventory(args.inventory)
        reasons = exceptions(args.base, args.head)
        report.update(measure(args.name, args.artifact, entries, reasons))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        report["errors"].append(str(error))
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2) + "\n")
    if report["errors"]:
        print("FAIL artifact-size: size budget could not be satisfied")
        print("FIX: docs/development/ci-base.md#artifact-size-budgets-staged")
        print("DETAILS:")
        print("\n".join(report["errors"]))
        return 1
    print(f"OK artifact-size {args.name}")
    return 0


if __name__ == "__main__":
    from check_dispatch import wrap
    wrap("artifact-size")
    raise SystemExit(main())
