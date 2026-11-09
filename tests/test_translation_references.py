"""Catalogue reference checks and finite native key domains."""

import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_translations",
    ROOT / "tests/scripts/repository_translations.py",
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
SCALES = """
scale!(Gender, 36, false, [Woman = 1, Man = 2]);
scale!(Education, 42, true, [School = 10]);
pub const SCALES: &[Scale] = &[entry!(Gender, "gender")];
let unused_test_entry = entry!(Education, "education");
"""


def fixture(keys: list[str], source: str) -> dict[str, str]:
    return {
        CHECK.APPLE: json.dumps({"strings": dict.fromkeys(keys, {})}),
        CHECK.SCALES: SCALES,
        "clients/ios/DYApp/View.swift": source,
    }


class TranslationReferences(unittest.TestCase):
    def test_new_unused_key_fails_but_literal_use_passes(self) -> None:
        texts = fixture(["used", "orphan"], 'Text("used")')
        self.assertEqual(
            CHECK.unused_apple_errors(texts),
            [CHECK.APPLE + ": unused translation key orphan"],
        )
        texts["clients/macos/src/View.swift"] = 'Text("orphan")'
        self.assertEqual(CHECK.unused_apple_errors(texts), [])

    def test_comments_and_tests_do_not_keep_keys(self) -> None:
        source = '// Text("unused")\n/* outer /* nested */ "unused" */'
        texts = fixture(["unused"], source)
        texts["clients/ios/Tests/View.swift"] = 'Text("unused")'
        self.assertEqual(len(CHECK.unused_apple_errors(texts)), 1)

    def test_only_registered_scale_values_are_referenced(self) -> None:
        source = (
            'LocalizedStringKey("profile.field." + scale.key)\n'
            'LocalizedStringKey("profile.value." + scale.key'
            ' + "." + choice.key)'
        )
        keys = [
            "profile.field.gender",
            "profile.value.gender.Man",
            "profile.value.gender.Woman",
            "profile.value.gender.Unknown",
            "profile.field.education",
        ]
        self.assertEqual(
            CHECK.unused_apple_errors(fixture(keys, source)),
            [
                CHECK.APPLE
                + ": unused translation key profile.field.education",
                CHECK.APPLE + ": unused translation key "
                "profile.value.gender.Unknown",
            ],
        )

    def test_removed_dynamic_lookup_does_not_keep_metadata_keys(self) -> None:
        texts = fixture(["profile.field.gender"], 'Text("other")')
        self.assertEqual(len(CHECK.unused_apple_errors(texts)), 1)

    def test_publication_keys_come_from_native_contract(self) -> None:
        texts = fixture(
            [
                "profile.publishBlocked.underage",
                "profile.publishBlocked.unused",
            ],
            'LocalizedStringKey("profile.publishBlocked." + code)',
        )
        texts[CHECK.PUBLISH] = 'Block::Underage => "underage",'
        self.assertEqual(
            CHECK.unused_apple_errors(texts),
            [
                CHECK.APPLE + ": unused translation key "
                "profile.publishBlocked.unused",
            ],
        )
        texts[CHECK.PUBLISH] = ""
        with self.assertRaisesRegex(ValueError, "publication block"):
            CHECK.unused_apple_errors(texts)

    def test_unterminated_block_comment_fails(self) -> None:
        with self.assertRaisesRegex(ValueError, "Unterminated"):
            CHECK.unused_apple_errors(fixture(["unused"], '/* "unused"'))

    def test_malformed_scale_metadata_fails_closed(self) -> None:
        for source in (
            "",
            SCALES.replace("Woman = 1", "Woman = bad"),
            SCALES.replace('entry!(Gender, "gender")', "other()"),
        ):
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    CHECK.scale_options(source)


if __name__ == "__main__":
    unittest.main()
