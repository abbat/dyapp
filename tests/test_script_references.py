"""Script reference roots, imports and disconnected cycles."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "repository_unused", ROOT / "tests/scripts/repository_unused.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class ScriptReferences(unittest.TestCase):
    def test_orphan_and_self_reference_fail(self) -> None:
        for source in ("print('standalone')", "# tools/orphan.py"):
            with self.subTest(source=source):
                self.assertEqual(
                    CHECK.script_reference_errors(
                        {
                            "tools/orphan.py": source,
                        }
                    ),
                    ["tools/orphan.py: unreferenced script"],
                )

    def test_commands_documentation_and_imports_are_roots(self) -> None:
        sources = (
            ("Makefile", "python3 tools/entry.py"),
            ("docs/use.md", "Run entry.py manually."),
            ("tests/test_entry.py", "import entry"),
            ("tests/test_entry.py", "from tools import entry"),
            ("tests/test_entry.py", "from . import entry"),
        )
        for name, source in sources:
            with self.subTest(source=source):
                self.assertEqual(
                    CHECK.script_reference_errors(
                        {
                            "tools/entry.py": "",
                            name: source,
                        }
                    ),
                    [],
                )

    def test_only_rooted_cycles_are_reachable(self) -> None:
        cycle = {"tools/a.py": "import b", "tools/b.py": "import a"}
        self.assertEqual(
            CHECK.script_reference_errors(cycle),
            [
                "tools/a.py: unreferenced script",
                "tools/b.py: unreferenced script",
            ],
        )
        self.assertEqual(
            CHECK.script_reference_errors(
                cycle | {"Makefile": "python3 tools/a.py"}
            ),
            [],
        )

    def test_exact_manual_entry_roots_its_helpers(self) -> None:
        texts = {"tools/a.py": "import b", "tools/b.py": ""}
        self.assertEqual(
            CHECK.script_reference_errors(
                texts, {"tools/a.py": "Manual recovery command"}
            ),
            [],
        )

    def test_empty_missing_and_stale_entries_fail(self) -> None:
        texts = {"tools/a.py": ""}
        self.assertIn(
            "tools/a.py: script entry needs a reason",
            CHECK.script_reference_errors(texts, {"tools/a.py": ""}),
        )
        self.assertIn(
            "tools/missing.py: stale script entry",
            CHECK.script_reference_errors(
                texts, {"tools/missing.py": "Manual command"}
            ),
        )
        self.assertEqual(
            CHECK.script_reference_errors(
                texts | {"Makefile": "python3 tools/a.py"},
                {"tools/a.py": "Manual command"},
            ),
            ["tools/a.py: stale script entry"],
        )

    def test_malformed_python_is_not_silently_ignored(self) -> None:
        with self.assertRaises(SyntaxError):
            CHECK.script_reference_errors({"tools/broken.py": "def ("})


if __name__ == "__main__":
    unittest.main()
