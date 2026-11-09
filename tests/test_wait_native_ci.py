"""A release waits for, and only for, its tag's Apple tests."""

import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "tools/scripts/wait-native-ci.py"
SPEC = importlib.util.spec_from_file_location("wait_native_ci", SCRIPT)
assert SPEC is not None and SPEC.loader is not None, (
    "Python source loader is unavailable"
)
WAIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WAIT)


def job(
    name: str, conclusion: str | None = "success", status: str = "completed"
) -> dict[str, str | None]:
    return {"name": name, "conclusion": conclusion, "status": status}


MACOS, IOS = "native - macos", "native - ios"


class Verdict(unittest.TestCase):
    def test_both_green_pass_whatever_else_failed(self) -> None:
        run = [job(MACOS), job(IOS), job("core / mutants", "failure")]
        self.assertEqual(WAIT.verdict([run]), "passed")

    def test_no_run_or_no_jobs_yet_waits(self) -> None:
        self.assertEqual(WAIT.verdict([]), "waiting")
        self.assertEqual(WAIT.verdict([[]]), "waiting")

    def test_one_platform_alone_waits(self) -> None:
        self.assertEqual(WAIT.verdict([[job(MACOS)]]), "waiting")

    def test_a_queued_platform_waits_even_after_the_other_failed(self) -> None:
        run = [job(MACOS, "failure"), job(IOS, None, "queued")]
        self.assertEqual(WAIT.verdict([run]), "waiting")

    def test_a_failed_or_cancelled_platform_fails(self) -> None:
        for conclusion in ("failure", "cancelled", "skipped"):
            run = [job(MACOS), job(IOS, conclusion)]
            self.assertEqual(WAIT.verdict([run]), "failed", conclusion)

    def test_green_jobs_must_share_one_run(self) -> None:
        runs = [
            [job(MACOS), job(IOS, "failure")],
            [job(MACOS, "failure"), job(IOS)],
        ]
        self.assertEqual(WAIT.verdict(runs), "failed")


class TagRuns(unittest.TestCase):
    def test_asks_only_for_the_tag_and_its_commit(self) -> None:
        asked = []

        def fetch(path):
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
