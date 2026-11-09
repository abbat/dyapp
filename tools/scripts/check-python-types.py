"""Check the complete Python inventory with its target-platform stdlib."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
LIBRARY = Path(os.environ.get("DYAPP_PYTHON_TYPES", "/opt/dyapp-python-types"))
WINDOWS = {
    "tests/scripts/check-windows-icon.py",
    "tests/scripts/windows-tray.py",
}


def groups(root: Path) -> dict[str, list[str]]:
    sources = {
        path.relative_to(root).as_posix()
        for folder in ("tools", "tests")
        for pattern in ("*.py", "*.pyi")
        for path in (root / folder).rglob(pattern)
    }
    if not WINDOWS <= sources:
        raise ValueError("Missing Windows Python source inventory")
    targets = {"linux": sorted(sources - WINDOWS), "win32": sorted(WINDOWS)}
    combined = set(targets["linux"]) | set(targets["win32"])
    if combined != sources or set(targets["linux"]) & set(targets["win32"]):
        raise ValueError("Incomplete or overlapping Python platform inventory")
    return targets


def environment(library: Path, root: Path) -> dict[str, str]:
    marker = json.loads((library / ".dyapp-python-types.json").read_text())
    lock = root / "tools/python-types/requirements.json"
    expected = hashlib.sha256(lock.read_bytes()).hexdigest()
    abi = f"cp{sys.version_info.major}{sys.version_info.minor}"
    if marker.get("lock_sha256") != expected or marker.get("abi") != abi:
        raise ValueError(
            "Python checker installation is stale; run make prepare"
        )
    env = dict(
        os.environ, PYTHONPATH=str(library), PYTHONDONTWRITEBYTECODE="1"
    )
    env.pop("MYPYPATH", None)
    return env


def check(root: Path = ROOT, library: Path = LIBRARY) -> int:
    env = environment(library, root)
    result = 0
    for platform, files in groups(root).items():
        command = [
            sys.executable,
            "-B",
            "-m",
            "mypy",
            "--config-file",
            str(root / ".mypy.ini"),
            "--strict",
            "--python-version",
            "3.11",
            "--platform",
            platform,
            "--cache-dir",
            str(
                root
                / "build/python-types"
                / f"cp{sys.version_info.major}{sys.version_info.minor}"
                / platform
            ),
            *files,
        ]
        print(f"Python types ({platform}): {len(files)} files", flush=True)
        status = subprocess.run(command, cwd=root, env=env, check=False)
        result = max(result, int(status.returncode != 0))
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--library", type=Path, default=LIBRARY)
    arguments = parser.parse_args()
    try:
        raise SystemExit(check(library=arguments.library))
    except (OSError, ValueError) as error:
        parser.exit(1, f"Python type check failed: {error}\n")
