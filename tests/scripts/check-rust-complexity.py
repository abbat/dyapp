"""Run strict Clippy with exact legacy function complexity budgets."""

import argparse
import json
import os
from pathlib import Path, PurePosixPath
import subprocess
import sys
import time
import tomllib

import rust_complexity as policy

ROOT = Path(__file__).resolve().parents[2]
SCOPES = {
    "workspace": "Cargo.toml",
    "linux": "clients/linux/Cargo.toml",
    "windows": "clients/windows/Cargo.toml",
}
THRESHOLDS = {
    "cognitive-complexity-threshold": 15,
    "too-many-lines-threshold": 50,
    "excessive-nesting-threshold": 5,
}


def row_key(row):
    fields = {"path", "function", "metrics"}
    if not isinstance(row, dict) or set(row) != fields:
        raise ValueError("Invalid complexity budget row")
    key = row["path"], row["function"]
    if not all(isinstance(value, str) and value.strip() for value in key):
        raise ValueError("Invalid function identity")
    path = PurePosixPath(key[0])
    invalid = [
        path.is_absolute(),
        ".." in path.parts,
        path.as_posix() != key[0],
        chr(92) in key[0],
        ":" in key[0],
    ]
    if any(invalid):
        raise ValueError("Budget path must be repository-relative")
    return key


def row_metrics(values):
    allowed = {"lines", "cognitive", "nesting", "nested_regions"}
    if not isinstance(values, dict) or not values:
        raise ValueError("Expected nonempty function metrics")
    if not values.keys() <= allowed:
        raise ValueError("Unknown function metrics")
    if any(type(value) is not int or value <= 0 for value in values.values()):
        raise ValueError("Metrics must be positive integers")
    if ("nesting" in values) != ("nested_regions" in values):
        raise ValueError("Nesting needs both depth and region count")
    return values


def budgets(rows):
    if not isinstance(rows, list):
        raise ValueError("Expected a list of complexity budgets")
    result = {}
    for row in rows:
        key = row_key(row)
        if key in result:
            raise ValueError("Duplicate function identity")
        result[key] = row_metrics(row["metrics"])
    return result


def budget_changes(path, current):
    base = os.environ.get("COMPLEXITY_BASE") or "origin/main"
    if set(base) == {"0"}:
        base = "origin/main"
    git = ["git", "-c", f"safe.directory={ROOT}"]
    subprocess.run(
        [*git, "rev-parse", "--verify", base + "^{commit}"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    )
    relative = path.relative_to(ROOT).as_posix()
    tree = subprocess.run(
        [*git, "ls-tree", "--name-only", base, "--", relative],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    if not tree.stdout.strip():
        return []  # First measured baseline; later revisions can only shrink.
    old = subprocess.check_output(
        [*git, "show", f"{base}:{relative}"], cwd=ROOT, text=True
    )
    return policy.growth_errors(current, budgets(json.loads(old)))


def check_thresholds(directory):
    git = ["git", "-c", f"safe.directory={ROOT}"]
    raw = subprocess.check_output(
        [
            *git,
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "*.rs",
        ],
        cwd=ROOT,
    )
    paths = sorted(set(os.fsdecode(path) for path in raw.split(b"\0") if path))
    if not paths:
        raise ValueError("Missing Rust source inventory")
    command = [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--manifest-path",
        str(ROOT / "tools/rust-functions/Cargo.toml"),
        "--",
        "--check-thresholds",
        *paths,
    ]
    log = directory / "thresholds.stderr"
    with log.open("w") as stderr:
        run = subprocess.run(command, cwd=ROOT, stderr=stderr, check=False)
    if run.returncode:
        raise ValueError(f"Invalid Rust lint configuration; see {log}")


def clippy(scope, directory):
    manifest = ROOT / SCOPES[scope]
    config = directory / "clippy"
    config.mkdir()
    text = (ROOT / "clippy.toml").read_text()
    settings = tomllib.loads(text)
    if any(settings.get(key) != value for key, value in THRESHOLDS.items()):
        raise ValueError("Configured Rust function limits must stay 15/50/5")
    (config / "clippy.toml").write_text(text)
    env = dict(os.environ, CLIPPY_CONF_DIR=str(config))
    common = ["--manifest-path", str(manifest), "--locked", "--all-features"]
    metadata = subprocess.check_output(
        ["cargo", "metadata", *common, "--no-deps", "--format-version=1"],
        cwd=manifest.parent,
        text=True,
    )
    (directory / "metadata.json").write_text(metadata)
    flags = [flag for lint in policy.LINTS for flag in ("--force-warn", lint)]
    command = [
        "cargo",
        "clippy",
        *common,
        "--workspace",
        "--all-targets",
        "--message-format=json",
        "--",
        "-D",
        "warnings",
        "-D",
        "clippy::todo",
        *flags,
    ]
    started = time.time_ns()
    with (directory / "clippy.jsonl").open("w") as output:
        with (directory / "clippy.stderr").open("w") as stderr:
            run = subprocess.run(
                command,
                cwd=manifest.parent,
                env=env,
                stdout=output,
                stderr=stderr,
                check=False,
            )
    if run.returncode:
        raise ValueError(f"Clippy failed; see {directory / 'clippy.stderr'}")
    records = [
        json.loads(line)
        for line in (directory / "clippy.jsonl").read_text().splitlines()
    ]
    if (ROOT / "clippy.toml").read_text() != text:
        raise ValueError("Clippy configuration changed during analysis")
    findings = policy.diagnostics(json.loads(metadata), records)
    return manifest.parent, started, findings


def sources(cwd, started, findings):
    result = {}
    for finding in findings:
        span = finding["span"]
        path = (cwd / span["file_name"]).resolve()
        if not path.is_relative_to(ROOT):
            raise ValueError(f"Complexity span outside repository: {path}")
        if path.stat().st_mtime_ns > started:
            raise ValueError(f"Source changed during Clippy: {path}")
        name = path.relative_to(ROOT).as_posix()
        span["file_name"] = name
        result[name] = path.read_bytes()
    return result


def measure(findings, contents, directory):
    if not contents:
        return {}
    command = [
        "cargo",
        "run",
        "--quiet",
        "--locked",
        "--manifest-path",
        str(ROOT / "tools/rust-functions/Cargo.toml"),
        "--",
        *sorted(contents),
    ]
    with (directory / "parser.stderr").open("w") as stderr:
        run = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=False,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
            check=False,
        )
    (directory / "functions.jsonl").write_text(run.stdout)
    if run.returncode:
        log = directory / "parser.stderr"
        raise ValueError(f"Source parser failed; see {log}")
    records = [json.loads(line) for line in run.stdout.splitlines()]
    functions = {item["path"]: item["functions"] for item in records}
    if len(functions) != len(records) or functions.keys() != contents.keys():
        raise ValueError("Incomplete or duplicated source parser output")
    for name, previous in contents.items():
        if (ROOT / name).read_bytes() != previous:
            raise ValueError(f"Source changed during parsing: {name}")
    return policy.measure(findings, contents, functions)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=SCOPES, default="workspace")
    args = parser.parse_args()
    directory = ROOT / "build/complexity" / args.scope / str(time.time_ns())
    directory.mkdir(parents=True)
    report = {"scope": args.scope, "errors": []}
    try:
        path = ROOT / "tests/scripts/rust-complexity" / (args.scope + ".json")
        allowed = budgets(json.loads(path.read_text()))
        changes = budget_changes(path, allowed)
        check_thresholds(directory)
        cwd, started, findings = clippy(args.scope, directory)
        contents = sources(cwd, started, findings)
        measured = measure(findings, contents, directory)
        report["functions"] = [
            {"path": key[0], "function": key[1], "metrics": value}
            for key, value in sorted(measured.items())
        ]
        report["errors"] = changes + policy.errors(measured, allowed)
    except (
        ValueError,
        KeyError,
        TypeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        report["errors"].append(str(error))
    destination = directory / "report.json"
    destination.write_text(json.dumps(report, indent=2) + "\n")
    for error in report["errors"]:
        print("error: Rust complexity: " + error, file=sys.stderr)
    print("Complexity report: " + str(destination))
    return bool(report["errors"])


if __name__ == "__main__":
    from check_wrapper import wrap

    log_directory = ROOT / "build/check-output"
    os.environ.setdefault("DYAPP_CHECK_LOG_DIR", str(log_directory))
    wrap("clippy")
    raise SystemExit(main())
