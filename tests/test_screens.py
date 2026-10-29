"""Screen pages in docs/design list only IDs their prototype contains."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_screens", ROOT / "tests/scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
PAGE, PROTOTYPE = next(iter(CHECK.SCREENS.items()))


class Screens(unittest.TestCase):
    def test_unknown_id_fails(self):
        texts = {
            PAGE: "`nav-profile`, `wizard-identity` and `gone`, `gone`\n",
            PROTOTYPE: '<button id="nav-profile"></button>'
                       '<section data-on="wizard-identity menu"></section>',
        }
        self.assertEqual(CHECK.screen_errors(texts),
                         [f"{PAGE}: `gone` not in {PROTOTYPE}"])

    def test_repository_screens_match(self):
        texts = {name: (ROOT / name).read_text()
                 for pair in CHECK.SCREENS.items() for name in pair}
        self.assertEqual(CHECK.screen_errors(texts), [])


if __name__ == "__main__":
    unittest.main()
