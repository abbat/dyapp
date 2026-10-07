"""Coverage boundaries and incomplete reports, executed only in Docker."""
import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "coverage_gate", ROOT / "scripts/check_coverage.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class Coverage(unittest.TestCase):
    @staticmethod
    def report(covered=7000, count=10000):
        return {
            "type": "llvm.coverage.json.export",
            "data": [{
                "files": [{"filename": f"/workspace/rust/{crate}/src/lib.rs"}
                          for crate in GATE.CRATES],
                "totals": {"lines": {"count": count, "covered": covered}},
            }],
        }

    def test_exact_threshold_and_above_pass(self):
        self.assertEqual(GATE.validate(self.report()), 70)
        self.assertGreater(GATE.validate(self.report(7001)), 70)

    def test_below_threshold_fails_without_rounding(self):
        with self.assertRaises(ValueError):
            GATE.validate(self.report(6999))

    def test_invalid_counts_fail(self):
        for covered, count in ((0, 0), (-1, 100), (101, 100),
                               (True, 100), (70, "100")):
            with self.subTest(covered=covered, count=count):
                with self.assertRaises(ValueError):
                    GATE.validate(self.report(covered, count))

    def test_empty_partial_and_wrong_format_fail(self):
        cases = [{}, {"type": "lcov"}, {"data": []}]
        for change in cases:
            report = copy.deepcopy(self.report()) if change else {}
            report.update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                GATE.validate(report)
        for files in ([], self.report()["data"][0]["files"][:-1]):
            report = self.report()
            report["data"][0]["files"] = files
            with self.subTest(files=files), self.assertRaises(ValueError):
                GATE.validate(report)


if __name__ == "__main__":
    unittest.main()
