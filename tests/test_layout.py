"""Prototype layout criteria: gap findings and the settings budgets."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    assert spec is not None and spec.loader is not None, (
        "Python source loader is unavailable"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


LAYOUT = load("check_layout", "tests/scripts/check-layout.py")
REPOSITORY = load("repository_settings", "tests/scripts/check-repository.py")


def gap(
    a,
    b,
    px,
    framed: bool = True,
    identifier: bool = False,
    label: bool = False,
):
    return {
        "scope": "#s",
        "a": a,
        "b": b,
        "gap": px,
        "framed": framed,
        "identifier": identifier,
        "label": label,
    }


class Layout(unittest.TestCase):
    def test_rules_and_exceptions(self) -> None:
        layout = {
            "one-en-440": [
                gap("div.actions", "p", 0),
                gap("label", "input", 2, label=True),
                gap("p", "p", 0, framed=False),
                gap("div.actions", "p.ok", 8),
                gap("code.full-id", "p", 8, framed=False, identifier=True),
                gap("code.full-id", "p.ok", 12, identifier=True),
            ]
        }
        self.assertEqual(
            LAYOUT.findings(layout),
            {
                "L2 #s div.actions | p": ["one-en-440 (0 px)"],
                "I1 #s code.full-id | p": ["one-en-440 (8 px)"],
            },
        )

    def test_new_findings_fail_and_fixed_ones_are_stale(self) -> None:
        found = {"L2 #s a | b": ["one (0 px)"]}
        self.assertEqual(LAYOUT.errors(found, ["L2 #s a | b"]), ([], []))
        self.assertEqual(
            LAYOUT.errors(found, ["I1 #s c | d"]),
            (["L2 #s a | b: one (0 px)"], ["I1 #s c | d"]),
        )


class Settings(unittest.TestCase):
    def test_counts_must_match_budget(self) -> None:
        texts = {
            prototype: '<section id="other"><li></section>'
            '<section id="settings"><input type="checkbox">'
            '<li><div class="list-item"></section>'
            for prototype in REPOSITORY.SCREENS.values()
        }
        exact = {"checkboxes": 1, "rows": 2}
        budgets = {"desktop": exact, "android": exact}
        self.assertEqual(REPOSITORY.settings_errors(texts, budgets), [])
        budgets["android"] = {"checkboxes": 0, "rows": 3}
        android = REPOSITORY.SCREENS["docs/design/mobile-screens.md"]
        self.assertEqual(
            REPOSITORY.settings_errors(texts, budgets),
            [
                f"{android}: 1 settings checkboxes, more than the budget 0",
                f"{android}: 2 settings rows, fewer: lower the budget 3",
            ],
        )


if __name__ == "__main__":
    unittest.main()
