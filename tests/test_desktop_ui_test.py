"""ui-test.js is evaluated in the app page, so it must not redeclare app globals."""
from pathlib import Path
import re
import unittest

UI = Path(__file__).resolve().parents[1] / "clients/desktop"
TOP = re.compile(r"^(?:const|let|class|function\*?|async function) ([\w$]+)", re.M)


class GlobalsTest(unittest.TestCase):
    def test_ui_test_names_do_not_shadow_app_globals(self):
        app = {name for path in (UI / "ui").glob("*.js")
               for name in TOP.findall(path.read_text())}
        suite = set(TOP.findall((UI / "ui-test.js").read_text()))
        # A clash is a SyntaxError on eval: the suite never starts and times out.
        self.assertEqual(sorted(app & suite), [])


if __name__ == "__main__":
    unittest.main()
