"""Rehearse a first deployment on three loopback nodes, production defaults.

Usage: rehearsal.py OLD_BIN_DIR NEW_BIN_DIR. Both hold `dyappd`; the
NEW_BIN_DIR one is also the client. The nodes listen on 127.1.0.1, 127.2.0.1
and 127.3.0.1, three /16 groups, so `network.distinct_outbound_groups` stays
on; only `network.guard_dials` is off, since the nodes dial loopback
addresses. Settings in the environment (a
`DYAPPD_NETWORK_STORAGE_TRUST_MINUTES` for a dry run, say) reach every node.

Steps, as in docs/manuals/bootstrap.md#first-deployment:
cold start of the old version, a canary that needs two holders, a rolling
upgrade one node at a time, and a rollback of the last node within the
store-format limit. Every step prints one line; the run takes about three
storage trust periods.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from typing import TYPE_CHECKING, cast

import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading
import time

if TYPE_CHECKING:
    import network_contracts as nc

spec = importlib.util.spec_from_file_location(
    "network_test", Path(__file__).with_name("network-test.py")
)
assert spec is not None and spec.loader is not None, (
    "Python source loader is unavailable"
)
_module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(_module)
net = cast("nc.NetworkHarness", _module)

HOSTS = ("127.1.0.1", "127.2.0.1", "127.3.0.1")
PORT = 33445
TRUST_MINUTES = int(
    os.environ.get("DYAPPD_NETWORK_STORAGE_TRUST_MINUTES", "60")
)
STARTED = time.monotonic()


def log(message: "str") -> None:
    minutes = (time.monotonic() - STARTED) / 60
    print(f"[{minutes:6.1f} min] {message}", flush=True)


def environment(storage: str, index: int) -> dict[str, str]:
    host = HOSTS[index]
    seeds = [f"{other}:{PORT}" for other in HOSTS if other != host]
    return {
        **os.environ,
        "DYAPPD_LISTEN": f'["{host}:{PORT}"]',
        "DYAPPD_EXTERNAL": f'["{host}:{PORT}"]',
        "DYAPPD_STORAGE_DIR": f"{storage}/{'abc'[index]}",
        "DYAPPD_ROLES": '["store", "search"]',
        "DYAPPD_NETWORK_GUARD_DIALS": "false",
        "DYAPPD_SEEDS": "[" + ", ".join(f'"{s}"' for s in seeds) + "]",
    }


def start(
    bin_dir: str, env: Mapping[str, str], client: str, tcp: str
) -> subprocess.Popen[bytes]:
    process = net.start_node(bin_dir, env)
    for _ in range(60):
        if net.healthy(client, tcp):
            return process
        if process.poll() is not None:
            break
        time.sleep(1)
    raise RuntimeError(f"Node did not start: {tcp}")


def halt(process: subprocess.Popen[bytes]) -> None:
    process.terminate()
    process.wait()


def canary(client: str, entry: str) -> tuple[str, nc.Reply]:
    """Publishes a fresh profile through `entry`; its status and holders."""
    signed = net.call(client, "sign-profile")
    status = net.call(client, "publish", entry, signed["record"])["status"]
    return status, signed


def holders(client: str, addresses: Sequence[str], signed: nc.Reply) -> int:
    records = [
        net.call(client, "get", tcp, signed["peer_id"]).get("record")
        for tcp in addresses
    ]
    return records.count(signed["record"])


def expect(
    client: str, entry: str, addresses: Sequence[str], status: str
) -> nc.Reply:
    """A canary through `entry` gets `status`; an accepted one, two holders."""
    got, signed = canary(client, entry)
    if got != status:
        raise RuntimeError(f"Canary through {entry}: {got}, expected {status}")
    if status == "STATUS_OK":
        # A replica can land just after the acceptor answers.
        for _ in range(10):
            count = holders(client, addresses, signed)
            if count >= 2:
                break
            time.sleep(1)
        else:
            raise RuntimeError(f"Canary through {entry} on {count} node(s)")
        log(f"canary through {entry}: {got}, {count} holders")
    else:
        log(f"canary through {entry}: {got}, as expected")
    return signed


def wait_trust(reason: "str") -> None:
    if TRUST_MINUTES:
        log(f"waiting {TRUST_MINUTES} + 1 min: {reason}")
        time.sleep((TRUST_MINUTES + 1) * 60)


def refused(old_dir: str, env: Mapping[str, str]) -> str:
    """The old dyappd exits on a store a newer one upgraded; its error line."""
    result = subprocess.run(
        [f"{old_dir}/dyappd"],
        env=env,
        capture_output=True,
        text=True,
        timeout=60,
    )
    lines = [
        line
        for line in net.ANSI.sub(
            "", result.stdout + result.stderr
        ).splitlines()
        if "store format" in line
    ]
    if result.returncode == 0 or not lines:
        raise RuntimeError("The old dyappd started on an upgraded store")
    return lines[-1].strip()


def upgrade_nodes(
    old_dir: str,
    new_dir: str,
    storage: str,
    client: str,
    envs: Sequence[Mapping[str, str]],
    addresses: Sequence[str],
    processes: list[subprocess.Popen[bytes]],
    before: Sequence[nc.Reply],
) -> tuple[str, list[nc.Reply]]:
    backup = f"{storage}/c.old"
    for i, (env, tcp) in enumerate(zip(envs, addresses)):
        if i == 2:
            kept = [
                signed for signed in before if holders(client, [tcp], signed)
            ]
        halt(processes[i])
        if i == 2:
            shutil.copytree(env["DYAPPD_STORAGE_DIR"], backup)
        processes[i] = start(new_dir, env, client, tcp)
        log(f"upgraded {tcp}")
        for signed in before:
            if holders(client, addresses, signed) < 2:
                raise RuntimeError("A canary lost its second holder")
        if TRUST_MINUTES:
            expect(client, tcp, addresses, "STATUS_FULL")
        expect(client, addresses[i - 1], addresses, "STATUS_OK")
        if i < 2:
            wait_trust(f"storage trust of {tcp} before the next node")
    return backup, kept


def rehearse(old_dir: "str", new_dir: "str", storage: "str") -> None:
    client = f"{new_dir}/dyappd"
    envs = [environment(storage, i) for i in range(3)]
    addresses = [net.addresses(host, PORT)[0] for host in HOSTS]
    keygens = []
    for env in envs:
        keygens.append(
            threading.Thread(target=net.dyappd, args=(old_dir, env, "keygen"))
        )
        keygens[-1].start()
    for thread in keygens:
        thread.join()
    log("node keys generated")
    processes = [
        start(old_dir, env, client, tcp) for env, tcp in zip(envs, addresses)
    ]
    try:
        log("cold start: three old nodes healthy")
        if TRUST_MINUTES:
            expect(client, addresses[0], addresses, "STATUS_FULL")
        wait_trust("storage trust after the cold start")
        before = [
            expect(client, tcp, addresses, "STATUS_OK") for tcp in addresses
        ]
        backup, kept = upgrade_nodes(
            old_dir,
            new_dir,
            storage,
            client,
            envs,
            addresses,
            processes,
            before,
        )
        halt(processes[2])
        log(f"rollback refused: {refused(old_dir, envs[2])}")
        shutil.rmtree(envs[2]["DYAPPD_STORAGE_DIR"])
        shutil.move(backup, envs[2]["DYAPPD_STORAGE_DIR"])
        processes[2] = start(old_dir, envs[2], client, addresses[2])
        log(f"rolled back {addresses[2]} from the pre-update backup")
        if sum(holders(client, [addresses[2]], s) for s in kept) != len(kept):
            raise RuntimeError("The rolled-back node lost a canary")
        log(f"rolled-back node still holds its {len(kept)} canaries")
        expect(client, addresses[0], addresses, "STATUS_OK")
        log("rehearsal passed")
    except BaseException:
        net.stop(processes, True)
        raise
    net.stop(processes, False)


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("Usage: rehearsal.py OLD_BIN_DIR NEW_BIN_DIR")
    Path("/tmp/ai").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir="/tmp/ai") as storage:
        rehearse(sys.argv[1], sys.argv[2], storage)


if __name__ == "__main__":
    from check_wrapper import wrap

    wrap("rust-network")
    main()
