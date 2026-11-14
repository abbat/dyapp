"""Permission changes bracket separate instrumentation processes.

JUnit reports are authoritative even when adb exits successfully.
"""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tests/scripts/android-lan-permission-phases.sh"
FAKE_ADB = """#!/usr/bin/env python3
import json
import os
from pathlib import Path
import sys
with Path(os.environ["FAKE_ADB_LOG"]).open("a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
if "instrument" in sys.argv:
    phase = sys.argv[sys.argv.index("localNetworkPermissionPhase") + 1]
    failed = phase == os.environ.get("FAIL_PHASE")
    print("FAILURES!!!" if failed else "OK (1 test)")
"""


class AndroidLanPhases(unittest.TestCase):
    def run_phase(self, mode: str, fail: str = "") -> tuple[int, list[str]]:
        with tempfile.TemporaryDirectory(dir=ROOT / "build") as directory:
            root = Path(directory)
            adb = root / "adb"
            adb.write_text(FAKE_ADB)
            adb.chmod(0o755)
            log = root / "calls.jsonl"
            env = dict(
                os.environ,
                PATH=str(root) + os.pathsep + os.environ["PATH"],
                FAKE_ADB_LOG=str(log),
                FAIL_PHASE=fail,
                DYAPP_CHECK_LOG_DIR=str(root / "checks"),
            )
            result = subprocess.run(
                [
                    "bash",
                    str(SCRIPT),
                    mode,
                    str(root / "reports"),
                    "10.0.2.2:33445",
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
                timeout=30,
            )
            calls = (
                [json.loads(line) for line in log.read_text().splitlines()]
                if log.exists()
                else []
            )
            events = []
            for call in calls:
                self.assertEqual(call[:3], ["-s", "emulator-5554", "shell"])
                if call[3] == "pm":
                    self.assertEqual(
                        call[5:],
                        [
                            "com.dyapp",
                            "android.permission.ACCESS_LOCAL_NETWORK",
                        ],
                    )
                    events.append(call[4])
                else:
                    self.assertEqual(call[3:6], ["am", "instrument", "-w"])
                    events.append(
                        call[call.index("localNetworkPermissionPhase") + 1]
                    )
                    self.assertEqual(
                        call[call.index("localNetworkNode") + 1],
                        "10.0.2.2:33445",
                    )
            return result.returncode, events

    def test_legacy_lane_never_mutates_an_unsupported_permission(self) -> None:
        code, events = self.run_phase("legacy")
        self.assertEqual(code, 0)
        self.assertEqual(events, ["legacy"])

    def test_modern_uses_separate_instrumentations(
        self,
    ) -> None:
        code, events = self.run_phase("modern")
        self.assertEqual(code, 0)
        self.assertEqual(
            events,
            [
                "grant",
                "granted",
                "revoke",
                "denied",
                "grant",
                "regranted",
                "revoke",
                "revoked",
                "grant",
            ],
        )

    def test_failed_junit_report_stops_even_when_adb_returns_zero(
        self,
    ) -> None:
        code, events = self.run_phase("modern", fail="denied")
        self.assertNotEqual(code, 0)
        self.assertEqual(events, ["grant", "granted", "revoke", "denied"])

    def test_unknown_mode_never_contacts_a_device(self) -> None:
        code, events = self.run_phase("unknown")
        self.assertEqual(code, 2)
        self.assertEqual(events, [])


if __name__ == "__main__":
    unittest.main()
