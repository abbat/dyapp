"""Exercise real bootstrap routes in neighboring Docker containers."""
import json
from pathlib import Path
import subprocess
import sys
from urllib.error import HTTPError
from urllib.request import Request, urlopen


def request(base, route, body=None, raw=False):
    operation = Request(base + route, data=body,
                        headers={"Content-Type": "application/x-protobuf"})
    with urlopen(operation, timeout=15) as response:
        data = response.read()
        return response.status, data if raw else json.loads(data)


def signed_profile():
    """A fresh identity's signed profile: (peer_id, protobuf bytes)."""
    output = subprocess.run(["/workspace/target/debug/test-peer",
                             "sign-profile"], check=True,
                            capture_output=True, text=True).stdout
    signed = json.loads(output)
    return signed["peer_id"], bytes.fromhex(signed["record"])


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--health":
        status, body = request(sys.argv[2], "/health")
        if status != 200 or body["status"] != "healthy":
            raise RuntimeError("Production bootstrap is not healthy")
        return
    subprocess.run(["bash", "/workspace/scripts/check-container-isolation.sh"],
                   check=True)
    peers = sys.argv[1:] or [
        "http://bootstrap-a:7070", "http://bootstrap-b:7070"]
    if len(peers) != 2:
        raise ValueError("Two neighboring bootstrap peers required")
    cases = []
    for index, base in enumerate(peers):
        status, body = request(base, "/health")
        if status != 200 or body["status"] != "healthy":
            raise RuntimeError(f"Unhealthy peer: {base}")
        peer_id, record = signed_profile()
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
    reports = Path("/reports")
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    print("2 production peers: health, profile roundtrip, independence passed")


if __name__ == "__main__":
    main()
