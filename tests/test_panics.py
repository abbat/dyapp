"""Clippy panic budgets fail on growth, missing builds and hidden packages."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_panics", ROOT / "scripts/check-panics.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def message(path="rust/ffi/src/lib.rs", package="ffi"):
    return {"reason": "compiler-message", "package_id": package,
            "message": {"code": {"code": "clippy::unwrap_used"}, "spans": [
                {"file_name": path, "line_start": 10, "column_start": 2,
                 "is_primary": True}]}}


class PanicBudgets(unittest.TestCase):
    def test_duplicates_dependencies_and_generated_code(self):
        metadata = {"packages": [{"id": "ffi", "name": "dyapp-ffi"}],
                    "workspace_members": ["ffi"],
                    "target_directory": str(ROOT / "target")}
        rows = [message(), message(), message(package="dependency"),
                message("target/debug/build/generated.rs"),
                {"reason": "build-finished", "success": True}]
        counts, locations, excluded = CHECK.count(metadata, rows)
        self.assertEqual(counts, {"dyapp-ffi": 1})
        self.assertEqual(len(locations), 1)
        self.assertEqual(excluded, {"dyapp-ffi": 1})
        self.assertIn("> 0", CHECK.errors(counts, {"dyapp-ffi": 0})[0])

    def test_incomplete_or_failed_clippy_is_not_zero(self):
        metadata = {"packages": [{"id": "ffi", "name": "dyapp-ffi"}],
                    "workspace_members": ["ffi"],
                    "target_directory": str(ROOT / "target")}
        for rows in [[], [{"reason": "build-finished", "success": False}]]:
            with self.assertRaisesRegex(ValueError, "successful Clippy"):
                CHECK.count(metadata, rows)

    def test_ratchet_and_ffi_zero(self):
        counts = {"dyapp-ffi": 0, "other": 2}
        self.assertEqual(CHECK.errors(counts, counts), [])
        budget = deepcopy(counts)
        budget["other"] = 1
        self.assertIn("> 1", CHECK.errors(counts, budget)[0])
        budget["other"] = 3
        self.assertIn("lower", CHECK.errors(counts, budget)[0])
        budget["dyapp-ffi"] = 1
        self.assertIn("must stay zero", CHECK.errors(counts, budget)[0])
        del budget["other"]
        self.assertIn("missing", " ".join(CHECK.errors(counts, budget)))


if __name__ == "__main__":
    unittest.main()
