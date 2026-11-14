"""Build an Android release from fresh source and output trees."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess


def prepare(
    source: Path, workspace: Path, cache: Path, epoch: str
) -> dict[str, str]:
    if workspace.exists():
        raise ValueError("Reproducible workspace must start absent")
    workspace.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, workspace, symlinks=True)
    if (workspace / "build").exists() or (
        workspace / "clients/android/.gradle"
    ).exists():
        raise ValueError("Committed snapshot contains cached project outputs")
    target = workspace / "build"
    home = target / "gradle-home"
    (home / "caches").mkdir(parents=True)
    for destination, original in (
        (home / "wrapper", cache / "wrapper"),
        (home / "caches/modules-2", cache / "caches/modules-2"),
    ):
        if not original.is_dir():
            raise ValueError(
                f"Prepared dependency cache is absent: {original}"
            )
        destination.symlink_to(original, target_is_directory=True)
    temporary = target / "tmp"
    temporary.mkdir()
    environment = os.environ.copy()
    environment.update(
        {
            "SOURCE_DATE_EPOCH": epoch,
            "GRADLE_USER_HOME": str(home),
            "DYAPP_ANDROID_FRESH": "1",
            "TMPDIR": str(temporary),
            "JAVA_TOOL_OPTIONS": os.environ.get("JAVA_TOOL_OPTIONS", "")
            + f" -Djava.io.tmpdir={temporary}",
        }
    )
    return environment


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--workspace", required=True, type=Path)
    parser.add_argument("--reports", required=True, type=Path)
    parser.add_argument("--epoch", required=True)
    args = parser.parse_args()
    if not args.epoch.isdigit():
        parser.error("Source epoch must be Unix seconds")
    cache = Path(os.environ["GRADLE_USER_HOME"])
    environment = prepare(args.source, args.workspace, cache, args.epoch)
    environment["DYAPP_CHECK_LOG_DIR"] = str(args.reports / "check-output")
    return subprocess.run(
        [
            "bash",
            str(args.workspace / "tools/scripts/build-android-release.sh"),
            "--raw",
            str(args.reports),
        ],
        cwd=args.workspace,
        env=environment,
        check=False,
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
