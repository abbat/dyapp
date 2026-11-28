"""Build Apple apps and run hosted XCTest/XCUITest in approved native CI."""

from __future__ import annotations

import typing

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import threading
import time


def run(
    arguments: "typing.Sequence[str]",
    log: "Path",
    env: "typing.Mapping[str, str] | None" = None,
    timeout: "float | None" = None,
) -> None:
    with log.open("w") as output:
        result = subprocess.run(
            arguments,
            stdout=output,
            stderr=subprocess.STDOUT,
            check=False,
            env=env,
            timeout=timeout,
        )
    text = log.read_text()
    print(text)
    if result.returncode:
        # The check wrapper shows only this error, so it carries the log tail.
        raise RuntimeError(
            f"Command failed ({result.returncode}): {arguments}\n"
            f"{text[-4096:]}"
        )


@contextmanager
def local_node(reports: Path) -> typing.Iterator[tuple[str, str]]:
    """Run loopback dyappd for native UI tests without public peers."""
    run(
        [
            "cargo",
            "build",
            "--locked",
            "-p",
            "dyapp-bootstrap",
            "--bin",
            "dyappd",
        ],
        reports / "node-build.log",
    )
    address = "127.0.0.1:33445"
    storage = reports / "node-storage"
    env = dict(
        os.environ,
        DYAPPD_LISTEN=json.dumps([address]),
        DYAPPD_EXTERNAL=json.dumps([address]),
        DYAPPD_STORAGE_DIR=str(storage),
        DYAPPD_NETWORK_ID_POW_BITS="8",
        DYAPPD_SEARCH_MIN_PROFILE_POW_BITS="4",
        DYAPPD_ROLES='["store", "search", "media"]',
    )
    run(
        ["build/debug/dyappd", "keygen"],
        reports / "node-keygen.log",
        env=env,
        timeout=60,
    )
    peer_id = (storage / "node.id").read_text().strip()
    metadata = {"address": address, "peer_id": peer_id}
    (reports / "node.json").write_text(json.dumps(metadata))
    with (reports / "node.log").open("w") as log:
        server = subprocess.Popen(
            ["build/debug/dyappd"],
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
        )
    try:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if server.poll() is not None:
                break
            try:
                with socket.create_connection(("127.0.0.1", 33445), timeout=1):
                    break
            except OSError:
                time.sleep(0.25)
        else:
            log_tail = (reports / "node.log").read_text()[-4096:]
            raise RuntimeError(f"Local dyappd did not listen: {log_tail}")
        if server.poll() is not None:
            log_tail = (reports / "node.log").read_text()[-4096:]
            raise RuntimeError(f"Local dyappd exited: {log_tail}")
        yield address, peer_id
    finally:
        if server.poll() is None:
            server.terminate()
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=10)


@contextmanager
def silent_endpoint() -> typing.Iterator[str]:
    """Accept local TCP without answering the peer handshake."""
    listener = socket.socket()
    listener.bind(("127.0.0.1", 33447))
    listener.listen()
    listener.settimeout(0.5)
    stopping = threading.Event()
    clients = []

    def accept_connections() -> None:
        while not stopping.is_set():
            try:
                client, _ = listener.accept()
                clients.append(client)
            except socket.timeout:
                continue
            except OSError:
                break

    worker = threading.Thread(target=accept_connections, daemon=True)
    worker.start()
    try:
        yield "127.0.0.1:33447"
    finally:
        stopping.set()
        listener.close()
        worker.join(timeout=2)
        for client in clients:
            client.close()


def keep_crash_reports(reports: "Path") -> "bool":
    """A crash ends a UI test with just "not running"; its report says why."""
    source = Path.home() / "Library" / "Logs" / "DiagnosticReports"
    kept = False
    for report in source.glob("DYApp*"):
        (reports / "crash-reports").mkdir(exist_ok=True)
        shutil.copy(report, reports / "crash-reports")
        kept = True
    return kept


def keep_log(
    reports: Path, simulator: str | None, name: str, predicate: str
) -> None:
    """Save the unified log lines matching predicate as name."""
    spawn = ["xcrun", "simctl", "spawn", simulator] if simulator else []
    with (reports / name).open("w") as output:
        try:
            subprocess.run(
                spawn
                + [
                    "log",
                    "show",
                    "--last",
                    "3h",
                    "--style",
                    "compact",
                    "--predicate",
                    predicate,
                ],
                stdout=output,
                stderr=subprocess.STDOUT,
                check=False,
                timeout=5 * 60,
            )
        except subprocess.TimeoutExpired:
            output.write("log show timed out\n")


def validate_ios_attachments(directory: Path, language: str) -> dict[str, str]:
    manifest = json.loads((directory / "manifest.json").read_text())
    digests = {}
    for state in ("eligible-target", "sent"):
        name = f"ios-profile-share-{language}-{state}"
        png = evidence_file(directory, manifest, name, ".png")
        if not png.read_bytes().startswith(b"\x89PNG\r\n\x1a\n"):
            raise RuntimeError(f"Invalid PNG attachment: {name}")
        geometry = evidence_file(
            directory, manifest, f"{name}-geometry", ".txt"
        )
        if not geometry.read_text().startswith("element="):
            raise RuntimeError(f"Invalid geometry attachment: {name}")
        for attachment in (png, geometry):
            digests[attachment.name] = hashlib.sha256(
                attachment.read_bytes()
            ).hexdigest()
    return digests


def evidence_file(
    directory: Path,
    manifest: list[dict[str, list[dict[str, str]]]],
    name: str,
    suffix: str,
) -> Path:
    exported_name = re.compile(
        rf"{re.escape(name)}(?:_\d+)*{re.escape(suffix)}"
    )
    matches = [
        Path(item["exportedFileName"])
        for test in manifest
        for item in test["attachments"]
        if exported_name.fullmatch(item.get("suggestedHumanReadableName", ""))
    ]
    if len(matches) != 1 or matches[0].name != str(matches[0]):
        raise RuntimeError(f"Expected exactly one exported attachment: {name}")
    return directory / matches[0]


def export_ios_evidence(
    reports: Path, result_bundle: Path, apps: list[Path], destination: str
) -> None:
    attachments = reports / "attachments"
    attachments.mkdir(exist_ok=True)
    digests = {}
    for language, test in (
        ("en", "testSharedProfileOpensFromItsCardAndSharesToAChat"),
        ("ru", "testSharedProfileInRussian"),
    ):
        directory = attachments / language
        run(
            [
                "xcrun",
                "xcresulttool",
                "export",
                "attachments",
                "--path",
                str(result_bundle),
                "--output-path",
                str(directory),
                "--test-id",
                f"ChatsUITests/{test}()",
            ],
            reports / f"attachments-{language}.log",
        )
        digests.update(
            {
                f"{language}/{name}": digest
                for name, digest in validate_ios_attachments(
                    directory, language
                ).items()
            }
        )
    binaries = [
        app / "DYApp"
        for app in apps
        if app.parent.name.endswith("-iphonesimulator")
    ]
    if len(binaries) != 1:
        raise RuntimeError("Expected one tested iOS simulator application")
    with binaries[0].open("rb") as binary:
        binary_sha256 = hashlib.file_digest(binary, "sha256").hexdigest()
    source = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], text=True
    ).strip()
    if source != os.environ["GITHUB_SHA"]:
        raise RuntimeError(
            "Tested iOS checkout differs from the CI source commit"
        )
    (attachments / "provenance.json").write_text(
        json.dumps(
            {
                "source": source,
                "binary_sha256": binary_sha256,
                "destination": destination,
                "attachment_sha256": digests,
            }
        )
    )


def failed_test_summary(bundle: Path, reports: Path) -> list[str]:
    if not bundle.is_dir():
        return []
    result = subprocess.run(
        [
            "xcrun",
            "xcresulttool",
            "get",
            "test-results",
            "summary",
            "--path",
            str(bundle),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode:
        return []
    (reports / "summary.json").write_text(result.stdout)
    try:
        issues = json.loads(result.stdout).get("testFailures", [])
    except json.JSONDecodeError:
        return []
    return [
        f"XCTest failure: {issue.get('testName')}: "
        f"{str(issue.get('failureText'))[:600]}"
        for issue in issues[:10]
    ]


def main() -> None:
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Native Apple tests require approved macOS CI")
    platform = os.environ["APP_PLATFORM"]
    if platform not in ("ios", "macos"):
        raise ValueError(f"Unsupported Apple platform: {platform}")
    reports = Path(os.environ["RUNNER_TEMP"]) / "apple-results"
    reports.mkdir(parents=True, exist_ok=True)
    os.environ["DYAPP_CHECK_LOG_DIR"] = str(reports / "check-output")
    derived = reports / "DerivedData"
    common = [
        "xcodebuild",
        "-project",
        f"clients/{platform}/DYApp.xcodeproj",
        "-scheme",
        "DYApp",
        "-derivedDataPath",
        str(derived),
    ]
    if platform == "ios" and os.environ.get("DYAPP_IOS_UI_FIXTURES") == "1":
        common.append("OTHER_SWIFT_FLAGS=$(inherited) -DIOS_UI_FIXTURES")
    unsigned = ["CODE_SIGNING_ALLOWED=NO"]
    build = common + unsigned
    # The iOS Simulator refuses an unsigned app the Keychain (OSStatus
    # -34018); an ad-hoc signature gives it an application identifier.
    # macOS uses the login keychain unsigned.
    test = common + (
        ["CODE_SIGNING_ALLOWED=YES", "CODE_SIGN_IDENTITY=-"]
        if platform == "ios"
        else unsigned
    )
    simulator = None
    run(["xcodebuild", "-version"], reports / "xcode-version.txt")
    if platform == "ios":
        run(
            build + ["-destination", "generic/platform=iOS", "build"],
            reports / "device-build.log",
        )
        devices = json.loads(
            subprocess.check_output(
                ["xcrun", "simctl", "list", "devices", "available", "--json"],
                text=True,
            )
        )
        candidates = [
            device
            for runtime, values in devices["devices"].items()
            if ".iOS-" in runtime
            for device in values
            if device.get("isAvailable")
            if device["name"].startswith("iPhone")
        ]
        if not candidates:
            raise RuntimeError(
                "No available iPhone simulator; required suite cannot skip"
            )
        device = sorted(candidates, key=lambda item: item["name"])[0]
        simulator = device["udid"]
        destination = (
            f"platform=iOS Simulator,id={simulator},arch={os.uname().machine}"
        )
    else:
        run(
            build
            + [
                "-destination",
                "generic/platform=macOS",
                "ARCHS=arm64 x86_64",
                "ONLY_ACTIVE_ARCH=NO",
                "build",
            ],
            reports / "universal-build.log",
        )
        destination = f"platform=macOS,arch={os.uname().machine}"
    result_bundle = reports / f"{platform}.xcresult"
    try:
        with local_node(reports), silent_endpoint():
            # Ends before the job's limit, which would skip the upload of
            # everything that says why the tests ran so long.
            run(
                test
                + [
                    "-destination",
                    destination,
                    "-parallel-testing-enabled",
                    "NO",
                    "-retry-tests-on-failure",
                    "-test-iterations",
                    "3",
                    # One wedged test must fail alone, or it takes all
                    # of the time limit below and no results are left.
                    "-test-timeouts-enabled",
                    "YES",
                    "-default-test-execution-time-allowance",
                    "600",
                    "-resultBundlePath",
                    str(result_bundle),
                    "test",
                ],
                reports / "tests.log",
                timeout=70 * 60,
            )
    except RuntimeError as error:
        failures = failed_test_summary(result_bundle, reports)
        raise RuntimeError("\n".join([*failures, str(error)])) from error
    finally:
        # The test peer's NSLog reaches neither tests.log nor the xcresult.
        keep_log(
            reports,
            simulator,
            "chat-peer.log",
            'eventMessage CONTAINS "ChatTestPeer"',
        )
        # A crash report lacks an uncaught exception's reason; the log has it.
        if keep_crash_reports(reports):
            keep_log(
                reports,
                simulator,
                "app-errors.log",
                'process == "DYApp" AND (messageType == error OR '
                "messageType == fault OR "
                'eventMessage CONTAINS[c] "exception")',
            )
    summary = subprocess.check_output(
        [
            "xcrun",
            "xcresulttool",
            "get",
            "test-results",
            "summary",
            "--path",
            str(result_bundle),
        ],
        text=True,
    )
    (reports / "summary.json").write_text(summary)
    run(
        [
            "python3",
            "tests/scripts/check-apple-results.py",
            str(reports / "summary.json"),
        ],
        reports / "inventory.log",
    )
    tests = subprocess.check_output(
        [
            "xcrun",
            "xcresulttool",
            "get",
            "test-results",
            "tests",
            "--path",
            str(result_bundle),
        ],
        text=True,
    )
    (reports / "tests.json").write_text(tests)
    run(
        [
            "python3",
            "tests/scripts/check-flaky.py",
            "--raw",
            str(reports / "tests.json"),
        ],
        reports / "flaky.log",
    )
    apps = list((derived / "Build" / "Products").glob("*/DYApp.app"))
    if not apps:
        raise RuntimeError("Application artifact missing")
    (reports / "artifacts.json").write_text(
        json.dumps([str(path) for path in apps])
    )
    if platform == "ios":
        export_ios_evidence(reports, result_bundle, apps, destination)
    print(f"{platform}: application artifacts and native tests verified")


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("apple-tests")
    main()
