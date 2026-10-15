"""Native guards and report exports work without installing SDKs."""
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


class NativeOutput(unittest.TestCase):
    def test_apple_cli_guards_keep_status_without_executing_native_tools(self):
        commands = [
            ([sys.executable, "scripts/apple-test-in-ci.py"],
             1, "apple-tests"),
            (["bash", "scripts/apple-test-in-ci.sh"], 2, "apple-tests"),
            (["bash", "scripts/build-apple-core.sh", "invalid"],
             2, "apple-core"),
        ]
        with tempfile.TemporaryDirectory() as directory:
            for arguments, status, name in commands:
                result = subprocess.run(
                    arguments, cwd=ROOT, capture_output=True, text=True,
                    check=False, env=dict(
                        os.environ, GITHUB_ACTIONS="false",
                        DYAPP_CHECK_LOG_DIR=directory))
                self.assertEqual(result.returncode, status, result.stdout)
                self.assertTrue(result.stdout.startswith(f"FAIL {name}:"))
                self.assertIn("FIX: docs/", result.stdout)
                self.assertIn(": raw captured output", result.stdout)

    def test_export_keeps_distinct_evidence_and_rejects_empty(self):
        spec = importlib.util.spec_from_file_location(
            "ui_export", ROOT / "scripts/export-ui-reports.py")
        exporter = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(exporter)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            docker = root / "docker"
            docker.write_text(
                "#!/usr/bin/env python3\n"
                "import os,sys\nfrom pathlib import Path\n"
                "if sys.argv[1] == 'ps':\n"
                " print('owned-container')\n"
                "elif os.environ.get('DYAPP_FIXTURE_EMPTY') != '1':\n"
                " Path(sys.argv[3], 'TEST-unit.xml').write_text("
                "'<testsuite tests=\"3\"/>')\n")
            docker.chmod(0o755)
            env = dict(os.environ, PATH=f"{root}:{os.environ['PATH']}")
            with patch.dict(os.environ, env, clear=True):
                first = exporter.export(root / "reports")
                second = exporter.export(root / "reports")
                self.assertNotEqual(first, second)
                self.assertEqual(
                    (first / "TEST-unit.xml").read_bytes(),
                    (second / "TEST-unit.xml").read_bytes())
                os.environ["DYAPP_FIXTURE_EMPTY"] = "1"
                with self.assertRaisesRegex(RuntimeError, "Empty Android"):
                    exporter.export(root / "reports")
            with patch.object(exporter.subprocess, "check_output",
                              return_value=""):
                with self.assertRaisesRegex(RuntimeError, "No Android"):
                    exporter.export(root / "absent")
            self.assertFalse((root / "absent").exists())


if __name__ == "__main__":
    unittest.main()
