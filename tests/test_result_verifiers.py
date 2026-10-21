"""Exercise strict report gates inside the dev Docker container."""
import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "apple_results", ROOT / "tests/scripts/check-apple-results.py")
APPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APPLE)


class Reports(unittest.TestCase):
    def test_apple_accepts_complete_inventory(self):
        self.assertEqual(APPLE.validate(self.apple_summary()), 3)

    @staticmethod
    def apple_summary():
        return {"totalTestCount": 3, "passedTests": 3, "failedTests": 0,
                "skippedTests": 0, "result": "Passed"}

    def test_apple_rejects_partial_failed_skipped_and_malformed(self):
        cases = [{}, {"totalTestCount": 0}, {"passedTests": 2},
                 {"failedTests": 1}, {"skippedTests": 1},
                 {"result": "Failed"}, {"passedTests": True},
                 {"totalTestCount": 2, "passedTests": 2}]
        for change in cases:
            data = copy.deepcopy(self.apple_summary()) if change else {}
            data.update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                APPLE.validate(data)

    def run_validator(self, script, contents, junit=False):
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            filename = "TEST-app.xml" if junit else "report.txt"
            path = Path(directory) / filename
            if contents is not None:
                path.write_text(contents)
            argument = directory if junit else str(path)
            return subprocess.run(
                ["python3", str(ROOT / "tests/scripts" / script), argument],
                capture_output=True, text=True, check=False)

    def test_cli_defaults_raw_and_retained_logs(self):
        coverage = {"type": "llvm.coverage.json.export", "data": [{
            "files": [{"filename": f"/rust/{crate}/src/lib.rs"} for crate in
                      ("bootstrap", "ffi", "identity", "messaging",
                       "p2p-net", "profile", "video")],
            "totals": {"lines": {"count": 100, "covered": 100}}}]}
        cases = [
            ("check_coverage.py", "coverage", json.dumps(coverage), False,
             "100.00% line coverage"),
            ("check-junit-results.py", "junit",
             '<testsuite tests="1"/>', True, "1 JUnit tests passed"),
            ("check-apple-results.py", "apple-results",
             json.dumps(self.apple_summary()), False, "3 native tests passed"),
            ("check-instrumentation-results.py", "instrumentation",
             "OK (3 tests)\nINSTRUMENTATION_CODE: -1\n", False,
             "INSTRUMENTATION_CODE: -1"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            logs = root / "logs"
            env = dict(os.environ, DYAPP_CHECK_LOG_DIR=str(logs))
            for script, name, contents, junit, raw_text in cases:
                path = root / ("TEST-app.xml" if junit else "report.txt")
                path.write_text(contents)
                argument = str(root if junit else path)
                command = ["python3", str(ROOT / "tests/scripts" / script)]
                result = subprocess.run(
                    command + [argument], capture_output=True, text=True,
                    env=env, check=False)
                self.assertEqual(result.returncode, 0, result.stdout)
                self.assertEqual(result.stdout, f"OK {name}\n")
                raw = subprocess.run(
                    command + ["--raw", argument], capture_output=True,
                    text=True, env=env, check=False)
                self.assertEqual(raw.returncode, 0, raw.stderr)
                self.assertIn(raw_text, raw.stdout)
                saved = list(logs.glob(f"{name}-*.log"))
                self.assertEqual(len(saved), 1)
                self.assertIn(raw_text, saved[0].read_text())
                if name == "instrumentation":
                    self.assertEqual(json.loads(
                        path.with_suffix(".json").read_text())["tests"], 3)

    def test_junit_requires_nonempty_complete_success(self):
        good = '<testsuite tests="1" failures="0" errors="0" skipped="0"/>'
        self.assertEqual(
            self.run_validator(
                "check-junit-results.py", good, True).returncode, 0)
        for bad in (None, "", "<broken", '<testsuite tests="0"/>',
                    '<testsuite tests="1" failures="1"/>',
                    '<testsuite tests="1" errors="1"/>',
                    '<testsuite tests="1" skipped="1"/>'):
            with self.subTest(report=bad):
                self.assertNotEqual(
                    self.run_validator(
                        "check-junit-results.py", bad, True).returncode, 0)

    def test_instrumentation_rejects_zero_exit_failure_output(self):
        good = "OK (3 tests)\nINSTRUMENTATION_CODE: -1\n"
        pretty = ("com.dyapp.AppUITests:...\n"
                  "Time: 39.948\n\nOK (3 tests)\n")
        for report in (good, pretty):
            self.assertEqual(
                self.run_validator(
                    "check-instrumentation-results.py", report).returncode, 0)
        for bad in (None, "", "OK (0 tests)\nINSTRUMENTATION_CODE: -1",
                    "OK (2 tests)\nINSTRUMENTATION_CODE: -1",
                    pretty + "Process crashed", good + "FAILURES!!!",
                    good.replace("-1", "-2"),
                    "INSTRUMENTATION_STATUS: partial\nOK (3 tests)",
                    "INSTRUMENTATION_CODE: -1\nFAILURES!!!"):
            with self.subTest(report=bad):
                self.assertNotEqual(
                    self.run_validator(
                        "check-instrumentation-results.py", bad).returncode, 0)


if __name__ == "__main__":
    unittest.main()
