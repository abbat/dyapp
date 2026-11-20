"""Cold PNG checks require acknowledged shutdown and preserve failures."""

from pathlib import Path
import subprocess
import unittest
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/android-png-phases.sh"
HARNESS = r"""#!/bin/bash
set -euo pipefail
source "$1"
events=$2
failure=$3
node=running
host=running
prepared=0
mark() { printf '%s\n' "$1" >> "$events"; }
png_phase() {
    mark "$1"
    case "$1" in
        prepare)
            [[ $node == running && $host == running ]] || return 90
            [[ $failure != prepare ]] || return 7
            prepared=1
            ;;
        reopenOffline)
            [[ $node == stopped && $host == stopped && $prepared == 1 ]] ||
                return 91
            [[ $failure != reopenOffline ]] || return 8
            ;;
        cleanup)
            [[ $failure != cleanup ]] || return 9
            prepared=0
            ;;
        *) return 92 ;;
    esac
}
stop_photo_host() {
    mark host-stop
    [[ $failure != host-stop ]] || return 10
    host=stopped
}
node_action() {
    mark "node-$1"
    [[ ${2:-} == png-node ]] || return 93
    case "$1" in
        stop)
            [[ $node == running && $host == stopped ]] || return 94
            [[ $failure != node-stop ]] || return 11
            node=stopped
            ;;
        start)
            [[ $node == stopped ]] || return 95
            [[ $failure != node-start ]] || return 12
            node=running
            ;;
        *) return 96 ;;
    esac
}
run_png_phases
"""


class AndroidPngPhases(unittest.TestCase):
    def run_phases(self, failure: str = "") -> tuple[int, list[str]]:
        directory = ROOT / "build/tool-fixtures/png-phases" / uuid4().hex
        directory.mkdir(parents=True)
        harness = directory / "harness.sh"
        harness.write_text(HARNESS)
        events = directory / "events.txt"
        result = subprocess.run(
            ["bash", str(harness), str(SCRIPT), str(events), failure],
            capture_output=True,
            text=True,
            check=False,
            timeout=10,
        )
        self.assertEqual(result.stderr, "")
        return result.returncode, events.read_text().splitlines()

    def test_cold_read_requires_both_shutdown_acknowledgements(self) -> None:
        code, calls = self.run_phases()
        self.assertEqual(code, 0)
        self.assertEqual(
            calls,
            [
                "prepare",
                "host-stop",
                "node-stop",
                "reopenOffline",
                "node-start",
                "cleanup",
            ],
        )

    def test_failed_preparation_stops_host_without_touching_the_node(
        self,
    ) -> None:
        code, calls = self.run_phases("prepare")
        self.assertNotEqual(code, 0)
        self.assertEqual(calls, ["prepare", "host-stop", "cleanup"])

    def test_uncertain_host_stop_never_starts_the_offline_transition(
        self,
    ) -> None:
        code, calls = self.run_phases("host-stop")
        self.assertNotEqual(code, 0)
        self.assertEqual(calls, ["prepare", "host-stop", "cleanup"])

    def test_uncertain_node_stop_is_not_followed_by_another_fifo_command(
        self,
    ) -> None:
        code, calls = self.run_phases("node-stop")
        self.assertNotEqual(code, 0)
        self.assertEqual(
            calls, ["prepare", "host-stop", "node-stop", "cleanup"]
        )

    def test_cold_read_failure_still_restores_node_and_attempts_cleanup(
        self,
    ) -> None:
        code, calls = self.run_phases("reopenOffline")
        self.assertNotEqual(code, 0)
        self.assertEqual(calls[-2:], ["node-start", "cleanup"])

    def test_failed_node_restart_and_cleanup_cannot_turn_green(self) -> None:
        for failure in ("node-start", "cleanup"):
            with self.subTest(failure=failure):
                code, calls = self.run_phases(failure)
                self.assertNotEqual(code, 0)
                self.assertEqual(calls[-1], "cleanup")

    def test_direct_execution_does_not_pass_without_running_any_phase(
        self,
    ) -> None:
        result = subprocess.run(
            ["bash", str(SCRIPT)], capture_output=True, check=False, timeout=10
        )
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
