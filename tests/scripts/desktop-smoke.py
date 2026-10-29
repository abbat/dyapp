"""Launch a real bundled Tauri app, check rendered UI and injected failure."""
import contextlib
import json
import os
from pathlib import Path
import re
import shutil
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
        lines.append(f"X11 socket exists="
                     f"{Path(f'/tmp/.X11-unix/X{number}').exists()}")
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


def run(arguments, timeout=60, env=None):
    # Output goes to files, not pipes: WebKit helpers inherit it, and
    # sandboxed ones (bwrap --new-session) escape killpg, so waiting for
    # pipe EOF could hang forever.
    with tempfile.TemporaryFile("w+") as out, \
            tempfile.TemporaryFile("w+") as err:
        process = subprocess.Popen(
            arguments, stdout=out, stderr=err, env=env,
            start_new_session=os.name == "posix")
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
            f"{stdout}\n{stderr}")
    return subprocess.CompletedProcess(
        arguments, process.returncode, stdout, stderr)


def read_varint(data, offset):
    """Read a protobuf varint from a signed profile record."""
    value = 0
    for shift in range(0, 70, 7):
        if offset >= len(data):
            raise RuntimeError("truncated signed profile")
        byte = data[offset]
        offset += 1
        value |= (byte & 0x7f) << shift
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
        fields[tag >> 3] = data[offset:offset + size]
        offset += size
    if set(fields) != {1, 2, 3}:
        raise RuntimeError("incomplete signed profile")
    if len(fields[1]) != 32 or len(fields[3]) != 64:
        raise RuntimeError("incomplete signed profile")
    return fields[2]


def check_profile_readback(node, seed, peer_id, env, storage):
    """Node's signed profile payload must equal the published local draft."""
    slot = (storage / "active-profile").read_text().strip()
    database = storage / "profiles" / slot / "client.db"
    if not database.is_file():
        raise RuntimeError("published profile database missing locally")
    with sqlite3.connect(database) as db:
        draft = db.execute(
            "SELECT `value` FROM `settings` WHERE `key` = ?",
            ("profile_draft",)).fetchone()
    if draft is None:
        raise RuntimeError("published profile draft missing locally")
    readback = subprocess.run([node, "get", seed, peer_id], env=env,
                              capture_output=True, text=True,
                              check=True, timeout=60)
    result = json.loads(readback.stdout)
    if result.get("status") != "STATUS_OK" or not result.get("record"):
        raise RuntimeError(f"published profile readback failed: {result}")
    if signed_profile_payload(result["record"]) != draft[0]:
        raise RuntimeError("node signed readback differs from UI draft")


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
        db.execute("UPDATE `record` SET `record` = ? WHERE `key` = ?",
                   (forged, key))


def smoke(executable, flows=True):
    """flows=False skips the stateful identity and network flows, which a
    stateless fixture app cannot pass; only tests of this script use it."""
    report_root = os.environ.get("RUNNER_TEMP")
    if not report_root:
        report_root = f"/reports/{os.environ['HOSTNAME']}"
    reports = Path(report_root)
    reports.mkdir(parents=True, exist_ok=True)
    results = []

    def case(name, extra, profile, broken=False, peer=True, timeout=60):
        arguments = [executable, "--ui-test", *extra]
        if os.environ.get("DYAPP_SMOKE_RU") == "1":
            arguments.append("--ui-test-ru")
        env = dict(os.environ, DYAPP_TEST_CONFIG_DIR=str(profile))
        process = run(arguments, timeout=timeout, env=env)
        match = re.search(r"UI_SMOKE_PASS:([0-9a-f]{64})?", process.stdout)
        found = match is not None and (match[1] is not None or not peer)
        marker_found = ("UI_SMOKE_FAIL" in process.stdout
                        if broken else found)
        passed = process.returncode == int(broken) and marker_found
        if not passed:
            raise RuntimeError(
                f"UI suite failed: {arguments}, exit={process.returncode}\n"
                f"{process.stdout}\n{process.stderr}")
        print(process.stdout, end="")
        print(process.stderr, end="", file=sys.stderr)
        results.append({"case": name, "exit": process.returncode,
                        "passed": True})
        return match.group(1) if match else None

    def remove(name, profile, slot, first):
        """Remove the opened identity; only its slot folder may go."""
        slots = Path(profile) / "profiles"
        kept = {s.name for s in slots.iterdir()} - {slot}
        if case(name, ["--ui-test-step=remove"], profile) != first:
            raise RuntimeError("the remaining identity did not open")
        if {s.name for s in slots.iterdir()} != kept:
            raise RuntimeError("removal touched the wrong slots")
        reopened = case(f"{name}-restart",
                        ["--ui-test-step=settings-restart"], profile)
        if reopened != first:
            raise RuntimeError("removed identity reopened after restart")

    def window(profile):
        """Ctrl+Q saves the moved window, a relaunch puts it back, and one
        saved on a display that went away comes back centred."""
        state = Path(profile) / "window.json"
        file = ["--ui-test-file"]
        case("window-save", [*file, "--ui-test-step=window:save"], profile,
             peer=False)
        saved = json.loads(state.read_text())
        if saved != {"normal": {"x": 120, "y": 80, "width": 900,
                                "height": 640}, "maximized": False}:
            raise RuntimeError(f"window state not saved: {saved}")
        case("window-restore", [*file, "--ui-test-step=window:restore"],
             profile, peer=False)
        saved["normal"]["x"] = -5000
        state.write_text(json.dumps(saved))
        case("window-off-screen", [*file, "--ui-test-step=window:centre"],
             profile, peer=False)

    def mixed(profile, first):
        """Add a file identity to a system one, then crash, bad pointer."""
        root = Path(profile)
        file = ["--ui-test-file"]
        second = case("mixed-add", [*file, "--ui-test-step=add"], profile)
        if second == first:
            raise RuntimeError("added identity reused the Peer ID")
        added = (root / "active-profile").read_text().strip()
        # A crash after reservation leaves a pending slot without a key.
        pending = root / "profiles" / "00000000-0000-4000-8000-000000000000"
        pending.mkdir()
        (pending / "storage-mode").write_bytes(b"file\n")
        (pending / "state").write_bytes(b"pending\n")
        if case("crash-restart", [*file, "--ui-test-existing"],
                profile) != second:
            raise RuntimeError("pending slot replaced the active identity")
        (root / "active-profile").write_bytes(b"../escape\n")
        if case("corrupt-pointer-pick", [*file, "--ui-test-step=pick"],
                profile) != second:
            raise RuntimeError("picker did not reopen the file identity")
        if case("pick-restart", [*file, "--ui-test-existing"],
                profile) != second:
            raise RuntimeError("picked identity was not reopened")
        remove("remove-ready", profile, added, first)
        case("readd", [*file, "--ui-test-step=add"], profile)
        added = (root / "active-profile").read_text().strip()
        key = root / "profiles" / added / "identity.key"
        key.write_bytes(b"not a key\n")
        case("broken-key", [*file, "--ui-test-step=recovery:unavailable"],
             profile, peer=False)
        key.unlink()
        case("missing-key", [*file, "--ui-test-step=recovery:missing"],
             profile, peer=False)
        remove("remove-missing", profile, added, first)

    def legacy(profile, peer_id):
        """Turn the file slot into a pre-slot root key and reopen it."""
        root = Path(profile)
        slot = (root / "active-profile").read_text().strip()
        (root / "profiles" / slot / "identity.key").rename(
            root / "identity.key")
        (root / "identity-storage").write_bytes(b"file\n")
        shutil.rmtree(root / "profiles")
        (root / "active-profile").unlink()
        if case("legacy-upgrade", ["--ui-test-file", "--ui-test-existing"],
                profile) != peer_id:
            raise RuntimeError("legacy key changed Peer ID")

    def network(profile):
        """Failed checks keep the draft but never activate it."""
        file = ["--ui-test-file"]
        peer_id = case("network-create", file, profile)
        case("network-check", [*file, "--ui-test-step=network"], profile)
        slot = (Path(profile) / "active-profile").read_text().strip()
        folder = Path(profile) / "profiles" / slot
        if not (folder / "network-draft").is_file():
            raise RuntimeError("the checked draft was not saved")
        if (folder / "network").exists():
            raise RuntimeError("a failed check activated the draft")
        if case("network-restart", [*file, "--ui-test-step=network-restart"],
                profile) != peer_id:
            raise RuntimeError("the network step replaced the identity")

    def profile_draft(profile):
        """A nonempty public draft and its preview survive an app restart."""
        file = ["--ui-test-file"]
        peer_id = case("profile-create", file, profile)
        saved = case("profile-save", [*file, "--ui-test-step=profile-save"],
                     profile)
        if saved != peer_id:
            raise RuntimeError("saving a profile replaced the identity")
        reopened = case("profile-restart",
                        [*file, "--ui-test-step=profile-restart"], profile)
        if reopened != peer_id:
            raise RuntimeError("reopening a profile replaced the identity")

    def connect(profile):
        """A loopback dyappd connects the form; Continue activates it."""
        node, storage = Path(executable).with_name("dyappd"), Path(profile)
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            seed = "127.0.0.1:%d" % probe.getsockname()[1]
        env = dict(os.environ, DYAPPD_LISTEN=f'["{seed}"]',
                   DYAPPD_EXTERNAL=f'["{seed}"]',
                   DYAPPD_STORAGE_DIR=str(storage / "node"),
                   DYAPPD_NETWORK_ID_POW_BITS="8")
        subprocess.run([node, "keygen"], env=env, check=True,
                       stdout=subprocess.DEVNULL, timeout=600)
        log = storage / "node.log"
        with log.open("w") as out:
            server = subprocess.Popen([node], env=env, stdout=out,
                                      stderr=subprocess.STDOUT)
        try:
            host, port = seed.split(":")
            for _ in range(60):
                try:
                    socket.create_connection((host, int(port)), 1).close()
                    break
                except OSError:
                    time.sleep(1)
            else:
                raise RuntimeError(f"dyappd did not start:\n"
                                   f"{log.read_text()[-4096:]}")
            file = ["--ui-test-file"]
            peer_id = case("connect-create", file, profile)
            failed = case("profile-publish-no-network",
                          [*file, "--ui-test-step=profile-publish-fail"],
                          profile, timeout=180)
            if failed != peer_id:
                raise RuntimeError("failed publish replaced the identity")
            case("search-no-network",
                 [*file, "--ui-test-step=search-no-network"], profile)
            connected = case("network-connect",
                             [*file, f"--ui-test-step=connect:{seed}"],
                             profile)
            if connected != peer_id:
                raise RuntimeError("connecting replaced the identity")
            slot = (storage / "active-profile").read_text().strip()
            if case("node", [*file, "--ui-test-step=node"], profile,
                    timeout=180) != peer_id:
                raise RuntimeError("the node step replaced the identity")
            folder = storage / "profiles" / slot
            saved = json.loads((folder / "node.json").read_text())
            if saved["enabled"] or not (folder / "node").is_dir():
                raise RuntimeError(f"the node did not run, then stop: {saved}")
            active = storage / "profiles" / slot / "network"
            active_before = active.read_bytes()
            settings_step = "--ui-test-step=network-settings-failure"
            settings_checked = case("network-settings-failure",
                                    [*file, settings_step], profile)
            if settings_checked != peer_id:
                raise RuntimeError("failed Settings check changed identity")
            if active.read_bytes() != active_before:
                raise RuntimeError("Settings check changed active network")
            published = case("profile-publish-retry",
                             [*file, "--ui-test-step=profile-publish-retry"],
                             profile, timeout=180)
            if published != peer_id:
                raise RuntimeError("publishing replaced the identity")
            check_profile_readback(node, seed, peer_id, env, storage)
            case("search", [*file, "--ui-test-step=search"], profile,
                 timeout=180)
            forge_search_signature(storage / "node")
            case("search-forged", [*file, "--ui-test-step=search-forged"],
                 profile, timeout=180)
            # A first run that connects on the wizard's network step.
            with tempfile.TemporaryDirectory(dir=reports) as fresh:
                case("wizard-connect",
                     [*file, f"--ui-test-step=wizard-connect:{seed}"], fresh)
                fresh = Path(fresh)
                picked = (fresh / "active-profile").read_text().strip()
                if not (fresh / "profiles" / picked / "network").is_file():
                    raise RuntimeError("the wizard did not activate a network")
        except RuntimeError as error:
            raise RuntimeError(f"{error}\ndyappd log:\n"
                               f"{log.read_text()[-8192:]}") from error
        finally:
            server.terminate()
            server.wait(timeout=30)
        slot = (storage / "active-profile").read_text().strip()
        if not (storage / "profiles" / slot / "network").is_file():
            raise RuntimeError("Continue did not activate the checked draft")

    for mode in ("system", "file"):
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            flags = ["--ui-test-file"] if mode == "file" else []
            peer_id = case(f"{mode}-create", flags, profile)
            reopened = case(f"{mode}-restart",
                            [*flags, "--ui-test-existing"], profile)
            if reopened != peer_id:
                raise RuntimeError(
                    f"{mode} storage changed Peer ID after restart")
            if flows:
                (legacy if mode == "file" else mixed)(profile, peer_id)
    if flows:
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            network(profile)
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            profile_draft(profile)
    # linux-test.sh builds dyappd beside the app; Windows and the installed
    # client packages have none.
    if flows and Path(executable).with_name("dyappd").exists():
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            connect(profile)
    language_step = "--ui-test-step=settings-language"
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        case("settings-english", ["--ui-test-file", language_step], profile)

    @contextlib.contextmanager
    def fake_tray(mode, profile):
        """A tray host on the session bus for one case (fake-tray.py)."""
        (Path(profile) / "tray-click").unlink(missing_ok=True)
        host = subprocess.Popen(  # pylint: disable=consider-using-with
            ["/usr/bin/python3", str(Path(__file__).with_name("fake-tray.py")),
             mode, str(profile)], stdout=subprocess.PIPE, text=True)
        try:
            if host.stdout.readline().strip() != "ready":
                raise RuntimeError(f"fake tray host did not start: {mode}")
            yield
        finally:
            host.terminate()
            host.wait(timeout=10)

    def tray(profile):
        """Without a host the tray settings are off; with one, Quit exits,
        close-to-tray hides the window until Open, the setting survives a
        relaunch, and a host that goes away brings the window back."""
        file = ["--ui-test-file"]
        case("tray-none", [*file, "--ui-test-step=tray:none"], profile,
             peer=False)
        with fake_tray("quit", profile):
            case("tray-quit", [*file, "--ui-test-step=tray:quit"], profile,
                 peer=False)
        with fake_tray("open", profile):
            case("tray-close", [*file, "--ui-test-step=tray:close"], profile,
                 peer=False)
        saved = json.loads((Path(profile) / "tray.json").read_text())
        if saved != {"show": True, "closeToTray": True}:
            raise RuntimeError(f"tray settings not saved: {saved}")
        with fake_tray("open", profile):
            case("tray-relaunch", [*file, "--ui-test-step=tray:persisted"],
                 profile, peer=False)
        with fake_tray("vanish", profile):
            case("tray-vanish", [*file, "--ui-test-step=tray:vanish"],
                 profile, peer=False)

    with tempfile.TemporaryDirectory(dir=reports) as profile:
        russian = ["--ui-test-file", "--ui-test-ru", language_step]
        case("russian-locale", russian, profile)
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        pseudo = ["--ui-test-file", "--pseudo-locale", language_step]
        case("pseudo-locale", pseudo, profile)
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        case("hidden-ready-rejected", ["--ui-test-file", "--break-ui"],
             profile, broken=True)
    if flows:
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            window(profile)
    if flows and os.name == "posix":
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            tray(profile)
    (reports / "desktop-ui-results.json").write_text(
        json.dumps(results, indent=2))
    print(f"{len(results)} desktop UI cases passed")


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("desktop-smoke")
    try:
        smoke(sys.argv[1], os.environ.get("DYAPP_SMOKE_FIXTURE") != "1")
    except (RuntimeError, OSError) as error:
        if os.environ.get("DYAPP_XVFB_DIAGNOSTICS") == "1":
            raise RuntimeError(f"{error}\n{display_diagnostics()}") from None
        raise
