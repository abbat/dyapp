"""Flow metrics keep empty samples and temporal boundaries explicit."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "flow_metrics", ROOT / "scripts/flow_metrics.py")
FLOW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(FLOW)


def run(sha, created, finished, conclusion, attempt=1):
    return {"head_sha": sha, "created_at": created, "completed_at": finished,
            "conclusion": conclusion, "attempt": attempt, "jobs": [],
            "workflow": ".github/workflows/ci.yml"}


def snapshot():
    return {"schema": 1, "now": "2026-10-15T12:00:00Z",
            "beads": [], "commits": [], "practices": [],
            "reviews": [], "observations": [],
            "runs": [run("base", "2026-10-01T10:00:00Z",
                         "2026-10-01T11:00:00Z", "success")]}


class Metrics(unittest.TestCase):
    def test_empty_samples_are_unknown_and_exact_boundaries_pass(self):
        data = snapshot()
        data["beads"] = [{"status": "in_progress", "assignee": "agent",
                          "issue_type": "task"} for _ in range(3)]
        data["runs"].append(run("bad", "2026-10-14T11:00:00Z",
                                "2026-10-14T12:00:00Z", "failure"))
        result = FLOW.report(data)
        self.assertEqual(result["errors"], [])
        self.assertEqual(result["ci"]["current_red_hours"], 24)
        self.assertIsNone(result["lead_hours_p50"])
        self.assertIsNone(result["revert_percent"])
        self.assertIsNone(result["false_positive_share"])

    def test_failed_case_wip_and_carry_in_red(self):
        data = snapshot()
        data["beads"] = [{"status": "in_progress", "assignee": "agent",
                          "issue_type": "task"} for _ in range(4)]
        data["runs"].append(run("bad", "2026-10-01T11:00:00Z",
                                "2026-10-01T12:00:00Z", "failure"))
        result = FLOW.report(data)
        self.assertEqual(len(result["errors"]), 2)
        self.assertEqual(result["ci"]["current_red_hours"], 14 * 24)

    def test_stale_green_does_not_clear_new_red_and_rerun_recovers(self):
        data = snapshot()
        data["runs"] += [
            run("bad", "2026-10-14T10:00:00Z",
                "2026-10-14T11:00:00Z", "failure"),
            run("old", "2026-10-13T10:00:00Z",
                "2026-10-14T12:00:00Z", "success"),
            run("bad", "2026-10-14T10:00:00Z",
                "2026-10-15T11:00:00Z", "success", attempt=2)]
        result = FLOW.report(data)["ci"]
        self.assertEqual(result["current_red_hours"], 0)
        self.assertEqual(result["max_red_hours"], 24)
        self.assertEqual(result["time_to_green_hours_p50"], 24)
        self.assertEqual(result["red_commit_percent"], 50)

    def test_weekly_cohort_hatches_reviews_and_job_minutes(self):
        data = snapshot()
        data["beads"] = [
            {"status": "closed", "issue_type": "task",
             "created_at": "2026-10-10T00:00:00Z",
             "closed_at": f"2026-10-{day}T00:00:00Z"}
            for day in [11, 12, 14]]
        data["commits"] = [{
            "at": "2026-10-10T00:00:00Z",
            "message": "Revert x\n\nThis reverts commit abc.",
            "trailers": "Large-Change: Atomic\nDocs-Unchanged: Not an API",
            "annotations": {"mutants::skip": 1}}]
        data["observations"] = [
            {"check": "docs-sync", "at": "2026-10-10T00:00:00Z",
             "false_positive": false} for false in [True, False]]
        data["practices"] = [
            {"id": "A4", "adopted_at": "2026-09-01T00:00:00Z"}]
        data["runs"][0]["jobs"] = [
            {"started_at": "2026-10-08T11:59:00Z",
             "completed_at": "2026-10-08T12:01:00Z"},
            {"started_at": "2026-10-15T11:59:00Z", "completed_at": None}]
        result = FLOW.report(data)
        self.assertEqual(result["throughput"], 3)
        self.assertEqual(result["lead_hours_p50"], 48)
        self.assertEqual(result["lead_hours_p85"], 96)
        self.assertEqual(result["ci"]["job_minutes"], 2)
        self.assertEqual(result["hatch_commits"], 1)
        self.assertEqual(result["revert_percent"], 100)
        share = result["false_positive_share"]["docs-sync"]
        self.assertEqual(share["percent"], 50)
        self.assertTrue(result["practice_reviews"][0]["overdue"])
        invalid = deepcopy(data)
        invalid["observations"][0]["false_positive"] = "false"
        with self.assertRaises(ValueError):
            FLOW.report(invalid)

    def test_invalid_sources_fail(self):
        data = snapshot()
        data["runs"] = []
        with self.assertRaisesRegex(ValueError, "No canonical"):
            FLOW.report(data)
        with self.assertRaisesRegex(ValueError, "timezone"):
            FLOW.instant("2026-10-15")


if __name__ == "__main__":
    unittest.main()
