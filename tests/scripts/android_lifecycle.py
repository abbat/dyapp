"""Observe emulator reboot and persisted job state before launching the app."""

import argparse
import json
import math
import subprocess
import sys
import time
import uuid
from collections.abc import Callable
from pathlib import Path

Runner = Callable[[list[str], float], subprocess.CompletedProcess[str]]
BOOT_ID = ["shell", "cat", "/proc/sys/kernel/random/boot_id"]


def command(
    args: list[str], timeout: float
) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        args, check=False, capture_output=True, text=True, timeout=timeout
    )


class AdbLifecycle:
    def __init__(
        self,
        serial: str = "emulator-5554",
        run: Runner = command,
        now: Callable[[], float] = time.monotonic,
    ) -> None:
        self.prefix = ["adb", "-s", serial]
        self.run = run
        self.now = now

    def _remaining(self, deadline: float, label: str) -> float:
        remaining = deadline - self.now()
        if remaining <= 0:
            raise RuntimeError(f"Readiness deadline expired: {label}")
        return remaining

    def _checked(self, args: list[str], timeout: float) -> str:
        deadline = self.now() + timeout
        reply = self.run(self.prefix + args, timeout)
        self._remaining(deadline, " ".join(args))
        if reply.returncode:
            raise RuntimeError(
                f"ADB {' '.join(args)} failed: {reply.stderr or reply.stdout}"
            )
        return reply.stdout.strip()

    def _wait(
        self,
        args: list[str],
        accept: Callable[[str], bool],
        label: str,
        timeout: float,
    ) -> str:
        deadline = self.now() + timeout
        while True:
            connected = self.run(
                self.prefix + ["wait-for-device"],
                self._remaining(deadline, label),
            )
            self._remaining(deadline, label)
            if connected.returncode:
                raise RuntimeError(f"ADB transport failed: {connected.stderr}")
            reply = self.run(
                self.prefix + args, self._remaining(deadline, label)
            )
            output = reply.stdout.strip()
            ready = reply.returncode == 0 and accept(output)
            self._remaining(deadline, label)
            if ready:
                return output

    def reboot(
        self, package: str, job_id: int, timeout: float = 600
    ) -> dict[str, str]:
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError(
                "A positive finite readiness deadline is required"
            )
        if not package.isascii() or not all(
            part.isidentifier() for part in package.split(".")
        ):
            raise ValueError("Invalid Android package name")
        if job_id < 0:
            raise ValueError("Invalid job ID")
        before = str(uuid.UUID(self._checked(BOOT_ID, timeout)))
        before_job = self._job_state(package, job_id, timeout)
        self._checked(["reboot"], timeout)
        after = self._wait(
            BOOT_ID,
            lambda value: changed_boot(value, before),
            "changed boot identity",
            timeout,
        )
        self._wait(
            ["shell", "getprop", "sys.boot_completed"],
            lambda value: value == "1",
            "boot completion",
            timeout,
        )
        self._wait(
            ["shell", "cmd", "package", "path", "android"],
            lambda value: value.startswith("package:"),
            "package manager",
            timeout,
        )
        state = self._job_state(package, job_id, timeout)
        return {
            "before": before,
            "after": after,
            "before_job": before_job,
            "job": state,
        }

    def _job_state(self, package: str, job_id: int, timeout: float) -> str:
        state = self._checked(
            [
                "shell",
                "cmd",
                "jobscheduler",
                "get-job-state",
                "--user",
                "current",
                package,
                str(job_id),
            ],
            timeout,
        )
        if not state:
            raise RuntimeError("Persisted job probe returned no state")
        return state


def changed_boot(value: str, before: str) -> bool:
    try:
        return str(uuid.UUID(value)) != before
    except ValueError:
        return False


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", default="emulator-5554")
    parser.add_argument("--package", default="com.dyapp")
    parser.add_argument("--job-id", type=int, default=1)
    parser.add_argument("--timeout", type=float, default=600)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = AdbLifecycle(args.serial).reboot(
            args.package, args.job_id, args.timeout
        )
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report) + "\n", encoding="utf-8")
    except (
        OSError,
        ValueError,
        RuntimeError,
        subprocess.TimeoutExpired,
    ) as error:
        print(f"ERROR Android reboot: {error}", file=sys.stderr)
        raise SystemExit(1) from error


if __name__ == "__main__":
    main()
