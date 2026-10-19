"""A1 Definition of Ready over `bd show --json` records."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "bead_ready", ROOT / "scripts/bead-ready.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
READY = {"id": "x-1", "issue_type": "task", "description": "Fix the parser.",
         "acceptance_criteria": "A truncated frame is rejected.",
         "notes": "Docs: none\nVerify: make test"}


class BeadReady(unittest.TestCase):
    def test_ready_bead_and_non_work_types_pass(self):
        self.assertEqual(CHECK.errors(READY), [])
        self.assertEqual(CHECK.errors({"id": "x-2", "issue_type": "epic"}),
                         [])

    def test_each_missing_part_is_reported(self):
        bead = dict(READY, description="Docs: none\n",
                    acceptance_criteria=" ", notes="Verify:\n")
        self.assertEqual(CHECK.errors(bead), [
            "x-1: no goal (empty description)",
            "x-1: no acceptance criterion",
            "x-1: no Verify: line",
        ])


if __name__ == "__main__":
    unittest.main()
