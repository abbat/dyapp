"""Exercise the libp2p node protocol on three dyappd peers.

`test-peer` is the client: the script has no libp2p library. Without
arguments the peers are the neighboring Docker containers; `--local BIN_DIR`
starts three dyappd processes from BIN_DIR on the loopback instead and
also stops one to check that replicas skip a departed node.
`--health MULTIADDR` checks one node.

The second and third nodes join through the first; the third allows
`FLOOD_LIMIT` requests per second per peer. Only the first serves media.
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
FLOOD_LIMIT = 5
# Node-ID proof of work of the test nodes, as in docker/compose.network.yml.
POW_BITS = "8"


def call(client, *args):
    result = subprocess.run([client, *args], capture_output=True, text=True,
                            timeout=90)
    if result.returncode:
        raise subprocess.SubprocessError(
            f"test-peer {args[0]}: {result.stderr.strip()}")
    return json.loads(result.stdout)


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
    """Run the suite against three loopback nodes with separate storage."""
    client = f"{bin_dir}/test-peer"
    with tempfile.TemporaryDirectory() as storage:
        peers, processes = [], []
        try:
            for port in (7071, 7072, 7073):
                tcp, quic = addresses("127.0.0.1", port)
                env = {**os.environ,
                       "DYAPPD__LISTEN": json.dumps([tcp, quic]),
                       "DYAPPD__EXTERNAL": json.dumps([tcp, quic]),
                       "DYAPPD__STORAGE__DIR": f"{storage}/{port}",
                       "DYAPPD__NETWORK__ID_POW_BITS": POW_BITS,
                       # Loopback nodes share one /16.
                       "DYAPPD__NETWORK__DISTINCT_OUTBOUND_GROUPS": "false"}
                if not peers:
                    env["DYAPPD__ROLES"] = '["store", "media"]'
                if peers:
                    env["DYAPPD__SEEDS"] = json.dumps([peers[0][0]])
                if port == 7073:
                    env["DYAPPD__LIMITS__REQUESTS_PER_SECOND"] = str(
                        FLOOD_LIMIT)
                subprocess.run([f"{bin_dir}/dyappd", "keygen"], env=env,
                               check=True, stdout=subprocess.DEVNULL)
                processes.append(subprocess.Popen(
                    [f"{bin_dir}/dyappd"], env=env))
                peers.append((tcp, quic))
                # A node dials its seed once, at start.
                for _ in range(60):
                    if healthy(client, tcp):
                        break
                    time.sleep(1)
                else:
                    raise RuntimeError(f"Node did not start: {tcp}")

            def churn():
                processes[1].terminate()
                processes[1].wait()
                return peers[1][0]

            suite(peers, client, churn)
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
           for host in ("bootstrap-a", "bootstrap-b", "bootstrap-c")], CLIENT)


def suite(peers, client, churn=None):
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
        other = peers[(index + 1) % len(peers)][0]
        if call(client, "get", other, peer_id)["status"] != "STATUS_NOT_FOUND":
            raise RuntimeError("Independent node storage unexpectedly shared")
        mailbox(client, tcp, quic, other)
        cases.append({"peer": tcp, "health": True, "roundtrip": True,
                      "independent_storage": True, "mailbox": True,
                      "media": media(client, tcp, quic)})
    cases.append(network(peers, client, churn))
    reports = Path(os.environ.get("RUNNER_TEMP", "/reports"))
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    churned = ", churn" if churn else ""
    print(f"{len(peers)} nodes over TCP and QUIC: info, profile roundtrip, "
          f"stale, mailbox, push, media, independence, routing, replication, "
          f"ack forwarding, repair, rate limit"
          f"{churned} passed")


def network(peers, client, churn):
    """Routing, replication, ack forwarding, the rate limit and churn."""
    ids = {call(client, "info", tcp)["peer_id"]: tcp for tcp, _ in peers}
    entry, limited = peers[0][0], peers[-1][0]
    # The last node knows only its seed at first: a lookup through it finds
    # every node once the seed has learned the others' addresses.
    for _ in range(30):
        found = call(client, "closest", limited, os.urandom(8).hex())
        if set(found["peers"]) == set(ids):
            break
        time.sleep(1)
    else:
        raise RuntimeError(f"Routing found {found['peers']}, not {list(ids)}")
    holders = replicate(client, entry, ids)
    # 15 requests stay within the 16 concurrent streams a node accepts on
    # one connection; more are dropped unanswered.
    counts = call(client, "flood", limited, str(3 * FLOOD_LIMIT))
    # The burst passes, the rest is refused, nothing is dropped.
    answered = min(counts.get("STATUS_NOT_FOUND", 0),
                   counts.get("STATUS_RATE_LIMITED", 0))
    if answered < FLOOD_LIMIT or "failed" in counts:
        raise RuntimeError(f"Rate limit boundary missed: {counts}")
    case = {"routing": True, "holders": holders, "flood": counts,
            "acked_on": acked_everywhere(client, entry, ids),
            "repaired_on": repaired(client, entry, ids)}
    if churn:
        gone = churn()
        live = {peer: tcp for peer, tcp in ids.items() if tcp != gone}
        case["holders_after_churn"] = replicate(client, entry, live)
    return case


def replicate(client, entry, ids):
    """Replicate a fresh profile; every holder is a live node serving it."""
    signed = call(client, "sign-profile")
    holders = call(client, "replicate", entry, signed["peer_id"],
                   signed["record"])["holders"]
    for holder in set(holders):
        if holder not in ids:
            raise RuntimeError(f"Replica on an unknown node: {holder}")
        reply = call(client, "get", ids[holder], signed["peer_id"])
        if reply != {"status": "STATUS_OK", "record": signed["record"]}:
            raise RuntimeError(f"Holder {holder} lost the replica")
    return holders


def mailbox(client, tcp, quic, other):
    """Put over TCP, fetch over QUIC, a stranger reads nothing, ack empties,
    a watching fetch gets the next envelope pushed."""
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
    watched = call(client, "watch", quic, device["secret"])
    if watched["pushed"] != [watched["id"]]:
        raise RuntimeError(f"Envelope not pushed to the watcher: {watched}")


def media(client, tcp, quic):
    """A blob kept, put and read back over QUIC on a media node; a
    node without the role answers UNSUPPORTED."""
    serves = "ROLE_MEDIA" in call(client, "info", tcp)["roles"]
    blob = os.urandom(64).hex()
    reply = call(client, "media", quic if serves else tcp, blob)
    expected = "STATUS_OK" if serves else "STATUS_UNSUPPORTED"
    if reply != {"keep": expected, "put": expected, "get": expected}:
        raise RuntimeError(f"Media roundtrip on {tcp}: {reply}")
    return serves


def spread(client, entry):
    """An envelope on every replica node of a fresh mailbox."""
    # Five replica keys may all land on one node of three: retry with a new
    # mailbox until the replicas span nodes.
    for _ in range(5):
        device = call(client, "device-key")
        put = call(client, "put-replicas", entry, device["mailbox"])
        holders = sorted(set(put["holders"]))
        if len(holders) > 1:
            return device, put, holders
    raise RuntimeError(f"Replicas never spanned nodes: {holders}")


def repaired(client, entry, ids):
    """An envelope on one replica node reaches another on a watching fetch."""
    device, _, holders = spread(client, entry)
    lone = call(client, "put", ids[holders[0]], device["mailbox"], "02")
    call(client, "watch", ids[holders[1]], device["secret"])
    # The inventory and its answer run after the fetch.
    for _ in range(10):
        if lone["id"] in call(client, "fetch", ids[holders[1]],
                              device["secret"])["ids"]:
            return holders[:2]
        time.sleep(1)
    raise RuntimeError(f"Envelope not repaired onto {holders[1]}")


def acked_everywhere(client, entry, ids):
    """An envelope on every replica node; an ack on one clears them all."""
    device, put, holders = spread(client, entry)
    reply = call(client, "ack", ids[holders[0]], device["secret"], put["id"])
    if reply["status"] != "STATUS_OK":
        raise RuntimeError("Mailbox ack failed")
    # The node forwards the ack after it answers.
    for _ in range(10):
        left = [holder for holder in holders
                if call(client, "fetch", ids[holder], device["secret"])["ids"]]
        if not left:
            return holders
        time.sleep(1)
    raise RuntimeError(f"Ack not forwarded to {left}")


if __name__ == "__main__":
    main()
