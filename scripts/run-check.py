"""Run a mapped check with concise diagnostics and a retained raw log."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import uuid

from capture import Capture

ROOT = Path(__file__).resolve().parents[1]
FIX = "docs/development/code-quality.md#check-output"
ANSI = re.compile(r"\x1b\[[0-?]*[ -/]*[@-~]")


def diagnostics(text, policy):
    lines = ANSI.sub("", text).splitlines()
    kind = policy["format"]
    if kind == "json":
        try:
            errors = json.loads(text)["errors"]
            if not isinstance(errors, list):
                return []
            selected = []
            for error in errors:
                if not isinstance(error, str) or not error.strip():
                    continue
                error = ANSI.sub("", error)
                parts = error.splitlines()
                if "pattern" in policy and len(parts) > 1:
                    selected.extend(line for line in parts
                                    if re.search(policy["pattern"], line))
                else:
                    selected.append(error)
            return selected
        except (ValueError, KeyError, TypeError):
            return []
    if kind == "python":
        starts = [i for i, line in enumerate(lines)
                  if re.match(r"^[\w.]*(?:Error|Exception):", line)]
        return [line for line in lines[starts[-1]:] if line] if starts else []
    if kind == "rust-test":
        start = next((i for i, line in enumerate(lines)
                      if line == "failures:"), None)
        if start is not None:
            return [line for line in lines[start:] if line]
        pattern = r"^(?:error(?:\[[^]]+\])?:|\s*-->\s)"
        return diagnostics(text, {"format": "lines", "pattern": pattern})
    if kind == "unittest":
        starts = [i for i, line in enumerate(lines)
                  if line.startswith(("FAIL:", "ERROR:"))]
        if not starts:
            return []
        selected = []
        for line in lines[starts[0]:]:
            if line.startswith("Ran "):
                break
            if line and set(line) not in ({"="}, {"-"}):
                selected.append(line)
        return selected
    if kind == "lines":
        if policy.get("details") and "DETAILS:" in lines:
            return [line for line in lines[lines.index("DETAILS:") + 1:]
                    if line]
        if policy.get("nested"):
            selected, active = [], False
            for line in lines:
                if line.startswith("FAIL "):
                    active = True
                if active:
                    selected.append(line)
                if line.endswith(": raw captured output"):
                    active = False
            if selected:
                return selected
        pattern = re.compile(policy["pattern"])
        return [line for line in lines if pattern.search(line)]
    raise ValueError(f"Unknown diagnostic format: {kind!r}")


def failure(name, fix, status, selected, log=None):
    cause = (selected[0] if selected
             else f"exit {status}; no recognized diagnostic")
    if status < 0:
        cause = f"terminated by signal {-status}"
    cause = " ".join(cause.splitlines())[:200]
    print(f"FAIL {name}: {cause}")
    print(f"FIX: {fix}")
    print("DETAILS:")
    for line in selected:
        print(line)
    if log is not None:
        print(f"{log}: raw captured output")
    return 128 - status if status < 0 else status


def run(name, command, policy, directory):
    directory.mkdir(parents=True, exist_ok=True)
    log = directory / f"{name}-{uuid.uuid4().hex}.log"
    captured = Capture(log_file=log)
    descriptor, drain = captured.pipe()
    status = 0
    try:
        try:
            status = subprocess.run(
                command, stdout=descriptor, stderr=subprocess.STDOUT,
                check=False).returncode
        except OSError as error:
            captured.write(f"{type(error).__name__}: {error}\n".encode())
            status = 127
        except KeyboardInterrupt:
            status = 130
    finally:
        os.close(descriptor)
        drain.join(timeout=10)
        captured.close()
    if status == 0:
        print(f"OK {name}")
        return 0
    text = captured.getvalue().decode(errors="replace")
    selected = diagnostics(text, policy)
    if not selected:
        selected = diagnostics(text, {"format": "python"})
    return failure(name, policy["fix"], status, selected, log)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log-dir", type=Path,
                        default=ROOT / "target" / "check-output")
    parser.add_argument("check")
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    try:
        policies = json.loads((ROOT / "scripts/check-output.json").read_text())
        if not re.fullmatch(r"[a-z][a-z0-9-]*", args.check):
            raise ValueError(f"Invalid check name: {args.check!r}")
        if args.check not in policies:
            raise ValueError(f"Unmapped check: {args.check!r}")
        command = args.command
        if command[:1] == ["--"]:
            command = command[1:]
        if not command:
            raise ValueError("A check command is required after --")
        policy = policies[args.check]
        if not policy["fix"].startswith("docs/") or "#" not in policy["fix"]:
            raise ValueError("FIX needs a repository documentation anchor")
        if "pattern" in policy:
            re.compile(policy["pattern"])
        diagnostics("", policy)
        return run(args.check, command, policy, args.log_dir)
    except (OSError, ValueError, KeyError, TypeError, re.error) as error:
        return failure("check-output", FIX, 1, [str(error)])


if __name__ == "__main__":
    raise SystemExit(main())
