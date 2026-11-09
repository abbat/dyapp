"""Release artifact checks must reject test hooks and absent build output."""

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
            )
            self.assertEqual(
                result.returncode == 0, succeeds, result.stderr.decode()
            )


if __name__ == "__main__":
    unittest.main()
