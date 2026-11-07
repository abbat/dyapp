"""Native Linux lint mode must propagate Clippy failures without building."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class LinuxLintTests(unittest.TestCase):
    def test_lint_only_and_failure(self):
        for status in (0, 37):
            with self.subTest(status=status), \
                    tempfile.TemporaryDirectory() as directory:
                cargo = Path(directory) / "cargo"
                cargo.write_text(
                    '#!/bin/sh\n[ "$1" = clippy ] || exit 99\n'
                    f'exit {status}\n')
                cargo.chmod(0o755)
                result = subprocess.run(
                    ["bash", str(ROOT / "tests/scripts/linux-test.sh"),
                     "--raw", "--lint"],
                    env=dict(os.environ,
                             PATH=f"{directory}:{os.environ['PATH']}"),
                    capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, status, result.stderr)
