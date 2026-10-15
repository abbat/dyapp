"""Quiet check output preserves failures and full raw evidence."""
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class CheckOutput(unittest.TestCase):
    def run_check(self, directory, name, command):
        return subprocess.run(
            [sys.executable, "-B", str(ROOT / "scripts/run-check.py"),
             "--log-dir", str(directory), name, "--", *command],
            cwd=ROOT, text=True, capture_output=True, check=False)

    def test_noisy_success_is_one_ok_line(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.run_check(directory, "flake8", [
                sys.executable, "-c", "print('passing progress')"])
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "OK flake8\n")
            self.assertEqual(result.stderr, "")

    def test_actual_python_check_and_third_party_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            report = root / "TEST-empty.xml"
            report.write_text('<testsuite tests="0"/>')
            bad = root / "bad.py"
            bad.write_text("bad =\n")
            commands = {
                "junit": [sys.executable, str(
                    ROOT / "scripts/check-junit-results.py"), str(root)],
                "flake8": [sys.executable, "-m", "flake8", str(bad)],
            }
            for name, command in commands.items():
                result = self.run_check(root, name, command)
                self.assertNotEqual(result.returncode, 0)
                lines = result.stdout.splitlines()
                self.assertTrue(lines[0].startswith(f"FAIL {name}:"))
                self.assertTrue(lines[1].startswith("FIX: docs/"))
                self.assertEqual(lines[2], "DETAILS:")
                self.assertNotIn("Traceback", result.stdout)
                self.assertEqual(result.stderr, "")
                if name == "flake8":
                    self.assertIn("E999", result.stdout)
                else:
                    self.assertIn("Empty JUnit inventory", result.stdout)

    def test_json_failure_omits_passing_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            payload = '{"errors":["broken fixture"],"passing":100}'
            command = f"print({payload!r}); exit(4)"
            result = self.run_check(directory, "json-check", [
                sys.executable, "-c", command])
            self.assertEqual(result.returncode, 4)
            self.assertIn("broken fixture", result.stdout)
            self.assertNotIn("passing", result.stdout)

    def test_large_unknown_failure_retains_raw_output(self):
        with tempfile.TemporaryDirectory() as directory:
            result = self.run_check(directory, "flake8", [
                sys.executable, "-c",
                "print('P' * 400000); print('unknown cause'); exit(7)"])
            self.assertEqual(result.returncode, 7)
            self.assertNotIn("P" * 100, result.stdout)
            log, = Path(directory).glob("*.log")
            self.assertIn("P" * 400000, log.read_text())
            self.assertIn("unknown cause", log.read_text())
            self.assertIn(str(log), result.stdout)

    def test_missing_command_and_signal_remain_failures(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = self.run_check(directory, "flake8",
                                     ["missing-check-fixture-command"])
            self.assertEqual(missing.returncode, 127)
            stopped = self.run_check(directory, "flake8", [
                sys.executable, "-c",
                "import os,signal; os.kill(os.getpid(), signal.SIGTERM)"])
            self.assertEqual(stopped.returncode, 128 + signal.SIGTERM)
            self.assertIn("signal 15", stopped.stdout)


if __name__ == "__main__":
    unittest.main()
