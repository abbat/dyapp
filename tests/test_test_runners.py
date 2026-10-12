"""Runner regressions; execute this module inside the dev Docker service."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]


class Runners(unittest.TestCase):
    def run_script(self, script, command, failing_service="", verbose="0",
                   ci=""):
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            docker = Path(directory) / "docker"
            log = Path(directory) / "calls"
            docker.write_text(
                '#!/bin/bash\n'
                'printf "%s\\n" "$*" >> "$CALL_LOG"\n'
                'printf "fake stdout\\n"; printf "fake stderr\\n" >&2\n'
                'if [[ $* == *"--no-build"* ]]; then exit 1; fi\n'
                'if [[ -n $FAIL_SERVICE && $* == *"$FAIL_SERVICE"* ]]; '
                'then exit 23; fi\n'
            )
            docker.chmod(0o755)
            result = subprocess.run(
                ["bash", str(ROOT / "scripts" / script), command],
                env={**os.environ, "PATH": f"{directory}:{os.environ['PATH']}",
                     "CALL_LOG": str(log), "FAIL_SERVICE": failing_service,
                     "DYAPP_VERBOSE": verbose, "GITHUB_ACTIONS": ci},
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
            self.assertIn(
                f"--profile all run --no-TTY --pull never {service}", calls)
        self.assertNotIn("build", calls)
        self.assertNotIn("linux-gui", calls)

    def test_failure_names_its_log_and_ci_replays_it(self):
        result, calls = self.run_script("docker-test.sh", "test")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(result.stdout.splitlines()), 1)
        self.assertEqual(result.stderr, "")
        log = Path(result.stdout.strip().split("; log: ")[1])
        self.assertEqual(log.read_text(),
                         "fake stdout\nfake stderr\n" * 2)
        self.assertIn("run --no-TTY", calls)
        failed, _ = self.run_script("docker-test.sh", "test", "dev")
        self.assertEqual(failed.returncode, 23)
        self.assertEqual(failed.stdout, "")
        self.assertIn("ERROR Docker Compose test (exit 23); log: ",
                      failed.stderr)
        replayed, _ = self.run_script("docker-test.sh", "test", "dev",
                                      ci="true")
        self.assertEqual(replayed.stdout, log.read_text())

    def test_verbose_and_interactive_shells_stream_output(self):
        for script, command, verbose in (("docker-test.sh", "test", "1"),
                                         ("docker-test.sh", "shell", "0"),
                                         ("ui-test.sh", "shell", "0")):
            with self.subTest(script=script, command=command):
                result, calls = self.run_script(script, command,
                                                verbose=verbose)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("fake stdout", result.stdout)
                self.assertIn("fake stderr", result.stderr)
                self.assertNotIn("; log: ", result.stdout)
                self.assertNotIn("--no-TTY", calls)

    def test_cleanup_failure_and_interrupt_preserve_status_and_log(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        for run_status, stop_status, expected in ((0, 19, 19), (23, 19, 23),
                                                  (130, 0, 130)):
            with self.subTest(run_status=run_status):
                def execute(command, **kwargs):
                    kwargs["stdout"].write((command[0] + "\n").encode())
                    if command == ["run"] and run_status == 130:
                        raise KeyboardInterrupt
                    return subprocess.CompletedProcess(
                        command, run_status if command == ["run"]
                        else stop_status)
                out, error = io.StringIO(), io.StringIO()
                with patch.object(runner.subprocess, "run",
                                  side_effect=execute), \
                        patch.object(runner.sys, "stdout", out), \
                        patch.object(runner.sys, "stderr", error), \
                        patch.dict(os.environ, {"GITHUB_ACTIONS": ""}):
                    self.assertEqual(runner.run_logged(
                        ["run"], {}, "stage", ["stop"]), expected)
                self.assertEqual(out.getvalue(), "")
                log = Path(error.getvalue().strip().split("; log: ")[1])
                self.assertEqual(log.read_text(), "run\nstop\n")

    def test_node_restarts_append_logs_and_verbose_inherits_streams(self):
        spec = importlib.util.spec_from_file_location(
            "network_test", ROOT / "scripts/network-test.py")
        network = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(network)
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            environment = {"DYAPP_NODE_LOG_DIR": directory,
                           "DYAPPD__STORAGE__DIR": "/storage/7071"}

            def spawn(command, **kwargs):
                kwargs["stdout"].write(b"node output\n")
                self.assertEqual(kwargs["stderr"], subprocess.STDOUT)
            with patch.object(network.subprocess, "Popen", side_effect=spawn):
                network.start_node("/bin", environment)
                network.start_node("/bin", environment)
            self.assertEqual(Path(directory, "node-7071.log").read_text(),
                             "node output\n" * 2)
            with patch.object(network.subprocess, "Popen") as invoke:
                network.start_node(
                    "/bin", {**environment, "DYAPP_VERBOSE": "1"})
            self.assertNotIn("stdout", invoke.call_args.kwargs)
            self.assertNotIn("stderr", invoke.call_args.kwargs)

    def test_local_node_failure_prints_retained_diagnostics(self):
        spec = importlib.util.spec_from_file_location(
            "network_test", ROOT / "scripts/network-test.py")
        network = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(network)

        process = Mock()

        def spawn(command, **kwargs):
            kwargs["stdout"].write(b"node startup failed\n")
            return process

        out = io.StringIO()
        with patch.object(network.subprocess, "run"), \
                patch.object(network.subprocess, "Popen", side_effect=spawn), \
                patch.object(network, "healthy", return_value=False), \
                patch.object(network.time, "sleep"), \
                patch.object(network.sys, "stdout", out), \
                patch.dict(os.environ, {"DYAPP_VERBOSE": "0"}):
            with self.assertRaisesRegex(RuntimeError, "Node did not start"):
                network.local("/bin")
        self.assertIn("node startup failed", out.getvalue())
        logs = Path(out.getvalue().splitlines()[0].split(": ")[1])
        self.assertEqual((logs / "node-7071.log").read_bytes(),
                         b"node startup failed\n")
        process.terminate.assert_called_once()
        process.wait.assert_called_once()

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

                def execute(command, **kwargs):
                    if command[1] == "compose":
                        return config
                    kwargs["stdout"].write(log)
                    return subprocess.CompletedProcess([], status)
                output = io.TextIOWrapper(io.BytesIO())
                with patch.object(runner.subprocess, "run",
                                  side_effect=execute) as invoke, \
                        patch.object(runner.sys, "stdout", output), \
                        patch.object(runner.sys, "stderr", io.StringIO()), \
                        patch.dict(os.environ, {"GITHUB_ACTIONS": ""}):
                    self.assertEqual(runner.build_images(
                        ["--profile", "build", "build", "first", "second"],
                        {}), status)
                output.flush()
                printed = output.buffer.getvalue().decode()
                # A local failed build prints only its ERROR line on stderr.
                if status == 0:
                    summaries = printed.splitlines()
                    self.assertEqual(len(summaries), 2)
                    for name, summary in zip(("first", "second"), summaries):
                        self.assertTrue(summary.startswith(
                            f"OK test:{name}; log: "))
                        self.assertEqual(Path(summary.split("; log: ")[1])
                                         .read_bytes(), log)
                else:
                    self.assertEqual(printed, "")
                builds = invoke.call_args_list[1:]
                self.assertEqual(len(builds), 2 if status == 0 else 1)
                for call in builds:
                    command = call.args[0]
                    self.assertEqual(command[:2], ["docker", "build"])
                    quota = str(runner.CPUS * 100000)
                    for flag, value in (("--cpu-quota", quota),
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
            config = subprocess.CompletedProcess(
                [], 0, stdout=json.dumps({"services": services}))
            with patch.object(runner.subprocess, "run", return_value=config), \
                    patch.object(runner, "run_logged",
                                 return_value=0) as invoke:
                self.assertEqual(runner.build_images(
                    ["build"], {"DYAPP_BUILD_CACHE": "gha"}), 0)
        remote, local = (call.args[0] for call in invoke.call_args_list)
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
        # The second helper leaves the process group, like a bwrap sandbox.
        hang = ["sh", "-c", "sleep 300 & setsid sleep 30 & sleep 300"]
        started = time.monotonic()
        with patch("sys.stdout", io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "timed out"):
                desktop.run(hang, timeout=1)
        self.assertLess(time.monotonic() - started, 15)

    def test_unknown_command_is_failure(self):
        for script in ("ui-test.sh", "docker-test.sh", "check-quality.sh"):
            result, calls = self.run_script(script, "invalid")
            self.assertEqual(result.returncode, 2)
            self.assertEqual(calls, "")


if __name__ == "__main__":
    unittest.main()
