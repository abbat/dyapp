"""Execute CI build steps with noisy fixture tools instead of native SDKs."""

import ast
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import unittest

import yaml

ROOT = Path(__file__).resolve().parents[1]
WORKFLOWS = ROOT / ".github/workflows"
TOOL = """#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
status = int(os.environ['FIXTURE_STATUS'])
Path(os.environ['FIXTURE_ARGS']).write_text(json.dumps(sys.argv[1:]))
print('passing compiler progress', file=sys.stderr)
if Path(sys.argv[0]).name == 'cargo':
    print(json.dumps({'reason': 'build-finished', 'success': status == 0}))
else:
    print('passing compiler progress')
if status:
    if Path(sys.argv[0]).name == 'gradlew':
        print('FAILURE: Build failed with an exception.')
        print('* What went wrong:\\nfixture compilation failed\\n* Try:')
        print('passing compiler help')
    else:
        prefix = 'error:'
        if Path(sys.argv[0]).name != 'cargo':
            prefix = 'Fixture.swift:7:3: error:'
        print(prefix + ' fixture compilation failed', file=sys.stderr)
sys.exit(status)
"""


def step(job: str, name: str) -> tuple[str, str]:
    workflow = yaml.safe_load((WORKFLOWS / "codeql.yml").read_text())
    selected = next(
        item
        for item in workflow["jobs"][job]["steps"]
        if item.get("name") == name
    )
    command = selected["run"]
    directory = selected.get("working-directory", ".")
    assert isinstance(command, str) and isinstance(directory, str)
    command = re.sub(
        r"(?:\.\./)*tools/scripts/run-check\.py",
        shlex.quote(str(ROOT / "tools/scripts/run-check.py")),
        command,
    )
    return command, directory


def execute(
    root: Path, job: str, name: str, status: int
) -> subprocess.CompletedProcess[str]:
    binaries = root / "bin"
    binaries.mkdir()
    android = root / "clients/android"
    android.mkdir(parents=True)
    for tool in [
        binaries / "cargo",
        binaries / "xcodebuild",
        android / "gradlew",
    ]:
        tool.write_text(TOOL)
        tool.chmod(0o755)
    command, directory = step(job, name)
    return subprocess.run(
        ["/bin/bash", "-e", "-c", command],
        cwd=root / directory,
        env=dict(
            os.environ,
            PATH=f"{binaries}:{os.environ['PATH']}",
            FIXTURE_STATUS=str(status),
            FIXTURE_ARGS=str(root / "arguments.json"),
        ),
        capture_output=True,
        text=True,
        check=False,
        timeout=15,
    )


class CiOutputTests(unittest.TestCase):
    def test_workflow_builds_capture_both_streams_and_keep_exit_status(
        self,
    ) -> None:
        cases = [
            ("kotlin", "build", "android-build"),
            ("swift", "build", "apple-build"),
            ("rust-analysis", "diagnostics", "clippy"),
        ]
        expected_argument = {
            "kotlin": ":app:compileDebugKotlin",
            "swift": "generic/platform=iOS Simulator",
            "rust-analysis": "--message-format=json",
        }
        for job, name, check in cases:
            for status in (0, 7):
                with self.subTest(job=job, status=status):
                    with tempfile.TemporaryDirectory() as directory:
                        root = Path(directory)
                        result = execute(root, job, name, status)
                        self.assertEqual(
                            result.returncode, status, result.stdout
                        )
                        self.assertEqual(result.stderr, "")
                        self.assertNotIn("passing compiler", result.stdout)
                        if status == 0:
                            self.assertEqual(result.stdout, f"OK {check}\n")
                        else:
                            self.assertIn(f"FAIL {check}:", result.stdout)
                            self.assertIn(
                                "fixture compilation failed", result.stdout
                            )
                            self.assertIn(
                                ": raw captured output", result.stdout
                            )
                        logs = list(
                            (root / "build/check-output").glob("*.log")
                        )
                        self.assertEqual(len(logs), 1)
                        self.assertIn(
                            "passing compiler progress", logs[0].read_text()
                        )
                        self.assertIn(
                            expected_argument[job],
                            json.loads((root / "arguments.json").read_text()),
                        )
                        if job == "rust-analysis":
                            expected = {
                                "reason": "build-finished",
                                "success": status == 0,
                            }
                            self.assertEqual(
                                (root / "clippy.json").read_text(),
                                json.dumps(expected) + "\n",
                            )
                            self.assertNotIn('"reason"', result.stdout)

    def test_every_declared_frontend_has_a_diagnostic_policy(self) -> None:
        policies = json.loads(
            (ROOT / "tools/scripts/check-output.json").read_text()
        )
        paths = [
            path
            for folder in ("tests/scripts", "tools/scripts")
            for path in (ROOT / folder).glob("*.py")
        ]
        checked = 0
        for path in paths:
            for call in ast.walk(ast.parse(path.read_text())):
                if not (
                    isinstance(call, ast.Call)
                    and isinstance(call.func, ast.Name)
                    and call.func.id == "wrap"
                    and call.args
                    and isinstance(call.args[0], ast.Constant)
                    and isinstance(call.args[0].value, str)
                ):
                    continue
                with self.subTest(frontend=path.name):
                    self.assertIn(call.args[0].value, policies)
                checked += 1
        self.assertGreater(checked, 0)

    def test_no_workflow_bypasses_capture_with_raw_mode(self) -> None:
        paths = sorted(WORKFLOWS.glob("*.yml"))
        self.assertTrue(paths)
        for path in paths:
            workflow = yaml.safe_load(path.read_text())
            for job in workflow["jobs"].values():
                for item in job.get("steps", []):
                    command = item.get("run", "")
                    with self.subTest(
                        workflow=path.name, step=item.get("name")
                    ):
                        self.assertNotRegex(
                            command, r"(?<![\w-])--raw(?:\s|$)"
                        )


if __name__ == "__main__":
    unittest.main()
