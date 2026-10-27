"""A noqa needs codes and a reason, and fails once flake8 no longer needs
it."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_noqa", ROOT / "tests/scripts/check-noqa.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class Noqa(unittest.TestCase):
    def test_needed_unused_and_malformed(self):
        sources = {"a.py": "import os  # noqa: F401 -- re-export\n"
                           "import sys  # noqa: F401,E402 -- shim\n"
                           "x = 1  # noqa\n"
                           "y = 2  # noqa: E501\n"}
        reported = {("a.py", 1, "F401"), ("a.py", 2, "F401")}
        self.assertEqual(CHECK.errors(sources, reported), [
            "a.py:2: unused noqa E402",
            "a.py:3: write `# noqa: CODE -- reason`",
            "a.py:4: write `# noqa: CODE -- reason`"])

    def test_flake8_reports_suppressed_codes(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "m.py"
            path.write_text("import os  # noqa: F401 -- test\n"
                            "x = 1  # noqa: E501 -- stale\n")
            sources = {str(path): path.read_text()}
            self.assertEqual(
                CHECK.errors(sources, CHECK.flake8([str(path)])),
                [f"{path}:2: unused noqa E501"])


if __name__ == "__main__":
    unittest.main()
