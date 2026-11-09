"""Native process, profile and media probes for desktop smoke."""

import contextlib
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


def display_diagnostics():
    """Bounded, allowlisted state while xvfb-run still owns the server."""
    display = os.environ.get("DISPLAY", "")
    local = re.fullmatch(r":([0-9]{1,6})(?:\.[0-9]{1,3})?", display)
    number = local[1] if local else ""
    lines = [
        "Xvfb/display diagnostics (before xvfb-run cleanup):",
        f"DISPLAY={display if local and len(display) < 32 else 'unavailable'}",
        f"XAUTHORITY configured={bool(os.environ.get('XAUTHORITY'))}",
        "DBUS_SESSION_BUS_ADDRESS configured="
        f"{bool(os.environ.get('DBUS_SESSION_BUS_ADDRESS'))}",
    ]
    if local:
        lines.append(
            f"X11 socket exists={Path(f'/tmp/.X11-unix/X{number}').exists()}"
        )
    servers = 0
    for process in Path("/proc").glob("[0-9]*"):
        try:
            if (process / "comm").read_text().strip() != "Xvfb":
                continue
            fields = (process / "status").read_text().splitlines()
        except OSError:
            continue
        state = next((s for s in fields if s.startswith("State:")), "State: ?")
        lines.append(f"Xvfb pid={process.name} {state}")
        servers += 1
        if servers == 8:
            lines.append("Xvfb process listing capped at 8")
            break
    if not servers:
        lines.append("No live Xvfb process found in /proc")
    return "\n".join(lines)[:4096]


def unlock_keyring():
    """A gnome-keyring that dies mid-suite comes back locked through D-Bus
    activation, and --unlock cannot unlock a running daemon: replace it with
    an unlocked one. Only a locked one: a concurrent case may be using it."""
    if os.environ.get("DYAPP_TEST_SECRET_SERVICE") != "1":
        return
    locked = subprocess.run(
        [
            "busctl",
            "--user",
            "get-property",
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets/aliases/default",
            "org.freedesktop.Secret.Collection",
            "Locked",
        ],
        capture_output=True,
        text=True,
        timeout=30,
        check=False,
    )
    if locked.stdout.strip() != "b true":
        return
    print(
        "Secret Service collection locked; replacing gnome-keyring",
        file=sys.stderr,
    )
    # The daemon it starts keeps any pipe open: no captured output.
    subprocess.run(
        [
            "gnome-keyring-daemon",
            "--replace",
            "--unlock",
            "--components=secrets",
        ],
        input=b"dyapp-ci-test-keyring",
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        timeout=30,
        check=False,
    )


def run(arguments, timeout=60, env=None):
    # Output goes to files, not pipes: WebKit helpers inherit it, and
    # sandboxed ones (bwrap --new-session) escape killpg, so waiting for
    # pipe EOF could hang forever.
    with (
        tempfile.TemporaryFile("w+") as out,
        tempfile.TemporaryFile("w+") as err,
    ):
        process = subprocess.Popen(
            arguments,
            stdout=out,
            stderr=err,
            env=env,
            start_new_session=os.name == "posix",
        )
        try:
            process.wait(timeout=timeout)
            timed_out = False
        except subprocess.TimeoutExpired:
            if os.name == "posix":
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            process.wait()
            timed_out = True
        out.seek(0)
        err.seek(0)
        stdout, stderr = out.read(), err.read()
    if timed_out:
        raise RuntimeError(
            f"UI suite timed out after {timeout}s: {arguments}\n"
            f"{stdout}\n{stderr}"
        )
    return subprocess.CompletedProcess(
        arguments, process.returncode, stdout, stderr
    )


def read_varint(data, offset):
    """Read a protobuf varint from a signed profile record."""
    value = 0
    for shift in range(0, 70, 7):
        if offset >= len(data):
            raise RuntimeError("truncated signed profile")
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7F) << shift
        if byte < 0x80:
            return value, offset
    raise RuntimeError("invalid signed profile varint")


def signed_profile_payload(record_hex):
    """Extract the signed payload from identity.proto's bytes fields."""
    data = bytes.fromhex(record_hex)
    fields = {}
    offset = 0
    while offset < len(data):
        tag, offset = read_varint(data, offset)
        size, offset = read_varint(data, offset)
        if tag not in (10, 18, 26) or offset + size > len(data):
            raise RuntimeError("invalid signed profile fields")
        fields[tag >> 3] = data[offset : offset + size]
        offset += size
    if set(fields) != {1, 2, 3}:
        raise RuntimeError("incomplete signed profile")
    if len(fields[1]) != 32 or len(fields[3]) != 64:
        raise RuntimeError("incomplete signed profile")
    return fields[2]


def check_profile_readback(node, seed, peer_id, env, storage, hidden=False):
    """Node's signed profile payload must equal the published local draft.

    Hidden, the node retains the ordinary record and the local draft advances.
    """
    slot = (storage / "active-profile").read_text().strip()
    database = storage / "profiles" / slot / "client.db"
    if not database.is_file():
        raise RuntimeError("published profile database missing locally")
    with sqlite3.connect(database) as db:
        draft = db.execute(
            "SELECT `value` FROM `settings` WHERE `key` = ?",
            ("profile_draft",),
        ).fetchone()
        flag = db.execute(
            "SELECT `value` FROM `settings` WHERE `key` = ?",
            ("profile_hidden",),
        ).fetchone()
    if (flag is not None and flag[0] == b"\x01") != hidden:
        raise RuntimeError(f"profile hidden flag is {flag}, expected {hidden}")
    if draft is None:
        raise RuntimeError("published profile draft missing locally")
    readback = subprocess.run(
        [node, "get", seed, peer_id],
        env=env,
        capture_output=True,
        text=True,
        check=True,
        timeout=60,
    )
    result = json.loads(readback.stdout)
    if result.get("status") != "STATUS_OK" or not result.get("record"):
        raise RuntimeError(f"published profile readback failed: {result}")
    payload = signed_profile_payload(result["record"])
    if (payload == draft[0]) == hidden:
        raise RuntimeError(
            "node readback has wrong hidden or published version"
        )
    return payload


def forge_search_signature(node_storage):
    """Flips the last byte, the signature, of the one record in the node's
    search index (profiles-idx.db or a later generation of it)."""
    indexes = sorted(node_storage.glob("profiles-idx*.db"))
    if len(indexes) != 1:
        raise RuntimeError(f"expected one search index, found {indexes}")
    with sqlite3.connect(indexes[0], timeout=30) as db:
        rows = db.execute("SELECT `key`, `record` FROM `record`").fetchall()
        if len(rows) != 1:
            raise RuntimeError(f"expected one indexed record, got {len(rows)}")
        key, record = rows[0]
        forged = record[:-1] + bytes([record[-1] ^ 1])
        db.execute(
            "UPDATE `record` SET `record` = ? WHERE `key` = ?", (forged, key)
        )


def expire_sticker_media(node_storage, owner):
    """Advance only this isolated sender's attachment deadlines, not clocks.

    Quarantine generated blob files to model completed expiry without waiting
    for node maintenance. Signed packets and MLS history stay intact; the
    receiver has no saved pack or retained image yet. Nothing is deleted.
    """
    database = node_storage / "media.db"
    if not database.is_file():
        raise RuntimeError("sticker media database is missing")
    with contextlib.closing(sqlite3.connect(database, timeout=30)) as db, db:
        blobs = db.execute(
            "SELECT DISTINCT `blobs`.`hash` FROM `blobs` "
            "JOIN `attachments` ON `blobs`.`owner` = `attachments`.`slot` "
            "WHERE `attachments`.`owner` = ?",
            (owner,),
        ).fetchall()
        changed = db.execute(
            "UPDATE `attachments` SET `expires` = ? WHERE `owner` = ?",
            (0, owner),
        ).rowcount
    if changed < 1 or not blobs:
        raise RuntimeError("no sticker attachment deadline was advanced")
    quarantine = node_storage / "expired-sticker-media"
    quarantine.mkdir()
    moved = 0
    for (digest,) in blobs:
        if len(digest) != 32:
            raise RuntimeError("invalid isolated media hash")
        name = digest.hex()
        source = node_storage / "data" / name[:2] / name[2:4] / name
        if source.is_file():
            source.replace(quarantine / name)
            moved += 1
    if not moved:
        raise RuntimeError("no individual sticker blob was quarantined")


def video_smoke(case, reports):
    """Linux-only raw Tauri channel and Canvas smoke.

    Runs separately from account flows.
    """
    for size in ("360p", "720p"):
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            case(
                f"video-{size}",
                [f"--ui-test-step=video:{size}"],
                profile,
                peer=False,
            )


def start_smoke_node(node, env, seed, log):
    with log.open("a") as out:
        started = subprocess.Popen(
            [node], env=env, stdout=out, stderr=subprocess.STDOUT
        )
    host, port = seed.split(":")
    for _ in range(60):
        try:
            socket.create_connection((host, int(port)), 1).close()
            return started
        except OSError:
            time.sleep(1)
    started.terminate()
    raise RuntimeError(f"dyappd did not start:\n{log.read_text()[-4096:]}")
