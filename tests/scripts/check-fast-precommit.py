#!/usr/bin/env python3
"""Fast staged checks for mistakes that otherwise surface in CI coverage."""

import re
import subprocess
import sys


ENV_CLEAR = re.compile(r"\.env_clear\s*\(\s*\)")
PROFILE_FORWARD = re.compile(
    r'\.envs\s*\(\s*std::env::var_os\s*\(\s*"LLVM_PROFILE_FILE"'
)


def missing_profile_lines(source: str) -> list[int]:
    """Find subprocesses whose cleared environment drops coverage output."""
    lines = source.splitlines()
    missing = []
    for index, line in enumerate(lines):
        if ENV_CLEAR.search(line):
            following = "\n".join(lines[index + 1:index + 7])
            if not PROFILE_FORWARD.search(following):
                missing.append(index + 1)
    return missing


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), stderr=subprocess.STDOUT)


def main() -> int:
    try:
        subprocess.run(("git", "diff", "--cached", "--check"), check=True)
        paths = git(
            "diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"
        ).split(b"\0")
        failures = []
        for raw_path in filter(None, paths):
            path = raw_path.decode("utf-8", "surrogateescape")
            parts = path.split("/")
            if not all((
                len(parts) >= 4,
                parts[0] == "rust",
                "tests" in parts[2:],
                path.endswith(".rs"),
            )):
                continue
            source = git("show", ":" + path).decode("utf-8")
            failures.extend(
                f"{path}:{line}" for line in missing_profile_lines(source)
            )
    except (OSError, subprocess.CalledProcessError, UnicodeError) as error:
        print(f"pre-commit: staged check failed: {error}", file=sys.stderr)
        return 1
    if failures:
        print("pre-commit: .env_clear() drops LLVM_PROFILE_FILE "
              "in Rust integration tests:", file=sys.stderr)
        for location in failures:
            print(f"  {location}", file=sys.stderr)
        print("Forward LLVM_PROFILE_FILE after env_clear(); see "
              "docs/testing/README.md#coverage-subprocesses", file=sys.stderr)
        return 1
    print("OK fast pre-commit")
    return 0


if __name__ == "__main__":
    sys.exit(main())
