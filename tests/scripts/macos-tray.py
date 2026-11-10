"""Mac smoke driver: native Dock reopen and registered tray action callbacks.

No Accessibility permission or coordinate clicks are required. A kqueue watches
atomic, process-bound readiness markers; native tray creation precedes actions.
"""

from collections.abc import Callable
import contextlib
import os
from pathlib import Path
import select
import subprocess
import sys
import time


def read_marker(path: Path) -> str:
    try:
        return path.read_text().strip()
    except FileNotFoundError:
        return ""


def main() -> None:
    if sys.platform != "darwin":
        raise RuntimeError("macOS driver requires macOS")
    executable, mode, profile = sys.argv[1:]
    root = Path(profile)
    app = next(
        path
        for path in Path(executable).resolve().parents
        if path.suffix == ".app"
    )
    previous = read_marker(root / "tray-ready")
    descriptor = os.open(root, os.O_RDONLY)
    try:
        with contextlib.closing(select.kqueue()) as changes:
            event = select.kevent(
                descriptor,
                filter=select.KQ_FILTER_VNODE,
                flags=select.KQ_EV_ADD | select.KQ_EV_CLEAR,
                fflags=select.KQ_NOTE_WRITE | select.KQ_NOTE_RENAME,
            )
            changes.control([event], 0, 0)
            print("ready", flush=True)

            def wait(check: Callable[[], str], phase: str) -> str:
                deadline = time.monotonic() + 30
                while not (value := check()):
                    remaining = deadline - time.monotonic()
                    if remaining <= 0:
                        raise TimeoutError(
                            f"macOS readiness timed out: {phase}"
                        )
                    changes.control(None, 1, remaining)
                return value

            process = wait(
                lambda: (
                    value
                    if (value := read_marker(root / "tray-ready")) != previous
                    else ""
                ),
                "native tray",
            )
            os.kill(int(process), 0)
            if mode != "quit":
                wait(
                    lambda: (
                        process
                        if read_marker(root / "hidden") == process
                        else ""
                    ),
                    "hidden",
                )
            if mode == "reopen":
                subprocess.run(
                    [
                        "/usr/bin/open",
                        "--env",
                        f"DYAPP_TEST_CONFIG_DIR={profile}",
                        "-a",
                        str(app),
                        "--args",
                        "--ui-test",
                        "--ui-test-file",
                        "--ui-test-step=single",
                    ],
                    check=True,
                    timeout=15,
                )
                marker = "dock-reopened"
            elif mode in {"open", "quit"}:
                environment = dict(os.environ, DYAPP_TEST_CONFIG_DIR=profile)
                subprocess.run(
                    [
                        executable,
                        "--ui-test",
                        "--ui-test-file",
                        f"--ui-test-tray-action={mode}",
                    ],
                    env=environment,
                    check=True,
                    timeout=15,
                    stdout=subprocess.DEVNULL,
                )
                marker = f"tray-action-{mode}"
            else:
                raise ValueError(f"unsupported macOS tray mode: {mode}")
            wait(
                lambda: (
                    process if read_marker(root / marker) == process else ""
                ),
                marker,
            )
    finally:
        os.close(descriptor)


if __name__ == "__main__":
    main()
