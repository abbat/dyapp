"""Reject malformed budgets, stale source and incomplete parser output."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import time
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]


def load(name, filename):
    spec = importlib.util.spec_from_file_location(
        name, ROOT / "tests/scripts" / filename
    )
    assert spec is not None and spec.loader is not None, (
        "Python source loader is unavailable"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


POLICY = load("complexity_policy", "rust_complexity.py")
with mock.patch.dict(sys.modules, rust_complexity=POLICY):
    RUNNER = load("complexity_runner", "check-rust-complexity.py")


def budget():
    return {
        "path": "rust/example.rs",
        "function": "fn retained",
        "metrics": {"lines": 70},
    }


class ComplexityRunnerTest(unittest.TestCase):
    def test_budget_schema_and_nesting_pair_are_strict(self) -> None:
        row = budget()
        self.assertEqual(
            RUNNER.budgets([row]),
            {("rust/example.rs", "fn retained"): {"lines": 70}},
        )
        malformed = [None, {}, [row, row]]
        for value in (False, 0, -1, "70"):
            invalid = copy.deepcopy(row)
            invalid["metrics"]["lines"] = value
            malformed.append([invalid])
        for metrics in ({"unknown": 1}, {"nesting": 6}, [], {}):
            invalid = dict(row, metrics=metrics)
            malformed.append([invalid])
        for rows in malformed:
            with self.assertRaises((ValueError, TypeError)):
                RUNNER.budgets(rows)

    def test_paths_cannot_escape_or_use_platform_separators(self) -> None:
        for path in (
            "/outside.rs",
            "../outside.rs",
            "rust/../../outside.rs",
            "rust\\example.rs",
            "C:/outside.rs",
        ):
            with self.subTest(path=path):
                with self.assertRaises(ValueError):
                    RUNNER.budgets([dict(budget(), path=path)])

    def test_budget_growth_is_compared_to_the_requested_commit(self) -> None:
        path = ROOT / "tests/scripts/rust-complexity/workspace.json"
        old = [budget()]
        current = RUNNER.budgets([dict(budget(), metrics={"lines": 71})])
        with mock.patch.dict(os.environ, COMPLEXITY_BASE="fixture-base"):
            with mock.patch.object(RUNNER.subprocess, "run") as run:
                run.return_value.stdout = path.relative_to(ROOT).as_posix()
                with mock.patch.object(
                    RUNNER.subprocess,
                    "check_output",
                    return_value=json.dumps(old),
                ):
                    errors = RUNNER.budget_changes(path, current)
                self.assertTrue(errors)
                self.assertIn(
                    "fixture-base^{commit}", run.call_args_list[0].args[0]
                )

    def test_threshold_guard_checks_untracked_sources_and_fails_closed(
        self,
    ) -> None:
        directory = ROOT / "build/tool-fixtures/complexity-thresholds"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        paths = b"rust/old.rs\0rust/new.rs\0"
        with mock.patch.object(
            RUNNER.subprocess, "check_output", return_value=paths
        ) as inventory:
            with mock.patch.object(RUNNER.subprocess, "run") as run:
                run.return_value.returncode = 1
                with self.assertRaises(ValueError):
                    RUNNER.check_thresholds(directory)
                self.assertIn("rust/new.rs", run.call_args.args[0])
                self.assertIn("--check-thresholds", run.call_args.args[0])
            self.assertIn("--others", inventory.call_args.args[0])
            self.assertIn("--exclude-standard", inventory.call_args.args[0])

    def test_source_outside_checkout_and_recent_edits_fail(self) -> None:
        directory = ROOT / "build/tool-fixtures/complexity-runner"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        path = directory / "example.rs"
        path.write_text("fn example() {}")
        with mock.patch.object(RUNNER, "ROOT", directory):
            outside = str(ROOT / "Cargo.toml")
            cases = ((outside, time.time_ns()), ("example.rs", 0))
            for filename, started in cases:
                finding = {"span": {"file_name": filename}}
                with self.assertRaises(ValueError):
                    RUNNER.sources(directory, started, [finding])

    def test_parser_must_return_each_requested_source_once(self) -> None:
        directory = ROOT / "build/tool-fixtures/complexity-parser"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        contents = {"example.rs": b"fn example() {}"}
        entry = {"path": "example.rs", "functions": []}
        for records in (
            [],
            [entry, entry],
            [{"path": "other.rs", "functions": []}],
        ):
            output = "\n".join(map(json.dumps, records))
            with mock.patch.object(RUNNER.subprocess, "run") as run:
                run.return_value.returncode = 0
                run.return_value.stdout = output
                with self.assertRaises(ValueError):
                    RUNNER.measure([], contents, directory)


if __name__ == "__main__":
    unittest.main()
