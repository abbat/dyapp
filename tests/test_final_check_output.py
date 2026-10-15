"""Negative controls for CI inventory, isolation and Gradle diagnostics."""
import json
import os
import socket
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class FinalChecks(unittest.TestCase):
    def test_summary_rejects_empty_missing_cancelled_and_skipped_jobs(self):
        good = {name: {"result": "success"} for name in
                ("core", "docker", "linux", "apple", "windows")}
        missing = {name: value for name, value in good.items()
                   if name != "apple"}
        cases = [good, {}, [], dict(good, apple={"result": "failure"}),
                 dict(good, apple={"result": "cancelled"}),
                 dict(good, apple={"result": "skipped"}),
                 missing]
        with tempfile.TemporaryDirectory() as directory:
            for index, data in enumerate(cases):
                result = subprocess.run(
                    [sys.executable,
                     str(ROOT / "scripts/check-ci-results.py")],
                    capture_output=True, text=True, check=False,
                    env=dict(os.environ, RESULTS=json.dumps(data),
                             DYAPP_CHECK_LOG_DIR=directory))
                self.assertEqual(result.returncode, int(index != 0))
                if index == 0:
                    self.assertEqual(result.stdout, "OK ci-results\n")
                else:
                    self.assertTrue(
                        result.stdout.startswith("FAIL ci-results:"))
                    self.assertNotIn("'success'", result.stdout)
                    self.assertNotIn('"outputs"', result.stdout)

    def test_isolation_default_raw_and_root_rejection_bind_to_map(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            status, control = root / "status", root / "docker.sock"
            script_source = ROOT / "scripts/check-container-isolation.sh"
            source = script_source.read_text()
            # The actual checker reads controlled OS facts; production gets
            # no environment override or test-only bypass.
            source = source.replace("/proc/self/status", str(status))
            source = source.replace("/var/run/docker.sock", str(control))
            source = source.replace("/tmp/.X11-unix/X0", str(root / "X0"))
            script = root / "isolation.sh"
            script.write_text(source)
            identity = root / "id"
            identity.write_text(
                "#!/bin/sh\nprintf '%s\\n' \"$DYAPP_FIXTURE_UID\"\n")
            identity.chmod(0o755)

            def run(uid="999", args=()):
                return subprocess.run(
                    ["bash", str(script), *args],
                    capture_output=True, text=True,
                    env=dict(os.environ,
                             PATH=f"{directory}:{os.environ['PATH']}",
                             DYAPP_FIXTURE_UID=uid))

            def facts(capabilities=0, privileges=1):
                status.write_text(
                    f"CapEff:\t{capabilities:016x}\n"
                    f"NoNewPrivs:\t{privileges}\n")

            facts()
            good = run()
            self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
            self.assertEqual(good.stdout, "OK container-isolation\n")
            raw = run(args=("--raw",))
            self.assertEqual(raw.returncode, 0)
            self.assertIn("Container isolation verified:", raw.stdout)
            bad = run(uid="0")
            self.assertEqual(bad.returncode, 1)
            self.assertIn("Runtime must not be root", bad.stdout)
            policies = json.loads(
                (ROOT / "scripts/check-output.json").read_text())
            self.assertIn(
                f'FIX: {policies["container-isolation"]["fix"]}\n', bad.stdout)
            for capabilities, privileges in [(1, 1), (0, 0)]:
                facts(capabilities, privileges)
                self.assertEqual(run().returncode, 1)
            facts()
            with socket.socket(socket.AF_UNIX) as endpoint:
                endpoint.bind(str(control))
                self.assertEqual(run().returncode, 1)

    def test_network_android_args_fail_before_setup(self):
        commands = [
            [sys.executable, str(ROOT / "scripts/network-test.py"),
             "--invalid"],
            ["bash", str(ROOT / "scripts/build-android-release.sh")],
        ]
        with tempfile.TemporaryDirectory() as directory:
            for command in commands:
                result = subprocess.run(
                    command, capture_output=True, text=True, check=False,
                    env=dict(os.environ, DYAPP_CHECK_LOG_DIR=directory))
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertTrue(result.stdout.startswith("FAIL "))
                self.assertIn(
                    "Usage:", result.stdout.replace("usage:", "Usage:"))

    def test_ascii_terminal_keeps_unicode_raw_evidence_and_child_status(self):
        with tempfile.TemporaryDirectory() as directory:
            program = ("import os; os.write(1, "
                       "bytes([101,114,114,111,114,58,32,226,152,131,10])); "
                       "raise SystemExit(23)")
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/run-check.py"),
                 "--log-dir", directory, "cargo", "--", sys.executable,
                 "-c", program], capture_output=True, text=True, check=False,
                env=dict(os.environ, PYTHONIOENCODING="ascii"))
            self.assertEqual(result.returncode, 23, result.stdout)
            self.assertTrue(result.stdout.startswith("FAIL cargo:"))
            self.assertIn("\\u2603", result.stdout)
            logs = list(Path(directory).glob("*.log"))
            self.assertEqual(len(logs), 1)
            self.assertIn(bytes([226, 152, 131]), logs[0].read_bytes())

    def test_nested_frames_use_only_mapped_fix_and_keep_status(self):
        known = "docs/testing/docker.md#troubleshooting"
        unknown = "docs/missing.md#unknown"
        for fix in (known, unknown):
            frame = (f"FAIL docker: fixture failure\nFIX: {fix}\n"
                     "DETAILS:\nactual-failing-item\n")
            with tempfile.TemporaryDirectory() as directory:
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/run-check.py"),
                     "--log-dir", directory, "docker", "--", sys.executable,
                     "-c", f"print({frame!r}); raise SystemExit(23)"],
                    capture_output=True, text=True, check=False)
                self.assertEqual(result.returncode, 23)
                self.assertEqual(sum(line.startswith("FAIL ") for line in
                                     result.stdout.splitlines()), 1)
                self.assertEqual(result.stdout.count("\nFIX:"), 1)
                self.assertIn(f"FIX: {known}\n", result.stdout)
                self.assertIn("actual-failing-item", result.stdout)
                if fix == unknown:
                    self.assertNotIn(unknown, result.stdout)
                logs = list(Path(directory).glob("*.log"))
                self.assertEqual(len(logs), 1)
                self.assertIn(fix, logs[0].read_text())

    def test_archived_gradle_failure_excludes_passing_tasks_and_try_help(self):
        failure = (
            "> Task :app:compileReleaseKotlin UP-TO-DATE\n"
            "FAILURE: Build failed with an exception.\n\n* What went wrong:\n"
            "Execution failed for task ':app:lintVitalAnalyzeRelease'.\n"
            "> Could not resolve com.android.tools.lint:lint-gradle:32.4.1.\n"
            "  > No cached version available for offline mode.\n"
            "* Try:\n> Run with --stacktrace option.\nBUILD FAILED in 12s\n")
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [sys.executable, str(ROOT / "scripts/run-check.py"),
                 "--log-dir", directory, "docker", "--", sys.executable,
                 "-c", f"print({failure!r}); raise SystemExit(7)"],
                capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, 7)
            self.assertIn("lintVitalAnalyzeRelease", result.stdout)
            self.assertIn("No cached version", result.stdout)
            self.assertNotIn("UP-TO-DATE", result.stdout)
            self.assertNotIn("--stacktrace", result.stdout)


if __name__ == "__main__":
    unittest.main()
