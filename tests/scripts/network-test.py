"""Exercise the libp2p node protocol on three dyappd peers.

`dyappd` is also the client: the script has no libp2p library. Without
arguments the peers are the neighboring Docker containers; `--local BIN_DIR`
starts three dyappd processes from BIN_DIR on the loopback instead and
also stops one to check that replicas skip a departed node, checks the deny
list and that idle nodes hold no pending state. `--plateau BIN_DIR` runs the
suite on three loopback nodes for `PLATEAU_SECONDS` and checks that their
memory levels off. `--health MULTIADDR` checks one node.

The second and third nodes join through the first; the third allows
`FLOOD_LIMIT` requests per second per peer. Every node serves media; only the
first serves TURN credentials and, in Docker, hands out a coturn relay.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import socket
import sqlite3
import subprocess
import sys
import tempfile
import time

from check_wrapper import Capture
from memory_report import write_report
from network_checks import (
    POW_BITS, TURN_SECRET, acked_everywhere, call, mailbox, media,
    media_spread, repaired, replica_holders, replicate, turn)

CLIENT = "/workspace/build/debug/dyappd"
FLOOD_LIMIT = 5
PLATEAU_SECONDS = 300
# ponytail: a fixed share of the warm-up peak; tune it if allocator noise
# fails runs without a leak.
PLATEAU_GROWTH = 0.10
ANSI = re.compile(r"\x1b\[[0-9;]*m")
# Output of every local node by port, across restarts, and its drain threads.
LOGS = {}
DRAINS = []


def healthy(client, addr):
    try:
        reply = call(client, "info", addr)
    except (subprocess.SubprocessError, OSError):
        return False
    return reply["status"] == "STATUS_OK" and "ROLE_STORE" in reply["roles"]


def addresses(host, port):
    """(TCP, QUIC) multiaddrs of a node."""
    return f"/ip4/{host}/tcp/{port}", f"/ip4/{host}/udp/{port}/quic-v1"


def start_node(bin_dir, environment):
    """Capture every incarnation of a local node in its port's buffer."""
    if environment.get("DYAPP_VERBOSE") == "1":
        return subprocess.Popen([f"{bin_dir}/dyappd"], env=environment)
    port = Path(environment["DYAPPD_STORAGE_DIR"]).name
    output, drain = LOGS.setdefault(port, Capture()).pipe()
    DRAINS.append(drain)
    try:
        return subprocess.Popen([f"{bin_dir}/dyappd"], env=environment,
                                stdout=output, stderr=subprocess.STDOUT)
    finally:
        os.close(output)


def dyappd(bin_dir, env, *args):
    """Run a dyappd subcommand on a node's storage; its standard output."""
    return subprocess.run([f"{bin_dir}/dyappd", *args], env=env, check=True,
                          capture_output=True, text=True).stdout


def start(bin_dir, storage, peers, processes, environments,
          flood=True):
    """Start three loopback nodes, appending to the lists the caller stops.

    The first serves TURN and shares its deny list; the third accepts deny
    lists, fetching them every maintenance minute, and with `flood` limits
    every peer to FLOOD_LIMIT requests per second.
    """
    client = f"{bin_dir}/dyappd"
    for port in (7071, 7072, 7073):
        tcp, quic = addresses("127.0.0.1", port)
        env = {**os.environ,
               "DYAPPD_LISTEN": f'["127.0.0.1:{port}"]',
               "DYAPPD_EXTERNAL": f'["127.0.0.1:{port}"]',
               "DYAPPD_STORAGE_DIR": f"{storage}/{port}",
               "DYAPPD_ROLES": '["store", "search", "media"]',
               "DYAPPD_NETWORK_ID_POW_BITS": POW_BITS,
               # The test client signs zero-difficulty profiles.
               "DYAPPD_SEARCH_MIN_PROFILE_POW_BITS": "0",
               # Loopback nodes share one /16.
               "DYAPPD_NETWORK_DISTINCT_OUTBOUND_GROUPS": "false",
               "DYAPPD_NETWORK_GUARD_DIALS": "false",
               # Repair and ack forwarding run seconds after
               # the nodes meet.
               "DYAPPD_NETWORK_STORAGE_TRUST_MINUTES": "0"}
        # Local suite clients share loopback and outpace idle cleanup.
        env["DYAPPD_LIMITS_MAX_CONNECTIONS_PER_IP_GROUP"] = "1000"
        if not peers:
            if loopback_v6():
                env["DYAPPD_LISTEN"] = f'["127.0.0.1:{port}", "[::1]:{port}"]'
            env["DYAPPD_ROLES"] = '["store", "search", "media", "turn"]'
            env["DYAPPD_TURN_URLS"] = '["turn:127.0.0.1:3478"]'
            env["DYAPPD_TURN_SECRET"] = TURN_SECRET
            env["DYAPPD_NETWORK_SHARE_DENY_LIST"] = "true"
        if peers:
            env["DYAPPD_SEEDS"] = '["127.0.0.1:7071"]'
        if port == 7073 and flood:
            env["DYAPPD_LIMITS_REQUESTS_PER_SECOND"] = str(FLOOD_LIMIT)
        if port == 7073:
            env["DYAPPD_NETWORK_ACCEPT_DENY_LISTS"] = "true"
            env["DYAPPD_MAINTENANCE_INTERVAL_MINUTES"] = "1"
        subprocess.run([f"{bin_dir}/dyappd", "keygen"], env=env,
                       check=True, stdout=subprocess.DEVNULL)
        environments.append(env)
        processes.append(start_node(bin_dir, env))
        peers.append((tcp, quic))
        # A node dials its seed once, at start.
        for _ in range(60):
            if healthy(client, tcp):
                break
            time.sleep(1)
        else:
            raise RuntimeError(f"Node did not start: {tcp}")
    friendly_addresses(client)


def loopback_v6():
    """Whether this host can listen on ::1."""
    try:
        with socket.socket(socket.AF_INET6) as probe:
            probe.bind(("::1", 0))
        return True
    except OSError:
        return False


def friendly_addresses(client):
    """The client dials host:port with an IPv4, bracketed IPv6 or DNS host."""
    forms = ["127.0.0.1:7071", "localhost:7071"]
    if loopback_v6():
        forms.append("[::1]:7071")
    for addr in forms:
        if call(client, "info", addr)["status"] != "STATUS_OK":
            raise RuntimeError(f"info {addr} failed")


def stop(processes, failed):
    """Stop every node; after a failure print their output."""
    if failed:
        # The node state line shows what each node still held.
        for process in processes:
            if process.poll() is None:
                process.send_signal(signal.SIGHUP)
        time.sleep(1)
    for process in processes:
        process.terminate()
        process.wait()
    if failed:
        for drain in DRAINS:
            drain.join(timeout=10)
        for port, captured in sorted(LOGS.items()):
            print(f"--- node {port} ---", flush=True)
            sys.stdout.write(captured.getvalue().decode(errors="replace"))
        sys.stdout.flush()


def local(bin_dir):
    """Run the suite against three loopback nodes with separate storage."""
    client = f"{bin_dir}/dyappd"
    Path("/tmp/ai").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir="/tmp/ai") as storage:
        peers, processes, environments = [], [], []
        failed = False
        try:
            start(bin_dir, storage, peers, processes, environments)
            denied = deny_mailbox(client, bin_dir, peers[0], processes[0],
                                  environments[0])
            profile_repair(client, bin_dir, peers, processes, environments)
            media_repair(client, bin_dir, storage, peers)

            def churn():
                processes[1].terminate()
                processes[1].wait()
                return peers[1][0]

            suite(peers, client, churn, relay=False,
                  reports=Path(storage) / "reports", storage=storage)
            drained(processes, (0, 2))
            sharer = call(client, "info", peers[0][0])["peer_id"]
            shared_deny(bin_dir, environments[2], sharer, denied)
            deny_group(client, bin_dir, peers[2], processes[2],
                       environments[2])
            processes[2].terminate()
            processes[2].wait()
            signed = call(client, "sign-profile")
            reply = call(client, "publish", peers[0][0], signed["record"])
            if reply["status"] != "STATUS_FULL":
                raise RuntimeError(f"No second profile holder: {reply}")
            device = call(client, "device-key")
            reply = call(client, "put", peers[0][0], device["mailbox"], "01")
            if reply["status"] != "STATUS_FULL":
                raise RuntimeError(f"No second mailbox holder: {reply}")
            print("Missing second holder: profile and mailbox refused")
            media_retention(client, bin_dir, storage, peers[0], processes,
                            environments[0])
            named_network(client, bin_dir, storage, processes)
        except BaseException:
            failed = True
            raise
        finally:
            stop(processes, failed)


def named_network(client, bin_dir, storage, processes):
    """A node of a named network serves only clients of that name and its
    storage refuses a start under another name."""
    tcp, _ = addresses("127.0.0.1", 7074)
    env = {**os.environ,
           "DYAPPD_LISTEN": '["127.0.0.1:7074"]',
           "DYAPPD_STORAGE_DIR": f"{storage}/7074",
           "DYAPPD_NETWORK_ID_POW_BITS": POW_BITS,
           "DYAPPD_NETWORK_NAME": "lab"}
    dyappd(bin_dir, env, "keygen")
    if (Path(storage) / "7074" / "network").read_text() != "lab":
        raise RuntimeError("keygen did not bind the storage to lab")
    processes.append(start_node(bin_dir, env))
    for _ in range(60):
        try:
            reply = call(client, "info", tcp, env=env)
            break
        except (subprocess.SubprocessError, OSError):
            time.sleep(1)
    else:
        raise RuntimeError(f"Named node did not start: {tcp}")
    if reply["status"] != "STATUS_OK":
        raise RuntimeError(f"Named client refused: {reply}")
    try:
        call(client, "info", tcp)
    except subprocess.SubprocessError:
        pass
    else:
        raise RuntimeError("A public client reached the lab node")
    processes[-1].terminate()
    processes[-1].wait()
    public = {k: v for k, v in env.items() if k != "DYAPPD_NETWORK_NAME"}
    result = subprocess.run([f"{bin_dir}/dyappd"], env=public,
                            capture_output=True, text=True, timeout=30)
    if result.returncode == 0 or "storage belongs to" not in result.stderr:
        raise RuntimeError(f"Lab storage started as public: {result.stderr}")
    print("Named network: lab client served, public client and storage "
          "refused")


def plateau(bin_dir):
    """Repeat mailbox and media load for PLATEAU_SECONDS; resident memory
    must level off.

    The first round warms caches up. Afterwards the peak of the second half
    may exceed the peak of the first half by PLATEAU_GROWTH at most.
    """
    client = f"{bin_dir}/dyappd"
    Path("/tmp/ai").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir="/tmp/ai") as storage:
        peers, processes, environments = [], [], []
        failed = False
        try:
            # Without the flood limit: node-to-node replication would hit it.
            start(bin_dir, storage, peers, processes, environments,
                  flood=False)

            def load():
                # The suite's per-node load; the full suite assumes one pass
                # (repair is shed under load).
                for index, (tcp, quic) in enumerate(peers):
                    mailbox(client, tcp, quic,
                            peers[(index + 1) % len(peers)][0])
                    media(client, tcp, quic)

            load()
            samples, started = [], time.monotonic()
            end = started + PLATEAU_SECONDS
            while time.monotonic() < end:
                load()
                samples.append({
                    "elapsed_seconds": time.monotonic() - started,
                    "rss_kib": [rss(process) for process in processes]})
            path, report = write_report(
                samples, time.monotonic() - started, PLATEAU_SECONDS,
                PLATEAU_GROWTH, Path(bin_dir) / "dyappd",
                os.environ.get("DYAPP_MEMORY_REPORT_DIR", "build"))
            print(f"Memory plateau report: {path}")
            for node in report["nodes"]:
                print(f"Node {node['node']} RSS peak: "
                      f"{node['first_peak_kib']} kB, then "
                      f"{node['second_peak_kib']} kB "
                      f"over {len(samples)} rounds")
            if report["failed_nodes"]:
                raise RuntimeError(
                    f"Nodes {report['failed_nodes']} memory still grows; "
                    f"RSS report: {path}")
            print("Memory plateau: every node levels off")
        except BaseException:
            failed = True
            raise
        finally:
            stop(processes, failed)


def rss(process):
    """Resident memory of a running process in kB."""
    status = Path(f"/proc/{process.pid}/status").read_text()
    return int(re.search(r"^VmRSS:\s+(\d+)", status, re.M).group(1))


def node_state(process, captured):
    """Send SIGHUP and return the fields of the node state line it logs."""
    mark = captured.total
    process.send_signal(signal.SIGHUP)
    for _ in range(50):
        text = ANSI.sub("", captured.since(mark).decode(errors="replace"))
        found = [line for line in text.splitlines() if "node state" in line]
        if found:
            return {key: int(value)
                    for key, value in re.findall(r"(\w+)=(\d+)", found[-1])}
        time.sleep(0.1)
    raise RuntimeError("No node state line after SIGHUP")


def drained(processes, indexes):
    """Idle nodes hold no pending request and nothing of a closed connection.

    Pending requests end by answer or the 20 s request timeout; lookups that
    still route to the stopped node take longer.
    """
    if os.environ.get("DYAPP_VERBOSE") == "1":
        print("Node state: skipped, node logs are streamed")
        return
    for index in indexes:
        captured = LOGS[str(7071 + index)]
        for _ in range(90):
            state = node_state(processes[index], captured)
            idle = state["pending"] == state["watches"] == 0
            if idle and state["connection_entries"] <= state["connections"]:
                break
            time.sleep(1)
        else:
            raise RuntimeError(f"Node state did not drain: {state}")
    print("Node state: no pending request or closed connection left")


def deny_mailbox(client, bin_dir, peer, process, env):
    """An operator entry refuses a mailbox on the running node after SIGHUP."""
    mailbox = call(client, "device-key")["mailbox"]
    dyappd(bin_dir, env, "deny", "add", mailbox, "network test")
    process.send_signal(signal.SIGHUP)
    for _ in range(50):
        reply = call(client, "put", peer[0], mailbox, "01")
        if reply["status"] == "STATUS_REFUSED":
            break
        time.sleep(0.1)
    else:
        raise RuntimeError(f"Denied mailbox accepted: {reply}")
    print("Deny list: a listed mailbox is refused after SIGHUP")
    return mailbox


def shared_deny(bin_dir, env, sharer, entry):
    """The accepting node stores the sharing node's signed list."""
    for _ in range(90):
        received = dyappd(bin_dir, env, "deny", "received")
        if f"{sharer}\t" in received and f"\t{entry}\n" in received:
            break
        time.sleep(1)
    else:
        raise RuntimeError(f"Shared deny list not received: {received}")
    print("Deny list: a shared list reaches an accepting node")


def deny_group(client, bin_dir, peer, process, env):
    """The client's IP group is refused while listed, served once removed."""
    group = "127.0.0.1/24"
    for action, refused in (("add", True), ("remove", False)):
        dyappd(bin_dir, env, "deny", action, group)
        process.send_signal(signal.SIGHUP)
        for _ in range(50):
            mailbox = call(client, "device-key")["mailbox"]
            reply = call(client, "put", peer[0], mailbox, "01")
            if (reply["status"] == "STATUS_REFUSED") == refused:
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Deny {action} {group}: {reply}")
    print("Deny list: a listed IP group is refused, served once removed")


def profile_repair(client, bin_dir, peers, processes, environments):
    """A holder misses a newer publish while offline; heartbeat repairs it."""
    node_ids = [call(client, "info", tcp)["peer_id"] for tcp, _ in peers]
    ids = set(node_ids)
    for _ in range(40):
        signed = call(client, "sign-profile")
        if set(replica_holders(client, peers[0][0], signed["peer_id"])) == ids:
            break
    else:
        raise RuntimeError("No profile replica keys spanning all three nodes")
    reply = call(client, "publish", peers[0][0], signed["record"])
    if reply["status"] != "STATUS_OK":
        raise RuntimeError("Initial repair profile publish failed")
    for tcp, _ in peers:
        for _ in range(30):
            reply = call(client, "get", tcp, signed["peer_id"])
            if reply.get("record") == signed["record"]:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("Initial profile replica missing")
    processes[1].terminate()
    processes[1].wait()
    # Let DHT lookups discard the departed holder before the bounded write.
    for _ in range(30):
        holders = replica_holders(client, peers[0][0], signed["peer_id"])
        if set(holders) == ids - {node_ids[1]}:
            break
        time.sleep(1)
    else:
        raise RuntimeError("Routing did not converge after holder departure")
    newer = call(client, "sign-profile", signed["secret"], "2")
    reply = call(client, "publish", peers[0][0], newer["record"])
    if reply["status"] != "STATUS_OK":
        raise RuntimeError(f"New publish with offline holder failed: {reply}")
    for _ in range(30):
        reply = call(client, "get", peers[2][0], signed["peer_id"])
        if reply.get("record") == newer["record"]:
            break
        time.sleep(0.2)
    else:
        raise RuntimeError("Live holder did not get the newer version")
    processes[1] = start_node(bin_dir, environments[1])
    for _ in range(60):
        if healthy(client, peers[1][0]):
            break
        time.sleep(1)
    else:
        raise RuntimeError("Profile holder did not restart")
    if call(client, "info", peers[1][0])["peer_id"] != node_ids[1]:
        raise RuntimeError("Restarted holder changed its peer ID")
    reply = call(client, "get", peers[1][0], signed["peer_id"])
    if reply.get("record") != signed["record"]:
        raise RuntimeError("Restarted holder did not retain its old version")
    # Replica puts never start repair. The third node has not inventoried
    # this owner, so its first heartbeat is eligible under the hourly limit.
    for _ in range(30):
        holders = replica_holders(client, peers[2][0], signed["peer_id"])
        if set(holders) == ids:
            break
        time.sleep(1)
    else:
        raise RuntimeError("Restarted holder missing from routing")
    reply = call(client, "heartbeat", peers[2][1], signed["secret"])
    if reply["status"] != "STATUS_OK":
        raise RuntimeError("Owner heartbeat failed")
    for _ in range(60):
        reply = call(client, "get", peers[1][1], signed["peer_id"])
        if reply.get("record") == newer["record"]:
            break
        time.sleep(0.2)
    else:
        raise RuntimeError(f"Heartbeat did not repair profile: {reply}")
    print("Profile repair: restarted holder receives the newer version")


def media_cache(client, storage, peer, blob_hash):
    """A later get uses the cache after this node loses its shard metadata."""
    target = Path(storage) / "7072"
    hash_bytes = bytes.fromhex(blob_hash)
    with sqlite3.connect(target / "media.db") as db:
        cached = db.execute("SELECT `size` FROM `cache` WHERE `hash` = ?",
                            (hash_bytes,)).fetchone()
        shards = db.execute("SELECT COUNT(*) FROM `shards` WHERE `hash` = ?",
                            (hash_bytes,)).fetchone()[0]
        if cached != (3 * 1024 * 1024,) or shards == 0:
            raise RuntimeError("Sharded media was not assembled and cached")
        db.execute("DELETE FROM `shards` WHERE `hash` = ?", (hash_bytes,))
        db.execute("DELETE FROM `manifests` WHERE `hash` = ?", (hash_bytes,))
    stored = call(client, "media-get", peer, blob_hash)
    if stored["status"] != "STATUS_OK" or hashlib.sha256(
            bytes.fromhex(stored["data"])).hexdigest() != blob_hash:
        raise RuntimeError("Cached media unreadable after losing local shards")
    print("Media cache: later pieces survive loss of local shard metadata")


def media_repair(client, bin_dir, storage, peers):
    """Remove only temporary fixture copies/shards and repair them by keep."""
    ids = {call(client, "info", tcp)["peer_id"]: tcp for tcp, _ in peers}
    entry = peers[0][0]
    for size in (512 * 1024, 3 * 1024 * 1024):
        for _ in range(40):
            seed = os.urandom(32)
            blob_hash = hashlib.sha256(seed * (size // 32)).hexdigest()
            if set(replica_holders(client, entry, blob_hash)) == set(ids):
                break
        else:
            raise RuntimeError("No media repair keys spanning all nodes")
        upload = call(client, "media-sized", entry, str(size), seed.hex())
        # A get right after a sharded put can miss shards still fanning out;
        # the holder loop below waits for every copy.
        if any(upload[k] != "STATUS_OK" for k in ("keep", "put")):
            raise RuntimeError(f"Media repair upload failed: {upload}")
        details = upload["details"]
        # Every selected holder must have finished the background fan-out.
        for tcp, _ in peers:
            for _ in range(30):
                if call(client, "media-get", tcp, blob_hash)["status"] == (
                        "STATUS_OK"):
                    break
                time.sleep(0.2)
            else:
                raise RuntimeError("Media repair source not ready")
        target = Path(storage) / "7072"
        hash_bytes = bytes.fromhex(blob_hash)
        if size <= 1024 * 1024:
            copy = target / "data" / blob_hash[:2] / blob_hash[2:4]
            (copy / blob_hash).unlink()

            def check():
                return (copy / blob_hash).is_file()
        else:
            with sqlite3.connect(target / "media.db") as db:
                row = db.execute("SELECT idx FROM shards WHERE hash = ? "
                                 "LIMIT 1", (hash_bytes,)).fetchone()
                if row is None:
                    raise RuntimeError("Media repair target has no shard")
                index = row[0]
                db.execute("DELETE FROM shards WHERE hash = ? AND idx = ?",
                           (hash_bytes, index))

            def check():
                with sqlite3.connect(target / "media.db") as db:
                    return db.execute(
                        "SELECT EXISTS (SELECT 1 FROM shards "
                        "WHERE hash = ? AND idx = ?)",
                        (hash_bytes, index)).fetchone()[0]

        # The initial keep already inventoried this hash on the acceptor.
        # Another holder has its own hourly schedule, as on a real owner visit.
        visit = peers[2][0]
        refreshed = call(client, "media-refresh", visit,
                         details["keep_record"], details["secret"])
        if refreshed["status"] != "STATUS_OK" or blob_hash in (
                refreshed["missing"]):
            raise RuntimeError(f"Media keep repair failed: {refreshed}")
        for _ in range(30):
            if check():
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("Keep did not restore the missing media item")
        if call(client, "media-get", peers[1][0], blob_hash)["status"] != (
                "STATUS_OK"):
            raise RuntimeError("Repaired media unreadable")
    print("Media repair: keep restores a whole copy and a missing shard")


def media_retention(client, bin_dir, storage, peer, processes, env):
    """Age fixture rows, refresh one keep, then exercise startup cleanup.

    This runs on the three-node loopback network inside the dev Docker image.
    """
    tcp, quic = peer
    peer_id = call(client, "info", tcp)["peer_id"]
    # The other two nodes have departed: restart the acceptor to exercise the
    # isolated-node exception without their stale in-memory routing entries.
    processes[0].terminate()
    processes[0].wait()
    processes[0] = start_node(bin_dir, env)
    for _ in range(60):
        if healthy(client, tcp):
            break
        time.sleep(1)
    else:
        raise RuntimeError("Media acceptor did not restart in isolation")
    if call(client, "info", tcp)["peer_id"] != peer_id:
        raise RuntimeError("Restarted node changed its peer ID")
    expired, refreshed = [
        call(client, "media-owned", tcp, os.urandom(64).hex())
        for _ in range(2)
    ]
    for result in (expired, refreshed):
        if any(result[key] != "STATUS_OK" for key in ("keep", "put", "get")):
            raise RuntimeError(f"Media retention setup failed: {result}")
    with sqlite3.connect(f"{storage}/7071/media.db") as db:
        cutoff = int(time.time()) - 31 * 86400
        for result in (expired, refreshed):
            db.execute("UPDATE owners SET last_seen = ? WHERE owner = ?",
                       (cutoff, result["details"]["owner"]))
    record = refreshed["details"]["keep_record"]
    if call(client, "media-keep", quic, record)["status"] != "STATUS_OK":
        raise RuntimeError("Fresh signed keep did not refresh media liveness")
    processes[0].terminate()
    processes[0].wait()
    processes[0] = start_node(bin_dir, env)
    for _ in range(60):
        if healthy(client, tcp):
            break
        time.sleep(1)
    else:
        raise RuntimeError("Media node did not restart")
    for result, expected in ((expired, "STATUS_NOT_FOUND"),
                             (refreshed, "STATUS_OK")):
        reply = call(client, "media-get", quic, result["details"]["hash"])
        if reply["status"] != expected:
            raise RuntimeError(f"Media retention cleanup failed: {reply}")
    print("Media retention: expired blob removed, refreshed keep survives")


def main():
    usage = ("Usage: network-test.py [--health MULTIADDR | --local BIN_DIR "
             "| --plateau BIN_DIR]")
    if sys.argv[1:] in (["--help"], ["-h"]):
        print(usage)
        return
    if len(sys.argv) == 3 and sys.argv[1] == "--health":
        if not healthy(CLIENT, sys.argv[2]):
            raise RuntimeError("Node is not healthy")
        return
    if len(sys.argv) == 3 and sys.argv[1] == "--local":
        local(sys.argv[2])
        return
    if len(sys.argv) == 3 and sys.argv[1] == "--plateau":
        plateau(sys.argv[2])
        return
    if len(sys.argv) != 1:
        raise ValueError(usage)
    # The client has no DNS transport: resolve the container names here.
    suite([addresses(socket.gethostbyname(host), 33445)
           for host in ("bootstrap-a", "bootstrap-b", "bootstrap-c")], CLIENT,
          relay=True)


def suite(peers, client, churn=None, relay=False, reports=None, storage=None):
    cases = []
    for index, (tcp, quic) in enumerate(peers):
        for addr in (tcp, quic):
            if not healthy(client, addr):
                raise RuntimeError(f"Unhealthy node: {addr}")
        own = call(client, "info", tcp)["peer_id"]
        for _ in range(40):
            signed = call(client, "sign-profile")
            holders = replica_holders(client, tcp, signed["peer_id"])
            if own in holders and len(set(holders)) >= 2:
                break
        else:
            raise RuntimeError("No profile keys held by the test node")
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
        other_id = call(client, "info", other)["peer_id"]
        expected_status = ("STATUS_OK" if other_id in holders
                           else "STATUS_NOT_FOUND")
        for _ in range(10):
            reply = call(client, "get", other, peer_id)
            if reply["status"] == expected_status:
                break
            time.sleep(0.2)
        else:
            raise RuntimeError("Profile placement differs from holders")
        mailbox(client, tcp, quic, other)
        cases.append({"peer": tcp, "health": True, "roundtrip": True,
                      "holder_placement": True, "mailbox": True,
                      "media": media(client, tcp, quic),
                      "turn": turn(client, tcp, relay)})
    cases.append(network(peers, client, churn, storage))
    reports = reports or Path(os.environ.get("RUNNER_TEMP", "/reports"))
    reports.mkdir(exist_ok=True)
    (reports / "network-results.json").write_text(json.dumps(cases, indent=2))
    churned = ", churn" if churn else ""
    print(f"{len(peers)} nodes over TCP and QUIC: info, profile roundtrip, "
          f"stale, mailbox, push, media, TURN, holder placement, routing, "
          f"relay announcement, replication, "
          f"ack forwarding, repair, rate limit"
          f"{churned} passed")


def network(peers, client, churn, storage):
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
    media_spread(client, entry, peers, ids)
    sharded_hash = media_spread(client, entry, peers, ids, 3 * 1024 * 1024)
    if storage:
        media_cache(client, storage, peers[1][0], sharded_hash)
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
        for tcp in live.values():
            stored = call(client, "media-get", tcp, sharded_hash)
            data = bytes.fromhex(stored["data"])
            checksum = hashlib.sha256(data).hexdigest()
            if stored["status"] != "STATUS_OK" or checksum != sharded_hash:
                raise RuntimeError("Sharded media unreadable after node loss")
        print("Media shards: 3 MiB survives loss of up to four shards")
    return case


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("rust-network")
    main()
