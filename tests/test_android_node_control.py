"""Linux emulator node transitions use completion signals instead of sleeps."""

import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile
from types import FrameType
from typing import NoReturn
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/android-node-control.sh"


def stop_fake(_number: int, _frame: FrameType | None) -> NoReturn:
    sys.exit(0)


def fake_node() -> NoReturn:
    signal.signal(signal.SIGTERM, stop_fake)
    os.write(int(os.environ["NODE_READY_FD"]), f"{os.getpid()}\n".encode())
    while True:
        signal.pause()


@unittest.skipUnless(
    sys.platform == "linux", "Linux emulator supervisor uses FIFOs and pidfds"
)
class AndroidNodeControl(unittest.TestCase):
    def setUp(self) -> None:
        scratch = ROOT / "build" / "android-node-control-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(dir=scratch))
        self.ready, writer = os.pipe()
        self.addCleanup(os.close, self.ready)
        self.addCleanup(os.close, writer)
        subprocess.run(
            ["bash", str(SCRIPT), "init", str(self.root)],
            check=True,
            timeout=5,
        )
        with (self.root / "control.log").open("wb") as log:
            self.server = subprocess.Popen(
                [
                    "bash",
                    str(SCRIPT),
                    "serve",
                    str(self.root),
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--fake-node",
                ],
                env={**os.environ, "NODE_READY_FD": str(writer)},
                pass_fds=(writer,),
                stdout=log,
                stderr=log,
            )
        self.addCleanup(self.shutdown)

    def shutdown(self) -> None:
        if self.server.poll() is not None:
            return
        try:
            self.request("shutdown")
        finally:
            if self.server.poll() is None:
                self.server.terminate()
            self.server.wait(timeout=5)

    def request(self, action: str) -> str:
        result = subprocess.run(
            ["bash", str(SCRIPT), "command", str(self.root), action],
            check=True,
            capture_output=True,
            text=True,
            timeout=5,
        )
        return result.stdout.strip()

    def next_pid(self) -> int:
        ready, _, _ = select.select([self.ready], [], [], 5)
        self.assertEqual(ready, [self.ready], "node readiness deadline")
        return int(os.read(self.ready, 128))

    def pidfd(self, pid: int) -> int:
        descriptor = os.pidfd_open(pid)
        self.addCleanup(os.close, descriptor)
        return descriptor

    def assert_exited(self, descriptor: int) -> None:
        ready, _, _ = select.select([descriptor], [], [], 0)
        self.assertEqual(ready, [descriptor], "acknowledged node still runs")

    def test_stop_restart_and_shutdown_acknowledge_the_same_child_lifecycle(
        self,
    ) -> None:
        first = self.next_pid()
        first_fd = self.pidfd(first)
        self.assertEqual(self.request("status"), f"status running {first}")
        self.assertEqual(self.request("start"), f"start spawned {first}")
        self.assertEqual(self.request("stop"), "stop stopped")
        self.assert_exited(first_fd)
        self.assertEqual(self.request("status"), "status stopped")
        self.assertEqual(self.request("stop"), "stop stopped")

        started = self.request("start")
        second = self.next_pid()
        second_fd = self.pidfd(second)
        self.assertEqual(started, f"start spawned {second}")
        self.assertEqual(self.request("status"), f"status running {second}")
        self.assertEqual(self.request("shutdown"), "shutdown stopped")
        self.assert_exited(second_fd)
        self.assertEqual(self.server.wait(timeout=5), 0)

    def test_invalid_action_cannot_stop_the_node(self) -> None:
        pid = self.next_pid()
        result = subprocess.run(
            ["bash", str(SCRIPT), "command", str(self.root), "invalid"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.request("status"), f"status running {pid}")

    def test_stale_acknowledgement_fails_and_termination_reaps_the_node(
        self,
    ) -> None:
        pid = self.next_pid()
        descriptor = self.pidfd(pid)
        with (self.root / "reply").open("w") as reply:
            reply.write("stop stopped\n")
        result = subprocess.run(
            ["bash", str(SCRIPT), "command", str(self.root), "status"],
            capture_output=True,
            text=True,
            timeout=5,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("Unexpected node acknowledgement", result.stderr)
        # A failed exchange is terminal; the remaining reply cannot be reused.
        self.server.terminate()
        self.assertEqual(self.server.wait(timeout=5), 128 + signal.SIGTERM)
        self.assert_exited(descriptor)


if __name__ == "__main__":
    if sys.argv[1:] == ["--fake-node"]:
        fake_node()
    unittest.main()
