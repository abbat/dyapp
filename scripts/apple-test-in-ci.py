"""Build Apple apps and run hosted XCTest/XCUITest in approved native CI."""
import json
import os
from pathlib import Path
import subprocess


def run(arguments, log):
    with log.open("w") as output:
        result = subprocess.run(
            arguments, stdout=output, stderr=subprocess.STDOUT, check=False)
    print(log.read_text())
    if result.returncode:
        raise RuntimeError(
            f"Command failed ({result.returncode}): {arguments}")


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Native Apple tests require approved macOS CI")
    platform = os.environ["APP_PLATFORM"]
    if platform not in ("ios", "macos"):
        raise ValueError(f"Unsupported Apple platform: {platform}")
    reports = Path(os.environ["RUNNER_TEMP"]) / "apple-results"
    reports.mkdir(parents=True, exist_ok=True)
    derived = reports / "DerivedData"
    common = ["xcodebuild", "-project", f"{platform}/DYApp.xcodeproj",
              "-scheme", "DYApp", "-derivedDataPath", str(derived),
              "CODE_SIGNING_ALLOWED=NO"]
    run(["xcodebuild", "-version"], reports / "xcode-version.txt")
    if platform == "ios":
        run(common + ["-destination", "generic/platform=iOS", "build"],
            reports / "device-build.log")
        devices = json.loads(subprocess.check_output(
            ["xcrun", "simctl", "list", "devices", "available", "--json"],
            text=True))
        candidates = [device for runtime, values in devices["devices"].items()
                      if ".iOS-" in runtime
                      for device in values
                      if device.get("isAvailable")
                      if device["name"].startswith("iPhone")]
        if not candidates:
            raise RuntimeError(
                "No available iPhone simulator; required suite cannot skip")
        device = sorted(candidates, key=lambda item: item["name"])[0]
        destination = f"platform=iOS Simulator,id={device['udid']}"
    else:
        run(common + ["-destination", "generic/platform=macOS",
                      "ARCHS=arm64 x86_64", "ONLY_ACTIVE_ARCH=NO", "build"],
            reports / "universal-build.log")
        destination = "platform=macOS"
    result_bundle = reports / f"{platform}.xcresult"
    run(common + ["-destination", destination,
                  "-parallel-testing-enabled", "NO",
                  "-resultBundlePath", str(result_bundle), "test"],
        reports / "tests.log")
    summary = subprocess.check_output(
        ["xcrun", "xcresulttool", "get", "test-results", "summary",
         "--path", str(result_bundle)], text=True)
    (reports / "summary.json").write_text(summary)
    run(["python3", "scripts/check-apple-results.py",
         str(reports / "summary.json")],
        reports / "inventory.log")
    apps = list((derived / "Build" / "Products").glob("*/DYApp.app"))
    if not apps:
        raise RuntimeError("Application artifact missing")
    (reports / "artifacts.json").write_text(
        json.dumps([str(path) for path in apps]))
    print(f"{platform}: application artifacts and native tests verified")


if __name__ == "__main__":
    main()
