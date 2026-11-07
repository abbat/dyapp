"""The smoke suite runs in the app page without redeclaring its globals."""
from html.parser import HTMLParser
from pathlib import Path
import re
import unittest

UI = Path(__file__).resolve().parents[1] / "clients/desktop"
TOP = re.compile(
    r"^(?:const|let|class|function\*?|async function) ([\w$]+)", re.M)


class Navigation(HTMLParser):
    def __init__(self):
        super().__init__()
        self.targets = set()
        self.sections = set()

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag == "button" and "data-area" in attrs:
            self.targets.add(attrs["data-area"])
        if tag == "section" and "id" in attrs:
            self.sections.add(attrs["id"])


def area_calls(source):
    return re.findall(r"""\bopenArea\(\s*['"]([^'"]+)['"]""", source)


class NavigationTest(unittest.TestCase):
    def test_smoke_targets_are_real_sidebar_destinations(self):
        navigation = Navigation()
        navigation.feed((UI / "ui/index.html").read_text())
        self.assertTrue(navigation.targets)
        self.assertLessEqual(navigation.targets, navigation.sections)
        calls = [(path.name, target) for path in (UI / "ui-tests").glob("*.js")
                 for target in area_calls(path.read_text())]
        self.assertTrue(calls)
        for path, target in calls:
            with self.subTest(path=path, target=target):
                self.assertIn(target, navigation.targets)

    def test_calls_preserve_a_wrong_target_for_rejection(self):
        self.assertEqual(area_calls(
            "await openArea('chats'); await openArea('chats-area');"),
            ["chats", "chats-area"])


class SmokeInventoryTest(unittest.TestCase):
    def test_all_smoke_scripts_are_registered_and_includes_exist(self):
        directory = UI / "ui-tests"
        includes = re.findall(r'include_str!\("([^"]+)"\)',
                              (directory / "mod.rs").read_text())
        self.assertTrue(includes)
        for name in includes:
            self.assertTrue((directory / name).is_file(), name)
        self.assertEqual(set(directory.glob("*.js")),
                         {directory / name for name in includes
                          if "/" not in name})
        self.assertFalse(list(UI.glob("ui-test*.js")))


class GlobalsTest(unittest.TestCase):
    def test_ui_test_names_do_not_shadow_app_globals(self):
        app = {name for path in (UI / "ui").glob("*.js")
               for name in TOP.findall(path.read_text())}
        suite = set(TOP.findall((UI / "ui-tests/main.js").read_text()))
        # A clash is a SyntaxError on eval: the suite never starts.
        self.assertEqual(sorted(app & suite), [])


if __name__ == "__main__":
    unittest.main()
