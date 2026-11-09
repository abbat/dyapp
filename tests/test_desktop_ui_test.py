"""Module entry and the isolated native smoke scope stay connected."""
from collections import Counter
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


class ModuleScripts(HTMLParser):
    def __init__(self):
        super().__init__()
        self.scripts = []

    def handle_starttag(self, tag, attrs):
        if tag == "script":
            self.scripts.append(dict(attrs))

    def external_modules(self):
        return bool(self.scripts) and all(
            script.get("type") == "module" and script.get("src")
            for script in self.scripts)


class ModuleEntryTest(unittest.TestCase):
    def test_shipped_page_loads_external_modules(self):
        page = ModuleScripts()
        page.feed((UI / "ui/index.html").read_text())
        self.assertTrue(page.external_modules())
        for script in page.scripts:
            self.assertTrue((UI / "ui" / script["src"]).is_file())

    def test_classic_and_inline_scripts_are_rejected(self):
        for source in ('<script src="app.js"></script>',
                       '<script type="module">run()</script>',
                       '<script type="module" src="entry.js"></script>'
                       '<script src="legacy.js"></script>', ''):
            page = ModuleScripts()
            page.feed(source)
            self.assertFalse(page.external_modules())


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


class SmokeScopeTest(unittest.TestCase):
    def test_concatenated_helpers_have_unique_top_level_bindings(self):
        directory = UI / "ui-tests"
        includes = re.findall(r'include_str!\("([^"]+)"\)',
                              (directory / "mod.rs").read_text())
        names = Counter(name for path in includes
                        for name in TOP.findall(
                            (directory / path).read_text()))
        self.assertTrue(names)
        self.assertEqual(sorted(name for name, count in names.items()
                                if count > 1), [])


if __name__ == "__main__":
    unittest.main()
