"""C3: the Scorecard score ratchet."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_scorecard", ROOT / "tests/scripts/check-scorecard.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
RESULT = {"score": 6.2, "checks": [
    {"name": "Maintained", "score": 10, "reason": "active"},
    {"name": "Code-Review", "score": 0, "reason": "no reviews"},
    {"name": "Fuzzing", "score": 3, "reason": "partial"},
    {"name": "Packaging", "score": -1, "reason": "not applicable"}]}


class Scorecard(unittest.TestCase):
    def test_below_the_baseline_fails(self):
        with self.assertRaisesRegex(ValueError, "6.2 < baseline 6.5"):
            CHECK.check(RESULT, 6.5)

    def test_baseline_failure_names_weak_checks_and_advisories(self):
        result = {"score": 7.0, "checks": [
            {"name": "Vulnerabilities", "score": 6,
             "reason": "4 existing vulnerabilities detected",
             "details": ["Warn: Project is vulnerable to: "
                         "https://osv.dev/RUSTSEC-2026-0387"]},
            {"name": "Fuzzing", "score": 10, "reason": "project is fuzzed"}]}
        with self.assertRaises(ValueError) as raised:
            CHECK.check(result, 7.1)
        self.assertIn("Vulnerabilities: 6 - 4 existing "
                      "vulnerabilities detected",
                      str(raised.exception))
        self.assertIn("RUSTSEC-2026-0387", str(raised.exception))
        self.assertNotIn("Fuzzing", str(raised.exception))

    def test_below_target_lists_weak_checks(self):
        self.assertEqual(CHECK.check(RESULT, 5.5).splitlines(), [
            "Scorecard 6.2 (baseline 5.5, target 7.0)",
            "Raise [scorecard] score in tests/scripts/baselines.toml to 6.2",
            "  Code-Review: 0 - no reviews",
            "  Fuzzing: 3 - partial"])


if __name__ == "__main__":
    unittest.main()
