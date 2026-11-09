"""Linux lint mode propagates the shared checker result without building."""

import os
from pathlib import Path
import subprocess
import time
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LinuxLintTests(unittest.TestCase):
    def test_lint_only_and_failure(self):
        for status in (0, 37):
            with self.subTest(status=status):
                directory = ROOT / "build/tool-fixtures/linux-lint"
                directory = directory / str(time.time_ns())
                directory.mkdir(parents=True)
                checker = directory / "python3"
                checker.write_text(
                    "#!/bin/sh\n"
                    '[ "$#" -eq 4 ] && [ "$1" = -B ] && '
                    '[ "$2" = "$EXPECTED_CHECKER" ] && '
                    '[ "$3" = --scope ] && [ "$4" = linux ] || exit 99\n'
                    f"exit {status}\n"
                )
                checker.chmod(0o755)
                cargo = directory / "cargo"
                cargo.write_text("#!/bin/sh\nexit 98\n")
                cargo.chmod(0o755)
                expected = ROOT / "tests/scripts/check-rust-complexity.py"
                result = subprocess.run(
                    [
                        "bash",
                        str(ROOT / "tests/scripts/linux-test.sh"),
                        "--raw",
                        "--lint",
                    ],
                    env=dict(
                        os.environ,
                        EXPECTED_CHECKER=str(expected),
                        PATH=f"{directory}:{os.environ['PATH']}",
                    ),
                    capture_output=True,
                    text=True,
                    check=False,
                )
                self.assertEqual(result.returncode, status, result.stderr)
