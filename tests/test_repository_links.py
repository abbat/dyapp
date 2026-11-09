"""Markdown link checks of check-repository.py, executed only in Docker."""

import typing

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_repository", ROOT / "tests/scripts/check-repository.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)

TARGET = """# Guide
## `make deb` → **package**
## Setup
## Setup
<a id="custom"></a>
"""


class Links(unittest.TestCase):
    @staticmethod
    def check(text: "str") -> "list[str]":
        documents = {"docs/a.md": text, "docs/b.md": TARGET}
        files = set(documents) | {"docs", "scripts/x.sh"}
        return typing.cast(
            "list[str]", CHECK.broken_links(documents, files.__contains__)
        )

    def test_resolving_links_pass(self) -> None:
        self.assertEqual(
            self.check(
                "[b](b.md) [s](../scripts/x.sh) [d](../docs/) "
                "[c](b.md#make-deb--package) [two](b.md#setup-1) "
                "[id](b.md#custom) [self](#top)\n# Top\n"
                "[ext](https://example.org/x.md) [mail](mailto:a@b.c)\n"
                "[ref]: b.md#guide\n"
            ),
            [],
        )

    def test_missing_file_and_anchor_fail(self) -> None:
        self.assertEqual(
            self.check(
                "[x](c.md)\n[y](b.md#setup-2)\n[z](#nowhere)\n"
                "[up](../../outside.md)\n[ref]: missing.md\n"
            ),
            [
                "docs/a.md:1: missing c.md",
                "docs/a.md:2: missing anchor b.md#setup-2",
                "docs/a.md:3: missing anchor #nowhere",
                "docs/a.md:4: missing ../../outside.md",
                "docs/a.md:5: missing missing.md",
            ],
        )

    def test_check_output_map_links_and_schema(self) -> None:
        documents = {"docs/b.md": TARGET}
        good = {
            "fixture": {
                "fix": "docs/b.md#setup",
                "format": "lines",
                "pattern": ".+",
            }
        }
        self.assertEqual(
            CHECK.output_map_errors(good, documents, documents.__contains__),
            [],
        )
        for fix, pattern in (
            ("docs/b.md#missing", ".+"),
            ("outside.md#setup", ".+"),
            ("docs/b.md#setup", "["),
        ):
            bad = {
                "fixture": {"fix": fix, "format": "lines", "pattern": pattern}
            }
            self.assertTrue(
                CHECK.output_map_errors(bad, documents, documents.__contains__)
            )

    def test_code_is_not_checked(self) -> None:
        self.assertEqual(
            self.check(
                "```\n[x](c.md)\n```\n`[y](c.md)`\n~~~md\n[z](c.md)\n~~~\n"
            ),
            [],
        )


if __name__ == "__main__":
    unittest.main()
