"""Core concise CLI defaults preserve explicit data APIs."""
from contextlib import redirect_stdout, redirect_stderr
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class CoreCheckCLI(unittest.TestCase):
    def run_cli(self, name, arguments, directory):
        environment = dict(os.environ, DYAPP_CHECK_LOG_DIR=str(directory))
        return subprocess.run(
            [sys.executable, "-B", str(ROOT / "tests/scripts" / name),
             *arguments],
            cwd=ROOT, env=environment, text=True, capture_output=True)

    def test_batch_default_raw_json_and_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            args = ["--base", "HEAD", "--head", "HEAD",
                    "--report", str(root / "report.json")]
            normal = self.run_cli("check-batches.py", args, root)
            self.assertEqual(normal.returncode, 0, normal.stderr)
            self.assertEqual(normal.stdout, "OK batches\n")
            raw = self.run_cli("check-batches.py", ["--raw", *args], root)
            self.assertEqual(raw.returncode, 0, raw.stderr)
            self.assertEqual(json.loads(raw.stdout),
                             json.loads((root / "report.json").read_text()))
            bad = self.run_cli("check-batches.py",
                               ["--base", "missing-fixture-ref"], root)
            self.assertNotEqual(bad.returncode, 0)
            self.assertTrue(bad.stdout.startswith("FAIL batches:"))
            self.assertIn("FIX: docs/", bad.stdout)
            self.assertNotIn("Traceback", bad.stdout)

    def test_trigger_diff_api_and_help_passthrough(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            normal = self.run_cli("triggers.py", [], root)
            self.assertEqual(normal.stdout, "OK triggers\n")
            diff = self.run_cli("triggers.py", ["HEAD", "HEAD"], root)
            self.assertEqual(diff.returncode, 0, diff.stderr)
            self.assertEqual(diff.stdout, "no triggers\n")
            help_result = self.run_cli("check-panics.py", ["--help"], root)
            self.assertEqual(help_result.returncode, 0)
            self.assertIn("usage:", help_result.stdout)
            self.assertNotIn("OK panics", help_result.stdout)

    def test_imports_do_not_dispatch_or_print(self):
        for name in ("check-batches.py", "check-repository.py", "docs-sync.py",
                     "triggers.py", "check-panics.py"):
            with self.subTest(name=name):
                spec = importlib.util.spec_from_file_location(
                    "quiet_import", ROOT / "tests/scripts" / name)
                module = importlib.util.module_from_spec(spec)
                stdout, stderr = io.StringIO(), io.StringIO()
                with redirect_stdout(stdout), redirect_stderr(stderr):
                    spec.loader.exec_module(module)
                self.assertEqual(stdout.getvalue(), "")
                self.assertEqual(stderr.getvalue(), "")


if __name__ == "__main__":
    unittest.main()
