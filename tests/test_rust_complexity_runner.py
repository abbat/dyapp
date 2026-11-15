"""Reject malformed budgets, stale source and incomplete parser output."""

import typing


import types

import copy
import importlib.util
import json
import os
from pathlib import Path
import sys
import time
import unittest
import unittest.mock

if typing.TYPE_CHECKING:
    import fixture_contracts as fc


ROOT = Path(__file__).resolve().parents[1]


def load(name: "str", filename: "str") -> "types.ModuleType":
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
with unittest.mock.patch.dict(sys.modules, rust_complexity=POLICY):
    RUNNER = load("complexity_runner", "check-rust-complexity.py")


def budget() -> "fc.Budget":
    return {
        "path": "rust/example.rs",
        "function": "fn retained",
        "metrics": {"lines": 70},
    }


class ComplexityRunnerTest(unittest.TestCase):
    def failed_clippy(self, output: str, stderr: str) -> str:
        directory = ROOT / "build/tool-fixtures/complexity-errors"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)

        def fail(*_args: object, **_kwargs: object) -> types.SimpleNamespace:
            (directory / "clippy.jsonl").write_text(output)
            (directory / "clippy.stderr").write_text(stderr)
            return types.SimpleNamespace(returncode=1)

        with (
            unittest.mock.patch.object(
                RUNNER.subprocess, "run", side_effect=fail
            ),
            unittest.mock.patch.object(
                RUNNER.subprocess, "check_output", return_value="{}"
            ),
            self.assertRaises(ValueError) as failure,
        ):
            RUNNER.clippy("workspace", directory)
        self.assertEqual((directory / "clippy.jsonl").read_text(), output)
        self.assertEqual((directory / "clippy.stderr").read_text(), stderr)
        return str(failure.exception)

    def test_failed_clippy_exposes_only_compiler_errors_with_locations(
        self,
    ) -> None:
        diagnostic: fc.CompilerRecord = {
            "reason": "compiler-message",
            "message": {
                "level": "error",
                "message": "unnecessary clone",
                "code": {"code": "clippy::cloned_ref_to_slice_refs"},
                "spans": [
                    {
                        "file_name": "rust/ffi/example.rs",
                        "line_start": 139,
                        "is_primary": True,
                    }
                ],
            },
        }
        warning = copy.deepcopy(diagnostic)
        warning["message"]["level"] = "warning"
        warning["message"]["message"] = "warning noise"
        output = "\n".join(map(json.dumps, [warning, diagnostic]))
        message = self.failed_clippy(output, "Compiling progress")
        self.assertIn("rust/ffi/example.rs:139: unnecessary clone", message)
        self.assertNotIn("warning noise", message)
        self.assertNotIn("Compiling progress", message)
        self.assertIn("clippy.jsonl", message)
        self.assertIn("clippy.stderr", message)

    def test_crashed_clippy_uses_bounded_stderr_despite_partial_json(
        self,
    ) -> None:
        stderr = "progress" * 1000 + "\nrustc terminated by SIGSEGV"
        message = self.failed_clippy('{"reason":', stderr)
        self.assertEqual(message.split("\n", 1)[1], stderr[-4096:])

    def test_failed_clippy_bounds_compiler_error_output(self) -> None:
        records: list[fc.CompilerRecord] = [
            {
                "reason": "compiler-message",
                "message": {
                    "level": "error",
                    "message": f"failure-{index}",
                    "code": {"code": "E0001"},
                    "spans": [],
                },
            }
            for index in range(10)
        ]
        message = self.failed_clippy("\n".join(map(json.dumps, records)), "")
        self.assertIn("compiler: failure-7", message)
        self.assertNotIn("failure-8", message)
        records[0]["message"]["message"] = "x" * 10000
        message = self.failed_clippy("\n".join(map(json.dumps, records)), "")
        self.assertEqual(
            message.split("\n", 1)[1],
            "compiler: " + "x" * (4096 - len("compiler: ")),
        )

    def test_budget_schema_and_nesting_pair_are_strict(self) -> None:
        row = budget()
        self.assertEqual(
            RUNNER.budgets([row]),
            {("rust/example.rs", "fn retained"): {"lines": 70}},
        )
        malformed: list[object] = [None, {}, [row, row]]
        for value in (False, 0, -1, "70"):
            invalid = copy.deepcopy(row)
            invalid["metrics"]["lines"] = value
            malformed.append([invalid])
        metrics: object
        for metrics in ({"unknown": 1}, {"nesting": 6}, [], {}):
            invalid_override = dict(row, metrics=metrics)
            malformed.append([invalid_override])
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
        # A missing base (a forced push drops the old head) means origin/main.
        for missing, expected in ((0, "fixture-base"), (1, "origin/main")):
            with (
                self.subTest(missing=missing),
                unittest.mock.patch.dict(
                    os.environ, COMPLEXITY_BASE="fixture-base"
                ),
                unittest.mock.patch.object(RUNNER.subprocess, "run") as run,
                unittest.mock.patch.object(
                    RUNNER.subprocess,
                    "check_output",
                    return_value=json.dumps(old),
                ),
            ):
                run.return_value.stdout = path.relative_to(ROOT).as_posix()
                run.return_value.returncode = missing
                self.assertTrue(RUNNER.budget_changes(path, current))
                self.assertIn(
                    expected + "^{commit}", run.call_args_list[1].args[0]
                )

    def test_threshold_guard_checks_untracked_sources_and_fails_closed(
        self,
    ) -> None:
        directory = ROOT / "build/tool-fixtures/complexity-thresholds"
        directory = directory / str(time.time_ns())
        directory.mkdir(parents=True)
        paths = b"rust/old.rs\0rust/new.rs\0"
        with unittest.mock.patch.object(
            RUNNER.subprocess, "check_output", return_value=paths
        ) as inventory:
            with unittest.mock.patch.object(RUNNER.subprocess, "run") as run:
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
        with unittest.mock.patch.object(RUNNER, "ROOT", directory):
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
            with unittest.mock.patch.object(RUNNER.subprocess, "run") as run:
                run.return_value.returncode = 0
                run.return_value.stdout = output
                with self.assertRaises(ValueError):
                    RUNNER.measure([], contents, directory)


if __name__ == "__main__":
    unittest.main()
