"""Exercise strict report gates inside the dev Docker container."""
import copy
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "apple_results", ROOT / "scripts/check-apple-results.py")
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
                ["python3", str(ROOT / "scripts" / script), argument],
                capture_output=True, text=True, check=False)

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
