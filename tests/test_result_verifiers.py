"""Exercise strict report gates inside the dev Docker container."""

import typing

import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

if typing.TYPE_CHECKING:
    import fixture_contracts as fc


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "apple_results", ROOT / "tests/scripts/check-apple-results.py"
)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
APPLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(APPLE)


class Reports(unittest.TestCase):
    def test_apple_accepts_complete_inventory(self) -> None:
        self.assertEqual(APPLE.validate(self.apple_summary()), 3)

    @staticmethod
    def apple_summary() -> "fc.AppleSummary":
        return {
            "totalTestCount": 3,
            "passedTests": 3,
            "failedTests": 0,
            "skippedTests": 0,
            "result": "Passed",
        }

    def test_apple_rejects_partial_failed_skipped_and_malformed(self) -> None:
        cases: list[dict[str, object]] = [
            {},
            {"totalTestCount": 0},
            {"passedTests": 2},
            {"failedTests": 1},
            {"skippedTests": 1},
            {"result": "Failed"},
            {"passedTests": True},
            {"totalTestCount": 2, "passedTests": 2},
        ]
        for change in cases:
            data: dict[str, object] = (
                dict(copy.deepcopy(self.apple_summary())) if change else {}
            )
            data.update(change)
            with self.subTest(change=change), self.assertRaises(ValueError):
                APPLE.validate(data)

    def run_validator(
        self,
        script: "str",
        contents: "str | None",
        junit: bool = False,
        extra: "typing.Sequence[str]" = (),
    ) -> "subprocess.CompletedProcess[str]":
        scratch = ROOT / "build/result-verifier-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            filename = "TEST-app.xml" if junit else "report.txt"
            path = Path(directory) / filename
            if contents is not None:
                path.write_text(contents)
            argument = directory if junit else str(path)
            result = subprocess.run(
                [
                    "python3",
                    str(ROOT / "tests/scripts" / script),
                    argument,
                    *extra,
                ],
                capture_output=True,
                text=True,
                check=False,
                timeout=30,
            )
            if (
                script == "check-instrumentation-results.py"
                and result.returncode
            ):
                self.assertFalse(path.with_suffix(".json").exists())
            return result

    def test_cli_defaults_raw_and_retained_logs(self) -> None:
        coverage = {
            "type": "llvm.coverage.json.export",
            "data": [
                {
                    "files": [
                        {"filename": f"/rust/{crate}/src/lib.rs"}
                        for crate in (
                            "bootstrap",
                            "client",
                            "ffi",
                            "identity",
                            "messaging",
                            "p2p-net",
                            "profile",
                            "video",
                        )
                    ],
                    "totals": {"lines": {"count": 100, "covered": 100}},
                }
            ],
        }
        cases = [
            (
                "check_coverage.py",
                "coverage",
                json.dumps(coverage),
                False,
                "100.00% line coverage",
            ),
            (
                "check-junit-results.py",
                "junit",
                '<testsuite tests="1"/>',
                True,
                "1 JUnit tests passed",
            ),
            (
                "check-apple-results.py",
                "apple-results",
                json.dumps(self.apple_summary()),
                False,
                "3 native tests passed",
            ),
            (
                "check-instrumentation-results.py",
                "instrumentation",
                "OK (3 tests)\nINSTRUMENTATION_CODE: -1\n",
                False,
                "INSTRUMENTATION_CODE: -1",
            ),
        ]
        scratch = ROOT / "build/result-verifier-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(dir=scratch) as directory:
            root = Path(directory)
            logs = root / "logs"
            env = dict(os.environ, DYAPP_CHECK_LOG_DIR=str(logs))
            for script, name, contents, junit, raw_text in cases:
                path = root / ("TEST-app.xml" if junit else "report.txt")
                path.write_text(contents)
                argument = str(root if junit else path)
                command = ["python3", str(ROOT / "tests/scripts" / script)]
                result = subprocess.run(
                    command + [argument],
                    capture_output=True,
                    text=True,
                    env=env,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout)
                self.assertEqual(result.stdout, f"OK {name}\n")
                raw = subprocess.run(
                    command + ["--raw", argument],
                    capture_output=True,
                    text=True,
                    env=env,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(raw.returncode, 0, raw.stderr)
                self.assertIn(raw_text, raw.stdout)
                saved = list(logs.glob(f"{name}-*.log"))
                self.assertEqual(len(saved), 1)
                self.assertIn(raw_text, saved[0].read_text())
                if name == "instrumentation":
                    self.assertEqual(
                        json.loads(path.with_suffix(".json").read_text())[
                            "tests"
                        ],
                        3,
                    )

    def test_junit_requires_nonempty_complete_success(self) -> None:
        good = '<testsuite tests="1" failures="0" errors="0" skipped="0"/>'
        self.assertEqual(
            self.run_validator(
                "check-junit-results.py", good, True
            ).returncode,
            0,
        )
        for bad in (
            None,
            "",
            "<broken",
            '<testsuite tests="0"/>',
            '<testsuite tests="1" failures="1"/>',
            '<testsuite tests="1" errors="1"/>',
            '<testsuite tests="1" skipped="1"/>',
        ):
            with self.subTest(report=bad):
                self.assertNotEqual(
                    self.run_validator(
                        "check-junit-results.py", bad, True
                    ).returncode,
                    0,
                )

    def test_instrumentation_rejects_zero_exit_failure_output(self) -> None:
        good = "OK (3 tests)\nINSTRUMENTATION_CODE: -1\n"
        pretty = "com.dyapp.AppUITests:...\nTime: 39.948\n\nOK (3 tests)\n"
        for report in (good, pretty):
            self.assertEqual(
                self.run_validator(
                    "check-instrumentation-results.py", report
                ).returncode,
                0,
            )
        for bad in (
            None,
            "",
            "OK (0 tests)\nINSTRUMENTATION_CODE: -1",
            "OK (2 tests)\nINSTRUMENTATION_CODE: -1",
            pretty + "Process crashed",
            good + "FAILURES!!!",
            good.replace("-1", "-2"),
            "INSTRUMENTATION_STATUS: partial\nOK (3 tests)",
            "INSTRUMENTATION_CODE: -1\nFAILURES!!!",
        ):
            with self.subTest(report=bad):
                self.assertNotEqual(
                    self.run_validator(
                        "check-instrumentation-results.py", bad
                    ).returncode,
                    0,
                )

    def test_instrumentation_rejects_ignored_and_assumed_tests(self) -> None:
        def record(current: int, code: int) -> str:
            fields = (
                "INSTRUMENTATION_STATUS: id=AndroidJUnitRunner\n"
                "INSTRUMENTATION_STATUS: class=com.dyapp.Fixture$Nested\n"
                f"INSTRUMENTATION_STATUS: current={current}\n"
                f"INSTRUMENTATION_STATUS: test=example{current}\n"
            )
            return (
                fields
                + "INSTRUMENTATION_STATUS_CODE: 1\n"
                + fields
                + f"INSTRUMENTATION_STATUS_CODE: {code}\n"
            )

        summary = (
            "INSTRUMENTATION_RESULT: stream=\nTime: 0\n\n"
            "OK (3 tests)\n\nINSTRUMENTATION_CODE: -1\n"
        )
        passed = "".join(record(index, 0) for index in (1, 2, 3))
        self.assertEqual(
            self.run_validator(
                "check-instrumentation-results.py", passed + summary
            ).returncode,
            0,
        )
        for code in (-3, -4):
            with self.subTest(code=code):
                # Ignored tests do not contribute to JUnit's run count.
                count = 4 if code == -3 else 3
                records = "".join(
                    record(index, code if index == 2 else 0)
                    for index in range(1, count + 1)
                )
                result = self.run_validator(
                    "check-instrumentation-results.py", records + summary
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("Skipped Android instrumentation", result.stdout)

        assumed = record(1, -4) + summary.replace("3 tests", "1 test")
        self.assertNotEqual(
            self.run_validator(
                "check-instrumentation-results.py", assumed, extra=("1",)
            ).returncode,
            0,
        )

    def test_instrumentation_rerun_takes_a_lower_minimum(self) -> None:
        rerun = "OK (1 test)\nINSTRUMENTATION_CODE: -1\n"
        self.assertNotEqual(
            self.run_validator(
                "check-instrumentation-results.py", rerun
            ).returncode,
            0,
        )
        self.assertEqual(
            self.run_validator(
                "check-instrumentation-results.py", rerun, extra=("1",)
            ).returncode,
            0,
        )


if __name__ == "__main__":
    unittest.main()
