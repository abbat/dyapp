"""Release frontends keep reports and reject invalid inputs concisely."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from test_android_release import fixture

ROOT = Path(__file__).resolve().parents[1]
CHECKS = {
    "check-artifact-size.py": "artifact-size",
    "report-artifact-size.py": "artifact-measurement",
    "check-reproducible.py": "reproducible",
    "check-android-release.py": "android-release",
    "package-apple-release.py": "apple-package",
    "apple-release-in-ci.py": "apple-release",
}


class ReleaseOutput(unittest.TestCase):
    def run_check(self, root, script, arguments):
        return subprocess.run(
            [sys.executable, str(ROOT / (
                "tests/scripts" if script == "check-reproducible.py"
                else "tools/scripts") / script), *arguments],
            capture_output=True, text=True, check=False,
            env=dict(os.environ, GITHUB_ACTIONS="false",
                     DYAPP_CHECK_LOG_DIR=str(root / "logs")))

    def test_cli_argparse_status_without_usage_noise(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for script, name in CHECKS.items():
                result = self.run_check(root, script, [])
                self.assertEqual(result.returncode, 2, result.stdout)
                self.assertTrue(result.stdout.startswith(f"FAIL {name}:"))
                self.assertIn("FIX: docs/", result.stdout)
                self.assertIn("DETAILS:\n", result.stdout)
                self.assertNotIn("usage:", result.stdout)
                self.assertEqual(result.stderr, "")

    def test_android_real_zip_reports_default_raw_and_missing_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            apk, aab, report = [root / name for name in
                                ("app.apk", "app.aab", "report.json")]
            fixture(apk, "apk")
            fixture(aab, "aab")
            arguments = ["--apk", str(apk), "--aab", str(aab),
                         "--report", str(report)]
            result = self.run_check(
                root, "check-android-release.py", arguments)
            self.assertEqual(result.returncode, 0, result.stdout)
            self.assertEqual(result.stdout, "OK android-release\n")
            data = json.loads(report.read_text())
            self.assertEqual(set(data["artifacts"]), {"apk", "aab"})
            raw = self.run_check(
                root, "check-android-release.py", ["--raw", *arguments])
            self.assertEqual(raw.stdout, "OK android-release packages\n")
            bad = self.run_check(
                root, "check-android-release.py",
                ["--apk", str(root / "absent"), "--aab", str(root / "absent"),
                 "--report", str(report)])
            self.assertEqual(bad.returncode, 1)
            self.assertIn("absent", bad.stdout)
            self.assertEqual(bad.stdout.count("FAIL android-release:"), 1)
            self.assertEqual(len(json.loads(report.read_text())["errors"]), 2)

    def test_native_apple_approval_requirement_is_not_bypassed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for script, arguments in [
                    ("apple-release-in-ci.py", ["ios"]),
                    ("package-apple-release.py",
                     ["ios", "absent", "out.ipa"])]:
                result = self.run_check(root, script, arguments)
                self.assertEqual(result.returncode, 1, result.stdout)
                self.assertIn("requires approved CI", result.stdout)
                self.assertIn("DETAILS:\n", result.stdout)


if __name__ == "__main__":
    unittest.main()
