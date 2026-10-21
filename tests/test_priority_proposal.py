"""WSJF estimates stay reproducible and bound to approval."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "priority_proposal", ROOT / "tools/scripts/priority-proposal.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
NOW = "2026-10-15T12:00:00+00:00"


def bead(key, kind="epic", parent=None, status="open"):
    return {"id": key, "title": key, "issue_type": kind, "priority": 1,
            "parent": parent, "status": status}


class Priorities(unittest.TestCase):
    def test_proxy_scores_nested_leaves_and_overrides(self):
        data = [bead("a"), bead("b"), bead("b1", "task", "b"),
                bead("b2", "task", "b"),
                bead("old", "task", "a", "closed"),
                bead("review")]
        data[-1]["labels"] = ["quorum"]
        result = CHECK.proposal(data, NOW)
        self.assertEqual([r["id"] for r in result["order"]], ["a", "b"])
        self.assertEqual(result["order"][1]["job_size"], 2)
        data[1]["metadata"] = {"wsjf": {
            "cost_of_delay": 20, "job_size": 2, "rationale": "Time critical"}}
        self.assertEqual(CHECK.proposal(data, NOW)["order"][0]["id"], "b")
        self.assertEqual(CHECK.proposal(data, NOW)["approval"], "pending")

    def test_practices_first_overrides_cross_group_scores(self):
        data = [bead("ordinary"), bead("practice-low"), bead("practice-high")]
        for item, score in zip(data, [100, 1, 5]):
            item["metadata"] = {"wsjf": {
                "cost_of_delay": score, "job_size": 1,
                "rationale": "Estimate"}}
        data[1]["labels"] = data[2]["labels"] = ["practices"]
        plain = CHECK.proposal(data, NOW)
        grouped = CHECK.proposal(data, NOW, practices_first=True)
        self.assertEqual([b["id"] for b in grouped["order"]],
                         ["practice-high", "practice-low", "ordinary"])
        self.assertEqual(plain["order"][0]["id"], "ordinary")
        plain_hash = plain["proposal_sha256"]
        self.assertNotEqual(plain_hash, grouped["proposal_sha256"])
        changed = deepcopy(grouped)
        changed["policy"] = "wsjf"
        self.assertIn("fingerprint", CHECK.check(changed, None, NOW)[0])

    def test_legacy_proposal_fingerprint_still_validates(self):
        document = CHECK.proposal([bead("a")], NOW)
        document.pop("policy")
        document["proposal_sha256"] = CHECK.fingerprint(document["order"])
        self.assertEqual(CHECK.check(document, None, NOW),
                         ["Human approval is missing"])

    def test_invalid_rating_and_parent_cycle_fail(self):
        for value in [0, -1, True, float("nan"), float("inf"), "1"]:
            with self.assertRaises(ValueError):
                CHECK.positive(value)
        data = [bead("a", parent="b"), bead("b", parent="a")]
        with self.assertRaisesRegex(ValueError, "Cycle"):
            CHECK.proposal(data, NOW)

    def test_expiration_and_approval_binding(self):
        document = CHECK.proposal([bead("a")], NOW)
        approval = {"approved_at": NOW, "approved_by": "Maintainer",
                    "proposal_sha256": document["proposal_sha256"]}
        self.assertEqual(CHECK.check(document, approval, NOW), [])
        self.assertIn("missing", CHECK.check(document, None, NOW)[0])
        future = "2026-10-30T12:00:00+00:00"
        self.assertIn("stale", CHECK.check(document, approval, future)[0])
        exact = "2026-10-29T12:00:00+00:00"
        self.assertEqual(CHECK.check(document, approval, exact), [])
        changed = deepcopy(document)
        changed["order"][0]["cost_of_delay"] = 99
        self.assertIn("fingerprint", CHECK.check(changed, approval, NOW)[0])
        changed = deepcopy(approval)
        changed["proposal_sha256"] = "another"
        errors = CHECK.check(document, changed, NOW)
        self.assertIn("another proposal", errors[0])


if __name__ == "__main__":
    unittest.main()
