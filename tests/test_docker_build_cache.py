"""Exercise optional-cache recovery with real fake-Docker subprocesses."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from typing import TypedDict

ROOT = Path(__file__).resolve().parents[1]
BLOB = "#31 ERROR: blob sha256:" + "a" * 64 + ": not found"


class RecordedBuild(TypedDict):
    args: list[str]
    buildkit: str


class DockerCacheRecovery(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory(dir=ROOT / "build")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.calls = self.root / "calls.jsonl"
        self.logs = self.root / "logs"
        self.dockerfile = self.root / "Dockerfile"
        self.dockerfile.write_text("FROM registry.example/base@sha256:abc\n")
        self.probe = self.root / "probe.py"
        scripts = ROOT / "tools/scripts"
        source = scripts / "docker-local.py"
        self.probe.write_text(
            "import os,runpy,sys\n"
            f"sys.path.insert(0, {str(scripts)!r})\n"
            f"module = runpy.run_path({str(source)!r})\n"
            "raise SystemExit(module['build_images'](\n"
            " ['build', 'app'], dict(os.environ),\n"
            " log_file=os.environ.get('EXPLICIT_LOG')))\n"
        )
        docker = self.root / "docker"
        docker.write_text(
            "#!/usr/bin/env python3\n"
            "import json,os,pathlib,sys\n"
            "args = sys.argv[1:]\n"
            "if args[:1] == ['compose']:\n"
            " print(os.environ['COMPOSE_CONFIG']); raise SystemExit(0)\n"
            "if args[:2] == ['image', 'prune']: raise SystemExit(0)\n"
            "file = pathlib.Path(os.environ['CALLS'])\n"
            "count = 0\n"
            "if file.exists(): count = len(file.read_text().splitlines())\n"
            "with file.open('a') as log:\n"
            " log.write(json.dumps({'args':args,\n"
            "  'buildkit':os.environ['DOCKER_BUILDKIT']}) + '\\n')\n"
            "if count == 0 and os.environ['FIRST_STATUS'] != '0':\n"
            " if os.environ['LARGE'] == '1': print('x' * 400000)\n"
            " print(os.environ['FIRST_OUTPUT'])\n"
            " raise SystemExit(int(os.environ['FIRST_STATUS']))\n"
            "print('controlled second build result')\n"
            "raise SystemExit(int(os.environ['SECOND_STATUS']))\n"
        )
        docker.chmod(0o755)

    def run_build(
        self,
        output: str = BLOB,
        first: int = 1,
        second: int = 0,
        *,
        verbose: bool = False,
        explicit: bool = False,
        large: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        config = {
            "services": {
                "app": {
                    "image": "dyapp:android-test",
                    "build": {"context": str(self.root)},
                }
            }
        }
        env = {
            **os.environ,
            "PATH": f"{self.root}:{os.environ['PATH']}",
            "CALLS": str(self.calls),
            "COMPOSE_CONFIG": json.dumps(config),
            "DYAPP_BUILD_CACHE": "gha",
            "GITHUB_REF": "refs/heads/main",
            "DYAPP_CHECK_LOG_DIR": str(self.logs),
            "DYAPP_VERBOSE": str(int(verbose)),
            "DYAPP_CPUS": "1",
            "FIRST_STATUS": str(first),
            "FIRST_OUTPUT": output,
            "SECOND_STATUS": str(second),
            "LARGE": str(int(large)),
        }
        if explicit:
            env["EXPLICIT_LOG"] = str(self.root / "requested.log")
        else:
            env.pop("EXPLICIT_LOG", None)
        return subprocess.run(
            [sys.executable, "-B", str(self.probe)],
            env=env,
            capture_output=True,
            text=True,
            check=False,
            timeout=30,
        )

    def recorded(self) -> list[RecordedBuild]:
        return [
            json.loads(line) for line in self.calls.read_text().splitlines()
        ]

    def test_missing_blob_recovers_once_and_keeps_both_logs(self) -> None:
        result = self.run_build()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        calls = self.recorded()
        self.assertEqual(len(calls), 2)
        initial, fresh = (call["args"] for call in calls)
        self.assertIn("--cache-from", initial)
        self.assertNotIn("--cache-from", fresh)
        self.assertIn("--no-cache", fresh)
        self.assertIn("--cache-to", fresh)
        self.assertEqual(initial[-1], fresh[-1])
        self.assertTrue(all(call["buildkit"] == "1" for call in calls))
        self.assertNotIn("FAIL", result.stdout)
        self.assertIn("INFO", result.stdout)
        logs = list(self.logs.glob("*.log"))
        self.assertEqual(len(logs), 2)
        self.assertTrue(any(BLOB in log.read_text() for log in logs))
        self.assertTrue(
            any("second build result" in log.read_text() for log in logs)
        )

    def test_second_failure_propagates_without_a_third_attempt(self) -> None:
        result = self.run_build(second=7)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(len(self.recorded()), 2)
        self.assertIn("FAIL", result.stdout)
        self.assertTrue(
            any(BLOB in file.read_text() for file in self.logs.glob("*.log"))
        )

    def test_compiler_failure_never_retries(self) -> None:
        result = self.run_build("error: E0277 controlled compiler rejection")
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(self.recorded()), 1)
        self.assertNotIn("INFO", result.stdout)
        self.assertIn("E0277", result.stdout)

    def test_interruption_never_retries_even_with_a_blob_diagnostic(
        self,
    ) -> None:
        result = self.run_build(first=130)
        self.assertEqual(result.returncode, 130)
        self.assertEqual(len(self.recorded()), 1)
        self.assertNotIn("INFO", result.stdout)

    def test_similar_unstructured_text_does_not_authorize_recovery(
        self,
    ) -> None:
        result = self.run_build("compiler: " + BLOB)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(self.recorded()), 1)

    def test_verbose_mode_preserves_streaming_without_hidden_recovery(
        self,
    ) -> None:
        result = self.run_build(verbose=True)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(len(self.recorded()), 1)
        self.assertIn(BLOB, result.stdout)

    def test_explicit_log_is_preserved_with_a_separate_failure_receipt(
        self,
    ) -> None:
        result = self.run_build(explicit=True)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertIn(BLOB, (self.root / "requested.log").read_text())
        copies = list(self.logs.glob("docker-cache-failure-*.log"))
        self.assertEqual(len(copies), 1)
        self.assertIn(BLOB, copies[0].read_text())

    def test_large_progress_keeps_complete_raw_evidence_and_recovers(
        self,
    ) -> None:
        result = self.run_build(large=True)
        self.assertEqual(result.returncode, 0, result.stdout)
        self.assertEqual(len(self.recorded()), 2)
        self.assertTrue(
            any(
                file.stat().st_size > 400000
                for file in self.logs.glob("*.log")
            )
        )

    def test_local_base_build_keeps_its_cpu_budget_and_never_retries(
        self,
    ) -> None:
        self.dockerfile.write_text("FROM dyapp:dev-local\n")
        result = self.run_build()
        self.assertEqual(result.returncode, 1)
        calls = self.recorded()
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0]["buildkit"], "0")
        self.assertIn("--cpu-quota", calls[0]["args"])


if __name__ == "__main__":
    unittest.main()
