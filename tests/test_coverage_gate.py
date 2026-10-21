"""Coverage boundaries and incomplete reports, executed only in Docker."""
import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "coverage_gate", ROOT / "tests/scripts/check_coverage.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class Coverage(unittest.TestCase):
    @staticmethod
    def report(covered=7500, count=10000):
        return {
            "type": "llvm.coverage.json.export",
            "data": [{
                "files": [{"filename": f"/workspace/rust/{crate}/src/lib.rs"}
                          for crate in GATE.CRATES],
                "totals": {"lines": {"count": count, "covered": covered}},
            }],
        }

    def test_exact_threshold_and_above_pass(self):
        self.assertEqual(GATE.validate(self.report(), 75), 75)
        self.assertGreater(GATE.validate(self.report(7501), 75), 75)

    def test_below_threshold_fails_without_rounding(self):
        with self.assertRaises(ValueError):
            GATE.validate(self.report(7499), 75)

    def test_invalid_counts_fail(self):
        for covered, count in ((0, 0), (-1, 100), (101, 100),
                               (True, 100), (70, "100")):
            with self.subTest(covered=covered, count=count):
                with self.assertRaises(ValueError):
                    GATE.validate(self.report(covered, count), 75)

    def test_empty_partial_and_wrong_format_fail(self):
        cases = [{}, {"type": "lcov"}, {"data": []}]
        for change in cases:
            report = copy.deepcopy(self.report()) if change else {}
            report.update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                GATE.validate(report, 75)
        for files in ([], self.report()["data"][0]["files"][:-1]):
            report = self.report()
            report["data"][0]["files"] = files
            with self.subTest(files=files), self.assertRaises(ValueError):
                GATE.validate(report, 75)

    def test_diff_coverage(self):
        diff = ("diff --git a/rust/a.rs b/rust/a.rs\n--- a/rust/a.rs\n"
                "+++ b/rust/a.rs\n@@ -1,0 +2,3 @@\n+x\n+y\n+z\n"
                "@@ -9 +11 @@\n-q\n+w\n"
                "diff --git a/rust/gone.rs b/rust/gone.rs\n"
                "+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n")
        changes = GATE.changed(diff)
        self.assertEqual(changes, {"rust/a.rs": {2, 3, 4, 11}})
        lcov = ("SF:/w/rust/a.rs\nDA:2,1\nDA:4,0\nDA:11,3\nDA:20,0\n"
                "end_of_record\nSF:/elsewhere/b.rs\nDA:1,0\n")
        lines = GATE.instrumented(lcov, Path("/w"))
        self.assertEqual(lines["rust/a.rs"], {2: 1, 4: 0, 11: 3, 20: 0})
        self.assertIn("/elsewhere/b.rs", lines)
        # Line 3 is not instrumented, line 20 is not changed.
        self.assertEqual(GATE.diff_coverage(changes, lines),
                         (2, 3, ["rust/a.rs:4"]))
        self.assertEqual(GATE.diff_coverage({}, lines), (0, 0, []))


if __name__ == "__main__":
    unittest.main()
