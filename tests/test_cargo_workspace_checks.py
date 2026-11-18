"""Exercise Cargo workspace coverage without compiling the workspaces."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]

GIT = """#!/usr/bin/env python3
import os, sys
if sys.argv[1:] != ["ls-files", "*Cargo.lock"]:
    sys.exit(2)
if os.environ.get("FAIL_GIT"):
    sys.exit(43)
sys.stdout.write(os.environ["LOCKS"])
"""

CARGO = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
with Path(os.environ["CALLS"]).open("a") as stream:
    stream.write(json.dumps([os.getcwd(), *sys.argv[1:]]) + "\\n")
if os.getcwd() == os.environ.get("FAIL_WORKSPACE"):
    print(f"unused fixture dependency in {os.getcwd()}")
    sys.exit(42)
"""


class CargoWorkspaceChecks(unittest.TestCase):
    def run_shear(
        self,
        locks: list[str],
        fail_workspace: str = "",
        fail_git: bool = False,
    ) -> "tuple[subprocess.CompletedProcess[str], list[list[str]]]":
        build = ROOT / "build"
        build.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir=build) as directory:
            root = Path(directory)
            tools = root / "bin"
            tools.mkdir()
            for name, source in {"git": GIT, "cargo": CARGO}.items():
                path = tools / name
                path.write_text(source)
                path.chmod(0o755)
            calls = root / "calls.jsonl"
            env = dict(
                os.environ,
                PATH=f"{tools}:{os.environ['PATH']}",
                LOCKS="".join(f"{lock}\n" for lock in locks),
                CALLS=str(calls),
                FAIL_WORKSPACE=fail_workspace,
                FAIL_GIT="1" if fail_git else "",
                DYAPP_CHECK_LOG_DIR=str(root / "logs"),
            )
            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "tests/scripts/rust-check.sh"),
                    "--raw",
                    "unused-dependencies",
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            lines = calls.read_text().splitlines() if calls.exists() else []
            return result, [json.loads(line) for line in lines]

    def test_every_tracked_workspace_is_checked(self) -> None:
        locks = subprocess.check_output(
            ["git", "ls-files", "*Cargo.lock"], cwd=ROOT, text=True
        ).splitlines()
        result, calls = self.run_shear(locks)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            [row[0] for row in calls],
            [str((ROOT / lock).parent) for lock in locks],
        )

    def test_an_unused_dependency_in_a_child_workspace_fails(self) -> None:
        result, calls = self.run_shear(
            [
                "Cargo.lock",
                "clients/linux/Cargo.lock",
                "tests/fuzz/Cargo.lock",
            ],
            fail_workspace=str(ROOT / "clients/linux"),
        )
        self.assertEqual(result.returncode, 42)
        self.assertIn("unused fixture dependency", result.stdout)
        self.assertEqual(len(calls), 2)

    def test_inventory_failure_cannot_pass_as_empty_coverage(self) -> None:
        result, calls = self.run_shear(["Cargo.lock"], fail_git=True)
        self.assertEqual(result.returncode, 43)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
