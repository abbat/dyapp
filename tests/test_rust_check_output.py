"""Use real rustc test binaries to exercise failed-test diagnostics."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RustOutput(unittest.TestCase):
    def test_real_failing_and_passing_test_binary(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "fixture.rs"
            binary = root / "fixture"
            source.write_text(
                "#[test] fn passes() {}\n"
                "#[test] fn fails() { assert_eq!(3, 2); }\n")
            subprocess.run(
                ["rustc", "--test", str(source), "-o", str(binary)],
                check=True, capture_output=True)
            prefix = [sys.executable, str(ROOT / "scripts/run-check.py"),
                      "--log-dir", str(root), "rust-test", "--", str(binary)]
            failed = subprocess.run(
                prefix, capture_output=True, text=True, check=False)
            self.assertEqual(failed.returncode, 101, failed.stdout)
            self.assertTrue(failed.stdout.startswith("FAIL rust-test:"))
            self.assertIn("assertion", failed.stdout)
            self.assertIn("left: 3", failed.stdout)
            self.assertNotIn("test passes ... ok", failed.stdout)
            passed = subprocess.run(
                prefix + ["passes", "--exact"], capture_output=True,
                text=True, check=False)
            self.assertEqual(passed.returncode, 0, passed.stdout)
            self.assertEqual(passed.stdout, "OK rust-test\n")
            self.assertEqual(len(list(root.glob("rust-test-*.log"))), 2)

    def test_stage_dispatch_with_failing_child_and_raw_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cargo = root / "cargo"
            cargo.write_text(
                "#!/bin/sh\n"
                "printf 'passing progress\\nfailures:\\n"
                "---- fixture stdout ----\\nassertion failed\\n'\n"
                "exit 101\n")
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                       DYAPP_CHECK_LOG_DIR=str(root / "logs"))
            command = ["bash", str(ROOT / "scripts/rust-check.sh")]
            default = subprocess.run(
                command + ["test"], capture_output=True, text=True,
                env=env, check=False)
            self.assertEqual(default.returncode, 101, default.stderr)
            self.assertTrue(default.stdout.startswith("FAIL rust-test:"))
            self.assertIn("assertion failed", default.stdout)
            self.assertNotIn("passing progress", default.stdout)
            raw = subprocess.run(
                command + ["--raw", "test"], capture_output=True,
                text=True, env=env, check=False)
            self.assertEqual(raw.returncode, 101)
            self.assertIn("passing progress", raw.stdout)

    def test_nextest_failure_details(self):
        output = ("    Starting 2 tests\\n        PASS [   0.001s] core ok\\n"
                  "        FAIL [   0.002s] core bad\\n"
                  "thread bad panicked at src/lib.rs:2:5\\n")
        with tempfile.TemporaryDirectory() as directory:
            failed = subprocess.run(
                [sys.executable, str(ROOT / "scripts/run-check.py"),
                 "--log-dir", directory, "rust-test", "--",
                 "sh", "-c", f"printf '{output}'; exit 100"],
                capture_output=True, text=True, check=False)
        self.assertEqual(failed.returncode, 100, failed.stdout)
        self.assertIn("FAIL [   0.002s] core bad\nthread bad panicked",
                      failed.stdout)
        self.assertNotIn("PASS", failed.stdout)


if __name__ == "__main__":
    unittest.main()
