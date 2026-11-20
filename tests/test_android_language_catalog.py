"""Check own Android translations against the language registry."""

from pathlib import Path
import re
import unittest
from xml.etree import ElementTree


ROOT = Path(__file__).resolve().parents[1]
RESOURCES = ROOT / "clients/android/app/src/main/res"
NAMESPACE = "{http://schemas.android.com/apk/res/android}"
LOCALE = re.compile(
    r"values-(?:b\+([A-Za-z0-9+]+)|"
    r"([a-z]{2,3})(?:-r([A-Z]{2}|[0-9]{3}))?)(?:-|$)"
)


def locale_tag(folder: str) -> str | None:
    """Read the locale prefix, not UI/night/density qualifiers."""
    found = LOCALE.match(folder)
    if not found:
        return None
    bcp47, language, region = found.groups()
    if bcp47:
        return bcp47.replace("+", "-")
    # Android reserves this legacy qualifier for car UI; b+car is unambiguous.
    if language == "car":
        return None
    return language + ("-" + region if region else "")


class AndroidLanguageCatalogTests(unittest.TestCase):
    def test_own_packaged_translations_match_registered_languages(
        self,
    ) -> None:
        document = ElementTree.parse(RESOURCES / "xml/app_languages.xml")
        self.assertEqual(document.getroot().tag, "locale-config")
        tags = [
            item.attrib[NAMESPACE + "name"]
            for item in document.getroot().findall("locale")
        ]
        self.assertTrue(tags, "Language registry must not be empty")
        self.assertEqual(len(tags), len(set(tags)), "Duplicate language tags")
        # Unqualified app resources are English; do not scan libraries.
        actual = {"en"}
        for folder in RESOURCES.iterdir():
            tag = locale_tag(folder.name)
            if tag and any(
                item.tag in {"string", "plurals", "string-array"}
                for resource in folder.glob("*.xml")
                for item in ElementTree.parse(resource).getroot()
            ):
                actual.add(tag)
        self.assertEqual(set(tags), actual, "Register every own translation")

    def test_locale_qualifiers_preserve_regions_and_scripts(self) -> None:
        for folder, expected in {
            "values-fr": "fr",
            "values-ru-night": "ru",
            "values-en-rGB": "en-GB",
            "values-es-r419": "es-419",
            "values-b+zh+Hans": "zh-Hans",
            "values-b+sr+Latn+RS-night": "sr-Latn-RS",
            "values-b+car": "car",
        }.items():
            with self.subTest(folder=folder):
                self.assertEqual(locale_tag(folder), expected)

    def test_nonlocale_qualifiers_do_not_advertise_translations(self) -> None:
        for folder in (
            "values",
            "values-night",
            "values-land",
            "values-car",
            "values-sw600dp",
            "values-v31",
        ):
            with self.subTest(folder=folder):
                self.assertIsNone(locale_tag(folder))
