"""Launch a real bundled Tauri app, check rendered UI and injected failure."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile


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
        print(stdout, end="")
        print(stderr, end="", file=sys.stderr)
        raise RuntimeError(f"UI suite timed out after {timeout}s: {arguments}")
    return subprocess.CompletedProcess(
        arguments, process.returncode, stdout, stderr)


def smoke(executable):
    results = []
    for broken in (False, True):
        arguments = [executable, "--ui-test"]
        if broken:
            arguments.append("--break-ui")
        process = run(arguments)
        print(process.stdout, end="")
        print(process.stderr, end="", file=sys.stderr)
        expected = "UI_SMOKE_FAIL" if broken else "UI_SMOKE_PASS"
        if process.returncode != int(broken) or expected not in process.stdout:
            raise RuntimeError(
                f"UI suite failed: {arguments}, exit={process.returncode}")
        case = "hidden-ready-rejected" if broken else "launch-render-close"
        results.append({"case": case, "exit": process.returncode,
                        "passed": True})
    report_root = os.environ.get("RUNNER_TEMP")
    if not report_root:
        report_root = f"/reports/{os.environ['HOSTNAME']}"
    reports = Path(report_root)
    reports.mkdir(parents=True, exist_ok=True)
    (reports / "desktop-ui-results.json").write_text(
        json.dumps(results, indent=2))
    print("2 desktop UI cases passed")


if __name__ == "__main__":
    smoke(sys.argv[1])
