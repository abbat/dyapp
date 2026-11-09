"""The workspace API gate rejects findings and incomplete coverage."""

import copy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "unused_public_gate", ROOT / "tests/scripts/check-unused-public.py"
)
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


def complete():
    return {
        "schema_version": 5,
        "summary": {
            "target": "x86_64-unknown-linux-gnu",
            "diagnostic_count": 0,
            "includes_non_production_targets": True,
            "feature_profiles": ["all-features"],
            "production": [
                {"package": "dyapp-bootstrap", "binary": "dyappd"},
                {"package": "dyapp-ffi", "library": "dyapp_ffi"},
            ],
        },
        "diagnostics": [],
    }


class UnusedPublicGateTest(unittest.TestCase):
    def test_complete_clean_analysis_passes(self) -> None:
        self.assertEqual(GATE.failures(complete(), 0), [])

    def test_a_new_finding_fails_even_if_tool_exits_zero(self) -> None:
        report = complete()
        report["summary"]["diagnostic_count"] = 1
        report["diagnostics"] = [
            {
                "code": "hawk::dead_public",
                "identity": {"crate": "fixture", "item": "new_unused"},
                "location": {"file": "src/lib.rs", "line": 12},
            }
        ]
        self.assertIn(
            "src/lib.rs:12: hawk::dead_public: fixture:new_unused",
            GATE.failures(report, 0),
        )

    def test_stale_marker_fails(self) -> None:
        report = complete()
        report["summary"]["diagnostic_count"] = 1
        report["diagnostics"] = [
            {
                "code": "hawk::unfulfilled_expectation",
                "message": "unused marker became stale",
            }
        ]
        self.assertTrue(GATE.failures(report, 1))

    def test_tool_failure_cannot_be_reported_as_clean(self) -> None:
        self.assertTrue(GATE.failures(complete(), 101))

    def test_missing_scope_is_rejected(self) -> None:
        incomplete = {
            "target": "",
            "includes_non_production_targets": False,
            "feature_profiles": [],
            "production": [],
        }
        for key, value in incomplete.items():
            report = copy.deepcopy(complete())
            report["summary"][key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                GATE.failures(report, 0)

    def test_missing_or_wrong_schema_is_rejected(self) -> None:
        for version in (None, 4, 6):
            report = complete()
            report["schema_version"] = version
            with self.subTest(version=version), self.assertRaises(ValueError):
                GATE.failures(report, 0)

    def test_truncated_diagnostics_are_rejected(self) -> None:
        report = complete()
        report["summary"]["diagnostic_count"] = 1
        with self.assertRaisesRegex(ValueError, "Incomplete"):
            GATE.failures(report, 0)


if __name__ == "__main__":
    unittest.main()
