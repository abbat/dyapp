"""Retained diagnostics survive an interrupted check without close()."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/scripts"))
from capture import Capture  # noqa: E402 -- project library path


class RetainedLogs(unittest.TestCase):
    def test_written_chunk_is_visible_before_close(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "check.log"
            captured = Capture(log_file=log)
            try:
                captured.write(b"phase: load\n")
                self.assertEqual(log.read_bytes(), b"phase: load\n")
                captured.write(b"phase: audit\n")
                self.assertEqual(
                    log.read_bytes(), b"phase: load\nphase: audit\n"
                )
            finally:
                captured.close()

    def test_abrupt_process_exit_keeps_small_diagnostic(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "abrupt.log"
            script = (
                "from capture import Capture; import os, sys; "
                "capture = Capture(log_file=sys.argv[1]); "
                "capture.write(b'last completed phase\\n'); os._exit(7)"
            )
            process = subprocess.run(
                [sys.executable, "-c", script, str(log)],
                env={**os.environ, "PYTHONPATH": str(ROOT / "tools/scripts")},
                capture_output=True,
                check=False,
                timeout=10,
            )
            self.assertEqual(process.returncode, 7, process.stderr)
            self.assertEqual(log.read_bytes(), b"last completed phase\n")
