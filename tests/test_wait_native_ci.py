"""Reuse completed Apple/Android native acceptance from the exact tag."""

import importlib.util
from pathlib import Path
import unittest
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from ci_contracts import Job, JobPage, RunPage


SCRIPT = Path(__file__).parents[1] / "tools/scripts/wait-native-ci.py"
SPEC = importlib.util.spec_from_file_location("wait_native_ci", SCRIPT)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
WAIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WAIT)


def job(
    name: str, conclusion: str | None = "success", status: str = "completed"
) -> "Job":
    return {"name": name, "conclusion": conclusion, "status": status}


MACOS, IOS = "native - macos", "native - ios"
ANDROID = "docker - android-emulator"
PERMISSIONS = "docker - android-permissions"
REQUIRED = (MACOS, IOS, ANDROID, "docker - android-lifecycle", PERMISSIONS)


def native_run(*changes: "Job") -> "list[Job]":
    jobs = {name: job(name) for name in REQUIRED}
    for changed in changes:
        jobs[changed["name"]] = changed
    return list(jobs.values())


class Verdict(unittest.TestCase):
    def test_all_native_jobs_green_pass_whatever_else_failed(self) -> None:
        run = native_run() + [job("core / mutants", "failure")]
        self.assertEqual(WAIT.verdict([run]), "passed")

    def test_no_run_or_no_jobs_yet_waits(self) -> None:
        self.assertEqual(WAIT.verdict([]), "waiting")
        self.assertEqual(WAIT.verdict([[]]), "waiting")

    def test_one_platform_alone_waits(self) -> None:
        self.assertEqual(WAIT.verdict([[job(MACOS)]]), "waiting")

    def test_a_queued_platform_waits_even_after_the_other_failed(self) -> None:
        run = native_run(job(MACOS, "failure"), job(IOS, None, "queued"))
        self.assertEqual(WAIT.verdict([run]), "waiting")

    def test_a_failed_or_cancelled_platform_fails(self) -> None:
        for name in REQUIRED:
            for conclusion in ("failure", "cancelled", "skipped"):
                run = native_run(job(name, conclusion))
                with self.subTest(name=name, conclusion=conclusion):
                    self.assertEqual(WAIT.verdict([run]), "failed")

    def test_missing_or_running_required_jobs_cannot_pass(self) -> None:
        for name in REQUIRED:
            with self.subTest(name=name):
                missing = [
                    item for item in native_run() if item["name"] != name
                ]
                self.assertEqual(WAIT.verdict([missing]), "waiting")
                stale = native_run(job(name, "success", "in_progress"))
                self.assertEqual(WAIT.verdict([stale]), "waiting")

    def test_green_jobs_must_share_one_run(self) -> None:
        runs = [
            native_run(job(MACOS, "failure")),
            native_run(job(IOS, "failure")),
        ]
        self.assertEqual(WAIT.verdict(runs), "failed")


class TagRuns(unittest.TestCase):
    def test_asks_only_for_the_tag_and_its_commit(self) -> None:
        asked: list[str] = []

        def fetch(path: str) -> "RunPage | JobPage":
            asked.append(path)
            if "/workflows/" in path:
                return {"workflow_runs": [{"id": 7}]}
            return {"jobs": [job(MACOS)]}

        self.assertEqual(
            WAIT.tag_runs("v1.2.3", "a" * 40, fetch), [[job(MACOS)]]
        )
        self.assertIn(f"head_sha={'a' * 40}&branch=v1.2.3", asked[0])
        self.assertIn("/runs/7/jobs", asked[1])


if __name__ == "__main__":
    unittest.main()
