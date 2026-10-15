"""Exercise Docker preflight failures without building images."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class Preflight(unittest.TestCase):
    def test_invalid_config_service_and_options_keep_status_and_evidence(self):
        cases = [
            (["build", "unknown"], '{"services": {}}', 0, 2, "Unknown"),
            (["build"], "invalid-json", 0, 1, "JSONDecodeError"),
            (["build"], "passing progress", 9, 9, "error: invalid compose"),
            (["build", "--parallel"], "", 0, 2, "Unsupported"),
            (["--log-file"], "", 0, 1, "IndexError"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls = root / "called"
            docker = root / "docker"
            docker.write_text(
                "#!/bin/sh\n"
                "echo called >> \"$CALLS\"\n"
                "echo \"$CONFIG\"\n"
                "echo 'error: invalid compose' >&2\n"
                "exit \"$STATUS\"\n")
            docker.chmod(0o755)
            for index, case in enumerate(cases):
                args, config, code, expected, cause = case
                logs = root / str(index)
                env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                           CONFIG=config, STATUS=str(code), CALLS=str(calls),
                           DYAPP_CHECK_LOG_DIR=str(logs))
                before = calls.read_text() if calls.exists() else ""
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/docker-local.py"),
                     *args], env=env, capture_output=True, text=True,
                    check=False)
                self.assertEqual(result.returncode, expected, result.stdout)
                self.assertTrue(result.stdout.startswith(
                    "FAIL Docker configuration:"))
                self.assertIn(cause, result.stdout)
                self.assertIn("FIX: docs/testing/docker.md#troubleshooting",
                              result.stdout)
                self.assertNotIn("passing progress", result.stdout)
                saved = list(logs.glob("docker-config-*.log"))
                self.assertEqual(len(saved), 1)
                self.assertTrue(saved[0].stat().st_size)
                if "--parallel" in args or "--log-file" in args:
                    after = calls.read_text() if calls.exists() else ""
                    self.assertEqual(before, after)

    def test_missing_docker_executable_preserves_startup_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            docker = root / "docker"
            docker.write_text("#!/nonexistent/docker-interpreter\n")
            docker.chmod(0o755)
            logs = root / "logs"
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/docker-local.py"),
                 "build"], capture_output=True, text=True, check=False,
                env=dict(os.environ, PATH=str(root),
                         DYAPP_CHECK_LOG_DIR=str(logs)))
            self.assertEqual(result.returncode, 1)
            self.assertIn("FAIL Docker configuration:", result.stdout)
            self.assertIn("FileNotFoundError", result.stdout)
            saved = list(logs.glob("docker-config-*.log"))
            self.assertEqual(len(saved), 1)
            self.assertIn("Traceback", saved[0].read_text())

    def test_unsupported_configuration_never_launches_a_build(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            config = {"services": {"app": {"build": {
                "context": ".", "network": "host"}, "image": "app"}}}
            docker = root / "docker"
            docker.write_text("#!/bin/sh\nprintf '%s\\n' \"$CONFIG\"\n")
            docker.chmod(0o755)
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/docker-local.py"),
                 "build", "app"], capture_output=True, text=True, check=False,
                env=dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                         CONFIG=json.dumps(config),
                         DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertEqual(result.returncode, 2, result.stdout)
            self.assertIn("Unsupported build configuration", result.stdout)


if __name__ == "__main__":
    unittest.main()
