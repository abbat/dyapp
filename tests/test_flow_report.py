"""Read-only flow collection rejects incomplete data and CLI failures."""
import importlib.util
import io
import json
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

from test_flow_metrics import FLOW, snapshot

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "flow_report", ROOT / "tools/scripts/flow-report.py")
CLI = importlib.util.module_from_spec(SPEC)
with patch.dict(sys.modules, {"flow_metrics": FLOW}):
    SPEC.loader.exec_module(CLI)


class Collection(unittest.TestCase):
    def test_pagination_must_be_complete(self):
        page = [{"total_count": 2, "jobs": [{"id": 1}]}]
        with patch.object(CLI, "command", return_value=json.dumps(page)):
            with self.assertRaisesRegex(ValueError, "Incomplete"):
                CLI.pages("endpoint", "jobs")

    def test_remote_failure_is_retried_twice(self):
        failed = SimpleNamespace(returncode=1, stderr="unavailable")
        with patch.object(CLI.subprocess, "run", return_value=failed) as call:
            with self.assertRaisesRegex(RuntimeError, "unavailable"):
                CLI.command("gh", "api", "endpoint")
            self.assertEqual(call.call_count, 3)

    def test_annotations_and_adoption_date(self):
        fields = "abc\0date\0message\0Large-Change: Atomic\0\n"
        diff = ('+++ b/file.rs\n+#[mutants::skip]\n'
                '+// gitleaks:allow test fixture\n+"mutants::skip"\n')
        with patch.object(CLI, "git", side_effect=[fields, diff]):
            result = CLI.commits("date")
        self.assertEqual(result[0]["annotations"],
                         {"mutants::skip": 1, "gitleaks:allow": 1})
        self.assertEqual(result[0]["trailers"], "Large-Change: Atomic")
        history = ("\0old\n+| A4 (adopted) | first\n"
                   "\0new\n+| A4 (adopted) | edit\n")
        with patch.object(CLI, "git", return_value=history):
            self.assertEqual(CLI.adoption_dates(),
                             [{"id": "A4", "adopted_at": "old"}])

    def test_cli_threshold_failure_and_empty_sample_success(self):
        data = snapshot()
        for count, expected in [(4, True), (0, False)]:
            data["beads"] = [{"status": "in_progress", "assignee": "agent",
                              "issue_type": "task"} for _ in range(count)]
            with patch.object(sys, "argv", ["report", "--snapshot", "input"]):
                with patch.object(Path, "read_text",
                                  return_value=json.dumps(data)):
                    with patch("sys.stdout", new_callable=io.StringIO) as out:
                        self.assertEqual(CLI.main(), expected)
                    result = json.loads(out.getvalue())
                    self.assertEqual(bool(result["errors"]), expected)

    def test_cli_health_exit_codes(self):
        data = {"schema": 1, "now": "2026-10-15T12:00:00Z",
                "runs": snapshot()["runs"]}
        bad = dict(data["runs"][0], head_sha="bad", conclusion="failure",
                   created_at="2026-10-15T10:00:00Z",
                   completed_at="2026-10-15T11:00:00Z")
        for runs, code in [([], 0), ([bad], 2)]:
            data["runs"] = snapshot()["runs"] + runs
            with patch.object(sys, "argv",
                              ["report", "--health", "--snapshot", "input"]):
                with patch.object(Path, "read_text",
                                  return_value=json.dumps(data)):
                    with patch("sys.stdout", new_callable=io.StringIO):
                        self.assertEqual(CLI.main(), code)


if __name__ == "__main__":
    unittest.main()
