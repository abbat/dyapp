"""Native translation domains stay finite and isolated."""
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tests/scripts"))
from desktop_translation_domains import (  # noqa: E402 -- script modules
    documented_errors, function_values, native_domains, typedef_values,
)
from desktop_translations import read_texts  # noqa: E402 -- script modules


class NativeTranslationDomains(unittest.TestCase):
    def test_typedef_values_do_not_leak_from_preceding_types(self) -> None:
        source = ("/** @typedef {'wrong'} First\n"
                  " * @typedef {{status: 'right'}} Second */")
        self.assertEqual(typedef_values(source, "Second"), ["right"])

    def test_missing_duplicate_and_unbounded_types_fail(self) -> None:
        for source in ("", "/** @typedef {string} Value */",
                       "/** @typedef {'a'} Value\n"
                       " * @typedef {'b'} Value */"):
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    typedef_values(source, "Value")

    def test_native_function_bounds_ignore_comments_and_string_braces(
            self) -> None:
        source = ('// fn key() { "unused" }\n'
                  'fn key() { match self { X => "storage.system.{value}" } }\n'
                  'fn other() { "unused" }')
        self.assertEqual(function_values(source, "key"),
                         {"storage.system.{value}"})

    def test_platform_functions_both_count(self) -> None:
        source = ('fn key() { "unix" }\n'
                  'fn key() { "windows" }')
        self.assertEqual(function_values(source, "key"), {"unix", "windows"})

    def test_missing_or_malformed_native_function_fails(self) -> None:
        for source in ("", "fn key() {"):
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    function_values(source, "key")

    def test_error_contract_belongs_to_the_named_function(self) -> None:
        tick = chr(96)
        source = ("/// Unrelated documentation.\n"
                  "#[expect(unreachable_pub)]\nfn first() {}\n"
                  "/// Errors are " + tick + "notConnected" + tick + ".\n"
                  "#[expect(unreachable_pub)]\npub async fn wanted() {}")
        self.assertEqual(documented_errors(source, "wanted"), ["notConnected"])

    def test_missing_and_empty_error_contracts_fail(self) -> None:
        for source in ("fn wanted() {}",
                       "/// Errors are a storage message.\nfn wanted() {}"):
            with self.subTest(source=source):
                with self.assertRaises(ValueError):
                    documented_errors(source, "wanted")

    def test_real_problem_type_must_match_native_mapping(self) -> None:
        texts = read_texts(ROOT)
        self.assertIn("profile.problem.underage",
                      native_domains(texts)["profile.problem."])
        key = "clients/desktop/ui/profile-fields.js"
        self.assertIn("'underage' | null", texts[key])
        texts[key] = texts[key].replace(
            "'underage' | null", "'unknown' | null")
        with self.assertRaisesRegex(ValueError, "differ"):
            native_domains(texts)


if __name__ == "__main__":
    unittest.main()
