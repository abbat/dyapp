"""Exercise the libp2p node protocol on two independent dyapp-node peers.

`test-peer` is the client: the script has no libp2p library. Without
arguments the peers are the neighboring Docker containers; `--local BIN_DIR`
starts two dyapp-node processes from BIN_DIR on the loopback instead.
`--health MULTIADDR` checks one node.
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

CLIENT = "/workspace/target/debug/test-peer"


def call(client, *args):
    output = subprocess.run([client, *args], check=True, capture_output=True,
                            text=True, timeout=30).stdout
    return json.loads(output)


def healthy(client, addr):
    try:
        reply = call(client, "info", addr)
    except (subprocess.SubprocessError, OSError):
        return False
    return reply["status"] == "STATUS_OK" and "ROLE_STORE" in reply["roles"]


def addresses(host, port):
    """(TCP, QUIC) multiaddrs of a node."""
    return f"/ip4/{host}/tcp/{port}", f"/ip4/{host}/udp/{port}/quic-v1"


def local(bin_dir):
    """Run the suite against two loopback nodes with separate storage."""
    client = f"{bin_dir}/test-peer"
    with tempfile.TemporaryDirectory() as storage:
        peers, processes = [], []
        try:
            for port in (7071, 7072):
                tcp, quic = addresses("127.0.0.1", port)
                node = f"{bin_dir}/dyapp-node"
                processes.append(subprocess.Popen([node], env={
                    **os.environ,
                    "DYAPP_NODE__LISTEN": json.dumps([tcp, quic]),
                    "DYAPP_NODE__STORAGE__DIR": f"{storage}/{port}"}))
                peers.append((tcp, quic))
            for tcp, _ in peers:
                for _ in range(60):
                    if healthy(client, tcp):
                        break
                    time.sleep(1)
                else:
                    raise RuntimeError(f"Node did not start: {tcp}")
            suite(peers, client)
        finally:
            for process in processes:
                process.terminate()
                process.wait()


def main():
    if len(sys.argv) == 3 and sys.argv[1] == "--health":
        if not healthy(CLIENT, sys.argv[2]):
            raise RuntimeError("Node is not healthy")
        return
    if len(sys.argv) == 3 and sys.argv[1] == "--local":
        local(sys.argv[2])
        return
    # The client has no DNS transport: resolve the container names here.
    suite([addresses(socket.gethostbyname(host), 7070)
           for host in ("bootstrap-a", "bootstrap-b")], CLIENT)


def suite(peers, client):
    cases = []
    for index, (tcp, quic) in enumerate(peers):
        for addr in (tcp, quic):
            if not healthy(client, addr):
                raise RuntimeError(f"Unhealthy node: {addr}")
        signed = call(client, "sign-profile")
        peer_id, record = signed["peer_id"], signed["record"]
        if call(client, "publish", tcp, record)["status"] != "STATUS_OK":
            raise RuntimeError("Profile publish failed")
        # Published over TCP, read back over QUIC: the stored bytes are the
        # signed bytes.
        reply = call(client, "get", quic, peer_id)
        if reply != {"status": "STATUS_OK", "record": record}:
            raise RuntimeError("Profile retrieval changed data")
        reply = call(client, "publish", tcp, record)
        if reply != {"status": "STATUS_STALE", "record": record}:
            raise RuntimeError("Stale profile version was accepted")
        other = peers[1 - index][0]
        if call(client, "get", other, peer_id)["status"] != "STATUS_NOT_FOUND":
            raise RuntimeError("Independent node storage unexpectedly shared")
        mailbox(client, tcp, quic, other)
        cases.append({"peer": tcp, "health": True, "roundtrip": True,
                      "independent_storage": True, "mailbox": True})
    reports = Path(os.environ.get("RUNNER_TEMP", "/reports"))
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    print("2 nodes over TCP and QUIC: info, profile roundtrip, stale, "
          "mailbox, independence passed")


def mailbox(client, tcp, quic, other):
    """Put over TCP, fetch over QUIC, a stranger reads nothing, ack empties."""
    device = call(client, "device-key")
    put = call(client, "put", tcp, device["mailbox"], "c0ffee")
    if put["status"] != "STATUS_OK":
        raise RuntimeError("Envelope put failed")
    expected = {"status": "STATUS_OK", "ids": [put["id"]], "more": False}
    if call(client, "fetch", quic, device["secret"]) != expected:
        raise RuntimeError("Mailbox fetch lost the envelope")
    stranger = call(client, "device-key")["secret"]
    if call(client, "fetch", tcp, stranger)["ids"]:
        raise RuntimeError("Another key read the mailbox")
    if call(client, "fetch", other, device["secret"])["ids"]:
        raise RuntimeError("Independent node storage unexpectedly shared")
    reply = call(client, "ack", tcp, device["secret"], put["id"])
    if reply["status"] != "STATUS_OK":
        raise RuntimeError("Mailbox ack failed")
    if call(client, "fetch", tcp, device["secret"])["ids"]:
        raise RuntimeError("Acknowledged envelope still served")


if __name__ == "__main__":
    main()
