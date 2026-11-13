"""Observe reboot readiness without sleeping or launching apps."""

import importlib.util
import subprocess
import unittest
from pathlib import Path
from typing import NamedTuple

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location(
    "android_lifecycle", ROOT / "tests/scripts/android_lifecycle.py"
)
assert SPEC is not None and SPEC.loader is not None
LIFECYCLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(LIFECYCLE)
OLD = "11111111-1111-4111-8111-111111111111"
NEW = "22222222-2222-4222-8222-222222222222"
BOOT = ["shell", "cat", "/proc/sys/kernel/random/boot_id"]
JOB = [
    "shell",
    "cmd",
    "jobscheduler",
    "get-job-state",
    "--user",
    "current",
    "com.dyapp",
    "1",
]
WAIT = ["wait-for-device"]
PROP = ["shell", "getprop", "sys.boot_completed"]
PACKAGE = ["shell", "cmd", "package", "path", "android"]


class Step(NamedTuple):
    args: list[str]
    output: str = ""
    elapsed: float = 0
    status: int = 0


class FakeAdb:
    def __init__(self, steps: list[Step]) -> None:
        self.steps = list(steps)
        self.clock = 0.0
        self.calls: list[list[str]] = []
        self.budgets: list[float] = []

    def now(self) -> float:
        return self.clock

    def __call__(
        self, command: list[str], timeout: float
    ) -> subprocess.CompletedProcess[str]:
        self.calls.append(command[3:])
        self.budgets.append(timeout)
        step = self.steps.pop(0)
        if command != ["adb", "-s", "emulator-5554", *step.args]:
            raise AssertionError(f"Unexpected ADB command: {command}")
        self.clock += step.elapsed
        return subprocess.CompletedProcess(
            command, step.status, stdout=step.output, stderr=""
        )


def setup_steps() -> list[Step]:
    return [Step(BOOT, OLD), Step(JOB, "waiting"), Step(["reboot"])]


class AndroidLifecycle(unittest.TestCase):
    def test_old_transport_and_transient_boot_states_cannot_pass_as_a_reboot(
        self,
    ) -> None:
        fake = FakeAdb(
            setup_steps()
            + [
                Step(WAIT, elapsed=1),
                Step(BOOT, OLD, elapsed=1),
                Step(WAIT, elapsed=1),
                Step(BOOT, status=1, elapsed=1),
                Step(WAIT, elapsed=1),
                Step(BOOT, NEW, elapsed=1),
                Step(WAIT),
                Step(PROP, "0", elapsed=3),
                Step(WAIT),
                Step(PROP, "1", elapsed=3),
                Step(WAIT),
                Step(PACKAGE, status=1, elapsed=3),
                Step(WAIT),
                Step(PACKAGE, "package:/dynamic/framework.apk", elapsed=3),
                Step(JOB, "pending ready"),
            ]
        )
        report = LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
            "com.dyapp", 1, 10
        )
        self.assertEqual(
            report,
            {
                "before": OLD,
                "after": NEW,
                "before_job": "waiting",
                "job": "pending ready",
            },
        )
        self.assertFalse(fake.steps)
        self.assertGreater(fake.clock, 10)
        self.assertEqual(fake.budgets[3:9], [10, 9, 8, 7, 6, 5])
        self.assertFalse(
            any("instrument" in command for command in fake.calls)
        )

    def test_unchanged_boot_expires_before_reading_completion(self) -> None:
        fake = FakeAdb(
            setup_steps()
            + [
                Step(WAIT, elapsed=4),
                Step(BOOT, OLD, elapsed=4),
                Step(WAIT, elapsed=4),
            ]
        )
        with self.assertRaisesRegex(RuntimeError, "changed boot identity"):
            LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                "com.dyapp", 1, 10
            )
        self.assertNotIn(PROP, fake.calls)

    def test_a_late_valid_boot_identity_cannot_pass(self) -> None:
        fake = FakeAdb(
            setup_steps() + [Step(WAIT), Step(BOOT, NEW, elapsed=11)]
        )
        with self.assertRaisesRegex(RuntimeError, "changed boot identity"):
            LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                "com.dyapp", 1, 10
            )
        self.assertNotIn(PROP, fake.calls)

    def test_missing_original_job_refuses_to_reboot(self) -> None:
        fake = FakeAdb([Step(BOOT, OLD), Step(JOB, "no such job", status=23)])
        with self.assertRaisesRegex(RuntimeError, "get-job-state"):
            LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                "com.dyapp", 1, 10
            )
        self.assertNotIn(["reboot"], fake.calls)

    def test_a_missing_restored_job_fails_before_any_app_launch(self) -> None:
        fake = FakeAdb(
            setup_steps()
            + [
                Step(WAIT),
                Step(BOOT, NEW),
                Step(WAIT),
                Step(PROP, "1"),
                Step(WAIT),
                Step(PACKAGE, "package:/framework.apk"),
                Step(JOB, "no such job", status=23),
            ]
        )
        with self.assertRaisesRegex(RuntimeError, "get-job-state"):
            LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                "com.dyapp", 1, 10
            )
        self.assertFalse(fake.steps)
        self.assertFalse(
            any(
                "instrument" in command or "am" in command
                for command in fake.calls
            )
        )

    def test_transport_failure_does_not_repeat_the_reboot(self) -> None:
        fake = FakeAdb(setup_steps() + [Step(WAIT, status=1)])
        with self.assertRaisesRegex(RuntimeError, "transport"):
            LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                "com.dyapp", 1, 10
            )
        self.assertEqual(fake.calls.count(["reboot"]), 1)

    def test_invalid_inputs_have_no_device_side_effects(self) -> None:
        cases = [
            ("com.dyapp;echo", 1, 10),
            ("com.dyapp", -1, 10),
            ("com.dyapp", 1, 0),
            ("com.dyapp", 1, float("nan")),
            ("com.dyapp", 1, float("inf")),
        ]
        for package, job, timeout in cases:
            with self.subTest(package=package, job=job, timeout=timeout):
                fake = FakeAdb([])
                with self.assertRaises(ValueError):
                    LIFECYCLE.AdbLifecycle(run=fake, now=fake.now).reboot(
                        package, job, timeout
                    )
                self.assertFalse(fake.calls)


if __name__ == "__main__":
    unittest.main()
