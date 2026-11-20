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
if (Path(sys.argv[0]).name == 'docker'
        and sys.argv[1:3] == ['image', 'inspect']):
    print('fixture-image')
    sys.exit(status)
Path(os.environ['FIXTURE_ARGS']).write_text(json.dumps(sys.argv[1:]))
if (Path(sys.argv[0]).name == 'docker' and status == 0
        and '--output' in sys.argv):
    output = sys.argv[sys.argv.index('--output') + 1]
    directory = Path(output.removeprefix('type=local,dest='))
    directory.mkdir(parents=True)
    (directory / 'manifest.json').write_text('fixture provenance')
    (directory / 'jna-fixture-hardened.aar').write_bytes(b'fixture')
if (Path(sys.argv[0]).name == 'gradlew'
        and os.environ.get('FIXTURE_REQUIRE_JNA') == '1'):
    argument = next(arg for arg in sys.argv
                    if arg.startswith('-Pdyapp.jnaNativeDir='))
    directory = Path(argument.partition('=')[2])
    present = (directory / 'manifest.json').is_file()
    if not directory.is_absolute() or not present:
        print('FAILURE: Build failed with an exception.')
        print('* What went wrong:\\nmissing prepared JNA bundle\\n* Try:')
        sys.exit(9)
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
    elif Path(sys.argv[0]).name == 'docker':
        print('ERROR: failed to solve: fixture compilation failed',
              file=sys.stderr)
    else:
        prefix = 'error:'
        if Path(sys.argv[0]).name != 'cargo':
            prefix = 'Fixture.swift:7:3: error:'
        print(prefix + ' fixture compilation failed', file=sys.stderr)
sys.exit(status)
"""


def step(
    job: str, name: str, workflow_name: str = "codeql.yml"
) -> tuple[str, str]:
    workflow = yaml.safe_load((WORKFLOWS / workflow_name).read_text())
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
    root: Path,
    job: str,
    name: str,
    status: int,
    *,
    prepare: bool = False,
    require_jna: bool = False,
    workflow_name: str = "codeql.yml",
) -> subprocess.CompletedProcess[str]:
    binaries = root / "bin"
    binaries.mkdir()
    android = root / "clients/android"
    android.mkdir(parents=True)
    for tool in [
        binaries / "cargo",
        binaries / "docker",
        binaries / "xcodebuild",
        android / "gradlew",
    ]:
        tool.write_text(TOOL)
        tool.chmod(0o755)
    (root / "build/prototype-review").mkdir(parents=True)
    command, directory = step(job, name, workflow_name)
    if prepare:
        preparation, _ = step("kotlin", "prepare JNA")
        command = "\n".join(
            [
                f"cd {shlex.quote(str(root))}",
                preparation,
                f"cd {shlex.quote(str(root / directory))}",
                command,
            ]
        )
    return subprocess.run(
        ["/bin/bash", "-e", "-c", command],
        cwd=root / directory,
        env=dict(
            os.environ,
            PATH=f"{binaries}:{os.environ['PATH']}",
            FIXTURE_STATUS=str(status),
            FIXTURE_ARGS=str(root / "arguments.json"),
            GITHUB_WORKSPACE=str(root),
            DYAPP_CHECK_LOG_DIR=str(root / "build/check-output"),
            FIXTURE_REQUIRE_JNA="1" if require_jna else "0",
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
            ("kotlin", "prepare JNA", "docker"),
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
                            "jna-native-export"
                            if name == "prepare JNA"
                            else expected_argument[job],
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

    def test_kotlin_build_requires_the_exported_jna_directory(self) -> None:
        workflow = yaml.safe_load((WORKFLOWS / "codeql.yml").read_text())
        names = [
            item.get("name") for item in workflow["jobs"]["kotlin"]["steps"]
        ]
        cache = next(
            index
            for index, item in enumerate(workflow["jobs"]["kotlin"]["steps"])
            if item.get("uses", "").startswith("Swatinem/rust-cache@")
        )
        self.assertLess(cache, names.index("prepare JNA"))
        self.assertLess(names.index("prepare JNA"), names.index("init"))
        for prepared in (False, True):
            with (
                self.subTest(prepared=prepared),
                tempfile.TemporaryDirectory() as directory,
            ):
                root = Path(directory)
                result = execute(
                    root,
                    "kotlin",
                    "build",
                    0,
                    prepare=prepared,
                    require_jna=True,
                )
                self.assertEqual(
                    result.returncode, 0 if prepared else 9, result.stdout
                )
                arguments = json.loads((root / "arguments.json").read_text())
                self.assertIn(
                    "-Pdyapp.jnaNativeDir="
                    + str(root / "build/codeql-jna-native"),
                    arguments,
                )
                if not prepared:
                    self.assertIn("missing prepared JNA bundle", result.stdout)

    def test_prototype_steps_keep_real_capture_mapping_and_exit_status(
        self,
    ) -> None:
        cases = [
            ("prepare existing capture image", "docker", "build"),
            ("capture", "ui-tests", "run"),
        ]
        for name, check, operation in cases:
            for status in (0, 7):
                with (
                    self.subTest(step=name, status=status),
                    tempfile.TemporaryDirectory(dir=ROOT / "build") as folder,
                ):
                    root = Path(folder)
                    result = execute(
                        root,
                        "capture",
                        name,
                        status,
                        workflow_name="prototype-review.yml",
                    )
                    self.assertEqual(
                        result.returncode,
                        status,
                        result.stdout + result.stderr,
                    )
                    self.assertEqual(result.stderr, "")
                    self.assertNotIn("passing compiler", result.stdout)
                    if status == 0:
                        self.assertEqual(result.stdout, f"OK {check}\n")
                    else:
                        self.assertIn(f"FAIL {check}:", result.stdout)
                    arguments = json.loads(
                        (root / "arguments.json").read_text()
                    )
                    self.assertIn("compose", arguments)
                    self.assertIn(operation, arguments)

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
