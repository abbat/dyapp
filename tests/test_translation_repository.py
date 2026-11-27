"""Canonical imports must retain real native values and key inventories."""

import json
from pathlib import PurePosixPath
import unittest
from xml.etree import ElementTree

from test_translation_renderers import MODULE, PLAN
from test_translation_sources import LOAD, ROOT


class TranslationRepository(unittest.TestCase):
    def test_canonical_sources_preserve_existing_native_catalogues(
        self,
    ) -> None:
        catalogue = LOAD(ROOT)
        outputs = PLAN(catalogue, ["android", "desktop", "apple"])
        apple = MODULE["APPLE"]
        self.assertEqual(
            json.loads(outputs[apple]),
            json.loads((ROOT / str(apple)).read_bytes()),
        )
        for tag, qualifier in catalogue.languages.items():
            with self.subTest(language=tag):
                desktop = PurePosixPath(f"clients/desktop/ui/i18n/{tag}.json")
                self.assertEqual(
                    json.loads(outputs[desktop]),
                    json.loads((ROOT / str(desktop)).read_bytes()),
                )
                directory = (
                    PurePosixPath("clients/android/app/src/main/res")
                    / qualifier
                )
                native = [
                    item.attrib["name"]
                    for path in (ROOT / str(directory)).glob("*.xml")
                    for item in ElementTree.parse(path).getroot()
                    if item.tag == "string"
                ]
                generated = [
                    item.attrib["name"]
                    for item in ElementTree.fromstring(
                        outputs[directory / "strings.xml"]
                    )
                ]
                self.assertEqual(len(native), len(set(native)))
                self.assertCountEqual(generated, native)


if __name__ == "__main__":
    unittest.main()
