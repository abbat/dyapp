"""Record measured artifact bytes and provenance, without claiming a budget."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def file_record(name, artifact, environment):
    if not artifact.is_file() or artifact.stat().st_size == 0:
        raise ValueError(f"Missing or empty artifact: {artifact}")
    digest = hashlib.sha256()
    with artifact.open("rb") as stream:
        while chunk := stream.read(1024 * 1024):
            digest.update(chunk)
    sha = digest.hexdigest()
    return {"artifact": name, "path": str(artifact.resolve()),
            "bytes": artifact.stat().st_size, "sha256": sha,
            "environment": environment}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("name")
    parser.add_argument("artifact", type=Path)
    parser.add_argument("--environment", required=True)
    parser.add_argument("--report", required=True, type=Path)
    args = parser.parse_args()
    result = {"artifact": args.name, "errors": []}
    try:
        result.update(file_record(args.name, args.artifact, args.environment))
        result["revision"] = subprocess.check_output(
            ["git", "-c", f"safe.directory={ROOT}", "rev-parse", "HEAD"],
            cwd=ROOT, text=True, stderr=subprocess.PIPE).strip()
        result["rustc"] = subprocess.check_output(
            ["rustc", "-Vv"], text=True, stderr=subprocess.PIPE).strip()
        result["platform"] = platform.platform()
        result["machine"] = platform.machine()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        result["errors"].append(str(error))
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2) + "\n")
    if result["errors"]:
        print("FAIL artifact-measurement: could not collect release evidence")
        print("FIX: docs/development/ci-base.md#artifact-size-budgets-staged")
        print("DETAILS:")
        print("\n".join(result["errors"]))
        return 1
    print(f"OK artifact-measurement {args.name}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
