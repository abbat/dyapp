"""Retain validated completion timing across outer job deadlines."""

import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest import TestCase, main, mock

from test_android_instrumentation import MONITOR, ROOT, status


class InstrumentationTiming(TestCase):
    def test_progress_records_wall_clock_without_using_it_as_a_deadline(self) -> None:
        events: list[MONITOR.ProgressEvent] = []
        with mock.patch.object(MONITOR.time, "time", side_effect=[100.0, 90.0]):
            progress = MONITOR.Progress(10, now=lambda: 0.0, emit=events.append)
            progress.feed(status(1, 1))
        self.assertEqual([event["unix_time"] for event in events], [100.0, 90.0])
        self.assertEqual(progress.remaining(), 10)

    def test_only_validated_transitions_emit_identity_and_elapsed_time(
        self,
    ) -> None:
        clock = 0.0
        output = io.StringIO()
        progress = MONITOR.Progress(
            10,
            now=lambda: clock,
            emit=lambda event: MONITOR.write_event(output, event),
        )
        clock = 2
        progress.feed(status(1, 1))
        clock = 9
        progress.feed(b"ordinary output\n" + status(1, 1))
        self.assertEqual(progress.remaining(), 3)
        progress.feed(status(1, 0))
        self.assertEqual(progress.remaining(), 10)
        clock = 10
        progress.feed(status(2, 1, "second"))
        progress.feed(status(2, 0, "wrong"))
        self.assertEqual(progress.remaining(), 9)
        clock = 18
        progress.feed(status(2, -2, "second"))
        clock = 19
        progress.feed(b"INSTRUMENTATION_CODE: -1\n")
        records = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(
            [
                (item["event"], item["elapsed_seconds"], item["completed"])
                for item in records
            ],
            [
                ("runner_started", 0, 0),
                ("test_started", 2, 0),
                ("test_completed", 9, 1),
                ("test_started", 10, 1),
                ("test_completed", 18, 2),
                ("runner_finished", 19, 2),
            ],
        )
        self.assertEqual(records[4]["current"], "2")
        self.assertEqual(records[4]["test_class"], "com.dyapp.Fixture")
        self.assertEqual(records[4]["test"], "second")
        self.assertEqual(records[4]["code"], "-2")

    def test_a_late_result_neither_renews_deadline_nor_creates_an_event(
        self,
    ) -> None:
        clock = 0.0
        output = io.StringIO()
        progress = MONITOR.Progress(
            10,
            now=lambda: clock,
            emit=lambda event: MONITOR.write_event(output, event),
        )
        clock = 10
        with self.assertRaises(TimeoutError):
            progress.feed(status(1, 1))
        self.assertEqual(len(output.getvalue().splitlines()), 1)

    def test_event_write_failure_reaps_process_and_keeps_raw_output(
        self,
    ) -> None:
        payload = status(1, 1) + status(1, 0)
        with (
            mock.patch.object(MONITOR.subprocess, "Popen") as spawn,
            mock.patch.object(
                MONITOR.selectors, "DefaultSelector"
            ) as selector,
            mock.patch.object(MONITOR.os, "read", return_value=payload),
        ):
            process = spawn.return_value.__enter__.return_value
            process.poll.return_value = None
            watcher = selector.return_value.__enter__.return_value
            watcher.select.return_value = [(None, 1)]
            emit = mock.Mock(
                side_effect=[None, None, OSError("event output failed")]
            )
            output = io.BytesIO()
            with self.assertRaisesRegex(OSError, "event output failed"):
                MONITOR.run(["owned-fixture"], output, 10, emit)
            self.assertEqual(output.getvalue(), payload)
            process.terminate.assert_called_once()
            process.wait.assert_called_once_with(timeout=30)

    def test_cli_appends_invocations_and_retains_latest_raw_result(
        self,
    ) -> None:
        scratch = ROOT / "build/android-instrumentation-timing-tests"
        scratch.mkdir(parents=True, exist_ok=True)
        report = Path(tempfile.mkdtemp(dir=scratch)) / "instrumentation.txt"
        payload = status(1, 1) + status(1, 0) + b"INSTRUMENTATION_CODE: -1\n"
        command = [
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
            "import os; os.write(1, " + repr(payload) + ")",
        ]
        for _ in range(2):
            result = subprocess.run(
                command, capture_output=True, text=True, timeout=30
            )
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(report.read_bytes(), payload)
        records = [
            json.loads(line)
            for line in report.with_suffix(".progress.jsonl")
            .read_text()
            .splitlines()
        ]
        self.assertEqual(len(records), 8)
        for first in (0, 4):
            self.assertEqual(records[first]["event"], "runner_started")
            self.assertEqual(records[first + 3]["event"], "runner_finished")
            self.assertEqual(records[first + 3]["completed"], 1)
            elapsed = [
                item["elapsed_seconds"] for item in records[first : first + 4]
            ]
            self.assertEqual(elapsed, sorted(elapsed))


if __name__ == "__main__":
    main()
