"""Bootstrap production code reads time through its clock; test modules and
the clock itself are free to read it directly."""

import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "check_clock", ROOT / "tests/scripts/check-clock.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
SRC = CHECK.SRC


class ClockReads(unittest.TestCase):
    def test_counts_production_reads_only(self) -> None:
        sources = {
            SRC / "clock.rs": "Instant::now()",
            SRC / "a.rs": "let t = Instant::now();\n"
            "// SystemTime::now() in a comment\n"
            "#[cfg(test)]\nmod b;\n"
            '#[cfg(test)]\n#[path = "a/c_tests.rs"]\nmod c;\n'
            "let u = chrono::Utc::now();\n"
            "#[cfg(test)]\nmod tests {\n    Instant::now();\n    mod g;\n}\n",
            SRC / "a/b.rs": "Instant::now()",
            SRC / "a/b/d.rs": "Instant::now()",
            SRC / "a/c_tests.rs": "SystemTime::now()",
            SRC / "a/tests/g.rs": "Utc::now()",
            SRC / "e.rs": "mod f;\nstd::time::SystemTime::now()",
        }
        self.assertEqual(CHECK.counts(sources), {"a.rs": 2, "e.rs": 1})

    def test_budget_only_goes_down(self) -> None:
        self.assertEqual(
            CHECK.errors({"a.rs": 2, "new.rs": 1}, {"a.rs": 3}),
            [
                f"{SRC / 'a.rs'}: 2 direct clock reads, budget 3;"
                " lower the budget in tests/scripts/check-clock.py",
                f"{SRC / 'new.rs'}: 1 direct clock reads, budget 0;"
                " read time through crate::clock::Clock",
            ],
        )

    def test_tree_is_within_budget(self) -> None:
        self.assertEqual(CHECK.main(), 0)


if __name__ == "__main__":
    unittest.main()
