"""Produce unsigned Release Apple packages only in approved native CI."""

from __future__ import annotations

from collections.abc import Sequence
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def run(command: Sequence[str], log: Path) -> None:
    with log.open("w") as stream:
        result = subprocess.run(
            command,
            cwd=ROOT,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=False,
        )
    if result.returncode:
        raise ValueError(f"Command exited {result.returncode}; see {log}")


def build_app(target: str, reports: Path) -> tuple[Path, str, str]:
    if target == "macos":
        app = reports / "DYApp.app"
        run(
            [
                sys.executable,
                str(ROOT / "tools/scripts/build-macos-client.py"),
                "--profile",
                "release",
                "--universal",
                "--output",
                str(app),
                "--log-dir",
                str(reports),
            ],
            reports / "build.log",
        )
        run(
            [
                sys.executable,
                str(ROOT / "tools/scripts/check-no-ui-smoke.py"),
                str(app / "Contents/MacOS/dyapp-macos"),
            ],
            reports / "instrumentation.log",
        )
        return app, "zip", "macos-macos15-universal-release-unsigned-zip"
    derived = reports / "DerivedData"
    run(
        [
            "xcodebuild",
            "-project",
            "clients/ios/DYApp.xcodeproj",
            "-scheme",
            "DYApp",
            "-configuration",
            "Release",
            "-derivedDataPath",
            str(derived),
            "CODE_SIGNING_ALLOWED=NO",
            "ONLY_ACTIVE_ARCH=NO",
            "-destination",
            "generic/platform=iOS",
            "-sdk",
            "iphoneos",
            "ARCHS=arm64",
            "build",
        ],
        reports / "build.log",
    )
    app = derived / "Build/Products/Release-iphoneos/DYApp.app"
    return app, "ipa", "ios-macos15-arm64-release-unsigned-ipa"


def main() -> "int":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=("ios", "macos"))
    args = parser.parse_args()
    try:
        if os.environ.get("GITHUB_ACTIONS") != "true":
            raise ValueError("Apple Release production requires approved CI")
        reports = Path(os.environ["RUNNER_TEMP"]) / f"release-{args.target}"
        reports.mkdir(parents=True, exist_ok=True)
        run(["xcodebuild", "-version"], reports / "xcode-version.txt")
        app, suffix, name = build_app(args.target, reports)
        archive = reports / f"DYApp-unsigned.{suffix}"
        run(
            [
                sys.executable,
                str(ROOT / "tools/scripts/package-apple-release.py"),
                args.target,
                str(app),
                str(archive),
            ],
            reports / "package.log",
        )
        if args.target == "macos":
            binary = app / "Contents/MacOS/dyapp-macos"
            run(
                [
                    sys.executable,
                    "-B",
                    str(ROOT / "tools/scripts/check-hardening.py"),
                    str(binary),
                ],
                reports / "hardening.log",
            )
            run(
                [
                    sys.executable,
                    "-B",
                    str(ROOT / "tests/scripts/check-macos-icon.py"),
                    str(archive),
                    str(ROOT / "clients/macos/icons/icon.icns"),
                ],
                reports / "icon.log",
            )
        run(
            [
                sys.executable,
                str(ROOT / "tools/scripts/report-artifact-size.py"),
                name,
                str(archive),
                "--environment",
                f"macos-15; {args.target}; Rust 1.99.0; Release; unsigned",
                "--report",
                str(reports / "measurement.json"),
            ],
            reports / "measurement.log",
        )
        base = subprocess.check_output(
            ["git", "rev-parse", "HEAD^"], cwd=ROOT, text=True
        ).strip()
        head = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=ROOT, text=True
        ).strip()
        run(
            [
                sys.executable,
                str(ROOT / "tools/scripts/check-artifact-size.py"),
                name,
                str(archive),
                "--base",
                base,
                "--head",
                head,
                "--report",
                str(reports / "size.json"),
            ],
            reports / "size.log",
        )
    except (
        OSError,
        ValueError,
        KeyError,
        subprocess.CalledProcessError,
    ) as error:
        print("FAIL apple-release: native producer failed")
        print("FIX: docs/development/build.md#unsigned-apple-release-packages")
        print("DETAILS:")
        print(str(error))
        return 1
    print(f"OK apple-release {args.target}")
    return 0


if __name__ == "__main__":
    from check_dispatch import wrap

    wrap("apple-release")
    raise SystemExit(main())
