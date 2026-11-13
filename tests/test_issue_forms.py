"""Issue form language copies stay in step."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "issue_forms", ROOT / "tests/scripts/issue_forms.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
DIR = ".github/ISSUE_TEMPLATE/"


def form(lang: str, required: str = "true", field: str = "id: what") -> str:
    return f"""labels: [bug, "lang:{lang}"]
body:
  - type: markdown
    attributes: {{value: {lang}}}
  - type: textarea
    {field}
    validations: {{required: {required}}}
"""


class IssueForms(unittest.TestCase):
    def test_repository_forms_match(self) -> None:
        texts = {
            str(path.relative_to(ROOT)): path.read_text(encoding="utf-8")
            for path in (ROOT / DIR).glob("*.yml")
        }
        self.assertEqual(CHECK.form_errors(texts), [])

    def test_matching_copies_pass(self) -> None:
        texts = {
            DIR + "bug-en.yml": form("en"),
            DIR + "bug-ru.yml": form("ru"),
        }
        self.assertEqual(CHECK.form_errors(texts), [])

    def test_drift_is_reported(self) -> None:
        cases = {
            "differ": form("ru", required="false"),
            "no id": form("ru", field="attributes: {label: x}"),
            "lang:ru": form("en"),
        }
        for error, ru in cases.items():
            texts = {DIR + "bug-en.yml": form("en"), DIR + "bug-ru.yml": ru}
            self.assertTrue(
                any(error in line for line in CHECK.form_errors(texts)), error
            )

    def test_missing_copy_is_reported(self) -> None:
        texts = {DIR + "bug-en.yml": form("en")}
        self.assertEqual(CHECK.form_errors(texts), ["bug: no ru copy"])


if __name__ == "__main__":
    unittest.main()
