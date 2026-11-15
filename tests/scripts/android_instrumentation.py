"""Stream am instrument results with completion-driven failure deadlines."""

import argparse
from collections.abc import Callable
import math
import os
from pathlib import Path
import selectors
import subprocess
import time
from typing import BinaryIO


class Progress:
    def __init__(
        self, timeout: float, now: Callable[[], float] = time.monotonic
    ) -> None:
        if not math.isfinite(timeout) or timeout <= 0:
            raise ValueError(
                "Instrumentation deadline must be finite and positive"
            )
        self.timeout = timeout
        self.now = now
        self.deadline = now() + timeout
        self.phase = "runner startup"
        self.fields: dict[str, str] = {}
        self.active: tuple[str, str, str] | None = None
        self.completed = 0
        self.finished = False
        self.buffer = b""
        self.discard = False

    def remaining(self) -> float:
        remaining = self.deadline - self.now()
        if remaining <= 0:
            raise TimeoutError(
                f"Android instrumentation stalled: {self.phase}"
            )
        return remaining

    def feed(self, chunk: bytes) -> None:
        self.remaining()
        for index, part in enumerate(chunk.split(b"\n")):
            if index:
                if not self.discard:
                    self._line(self.buffer.rstrip(b"\r"))
                self.buffer = b""
                self.discard = False
            if not self.discard:
                # Bound buffering even when a test prints one huge line.
                if len(self.buffer) + len(part) > 4096:
                    self.buffer = b""
                    self.discard = True
                else:
                    self.buffer += part

    def _line(self, line: bytes) -> None:
        if self.finished:
            return
        text = line.decode("utf-8", errors="replace")
        if text.startswith("INSTRUMENTATION_STATUS: "):
            key, separator, value = text.removeprefix(
                "INSTRUMENTATION_STATUS: "
            ).partition("=")
            if separator and key in ("id", "current", "class", "test"):
                self.fields[key] = value
        elif text.startswith("INSTRUMENTATION_STATUS_CODE: "):
            fields, self.fields = self.fields, {}
            current = fields.get("current", "")
            if (
                fields.get("id") != "AndroidJUnitRunner"
                or not current.isascii()
                or not current.isdecimal()
                or len(current) > 10
                or int(current) != self.completed + 1
                or not fields.get("class")
                or not fields.get("test")
            ):
                return
            test = (current, fields["class"], fields["test"])
            code = text.removeprefix("INSTRUMENTATION_STATUS_CODE: ")
            if code == "1" and self.active is None:
                self.active = test
                self.phase = f"{test[1]}#{test[2]}"
                if not self.completed:
                    self.deadline = self.now() + self.timeout
            elif code in ("0", "-1", "-2", "-3", "-4") and test == self.active:
                self.completed += 1
                self.active = None
                self.phase = (
                    f"next test or final result after {test[1]}#{test[2]}"
                )
                self.deadline = self.now() + self.timeout
        elif text.startswith("INSTRUMENTATION_CODE: "):
            self.finished = True
            self.phase = "ADB exit after final result"
            self.deadline = self.now() + self.timeout


def capture(
    process: subprocess.Popen[bytes], output: BinaryIO, progress: Progress
) -> None:
    assert process.stdout is not None
    with selectors.DefaultSelector() as ready:
        ready.register(process.stdout, selectors.EVENT_READ)
        while True:
            if not ready.select(progress.remaining()):
                raise TimeoutError(
                    f"Android instrumentation stalled: {progress.phase}"
                )
            progress.remaining()
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                return
            output.write(chunk)
            output.flush()
            progress.feed(chunk)


def run(command: list[str], output: BinaryIO, timeout: float) -> int:
    progress = Progress(timeout)
    with subprocess.Popen(
        command, stdout=subprocess.PIPE, stderr=subprocess.STDOUT
    ) as process:
        try:
            capture(process, output, progress)
            try:
                status = process.wait(timeout=progress.remaining())
                if status == 0 and not progress.finished:
                    raise RuntimeError(
                        "Android instrumentation ended without a final result"
                    )
                return status
            except subprocess.TimeoutExpired as error:
                raise TimeoutError(
                    "Android instrumentation stalled: ADB process exit"
                ) from error
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=30)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--deadline", type=float, default=300)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("An instrumentation command is required after --")
    try:
        Progress(args.deadline)
        with args.report.open("wb") as output:
            status = run(command, output, args.deadline)
    except TimeoutError as error:
        parser.exit(124, f"{error}; report: {args.report}\n")
    except RuntimeError as error:
        parser.exit(125, f"{error}; report: {args.report}\n")
    except (OSError, ValueError) as error:
        parser.exit(1, f"Android instrumentation runner: {error}\n")
    raise SystemExit(status)


if __name__ == "__main__":
    main()
