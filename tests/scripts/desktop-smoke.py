"""Launch a real bundled Tauri app, check rendered UI and injected failure."""
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile


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


def smoke(executable):
    report_root = os.environ.get("RUNNER_TEMP")
    if not report_root:
        report_root = f"/reports/{os.environ['HOSTNAME']}"
    reports = Path(report_root)
    reports.mkdir(parents=True, exist_ok=True)
    results = []

    def case(name, extra, profile, broken=False, peer=True):
        arguments = [executable, "--ui-test", *extra]
        env = dict(os.environ, DYAPP_TEST_CONFIG_DIR=str(profile))
        process = run(arguments, env=env)
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
        (root / "profiles" / added / "identity.key").unlink()
        case("missing-key", [*file, "--ui-test-step=recovery"], profile,
             peer=False)
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

    for mode in ("system", "file"):
        with tempfile.TemporaryDirectory(dir=reports) as profile:
            flags = ["--ui-test-file"] if mode == "file" else []
            peer_id = case(f"{mode}-create", flags, profile)
            reopened = case(f"{mode}-restart",
                            [*flags, "--ui-test-existing"], profile)
            if reopened != peer_id:
                raise RuntimeError(
                    f"{mode} storage changed Peer ID after restart")
            (legacy if mode == "file" else mixed)(profile, peer_id)
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        case("russian-locale", ["--ui-test-file", "--ui-test-ru"], profile)
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        case("pseudo-locale", ["--ui-test-file", "--pseudo-locale"], profile)
    with tempfile.TemporaryDirectory(dir=reports) as profile:
        case("hidden-ready-rejected", ["--ui-test-file", "--break-ui"],
             profile, broken=True)
    (reports / "desktop-ui-results.json").write_text(
        json.dumps(results, indent=2))
    print(f"{len(results)} desktop UI cases passed")


if __name__ == "__main__":
    from check_wrapper import wrap
    wrap("desktop-smoke")
    try:
        smoke(sys.argv[1])
    except (RuntimeError, OSError) as error:
        if os.environ.get("DYAPP_XVFB_DIAGNOSTICS") == "1":
            raise RuntimeError(f"{error}\n{display_diagnostics()}") from None
        raise
