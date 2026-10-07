"""Runner regressions; execute this module inside the dev Docker service."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


class Runners(unittest.TestCase):
    def run_script(self, script, command, failing_service=""):
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            docker = Path(directory) / "docker"
            log = Path(directory) / "calls"
            docker.write_text(
                '#!/bin/bash\n'
                'printf "%s\\n" "$*" >> "$CALL_LOG"\n'
                'if [[ $* == *"--no-build"* ]]; then exit 1; fi\n'
                'if [[ -n $FAIL_SERVICE && $* == *"$FAIL_SERVICE"* ]]; '
                'then exit 23; fi\n'
            )
            docker.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "scripts" / script), command],
                env={**os.environ, "PATH": f"{directory}:{os.environ['PATH']}",
                     "CALL_LOG": str(log), "FAIL_SERVICE": failing_service},
                capture_output=True, text=True, check=False,
            )
            return result, log.read_text() if log.exists() else ""

    def test_ui_all_requires_each_suite(self):
        for service in (
                "android-unit-test", "android-emulator-test", "linux-test"):
            with self.subTest(service=service):
                result, _ = self.run_script("ui-test.sh", "all", service)
                self.assertEqual(result.returncode, 23)
                self.assertNotIn("inventory passed", result.stdout)

    def test_dev_failure_propagates(self):
        for command, service in (("test", "dev"), ("build", "build"),
                                 ("quality", "quality"),
                                 ("coverage", "coverage"),
                                 ("security", "security")):
            with self.subTest(command=command):
                result, _ = self.run_script("docker-test.sh", command, service)
                self.assertEqual(result.returncode, 23)

    def test_ui_success_runs_all_without_build_or_host_gui(self):
        result, calls = self.run_script("ui-test.sh", "all")
        self.assertEqual(result.returncode, 0, result.stderr)
        runs = [line for line in calls.splitlines() if " run " in line]
        self.assertEqual(len(runs), 3)
        for service in (
                "android-unit-test", "android-emulator-test", "linux-test"):
            self.assertIn(f"--profile all run --pull never {service}", calls)
        self.assertNotIn("build", calls)
        self.assertNotIn("linux-gui", calls)

    def test_local_docker_runs_serialize(self):
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            docker = Path(directory) / "docker"
            log = Path(directory) / "concurrency"
            docker.write_text(
                '#!/usr/bin/python3\n'
                'import os,time\n'
                'with open(os.environ["CALL_LOG"], "a") as out:\n'
                ' out.write("start\\n"); out.flush()\n'
                ' time.sleep(0.2); out.write("end\\n")\n'
            )
            docker.chmod(0o755)
            environment = {
                **os.environ, "PATH": f"{directory}:{os.environ['PATH']}",
                "CALL_LOG": str(log),
            }
            arguments = ["python3", str(ROOT / "scripts/docker-local.py"),
                         "run", "dev"]
            first = subprocess.Popen(arguments, env=environment)
            second = subprocess.Popen(arguments, env=environment)
            self.assertEqual(first.wait(timeout=10), 0)
            self.assertEqual(second.wait(timeout=10), 0)
            self.assertEqual(log.read_text().splitlines(),
                             ["start", "end"] * 4)

    def test_image_builds_have_cpu_quota_and_stop_on_failure(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        services = {
            name: {"image": f"test:{name}",
                   "build": {"context": "/context",
                             "dockerfile": "Dockerfile"}}
            for name in ("first", "second")
        }
        config = subprocess.CompletedProcess(
            [], 0, stdout=json.dumps({"services": services}))
        for status in (0, 23):
            with self.subTest(status=status):
                log = b"build log\n"
                results = [config,
                           subprocess.CompletedProcess([], status, stdout=log)]
                if status == 0:
                    results.append(
                        subprocess.CompletedProcess([], 0, stdout=log))
                output = io.TextIOWrapper(io.BytesIO())
                with patch.object(runner.subprocess, "run",
                                  side_effect=results) as invoke, \
                        patch.object(runner.sys, "stdout", output), \
                        patch.object(runner.sys, "stderr", io.StringIO()):
                    self.assertEqual(runner.build_images(
                        ["--profile", "build", "build", "first", "second"],
                        {}), status)
                output.flush()
                printed = output.buffer.getvalue().decode()
                # The build log is shown only for a failed build.
                if status == 0:
                    self.assertEqual(printed,
                                     "OK test:first\nOK test:second\n")
                else:
                    self.assertEqual(printed, "build log\n")
                builds = invoke.call_args_list[1:]
                self.assertEqual(len(builds), 2 if status == 0 else 1)
                for call in builds:
                    command = call.args[0]
                    self.assertEqual(command[:2], ["docker", "build"])
                    for flag, value in (("--cpu-quota", "400000"),
                                        ("--cpu-period", "100000")):
                        self.assertEqual(command[command.index(flag) + 1],
                                         value)
                    self.assertEqual(
                        call.kwargs["env"]["DOCKER_BUILDKIT"], "0")

    def test_ci_cache_uses_buildkit_except_on_local_base_images(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory() as directory:
            for name, base in (("remote", "debian"), ("local", "dyapp:dev")):
                Path(directory, name).write_text(f"FROM {base}\n")
            services = {
                name: {"image": f"test:{name}",
                       "build": {"context": directory, "dockerfile": name}}
                for name in ("remote", "local")
            }
            ok = subprocess.CompletedProcess([], 0, stdout=b"")
            config = subprocess.CompletedProcess(
                [], 0, stdout=json.dumps({"services": services}))
            with patch.object(runner.subprocess, "run",
                              side_effect=[config, ok, ok]) as invoke, \
                    patch.object(runner.sys, "stdout", io.StringIO()):
                self.assertEqual(runner.build_images(
                    ["build"], {"DYAPP_BUILD_CACHE": "gha"}), 0)
        remote, local = (call.args[0] for call in invoke.call_args_list[1:])
        self.assertEqual(remote[:3], ["docker", "buildx", "build"])
        self.assertIn("type=gha,scope=test-remote", remote)
        self.assertEqual(local[:2], ["docker", "build"])

    def test_windows_ui_report_does_not_require_container_hostname(self):
        spec = importlib.util.spec_from_file_location(
            "desktop_smoke", ROOT / "scripts/desktop-smoke.py")
        desktop = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(desktop)
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            outcomes = [
                subprocess.CompletedProcess([], 0, "UI_SMOKE_PASS", ""),
                subprocess.CompletedProcess([], 1, "UI_SMOKE_FAIL", ""),
            ]
            environment = {"RUNNER_TEMP": directory}
            with patch.dict(os.environ, environment, clear=True):
                with patch.object(desktop, "run", side_effect=outcomes):
                    desktop.smoke("windows.exe")
            report = json.loads(
                (Path(directory) / "desktop-ui-results.json").read_text())
            self.assertEqual(len(report), 2)
            self.assertTrue(all(case["passed"] for case in report))

    def test_desktop_timeout_kills_helpers_holding_stdout(self):
        spec = importlib.util.spec_from_file_location(
            "desktop_smoke", ROOT / "scripts/desktop-smoke.py")
        desktop = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(desktop)
        hang = ["sh", "-c", "sleep 300 & sleep 300"]
        with patch("sys.stdout", io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                desktop.run(hang, timeout=1)

    def test_unknown_command_is_failure(self):
        for script in ("ui-test.sh", "docker-test.sh", "check-quality.sh"):
            result, calls = self.run_script(script, "invalid")
            self.assertEqual(result.returncode, 2)
            self.assertEqual(calls, "")


if __name__ == "__main__":
    unittest.main()
