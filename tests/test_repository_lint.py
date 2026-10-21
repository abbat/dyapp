"""Tracker-ID and commit-message checks of check-repository.py."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_repository", ROOT / "tests/scripts/check-repository.py")
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

    def test_orphans(self):
        documents = {"docs/README.md": "[a](a/README.md)\n[x](https://x)\n",
                     "docs/a/README.md": "[b](../b.md#top)\n",
                     "docs/b.md": "", "docs/c.md": "[b](b.md)\n"}
        files = list(documents) + ["docs/img.png", "README.md"]
        self.assertEqual(CHECK.orphans(documents, files),
                         ["docs/c.md", "docs/img.png"])

    def test_size_budgets(self):
        size = {"max_lines": 2, "files": {"big.rs": 3, "gone.rs": 5}}
        texts = {"big.rs": "\n" * 3, "a.py": "\n" * 2, "doc.md": "\n" * 9}
        self.assertEqual(CHECK.size_errors(texts, size),
                         ["gone.rs: budgeted but not tracked"])
        texts.update({"big.rs": "\n" * 4, "a.py": "\n" * 3})
        self.assertEqual(CHECK.size_errors(texts, size), [
            "a.py: 3 > 2 lines", "big.rs: 4 > 3 lines",
            "gone.rs: budgeted but not tracked"])
        texts = {"big.rs": "\n", "gone.rs": "\n" * 4}
        self.assertEqual(CHECK.size_errors(texts, size), [
            "big.rs: 1 lines, lower its budget (or drop it)",
            "gone.rs: 4 lines, lower its budget"])


if __name__ == "__main__":
    unittest.main()
