"""Exercise real bootstrap routes on two independent peers.

Without arguments the peers are the neighboring Docker containers;
`--local BINARY` starts two test-peer processes on the loopback instead.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def request(base, route, body=None, raw=False):
    operation = Request(base + route, data=body,
                        headers={"Content-Type": "application/x-protobuf"})
    with urlopen(operation, timeout=15) as response:
        data = response.read()
        return response.status, data if raw else json.loads(data)


def signed_profile(binary):
    """A fresh identity's signed profile: (peer_id, protobuf bytes)."""
    output = subprocess.run([binary, "sign-profile"], check=True,
                            capture_output=True, text=True).stdout
    signed = json.loads(output)
    return signed["peer_id"], bytes.fromhex(signed["record"])


def healthy(base):
    status, body = request(base, "/health")
    return status == 200 and body["status"] == "healthy"


def local(binary):
    """Run the suite against two loopback peers with separate storage."""
    with tempfile.TemporaryDirectory() as storage:
        peers, processes = [], []
        try:
            for port in (7071, 7072):
                processes.append(subprocess.Popen([binary], env={
                    **os.environ, "TEST_PEER_ADDR": f"127.0.0.1:{port}",
                    "TEST_PEER_STORAGE": f"{storage}/{port}"}))
                peers.append(f"http://127.0.0.1:{port}")
            for base in peers:
                for _ in range(60):
                    try:
                        if healthy(base):
                            break
                    except OSError:
                        pass
                    time.sleep(1)
                else:
                    raise RuntimeError(f"Peer did not start: {base}")
            suite(peers, binary)
        finally:
            for process in processes:
                process.terminate()
                process.wait()


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--health":
        if not healthy(sys.argv[2]):
            raise RuntimeError("Production bootstrap is not healthy")
        return
    if len(sys.argv) == 3 and sys.argv[1] == "--local":
        local(sys.argv[2])
        return
    suite(["http://bootstrap-a:7070", "http://bootstrap-b:7070"],
          "/workspace/target/debug/test-peer")


def suite(peers, binary):
    cases = []
    for index, base in enumerate(peers):
        status, body = request(base, "/health")
        if status != 200 or body["status"] != "healthy":
            raise RuntimeError(f"Unhealthy peer: {base}")
        peer_id, record = signed_profile(binary)
        status, body = request(base, "/profiles", record)
        if status != 201 or body["peer_id"] != peer_id:
            raise RuntimeError("Production profile creation failed")
        status, body = request(base, f"/profiles/{peer_id}", raw=True)
        if status != 200 or body != record:
            raise RuntimeError("Production profile retrieval changed data")
        try:
            request(base, "/profiles", record)
        except HTTPError as error:
            if error.code != 409:
                raise
        else:
            raise RuntimeError("Stale profile version was accepted")
        other = peers[1 - index]
        try:
            request(other, f"/profiles/{peer_id}")
        except HTTPError as error:
            if error.code != 404:
                raise
        else:
            raise RuntimeError("Independent peer storage unexpectedly shared")
        cases.append({"peer": base, "health": True, "roundtrip": True,
                      "independent_storage": True})
    reports = Path(os.environ.get("RUNNER_TEMP", "/reports"))
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    print("2 production peers: health, profile roundtrip, independence passed")


if __name__ == "__main__":
    main()
