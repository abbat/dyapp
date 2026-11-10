"""Release artifact checks must reject test hooks and absent build output."""

import os
from pathlib import Path
import subprocess
import sys
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools/scripts/check-no-ui-smoke.py"


class NoUiSmokeTest(unittest.TestCase):
    def test_missing_empty_and_instrumented_artifacts_fail(self) -> None:
        directory = ROOT / "build/tool-fixtures/no-ui-smoke"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        cases = (
            (None, False),
            (b"", False),
            (b"ordinary native bytes", True),
            (b"before ui_test_result after", False),
            (b"before UI_SMOKE_PASS: after", False),
            (b"before UI_SMOKE_FAIL: after", False),
        )
        for index, (data, succeeds) in enumerate(cases):
            path = directory / str(index)
            if data is not None:
                path.write_bytes(data)
            result = subprocess.run(
                [sys.executable, str(SCRIPT), str(path)],
                capture_output=True,
                check=False,
                env=dict(
                    os.environ, DYAPP_CHECK_LOG_DIR=str(directory / "logs")
                ),
            )
            self.assertEqual(
                result.returncode == 0, succeeds, result.stderr.decode()
            )
            self.assertEqual(result.stderr, b"")
            if succeeds:
                self.assertEqual(result.stdout, b"OK no-ui-smoke\n")
            else:
                self.assertTrue(result.stdout.startswith(b"FAIL no-ui-smoke:"))
                self.assertIn(b": raw captured output", result.stdout)
            raw = subprocess.run(
                [sys.executable, str(SCRIPT), "--raw", str(path)],
                capture_output=True,
                check=False,
            )
            self.assertEqual(raw.returncode, result.returncode)
            self.assertNotIn(b"OK no-ui-smoke", raw.stdout)

    def test_production_launcher_maps_startup_failure_without_sdk(
        self,
    ) -> None:
        directory = ROOT / "build/tool-fixtures/production-output"
        directory = directory / str(time.time_ns())
        result = subprocess.run(
            [
                sys.executable,
                str(ROOT / "tests/scripts/production-desktop-smoke.py"),
            ],
            capture_output=True,
            check=False,
            env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(directory)),
        )
        self.assertEqual(result.returncode, 1)
        self.assertTrue(
            result.stdout.startswith(b"FAIL production-desktop-smoke:")
        )
        self.assertIn(b"Usage: production-desktop-smoke.py", result.stdout)
        self.assertNotIn(b"Unmapped check", result.stdout)
        self.assertEqual(result.stderr, b"")


if __name__ == "__main__":
    unittest.main()
