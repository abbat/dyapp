"""B5 flaky detection from JUnit reports and the quarantine deadline."""
from datetime import date
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_flaky", ROOT / "scripts/check-flaky.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
REPORT = b"""<testsuites><testsuite name="core">
<testcase classname="core" name="stable"/>
<testcase classname="core" name="retried"><flakyFailure/></testcase>
<testcase classname="core" name="erred"><flakyError/></testcase>
<testcase classname="core" name="failed"><failure/></testcase>
</testsuite></testsuites>"""
TODAY = date(2026, 10, 19)


class Flaky(unittest.TestCase):
    def test_only_tests_passing_on_retry_are_flaky(self):
        self.assertEqual(CHECK.flaky_tests(REPORT),
                         ["core erred", "core retried"])

    def test_xcresult_tests_passing_on_repetition_are_flaky(self):
        def case(name, result, *runs):
            return {"nodeType": "Test Case", "name": name, "result": result,
                    "children": [{"nodeType": "Repetition", "result": run}
                                 for run in runs]}
        report = {"testNodes": [{"nodeType": "Test Plan", "children": [{
            "nodeType": "UI test bundle", "children": [{
                "nodeType": "Test Suite", "name": "AppUITests",
                "children": [case("testStable()", "Passed"),
                             case("testRetried()", "Passed",
                                  "Failed", "Passed"),
                             case("testBroken()", "Failed",
                                  "Failed", "Failed", "Failed")]}]}]}]}
        self.assertEqual(CHECK.xcresult_flaky(report),
                         ["AppUITests testRetried()"])

    def test_android_failures_before_a_passing_rerun_are_flaky(self):
        output = ("com.dyapp.AppUITests:.E\nThere was 1 failure:\n"
                  "1) launches(com.dyapp.AppUITests)\njava.lang.Error\n"
                  "\nFAILURES!!!\nTests run: 3,  Failures: 1\n")
        self.assertEqual(CHECK.instrumentation_failures(output),
                         ["com.dyapp.AppUITests launches"])
        self.assertEqual(CHECK.instrumentation_failures("Process crashed."),
                         ["android instrumentation"])

    def test_unlisted_and_expired_tests_fail(self):
        flaky = ["core erred", "core retried"]
        quarantine = {"core erred": date(2026, 10, 12),
                      "core old": date(2026, 10, 11)}
        self.assertEqual(CHECK.flaky_errors(flaky, quarantine, TODAY), [
            "core retried: flaky, open a bead and add it to [flaky] in "
            "baselines.toml with 2026-10-19",
            "core old: quarantined since 2026-10-11, more than 7 days; "
            "fix it and remove the entry",
        ])
        quarantine["core retried"] = TODAY
        del quarantine["core old"]
        self.assertEqual(CHECK.flaky_errors(flaky, quarantine, TODAY), [])

    def test_repository_quarantine_is_fresh(self):
        quarantine = CHECK.tomllib.loads(CHECK.BASELINES.read_text())["flaky"]
        self.assertEqual(CHECK.flaky_errors([], quarantine, date.today()), [])


if __name__ == "__main__":
    unittest.main()
