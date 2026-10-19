"""Real subprocess controls for desktop cases and prepared unit inventories."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DesktopOutput(unittest.TestCase):
    def test_ui_rejection_passes_and_only_failed_case_output_is_shown(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / "app"
            app.write_text(
                "#!/usr/bin/env python3\n"
                "import os,sys\n"
                "if '--break-ui' in sys.argv:\n"
                " print('failed-case-detail')\n"
                " if os.environ.get('DYAPP_FIXTURE_BROKEN') == '1':\n"
                "  raise SystemExit(0)\n"
                " print('UI_SMOKE_FAIL'); raise SystemExit(1)\n"
                "print('passing-case-noise'); print('UI_SMOKE_PASS')\n")
            app.chmod(0o755)
            for broken in ("0", "1"):
                reports = root / broken
                result = subprocess.run(
                    [sys.executable, str(ROOT / "scripts/desktop-smoke.py"),
                     str(app)], capture_output=True, text=True, check=False,
                    env=dict(os.environ, RUNNER_TEMP=str(reports),
                             DYAPP_CHECK_LOG_DIR=str(root / "logs"),
                             DYAPP_FIXTURE_BROKEN=broken))
                if broken == "0":
                    self.assertEqual(result.returncode, 0, result.stdout)
                    self.assertEqual(result.stdout, "OK desktop-smoke\n")
                    data = json.loads(
                        (reports / "desktop-ui-results.json").read_text())
                    self.assertEqual(len(data), 3)
                    self.assertTrue(all(case["passed"] for case in data))
                else:
                    self.assertEqual(result.returncode, 1)
                    self.assertIn("FAIL desktop-smoke:", result.stdout)
                    self.assertIn("failed-case-detail", result.stdout)
                    self.assertNotIn("passing-case-noise", result.stdout)
                    pointer = next(line for line in result.stdout.splitlines()
                                   if line.endswith(": raw captured output"))
                    retained = Path(pointer.removesuffix(
                        ": raw captured output")).read_text()
                    self.assertIn("passing-case-noise", retained)
                    self.assertIn("failed-case-detail", retained)

    def test_rust_inventory_preserves_exit_101_and_assertion(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, binary = root / "fixture.rs", root / "fixture"
            source.write_text("#[test] fn fails() { assert_eq!(1, 2); }\n")
            subprocess.run(
                ["rustc", "--test", str(source), "-o", str(binary)],
                check=True, capture_output=True)
            inventory = root / "units.json"
            inventory.write_text(json.dumps({
                "reason": "compiler-artifact", "profile": {"test": True},
                "executable": str(binary)}) + "\n")
            result = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertEqual(result.returncode, 101, result.stdout)
            self.assertTrue(result.stdout.startswith("FAIL rust-test:"))
            self.assertIn("left: 1", result.stdout)
            listing = root / "bad-listing"
            listing.write_text(
                "#!/bin/sh\necho listing-failure >&2\nexit 37\n")
            listing.chmod(0o755)
            inventory.write_text(json.dumps({
                "reason": "compiler-artifact", "profile": {"test": True},
                "executable": str(listing)}) + "\n")
            bad_list = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertEqual(bad_list.returncode, 37)
            self.assertTrue(any("listing-failure" in path.read_text()
                                for path in (root / "logs").glob("*.log")))
            inventory.write_text("")
            empty = subprocess.run(
                [sys.executable,
                 str(ROOT / "scripts/run-cargo-unit-binaries.py"),
                 str(inventory)], capture_output=True, text=True, check=False,
                env=dict(os.environ, DYAPP_CHECK_LOG_DIR=str(root / "logs")))
            self.assertNotEqual(empty.returncode, 0)
            self.assertIn("No unit test binaries", empty.stdout)


if __name__ == "__main__":
    unittest.main()
