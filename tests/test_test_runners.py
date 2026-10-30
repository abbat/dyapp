"""Runner regressions; execute this module inside the dev Docker service."""
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/scripts"))

# This test imports the runner from the path added above.
from capture import Capture  # noqa: E402 -- path set above


class Captures(unittest.TestCase):
    def test_keeps_head_and_tail_with_marker(self):
        captured = Capture(head=4, tail=4)
        for chunk in (b"ab", b"cdef", b"ghij"):
            captured.write(chunk)
        self.assertEqual(captured.getvalue(),
                         b"abcd\n[... 2 bytes truncated ...]\nghij")
        self.assertEqual(captured.since(7), b"hij")
        self.assertEqual(captured.since(0), b"ghij")
        small = Capture(head=4, tail=4)
        small.write(b"abcdef")
        self.assertEqual(small.getvalue(), b"abcdef")
        self.assertEqual(small.since(3), b"def")

    def test_pipe_copies_child_output(self):
        captured = Capture()
        output, drain = captured.pipe()
        subprocess.run(["sh", "-c", "echo out; echo err >&2"],
                       stdout=output, stderr=subprocess.STDOUT, check=True)
        os.close(output)
        drain.join(timeout=10)
        self.assertEqual(captured.getvalue(), b"out\nerr\n")


class Runners(unittest.TestCase):
    def run_script(self, script, command, failing_service="", verbose="0"):
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
                ["bash", str(ROOT / "tests/scripts" / script), command],
                env={**os.environ, "PATH": f"{directory}:{os.environ['PATH']}",
                     "CALL_LOG": str(log), "FAIL_SERVICE": failing_service,
                     "DYAPP_VERBOSE": verbose},
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
        for command, service in (("test", "dev"),
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

    def test_failure_prints_captured_output(self):
        result, calls = self.run_script("docker-test.sh", "test")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "OK docker-tests\n")
        self.assertEqual(result.stderr, "")
        self.assertIn("run --no-TTY", calls)
        failed, _ = self.run_script("docker-test.sh", "test", "dev")
        self.assertEqual(failed.returncode, 23)
        self.assertTrue(failed.stdout.startswith("FAIL Docker Compose test:"))
        self.assertIn("FIX: docs/testing/docker.md#troubleshooting",
                      failed.stdout)
        self.assertIn("DETAILS:\n", failed.stdout)
        self.assertIn(": raw captured output", failed.stdout)
        self.assertNotIn("fake stdout", failed.stdout)
        self.assertEqual(failed.stderr, "")
        self.assertEqual(failed.stdout.count("\nFIX:"), 1)
        self.assertEqual(sum(line.startswith("FAIL ") for line in
                             failed.stdout.splitlines()), 1)

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

    def test_cleanup_failure_and_interrupt_preserve_status_and_output(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        for run_status, stop_status, expected in ((0, 19, 19), (23, 19, 23),
                                                  (130, 0, 130)):
            with self.subTest(run_status=run_status):
                def execute(command, **kwargs):
                    os.write(kwargs["stdout"], (command[0] + "\n").encode())
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
                self.assertTrue(out.getvalue().startswith("FAIL stage:"))
                self.assertIn(": raw captured output", out.getvalue())
                self.assertNotIn("\nrun\n", out.getvalue())
                self.assertEqual(error.getvalue(), "")

    def test_log_file_gets_a_full_copy(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            log = Path(directory, "output.log")
            out = io.StringIO()
            with patch.object(runner.sys, "stdout", out):
                self.assertEqual(runner.run_logged(
                    ["echo", "hello"], {}, "stage", log_file=log), 0)
            self.assertEqual(out.getvalue(), "OK stage\n")
            self.assertEqual(log.read_text(), "hello\n")

    def test_nested_failure_keeps_details_without_cleanup_progress(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        command = [sys.executable, "-c",
                   ("print('passing progress\\nFAIL inner: assertion\\n"
                    "FIX: docs/testing/docker.md#troubleshooting\\nDETAILS:\\n"
                    "expected 3, got 2\\ninner.log: raw captured output'); "
                    "raise SystemExit(17)")]
        out = io.StringIO()
        with patch.object(runner.sys, "stdout", out):
            status = runner.run_logged(
                command, {}, "outer", ["echo", "cleanup progress"])
        self.assertEqual(status, 17)
        self.assertIn("expected 3, got 2", out.getvalue())
        self.assertNotIn("passing progress", out.getvalue())
        self.assertNotIn("cleanup progress", out.getvalue())

    def test_unknown_large_failure_retains_full_log_and_signal_status(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "full.log"
            command = [sys.executable, "-c",
                       ("import sys; print('passing progress' * 30000); "
                        "sys.exit(27)")]
            out = io.StringIO()
            with patch.object(runner.sys, "stdout", out):
                self.assertEqual(runner.run_logged(
                    command, {}, "large", log_file=log), 27)
            self.assertNotIn("passing progress", out.getvalue())
            self.assertIn(str(log), out.getvalue())
            self.assertGreater(log.stat().st_size, 400000)
            out = io.StringIO()
            with patch.object(runner.sys, "stdout", out):
                self.assertEqual(runner.run_logged(
                    ["sh", "-c", "kill -TERM $$"], {}, "signal",
                    log_file=log), 143)
            self.assertIn("terminated by signal 15", out.getvalue())

    def test_node_restarts_append_logs_and_verbose_inherits_streams(self):
        spec = importlib.util.spec_from_file_location(
            "network_test", ROOT / "tests/scripts/network-test.py")
        network = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(network)
        environment = {"DYAPPD_STORAGE_DIR": "/storage/7071"}

        def spawn(command, **kwargs):
            os.write(kwargs["stdout"], b"node output\n")
            self.assertEqual(kwargs["stderr"], subprocess.STDOUT)
        with patch.object(network.subprocess, "Popen", side_effect=spawn):
            network.start_node("/bin", environment)
            network.start_node("/bin", environment)
        for drain in network.DRAINS:
            drain.join(timeout=10)
        self.assertEqual(network.LOGS["7071"].getvalue(),
                         b"node output\n" * 2)
        with patch.object(network.subprocess, "Popen") as invoke:
            network.start_node(
                "/bin", {**environment, "DYAPP_VERBOSE": "1"})
        self.assertNotIn("stdout", invoke.call_args.kwargs)
        self.assertNotIn("stderr", invoke.call_args.kwargs)

    def test_local_node_failure_prints_retained_diagnostics(self):
        spec = importlib.util.spec_from_file_location(
            "network_test", ROOT / "tests/scripts/network-test.py")
        network = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(network)

        process = Mock()

        def spawn(command, **kwargs):
            os.write(kwargs["stdout"], b"node startup failed\n")
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
        self.assertIn("--- node 7071 ---\nnode startup failed\n",
                      out.getvalue())
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
            arguments = ["python3",
                         str(ROOT / "tools/scripts/docker-local.py"),
                         "run", "dev"]
            first = subprocess.Popen(arguments, env=environment)
            second = subprocess.Popen(arguments, env=environment)
            self.assertEqual(first.wait(timeout=10), 0)
            self.assertEqual(second.wait(timeout=10), 0)
            self.assertEqual(log.read_text().splitlines(),
                             ["start", "end"] * 4)

    def test_bind_directories_come_from_the_compose_files(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        made = []
        with patch.object(runner.Path, "mkdir",
                          lambda path, **_: made.append(path)), \
                patch.object(runner.Path, "chmod"):
            runner.make_bind_directories(
                ["-f", "tests/docker/compose.ui.yml",
                 "--profile", "all", "run"])
        self.assertIn(ROOT / "build/linux-reports", made)
        self.assertIn(ROOT / "build/ux-research/current", made)
        self.assertNotIn(ROOT / "build/workspace", made)

    def test_image_builds_have_cpu_quota_and_stop_on_failure(self):
        spec = importlib.util.spec_from_file_location(
            "docker_local", ROOT / "tools/scripts/docker-local.py")
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
                    if command[1] in ("compose", "image"):
                        return config
                    os.write(kwargs["stdout"], log)
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
                if status == 0:
                    self.assertEqual(printed,
                                     "OK test:first\nOK test:second\n")
                else:
                    self.assertTrue(printed.startswith("FAIL test:first:"))
                    self.assertNotIn(log.decode(), printed)
                    self.assertIn(": raw captured output", printed)
                builds = [call for call in invoke.call_args_list
                          if call.args[0][1] == "build"]
                self.assertEqual(len(builds), 2 if status == 0 else 1)
                prunes = [call.args[0] for call in invoke.call_args_list
                          if call.args[0][1] == "image"]
                self.assertEqual(prunes, [[
                    "docker", "image", "prune", "-f", "--filter",
                    "label=dyapp.local"]] if status == 0 else [])
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
            "docker_local", ROOT / "tools/scripts/docker-local.py")
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
                self.assertEqual(runner.build_images(
                    ["build"], {"DYAPP_BUILD_CACHE": "gha",
                                "GITHUB_EVENT_NAME": "pull_request"}), 0)
        remote, local, pull, _ = (call.args[0]
                                  for call in invoke.call_args_list)
        self.assertEqual(remote[:3], ["docker", "buildx", "build"])
        self.assertIn("type=gha,scope=test-remote", remote)
        self.assertIn("--cache-to", remote)
        self.assertEqual(local[:2], ["docker", "build"])
        self.assertIn("--cache-from", pull)
        self.assertNotIn("--cache-to", pull)

    def test_desktop_signed_profile_payload(self):
        spec = importlib.util.spec_from_file_location(
            "desktop_smoke", ROOT / "tests/scripts/desktop-smoke.py")
        desktop = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(desktop)
        payload = bytes(range(128))
        record = b"".join((b"\x0a\x20", b"k" * 32, b"\x12\x80\x01",
                           payload, b"\x1a\x40", b"s" * 64))
        self.assertEqual(desktop.signed_profile_payload(record.hex()), payload)
        with self.assertRaisesRegex(RuntimeError, "invalid signed profile"):
            desktop.signed_profile_payload(record[:-1].hex())

    def test_windows_ui_report_does_not_require_container_hostname(self):
        spec = importlib.util.spec_from_file_location(
            "desktop_smoke", ROOT / "tests/scripts/desktop-smoke.py")
        desktop = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(desktop)
        Path("/tmp/ai").mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(dir="/tmp/ai") as directory:
            outcomes = [
                subprocess.CompletedProcess(
                    [], 0, "UI_SMOKE_PASS:" + "0" * 64, "")
                for _ in range(7)
            ] + [subprocess.CompletedProcess([], 1, "UI_SMOKE_FAIL", "")]
            environment = {"RUNNER_TEMP": directory}
            with patch.dict(os.environ, environment, clear=True):
                with patch.object(desktop, "run", side_effect=outcomes):
                    desktop.smoke("windows.exe", flows=False)
            report = json.loads(
                (Path(directory) / "desktop-ui-results.json").read_text())
            self.assertEqual(len(report), 8)
            self.assertTrue(all(case["passed"] for case in report))

    def test_desktop_timeout_kills_helpers_holding_stdout(self):
        spec = importlib.util.spec_from_file_location(
            "desktop_smoke", ROOT / "tests/scripts/desktop-smoke.py")
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
        for script in ("ui-test.sh", "docker-test.sh", "check-quality.sh",
                       "image-test.sh"):
            result, calls = self.run_script(script, "invalid")
            self.assertEqual(result.returncode, 2)
            self.assertEqual(calls, "")


if __name__ == "__main__":
    unittest.main()
