"""Checks against live dyappd peers for network-test.py, and a STUN client."""
import base64
import hashlib
import hmac
import json
import os
import socket
import struct
import subprocess
import time

# Node-ID proof of work of test nodes; see tests/docker/compose.network.yml.
POW_BITS = "8"
# coturn static-auth-secret, as in tests/docker/compose.network.yml.
TURN_SECRET = "network-test-secret"


def call(client, *args, env=None):
    result = subprocess.run([client, *args], capture_output=True, text=True,
                            timeout=90, env=env)
    if result.returncode:
        raise subprocess.SubprocessError(
            f"dyappd {args[0]}: {result.stderr.strip()}")
    return json.loads(result.stdout)


def replicate(client, entry, ids):
    """Replicate a fresh profile; every holder is a live node serving it."""
    for _ in range(40):
        signed = call(client, "sign-profile")
        if set(replica_holders(client, entry, signed["peer_id"])) == set(ids):
            break
    else:
        raise RuntimeError("No replica keys spanning every live node")
    holders = call(client, "replicate", entry, signed["peer_id"],
                   signed["record"])["holders"]
    for holder in set(holders):
        if holder not in ids:
            raise RuntimeError(f"Replica on an unknown node: {holder}")
        for _ in range(10):
            reply = call(client, "get", ids[holder], signed["peer_id"])
            if reply == {"status": "STATUS_OK", "record": signed["record"]}:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError(f"Holder {holder} lost the replica")
    return holders


def mailbox(client, tcp, quic, other):
    """Put over TCP, fetch over QUIC, a stranger reads nothing, ack empties,
    a watching fetch gets the next envelope pushed."""
    device, put, _ = spread(client, tcp)
    if put["status"] != "STATUS_OK":
        raise RuntimeError("Envelope put failed")
    expected = {"status": "STATUS_OK", "ids": [put["id"]], "more": False}
    if call(client, "fetch", quic, device["secret"]) != expected:
        raise RuntimeError("Mailbox fetch lost the envelope")
    stranger = call(client, "device-key")["secret"]
    if call(client, "fetch", tcp, stranger)["ids"]:
        raise RuntimeError("Another key read the mailbox")
    if put["id"] not in call(client, "fetch", other, device["secret"])["ids"]:
        raise RuntimeError("Acceptor did not replicate the mailbox")
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
    own = call(client, "info", tcp)["peer_id"]
    for _ in range(40):
        blob = os.urandom(64)
        holders = replica_holders(client, tcp,
                                  hashlib.sha256(blob).hexdigest())
        # A put needs a second holder; all three keys may land on one node.
        if not serves or (own in holders and len(set(holders)) >= 2):
            break
    else:
        raise RuntimeError("No media replica key held by the test node")
    reply = call(client, "media", quic if serves else tcp, blob.hex())
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


def replica_holders(client, entry, key, count=5):
    return [call(client, "closest", entry, key + f"{i:02x}")["peers"][0]
            for i in range(count)]


def media_spread(client, entry, peers, ids, size=512 * 1024):
    """One upload reaches all manifest holders, over TCP and QUIC."""
    for _ in range(40):
        seed = os.urandom(32)
        blob_hash = hashlib.sha256(seed * (size // len(seed))).hexdigest()
        if set(replica_holders(client, entry, blob_hash)) != set(ids):
            continue
        if size > 1024 * 1024:
            shard_holders = replica_holders(client, entry, blob_hash, 7)
            departed = next(key for key, value in ids.items()
                            if value == peers[1][0])
            if shard_holders.count(departed) > 4:
                continue
        break
    else:
        raise RuntimeError("No media keys spanning all three nodes")
    reply = call(client, "media-sized", entry, str(size), seed.hex())
    # A get right after a sharded put can miss shards still fanning out;
    # the loop below waits.
    if any(reply[key] != "STATUS_OK" for key in ("keep", "put")):
        raise RuntimeError(f"{size} byte media upload failed: {reply}")
    for index, (tcp, quic) in enumerate(peers):
        for _ in range(20):
            stored = call(client, "media-get", quic if index % 2 else tcp,
                          blob_hash)
            if stored["status"] == "STATUS_OK":
                data = bytes.fromhex(stored["data"])
                actual_hash = hashlib.sha256(data).hexdigest()
                if len(data) != size or actual_hash != blob_hash:
                    raise RuntimeError("Media replica changed its bytes")
                break
            time.sleep(0.2)
        else:
            raise RuntimeError(f"Media replica missing on {tcp}")
    print(f"Media replication: one {size // 1024} KiB upload on all nodes")
    return blob_hash


def spread(client, entry):
    """One client put reaches all three holders via acceptor fan-out."""
    nodes = set(call(client, "closest", entry, os.urandom(8).hex())["peers"])
    for _ in range(40):
        device = call(client, "device-key")
        found = replica_holders(client, entry, device["mailbox"])
        holders = sorted(set(found))
        if set(holders) == nodes and len(holders) >= 2:
            put = call(client, "put-replicas", entry, device["mailbox"])
            return device, put, holders
    raise RuntimeError(f"Replicas never spanned all nodes: {holders}")


def repaired(client, entry, ids):
    """An envelope on one replica node reaches another on a watching fetch."""
    device, _, holders = spread(client, entry)
    lone = call(
        client, "put-local", ids[holders[0]], device["mailbox"], "02", POW_BITS
    )
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
