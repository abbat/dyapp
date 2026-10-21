"""Exporter payload and failure controls without Docker or SDK preparation."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ExportChecks(unittest.TestCase):
    def test_exports_keep_copy_arguments_and_reject_invalid_containers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            docker = root / "docker"
            docker.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "from pathlib import Path\n"
                "args = sys.argv[1:]\n"
                "with open(os.environ['CALLS'], 'a') as output:\n"
                " output.write(json.dumps(args) + '\\n')\n"
                "if args[0] == 'ps':\n"
                " if os.environ['MODE'] != 'empty':"
                " print('abcdef123456')\n"
                "elif args[0] == 'inspect':\n"
                " mode = os.environ['MODE']\n"
                " print(json.dumps([{'Config': {\n"
                " 'Hostname': 'bad' if mode == 'hostname'"
                " else 'abcdef123456',\n"
                " 'Env': ['COVERAGE_RUN_ID=' +"
                " ('bad' if mode == 'run-id' else 'a' * 32)]}}]))\n"
                "elif args[0] == 'cp':\n"
                " if os.environ['MODE'] == 'copy': sys.exit(23)\n"
                " Path(args[-1]).write_text(args[-2])\n")
            docker.chmod(0o755)
            calls = root / "calls"
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       CALLS=str(calls),
                       DYAPP_CHECK_LOG_DIR=str(root / "logs"))

            def run(script, args, mode="good"):
                return subprocess.run(
                    [sys.executable, str(ROOT / "scripts" / script), *args],
                    env=dict(env, MODE=mode), capture_output=True, text=True)

            for script, name, args in [
                ("export-coverage.py", "coverage-export",
                 [str(root / "coverage")]),
                ("export-ui-results.py", "ui-export",
                 ["android", str(root / "android")]),
            ]:
                good = run(script, args)
                self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
                self.assertEqual(good.stdout, f"OK {name}\n")
                for mode in ("empty", "copy",
                             "run-id" if name == "coverage-export"
                             else "hostname"):
                    bad = run(script, args, mode)
                    self.assertEqual(bad.returncode, 1)
                    self.assertTrue(bad.stdout.startswith(f"FAIL {name}:"))
                    self.assertIn("\nFIX: docs/", bad.stdout)
                before = calls.read_text()
                for flags in (["--help"], ["--raw", "--help"]):
                    self.assertEqual(run(script, flags).returncode, 0)
                self.assertEqual(calls.read_text(), before)
                raw = run(script, ["--raw", *args])
                self.assertEqual(raw.returncode, 0)
                self.assertEqual(raw.stdout, "")
            coverage_source = ("abcdef123456:/workspace/build/coverage."
                               f'{"a" * 32}/coverage.json')
            self.assertEqual((root / "coverage/coverage.json").read_text(),
                             coverage_source)
            self.assertEqual(
                (root / "android/app-debug.apk").read_text(),
                "abcdef123456:/app/build/android/app/outputs/apk/"
                "debug/app-debug.apk")
            self.assertTrue(json.loads(calls.read_text().splitlines()[0]))

    def test_model_report_is_data_and_self_check_is_a_check(self):
        with tempfile.TemporaryDirectory() as directory:
            def run(*args):
                return subprocess.run(
                    [sys.executable, str(ROOT / "scripts/search-model.py"),
                     *args], capture_output=True, text=True,
                    env=dict(os.environ, DYAPP_CHECK_LOG_DIR=directory))

            self.assertEqual(run("--self-check").stdout, "OK search-model\n")
            self.assertEqual(run("--raw", "--self-check").stdout,
                             "self-check OK\n")
            report = run()
            self.assertEqual(report.returncode, 0)
            self.assertIn("defaults (no --bench)", report.stdout)
            self.assertEqual(run("--raw").stdout, report.stdout)
            bad = run("--self-check", "--invalid")
            self.assertEqual(bad.returncode, 2)
            self.assertTrue(bad.stdout.startswith("FAIL search-model:"))


if __name__ == "__main__":
    unittest.main()
