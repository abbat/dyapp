"""B7 source inventory and real repository-check failure controls."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_debt", ROOT / "tests/scripts/check-repository.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
MARKERS = ("TO" + "DO", "FIX" + "ME")


class DebtMarkers(unittest.TestCase):
    def test_every_source_extension_rejects_comments_and_literals(self):
        for suffix in CHECK.DEBT_SOURCE:
            path = "nested/example" + suffix
            text = f"// {MARKERS[0]}: deferred\n'{MARKERS[1]}'\n"
            self.assertEqual(CHECK.debt_errors({path: text}), [
                f"{path}:1: {MARKERS[0]} debt marker",
                f"{path}:2: {MARKERS[1]} debt marker",
            ])
        self.assertEqual(CHECK.debt_errors({
            "docs/example.md": MARKERS[0],
            "data.json": f'"{MARKERS[1]}"',
            "code.rs": "let placeholder_value = 1;\n",
        }), [])

    def test_word_boundaries_and_all_findings_are_retained(self):
        first, second = MARKERS
        self.assertEqual(CHECK.debt_errors({
            "a.rs": f"{first}S prefix{second} {first}_name",
        }), [])
        self.assertEqual(CHECK.debt_errors({
            "b.rs": f"// {second}\n",
            "a.rs": f"// {first} {second}\n",
        }), [f"a.rs:1: {first} debt marker",
             f"a.rs:1: {second} debt marker",
             f"b.rs:1: {second} debt marker"])

    def test_main_rejects_a_source_marker_before_unrelated_repo_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            previous = Path.cwd()
            try:
                os.chdir(directory)
                Path("example.rs").write_text(f"// {MARKERS[0]}: fixture\n")
                with patch.object(CHECK.subprocess, "check_output",
                                  return_value=b"example.rs\0"):
                    with self.assertRaisesRegex(
                            ValueError, r"example\.rs:1: .* debt marker"):
                        CHECK.main()
            finally:
                os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
