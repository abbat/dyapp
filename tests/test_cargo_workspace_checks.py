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
if sys.argv[1:2] == ["-C"] and sys.argv[3:5] == ["log", "-1"]:
    sys.stdout.write("advisory DB: fixture\\n")
    sys.exit(0)
if sys.argv[1:] != ["ls-files", "*Cargo.lock"]:
    sys.exit(2)
if os.environ.get("FAIL_GIT"):
    sys.exit(43)
sys.stdout.write(os.environ["LOCKS"])
"""

CARGO = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
command = sys.argv[1:]
with Path(os.environ["CALLS"]).open("a") as stream:
    stream.write(json.dumps([os.getcwd(), *command]) + "\\n")
failure = os.environ.get("FAIL_WORKSPACE")
if failure:
    workspace = str(Path(os.environ["ROOT"]) / failure)
    if (
        os.getcwd() == workspace
        or any(failure in argument for argument in command)
    ):
        print(f"fixture failure in {failure}")
        sys.exit(42)
"""


class CargoWorkspaceChecks(unittest.TestCase):
    def run_stage(
        self,
        stage: str,
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
                ROOT=str(ROOT),
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
                    stage,
                ],
                env=env,
                capture_output=True,
                text=True,
                check=False,
            )
            lines = calls.read_text().splitlines() if calls.exists() else []
            return result, [json.loads(line) for line in lines]

    def tracked_locks(self) -> list[str]:
        return subprocess.check_output(
            ["git", "ls-files", "*Cargo.lock"], cwd=ROOT, text=True
        ).splitlines()

    def test_unused_dependencies_cover_every_tracked_workspace(self) -> None:
        locks = self.tracked_locks()
        result, calls = self.run_stage("unused-dependencies", locks)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            [row[0] for row in calls],
            [str((ROOT / lock).parent) for lock in locks],
        )

    def test_an_unused_dependency_in_a_child_workspace_fails(self) -> None:
        result, calls = self.run_stage(
            "unused-dependencies",
            [
                "Cargo.lock",
                "clients/linux/Cargo.lock",
                "tests/fuzz/Cargo.lock",
            ],
            fail_workspace="clients/linux",
        )
        self.assertEqual(result.returncode, 42)
        self.assertIn("Checking clients/linux/Cargo.lock", result.stdout)
        self.assertEqual(len(calls), 2)

    def test_inventory_failure_cannot_pass_as_empty_coverage(self) -> None:
        result, calls = self.run_stage("audit", ["Cargo.lock"], fail_git=True)
        self.assertEqual(result.returncode, 43)
        self.assertEqual(calls, [])

    def test_audit_checks_every_tracked_lockfile(self) -> None:
        locks = self.tracked_locks()
        result, calls = self.run_stage("audit", locks)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(
            calls,
            [[str(ROOT), "audit", "--file", lock] for lock in locks],
        )

    def test_an_advisory_in_a_child_workspace_fails_audit(self) -> None:
        result, calls = self.run_stage(
            "audit",
            [
                "Cargo.lock",
                "clients/linux/Cargo.lock",
                "tests/fuzz/Cargo.lock",
            ],
            fail_workspace="clients/linux",
        )
        self.assertEqual(result.returncode, 42)
        self.assertIn("Auditing clients/linux/Cargo.lock", result.stdout)
        self.assertEqual(len(calls), 2)

    def test_deny_checks_every_tracked_manifest(self) -> None:
        locks = self.tracked_locks()
        result, calls = self.run_stage("security", locks)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        mode = "--frozen" if Path("/opt/advisory-dbs").is_dir() else "--locked"
        self.assertEqual(
            calls,
            [
                [
                    str(ROOT),
                    "deny",
                    mode,
                    "--manifest-path",
                    f"{lock.removesuffix('Cargo.lock')}Cargo.toml",
                    "--config",
                    "deny.toml",
                    "check",
                    "advisories",
                    "bans",
                    "licenses",
                    "sources",
                ]
                for lock in locks
            ],
        )

    def test_a_policy_failure_in_a_child_workspace_fails_deny(self) -> None:
        result, calls = self.run_stage(
            "security",
            [
                "Cargo.lock",
                "clients/linux/Cargo.lock",
                "tests/fuzz/Cargo.lock",
            ],
            fail_workspace="clients/linux",
        )
        self.assertEqual(result.returncode, 42)
        self.assertIn("Checking clients/linux/Cargo.toml", result.stdout)
        self.assertEqual(len(calls), 2)

    def test_security_workflow_uses_the_tested_audit_stage(self) -> None:
        workflow = (ROOT / ".github/workflows/security.yml").read_text()
        self.assertIn(
            "- run: bash tests/scripts/rust-check.sh audit", workflow
        )


if __name__ == "__main__":
    unittest.main()
