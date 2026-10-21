"""Quorum proof binds the range and distinct closed participants."""
from copy import deepcopy
import importlib.util
import io
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

import test_triggers

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "quorum_check", ROOT / "tests/scripts/quorum-check.py")
CHECK = importlib.util.module_from_spec(SPEC)
with patch.dict(sys.modules, {"triggers": test_triggers.CHECK}):
    SPEC.loader.exec_module(CHECK)
BASE, HEAD = "a" * 40, "b" * 40


def proof():
    quorum = {"issue_type": "epic", "status": "closed", "labels": ["quorum"],
              "description": f"Snapshot: {HEAD}\nDiff base: {BASE}\n"}
    members = [{"issue_type": "epic", "status": "closed", "labels": ["quorum"],
                "assignee": name} for name in ["model-a", "model-b"]]
    return quorum, members


class Quorum(unittest.TestCase):
    def test_trigger_classification(self):
        self.assertFalse(CHECK.needs_review([], False, False))
        self.assertTrue(CHECK.needs_review([], True, True))
        self.assertTrue(CHECK.needs_review(["checks: a.py"], False, False))
        self.assertFalse(CHECK.needs_review(["checks: a.py"], True, False))
        self.assertTrue(CHECK.needs_review(["protocol: x.proto"], True, False))
        self.assertTrue(CHECK.needs_review(["checks: a.py"], True, True))

    def test_proof_is_closed_and_bound_to_both_commits(self):
        quorum, members = proof()
        self.assertEqual(CHECK.proof_errors(quorum, members, BASE, HEAD), [])
        for key, value in [("status", "open"), ("labels", []),
                           ("description", f"Snapshot: {BASE}")]:
            changed = deepcopy(quorum)
            changed[key] = value
            self.assertTrue(CHECK.proof_errors(changed, members, BASE, HEAD))
        self.assertTrue(CHECK.proof_errors(quorum, members, HEAD, HEAD))

    def test_distinct_participants_and_withdrawal(self):
        quorum, members = proof()
        members[1]["assignee"] = "model-a"
        self.assertTrue(CHECK.proof_errors(quorum, members, BASE, HEAD))
        quorum, members = proof()
        members[1]["close_reason"] = "withdrawn: unavailable"
        self.assertTrue(CHECK.proof_errors(quorum, members, BASE, HEAD))

    def test_cli_rejects_missing_quorum_and_passes_explicit_tightening(self):
        policy = '[[trigger]]\nname="checks"\npaths=["scripts/*"]\n'
        diff = "diff --git a/scripts/a.py b/scripts/a.py\n@@ -0,0 +1 @@\n+x\n"
        for extra, expected in [([], True), (["--checks-tightening"], False)]:
            outputs = ["", BASE, HEAD, "", diff, policy, policy, ""]
            with patch.object(CHECK, "git", side_effect=outputs):
                argv = ["check", "--base", BASE, *extra]
                with patch.object(sys, "argv", argv):
                    with patch("sys.stdout", new_callable=io.StringIO):
                        self.assertEqual(CHECK.main(), expected)


if __name__ == "__main__":
    unittest.main()
