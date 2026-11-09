"""Concise check defaults with an explicit legacy machine-output mode."""

from collections.abc import Sequence
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
from typing import NotRequired, Protocol, TypedDict, cast


class DiagnosticPolicy(TypedDict):
    format: str
    pattern: NotRequired[str]
    details: NotRequired[bool]
    nested: NotRequired[bool]
    gradle: NotRequired[bool]


class CheckPolicy(DiagnosticPolicy):
    fix: str


class CheckRunner(Protocol):
    def main(self) -> int:
        """Run the CLI and return its exit status."""
        ...

    def diagnostics(
        self,
        text: str,
        policy: DiagnosticPolicy,
    ) -> list[str]:
        """Select failing diagnostics according to the mapped format."""
        ...

    def failure(
        self,
        name: str,
        fix: str,
        status: int,
        selected: Sequence[str],
        log: str | os.PathLike[str] | None = None,
    ) -> int:
        """Print a failure frame and preserve the child exit status."""
        ...


def load_runner() -> CheckRunner:
    """Load the shared runner without invoking its CLI."""
    path = Path(__file__).with_name("run-check.py")
    spec = importlib.util.spec_from_file_location("check_runner", path)
    if spec is None or spec.loader is None:
        raise ImportError(f"Cannot load check runner: {path}")
    runner = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runner)
    # This source module implements the three declared runner operations.
    return cast(CheckRunner, runner)


def wrap(name: str, data: bool = False) -> None:
    """Dispatch only from a CLI guard; preserve raw/help/data calls and CWD."""
    if "--raw" in sys.argv[1:]:
        sys.argv.remove("--raw")
        return
    if data or any(value in sys.argv[1:] for value in ("--help", "-h")):
        return
    script = Path(sys.argv[0]).resolve()
    runner_path = Path(__file__).with_name("run-check.py")
    runner = load_runner()
    base = (
        Path(tempfile.gettempdir()) / "ai"
        if os.name == "nt"
        else Path("/tmp/ai")
    )
    directory = os.environ.get(
        "DYAPP_CHECK_LOG_DIR", str(base / "check-output")
    )
    previous = sys.argv
    sys.argv = [
        str(runner_path),
        "--log-dir",
        directory,
        name,
        "--",
        sys.executable,
        "-B",
        str(script),
        "--raw",
        *previous[1:],
    ]
    try:
        status = runner.main()
    finally:
        sys.argv = previous
    raise SystemExit(status)
