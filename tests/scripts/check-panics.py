"""Ratchet panic lints (B8)."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]
LINTS = ("clippy::unwrap_used", "clippy::expect_used", "clippy::panic")


def count(metadata, messages):
    packages = {p["id"]: p["name"] for p in metadata["packages"]
                if p["id"] in metadata["workspace_members"]}
    counts = Counter(dict.fromkeys(packages.values(), 0))
    seen, locations = set(), []
    excluded = Counter()
    finished = False
    target = Path(metadata["target_directory"]).resolve()
    for item in messages:
        if item["reason"] == "build-finished":
            finished = item["success"]
        message = item.get("message", {})
        code = (message.get("code") or {}).get("code")
        name = packages.get(item.get("package_id"))
        if not name or code not in LINTS:
            continue
        spans = []
        for span in message["spans"]:
            path = (ROOT / span["file_name"]).resolve()
            if all((span["is_primary"], path.is_relative_to(ROOT),
                    not path.is_relative_to(target))):
                spans.append((str(path.relative_to(ROOT)), span["line_start"],
                              span["column_start"]))
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


def errors(counts, budgets):
    result = []
    if budgets.get("dyapp-ffi") != 0:
        result.append("FFI panic budget must stay zero")
    for name in sorted(counts.keys() | budgets.keys()):
        if name not in counts or name not in budgets:
            result.append(f"{name}: missing workspace member or budget")
        elif counts[name] > budgets[name]:
            result.append(
                f"{name}: {counts[name]} > {budgets[name]} lint diagnostics")
        elif counts[name] < budgets[name]:
            result.append(f"{name}: lower lint budget to {counts[name]}")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    args.report = args.report or Path("build") / "panics.json"
    result = {"errors": []}
    try:
        metadata = json.loads(subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--locked",
             "--format-version=1"],
            cwd=ROOT, text=True))
        flags = [flag for lint in LINTS for flag in ("--force-warn", lint)]
        run = subprocess.run(
            ["cargo", "clippy", "--workspace", "--all-features",
             "--lib", "--bins",
             "--locked", "--message-format=json", "--", *flags],
            cwd=ROOT, capture_output=True, text=True)
        if run.returncode:
            raise ValueError("Clippy failed: " + run.stderr[-4096:])
        messages = map(json.loads, run.stdout.splitlines())
        counts, locations, excluded = count(metadata, messages)
        policy = tomllib.loads(
            (ROOT / "tests/scripts/baselines.toml").read_text())
        budgets = policy["panics"]
        failures = errors(counts, budgets)
        result.update({"counts": counts, "budgets": budgets,
                       "diagnostics": locations,
                       "excluded_diagnostics": excluded, "errors": failures})
    except (ValueError, KeyError, TypeError, OSError,
            subprocess.CalledProcessError) as error:
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
