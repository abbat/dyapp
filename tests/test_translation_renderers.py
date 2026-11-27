"""Native catalogue byte contracts, not SDK/runtime acceptance."""

import json
from pathlib import Path, PurePosixPath
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch
from xml.etree import ElementTree

from test_translation_sources import LOAD, ROOT, prepare

with patch.object(sys, "path", [str(ROOT / "tools/scripts"), *sys.path]):
    MODULE = runpy.run_path(
        str(ROOT / "tools/scripts/translation_renderers.py")
    )
PLAN = MODULE["plan"]
ANDROID = MODULE["android"]
APPLE = MODULE["apple"]


class TranslationRenderers(unittest.TestCase):
    def test_all_native_outputs_keep_configured_languages_and_argument_types(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prepare(root)
            catalogue = LOAD(root)
            before = set(root.rglob("*"))
            outputs = PLAN(catalogue, ["desktop", "android", "apple"])
            self.assertEqual(set(root.rglob("*")), before)
            self.assertEqual(len(outputs), 7)
            for tag, qualifier in catalogue.languages.items():
                xml = outputs[
                    PurePosixPath("clients/android/app/src/main/res")
                    / qualifier
                    / "strings.xml"
                ]
                strings = ElementTree.fromstring(xml).findall("string")
                self.assertEqual(
                    [(item.attrib, item.text) for item in strings],
                    [({"name": "total"}, '"%1$d entries"')],
                )
                desktop = outputs[
                    PurePosixPath(f"clients/desktop/ui/i18n/{tag}.json")
                ]
                self.assertEqual(
                    json.loads(desktop), {"total": "{number} entries"}
                )
            apple = json.loads(outputs[MODULE["APPLE"]])
            self.assertEqual(apple["sourceLanguage"], "en")
            units = apple["strings"]["total"]["localizations"]
            self.assertEqual(set(units), {"en", "ru", "fr"})
            for value in units.values():
                self.assertEqual(
                    value["stringUnit"],
                    {"state": "translated", "value": "%1$lld entries"},
                )
            self.assertEqual(
                outputs, PLAN(catalogue, ["apple", "android", "desktop"])
            )

    def test_android_aapt_escaping_has_independent_literal_witnesses(
        self,
    ) -> None:
        for text, expected in (
            ("  a  ", '"  a  "'),
            ("a'b", '"a\\\'b"'),
            ('a"b', '"a\\"b"'),
            ("a\\b", '"a\\\\b"'),
            ("\n\t\r", '"\\n\\t\\u000d"'),
            ("@string/other", '"\\@string/other"'),
            ("?attr/other", '"\\?attr/other"'),
            ("#123", '"\\#123"'),
            ("\u0414 <>&", '"\u0414 <>&"'),
        ):
            with self.subTest(text=text):
                output = ANDROID({"text": text})
                value = ElementTree.fromstring(output).find("string")
                assert value is not None
                self.assertEqual(value.text, expected)
                self.assertTrue(output.endswith(b"\n"))
                self.assertNotIn(b"\r", output)
        output = ANDROID({"text": "<>&"})
        self.assertIn(b"&lt;&gt;&amp;", output)

    def test_native_ordering_is_deterministic_and_unicode_is_not_lost(
        self,
    ) -> None:
        first = {"z": "\u0414", "a": "A"}
        second = {"a": "A", "z": "\u0414"}
        self.assertEqual(ANDROID(first), ANDROID(second))
        left = APPLE({"ru": first, "en": second}, "en")
        right = APPLE({"en": first, "ru": second}, "en")
        self.assertEqual(left, right)
        self.assertIn("\u0414".encode(), left)

    def test_empty_selection_unknown_platform_and_incomplete_apple_fail(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prepare(root)
            catalogue = LOAD(root)
            for selection in ([], ["unknown"]):
                with self.assertRaises(ValueError):
                    PLAN(catalogue, selection)
        for locales, default in (
            ({"ru": {"key": "Text"}}, "en"),
            ({"en": {"key": "Text"}, "ru": {}}, "en"),
        ):
            with self.assertRaises(ValueError):
                APPLE(locales, default)


if __name__ == "__main__":
    unittest.main()
