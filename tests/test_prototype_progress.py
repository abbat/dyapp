"""Drive readiness deadlines with callbacks, without timing assumptions."""

from collections.abc import Callable
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent / "scripts"))
from prototype_progress import Progress  # noqa: E402 -- project module path


class ReadinessDeadlines(unittest.TestCase):
    def test_only_verified_progress_replaces_the_failure_deadline(
        self,
    ) -> None:
        callbacks: list[Callable[[], bool]] = []
        cancelled: list[int] = []
        failures: list[str] = []

        def schedule(callback: Callable[[], bool]) -> int:
            callbacks.append(callback)
            return len(callbacks)

        def cancel(timer: int) -> bool:
            cancelled.append(timer)
            return True

        progress = Progress(schedule, cancel, failures.append)
        progress.begin("load")
        self.assertEqual(failures, [])
        self.assertEqual(len(callbacks), 1)
        # The caller received the real load-completion callback.
        progress.begin("layout")
        self.assertEqual(cancelled, [1])
        self.assertFalse(callbacks[0]())
        self.assertEqual(failures, [])
        self.assertFalse(callbacks[1]())
        self.assertEqual(failures, ["layout"])
        self.assertFalse(callbacks[1]())
        self.assertEqual(failures, ["layout"])
        progress.close()
        self.assertEqual(cancelled, [1])

    def test_success_cancels_last_deadline_and_ignores_late_callback(
        self,
    ) -> None:
        callbacks: list[Callable[[], bool]] = []
        cancelled: list[int] = []
        failures: list[str] = []

        def schedule(callback: Callable[[], bool]) -> int:
            callbacks.append(callback)
            return 42

        def cancel(timer: int) -> bool:
            cancelled.append(timer)
            return True

        progress = Progress(schedule, cancel, failures.append)
        progress.begin("snapshot")
        progress.close()
        progress.close()
        self.assertEqual(cancelled, [42])
        self.assertFalse(callbacks[0]())
        self.assertEqual(failures, [])
