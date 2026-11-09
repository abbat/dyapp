#!/usr/bin/env python3
"""Fast staged Python complexity and Rust coverage-forwarding checks."""

from collections.abc import Sequence
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import sys
from tempfile import TemporaryDirectory


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
            following = "\n".join(lines[index + 1 : index + 7])
            if not PROFILE_FORWARD.search(following):
                missing.append(index + 1)
    return missing


def git(*args: str) -> bytes:
    return subprocess.check_output(("git", *args), stderr=subprocess.STDOUT)


def python_command(directory: str | Path) -> list[str]:
    return [
        "python3",
        "-B",
        "-m",
        "flake8",
        "--isolated",
        "--disable-noqa",
        "--select=C901,E9",
        "--max-complexity=10",
        "--filename=*.py,*.pyi",
        "--exclude=",
        str(directory),
    ]


def python_lint(directory: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "docker",
            "run",
            "--rm",
            "--pull=never",
            "--network",
            "none",
            "--cpus=2",
            "-v",
            f"{directory}:/staged:ro",
            "--entrypoint",
            "python3",
            "dyapp:dev-prepared",
            *python_command("/staged")[1:],
        ],
        capture_output=True,
        text=True,
        check=False,
        timeout=60,
    )


def check_python(paths: Sequence[str]) -> bool:
    selected = [path for path in paths if path.endswith((".py", ".pyi"))]
    if not selected:
        return True
    root = Path(os.fsdecode(git("rev-parse", "--show-toplevel")).strip())
    staging = root / "build/pre-commit"
    staging.mkdir(parents=True, exist_ok=True)
    with TemporaryDirectory(prefix="python-", dir=staging) as temporary:
        directory = Path(temporary)
        for name in selected:
            relative = PurePosixPath(name)
            if any(
                (
                    relative.is_absolute(),
                    ".." in relative.parts,
                    "\\" in name,
                    ":" in name,
                )
            ):
                raise ValueError("Unsafe staged Python path")
            target = directory / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(git("show", ":" + name))
        result = python_lint(directory)
    if result.returncode:
        print("FAIL flake8: staged Python complexity or syntax check failed")
        print("FIX: docs/development/linting.md#python")
        print("DETAILS:")
        text = result.stdout + result.stderr
        print(text.replace("/staged/", "").replace(str(directory) + "/", ""))
        return False
    return True


def profile_failures(paths: Sequence[str]) -> list[str]:
    failures: list[str] = []
    for path in paths:
        parts = path.split("/")
        if not all(
            (
                len(parts) >= 4,
                parts[0] == "rust",
                "tests" in parts[2:],
                path.endswith(".rs"),
            )
        ):
            continue
        source = git("show", ":" + path).decode("utf-8")
        failures.extend(
            f"{path}:{line}" for line in missing_profile_lines(source)
        )
    return failures


def main() -> int:
    try:
        subprocess.run(("git", "diff", "--cached", "--check"), check=True)
        paths = [
            os.fsdecode(path)
            for path in git(
                "diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"
            ).split(b"\0")
            if path
        ]
        failures = profile_failures(paths)
        if not failures and not check_python(paths):
            return 1
    except (
        OSError,
        subprocess.SubprocessError,
        UnicodeError,
        ValueError,
    ) as error:
        print(f"pre-commit: staged check failed: {error}", file=sys.stderr)
        return 1
    if failures:
        print(
            "pre-commit: .env_clear() drops LLVM_PROFILE_FILE "
            "in Rust integration tests:",
            file=sys.stderr,
        )
        for location in failures:
            print(f"  {location}", file=sys.stderr)
        print(
            "Forward LLVM_PROFILE_FILE after env_clear(); see "
            "docs/testing/README.md#coverage-subprocesses",
            file=sys.stderr,
        )
        return 1
    print("OK fast pre-commit")
    return 0


if __name__ == "__main__":
    sys.exit(main())
