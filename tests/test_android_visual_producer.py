"""Reject untrusted APK producers without waiting for unrelated Apple jobs."""

import copy
import importlib.util
from pathlib import Path
from typing import cast
from unittest import TestCase, main

SPEC = importlib.util.spec_from_file_location(
    "android_visual_producer",
    Path(__file__).parent / "scripts/android_visual_producer.py",
)
assert SPEC is not None and SPEC.loader is not None
PRODUCER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PRODUCER)

SOURCE = "a" * 40
REPO = {"id": 7, "full_name": "abbat/dyapp"}
RUN = {
    "id": 42,
    "path": ".github/workflows/ci.yml",
    "event": "push",
    "head_branch": "main",
    "head_sha": SOURCE,
    "repository": REPO,
    "head_repository": REPO,
    "status": "in_progress",
}
JOB = {
    "name": "docker - android",
    "status": "completed",
    "conclusion": "success",
    "head_sha": SOURCE,
    "run_id": 42,
}
ARTIFACT = {
    "id": 13,
    "name": "android-runtime-packages",
    "expired": False,
    "digest": "sha256:" + "b" * 64,
    "workflow_run": {
        "id": 42,
        "head_sha": SOURCE,
        "head_branch": "main",
        "head_repository_id": 7,
    },
}


class AndroidVisualProducer(TestCase):
    def test_accepts_completed_android_while_other_jobs_run(self) -> None:
        result = PRODUCER.validate(RUN, [JOB], [ARTIFACT], "abbat/dyapp", 42)
        self.assertEqual(result["source_sha"], SOURCE)
        self.assertEqual(result["artifact_id"], "13")

    def test_rejects_untrusted_or_mismatched_run(self) -> None:
        changes = (
            {"id": 43},
            {"event": "pull_request"},
            {"head_branch": "dependabot/gradle"},
            {"head_repository": {"id": 9, "full_name": "fork/dyapp"}},
            {"path": ".github/workflows/other.yml"},
            {"head_sha": "invalid"},
        )
        for change in changes:
            with self.subTest(change=change), self.assertRaises(ValueError):
                PRODUCER.validate(
                    RUN | change, [JOB], [ARTIFACT], "abbat/dyapp", 42
                )

    def test_requires_one_successful_matching_build(self) -> None:
        for jobs in (
            [],
            [JOB, JOB],
            [JOB | {"conclusion": "failure"}],
            [JOB | {"status": "in_progress"}],
            [JOB | {"head_sha": "c" * 40}],
            [JOB | {"run_id": 43}],
        ):
            with self.subTest(jobs=jobs), self.assertRaises(ValueError):
                PRODUCER.validate(RUN, jobs, [ARTIFACT], "abbat/dyapp", 42)

    def test_rejects_expired_duplicate_or_wrong_source_artifact(self) -> None:
        for artifacts in (
            [],
            [ARTIFACT, ARTIFACT],
            [ARTIFACT | {"expired": True}],
            [ARTIFACT | {"digest": "unverified"}],
            [ARTIFACT | {"id": -1}],
        ):
            with (
                self.subTest(artifacts=artifacts),
                self.assertRaises(ValueError),
            ):
                PRODUCER.validate(RUN, [JOB], artifacts, "abbat/dyapp", 42)
        for key, value in (
            ("id", 43),
            ("head_sha", "c" * 40),
            ("head_branch", "feature"),
            ("head_repository_id", 9),
        ):
            artifact = copy.deepcopy(ARTIFACT)
            origin = cast(dict[str, object], ARTIFACT["workflow_run"])
            artifact["workflow_run"] = origin | {key: value}
            with self.subTest(key=key), self.assertRaises(ValueError):
                PRODUCER.validate(RUN, [JOB], [artifact], "abbat/dyapp", 42)


if __name__ == "__main__":
    main()
