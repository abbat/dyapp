"""Concise check defaults with an explicit legacy machine-output mode."""
import importlib.util
import os
from pathlib import Path
import sys
import tempfile


def load_runner():
    """Load the shared runner without invoking its CLI."""
    path = Path(__file__).with_name("run-check.py")
    spec = importlib.util.spec_from_file_location("check_runner", path)
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    return runner


def wrap(name, data=False):
    """Dispatch only from a CLI guard; preserve raw/help/data calls and CWD."""
    if "--raw" in sys.argv[1:]:
        sys.argv.remove("--raw")
        return
    if data or any(value in sys.argv[1:] for value in ("--help", "-h")):
        return
    script = Path(sys.argv[0]).resolve()
    runner_path = Path(__file__).with_name("run-check.py")
    runner = load_runner()
    base = (Path(tempfile.gettempdir()) / "ai" if os.name == "nt"
            else Path("/tmp/ai"))
    directory = os.environ.get("DYAPP_CHECK_LOG_DIR",
                               str(base / "check-output"))
    previous = sys.argv
    sys.argv = [
        str(runner_path), "--log-dir", directory, name, "--",
        sys.executable, "-B", str(script), "--raw", *previous[1:]]
    try:
        status = runner.main()
    finally:
        sys.argv = previous
    raise SystemExit(status)
