"""Launch a real bundled Tauri app, check rendered UI and injected failure."""
import json
import os
from pathlib import Path
import re
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


def run(arguments, timeout=60):
    # Output goes to files, not pipes: WebKit helpers inherit it, and
    # sandboxed ones (bwrap --new-session) escape killpg, so waiting for
    # pipe EOF could hang forever.
    with tempfile.TemporaryFile("w+") as out, \
            tempfile.TemporaryFile("w+") as err:
        process = subprocess.Popen(
            arguments, stdout=out, stderr=err,
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
    results = []
    cases = (("launch-render-close", []),
             ("pseudo-locale", ["--pseudo-locale"]),
             ("hidden-ready-rejected", ["--break-ui"]))
    for case, extra in cases:
        broken = extra == ["--break-ui"]
        arguments = [executable, "--ui-test", *extra]
        process = run(arguments)
        expected = "UI_SMOKE_FAIL" if broken else "UI_SMOKE_PASS"
        if process.returncode != int(broken) or expected not in process.stdout:
            raise RuntimeError(
                f"UI suite failed: {arguments}, exit={process.returncode}\n"
                f"{process.stdout}\n{process.stderr}")
        print(process.stdout, end="")
        print(process.stderr, end="", file=sys.stderr)
        results.append({"case": case, "exit": process.returncode,
                        "passed": True})
    report_root = os.environ.get("RUNNER_TEMP")
    if not report_root:
        report_root = f"/reports/{os.environ['HOSTNAME']}"
    reports = Path(report_root)
    reports.mkdir(parents=True, exist_ok=True)
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
