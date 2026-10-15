"""Produce unsigned Release Apple packages only in approved native CI."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def run(command, log):
    with log.open("w") as stream:
        result = subprocess.run(command, cwd=ROOT, stdout=stream,
                                stderr=subprocess.STDOUT, check=False)
    if result.returncode:
        raise ValueError(f"Command exited {result.returncode}; see {log}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=("ios", "macos"))
    args = parser.parse_args()
    try:
        if os.environ.get("GITHUB_ACTIONS") != "true":
            raise ValueError("Apple Release production requires approved CI")
        reports = Path(os.environ["RUNNER_TEMP"]) / f"release-{args.target}"
        reports.mkdir(parents=True, exist_ok=True)
        run(["xcodebuild", "-version"], reports / "xcode-version.txt")
        derived = reports / "DerivedData"
        command = [
            "xcodebuild", "-project", f"{args.target}/DYApp.xcodeproj",
            "-scheme", "DYApp", "-configuration", "Release",
            "-derivedDataPath", str(derived), "CODE_SIGNING_ALLOWED=NO",
            "ONLY_ACTIVE_ARCH=NO"]
        if args.target == "ios":
            command += ["-destination", "generic/platform=iOS", "-sdk",
                        "iphoneos", "ARCHS=arm64", "build"]
            product, suffix = "Release-iphoneos", "ipa"
            name = "ios-macos15-arm64-release-unsigned-ipa"
        else:
            command += ["-destination", "generic/platform=macOS", "-sdk",
                        "macosx", "ARCHS=arm64 x86_64", "build"]
            product, suffix = "Release", "zip"
            name = "macos-macos15-universal-release-unsigned-zip"
        run(command, reports / "build.log")
        app = derived / "Build" / "Products" / product / "DYApp.app"
        archive = reports / f"DYApp-unsigned.{suffix}"
        run([sys.executable, str(ROOT / "scripts/package-apple-release.py"),
             args.target, str(app), str(archive)], reports / "package.log")
        run([sys.executable, str(ROOT / "scripts/report-artifact-size.py"),
             name, str(archive), "--environment",
             f"macos-15; {args.target}; Rust 1.99.0; Release; unsigned",
             "--report", str(reports / "measurement.json")],
            reports / "measurement.log")
    except (OSError, ValueError, KeyError) as error:
        print("FAIL apple-release: native producer failed")
        print("FIX: docs/development/build.md#unsigned-apple-release-packages")
        print("DETAILS:")
        print(str(error))
        return 1
    print(f"OK apple-release {args.target}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
