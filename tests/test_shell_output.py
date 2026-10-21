"""Image cleanup precedence and shell CLI preflight use real child statuses."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
DOCKER = """#!/usr/bin/env python3
import os,sys
args=sys.argv[1:]
if args[0]=='run':
 if args[-1]=='keygen':
  if os.environ['DYAPP_FIXTURE_PRIMARY']=='23':
   print('error: keygen fixture failure',file=sys.stderr)
   raise SystemExit(23)
 elif '-d' not in args:
  raise SystemExit(1)
elif args[0]=='exec':
 if 'sh' in args: raise SystemExit(1)
 if 'status' in args: print('deny_entries 1')
elif args[0]=='logs': print('listening')
elif args[0]=='inspect': print('true')
elif args[0]=='volume':
 status=int(os.environ['DYAPP_FIXTURE_CLEANUP'])
 if status: print('error: cleanup fixture failure',file=sys.stderr)
 raise SystemExit(status)
"""


class ShellOutput(unittest.TestCase):
    def test_image_cleanup_failure_never_masks_primary_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, code in [("docker", DOCKER),
                               ("sleep", "#!/bin/sh\nexit 0\n")]:
                path = root / name
                path.write_text(code)
                path.chmod(0o755)
            cases = [(0, 0, 0), (0, 19, 19), (23, 19, 23)]
            for primary, cleanup, expected in cases:
                result = subprocess.run(
                    ["bash", str(ROOT / "tests/scripts/image-test.sh")],
                    capture_output=True, text=True, check=False,
                    env=dict(os.environ, PATH=f"{root}:{os.environ['PATH']}",
                             DYAPP_VERBOSE="0", DYAPP_CHECK_LOG_DIR=str(root),
                             DYAPP_FIXTURE_PRIMARY=str(primary),
                             DYAPP_FIXTURE_CLEANUP=str(cleanup)))
                self.assertEqual(result.returncode, expected, result.stdout)
                if expected:
                    self.assertTrue(
                        result.stdout.startswith("FAIL image-test:"))
                    self.assertIn("FIX: docs/", result.stdout)
                else:
                    self.assertEqual(result.stdout, "OK image-test\n")

    def test_ui_cli_errors_and_help_do_not_probe_kvm(self):
        for args, status in [(["--help"], 0), (["invalid"], 2),
                             (["shell", "invalid"], 2)]:
            result = subprocess.run(
                ["bash", str(ROOT / "tests/scripts/ui-test.sh"), *args],
                capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_KVM="1", DYAPP_VERBOSE="0"))
            self.assertEqual(result.returncode, status, result.stdout)
            self.assertNotIn("stat:", result.stdout + result.stderr)
            if status:
                self.assertTrue(result.stdout.startswith("FAIL ui-tests:"))
            else:
                self.assertTrue(result.stdout.startswith("Usage:"))


if __name__ == "__main__":
    unittest.main()
