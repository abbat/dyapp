"""Repository Unicode check: hidden characters and non-ASCII paths."""
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_unicode", ROOT / "tests/scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
# Escaped, so this file passes the check itself.
WAVE, MAN, LAPTOP, JOINER = "\U0001f44b", "\U0001f468", "\U0001f4bb", "\u200d"


class Unicode(unittest.TestCase):
    def test_format_and_private_use_characters_fail_with_file_and_line(self):
        self.assertEqual(CHECK.unicode_errors([], {
            "a.dart": "ok\nif (admin\u202e) {}\n",
            "b.yml": "key: va\u200blue\n",
            "c.md": "icon \ue000\n",
        }), ["a.dart:2: U+202E (Cf) character",
             "b.yml:1: U+200B (Cf) character",
             "c.md:1: U+E000 (Co) character"])

    def test_cyrillic_cjk_and_emoji_pass(self):
        self.assertEqual(CHECK.unicode_errors([], {
            "docs/ru.md": "Привет 你好"
                          f" {WAVE} {MAN}{JOINER}{LAPTOP}\n",
        }), [])
        # A joiner that joins no pictographs is hidden text.
        self.assertEqual(CHECK.unicode_errors([], {"x.py": f"a{JOINER}b\n"}),
                         ["x.py:1: U+200D (Cf) character"])

    def test_swift_and_kotlin_sources_are_ascii(self):
        self.assertEqual(CHECK.unicode_errors([], {
            "A.swift": "let pаssword = 1\n",
            "B.kt": "// ч\n",
            "c.dart": "var pаssword = 1;\n",
        }), ["A.swift:1: U+0430 in ASCII-only source",
             "B.kt:1: U+0447 in ASCII-only source"])

    def test_non_ascii_path_fails(self):
        lookalike = "docs/r\u0435adme.md"
        self.assertEqual(CHECK.unicode_errors(["docs/ok.md", lookalike], {}),
                         [f"{lookalike}: non-ASCII path"])


if __name__ == "__main__":
    unittest.main()
