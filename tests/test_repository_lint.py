"""Tracker-ID and commit-message checks of check-repository.py."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_repository", ROOT / "scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

ALLOWED = {"dyapp-ffi"}
# Built at run time so this file itself passes the tracker-ID scan.
ID, CHILD = "dyapp-" + "ab1c", "dyapp-" + "x1lu.2"
TRAILER = "\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n"


class Lint(unittest.TestCase):
    def errors(self, message):
        return " ".join(CHECK.message_errors(message, ALLOWED))

    def test_tracker_ids(self):
        text = f"see {ID}, {CHILD} and dyapp-ffi, dyapp-p2p-net"
        self.assertEqual(CHECK.bead_ids(text, ALLOWED), [ID, CHILD])

    def test_good_messages(self):
        for subject in ("Add a check", "ci: Pin the image", "Embed the key",
                        "Process the queue", "Focus on dyapp-ffi"):
            self.assertEqual(self.errors(subject + TRAILER), "", subject)

    def test_bad_messages(self):
        cases = {
            "Added a check": "not imperative",
            "Adds a check": "not imperative",
            "Fixing it": "not imperative",
            "add a check": "capital letter",
            "Add a check.": "period",
            "A" * 73: "longer than 72",
            "Close " + ID: "tracker IDs",
        }
        for subject, error in cases.items():
            self.assertIn(error, self.errors(subject + TRAILER), subject)
        self.assertIn("no Co-Authored-By trailer", self.errors("Add a check"))
        self.assertEqual(CHECK.message_errors("Bump x", ALLOWED, True), [])
        self.assertIn("no blank line",
                      self.errors("Add a check\nbody" + TRAILER))


if __name__ == "__main__":
    unittest.main()
