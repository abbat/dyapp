"""Exercise the libp2p node protocol on three dyappd peers.

`test-peer` is the client: the script has no libp2p library. Without
arguments the peers are the neighboring Docker containers; `--local BIN_DIR`
starts three dyappd processes from BIN_DIR on the loopback instead and
also stops one to check that replicas skip a departed node.
`--health MULTIADDR` checks one node.

The second and third nodes join through the first; the third allows
`FLOOD_LIMIT` requests per second per peer. Only the first serves media
and TURN credentials; in Docker it hands out a coturn relay.
"""
import base64
import hashlib
import hmac
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time

CLIENT = "/workspace/target/debug/test-peer"
FLOOD_LIMIT = 5
# Node-ID proof of work of the test nodes, as in docker/compose.network.yml.
POW_BITS = "8"
# coturn static-auth-secret, as in docker/compose.network.yml.
TURN_SECRET = "network-test-secret"


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
                       "DYAPPD__LISTEN": f'["127.0.0.1:{port}"]',
                       "DYAPPD__EXTERNAL": f'["127.0.0.1:{port}"]',
                       "DYAPPD__STORAGE__DIR": f"{storage}/{port}",
                       "DYAPPD__NETWORK__ID_POW_BITS": POW_BITS,
                       # Loopback nodes share one /16.
                       "DYAPPD__NETWORK__DISTINCT_OUTBOUND_GROUPS": "false",
                       # Repair and ack forwarding run seconds after
                       # the nodes meet.
                       "DYAPPD__NETWORK__STORAGE_TRUST_MINUTES": "0"}
                if not peers:
                    env["DYAPPD__ROLES"] = '["store", "media", "turn"]'
                    env["DYAPPD__TURN__URLS"] = '["turn:127.0.0.1:3478"]'
                    env["DYAPPD__TURN__SECRET"] = TURN_SECRET
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

            suite(peers, client, churn, relay=False)
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
           for host in ("bootstrap-a", "bootstrap-b", "bootstrap-c")], CLIENT,
          relay=True)


def suite(peers, client, churn=None, relay=False):
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
                      "media": media(client, tcp, quic),
                      "turn": turn(client, tcp, relay)})
    cases.append(network(peers, client, churn))
    reports = Path(os.environ.get("RUNNER_TEMP", "/reports"))
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    churned = ", churn" if churn else ""
    print(f"{len(peers)} nodes over TCP and QUIC: info, profile roundtrip, "
          f"stale, mailbox, push, media, TURN, independence, routing, "
          f"relay announcement, replication, "
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
    # The turn node announces its relay once its routing table fills.
    relays = []
    for _ in range(30):
        relays = call(client, "relays", limited)["providers"]
        if relays:
            break
        time.sleep(1)
    if relays != [call(client, "info", entry)["peer_id"]]:
        raise RuntimeError(f"TURN relays in the DHT: {relays}")
    holders = replicate(client, entry, ids)
    # 15 requests stay within the 16 concurrent streams a node accepts on
    # one connection; more are dropped unanswered.
    counts = call(client, "flood", limited, str(3 * FLOOD_LIMIT))
    # The burst passes, the rest is refused, nothing is dropped.
    answered = min(counts.get("STATUS_NOT_FOUND", 0),
                   counts.get("STATUS_RATE_LIMITED", 0))
    if answered < FLOOD_LIMIT or "failed" in counts:
        raise RuntimeError(f"Rate limit boundary missed: {counts}")
    case = {"routing": True, "relays": relays, "holders": holders,
            "flood": counts,
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


def turn(client, tcp, relay):
    """A turn node's credentials follow coturn's use-auth-secret scheme
    and, with `relay`, allocate on the coturn relay while a wrong password
    does not; other nodes answer UNSUPPORTED."""
    reply = call(client, "turn", tcp)
    if "ROLE_TURN" not in call(client, "info", tcp)["roles"]:
        if reply["status"] != "STATUS_UNSUPPORTED":
            raise RuntimeError(f"TURN on {tcp}: {reply}")
        return False
    digest = hmac.new(TURN_SECRET.encode(), reply["username"].encode(),
                      "sha1").digest()
    password = base64.b64encode(digest).decode()
    if reply["status"] != "STATUS_OK" or reply["password"] != password:
        raise RuntimeError(f"TURN credentials on {tcp}: {reply}")
    if relay:
        host, port = reply["urls"][0].split(":")[1:]
        address = (socket.gethostbyname(host), int(port))
        user = reply["username"]
        if allocate(address, user, "wrong") != 401:
            raise RuntimeError("TURN relay took a wrong password")
        if allocate(address, user, reply["password"]):
            raise RuntimeError("TURN relay refused the node's credentials")
    return True


def allocate(address, username, password):
    """A STUN Allocate with long-term credentials (RFC 5766): 0 on success,
    else the error code."""
    udp = attribute(0x0019, b"\x11\0\0\0")
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.settimeout(2)
        attrs = stun(sock, address, udp)
        if 0x0015 not in attrs:
            return error_code(attrs)
        realm, nonce = attrs[0x0014], attrs[0x0015]
        body = b"".join((udp, attribute(0x0006, username.encode()),
                         attribute(0x0014, realm),
                         attribute(0x0015, nonce)))
        key = hashlib.md5(b":".join((username.encode(), realm,
                                     password.encode())),
                          usedforsecurity=False)
        return error_code(stun(sock, address, body, key.digest()))


def attribute(kind, value):
    padding = b"\0" * (-len(value) % 4)
    return struct.pack("!HH", kind, len(value)) + value + padding


def stun(sock, address, body, key=None):
    """Attributes of the answer to an Allocate; coturn may still be
    starting, so a lost datagram is sent again."""
    txid = os.urandom(12)
    if key:
        header = struct.pack("!HHI12s", 3, len(body) + 24, 0x2112A442, txid)
        mac = hmac.new(key, header + body, "sha1").digest()
        body += attribute(0x0008, mac)
    message = struct.pack("!HHI12s", 3, len(body), 0x2112A442, txid) + body
    for _ in range(15):
        sock.sendto(message, address)
        try:
            reply = sock.recv(2048)
        except TimeoutError:
            continue
        attrs, rest = {}, reply[20:]
        while len(rest) >= 4:
            kind, size = struct.unpack("!HH", rest[:4])
            attrs[kind] = rest[4:4 + size]
            rest = rest[4 + size + (-size % 4):]
        return attrs
    raise RuntimeError(f"No STUN answer from {address}")


def error_code(attrs):
    code = attrs.get(0x0009)
    return (code[2] & 7) * 100 + code[3] if code else 0


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
