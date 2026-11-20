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
if "exec-out" in sys.argv:
    print(Path(os.environ["FAKE_RECEIPT"]).read_text())
    sys.exit(0)
if "instrument" in sys.argv:
    phase = sys.argv[sys.argv.index("localNetworkPermissionPhase") + 1]
    failed = phase == os.environ.get("FAIL_PHASE")
    if not failed and "localNetworkPublicNode" in sys.argv:
        public = sys.argv[sys.argv.index("localNetworkPublicNode") + 1]
        if phase == os.environ.get("FAIL_RECEIPT"):
            public = "wrong-route"
        Path(os.environ["FAKE_RECEIPT"]).write_text(json.dumps({
            "phase": phase,
            "node": sys.argv[sys.argv.index("localNetworkNode") + 1],
            "publicNode": public, "nodePeer": "owned-node",
            "owner": "retained-owner", "process": f"process-{phase}",
            "sdk": 37, "target": 37,
        }))
    for code in (1, -2 if failed else 0):
        for key, value in {
            "id": "AndroidJUnitRunner", "current": "1",
            "class": "com.dyapp.LocalNetworkPermissionPhaseTests",
            "test": phase,
        }.items():
            print(f"INSTRUMENTATION_STATUS: {key}={value}")
        print(f"INSTRUMENTATION_STATUS_CODE: {code}")
    print("INSTRUMENTATION_RESULT: stream=")
    print("FAILURES!!!" if failed else "OK (1 test)")
    print("INSTRUMENTATION_CODE: -1")
"""


class AndroidLanPhases(unittest.TestCase):
    def run_phase(
        self,
        mode: str,
        fail: str = "",
        *,
        public_node: str = "198.51.100.2:33445",
        fail_receipt: str = "",
    ) -> tuple[int, list[str]]:
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
                FAKE_RECEIPT=str(root / "device-receipt.json"),
                FAIL_PHASE=fail,
                FAIL_RECEIPT=fail_receipt,
                DYAPP_CHECK_LOG_DIR=str(root / "checks"),
            )
            result = subprocess.run(
                [
                    "bash",
                    str(SCRIPT),
                    mode,
                    str(root / "reports"),
                    "10.0.2.2:33445",
                    *(
                        [public_node]
                        if mode == "modern" and public_node
                        else []
                    ),
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
                self.assertEqual(call[:2], ["-s", "emulator-5554"])
                if call[2] == "exec-out":
                    self.assertEqual(
                        call[3:],
                        [
                            "run-as",
                            "com.dyapp",
                            "cat",
                            "no_backup/lan-permission-phase.json",
                        ],
                    )
                    continue
                self.assertEqual(call[2], "shell")
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
                    self.assertEqual(
                        call[3:7], ["am", "instrument", "-w", "-r"]
                    )
                    events.append(
                        call[call.index("localNetworkPermissionPhase") + 1]
                    )
                    self.assertEqual(
                        call[call.index("localNetworkNode") + 1],
                        "10.0.2.2:33445",
                    )
                    if mode == "modern":
                        self.assertEqual(
                            call[call.index("localNetworkPublicNode") + 1],
                            public_node,
                        )
                    else:
                        self.assertNotIn("localNetworkPublicNode", call)
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
                "cleanup",
            ],
        )

    def test_failed_junit_report_stops_even_when_adb_returns_zero(
        self,
    ) -> None:
        code, events = self.run_phase("modern", fail="denied")
        self.assertNotEqual(code, 0)
        self.assertEqual(events, ["grant", "granted", "revoke", "denied"])

    def test_cleanup_failure_is_not_reported_as_a_passing_sequence(
        self,
    ) -> None:
        code, events = self.run_phase("modern", fail="cleanup")
        self.assertNotEqual(code, 0)
        self.assertEqual(events[-2:], ["grant", "cleanup"])

    def test_missing_public_control_never_contacts_a_device(self) -> None:
        for public_node in ("", "10.0.2.2:33445"):
            with self.subTest(public_node=public_node):
                code, events = self.run_phase(
                    "modern", public_node=public_node
                )
                self.assertEqual(code, 2)
                self.assertEqual(events, [])

    def test_wrong_public_receipt_stops_even_after_passing_junit(self) -> None:
        code, events = self.run_phase("modern", fail_receipt="denied")
        self.assertNotEqual(code, 0)
        self.assertEqual(events, ["grant", "granted", "revoke", "denied"])

    def test_unknown_mode_never_contacts_a_device(self) -> None:
        code, events = self.run_phase("unknown")
        self.assertEqual(code, 2)
        self.assertEqual(events, [])


if __name__ == "__main__":
    unittest.main()
