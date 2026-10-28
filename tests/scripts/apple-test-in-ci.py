"""Build Apple apps and run hosted XCTest/XCUITest in approved native CI."""
from contextlib import contextmanager
import json
import os
from pathlib import Path
import socket
import subprocess
import threading
import time


def run(arguments, log, env=None, timeout=None):
    with log.open("w") as output:
        result = subprocess.run(
            arguments, stdout=output, stderr=subprocess.STDOUT,
            check=False, env=env, timeout=timeout)
    text = log.read_text()
    print(text)
    if result.returncode:
        # The check wrapper shows only this error, so it carries the log tail.
        raise RuntimeError(
            f"Command failed ({result.returncode}): {arguments}\n"
            f"{text[-4096:]}")


@contextmanager
def local_node(reports):
    """Run loopback dyappd for native UI tests without public peers."""
    run(["cargo", "build", "--locked", "-p", "dyapp-bootstrap",
         "--bin", "dyappd"],
        reports / "node-build.log")
    address = "127.0.0.1:33445"
    storage = reports / "node-storage"
    env = dict(os.environ, DYAPPD_LISTEN=json.dumps([address]),
               DYAPPD_EXTERNAL=json.dumps([address]),
               DYAPPD_STORAGE_DIR=str(storage), DYAPPD_NETWORK_ID_POW_BITS="8")
    run(["build/debug/dyappd", "keygen"], reports / "node-keygen.log", env=env,
        timeout=60)
    peer_id = (storage / "node.id").read_text().strip()
    metadata = {"address": address, "peer_id": peer_id}
    (reports / "node.json").write_text(json.dumps(metadata))
    with (reports / "node.log").open("w") as log:
        server = subprocess.Popen(["build/debug/dyappd"], env=env,
                                  stdout=log, stderr=subprocess.STDOUT)
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
def silent_endpoint():
    """Accept local TCP without answering the peer handshake."""
    listener = socket.socket()
    listener.bind(("127.0.0.1", 33447))
    listener.listen()
    listener.settimeout(0.5)
    stopping = threading.Event()
    clients = []

    def accept_connections():
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


def main():
    if os.environ.get("GITHUB_ACTIONS") != "true":
        raise RuntimeError("Native Apple tests require approved macOS CI")
    platform = os.environ["APP_PLATFORM"]
    if platform not in ("ios", "macos"):
        raise ValueError(f"Unsupported Apple platform: {platform}")
    reports = Path(os.environ["RUNNER_TEMP"]) / "apple-results"
    reports.mkdir(parents=True, exist_ok=True)
    os.environ["DYAPP_CHECK_LOG_DIR"] = str(reports / "check-output")
    derived = reports / "DerivedData"
    common = ["xcodebuild", "-project", f"clients/{platform}/DYApp.xcodeproj",
              "-scheme", "DYApp", "-derivedDataPath", str(derived)]
    unsigned = ["CODE_SIGNING_ALLOWED=NO"]
    build = common + unsigned
    # The iOS Simulator refuses an unsigned app the Keychain (OSStatus
    # -34018); an ad-hoc signature gives it an application identifier.
    # macOS uses the login keychain unsigned.
    test = common + (["CODE_SIGNING_ALLOWED=YES", "CODE_SIGN_IDENTITY=-"]
                     if platform == "ios" else unsigned)
    run(["xcodebuild", "-version"], reports / "xcode-version.txt")
    if platform == "ios":
        run(build + ["-destination", "generic/platform=iOS", "build"],
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
        destination = (f"platform=iOS Simulator,id={device['udid']},"
                       f"arch={os.uname().machine}")
    else:
        run(build + ["-destination", "generic/platform=macOS",
                     "ARCHS=arm64 x86_64", "ONLY_ACTIVE_ARCH=NO", "build"],
            reports / "universal-build.log")
        destination = f"platform=macOS,arch={os.uname().machine}"
    result_bundle = reports / f"{platform}.xcresult"
    with local_node(reports), silent_endpoint():
        run(test + ["-destination", destination,
                    "-parallel-testing-enabled", "NO",
                    "-retry-tests-on-failure", "-test-iterations", "3",
                    "-resultBundlePath", str(result_bundle), "test"],
            reports / "tests.log")
    summary = subprocess.check_output(
        ["xcrun", "xcresulttool", "get", "test-results", "summary",
         "--path", str(result_bundle)], text=True)
    (reports / "summary.json").write_text(summary)
    run(["python3", "tests/scripts/check-apple-results.py",
         str(reports / "summary.json")],
        reports / "inventory.log")
    tests = subprocess.check_output(
        ["xcrun", "xcresulttool", "get", "test-results", "tests",
         "--path", str(result_bundle)], text=True)
    (reports / "tests.json").write_text(tests)
    run(["python3", "tests/scripts/check-flaky.py", "--raw",
         str(reports / "tests.json")],
        reports / "flaky.log")
    apps = list((derived / "Build" / "Products").glob("*/DYApp.app"))
    if not apps:
        raise RuntimeError("Application artifact missing")
    (reports / "artifacts.json").write_text(
        json.dumps([str(path) for path in apps]))
    print(f"{platform}: application artifacts and native tests verified")


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("apple-tests")
    main()
