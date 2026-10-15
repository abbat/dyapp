"""Pure parts of ci-report-import.py: run choice, failures, excerpt, dedup."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "ci_report_import", ROOT / "scripts/ci-report-import.py")
IMPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(IMPORT)


def job(name, conclusion, step="build"):
    return {"name": name, "conclusion": conclusion,
            "steps": [{"name": "setup", "conclusion": "success"},
                      {"name": step, "conclusion": conclusion}]}


class Import(unittest.TestCase):
    def test_latest(self):
        runs = [{"workflowName": "ci", "databaseId": 3,
                 "status": "in_progress"},
                {"workflowName": "ci", "databaseId": 2, "status": "completed"},
                {"workflowName": "ci", "databaseId": 1, "status": "completed"},
                {"workflowName": "sec", "databaseId": 0,
                 "status": "completed"}]
        self.assertEqual(IMPORT.latest(runs), [2, 0])

    def test_failures(self):
        view = {"workflowName": "ci",
                "jobs": [job("core", "failure", "deny"),
                         job("ui", "success"), job("required", "failure")]}
        self.assertEqual(IMPORT.failures(view), [
            ("ci:ci/core", "CI ci / core fails: deny", "core", "deny")])
        view["jobs"] = [job("required", "failure")]
        self.assertEqual([f[0] for f in IMPORT.failures(view)],
                         ["ci:ci/required"])

    def test_excerpt(self):
        log = "\n".join(f"{name}\tUNKNOWN STEP\t2026-10-14T20:03:54.1Z {text}"
                        for name, text in [
                            ("core", "^[[36;1m  printf '::error::%s'^[[0m"),
                            ("core", "  CACHE_ON_FAILURE: false"),
                            ("core", "test a::b ... ok"),
                            ("core", "test result: ok. 3 passed; 0 failed"),
                            ("core", "\x1b[31merror[vulnerability]\x1b[0m: x"),
                            ("ui", "error: other job")])
        self.assertEqual(IMPORT.excerpt(log, "core"),
                         "error[vulnerability]: x")
        self.assertEqual(IMPORT.excerpt(log, "ui"), "error: other job")
        quiet = "core\ts\t2026-10-14T20:03:54Z done"
        self.assertEqual(IMPORT.excerpt(quiet, "core"), "done")

    def test_action(self):
        bead = {"status": "open", "description": "Run 7 (push, x): u",
                "notes": ""}
        self.assertEqual(IMPORT.action([], 7), "create")
        self.assertEqual(IMPORT.action([bead], 7), "skip")
        self.assertEqual(IMPORT.action([bead], 8), "note")
        bead.update(status="closed", notes="Run 8 (push, y): u")
        self.assertEqual(IMPORT.action([bead], 8), "skip")
        self.assertEqual(IMPORT.action([bead], 9), "reopen")


if __name__ == "__main__":
    unittest.main()
