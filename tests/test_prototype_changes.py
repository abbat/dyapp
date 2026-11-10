"""Exercise prototype selection against real push and diverged PR histories."""

from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class PrototypeChanges(unittest.TestCase):
    def test_push_pr_manual_and_failed_comparison(self) -> None:
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        root = Path(tempfile.mkdtemp(prefix="prototype-changes-", dir=build))

        def git(*args: str) -> str:
            return subprocess.check_output(
                [
                    "git",
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    *args,
                ],
                cwd=root,
                text=True,
                stderr=subprocess.STDOUT,
            ).strip()

        git("init", "--quiet")
        style = root / "clients/desktop/ui/style.css"
        style.parent.mkdir(parents=True)
        style.write_text("body { color: black; }\n")
        rust = root / "rust/example.rs"
        rust.parent.mkdir()
        rust.write_text("const VALUE: u8 = 0;\n")
        git("add", ".")
        git("commit", "--quiet", "-m", "Initial fixture")
        initial = git("rev-parse", "HEAD")
        git("checkout", "--quiet", "-b", "base")
        style.write_text("body { color: white; }\n")
        git("add", ".")
        git("commit", "--quiet", "-m", "UI fixture change")
        ui = git("rev-parse", "HEAD")
        git("checkout", "--quiet", "-b", "topic", initial)
        rust.write_text("const VALUE: u8 = 1;\n")
        git("add", ".")
        git("commit", "--quiet", "-m", "Unrelated fixture change")
        unrelated = git("rev-parse", "HEAD")
        cases = [
            ("push", initial, unrelated, False),
            ("push", initial, ui, True),
            ("pull_request", ui, unrelated, False),
            ("workflow_dispatch", initial, unrelated, True),
            ("push", "0" * 40, unrelated, True),
            ("push", "missing-revision", unrelated, None),
            ("unknown-event", initial, unrelated, None),
        ]
        for index, (event, base, head, expected) in enumerate(cases):
            output = root / f"output-{index}"
            result = subprocess.run(
                [
                    sys.executable,
                    "-B",
                    str(ROOT / "tests/scripts/check-prototype-changes.py"),
                ],
                cwd=root,
                capture_output=True,
                text=True,
                check=False,
                env=dict(
                    os.environ,
                    EVENT_NAME=event,
                    DIFF_BASE=base,
                    DIFF_HEAD=head,
                    GITHUB_OUTPUT=str(output),
                    DYAPP_CHECK_LOG_DIR=str(root / "logs"),
                ),
            )
            if expected is None:
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(output.exists())
            else:
                self.assertEqual(
                    result.returncode, 0, result.stdout + result.stderr
                )
                self.assertEqual(
                    output.read_text(), f"needed={str(expected).lower()}\n"
                )
                self.assertEqual(result.stdout, "OK prototype-changes\n")


if __name__ == "__main__":
    unittest.main()
