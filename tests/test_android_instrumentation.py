"""Completion callbacks renew instrumentation budgets without sleeps."""

import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest import TestCase, main, mock

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_instrumentation",
    ROOT / "tests/scripts/android_instrumentation.py",
)
assert SPEC is not None and SPEC.loader is not None
MONITOR = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MONITOR)


def status(current: int, code: int, name: str = "example") -> bytes:
    return (
        "INSTRUMENTATION_STATUS: id=AndroidJUnitRunner\r\n"
        f"INSTRUMENTATION_STATUS: current={current}\r\n"
        "INSTRUMENTATION_STATUS: class=com.dyapp.Fixture\r\n"
        f"INSTRUMENTATION_STATUS: test={name}\r\n"
        f"INSTRUMENTATION_STATUS_CODE: {code}\r\n"
    ).encode()


def phase(
    case: str, name: str = "example", suite: str = "com.dyapp.Fixture"
) -> bytes:
    return (
        "INSTRUMENTATION_STATUS: id=DYAppVisual\n"
        f"INSTRUMENTATION_STATUS: class={suite}\n"
        f"INSTRUMENTATION_STATUS: test={name}\n"
        f"INSTRUMENTATION_STATUS: case={case}\n"
        "INSTRUMENTATION_STATUS_CODE: 2\n"
    ).encode()


class AndroidInstrumentation(TestCase):
    def test_completed_visual_cases_renew_without_fabricating_junit_results(
        self,
    ) -> None:
        clock = 0.0
        events: list[dict[str, object]] = []
        progress = MONITOR.Progress(
            10, now=lambda: clock, emit=events.append, phase_callbacks=True
        )
        progress.feed(status(1, 1))
        for case in range(1, 9):
            clock += 8
            progress.feed(phase(str(case)))
            self.assertEqual(progress.remaining(), 10)
            self.assertEqual(progress.completed, 0)
        self.assertEqual(progress.completed_cases, 8)
        self.assertEqual(
            [
                event["code"]
                for event in events
                if event["event"] == "phase_completed"
            ],
            [str(case) for case in range(1, 9)],
        )
        progress.feed(status(1, 0))
        self.assertEqual(progress.completed, 1)

    def test_visual_callbacks_require_opt_in_and_an_active_matching_test(
        self,
    ) -> None:
        clock = 0.0
        default = MONITOR.Progress(10, now=lambda: clock)
        enabled = MONITOR.Progress(10, now=lambda: clock, phase_callbacks=True)
        enabled.feed(phase("1"))
        default.feed(status(1, 1))
        enabled.feed(status(1, 1))
        clock = 2
        default.feed(phase("1"))
        self.assertEqual(default.remaining(), 8)
        for callback in (
            phase("2"),
            phase("0"),
            phase("-1"),
            phase("one"),
            phase("١"),
            phase("1", name="other"),
            phase("1", suite="com.dyapp.Other"),
            b"arbitrary completion log\n",
        ):
            enabled.feed(callback)
            self.assertEqual(enabled.remaining(), 8)
        enabled.feed(phase("1"))
        clock = 4
        enabled.feed(phase("1"))
        self.assertEqual(enabled.remaining(), 8)
        self.assertEqual(enabled.completed_cases, 1)

    def test_late_phase_cannot_revive_expired_deadline_and_sequence_resets(
        self,
    ) -> None:
        clock = 0.0
        progress = MONITOR.Progress(
            10, now=lambda: clock, phase_callbacks=True
        )
        progress.feed(status(1, 1))
        clock = 8
        progress.feed(phase("1"))
        progress.feed(status(1, 0))
        progress.feed(status(2, 1, "next"))
        clock = 16
        progress.feed(phase("1", name="next"))
        self.assertEqual(progress.completed_cases, 1)
        self.assertEqual(progress.remaining(), 10)
        clock = 26
        with self.assertRaises(TimeoutError):
            progress.feed(phase("2", name="next"))
        self.assertEqual(progress.completed_cases, 1)

    def test_healthy_sequence_can_outlive_the_initial_budget(self) -> None:
        clock = 0.0
        progress = MONITOR.Progress(10, now=lambda: clock)
        clock = 8
        progress.feed(status(1, 1))
        self.assertEqual(progress.remaining(), 10)
        for current in range(1, 5):
            clock += 8
            progress.feed(status(current, 0))
            self.assertEqual(progress.remaining(), 10)
            if current < 4:
                progress.feed(status(current + 1, 1))
        self.assertEqual(progress.completed, 4)
        self.assertGreater(clock, 10)
        progress.feed(b"INSTRUMENTATION_CODE: -1\n")
        self.assertEqual(progress.phase, "ADB exit after final result")

    def test_noise_duplicate_start_and_orphan_completion_do_not_renew(
        self,
    ) -> None:
        clock = 0.0
        progress = MONITOR.Progress(10, now=lambda: clock)
        progress.feed(status(1, 1))
        for current, code, name in (
            (1, 1, "example"),
            (2, 0, "other"),
            (1, 0, "wrong"),
        ):
            clock += 2
            progress.feed(
                b"arbitrary log output\n" + status(current, code, name)
            )
            self.assertEqual(progress.remaining(), 10 - clock)
        clock = 10
        with self.assertRaisesRegex(TimeoutError, "com.dyapp.Fixture#example"):
            progress.feed(status(1, 0))
        self.assertEqual(progress.completed, 0)

    def test_each_terminal_test_outcome_is_progress_not_a_suite_verdict(
        self,
    ) -> None:
        for code in (0, -1, -2, -3, -4):
            with self.subTest(code=code):
                clock = 0.0
                progress = MONITOR.Progress(10, now=lambda: clock)
                progress.feed(status(1, 1))
                clock = 9
                progress.feed(status(1, code))
                self.assertEqual(progress.completed, 1)
                self.assertEqual(progress.remaining(), 10)

    def test_partial_lines_are_bounded_and_cannot_create_callbacks(
        self,
    ) -> None:
        clock = 0.0
        progress = MONITOR.Progress(10, now=lambda: clock)
        payload = status(1, 1) + status(1, 0)
        for byte in payload:
            progress.feed(bytes([byte]))
        self.assertEqual(progress.completed, 1)
        clock = 4
        progress.feed(b"x" * 65536)
        self.assertLessEqual(len(progress.buffer), 4096)
        progress.feed(status(2, 1))
        progress.feed(status(2, 0))
        self.assertEqual(progress.completed, 1)
        self.assertEqual(progress.remaining(), 6)
        progress.feed(status(2, 1) + status(2, 0))
        self.assertEqual(progress.completed, 2)

    def test_long_stack_lines_do_not_discard_a_real_completion(self) -> None:
        progress = MONITOR.Progress(10, now=lambda: 0)
        progress.feed(status(1, 1))
        metadata, code = status(1, -2).rsplit(
            b"INSTRUMENTATION_STATUS_CODE:", 1
        )
        progress.feed(
            metadata + b"INSTRUMENTATION_STATUS: stack=" + b"x" * 65536 + b"\n"
        )
        progress.feed(b"INSTRUMENTATION_STATUS_CODE:" + code)
        self.assertEqual(progress.completed, 1)

    def test_repeated_completions_and_final_markers_do_not_extend_exit(
        self,
    ) -> None:
        clock = 0.0
        progress = MONITOR.Progress(10, now=lambda: clock)
        progress.feed(status(1, 1) + status(1, 0))
        clock = 4
        progress.feed(status(1, 0))
        self.assertEqual(progress.remaining(), 6)
        progress.feed(b"INSTRUMENTATION_CODE: -1\n")
        clock = 8
        progress.feed(
            b"INSTRUMENTATION_CODE: -1\n" + status(2, 1) + status(2, 0)
        )
        self.assertEqual(progress.remaining(), 6)
        self.assertEqual(progress.completed, 1)

    def test_invalid_budgets_and_unidentified_statuses_cannot_admit_work(
        self,
    ) -> None:
        for budget in (0, -1, float("nan"), float("inf")):
            with self.subTest(budget=budget), self.assertRaises(ValueError):
                MONITOR.Progress(budget)
        progress = MONITOR.Progress(10, now=lambda: 0)
        for payload in (
            status(1, 1).replace(b"AndroidJUnitRunner", b"another"),
            status(1, 1).replace(b"current=1", b"current=" + b"9" * 100),
            status(1, 1).replace(b"current=1", b"current=wrong"),
            status(1, 1).replace(b"test=example", b"test="),
        ):
            progress.feed(payload + status(1, 0))
            self.assertEqual(progress.completed, 0)
            self.assertIsNone(progress.active)

    def test_stream_keeps_bytes_exit_status_and_existing_result_validation(
        self,
    ) -> None:
        scratch = ROOT / "build/android-instrumentation-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        folder = Path(tempfile.mkdtemp(dir=scratch))
        success = (
            status(1, 1)
            + status(1, 0)
            + b"INSTRUMENTATION_RESULT: stream=\nTime: 0\n\nOK (1 test)\n\n"
            + b"INSTRUMENTATION_CODE: -1\n"
        )
        report = folder / "result.txt"
        launched = subprocess.run(
            [
                sys.executable,
                "-B",
                str(ROOT / "tests/scripts/android_instrumentation.py"),
                "--report",
                str(report),
                "--deadline",
                "30",
                "--",
                sys.executable,
                "-c",
                "import os; os.write(1, " + repr(success) + ")",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(launched.returncode, 0, launched.stderr)
        self.assertEqual(report.read_bytes(), success)
        checked = subprocess.run(
            [
                sys.executable,
                "-B",
                str(ROOT / "tests/scripts/check-instrumentation-results.py"),
                str(report),
                "1",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertEqual(checked.returncode, 0, checked.stderr)
        with io.BytesIO() as output:
            self.assertEqual(
                MONITOR.run(
                    [sys.executable, "-c", "raise SystemExit(7)"], output, 30
                ),
                7,
            )

    def test_missing_final_result_stops_without_a_retryable_success(
        self,
    ) -> None:
        with self.assertRaisesRegex(RuntimeError, "without a final result"):
            MONITOR.run(
                [sys.executable, "-c", "print('transport ended')"],
                io.BytesIO(),
                30,
            )

    def test_failed_raw_run_keeps_junit_retry_selection_and_is_rejected(
        self,
    ) -> None:
        failed = (
            status(1, 1)
            + status(1, -2)
            + b"INSTRUMENTATION_RESULT: stream=\nTime: 0\n"
            b"There was 1 failure:\n1) example(com.dyapp.Fixture$Nested)\n"
            b"java.lang.AssertionError\nFAILURES!!!\n"
            b"Tests run: 1, Failures: 1\n\nINSTRUMENTATION_CODE: -1\n"
        )
        output = io.BytesIO()
        self.assertEqual(
            MONITOR.run(
                [
                    sys.executable,
                    "-c",
                    "import os; os.write(1, " + repr(failed) + ")",
                ],
                output,
                30,
            ),
            0,
        )
        self.assertEqual(output.getvalue(), failed)
        spec = importlib.util.spec_from_file_location(
            "instrumentation_flaky", ROOT / "tests/scripts/check-flaky.py"
        )
        assert spec is not None and spec.loader is not None
        flaky = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(flaky)
        self.assertEqual(
            flaky.instrumentation_failures(failed.decode()),
            ["com.dyapp.Fixture$Nested example"],
        )
        scratch = ROOT / "build/android-instrumentation-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        report = Path(tempfile.mkdtemp(dir=scratch)) / "failure.txt"
        report.write_bytes(failed)
        checked = subprocess.run(
            [
                sys.executable,
                "-B",
                str(ROOT / "tests/scripts/check-instrumentation-results.py"),
                str(report),
                "1",
            ],
            capture_output=True,
            text=True,
            timeout=30,
        )
        self.assertNotEqual(checked.returncode, 0)
        self.assertFalse(report.with_suffix(".json").exists())

    def test_readiness_timeout_terminates_and_reaps_its_owned_process(
        self,
    ) -> None:
        with (
            mock.patch.object(MONITOR.subprocess, "Popen") as spawn,
            mock.patch.object(
                MONITOR.selectors, "DefaultSelector"
            ) as selector,
        ):
            process = spawn.return_value.__enter__.return_value
            process.poll.return_value = None
            watcher = selector.return_value.__enter__.return_value
            watcher.select.return_value = []
            with self.assertRaisesRegex(TimeoutError, "runner startup"):
                MONITOR.run(["owned-fixture"], io.BytesIO(), 10)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=30)

    def test_closed_output_does_not_leave_an_unbounded_process_wait(
        self,
    ) -> None:
        with (
            mock.patch.object(MONITOR.subprocess, "Popen") as spawn,
            mock.patch.object(
                MONITOR.selectors, "DefaultSelector"
            ) as selector,
            mock.patch.object(MONITOR.os, "read", return_value=b""),
        ):
            process = spawn.return_value.__enter__.return_value
            process.poll.return_value = None
            process.wait.side_effect = [
                subprocess.TimeoutExpired("owned-fixture", 10),
                0,
            ]
            watcher = selector.return_value.__enter__.return_value
            watcher.select.return_value = [(None, 1)]
            with self.assertRaisesRegex(TimeoutError, "ADB process exit"):
                MONITOR.run(["owned-fixture"], io.BytesIO(), 10)
            process.terminate.assert_called_once()
            self.assertEqual(process.wait.call_count, 2)

    def test_unresponsive_process_is_killed_after_termination(self) -> None:
        with (
            mock.patch.object(MONITOR.subprocess, "Popen") as spawn,
            mock.patch.object(
                MONITOR.selectors, "DefaultSelector"
            ) as selector,
        ):
            process = spawn.return_value.__enter__.return_value
            process.poll.return_value = None
            process.wait.side_effect = [
                subprocess.TimeoutExpired("owned-fixture", 30),
                0,
            ]
            watcher = selector.return_value.__enter__.return_value
            watcher.select.return_value = []
            with self.assertRaises(TimeoutError):
                MONITOR.run(["owned-fixture"], io.BytesIO(), 10)
            process.kill.assert_called_once()
            self.assertEqual(process.wait.call_count, 2)


if __name__ == "__main__":
    main()
