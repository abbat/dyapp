"""Complete source inventories and malformed catalogue refusal."""

from collections.abc import Callable
import json
from pathlib import Path
import runpy
import sys
import tempfile
from typing import Any
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
with patch.object(sys, "path", [str(ROOT / "tools/scripts"), *sys.path]):
    MODULE = runpy.run_path(str(ROOT / "tools/scripts/translation_sources.py"))
LOAD = MODULE["load"]
Mutation = Callable[[dict[str, Any]], None]


def write(path: Path, value: object) -> None:
    path.write_text(json.dumps(value), encoding="utf-8")


def prepare(root: Path) -> Path:
    directory = root / "i18n"
    directory.mkdir()
    binding = {
        "android": {"total": {"count": "%1$d"}},
        "apple": {"total": {"count": "%1$lld"}},
        "desktop": {"total": {"count": "{number}"}},
    }
    write(
        directory / "catalogue.json",
        {
            "schema": 1,
            "default": "en",
            "languages": {
                "en": "values",
                "ru": "values-ru",
                "fr": "values-fr",
            },
            "messages": {
                "common.total": {
                    "arguments": {"count": "integer"},
                    "bindings": binding,
                    "context": "A counted local collection",
                }
            },
        },
    )
    for tag in ("en", "ru", "fr"):
        write(directory / f"{tag}.json", {"common.total": "{count} entries"})
    return directory


class TranslationSources(unittest.TestCase):
    def test_three_languages_and_native_bindings_survive_loading(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            prepare(root)
            value = LOAD(root)
            self.assertEqual(value.default, "en")
            self.assertEqual(set(value.languages), {"en", "ru", "fr"})
            self.assertEqual(set(value.texts), {"en", "ru", "fr"})
            entry = value.messages["common.total"]
            self.assertEqual(entry.arguments, {"count": "integer"})
            self.assertEqual(len(entry.bindings), 3)
            self.assertEqual(entry.bindings[0].tokens, {"count": "%1$d"})

    def test_existing_mixed_case_android_enum_key_is_preserved(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            directory = prepare(root)
            path = directory / "catalogue.json"
            data = json.loads(path.read_text())
            entry = data["messages"]["common.total"]
            key = "profile_value_career_ambition_High"
            entry["bindings"]["android"] = {key: {"count": "%1$d"}}
            write(path, data)
            binding = LOAD(root).messages["common.total"].bindings[0]
            self.assertEqual(binding.key, key)

    def test_bad_metadata_is_refused_before_any_output(self) -> None:
        cases: tuple[tuple[str, Mutation], ...] = (
            ("unknown field", lambda data: data.update(extra=True)),
            ("bool schema", lambda data: data.update(schema=True)),
            ("new schema", lambda data: data.update(schema=2)),
            ("absent default", lambda data: data.update(default="missing")),
            ("empty messages", lambda data: data.update(messages={})),
            (
                "case-only language collision",
                lambda data: data["languages"].update(EN="values-en"),
            ),
            (
                "case-only qualifier collision",
                lambda data: data["languages"].update(fr="values-RU"),
            ),
            (
                "qualifier collision",
                lambda data: data["languages"].update(fr="values-ru"),
            ),
            (
                "traversal",
                lambda data: data["languages"].update(
                    {"../outside": "values-x"}
                ),
            ),
            (
                "qualifier traversal",
                lambda data: data["languages"].update(fr="../outside"),
            ),
            (
                "missing context",
                lambda data: data["messages"]["common.total"].update(
                    context=""
                ),
            ),
            (
                "unknown platform",
                lambda data: data["messages"]["common.total"][
                    "bindings"
                ].update(web={"total": {"count": "{number}"}}),
            ),
            (
                "unknown message field",
                lambda data: data["messages"]["common.total"].update(
                    plurals={}
                ),
            ),
            (
                "native collision",
                lambda data: data["messages"].update(
                    {"another.total": data["messages"]["common.total"]}
                ),
            ),
        )
        for name, change in cases:
            with (
                self.subTest(name=name),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                directory = prepare(root)
                source = directory / "catalogue.json"
                data = json.loads(source.read_text())
                change(data)
                write(source, data)
                before = {p.name: p.read_bytes() for p in directory.iterdir()}
                with self.assertRaises(ValueError):
                    LOAD(root)
                self.assertEqual(
                    {p.name: p.read_bytes() for p in directory.iterdir()},
                    before,
                )

    def test_missing_extra_and_changed_locale_values_are_refused(self) -> None:
        for value in (
            {},
            {"unknown.key": "Text"},
            {"common.total": "Lost arg"},
        ):
            with (
                self.subTest(value=value),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                directory = prepare(root)
                write(directory / "ru.json", value)
                with self.assertRaises(ValueError):
                    LOAD(root)
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            directory = prepare(root)
            write(directory / "unexpected.json", {})
            with self.assertRaisesRegex(ValueError, "source inventory"):
                LOAD(root)

    def test_duplicate_nonfinite_and_oversized_json_are_refused(self) -> None:
        for source, expected in (
            ('{"schema":1,"schema":1}', "Duplicate catalogue key"),
            ('{"schema":NaN}', "Non-finite"),
            (" " * (MODULE["MAX_BYTES"] + 1), "exceeds"),
        ):
            with (
                self.subTest(expected=expected),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                directory = root / "i18n"
                directory.mkdir()
                (directory / "catalogue.json").write_text(source)
                with self.assertRaisesRegex(ValueError, expected):
                    LOAD(root)

    def test_a_registered_locale_symlink_is_not_read(self) -> None:
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            directory = prepare(root)
            path = directory / "catalogue.json"
            data = json.loads(path.read_text())
            data["languages"]["de"] = "values-de"
            write(path, data)
            (directory / "de.json").symlink_to(directory / "en.json")
            with self.assertRaisesRegex(ValueError, "regular file"):
                LOAD(root)


if __name__ == "__main__":
    unittest.main()
